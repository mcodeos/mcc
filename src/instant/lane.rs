// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase D lane layer (implementation plan §9 D / design §11.3 ③): the
//! statement-level structured connectivity storage.
//!
//! Every source connection statement of the frozen tree produces one [`Trunk`]
//! carrying its source span and the directed point-group pairs (`Lane`s) in
//! written order. The lane layer keeps each statement's grouping explicit
//! (statement adjacency for drawing / layout, bundle membership for vectors),
//! while the derived electrical nets (union-find equivalence classes) are a
//! separate layer ([`derive_nets`]).
//!
//! One source statement can explode into several `ConnectionInst`s — a chain
//! (`A -> B -> C`) splits into per-pair connections and a vector broadcast
//! (`c[1:2].Cap([VDD, GND])`) into per-member wirings — so the collector
//! groups connections by their statement span back into the one statement
//! trunk (contract: trunk count = statement count).
//!
//! Physical points are [`PointId`] = `(NodeId, DefMemberId)` (design §4, D1):
//! component pins (device node + def pin ledger id) and module ports (module
//! node + port ordinal, the port-ordinal convention). Interface members,
//! labels and bus members are not physical points — they resolve to `None`
//! for now (interface members bind to their pin / port in the description
//! layer, Phase G; net-anchored labels are a Phase G step), and the lane
//! list stays informational: an unresolvable endpoint simply skips its lane.
//!
//! Honest boundary of the collector: scalar chain statements plus vector
//! slices. A vector broadcast keeps its bundle — member endpoints of the
//! same (vector node, member pin) collapse into a [`PointGroup::Slice`] lane
//! (design §4, keep-bundle), and a both-sides-member alignment
//! (`c[1:2].1 -> d[1:2].1`) emits one `Slice -> Slice` lane that `derive_nets`
//! zips positionally. Same-name pad groups / quarantined bracket literals
//! still emit no lanes.

use crate::db::defmember::DefMemberId;
use crate::instant::arena::NodeArena;
use crate::instant::identity::{IdentityRegistry, NodeId};
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_mod::McModuleInst;
use crate::instant::mc_net::{ConnectionInst, NetPoint};
use crate::semantic::common::SourcePos;
use std::collections::{HashMap, HashSet};

/// Global physical point: circuit node + stable pin ordinal (invariant C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointId {
    /// Owning node of the modelling tree.
    pub node: NodeId,
    /// Stable pin ordinal (def member ledger generation).
    pub pin: DefMemberId,
}

impl std::fmt::Display for PointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.node, self.pin.0)
    }
}

/// One side of a lane: a scalar point or a preserved vector slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointGroup {
    /// Scalar point.
    One(PointId),
    /// Vector slice — the bundle is preserved, not exploded member-by-member
    /// (design §4). Produced by a vector broadcast (`c[1:2].Cap([VDD, GND])`,
    /// scalar member against a preserved bundle) and by both-sides member
    /// alignment (`c[1:2].1 -> d[1:2].1`, one `Slice -> Slice` lane that
    /// `derive_nets` zips positionally).
    Slice {
        base: PointId,
        members: Vec<PointId>,
    },
}

/// One directed point-group pair of a connection statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    /// Left (source) side.
    pub source: PointGroup,
    /// Right (target) side.
    pub target: PointGroup,
}

/// Statement-level trunk: one structured trunk per source connection
/// statement (design §4 / §11.3 ③).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trunk {
    /// Build-scoped ordinal (trunks are not persistent objects).
    pub id: usize,
    /// The source statement that produced this trunk.
    pub stmt_span: Option<SourcePos>,
    /// The statement's resolvable physical points in written order
    /// (first-seen, deduplicated), each paired with the label candidate of
    /// the connection that first referenced it (`ConnectionInst::net_name` —
    /// the first label/port owner of that connection). Per-point labels
    /// matter because one statement can explode into connections with
    /// different net names (a broadcast `c[1:2].Cap([VDD, GND])` wires
    /// members to VDD and GND). Points include the resolvable side of a
    /// skipped lane, so a `GND -> c1.2` statement still names the net that
    /// `c1.2` joins.
    pub points: Vec<(PointId, Option<String>)>,
    /// Directed point-group pairs in written order.
    pub lanes: Vec<Lane>,
}

