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
//! (`A -> B -> C`) splits into per-pair connections and a vector-receiver
//! func call (`c[1:2].Cap([VDD, GND])`) into per-member wirings (vec-dianlu
//! §7.6 per-member dispatch, read as a group of independent statements) — so
//! the collector groups connections by their statement span back into the one
//! statement trunk (contract: trunk count = statement count).
//!
//! Physical points are [`PointId`] = `(NodeId, DefMemberId)` (design §4, D1):
//! component pins (device node + def member id) and module ports (module
//! node + port member id). Module ports take their member id from the def's
//! registry-owned ledger (T4) for child instances; the root module's io
//! ports are the circuit boundary and stay positionally anchored, so a
//! boundary rename is a label-only change (world-equivalence §10.3).
//! Interface members,
//! labels and bus members are not physical points — they resolve to `None`
//! for now (interface members bind to their pin / port in the description
//! layer, Phase G; net-anchored labels are a Phase G step), and the lane
//! list stays informational: an unresolvable endpoint simply skips its lane.
//!
//! Honest boundary of the collector: scalar chain statements plus vector
//! rows. A per-member dispatch keeps its bundle — member endpoints of the
//! same (vector node, member pin) collapse into a [`PointGroup::Slice`] lane
//! (design §4, keep-bundle), a row-aligned member column vs an equal number
//! of distinct scalar endpoints (`c[1:2] -> [VDD, GND]`) emits per-index
//! `One -> One` lanes (§5.2 row zip), and a both-sides-member alignment
//! (`c[1:2].1 -> d[1:2].1`) emits one `Slice -> Slice` lane that `derive_nets`
//! zips positionally. Same-name pad groups / quarantined bracket literals
//! still emit no lanes.

use crate::db::defmember::DefMemberId;
use crate::db::defregistry::{self, DefKind};
use crate::instant::arena::NodeArena;
use crate::instant::identity::{IdentityRegistry, NodeId};
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_mod::McModuleInst;
use crate::instant::mc_net::{ConnectionInst, NetPoint};
use crate::semantic::common::{McSpaceName, SourcePos};
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
    /// (design §4). Produced by a per-member dispatch
    /// (`c[1:2].Cap([VDD, GND])` runs every member against the shared scalar
    /// net, vec-dianlu §7.6) and by both-sides member alignment
    /// (`c[1:2].1 -> d[1:2].1`, one `Slice -> Slice` lane that `derive_nets`
    /// zips positionally).
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

/// One thing a statement asked the lane layer for that could not be honored
/// as written (U373). The lane layer is a pure function with no diagnostic
/// channel, so defects are recorded on the trunk they belong to and the
/// DianLu assembly converts them to diagnostics on the net-diag face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaneDefect {
    /// A vector member the statement's bundle was assembling is not
    /// resolvable to a physical point — the declared member name finds no
    /// component in the scope, or that component owns no arena node. The
    /// member is left out of its slice, so a positional zip pairs the
    /// remaining members by declared order: a mid-slice loss shifts every
    /// later pairing. Warning, never silent (U373 hidden B).
    MemberLost {
        /// The statement that assembled the bundle.
        span: Option<SourcePos>,
        /// The vector's declared base name (`"c"` for `c[1:2]`).
        vector: String,
        /// The declared member name that does not resolve.
        member: String,
    },
    /// A both-sides slice pair (`c[1:2].1 -> d[1:2].1`) whose two ordered
    /// member lists differ in length after resolution. The lane is withheld —
    /// pairing unequal slices positionally would silently truncate the longer
    /// side or misalign every member past the first loss — and the mismatch
    /// is reported (E4181; the written form is already rejected at Pass1 by
    /// the shape gate, so reaching the lane layer means member drift).
    PairWidth {
        /// The slice-pairing statement.
        span: Option<SourcePos>,
        /// Resolvable member count on the left side.
        left: usize,
        /// Resolvable member count on the right side.
        right: usize,
    },
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
    /// different net names (a per-member dispatch `c[1:2].Cap([VDD, GND])`
    /// wires members to VDD and GND). Points include the resolvable side of a
    /// skipped lane, so a `GND -> c1.2` statement still names the net that
    /// `c1.2` joins.
    pub points: Vec<(PointId, Option<String>)>,
    /// Directed point-group pairs in written order.
    pub lanes: Vec<Lane>,
    /// Things the statement asked for that could not be honored as written
    /// ([`LaneDefect`]) — reported by the DianLu assembly, never silent.
    pub defects: Vec<LaneDefect>,
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
    // The root module is the circuit's own boundary: its io ports are
    // positionally anchored (see `resolve_port_ordinal`), every nested
    // module is a child whose ports take ledger identities.
    collect_module(root, &view, &mut trunks, true);
    trunks
}

