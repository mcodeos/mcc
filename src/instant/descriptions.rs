// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase G description layer (design §12 / plan §9 G item ④): the class
//! template instantiations of one circuit — func expansion groups, bus
//! groups and interface member bindings — as a derived layer on the
//! [`DianLu`](crate::instant::dianlu::DianLu).
//!
//! The physical layer already anchors the first row of the design §12.3
//! template table (a device instance carries `def: DefId` + `params` + `pins`
//! — the ComponentTemplate instantiation). The description layer collects the
//! rest:
//!
//! - [`FuncGroup`] — one func template expansion (module func / component
//!   method body), anchored to the def-space func entry when the host
//!   resolves; participants are the expansion's direct component /
//!   sub-module products, and its lanes reference the statement trunks the
//!   expansion's connections group into.
//! - [`BusGroup`] — one bus bundle (`PWR{VCC, GND}`) as its member points in
//!   declaration order. Buses are writing syntax (no first-class def today),
//!   so the group is content-addressed by name + member points.
//! - [`IfaceBinding`] — one member-table port binding (`UART0::UART.TTL(DCE)`,
//!   `XTAL{X1, X2}`) as its port node + member names + member points.
//! - [`EnumRef`] — enum parameter value references; the def-space enum
//!   template anchor is not resolved at instantiation time today, so the
//!   list stays empty (honest boundary, design §12.5).
//!
//! Description-layer entities are content-addressed (no independent identity
//! — the same discipline as lanes, design §12.5): a template edit re-runs
//! the instantiation and the diff is driven by template identity + member
//! correspondence, never by an id.

use crate::db::defregistry::DefId;
use crate::instant::identity::NodeId;
use crate::instant::inststore::TreeView;
use crate::instant::lane::{PointId, Trunk};
use crate::instant::mc_mod::McModuleInst;
use crate::instant::nettab::NetTableStore;
use crate::instant::overlays::Overlays;
use crate::instant::provenance::ExpansionKind;
use crate::semantic::common::{SourcePos, SourcePosSet};
use std::collections::HashMap;

/// Where a description group was written (design §12.2, extended): the
/// statement trunks it is anchored on, and the source positions that named it.
///
/// This is the group's row of the same axis the clause layer uses — a group is
/// not a source statement itself (it is derived from the writing syntax), so it
/// states its own anchors rather than pretending to be one.
///
/// The two fields are not the same relation for every group kind, and the
/// difference is deliberate: for a func group the trunks are the lanes of the
/// call statement that issued the expansion (what **made** it), while for a bus
/// bundle and a member-table port they are the statements that **name** the
/// group's members — a bundle is a grouping of names, and its members may be
/// named by statements that form no net with each other. Read per kind, never
/// as one uniform "formed by".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupSites {
    /// The statement trunks this group is anchored on, in trunk order.
    pub trunks: Vec<usize>,
    /// The source positions that named the group, in walk order. Empty for a
    /// group whose syntax carries no position at all.
    pub wired_at: SourcePosSet,
}

/// One lane of a func expansion group, referenced content-addressably
/// (design §12.2 `LaneRef`): the statement trunk plus the connection's
/// ordinal inside the expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneRef {
    /// Build-scoped trunk id of the statement the connection grouped into.
    pub trunk: usize,
    /// The connection's ordinal within the func expansion's connections.
    pub ordinal: usize,
}

