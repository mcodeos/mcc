// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase G (D10): `CircuitWorld` — the aggregate container of the dianlu-tree
//! refactor (design §11.3 / plan §9 G).
//!
//! The world is a *composite object, not a third authority* (§12.4 principle 2
//! holds): the [`DianLu`] and the definition space stay the single authorities;
//! the world aggregates one definition space + many circuits.
//!
//! - [`CircuitWorld::instance_registry`] — the identity registries, promoted
//!   from per-build scratch to a persistent per-world field (one per circuit
//!   key). A circuit's registry survives across rebuilds: the same canonical
//!   path keeps the same [`NodeId`] (D1) and the same net label keeps the same
//!   [`NetId`] (D9) unless the object was deleted (tombstones never reused).
//! - [`CircuitWorld::circuits`] — one [`DianLu`] per [`CircuitKey`] (design
//!   §12.2: one definition space, many circuits).
//! - [`CircuitWorld::invalidation`] — the def→circuits reverse index (design
//!   §12.6): which circuits a changed def must re-instantiate. Built from each
//!   circuit's frozen circuit→def edges (Phase F).
//! - [`CircuitWorld::checkpoints`] — per-circuit versioned snapshots (design
//!   §11.5.1): the registry journal tail separator + the alive node set + the
//!   net point-set snapshots. [`CircuitWorld::diff_versions`] answers "what
//!   changed in this circuit between two builds" without external state.
//!
//! The world is the home of the Phase G diff/checkpoint/invalidation surface;
//! the single-circuit CLI path (`mcb_instantiate`) is unchanged.

use crate::db::defregistry::{def_id, kind_of, DefId};
use crate::instant::dianlu::DianLu;
use crate::instant::identity::{CircuitKey, IdentityRegistry, NodeId};
use crate::instant::lane::{Net, NetId, PointId};
use crate::McIds;
use crate::McSpaceName;
use std::collections::{HashMap, HashSet};
use std::error::Error;

/// One net's point-set snapshot at one checkpoint (design §11.5.1 / D9) — the
/// carry for unlabeled-net identity across builds (a labeled net's identity is
/// its interned label; an unlabeled net's is its snapshot + overlap matching).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSnapshot {
    /// The net id at the checkpoint (interned for labeled nets, build-scoped
    /// for unlabeled ones).
    pub id: NetId,
    /// The net's name attribute (`None` for an unlabeled net).
    pub label: Option<String>,
    /// Member physical points in derived-net order.
    pub points: Vec<PointId>,
}

/// One versioned circuit checkpoint (design §11.5.1): the registry journal
/// tail separator + the alive node set + the net point-set snapshots. Each
/// build of a circuit appends one; `diff_versions` compares two of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitCheckpoint {
    /// Per-circuit monotonic version (the journal tail separator).
    pub version: u64,
    /// Alive (canonical path, node id) pairs at this version — the node half
    /// of the checkpoint's alive set.
    pub alive_nodes: Vec<(String, NodeId)>,
    /// Net point-set snapshots in derived-net order.
    pub nets: Vec<NetSnapshot>,
}

impl CircuitCheckpoint {
    /// Capture one checkpoint from the circuit's persistent registry and its
    /// finalized net layer (design §11.5.1).
    pub fn capture(registry: &IdentityRegistry, nets: &[Net], version: u64) -> Self {
        let mut alive_nodes = registry.alive_paths();
        alive_nodes.sort();
        let nets = nets
            .iter()
            .map(|n| NetSnapshot {
                id: n.id,
                label: n.label.clone(),
                points: n.points.clone(),
            })
            .collect();
        CircuitCheckpoint {
            version,
            alive_nodes,
            nets,
        }
    }
}

/// One node's change between two checkpoints (design §10): the path appeared
/// (added) or disappeared (removed). Compared by canonical path; the id is the
/// id the path carried on the side it was alive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePathChange {
    /// Canonical path of the node (`main.c1`, ...).
    pub path: String,
    /// The node id on the side it was alive.
    pub id: NodeId,
    /// Alive at t2, not alive at t1.
    pub added: bool,
    /// Alive at t1, not alive at t2.
    pub removed: bool,
}