fn collect_module(inst: &McModuleInst, view: &TreeView, out: &mut Vec<Trunk>, is_root: bool) {
    // One trunk per source statement (contract: trunk count = statement
    // count): the engine may explode one statement into several connections
    // (chain pairs, per-member dispatch wirings) that share the statement's
    // source span — they re-collapse here. Span-less engine-generated
    // connections
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
        out.push(trunk_from_connections(
            inst,
            span,
            conns,
            out.len(),
            view,
            is_root,
        ));
    }
    for sub in view.sub_modules(inst) {
        collect_module(sub, view, out, false);
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
    is_root: bool,
) -> Trunk {
    let mut seen: HashSet<PointId> = HashSet::new();
    let mut points: Vec<(PointId, Option<String>)> = Vec::new();
    let mut lanes: Vec<Lane> = Vec::new();
    let mut defects: Vec<LaneDefect> = Vec::new();

    // Vector-row aggregation (design §4 / §11.3 ③, plan §9 D item ①):
    // member endpoints of the same (vector node, member pin) collapse into
    // one `Slice` lane at statement end, so a per-member dispatch (or a
    // member column genuinely sharing a net) stays a bundle instead of
    // exploding member-by-member.
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
            .map(|p| resolve_point(inst, p, view, is_root))
            .collect();
        let members: Vec<Option<(NodeId, DefMemberId)>> = conn
            .points
            .iter()
            .map(|p| vector_member(inst, p, view).map(|(vn, pin, _)| (vn, pin)))
            .collect();

        // Resolvable physical points of the statement, written order, deduped
        // — interned by the net layer even when the lane is skipped (see
        // [`Trunk::points`]). The label candidate is this connection's net
        // name, so a per-member dispatch statement keeps per-member naming.
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
        let (members, lost) = order_members(inst, vec_node, &acc.members, view);
        record_lost(&mut defects, span.clone(), inst, vec_node, lost);
        let make_slice = |members: Vec<PointId>| PointGroup::Slice {
            base: PointId {
                node: vec_node,
                pin,
            },
            members,
        };
        // ── Row-aligned member column vs equal-count distinct endpoints ──
        // A member bundle whose other-side endpoints are as many DISTINCT
        // scalars as there are members is a positional row zip
        // (`c[1:2] -> [VDD, GND]` → c1.2↔VDD, c2.2↔GND, vec-dianlu §5.2):
        // the members share nothing, so a `Slice -> One`/`One -> Slice` fan
        // would falsely short them (the slice merges every member onto every
        // endpoint in `derive_nets`). Emit per-index `One -> One` lanes.
        // When endpoints are FEWER than members, the members genuinely share
        // them (the §7.6 per-member dispatch fan, e.g. `c[1:2].Cap([VDD,GND])`
        // runs each member onto the same scalar net) — keep the Slice fan.
        let mut one_side_lanes =
            |members: Vec<PointId>, scalars: &[PointId], member_on_source: bool| {
                if scalars.len() == members.len() {
                    // Positional 1:1 alignment.
                    for (m, s) in members.iter().zip(scalars.iter()) {
                        let (a, b) = if member_on_source {
                            (PointGroup::One(*m), PointGroup::One(*s))
                        } else {
                            (PointGroup::One(*s), PointGroup::One(*m))
                        };
                        lanes.push(Lane {
                            source: a,
                            target: b,
                        });
                    }
                } else {
                    // Fan: slice carries every member to each shared endpoint.
                    let slice = make_slice(members);
                    for s in scalars {
                        let (a, b) = if member_on_source {
                            (slice.clone(), PointGroup::One(*s))
                        } else {
                            (PointGroup::One(*s), slice.clone())
                        };
                        lanes.push(Lane {
                            source: a,
                            target: b,
                        });
                    }
                }
            };
        one_side_lanes(members.clone(), &acc.targets, true);
        one_side_lanes(members.clone(), &acc.sources, false);
    }

    // Both-sides-member alignment: one `Slice -> Slice` lane per bundle pair.
    // Member order follows each side's declared member-set order, so the
    // positional zip in `derive_nets` aligns c1.1↔d1.1, c2.1↔d2.1. A pair
    // whose resolved widths differ is withheld and reported (U373): the
    // written form is rejected at Pass1, so reaching here with unequal widths
    // means a member was lost after acceptance — zipping would truncate or
    // misalign.
    for (src_key, tgt_key) in slice_pairs {
        let (src_vec, src_pin) = src_key;
        let (tgt_vec, tgt_pin) = tgt_key;
        let (src_members, src_lost) = order_members(inst, src_vec, &bundles[&src_key].members, view);
        let (tgt_members, tgt_lost) = order_members(inst, tgt_vec, &bundles[&tgt_key].members, view);
        record_lost(&mut defects, span.clone(), inst, src_vec, src_lost);
        record_lost(&mut defects, span.clone(), inst, tgt_vec, tgt_lost);
        if src_members.len() != tgt_members.len() {
            defects.push(LaneDefect::PairWidth {
                span: span.clone(),
                left: src_members.len(),
                right: tgt_members.len(),
            });
            continue;
        }
        lanes.push(Lane {
            source: PointGroup::Slice {
                base: PointId {
                    node: src_vec,
                    pin: src_pin,
                },
                members: src_members,
            },
            target: PointGroup::Slice {
                base: PointId {
                    node: tgt_vec,
                    pin: tgt_pin,
                },
                members: tgt_members,
            },
        });
    }

    Trunk {
        id,
        stmt_span: span,
        points,
        lanes,
        defects,
    }
}