/// One func template expansion (design §12.3 `FuncGroup`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncGroup {
    /// The func def-space anchor when the host resolves (`None` for
    /// expansions whose host name is not recoverable at instantiation time —
    /// the group is then content-addressed by `name` + `def_site`).
    pub template: Option<DefId>,
    /// Called function name (last segment of a chained call like `uC.i2c`).
    pub name: String,
    /// Function definition site — the content address of the template.
    pub def_site: Option<SourcePos>,
    /// The expansion's direct component / sub-module products.
    pub participants: Vec<NodeId>,
    /// The statement trunks the expansion's connections group into, one per
    /// call statement, in trunk order.
    ///
    /// Read from the record subtree's call sites mapped through the layer's
    /// `span_trunk` table, so a func body's own lines — which are not
    /// statements of any module — are dropped and the user's call is kept.
    pub lanes: Vec<LaneRef>,
    /// The statements that issued the expansion, and the positions they were
    /// written at.
    ///
    /// `lanes` and `sites.trunks` therefore carry the same trunk list today,
    /// and that is not a redundancy to remove: `lanes` is the **lane-side**
    /// anchor (it carries the connection ordinal inside the expansion, 0 until
    /// a per-connection grouping exists) while `trunks` is the site-side one,
    /// the field every group kind answers with. `wired_at` is the half no
    /// trunk id can express — a func body lives in whichever file defines it,
    /// so the call's own position is the only cross-build-stable way to say
    /// where the group was written.
    pub sites: GroupSites,
}

/// One bus bundle as its member points in declaration order (design §12.3
/// `BusGroup`). Buses are writing syntax — no first-class def, so the group
/// is content-addressed by name + member points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusGroup {
    /// The bus bundle name (`PWR`, `uC`, ...).
    pub name: String,
    /// Member points in declaration order (empty when a member has no net).
    pub member_points: Vec<PointId>,
    /// The statements that name this bundle's members. `wired_at` is **empty**
    /// here and that is a stated gap, not an omission: the per-module bus table
    /// stores names and carries no position, and the members' own positions
    /// live on their points, which this layer reaches by `PointId` and cannot
    /// turn back into a position today.
    pub sites: GroupSites,
}

/// One member-table port binding (design §12.3 `IfaceBinding`): an N×1 port
/// (`UART0::UART.TTL(DCE)`, `XTAL{X1, X2}`, `[VDD_3V3, GND]::DC(3.3V)`) as
/// its port node + member names + the member points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfaceBinding {
    /// The port node (canonical `main.UART0` interned in the registry).
    pub port: NodeId,
    /// The port name.
    pub name: String,
    /// Member names in declaration order.
    pub members: Vec<String>,
    /// Member points in declaration order (empty when a member has no net).
    pub points: Vec<PointId>,
    /// The port line's own sites — the port is a declaration, so its wiring
    /// sites are where it was written — plus the statements naming its members.
    pub sites: GroupSites,
}

/// One enum parameter value reference (design §12.3 `EnumRef`). The def-space
/// enum template anchor is not resolved at instantiation time today — the
/// honest boundary keeps the list empty (design §12.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumRef {
    /// The enum def-space anchor when resolved (`None` until the instantiation
    /// layer carries it).
    pub template: Option<DefId>,
    /// The referenced enum value.
    pub value: String,
    /// The instance node the value binds to.
    pub target: NodeId,
    /// Always empty: no enum ref is derived today (see above), so there is no
    /// syntax to anchor. The field exists so every group kind answers the same
    /// question in the same place.
    pub sites: GroupSites,
}

/// The description layer of one circuit (design §12.2): the class template
/// instantiations, derived per build on the [`DianLu`](crate::instant::dianlu::DianLu).
#[derive(Debug, Clone, Default)]
pub struct DescriptionLayer {
    /// Func template expansions (module funcs / component methods), in
    /// module-tree order.
    pub func_groups: Vec<FuncGroup>,
    /// Bus bundles, in module-tree order.
    pub bus_groups: Vec<BusGroup>,
    /// Member-table port bindings, in module-tree order.
    pub iface_bindings: Vec<IfaceBinding>,
    /// Enum parameter references (empty today — see [`EnumRef`]).
    pub enum_refs: Vec<EnumRef>,
    /// Statement span → the trunk of the statement written there. The
    /// statement's source position is the only cross-build-stable key it has
    /// (`Trunk::id` is a build-scoped ordinal), so this is the lookup that
    /// turns "the statement at `uri:line`" into a trunk.
    ///
    /// A table on the layer rather than a local of the derivation: it is a
    /// reading of the lane layer in its own right, and the union of the
    /// two directions (span → trunk here, trunk → span in [`Self::trunk_spans`])
    /// is what makes a group's `trunks` comparable across builds.
    pub span_trunk: HashMap<SourcePos, usize>,
    /// The reverse reading: trunk id → the position of the statement it came
    /// from (`None` for a trunk built from the lane layer's non-statement
    /// sources, which carry no statement span). Parallel to the lane layer's
    /// trunk order, so index = trunk id.
    pub trunk_spans: Vec<Option<SourcePos>>,
}

