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
use crate::instant::lane::{PointId, Trunk};
use crate::instant::mc_mod::McModuleInst;
use crate::instant::net_store::NetTableStore;
use crate::instant::overlays::Overlays;
use crate::instant::provenance::ExpansionKind;
use crate::semantic::common::SourcePos;
use std::collections::HashMap;

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
    /// The statement trunks the expansion's connections group into.
    pub lanes: Vec<LaneRef>,
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
    ) -> Self {
        let mut dl = DescriptionLayer::default();
        let span_trunk: HashMap<SourcePos, usize> = lanes
            .iter()
            .filter_map(|t| t.stmt_span.clone().map(|s| (s, t.id)))
            .collect();
        derive_module(tree, &tree.name, overlays, net_store, &span_trunk, &mut dl);
        dl
    }
}

/// Recursive derivation over one module's scope, in the same order as the
/// identity resume (dianlu.rs `resume_module`).
fn derive_module(
    module: &McModuleInst,
    path: &str,
    overlays: &Overlays,
    net_store: &NetTableStore,
    span_trunk: &HashMap<SourcePos, usize>,
    dl: &mut DescriptionLayer,
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
        let groups =
            expansion.group_products(&module.components, &module.sub_modules, &module.connections);
        for (i, rec) in expansion.records.iter().enumerate() {
            if !matches!(
                rec.kind,
                ExpansionKind::InstanceMethod | ExpansionKind::UserFunc | ExpansionKind::AutoInvoke
            ) {
                continue;
            }
            let mut participants = Vec::new();
            let mut stack: Vec<usize> = vec![i];
            while let Some(ri) = stack.pop() {
                let g = &groups.by_record[ri];
                for &ci in &g.components {
                    if let Some(id) = module.components[ci].node_id {
                        if !participants.contains(&id) {
                            participants.push(id);
                        }
                    }
                }
                for &si in &g.sub_modules {
                    if let Some(id) = module.sub_modules[si].node_id {
                        if !participants.contains(&id) {
                            participants.push(id);
                        }
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
            let mut lanes = Vec::new();
            let mut top = i;
            while let Some(p) = expansion.records[top].parent {
                top = p;
            }
            if let Some(cs) = &expansion.records[top].call_site {
                if let Some(&tid) = span_trunk.get(cs) {
                    lanes.push(LaneRef {
                        trunk: tid,
                        ordinal: 0,
                    });
                }
            }
            dl.func_groups.push(FuncGroup {
                template: None,
                name: rec.func_name.clone(),
                def_site: rec.def_site.clone(),
                participants,
                lanes,
            });
        }
    }

    // ── Bus groups: the module's frozen bus table (curly port bundles and
    // bus accesses), each bundle's members in declaration order. ──
    for bus in net_store.buses_of(path).values() {
        let member_points = bus
            .members
            .iter()
            .flat_map(|m| overlays.point_index.get(m).into_iter().flatten().copied())
            .collect();
        dl.bus_groups.push(BusGroup {
            name: bus.name.clone(),
            member_points,
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
        let points = port
            .bus_members
            .iter()
            .flat_map(|m| overlays.point_index.get(m).into_iter().flatten().copied())
            .collect();
        dl.iface_bindings.push(IfaceBinding {
            port: pid,
            name: port.name.clone(),
            members: port.bus_members.clone(),
            points,
        });
    }

    // ── Enum refs: empty by design (see `EnumRef`) — honest boundary. ──

    for sub in &module.sub_modules {
        derive_module(
            sub,
            &format!("{path}.{}", sub.name),
            overlays,
            net_store,
            span_trunk,
            dl,
        );
    }
}