/// One net's member delta between two checkpoints (design §10.4 / D9) — the
/// per-net granularity (`VDD: +c2.1, -c3.2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetDelta {
    /// The matched net's label (`None` for an unlabeled net).
    pub label: Option<String>,
    /// Points present at t2 but not at t1.
    pub added: Vec<PointId>,
    /// Points present at t1 but not at t2.
    pub removed: Vec<PointId>,
}

/// The version diff between two checkpoints of one circuit (design §10.2 /
/// §11.5.1): the node-set changes plus the per-net member deltas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitDiff {
    /// Node additions / removals, ordered by path.
    pub nodes: Vec<NodePathChange>,
    /// Per-net member deltas (labeled nets matched by label; unlabeled nets
    /// matched by bipartite overlap, D9).
    pub nets: Vec<NetDelta>,
}

/// The aggregate container (design §11.3 D10 / plan §9 G).
#[derive(Default)]
pub struct CircuitWorld {
    /// Default flat-table start id for every instantiation in this world.
    start_id: u32,
    /// Persistent identity registries — one per circuit key. Promoted from
    /// per-build scratch: a circuit's registry survives across rebuilds, so
    /// the same path keeps the same `NodeId` and the same label keeps the same
    /// `NetId` (D1 / D9) unless the object was deleted.
    instance_registry: HashMap<CircuitKey, IdentityRegistry>,
    /// One instantiation per circuit key — one definition space, many
    /// circuits (design §12.2).
    circuits: HashMap<CircuitKey, DianLu>,
    /// Invalidation domain (design §12.6): def → affected circuit keys, built
    /// from each circuit's frozen circuit→def edges (Phase F).
    invalidation: HashMap<DefId, Vec<CircuitKey>>,
    /// Per-circuit versioned checkpoints (design §11.5.1).
    checkpoints: HashMap<CircuitKey, Vec<CircuitCheckpoint>>,
    /// Per-circuit checkpoint version counter (monotonic — the journal tail
    /// separator).
    next_version: HashMap<CircuitKey, u64>,
}

impl CircuitWorld {
    /// An empty world whose instantiations seed the flat projection at
    /// `start_id`.
    pub fn new(start_id: u32) -> Self {
        CircuitWorld {
            start_id,
            ..CircuitWorld::default()
        }
    }

    // ========================================================================
    // Read side
    // ========================================================================

    /// The instantiated circuit under `key`, if any.
    pub fn circuit(&self, key: &CircuitKey) -> Option<&DianLu> {
        self.circuits.get(key)
    }

    /// Every instantiated circuit in the world.
    pub fn circuits(&self) -> impl Iterator<Item = (&CircuitKey, &DianLu)> {
        self.circuits.iter()
    }

    /// The persistent identity registry of `key`, if the circuit was built.
    pub fn registry(&self, key: &CircuitKey) -> Option<&IdentityRegistry> {
        self.instance_registry.get(key)
    }

    /// The versioned checkpoints of `key` (empty until the first build).
    pub fn checkpoints(&self, key: &CircuitKey) -> Option<&[CircuitCheckpoint]> {
        self.checkpoints.get(key).map(|v| v.as_slice())
    }