/// Record the members [`order_members`] could not resolve as
/// [`LaneDefect::MemberLost`] under `vector`'s declared base name.
fn record_lost(
    defects: &mut Vec<LaneDefect>,
    span: Option<SourcePos>,
    inst: &McModuleInst,
    vec_node: NodeId,
    lost: Vec<String>,
) {
    if lost.is_empty() {
        return;
    }
    let vector = inst
        .vectors
        .iter()
        .find(|v| v.node_id == Some(vec_node))
        .map(|v| v.base.clone())
        .unwrap_or_default();
    for member in lost {
        let defect = LaneDefect::MemberLost {
            span: span.clone(),
            vector: vector.clone(),
            member,
        };
        // The same bundle can be ordered twice in one trunk (the fan face
        // and the slice-pair face each call `order_members`); one loss is
        // one report per statement.
        if !defects.contains(&defect) {
            defects.push(defect);
        }
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
/// written order, never sorted — §11.2 ordering contract). Returns the
/// ordered members plus the declared member names whose component lookup
/// fails (no component under the declared name, or the component owns no
/// arena node) — the caller reports them, never silently dropping one (U373
/// hidden B). A declared member the statement simply did not touch is absent
/// from `members` by construction and is not a loss; only a member the
/// declaration names but the scope cannot resolve is.
fn order_members(
    inst: &McModuleInst,
    vec_node: NodeId,
    members: &[PointId],
    view: &TreeView,
) -> (Vec<PointId>, Vec<String>) {
    let Some(vec) = inst.vectors.iter().find(|v| v.node_id == Some(vec_node)) else {
        return (members.to_vec(), Vec::new());
    };
    let mut by_node: HashMap<NodeId, PointId> = HashMap::new();
    for m in members {
        by_node.insert(m.node, *m);
    }
    let mut ordered = Vec::new();
    let mut lost = Vec::new();
    for mid in &vec.member_ids {
        let node = view
            .components(inst)
            .find(|c| c.name == mid.as_str())
            .and_then(|c| c.node_id);
        match node {
            Some(node) => {
                if let Some(pid) = by_node.get(&node).copied() {
                    ordered.push(pid);
                }
            }
            // The declaration names an instance the scope cannot resolve:
            // the member can never join a bundle — report, don't drop silent.
            None => lost.push(mid.clone()),
        }
    }
    (ordered, lost)
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
/// ②):
/// - a component pin in the statement's own module scope → `(component node,
///   def pin ledger id)` — this also covers interface / bus members: Pass2
///   normalizes `U2.SPI.SCLK`, `U1.UART0.TX` and idx aliases like `G1.GPIO1`
///   to the physical pin path (`U2.1` etc.), so the leaf is a ledger pin id;
/// - a sub-module port (`sub1.clk`) → `(child module node, port member id
///   from the child def's ledger)`;
/// - the scope module's own port (`A`, owner-less) → `(module node, port
///   member id)` — for the root module the port ordinal is used directly
///   (the circuit boundary anchors positionally, so a boundary rename is a
///   label-only change; see [`resolve_port_ordinal`]).
///
/// Returns `None` only for non-physical points: an owner-less path that is
/// not a module port (a bare net label), an unknown owner, or an unresolvable
/// pin leaf. Statements whose endpoints were rejected upstream (bracket
/// literals, `[A,B][1]`-style group subscripts, an unequal row count that the
/// §5.3.1/§5.3.3 gate already rejected) never reach this point. A `PortInst`
/// is a module boundary point, so both the parent-side reference (`sub1.clk`)
/// and the sub-module's own reference (`clk`) land on the SAME `PointId` —
/// the derived net layer then merges a parent net with the sub-module's
/// internal net through the port (the boundary is transparent to
/// connectivity).
fn resolve_point(
    inst: &McModuleInst,
    p: &NetPoint,
    view: &TreeView,
    is_root: bool,
) -> Option<PointId> {
    match &p.owner {
        Some(owner) => {
            if let Some(comp) = view.components(inst).find(|c| c.name == owner.as_str()) {
                resolve_comp_pin(comp, p)
            } else if let Some(sub) = view.sub_modules(inst).find(|s| s.name == owner.as_str()) {
                let port_name = p.path.rsplit('.').next().unwrap_or(&p.path);
                // A referenced sub-module is always a child instance — its
                // ports take ledger identities (never the circuit boundary).
                resolve_port_ordinal(sub, port_name, false)
            } else {
                None
            }
        }
        // Owner-less points: the module's own port (found in the port table)
        // or a bare net label that is not a port (left unresolved — a
        // net-anchored label needs the label's own physical anchor, a Phase G
        // description-layer step). The scope module's own ports are the
        // circuit boundary exactly when this scope is the root module.
        None => resolve_port_ordinal(inst, &p.path, is_root),
    }
}

/// Component pin → `(device node, def member id)`. The id comes from the
/// component def's registry-owned account ledger (T4) — stable across
/// re-parses, so a mid-table pin insert across def edits never shifts later
/// pins' `PointId`s (invariant C).
fn resolve_comp_pin(comp: &McComponentInst, p: &NetPoint) -> Option<PointId> {
    let pin_name = p.path.rsplit('.').next().unwrap_or(&p.path);
    point_of_comp_pin(comp, pin_name)
}

/// Component pin → `PointId`, from the pin's own member name.
///
/// The single authority for "which physical point is this component pin":
/// both the net layer here and the flat table's pin rows (`insttab`) resolve
/// through it, so a pin's `PointId` is computed one way in the whole build.
/// `None` when the component owns no arena node or the pin is not a live
/// ledger member.
pub(crate) fn point_of_comp_pin(comp: &McComponentInst, pin_name: &str) -> Option<PointId> {
    let node = comp.node_id?;
    let pin = comp_pin_member_id(comp, pin_name)?;
    Some(PointId { node, pin })
}

/// The stable member id of a component pin: the def's registry-owned ledger
/// first (T4; the ledger merges by name across re-parse), with a fallback to
/// the parse artifact's declaration order for defs the registry does not
/// hold — synthetic component defs and directly-constructed instances in
/// tests never pass through `register`, so their ids follow the historical
/// first-registration order (identical to a fresh ledger).
fn comp_pin_member_id(comp: &McComponentInst, pin_name: &str) -> Option<DefMemberId> {
    if !comp.def.uri.is_empty() {
        let sn = McSpaceName::new(&comp.def.name, comp.def.uri.clone());
        if let Some(id) = defregistry::def_member_id_of(&sn, DefKind::Component, pin_name) {
            return Some(id);
        }
    }
    comp.def
        .pins
        .decl_order
        .iter()
        .position(|pid| pid == pin_name)
        .map(|ord| DefMemberId(ord as u32))
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
    let pin = comp_pin_member_id(comp, pin_name)?;
    Some((vec.node_id?, pin, PointId { node, pin }))
}

/// Module port → `(module node, port member id)`. A module's own port is the
/// module node plus the port's id in the module def's registry-owned port
/// ledger (synced from the built port table at instantiation); ports are not
/// def pins, but their ledger plays the same role — a port inserted
/// mid-table across def edits no longer shifts the later ports' ids
/// (invariant C). The root module instance is the exception: its io ports
/// are the circuit's physical boundary, where a rename is a pure label
/// change (world-equivalence §10.3), so they stay anchored by their
/// positional ordinal in the built port table (`positional_boundary = true`).
/// The positional ordinal is also the fallback when a child def is not a
/// registered identity (func-expanded synthetic modules) or its ledger was
/// never synced (defect/error paths) — on a fresh registration the ledger id
/// equals that ordinal, so the fallback is behavior-neutral.
pub(crate) fn resolve_port_ordinal(
    module: &McModuleInst,
    port_name: &str,
    positional_boundary: bool,
) -> Option<PointId> {
    let node = module.node_id?;
    let pin = if positional_boundary {
        port_ordinal_fallback(module, port_name)
    } else if !module.def_uri.is_empty() {
        let sn = McSpaceName::new(&module.def.name, module.def_uri.clone());
        defregistry::def_member_id_of(&sn, DefKind::Module, port_name)
            .or_else(|| port_ordinal_fallback(module, port_name))
    } else {
        port_ordinal_fallback(module, port_name)
    }?;
    Some(PointId { node, pin })
}

/// Historical positional port ordinal — `port_name`'s slot in the module's
/// built port table (see [`resolve_port_ordinal`]).
fn port_ordinal_fallback(module: &McModuleInst, port_name: &str) -> Option<DefMemberId> {
    let ord = module.ports.iter().position(|p| p.name == port_name)?;
    Some(DefMemberId(ord as u32))
}

// Net layer — union-find equivalence derivation (design §11.3 ③ "net layer")

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
    /// The statement trunks that **formed** this net, in first-seen order: a
    /// trunk is recorded when one of its lanes actually merged two distinct
    /// member classes here. A trunk that only names a member — a lane whose
    /// other endpoint resolves to nothing, or a redundant merge — is **not**
    /// recorded, which is what keeps this from being a list of everything the
    /// net is near. Semantic: "made by", not "mentioned by".
    pub trunks: Vec<usize>,
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
    // The statement trunks that formed each net, as one representative member
    // point per effective merge: `(trunk id, a member point of the merge)`.
    // Recorded here, at the union, because this is the only place that knows a
    // merge happened at all — the interning loop above sees points, not edges.
    let mut formed: Vec<(usize, usize)> = Vec::new();
    for trunk in trunks {
        for lane in &trunk.lanes {
            match (&lane.source, &lane.target) {
                (PointGroup::One(a), PointGroup::One(b)) => {
                    if let (Some(&ia), Some(&ib)) = (index.get(a), index.get(b)) {
                        if union_find_union(&mut parent, ia, ib) {
                            formed.push((trunk.id, ia));
                        }
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
                            if union_find_union(&mut parent, ia, im) {
                                formed.push((trunk.id, ia));
                            }
                        }
                    }
                }
                (PointGroup::Slice { members, .. }, PointGroup::One(b)) => {
                    let Some(&ib) = index.get(b) else { continue };
                    for m in members {
                        if let Some(&im) = index.get(m) {
                            if union_find_union(&mut parent, im, ib) {
                                formed.push((trunk.id, ib));
                            }
                        }
                    }
                }
                // Both sides preserved slices (member-aligned statement,
                // `c[1:2].1 -> d[1:2].1`): positional zip — c1.1↔d1.1,
                // c2.1↔d2.1 — never a cross product.
                (PointGroup::Slice { members: m1, .. }, PointGroup::Slice { members: m2, .. }) => {
                    for (a, b) in m1.iter().zip(m2.iter()) {
                        if let (Some(&ia), Some(&ib)) = (index.get(a), index.get(b)) {
                            if union_find_union(&mut parent, ia, ib) {
                                formed.push((trunk.id, ia));
                            }
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

    // Point index -> net slot, so a merge recorded at the union can be read
    // back as "which net did this trunk form".
    let mut slot_of_point: Vec<usize> = vec![0; points.len()];
    for (slot, idxs) in members.iter().enumerate() {
        for &i in idxs {
            slot_of_point[i] = slot;
        }
    }
    let mut formed_trunks: Vec<Vec<usize>> = vec![Vec::new(); members.len()];
    for (tid, pi) in formed {
        let t = &mut formed_trunks[slot_of_point[pi]];
        if !t.contains(&tid) {
            t.push(tid);
        }
    }

    members
        .into_iter()
        .enumerate()
        .map(|(slot, idxs)| Net {
            id: NetId(slot as u32),
            points: idxs.iter().map(|&i| points[i]).collect(),
            label: idxs.iter().find_map(|&i| labels[i].clone()),
            trunks: formed_trunks[slot].clone(),
        })
        .collect()
}

fn union_find_find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        parent[x] = union_find_find(parent, parent[x]);
    }
    parent[x]
}

/// Merge two classes. Returns `true` when they were distinct and are now one —
/// asking the question here is what lets a caller record "this trunk formed
/// this net" without recording every trunk that merely mentions a member.
fn union_find_union(parent: &mut [usize], a: usize, b: usize) -> bool {
    let ra = union_find_find(parent, a);
    let rb = union_find_find(parent, b);
    if ra != rb {
        // Smaller index wins (stability — deterministic roots).
        if ra < rb {
            parent[rb] = ra;
        } else {
            parent[ra] = rb;
        }
        true
    } else {
        false
    }
}

// Phase G (D9) — persistent net identity

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

// ── U373 locks: hand-built fixture + lane-defect tests ──────────────────────
// The arena/store mutators are `pub(crate)`, so the fixture lives in-crate.
// The module def comes from a parsed empty `main` (`McModule` has private
// fields and no literal constructor); the rest — vector nodes, component
// instances, connections — is laid down directly, mirroring the Phase C
// construction-time pattern (insert each node, then `add_child_grouped`).

#[cfg(test)]
pub(crate) mod test_fixture {
    use super::*;
    use crate::instant::arena::{Node, NodeKind};
    use crate::instant::inststore::NodeInstance;
    use crate::semantic::basic::mc_paramd::McParamDeclares;
    use crate::semantic::common::{ConnOp, IOType};
    use crate::semantic::component::mc_attr::McAttributes;
    use crate::semantic::component::mc_layout::McLayout;
    use crate::semantic::component::mc_pins::{McPin, McPins};
    use crate::semantic::component::McComponent;
    use crate::semantic::mc_func::McFunctions;
    use crate::semantic::mc_inst::McInstances;
    use crate::McIds;
    use std::rc::Rc;
    use std::sync::{Arc, OnceLock};

    pub(crate) const TEST_URI: &str = "/mcc/lane-fixture.mc";

    /// The parsed empty `main` def (parse once, share across tests).
    pub(crate) fn main_def() -> Arc<crate::semantic::module::McModule> {
        static DEF: OnceLock<Arc<crate::semantic::module::McModule>> = OnceLock::new();
        DEF.get_or_init(|| {
            let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            crate::mcc_set_system_root(&crate::cli::datadir::data_root());
            crate::mcc_init();
            let uri: crate::McURI = TEST_URI.into();
            crate::mcc_load_from_string(&uri, "module main {}\n");
            crate::definition_space()
                .workspace_modules()
                .into_iter()
                .find(|(sn, _)| sn.ident.to_string() == "main")
                .expect("module 'main' parsed")
                .1
        })
        .clone()
    }

    /// A two-pin CAP def (`1`, `2` in declaration order; unregistered in the
    /// def registry, so pin ids resolve through the `decl_order` fallback —
    /// the same fallback lane.rs applies to unregistered defs).
    fn cap_def() -> Arc<McComponent> {
        let mut pins = McPins::new();
        for pid in ["1", "2"] {
            pins.pins.insert(
                pid.to_string(),
                McPin {
                    iotype: IOType::In,
                    id: pid.to_string(),
                    names: vec![pid.to_string()],
                    values: Arc::new(vec![]),
                    active_low: false,
                    is_nc: false,
                    attrs: McAttributes::new(),
                },
            );
            pins.decl_order.push(pid.to_string());
        }
        Arc::new(McComponent {
            name: McIds::from("CAP"),
            params: McParamDeclares::new(),
            pins,
            attrs: McAttributes::new(),
            funcs: McFunctions::new(),
            insts: McInstances::new(),
            layout: McLayout {
                left: vec![],
                right: vec![],
                top: vec![],
                bottom: vec![],
            },
            uri: crate::McURI::from("/mcc/cap-def.mc"),
            cond_pins: vec![],
            cond_attrs: vec![],
            cond_errors: Vec::new(),
            span: 0..0,
            anon_counter: 0,
            is_abstract: false,
            variant_base: None,
            adopts: Vec::new(),
        })
    }

    /// A component instance named `name` at arena node `node`.
    pub(crate) fn comp(name: &str, node: NodeId) -> (NodeId, NodeInstance) {
        let mut inst = McComponentInst::new(name, cap_def());
        inst.node_id = Some(node);
        (node, NodeInstance::Component(Rc::new(inst)))
    }

    /// A vector grouping node over `member_ids`.
    pub(crate) fn vector(
        node: NodeId,
        base: &str,
        member_ids: &[&str],
    ) -> crate::instant::mc_mod::McVectorInst {
        crate::instant::mc_mod::McVectorInst {
            base: base.to_string(),
            member_names: member_ids.iter().map(|m| m.to_string()).collect(),
            member_ids: member_ids.iter().map(|m| m.to_string()).collect(),
            shape: None,
            node_id: Some(node),
        }
    }

    /// One exploded per-member connection `a -> b` (`a`/`b` are
    /// `owner.pin` paths), carrying `span` so the collector groups the
    /// per-member connections back into one statement trunk.
    pub(crate) fn conn(id: u32, a: &str, b: &str, offset: u32) -> ConnectionInst {
        let mut pa = NetPoint::new(a, IOType::In, None);
        let mut pb = NetPoint::new(b, IOType::In, None);
        pa.owner = Some(a.split('.').next().unwrap_or(a).to_string());
        pb.owner = Some(b.split('.').next().unwrap_or(b).to_string());
        let mut c = ConnectionInst::new(id, vec![pa, pb]);
        c.op = Some(ConnOp::Series);
        c.source_span = Some(SourcePos::new(TEST_URI, offset));
        c
    }

    /// The frozen tree + companion arena/store. `c_members` / `d_members` are
    /// the vector member sets; every listed member gets a component instance
    /// in the store EXCEPT the names in `missing` (the U373 drift: a declared
    /// member that never materialized). `conns` are the exploded per-member
    /// connections of one statement.
    pub(crate) fn build(
        c_members: &[&str],
        d_members: &[&str],
        missing: &[&str],
        conns: Vec<ConnectionInst>,
    ) -> (
        McModuleInst,
        NodeArena,
        InstanceStore,
        std::collections::HashMap<String, NodeId>,
    ) {
        let root = NodeId(1);
        let vec_c = NodeId(2);
        let vec_d = NodeId(3);
        let mut next = 10u32;
        let mut members: Vec<NodeId> = Vec::new();
        let mut contents: Vec<(NodeId, NodeInstance)> = Vec::new();
        let mut nodes_of: std::collections::HashMap<String, NodeId> =
            std::collections::HashMap::new();
        for name in c_members.iter().chain(d_members.iter()) {
            if missing.contains(name) {
                continue; // declared but never materialized — the drift
            }
            let node = NodeId(next);
            next += 1;
            members.push(node);
            nodes_of.insert(name.to_string(), node);
            contents.push(comp(name, node));
        }

        let mut arena = NodeArena::new(root);
        arena.insert(Node {
            id: root,
            kind: NodeKind::Module,
            parent: None,
            children: Vec::new(),
            name: "main".to_string(),
        });
        arena.insert(Node {
            id: vec_c,
            kind: NodeKind::Vector,
            parent: Some(root),
            children: Vec::new(),
            name: "c".to_string(),
        });
        arena.insert(Node {
            id: vec_d,
            kind: NodeKind::Vector,
            parent: Some(root),
            children: Vec::new(),
            name: "d".to_string(),
        });
        arena.add_child_grouped(root, vec_c, NodeKind::Vector);
        arena.add_child_grouped(root, vec_d, NodeKind::Vector);
        for (i, node) in members.iter().enumerate() {
            arena.insert(Node {
                id: *node,
                kind: NodeKind::Device,
                parent: Some(root),
                children: Vec::new(),
                name: if i < c_members.len() {
                    c_members[i].to_string()
                } else {
                    d_members[i - c_members.len()].to_string()
                },
            });
            arena.add_child_grouped(root, *node, NodeKind::Device);
        }

        let mut store = InstanceStore::default();
        for (id, inst) in contents {
            store.insert(id, inst);
        }

        let mut tree = McModuleInst::new("main", main_def());
        tree.node_id = Some(root);
        tree.vectors = vec![vector(vec_c, "c", c_members), vector(vec_d, "d", d_members)];
        tree.connections = conns;
        (tree, arena, store, nodes_of)
    }
}


#[cfg(test)]
mod tests {
    use super::test_fixture::{build, conn, TEST_URI};
    use super::{
        collect_stmt_trunks, derive_nets, ConnectionInst, LaneDefect, PointGroup, PointId, Trunk,
        Net,
    };
    use crate::instant::identity::NodeId;

    /// One exploded statement (`c[1:2].1 -> d[1:2].1`): every per-member
    /// connection shares the statement span, so the collector groups them
    /// back into one trunk. Returns the trunk, the derived nets, and the
    /// member-name → arena-node map.
    fn member_pair_trunk(
        c_members: &[&str],
        d_members: &[&str],
        missing: &[&str],
    ) -> (Trunk, Vec<Net>, std::collections::HashMap<String, NodeId>) {
        let conns: Vec<ConnectionInst> = c_members
            .iter()
            .zip(d_members.iter())
            .enumerate()
            .map(|(i, (c, d))| conn(i as u32, &format!("{c}.1"), &format!("{d}.1"), 100))
            .collect();
        let (tree, arena, store, nodes) = build(c_members, d_members, missing, conns);
        let trunks = collect_stmt_trunks(&tree, &arena, &store);
        assert_eq!(trunks.len(), 1, "one statement span → one trunk");
        let nets = derive_nets(&trunks);
        (trunks.into_iter().next().unwrap(), nets, nodes)
    }

    /// U373 grid 1 (green lock): `c[1:2].1 -> d[1:2].1` with every member
    /// materialized — one positional Slice→Slice lane, zero defects, two
    /// 2-point nets, never a cross product.
    #[test]
    fn member_lane__equal_width_slice_pair_is_quiet_and_positional() {
        let (trunk, nets, nodes) = member_pair_trunk(&["c1", "c2"], &["d1", "d2"], &[]);
        assert!(
            trunk.defects.is_empty(),
            "equal width records nothing: {:?}",
            trunk.defects
        );
        assert_eq!(trunk.lanes.len(), 1, "one Slice→Slice lane");
        let (src, tgt) = match (&trunk.lanes[0].source, &trunk.lanes[0].target) {
            (PointGroup::Slice { members: s, .. }, PointGroup::Slice { members: t, .. }) => (s, t),
            other => panic!("expected Slice→Slice, got {other:?}"),
        };
        assert_eq!(src.len(), 2);
        assert_eq!(tgt.len(), 2);
        assert_eq!(nets.len(), 2, "two 2-point nets: {nets:?}");
        let (c1, d1) = (nodes["c1"], nodes["d1"]);
        let joined = nets
            .iter()
            .find(|n| n.points.contains(&PointId { node: c1, pin: src[0].pin }))
            .expect("c1's net");
        assert!(
            joined
                .points
                .iter()
                .any(|p| p.node == d1 && p.pin == tgt[0].pin),
            "c1.1 pairs d1.1, positionally: {joined:?}"
        );
        assert_eq!(joined.points.len(), 2, "no cross product: {joined:?}");
    }

    /// U373 hidden B (the fix): a declared member whose component never
    /// materialized is REPORTED (`MemberLost`), not silently dropped. The
    /// unequal widths the loss produces also fire the width gate
    /// (`PairWidth`) and the lane is WITHHELD — the old zip would have
    /// truncated `d2` silently.
    #[test]
    fn member_lane__lost_member_is_reported_and_lane_withheld() {
        let (trunk, nets, nodes) = member_pair_trunk(&["c1", "c2"], &["d1", "d2"], &["c2"]);
        assert!(
            trunk.defects.iter().any(|d| matches!(d,
                LaneDefect::MemberLost { vector, member, .. }
                if vector == "c" && member == "c2")),
            "the lost member is reported: {:?}",
            trunk.defects
        );
        assert!(
            trunk.defects.iter().any(|d| matches!(d,
                LaneDefect::PairWidth { left: 1, right: 2, .. })),
            "the width drift is reported: {:?}",
            trunk.defects
        );
        assert!(
            !trunk
                .lanes
                .iter()
                .any(|l| matches!(l.source, PointGroup::Slice { .. })),
            "no lane at unequal width: {:?}",
            trunk.lanes
        );
        // Fail open: no c-side point shares a net with any d-side point —
        // the downstream unconnected-pin checks report the hanging ends.
        let (c1, d1, d2) = (nodes["c1"], nodes["d1"], nodes["d2"]);
        for net in &nets {
            let c_side = net.points.iter().any(|p| p.node == c1);
            let d_side = net.points.iter().any(|p| p.node == d1 || p.node == d2);
            assert!(
                !(c_side && d_side),
                "withheld lane merges nothing: {net:?}"
            );
        }
    }

    /// U373 hidden B, the misalignment case: the MIDDLE member lost. The old
    /// positional zip would pair `c3.1 ↔ d2.1` (one step off — miswiring).
    /// Now: `MemberLost` + `PairWidth`, and the lane is withheld.
    #[test]
    fn member_lane__mid_slice_loss_cannot_misalign() {
        let (trunk, nets, _nodes) =
            member_pair_trunk(&["c1", "c2", "c3"], &["d1", "d2", "d3"], &["c2"]);
        assert!(
            trunk.defects.iter().any(|d| matches!(d,
                LaneDefect::MemberLost { vector, member, .. }
                if vector == "c" && member == "c2")),
            "middle loss reported: {:?}",
            trunk.defects
        );
        assert!(
            trunk.defects.iter().any(|d| matches!(d,
                LaneDefect::PairWidth { left: 2, right: 3, .. })),
            "2 vs 3 drift reported: {:?}",
            trunk.defects
        );
        for net in &nets {
            assert_eq!(
                net.points.len(),
                1,
                "no pairing survives the gate — c3.1 must not meet d2.1: {net:?}"
            );
        }
    }

    /// A member the statement simply did not touch is absent from its bundle
    /// by construction and is NOT a loss: a single-member statement stays
    /// quiet and still pairs positionally at width 1.
    #[test]
    fn member_lane__untouched_member_is_not_a_loss() {
        let conns = vec![conn(0, "c1.1", "d1.1", 100)];
        let (tree, arena, store, _nodes) = build(&["c1", "c2"], &["d1", "d2"], &[], conns);
        let trunks = collect_stmt_trunks(&tree, &arena, &store);
        assert!(
            trunks.iter().all(|t| t.defects.is_empty()),
            "an untouched member is not a loss: {:?}",
            trunks[0].defects
        );
        let nets = derive_nets(&trunks);
        assert_eq!(nets.len(), 1, "width 1 pairs positionally: {nets:?}");
        assert_eq!(nets[0].points.len(), 2);
    }

    /// The recorded span is the statement's: a lost member's diagnostic can
    /// cite the defecting statement (AGENTS rule: a diagnostic cites a real
    /// source location).
    #[test]
    fn member_lane__defects_carry_the_statement_span() {
        let (tree, arena, store, _nodes) = build(
            &["c1", "c2"],
            &["d1", "d2"],
            &["c2"],
            vec![conn(0, "c1.1", "d1.1", 100), conn(1, "c2.1", "d2.1", 100)],
        );
        let trunks = collect_stmt_trunks(&tree, &arena, &store);
        let trunk = &trunks[0];
        assert_eq!(trunk.defects.len(), 2, "one loss + one width: {trunk:?}");
        for d in &trunk.defects {
            let span = match d {
                LaneDefect::MemberLost { span, .. } | LaneDefect::PairWidth { span, .. } => span,
            };
            let sp = span.as_ref().expect("the statement span is carried");
            assert_eq!(sp.uri.to_string(), TEST_URI);
            assert_eq!(sp.offset, 100);
        }
    }
}