/// Collect the lane layer from a frozen tree: one [`Trunk`] per source
/// connection statement, walking sub-modules through the store-backed view
/// (arena children edges + instance store, Phase C S3).
pub fn collect_stmt_trunks(
    root: &McModuleInst,
    arena: &NodeArena,
    store: &InstanceStore,
) -> Vec<Trunk> {
    let view = TreeView::new(arena, store);
    let mut trunks: Vec<Trunk> = Vec::new();
    collect_module(root, &view, &mut trunks);
    trunks
}

fn collect_module(inst: &McModuleInst, view: &TreeView, out: &mut Vec<Trunk>) {
    // One trunk per source statement (contract: trunk count = statement
    // count): the engine may explode one statement into several connections
    // (chain pairs, vector broadcasts) that share the statement's source
    // span — they re-collapse here. Span-less engine-generated connections
    // (projection trunks) carry no statement and stay per-connection.
    let mut groups: Vec<(Option<SourcePos>, Vec<&ConnectionInst>)> = Vec::new();
    for conn in &inst.connections {
        match &conn.source_span {
            Some(sp) => match groups.iter_mut().find(|(g, _)| g.as_ref() == Some(sp)) {
                Some((_, conns)) => conns.push(conn),
                None => groups.push((Some(sp.clone()), vec![conn])),
            },
            None => groups.push((None, vec![conn])),
        }
    }
    for (span, conns) in groups {
        out.push(trunk_from_connections(inst, span, conns, out.len(), view));
    }
    for sub in view.sub_modules(inst) {
        collect_module(sub, view, out);
    }
}

/// Bundle aggregation state for one (vector node, member pin) key: the member
/// points (written order) and the non-member endpoints, split by the written
/// direction (a member on the source side → the other endpoint is a target).
struct BundleAcc {
    members: Vec<PointId>,
    sources: Vec<PointId>,
    targets: Vec<PointId>,
}

impl Default for BundleAcc {
    fn default() -> Self {
        BundleAcc {
            members: Vec::new(),
            sources: Vec::new(),
            targets: Vec::new(),
        }
    }
}