    /// The circuit keys a def change would invalidate (design §12.6).
    pub fn invalidated(&self, def: DefId) -> &[CircuitKey] {
        self.invalidation
            .get(&def)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    // ========================================================================
    // Build side
    // ========================================================================

    /// Instantiate (or re-instantiate) the circuit of `entry`. The circuit's
    /// persistent registry is carried across builds, the invalidation index is
    /// rebuilt from the fresh circuit→def edges, and a versioned checkpoint is
    /// appended. Returns the circuit key of the built circuit.
    pub fn instantiate(&mut self, entry: &McSpaceName) -> Result<CircuitKey, Box<dyn Error>> {
        let (matched_uri, _def) = crate::build::pass2::resolve_entry_module(entry)?;
        let key = CircuitKey::new(&matched_uri, &entry.ident.to_string());

        let mut registry = self
            .instance_registry
            .remove(&key)
            .unwrap_or_else(|| IdentityRegistry::new(key.clone()));

        let dl = crate::build::pass2::mcb_instantiate_with_registry(
            entry,
            self.start_id,
            &mut registry,
        )?;

        // The authoritative registry stays in the world (the DianLu holds a
        // frozen clone for its own queries).
        self.instance_registry.insert(key.clone(), registry);
        self.index_invalidation(&key, &dl);
        self.push_checkpoint(&key, &dl);
        self.circuits.insert(key.clone(), dl);
        Ok(key)
    }

    /// Re-instantiate every circuit affected by `def_ids` (design §12.6): the
    /// def→circuits reverse index picks the affected set, and only those
    /// circuits are rebuilt — unaffected circuits keep their DianLu and their
    /// identity registries untouched. Returns the rebuilt circuit keys. A
    /// failed rebuild keeps the old circuit in place (the caller re-runs on
    /// demand).
    pub fn rebuild_invalidated(&mut self, def_ids: &[DefId]) -> Vec<CircuitKey> {
        let mut affected: Vec<CircuitKey> = Vec::new();
        let mut seen: HashSet<CircuitKey> = HashSet::new();
        for id in def_ids {
            if let Some(keys) = self.invalidation.get(id) {
                for key in keys {
                    if seen.insert(key.clone()) {
                        affected.push(key.clone());
                    }
                }
            }
        }
        let mut rebuilt = Vec::new();
        for key in &affected {
            let entry = McSpaceName::new(&McIds::from(key.top.clone()), key.entry_uri.clone());
            if self.instantiate(&entry).is_ok() {
                rebuilt.push(key.clone());
            }
        }
        rebuilt
    }

    /// Drop every circuit, registry, checkpoint and invalidation entry —
    /// a world reset.
    pub fn clear(&mut self) {
        self.instance_registry.clear();
        self.circuits.clear();
        self.invalidation.clear();
        self.checkpoints.clear();
        self.next_version.clear();
    }

    // ========================================================================
    // Version diff (design §10.2 / §11.5.1)
    // ========================================================================

    /// The diff between checkpoints `t1` and `t2` of `key` (0-based checkpoint
    /// positions): node set changes + per-net member deltas. `None` when the
    /// circuit or a checkpoint position does not exist.
    pub fn diff_versions(&self, key: &CircuitKey, t1: usize, t2: usize) -> Option<CircuitDiff> {
        let cps = self.checkpoints.get(key)?;
        let a = cps.get(t1)?;
        let b = cps.get(t2)?;
        Some(diff_checkpoints(a, b))
    }

    /// Semantic equivalence of two checkpoints of `key` (design §10.3): the
    /// net membership — canonical point paths, labels and ids ignored — is
    /// identical. A renamed net label is NOT a semantic change ("rename not
    /// definition = equivalent").
    pub fn semantic_equivalent(&self, key: &CircuitKey, t1: usize, t2: usize) -> bool {
        let Some(cps) = self.checkpoints.get(key) else {
            return false;
        };
        let (Some(a), Some(b)) = (cps.get(t1), cps.get(t2)) else {
            return false;
        };
        canonical_net_membership(a) == canonical_net_membership(b)
    }

    /// D9 label uniqueness invariant (design §11.5.3): two distinct nets in
    /// one circuit must never carry the same label — a same-name pair means
    /// the nets should have merged, or the input is ambiguous. Returns the
    /// offending labels (empty in a valid circuit).
    pub fn label_violations(&self, key: &CircuitKey) -> Vec<String> {
        let Some(dl) = self.circuits.get(key) else {
            return Vec::new();
        };
        // After net finalization two same-labeled nets collapse onto one
        // interned NetId — a duplicated id across nets is the violation.
        let mut id_labels: HashMap<NetId, Vec<String>> = HashMap::new();
        for net in dl.nets() {
            id_labels
                .entry(net.id)
                .or_default()
                .push(net.label.clone().unwrap_or_default());
        }
        let mut bad: Vec<String> = id_labels
            .into_values()
            .filter(|labels| labels.len() > 1)
            .filter_map(|labels| labels.iter().find(|l| !l.is_empty()).cloned())
            .collect();
        bad.sort();
        bad.dedup();
        bad
    }

    // ========================================================================
    // Internals
    // ========================================================================

    /// Rebuild the def→circuits reverse index for one circuit (design §12.6):
    /// drop the circuit's stale entries, then record every def its frozen
    /// circuit→def edges resolve to (Phase F).
    fn index_invalidation(&mut self, key: &CircuitKey, dl: &DianLu) {
        self.invalidation.retain(|_, keys| {
            keys.retain(|k| k != key);
            !keys.is_empty()
        });
        for dep in dl.deps() {
            if let Some(kind) = kind_of(dep) {
                if let Some(id) = def_id(dep, kind) {
                    let keys = self.invalidation.entry(id).or_default();
                    if !keys.contains(key) {
                        keys.push(key.clone());
                    }
                }
            }
        }
    }

    /// Append one versioned checkpoint (journal tail separator + snapshots).
    fn push_checkpoint(&mut self, key: &CircuitKey, dl: &DianLu) {
        let version = {
            let v = self.next_version.entry(key.clone()).or_insert(0);
            *v += 1;
            *v
        };
        let registry = self
            .instance_registry
            .get(key)
            .expect("the registry is stored before the checkpoint");
        let cp = CircuitCheckpoint::capture(registry, dl.nets(), version);
        self.checkpoints.entry(key.clone()).or_default().push(cp);
    }
}

/// Diff two checkpoints: node set changes + per-net member deltas (D9: labeled
/// nets match by label, unlabeled nets by bipartite overlap).
fn diff_checkpoints(a: &CircuitCheckpoint, b: &CircuitCheckpoint) -> CircuitDiff {
    let a_alive: HashMap<&str, NodeId> = a
        .alive_nodes
        .iter()
        .map(|(p, id)| (p.as_str(), *id))
        .collect();
    let b_alive: HashMap<&str, NodeId> = b
        .alive_nodes
        .iter()
        .map(|(p, id)| (p.as_str(), *id))
        .collect();

    let mut paths: Vec<&str> = a_alive.keys().chain(b_alive.keys()).copied().collect();
    paths.sort_unstable();
    paths.dedup();

    let mut nodes = Vec::new();
    for path in paths {
        match (a_alive.get(path), b_alive.get(path)) {
            (Some(_), None) => nodes.push(NodePathChange {
                path: path.to_string(),
                id: a_alive[path],
                added: false,
                removed: true,
            }),
            (None, Some(_)) => nodes.push(NodePathChange {
                path: path.to_string(),
                id: b_alive[path],
                added: true,
                removed: false,
            }),
            _ => {}
        }
    }

    CircuitDiff {
        nodes,
        nets: net_deltas(&a.nets, &b.nets),
    }
}

/// Per-net member deltas (D9): labeled nets match by label (same label = same
/// net — the interned id is the same across builds); unlabeled nets match by
/// greedy bipartite overlap. Deterministic on both snapshots' orders.
fn net_deltas(a: &[NetSnapshot], b: &[NetSnapshot]) -> Vec<NetDelta> {
    let a_by_label: HashMap<&str, &NetSnapshot> = a
        .iter()
        .filter_map(|n| n.label.as_deref().map(|l| (l, n)))
        .collect();
    // Indices of `a` already matched by overlap (unlabeled nets match at most
    // one t2 net).
    let mut used: HashSet<usize> = HashSet::new();
    let mut out: Vec<NetDelta> = Vec::new();

    for bn in b {
        if let Some(label) = &bn.label {
            let (added, removed) = match a_by_label.get(label.as_str()) {
                Some(an) => (
                    bn.points
                        .iter()
                        .copied()
                        .filter(|p| !an.points.contains(p))
                        .collect(),
                    an.points
                        .iter()
                        .copied()
                        .filter(|p| !bn.points.contains(p))
                        .collect(),
                ),
                None => (bn.points.clone(), Vec::new()),
            };
            out.push(NetDelta {
                label: Some(label.clone()),
                added,
                removed,
            });
            continue;
        }

        // Unlabeled: pick the unmatched t1 net with the largest point overlap;
        // ties by t1 point count, then by snapshot position (deterministic).
        let mut best: Option<(usize, usize)> = None; // (overlap, a index)
        for (ai, an) in a.iter().enumerate() {
            if an.label.is_some() || used.contains(&ai) {
                continue;
            }
            let overlap = an.points.iter().filter(|p| bn.points.contains(p)).count();
            if overlap == 0 {
                continue;
            }
            let better = match best {
                None => true,
                Some((bo, bi)) => {
                    overlap > bo
                        || (overlap == bo && an.points.len() > a[bi].points.len())
                        || (overlap == bo && an.points.len() == a[bi].points.len() && ai < bi)
                }
            };
            if better {
                best = Some((overlap, ai));
            }
        }

        match best {
            Some((_, ai)) => {
                used.insert(ai);
                let an = &a[ai];
                out.push(NetDelta {
                    label: None,
                    added: bn
                        .points
                        .iter()
                        .copied()
                        .filter(|p| !an.points.contains(p))
                        .collect(),
                    removed: an
                        .points
                        .iter()
                        .copied()
                        .filter(|p| !bn.points.contains(p))
                        .collect(),
                });
            }
            None => out.push(NetDelta {
                label: None,
                added: bn.points.clone(),
                removed: Vec::new(),
            }),
        }
    }

    // Unmatched t1 nets (unlabeled, no overlap with any t2 net) are removed.
    for (ai, an) in a.iter().enumerate() {
        if an.label.is_none() && !used.contains(&ai) {
            out.push(NetDelta {
                label: None,
                added: Vec::new(),
                removed: an.points.clone(),
            });
        }
    }
    out
}

/// The canonical net membership of a checkpoint: each net as its sorted
/// (canonical node path, pin ordinal) point set, the multiset of nets sorted.
/// Labels and ids are ignored — a renamed label is not a semantic change.
fn canonical_net_membership(cp: &CircuitCheckpoint) -> Vec<Vec<(String, u32)>> {
    let id_path: HashMap<NodeId, String> = cp
        .alive_nodes
        .iter()
        .map(|(p, id)| (*id, p.clone()))
        .collect();
    let mut nets: Vec<Vec<(String, u32)>> = cp
        .nets
        .iter()
        .map(|n| {
            let mut pts: Vec<(String, u32)> = n
                .points
                .iter()
                .filter_map(|p| id_path.get(&p.node).map(|path| (path.clone(), p.pin.0)))
                .collect();
            pts.sort();
            pts
        })
        .collect();
    nets.sort();
    nets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> CircuitKey {
        CircuitKey::new("/proj/main.mc", "main")
    }

    fn snap(label: Option<&str>, ids: &[u32]) -> NetSnapshot {
        NetSnapshot {
            id: NetId(1),
            label: label.map(str::to_string),
            points: ids
                .iter()
                .map(|p| PointId {
                    node: NodeId(1),
                    pin: crate::db::defmember::DefMemberId(*p),
                })
                .collect(),
        }
    }

    /// D9: a labeled net's member delta is reported per label; a renamed label
    /// is a different net object (the old label's net is removed, the new one
    /// added), while an unlabeled net matches by overlap.
    #[test]
    fn dlu_world__net_deltas_match_labeled_by_label_and_unlabeled_by_overlap() {
        let a = vec![snap(Some("VDD"), &[1, 2, 3])];
        let b = vec![snap(Some("VDD"), &[1, 2, 4])];
        let d = net_deltas(&a, &b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].label.as_deref(), Some("VDD"));
        assert_eq!(
            d[0].added.iter().map(|p| p.pin.0).collect::<Vec<_>>(),
            vec![4],
            "+c4"
        );
        assert_eq!(
            d[0].removed.iter().map(|p| p.pin.0).collect::<Vec<_>>(),
            vec![3],
            "-c3"
        );

        // Unlabeled nets with a 2-point overlap match and report the delta.
        let a2 = vec![NetSnapshot {
            id: NetId(1),
            label: None,
            points: vec![
                PointId {
                    node: NodeId(1),
                    pin: crate::db::defmember::DefMemberId(1),
                },
                PointId {
                    node: NodeId(2),
                    pin: crate::db::defmember::DefMemberId(1),
                },
                PointId {
                    node: NodeId(3),
                    pin: crate::db::defmember::DefMemberId(1),
                },
            ],
        }];
        let b2 = vec![NetSnapshot {
            id: NetId(9),
            label: None,
            points: vec![
                PointId {
                    node: NodeId(2),
                    pin: crate::db::defmember::DefMemberId(1),
                },
                PointId {
                    node: NodeId(3),
                    pin: crate::db::defmember::DefMemberId(1),
                },
                PointId {
                    node: NodeId(4),
                    pin: crate::db::defmember::DefMemberId(1),
                },
            ],
        }];
        let d2 = net_deltas(&a2, &b2);
        assert_eq!(d2.len(), 1);
        assert_eq!(d2[0].label, None);
        assert_eq!(
            d2[0].added.iter().map(|p| p.node.0).collect::<Vec<_>>(),
            vec![4]
        );
        assert_eq!(
            d2[0].removed.iter().map(|p| p.node.0).collect::<Vec<_>>(),
            vec![1]
        );
    }