impl DescriptionLayer {
    /// Derive the description layer from the frozen tree, its statement
    /// trunks, the circuit overlay (point lookups) and the net-table store
    /// (per-module bus tables). Deterministic: modules walk in tree order,
    /// buses / ports in their storage order.
    pub fn derive(
        tree: &McModuleInst,
        lanes: &[Trunk],
        overlays: &Overlays,
        net_store: &NetTableStore,
        view: &TreeView,
    ) -> Self {
        let mut dl = DescriptionLayer::default();
        let span_trunk: HashMap<SourcePos, usize> = lanes
            .iter()
            .filter_map(|t| t.stmt_span.clone().map(|s| (s, t.id)))
            .collect();
        // Point → the statement trunks that name it. Built once and read by
        // every group kind: a bundle's or a port's members are points, and
        // "which statements wrote this group" is a question about those points.
        let mut point_trunks: HashMap<PointId, Vec<usize>> = HashMap::new();
        for t in lanes {
            for (pid, _) in &t.points {
                let named = point_trunks.entry(*pid).or_default();
                if !named.contains(&t.id) {
                    named.push(t.id);
                }
            }
        }
        derive_module(
            tree,
            &tree.name,
            overlays,
            net_store,
            &span_trunk,
            &point_trunks,
            &mut dl,
            view,
        );
        // The tables are published after the walk: the walk reads them, and
        // threading `&dl` through it while also writing `dl` would need the
        // borrow split for nothing.
        let mut trunk_spans: Vec<Option<SourcePos>> = vec![None; lanes.len()];
        for t in lanes {
            if let Some(i) = trunk_spans.get_mut(t.id) {
                *i = t.stmt_span.clone();
            }
        }
        dl.span_trunk = span_trunk;
        dl.trunk_spans = trunk_spans;
        dl
    }
}

