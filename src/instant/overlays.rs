// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase E overlay layer (design §3/§4, plan §9 E): labels and buses leave
//! the frozen modelling tree — `McModuleInst` never carries them — and live
//! in the overlay layer.
//!
//! Two structures live here:
//!
//! - [`ModuleOverlay`]: one module's label registry and bus table. The
//!   construction scratch freezes a fragment per canonical module path into
//!   the [`NetTableStore`](crate::instant::nettab::NetTableStore) (the
//!   same carrier as the Phase D string net tables), and the projection /
//!   query consumers read them back from there via `labels_of` / `buses_of`.
//! - [`Overlays`]: the circuit-level derived overlay on the
//!   [`DianLu`](crate::instant::dianlu::DianLu) — the label → net annotation
//!   overlay (each named net, content is identity, design §12.4 principle 3)
//!   plus the `name_index` / `point_index` lookup indexes (design §5 D5).
//!   Vectors are first-class arena nodes (Phase C), not overlay members; the
//!   description layer (interface bindings, bus groups — Phase G) is a
//!   separate structure.

use crate::instant::identity::NodeId;
use crate::instant::inststore::TreeView;
use crate::instant::lane::{Net, NetId, PointId};
use crate::instant::mc_bus::McBusInst;
use crate::instant::mc_mod::McModuleInst;
use crate::instant::mc_net::NetPoint;
use std::collections::HashMap;

/// One module's label registry and bus table — the frozen per-module overlay
/// data (design §3/§4, plan §9 E item ①).
#[derive(Debug, Clone, Default)]
pub struct ModuleOverlay {
    /// Label registry (label name → net point).
    pub labels: HashMap<String, NetPoint>,
    /// Bus table (bus name → bus instance).
    pub buses: HashMap<String, McBusInst>,
}

impl ModuleOverlay {
    /// Wrap a module's label registry and bus table.
    pub fn new(labels: HashMap<String, NetPoint>, buses: HashMap<String, McBusInst>) -> Self {
        ModuleOverlay { labels, buses }
    }
}

/// Circuit-level derived overlay (design §3/§4, plan §9 E): the label → net
/// annotation overlay and the canonical-symbol lookup indexes, derived per
/// build on the [`DianLu`](crate::instant::dianlu::DianLu). Per-build cache —
/// cross-build persistent interning stays in the identity registry (design
/// §5 D6; no overlapping responsibility).
#[derive(Debug, Clone, Default)]
pub struct Overlays {
    /// Label → net annotation overlay (design §4 `Overlays`): each named net
    /// as `(net id, symbol)`. Content is identity (§12.4 principle 3) — the
    /// net's name attribute, derived from the net layer.
    pub labels: Vec<(NetId, String)>,
    /// `name_index`: canonical symbol → node set (design §5 D5, member-set
    /// normalised): `c[1:2]` — the vector base — hits every member node in
    /// one lookup.
    pub name_index: HashMap<String, Vec<NodeId>>,
    /// `point_index`: canonical symbol → physical point set — a vector slice
    /// / broadcast (`c.1`) hits every member point in one lookup.
    pub point_index: HashMap<String, Vec<PointId>>,
}

impl Overlays {
    /// Derive the per-build overlay (design §5 D5) from the frozen tree and
    /// its derived net layer. Deterministic: `labels` follows derived-net
    /// order, and the index walks the tree in the same order as the identity
    /// resume (dianlu.rs `resume_module`).
    ///
    /// - `labels` — every named net as `(NetId, name)` in derived-net order
    ///   (net labels come first-seen from the lane layer; a net name is the
    ///   net's name attribute — §12.4 principle 3, content is identity).
    /// - `name_index` — canonical symbol → node set. Each node is registered
    ///   under its canonical path (`main.c1`) and its member-set symbol
    ///   (bare name `c1`); a vector's base (`c`) maps to its member node
    ///   set, so `c[1:2]` hits every member node in one lookup.
    /// - `point_index` — canonical symbol → physical point set. A named
    ///   net's points, so a net-name / broadcast lookup hits every member
    ///   point in one pass. Pin-path symbols (`c1.1`) need the def pin
    ///   ledger to reverse ordinals and arrive with the description layer
    ///   (Phase G) — the net-name index is the honest Phase E boundary.
    pub fn derive(tree: &McModuleInst, nets: &[Net], view: &TreeView) -> Self {
        let mut labels: Vec<(NetId, String)> = Vec::new();
        let mut point_index: HashMap<String, Vec<PointId>> = HashMap::new();
        for net in nets {
            let Some(name) = net.label.as_deref() else {
                continue;
            };
            labels.push((net.id, name.to_string()));
            point_index
                .entry(name.to_string())
                .or_default()
                .extend(net.points.iter().copied());
        }

        let mut name_index: HashMap<String, Vec<NodeId>> = HashMap::new();
        index_module(&mut name_index, &tree.name, tree, view);
        Overlays {
            labels,
            name_index,
            point_index,
        }
    }
}

/// Recursive name-index walk over one module's scope, in the same order as
/// the identity resume (dianlu.rs `resume_module`): the module node, its
/// ports, its vectors (base → member node set), its components, and its
/// sub-modules. Every node is keyed by its canonical path and its
/// member-set symbol (bare name).
///
/// Phase C S3-D: children resolve through the [`TreeView`] (arena edges +
/// instance store) instead of the tree's `components` / `sub_modules` Vecs.
fn index_module(
    name_index: &mut HashMap<String, Vec<NodeId>>,
    path: &str,
    module: &McModuleInst,
    view: &TreeView,
) {
    if let Some(id) = module.node_id {
        name_index.entry(path.to_string()).or_default().push(id);
        // The module's own bare name is its member-set symbol — for the root
        // module the canonical path already IS that name, so register once.
        if path != module.name {
            name_index.entry(module.name.clone()).or_default().push(id);
        }
    }
    for port in &module.ports {
        if let Some(id) = port.node_id {
            name_index
                .entry(format!("{path}.{}", port.name))
                .or_default()
                .push(id);
            name_index.entry(port.name.clone()).or_default().push(id);
        }
    }
    for vec in &module.vectors {
        // The vector base names the ordered member set (`c[1:2]` → `c`):
        // one lookup hits every member node. Members are module-level
        // components (the physical member instances stay in `components`);
        // `member_ids` may carry a dotted prefix (a func-invocation scope),
        // so match on the last segment.
        let members: Vec<NodeId> = vec
            .member_ids
            .iter()
            .filter_map(|mid| {
                let member = mid.rsplit('.').next().unwrap_or(mid);
                view.components(module)
                    .find(|c| c.name == member)
                    .and_then(|c| c.node_id)
            })
            .collect();
        if !members.is_empty() {
            name_index
                .entry(vec.base.clone())
                .or_default()
                .extend(members.iter().copied());
        }
        // The grouping node itself is a first-class arena node (Phase C),
        // reachable under its canonical path.
        if let Some(id) = vec.node_id {
            name_index
                .entry(format!("{path}.{}", vec.base))
                .or_default()
                .push(id);
        }
    }
    for comp in view.components(module) {
        if let Some(id) = comp.node_id {
            name_index
                .entry(format!("{path}.{}", comp.name))
                .or_default()
                .push(id);
            name_index.entry(comp.name.clone()).or_default().push(id);
        }
    }
    for sub in view.sub_modules(module) {
        let sub_path = format!("{path}.{}", sub.name);
        index_module(name_index, &sub_path, sub, view);
    }
}