fn trunk_from_connections(
    inst: &McModuleInst,
    span: Option<SourcePos>,
    conns: Vec<&ConnectionInst>,
    id: usize,
    view: &TreeView,
) -> Trunk {
    let mut seen: HashSet<PointId> = HashSet::new();
    let mut points: Vec<(PointId, Option<String>)> = Vec::new();
    let mut lanes: Vec<Lane> = Vec::new();

    // Vector-slice aggregation (design §4 / §11.3 ③, plan §9 D item ①):
    // member endpoints of the same (vector node, member pin) collapse into
    // one `Slice` lane at statement end, so a broadcast / parallel member
    // wiring stays a bundle instead of exploding member-by-member.
    let mut bundles: HashMap<(NodeId, DefMemberId), BundleAcc> = HashMap::new();
    let mut bundle_order: Vec<(NodeId, DefMemberId)> = Vec::new();
    // Both-sides-member alignment (`c[1:2].1 -> d[1:2].1`): each connection
    // whose two endpoints are both vector members pairs their bundles
    // (source bundle -> target bundle, deduped). One aligned lane per pair
    // is emitted at statement end; `derive_nets` zips the member slices
    // positionally.
    let mut slice_pairs: Vec<((NodeId, DefMemberId), (NodeId, DefMemberId))> = Vec::new();

    for conn in conns {
        let resolved: Vec<Option<PointId>> = conn
            .points
            .iter()
            .map(|p| resolve_point(inst, p, view))
            .collect();
        let members: Vec<Option<(NodeId, DefMemberId)>> = conn
            .points
            .iter()
            .map(|p| vector_member(inst, p, view).map(|(vn, pin, _)| (vn, pin)))
            .collect();

        // Resolvable physical points of the statement, written order, deduped
        // — interned by the net layer even when the lane is skipped (see
        // [`Trunk::points`]). The label candidate is this connection's net
        // name, so a broadcast statement keeps per-member naming.
        for pid in resolved.iter().flatten() {
            if seen.insert(*pid) {
                points.push((*pid, conn.net_name.clone()));
            }
        }

        // A connection touching a vector member aggregates into the bundle;
        // its non-member endpoint is recorded with the written direction
        // (member on the source side → other is a target, and vice versa).
        // A both-sides-member connection (`c[1:2].1 -> d[1:2].1`) pairs the
        // two bundles. Other bundle-expanding connections (same-name pad
        // groups, quarantined phantoms) keep their scalar skip below.
        let has_member = members.iter().any(Option::is_some);
        if has_member {
            for (pair, mk) in resolved.windows(2).zip(members.windows(2)) {
                let (a, b) = (pair[0], pair[1]);
                match (mk[0], mk[1]) {
                    (Some(key), None) => {
                        if let Some(pid) = a {
                            let acc = bundle_entry(&mut bundles, &mut bundle_order, key);
                            if !acc.members.contains(&pid) {
                                acc.members.push(pid);
                            }
                            if let Some(op) = b {
                                if !acc.targets.contains(&op) {
                                    acc.targets.push(op);
                                }
                            }
                        }
                    }
                    (None, Some(key)) => {
                        if let Some(pid) = b {
                            let acc = bundle_entry(&mut bundles, &mut bundle_order, key);
                            if !acc.members.contains(&pid) {
                                acc.members.push(pid);
                            }
                            if let Some(op) = a {
                                if !acc.sources.contains(&op) {
                                    acc.sources.push(op);
                                }
                            }
                        }
                    }
                    (Some(k0), Some(k1)) => {
                        if let (Some(pa), Some(pb)) = (a, b) {
                            let acc0 = bundle_entry(&mut bundles, &mut bundle_order, k0);
                            if !acc0.members.contains(&pa) {
                                acc0.members.push(pa);
                            }
                            let acc1 = bundle_entry(&mut bundles, &mut bundle_order, k1);
                            if !acc1.members.contains(&pb) {
                                acc1.members.push(pb);
                            }
                            if !slice_pairs.iter().any(|(x, y)| *x == k0 && *y == k1) {
                                slice_pairs.push((k0, k1));
                            }
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        if conn.points.iter().any(is_bundle_point) {
            continue;
        }
        for pair in resolved.windows(2) {
            match (pair[0], pair[1]) {
                (Some(a), Some(b)) => lanes.push(Lane {
                    source: PointGroup::One(a),
                    target: PointGroup::One(b),
                }),
                // Unresolvable endpoint (label, bus member): skip the lane —
                // the trunk stays informational.
                _ => {}
            }
        }
    }

    // Assemble the Slice lanes: member order follows the vector's declared
    // member-set order (contract: bundle member order = written order).
    for key in bundle_order {
        let acc = &bundles[&key];
        let (vec_node, pin) = key;
        let members = order_members(inst, vec_node, &acc.members, view);
        let slice = PointGroup::Slice {
            base: PointId {
                node: vec_node,
                pin,
            },
            members,
        };
        for src in &acc.sources {
            lanes.push(Lane {
                source: PointGroup::One(*src),
                target: slice.clone(),
            });
        }
        for tgt in &acc.targets {
            lanes.push(Lane {
                source: slice.clone(),
                target: PointGroup::One(*tgt),
            });
        }
    }

    // Both-sides-member alignment: one `Slice -> Slice` lane per bundle pair.
    // Member order follows each side's declared member-set order, so the
    // positional zip in `derive_nets` aligns c1.1↔d1.1, c2.1↔d2.1.
    for (src_key, tgt_key) in slice_pairs {
        let (src_vec, src_pin) = src_key;
        let (tgt_vec, tgt_pin) = tgt_key;
        let src_slice = PointGroup::Slice {
            base: PointId {
                node: src_vec,
                pin: src_pin,
            },
            members: order_members(inst, src_vec, &bundles[&src_key].members, view),
        };
        let tgt_slice = PointGroup::Slice {
            base: PointId {
                node: tgt_vec,
                pin: tgt_pin,
            },
            members: order_members(inst, tgt_vec, &bundles[&tgt_key].members, view),
        };
        lanes.push(Lane {
            source: src_slice,
            target: tgt_slice,
        });
    }

    Trunk {
        id,
        stmt_span: span,
        points,
        lanes,
    }
}

fn bundle_entry<'a>(
    bundles: &'a mut HashMap<(NodeId, DefMemberId), BundleAcc>,
    order: &mut Vec<(NodeId, DefMemberId)>,
    key: (NodeId, DefMemberId),
) -> &'a mut BundleAcc {
    if !bundles.contains_key(&key) {
        order.push(key);
    }
    bundles.entry(key).or_default()
}

/// Reorder member points by the vector's declared member-set order (strict
/// written order, never sorted — §11.2 ordering contract).
fn order_members(
    inst: &McModuleInst,
    vec_node: NodeId,
    members: &[PointId],
    view: &TreeView,
) -> Vec<PointId> {
    let Some(vec) = inst.vectors.iter().find(|v| v.node_id == Some(vec_node)) else {
        return members.to_vec();
    };
    let mut by_node: HashMap<NodeId, PointId> = HashMap::new();
    for m in members {
        by_node.insert(m.node, *m);
    }
    vec.member_ids
        .iter()
        .filter_map(|mid| {
            let node = view
                .components(inst)
                .find(|c| c.name == mid.as_str())?
                .node_id?;
            by_node.get(&node).copied()
        })
        .collect()
}

/// Whether a point expands to a bundle, so the statement must wait for
/// keep-bundle lanes (design §4 / §11.3 ③). A scalar point never expands:
/// same-name pad groups fan in to multiple physical pads, and bracket /
/// comma literals were quarantined to `@_phantom_<N>` by `NetPoint::new`
/// (a real statement endpoint never carries raw `[` `]` `,`). `member_name`
/// is NOT a bundle signal — name-based matching sets it for scalar pins too
/// (e.g. `c1.1` carries `member_name = "1"`).
fn is_bundle_point(p: &NetPoint) -> bool {
    !p.same_name_pads.is_empty()
        || p.path.starts_with("@_phantom_")
        || p.path.contains(['[', ']', ','])
}

/// Resolve one connection point to a physical point (design §4 / §9 D item
/// ②, port-ordinal convention):
/// - a component pin in the statement's own module scope → `(component node,
///   def pin ledger id)` — this also covers interface / bus members: Pass2
///   normalizes `U2.SPI.SCLK`, `U1.UART0.TX` and idx aliases like `G1.GPIO1`
///   to the physical pin path (`U2.1` etc.), so the leaf is a ledger pin id;
/// - the module's own port (`A`, owner-less) or a sub-module port
///   (`sub1.clk`) → `(module node, port ordinal in the module's port table)`.
///
/// Returns `None` only for non-physical points: an owner-less path that is
/// not a module port (a bare net label), an unknown owner, or an unresolvable
/// pin leaf. Statements whose endpoints were rejected upstream (bracket
/// literals, `[A,B][1]`-style group subscripts, whole-slice broadcasts —
/// §10.4 / name-equivalence R-family) never reach this point. A `PortInst`
/// is a module boundary point, so both the parent-side reference (`sub1.clk`)
/// and the sub-module's own reference (`clk`) land on the SAME `PointId` —
/// the derived net layer then merges a parent net with the sub-module's
/// internal net through the port (the boundary is transparent to
/// connectivity).
fn resolve_point(inst: &McModuleInst, p: &NetPoint, view: &TreeView) -> Option<PointId> {
    match &p.owner {
        Some(owner) => {
            if let Some(comp) = view.components(inst).find(|c| c.name == owner.as_str()) {
                resolve_comp_pin(comp, p)
            } else if let Some(sub) = view.sub_modules(inst).find(|s| s.name == owner.as_str()) {
                let port_name = p.path.rsplit('.').next().unwrap_or(&p.path);
                resolve_port_ordinal(sub, port_name)
            } else {
                None
            }
        }
        // Owner-less points: the module's own port (found in the port table)
        // or a bare net label that is not a port (left unresolved — a
        // net-anchored label needs the label's own physical anchor, a Phase G
        // description-layer step).
        None => resolve_port_ordinal(inst, &p.path),
    }
}

/// Component pin → `(device node, def pin ledger id)`.
fn resolve_comp_pin(comp: &McComponentInst, p: &NetPoint) -> Option<PointId> {
    let node = comp.node_id?;
    let pin_name = p.path.rsplit('.').next().unwrap_or(&p.path);
    let pin = comp.def.pins.ledger.id_of(pin_name)?;
    Some(PointId { node, pin })
}

/// Whether a point is a member of a declared vector instance (plan §9 D item
/// ①): the owner is in some vector's member set. Returns the vector grouping
/// node, the shared member pin (all members of one vector share the same def
/// pin table, so the pin ordinal is identical across members), and the
/// member's own physical point.
fn vector_member(
    inst: &McModuleInst,
    p: &NetPoint,
    view: &TreeView,
) -> Option<(NodeId, DefMemberId, PointId)> {
    let owner = p.owner.as_deref()?;
    let vec = inst
        .vectors
        .iter()
        .find(|v| v.member_ids.iter().any(|m| m == owner))?;
    let comp = view.components(inst).find(|c| c.name == owner)?;
    let node = comp.node_id?;
    let pin_name = p.path.rsplit('.').next().unwrap_or(&p.path);
    let pin = comp.def.pins.ledger.id_of(pin_name)?;
    Some((vec.node_id?, pin, PointId { node, pin }))
}

/// Module port → `(module node, port ordinal)` — the port-ordinal
/// convention: a module's own port is the module node plus the port's
/// position in the module's port table. Ports are not def pins (no member
/// ledger), so the ordinal doubles as the pin slot of the module node.
fn resolve_port_ordinal(module: &McModuleInst, port_name: &str) -> Option<PointId> {
    let node = module.node_id?;
    let ord = module.ports.iter().position(|p| p.name == port_name)?;
    Some(PointId {
        node,
        pin: DefMemberId(ord as u32),
    })
}

// ============================================================================
// Net layer — union-find equivalence derivation (design §11.3 ③ "net layer")
// ============================================================================

/// Build-scoped ordinal of a derived net. Data is re-derived from the lane
/// layer every build (not primary storage); persistent identity (D9) is a
/// Phase G step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetId(pub u32);

/// Derived net: the union-find equivalence class of [`Lane`]s sharing an
/// endpoint (design §11.3 ③ "net layer"). Derived from [`Trunk`]s, never
/// primary storage — the projection `NetTable` stays the authoritative flat
/// netlist (plan §9 D, invariant B).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    /// Build-scoped ordinal (persistent identity is Phase G / D9).
    pub id: NetId,
    /// Member physical points in first-seen written order.
    pub points: Vec<PointId>,
    /// Net name attribute — the first connection label among the net's
    /// member points (the per-point label of [`Trunk::points`]), in written
    /// order. `None` for owner-only nets (e.g. `c1.1 -> c2.1`).
    pub label: Option<String>,
}

/// Derive the net layer from the lane layer: union-find merges every lane
/// that shares an endpoint into one equivalence class (plan §9 D item 2).
/// Points are interned from the trunks' full resolvable point lists (so a
/// statement's label names the net even when the port-boundary lane is
/// skipped), and lanes drive the union edges.
///
/// Both `One`/`One` scalar lanes and `Slice` bundle lanes participate: a
/// scalar-vs-slice lane unions the scalar with every bundle member, and a
/// slice-vs-slice lane unions positionally (c1.1↔d1.1, c2.1↔d2.1 — never a
/// cross product). The bundle base is a grouping node, not a physical point,
/// so it never enters the union. Port/label endpoints that resolve to `None`
/// carry their name on the component side only (the per-point label).
pub fn derive_nets(trunks: &[Trunk]) -> Vec<Net> {
    // Union-find parent array, parallel to `points`.
    let mut index: HashMap<PointId, usize> = HashMap::new();
    let mut points: Vec<PointId> = Vec::new();
    // Per-point label: the first statement net name that labels the point
    // (a named trunk wins over an earlier unnamed claim).
    let mut labels: Vec<Option<String>> = Vec::new();

    for trunk in trunks {
        for (pid, name) in &trunk.points {
            match index.get(pid) {
                Some(&i) => {
                    if labels[i].is_none() && name.is_some() {
                        labels[i] = name.clone();
                    }
                }
                None => {
                    let i = points.len();
                    index.insert(*pid, i);
                    points.push(*pid);
                    labels.push(name.clone());
                }
            }
        }
    }

    let mut parent: Vec<usize> = (0..points.len()).collect();
    for trunk in trunks {
        for lane in &trunk.lanes {
            match (&lane.source, &lane.target) {
                (PointGroup::One(a), PointGroup::One(b)) => {
                    if let (Some(&ia), Some(&ib)) = (index.get(a), index.get(b)) {
                        union_find_union(&mut parent, ia, ib);
                    }
                }
                // A scalar endpoint against a preserved slice unions the
                // endpoint with every bundle member (the members share the
                // scalar's net); the bundle base is a grouping node, not a
                // physical point, so it never enters the union.
                (PointGroup::One(a), PointGroup::Slice { members, .. }) => {
                    let Some(&ia) = index.get(a) else { continue };
                    for m in members {
                        if let Some(&im) = index.get(m) {
                            union_find_union(&mut parent, ia, im);
                        }
                    }
                }
                (PointGroup::Slice { members, .. }, PointGroup::One(b)) => {
                    let Some(&ib) = index.get(b) else { continue };
                    for m in members {
                        if let Some(&im) = index.get(m) {
                            union_find_union(&mut parent, im, ib);
                        }
                    }
                }
                // Both sides preserved slices (member-aligned statement,
                // `c[1:2].1 -> d[1:2].1`): positional zip — c1.1↔d1.1,
                // c2.1↔d2.1 — never a cross product.
                (PointGroup::Slice { members: m1, .. }, PointGroup::Slice { members: m2, .. }) => {
                    for (a, b) in m1.iter().zip(m2.iter()) {
                        if let (Some(&ia), Some(&ib)) = (index.get(a), index.get(b)) {
                            union_find_union(&mut parent, ia, ib);
                        }
                    }
                }
            }
        }
    }

    // Group by root, emitting nets in first-seen point order ([P0-DET] — the
    // union order is deterministic, so the roots are too; HashMap grouping
    // only maps roots to slots, never reorders the points).
    let mut slot_of_root: HashMap<usize, usize> = HashMap::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    for i in 0..points.len() {
        let root = union_find_find(&mut parent, i);
        let slot = *slot_of_root.entry(root).or_insert_with(|| {
            members.push(Vec::new());
            members.len() - 1
        });
        members[slot].push(i);
    }

    members
        .into_iter()
        .enumerate()
        .map(|(slot, idxs)| Net {
            id: NetId(slot as u32),
            points: idxs.iter().map(|&i| points[i]).collect(),
            label: idxs.iter().find_map(|&i| labels[i].clone()),
        })
        .collect()
}

fn union_find_find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        parent[x] = union_find_find(parent, parent[x]);
    }
    parent[x]
}