/// Recursive derivation over one module's scope, in the same order as the
/// identity resume (dianlu.rs `resume_module`).
///
/// Phase C S3-D: children resolve through the [`TreeView`] (arena edges +
/// store) — `group_products` buckets arena node ids and the sub-module
/// recursion walks the view.
fn derive_module(
    module: &McModuleInst,
    path: &str,
    overlays: &Overlays,
    net_store: &NetTableStore,
    span_trunk: &HashMap<SourcePos, usize>,
    point_trunks: &HashMap<PointId, Vec<usize>>,
    dl: &mut DescriptionLayer,
    view: &TreeView,
) {
    // ── Func groups: every func-kind expansion record, grouped by its
    // products (provenance.rs `group_products`). A func body's statements are
    // themselves calls, so its direct products often live on descendant
    // records — the group aggregates the record's whole descendant subtree
    // (products tag the innermost record). The lanes anchor on the call
    // statement that issued the expansion: expansion connections carry no
    // module statement span (they are span-less or carry func-body spans), so
    // the group references the top-level call site's statement trunk. ──
    let expansion = &module.expansion;
    if !expansion.records.is_empty() {
        let groups = expansion.group_products(view, module);
        for (i, rec) in expansion.records.iter().enumerate() {
            if !matches!(
                rec.kind,
                ExpansionKind::InstanceMethod | ExpansionKind::UserFunc | ExpansionKind::AutoInvoke
            ) {
                continue;
            }
            let mut participants = Vec::new();
            let mut call_sites = SourcePosSet::new();
            let mut stack: Vec<usize> = vec![i];
            while let Some(ri) = stack.pop() {
                let g = &groups.by_record[ri];
                if let Some(cs) = &expansion.records[ri].call_site {
                    call_sites.insert(cs.clone());
                }
                // S3-D: the groups carry arena node ids directly (was
                // `components[ci].node_id` through the tree Vec).
                for &id in &g.components {
                    if !participants.contains(&id) {
                        participants.push(id);
                    }
                }
                for &id in &g.sub_modules {
                    if !participants.contains(&id) {
                        participants.push(id);
                    }
                }
                for (r, rec2) in expansion.records.iter().enumerate() {
                    if rec2.parent == Some(ri) {
                        stack.push(r);
                    }
                }
            }
            // The caller module also logs the call statement as its own func
            // record (statement attribution); that record instantiates no
            // instances — only the expansion scope carries the participant
            // products. Skip groups without participants: a func group is the
            // virtual grouping of a template expansion's participating
            // instances (design §12.3), and the call site is already a
            // statement trunk.
            if participants.is_empty() {
                continue;
            }
            // The statement the expansion was issued at. `call_sites` is every
            // site the record's subtree was written at — the call statement
            // and, below it, the func body's own lines — and only the
            // module-level statements among those are in `span_trunk`. Mapping
            // the set through the table is what picks the call out; reading
            // the site off the log's outermost record (as this did) answered
            // `None` for every call that expanded through a body, which is
            // every call that has products to group at all.
            let mut trunks: Vec<usize> = call_sites
                .iter()
                .filter_map(|cs| span_trunk.get(cs).copied())
                .collect();
            trunks.sort_unstable();
            trunks.dedup();
            let lanes: Vec<LaneRef> = trunks
                .iter()
                .map(|&trunk| LaneRef { trunk, ordinal: 0 })
                .collect();
            dl.func_groups.push(FuncGroup {
                template: None,
                name: rec.func_name.clone(),
                def_site: rec.def_site.clone(),
                participants,
                lanes,
                sites: GroupSites {
                    trunks,
                    wired_at: call_sites,
                },
            });
        }
    }

    // ── Bus groups: the module's frozen bus table (curly port bundles and
    // bus accesses), each bundle's members in declaration order. ──
    for bus in net_store.buses_of(path).values() {
        let member_points: Vec<PointId> = bus
            .members
            .iter()
            .flat_map(|m| overlays.point_index.get(m).into_iter().flatten().copied())
            .collect();
        let mut trunks: Vec<usize> = Vec::new();
        for p in &member_points {
            for &t in point_trunks.get(p).into_iter().flatten() {
                if !trunks.contains(&t) {
                    trunks.push(t);
                }
            }
        }
        trunks.sort_unstable();
        dl.bus_groups.push(BusGroup {
            name: bus.name.clone(),
            member_points,
            sites: GroupSites {
                trunks,
                // The bus table holds names, not positions — see `BusGroup`.
                wired_at: SourcePosSet::new(),
            },
        });
    }

    // ── Interface bindings: N×1 member-table ports, members in declaration
    // order with their physical points. ──
    for port in &module.ports {
        if port.bus_members.is_empty() {
            continue;
        }
        let Some(pid) = port.node_id else {
            continue;
        };
        let points: Vec<PointId> = port
            .bus_members
            .iter()
            .flat_map(|m| overlays.point_index.get(m).into_iter().flatten().copied())
            .collect();
        let mut trunks: Vec<usize> = Vec::new();
        for p in &points {
            for &t in point_trunks.get(p).into_iter().flatten() {
                if !trunks.contains(&t) {
                    trunks.push(t);
                }
            }
        }
        trunks.sort_unstable();
        dl.iface_bindings.push(IfaceBinding {
            port: pid,
            name: port.name.clone(),
            members: port.bus_members.clone(),
            points,
            sites: GroupSites {
                trunks,
                // The port line is the port's own declaration site, and it is
                // the only position this group has: a port is written once.
                wired_at: port.net_point.src_pos.clone(),
            },
        });
    }

    // ── Enum refs: empty by design (see `EnumRef`) — honest boundary. ──

    for sub in view.sub_modules(module) {
        derive_module(
            sub,
            &format!("{path}.{}", sub.name),
            overlays,
            net_store,
            span_trunk,
            point_trunks,
            dl,
            view,
        );
    }
}
