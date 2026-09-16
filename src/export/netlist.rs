// Copyright (c) 2026 MCode
//! Netlist export

use crate::export::NodeArena;
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::nettab::NetTableStore;
use crate::McModuleInst;
use crate::NetPoint;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// How a net point names its owning instance.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PointNaming {
    /// The name the instance carries inside its own module (`F1.1`) — what the
    /// netlist and KiCad exports print.
    Local,
    /// The instance's hierarchical path (`main.F1.1`) — the key a flat-table
    /// lookup needs to read the instance's class back.
    Hierarchical,
}

pub fn build_netlist(
    tree: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
    format: u8,
    net_store: &NetTableStore,
) -> (String, Value, usize) {
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_nets(
        tree,
        arena,
        inst_store,
        net_store,
        PointNaming::Local,
        &mut nets,
    );
    let nets: BTreeMap<String, Vec<String>> = nets
        .into_iter()
        .filter(|(n, _)| {
            n != "NC"
                && !crate::instant::mc_net::is_anon_net_name(n)
                && !n.starts_with(crate::semantic::basic::mc_bus::McBus::ERROR_PREFIX)
        })
        .collect();
    let count = nets.len();
    if format == 1 {
        let items: Vec<Value> = nets
            .iter()
            .map(|(name, points)| json!({ "name": name, "points": points }))
            .collect();
        (String::new(), Value::Array(items), count)
    } else {
        let mut out = String::new();
        out.push_str(&format!("# Netlist: top={}\n", top));
        out.push_str(&format!("# Generated: {}\n\n", super::chrono_like_now()));
        for (name, points) in &nets {
            out.push_str(&format!("{}: {}\n", name, points.join(" ")));
        }
        (out, Value::Null, count)
    }
}

/// Collect the flat netlist from the tree-level string net tables (Phase D —
/// sourced from the frozen `net_store`, never from the tree). Walks the
/// module tree arena-first with the canonical module path (the same path the
/// store keys on: `main`, `main.ldo`, ...) and merges every module's net
/// points into one name-keyed map.
///
/// `naming` decides whether a point carries the instance's local name or its
/// hierarchical path; the walk computes the module path either way.
pub fn collect_nets(
    inst: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    net_store: &NetTableStore,
    naming: PointNaming,
    out: &mut BTreeMap<String, Vec<String>>,
) {
    let view = TreeView::new(arena, inst_store);
    let mut walk = |_m: &McModuleInst, path: &str, out: &mut BTreeMap<String, Vec<String>>| {
        let Some(table) = net_store.get(path) else {
            return;
        };
        for (name, points) in table {
            for np in points {
                let pt = pin_label(np, path, naming);
                let entry = out.entry(name.clone()).or_default();
                if !entry.contains(&pt) {
                    entry.push(pt);
                }
            }
        }
    };
    collect_nets_impl(inst, &view, &inst.name.clone(), &mut walk, out);
}

fn collect_nets_impl(
    inst: &McModuleInst,
    view: &TreeView,
    path: &str,
    f: &mut impl FnMut(&McModuleInst, &str, &mut BTreeMap<String, Vec<String>>),
    out: &mut BTreeMap<String, Vec<String>>,
) {
    f(inst, path, out);
    for sub in view.sub_modules(inst) {
        let sub_path = format!("{path}.{}", sub.name);
        collect_nets_impl(sub, view, &sub_path, f, out);
    }
}

/// Label one net point for the flat map. A point with no owner (a port or a
/// label) keeps its own path under either naming.
fn pin_label(np: &NetPoint, module_path: &str, naming: PointNaming) -> String {
    let Some(owner) = &np.owner else {
        return np.path.clone();
    };
    let inst = match naming {
        PointNaming::Local => owner.clone(),
        PointNaming::Hierarchical => format!("{module_path}.{owner}"),
    };
    format!("{}.{}", inst, last_segment(&np.path))
}

fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::common::IOType;

    #[test]
    fn pin_label_carries_the_module_path_only_when_asked() {
        let pin = NetPoint::with_owner("C1.1", "C1", IOType::InOut);
        assert_eq!(pin_label(&pin, "main", PointNaming::Local), "C1.1");
        assert_eq!(
            pin_label(&pin, "main", PointNaming::Hierarchical),
            "main.C1.1"
        );
        assert_eq!(
            pin_label(&pin, "main.ldo", PointNaming::Hierarchical),
            "main.ldo.C1.1"
        );
    }

    #[test]
    fn a_point_without_an_owner_keeps_its_own_path() {
        let label = NetPoint::new("V5V", IOType::Label);
        assert_eq!(pin_label(&label, "main", PointNaming::Local), "V5V");
        assert_eq!(pin_label(&label, "main", PointNaming::Hierarchical), "V5V");
    }
}