fn union_find_union(parent: &mut [usize], a: usize, b: usize) {
    let ra = union_find_find(parent, a);
    let rb = union_find_find(parent, b);
    if ra != rb {
        // Smaller index wins (stability — deterministic roots).
        if ra < rb {
            parent[rb] = ra;
        } else {
            parent[ra] = rb;
        }
    }
}

// ============================================================================
// Phase G (D9) — persistent net identity
// ============================================================================

/// Assign persistent identity to the derived net layer (plan §9 G item 5,
/// design §11.1 D9).
///
/// - Labeled nets intern their label into the circuit's persistent
///   [`IdentityRegistry`] — same label, same `NetId` across rebuilds (the
///   label is the net's name attribute, so a net keeps its id when its member
///   set grows or shrinks).
/// - Unlabeled nets carry no stable key; they receive build-scoped ids past
///   the interned range (no collision within the build). Their cross-build
///   identity is carried by the checkpoint net snapshots + bipartite overlap
///   matching, never by the id itself.
/// - Interned labels that no longer appear in the circuit are tombstoned
///   (rename = tombstone + fresh id, the node discipline).
///
/// Deterministic: labeled first (derived-net order), then unlabeled.
pub fn finalize_net_ids(nets: &mut [Net], registry: &mut IdentityRegistry) {
    for net in nets.iter_mut() {
        if let Some(label) = &net.label {
            net.id = registry.intern_net(label);
        }
    }
    let mut next = registry.next_net_id();
    for net in nets.iter_mut() {
        if net.label.is_none() {
            net.id = next;
            next = NetId(next.0 + 1);
        }
    }
    let active: HashSet<String> = nets.iter().filter_map(|n| n.label.clone()).collect();
    registry.reconcile_net_labels(&active);
}