    /// Checkpoint capture + diff: added / removed nodes are reported by path.
    #[test]
    fn dlu_world__diff_checkpoints_report_node_add_and_remove() {
        let mut reg = IdentityRegistry::new(key());
        let a = reg.intern("main.c1");
        let b = reg.intern("main.c2");
        let cp1 = CircuitCheckpoint::capture(&reg, &[], 1);
        // Delete c2 and add c3.
        reg.delete(b);
        reg.intern("main.c3");
        let cp2 = CircuitCheckpoint::capture(&reg, &[], 2);

        let diff = diff_checkpoints(&cp1, &cp2);
        assert_eq!(diff.nodes.len(), 2);
        assert!(diff
            .nodes
            .iter()
            .any(|n| n.path == "main.c2" && n.removed && n.id == b));
        assert!(diff
            .nodes
            .iter()
            .any(|n| n.path == "main.c3" && n.added && n.id > b));
        assert_eq!(
            diff.nodes.iter().find(|n| n.path == "main.c1").is_none(),
            true,
            "unchanged nodes do not appear"
        );
        let _ = a;
    }

    /// Semantic equivalence ignores labels: the same membership set with a
    /// renamed label is equivalent ("rename not definition").
    #[test]
    fn dlu_world__canonical_membership_ignores_labels() {
        let a = CircuitCheckpoint {
            version: 1,
            alive_nodes: vec![
                ("main".to_string(), NodeId(1)),
                ("main.c1".to_string(), NodeId(2)),
            ],
            nets: vec![snap(Some("VDD"), &[1, 2])],
        };
        let b = CircuitCheckpoint {
            version: 2,
            alive_nodes: vec![
                ("main".to_string(), NodeId(1)),
                ("main.c1".to_string(), NodeId(2)),
            ],
            nets: vec![snap(Some("V5V"), &[1, 2])],
        };
        assert_eq!(canonical_net_membership(&a), canonical_net_membership(&b));
    }
}
