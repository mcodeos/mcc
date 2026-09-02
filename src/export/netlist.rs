// Copyright (c) 2026 MCode
//! Netlist export

use crate::export::NodeArena;
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::nettab::NetTableStore;
use crate::McModuleInst;
use crate::NetPoint;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn build_netlist(
    tree: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
    format: u8,
    net_store: &NetTableStore,
) -> (String, Value, usize) {
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_nets(tree, arena, inst_store, net_store, &mut nets);
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
pub fn collect_nets(
    inst: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    net_store: &NetTableStore,
    out: &mut BTreeMap<String, Vec<String>>,
) {
    let view = TreeView::new(arena, inst_store);
    let mut walk = |_m: &McModuleInst, path: &str, out: &mut BTreeMap<String, Vec<String>>| {
        let Some(table) = net_store.get(path) else {
            return;
        };
        for (name, points) in table {
            for np in points {
                let pt = pin_label(np);
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

fn pin_label(np: &NetPoint) -> String {
    if let Some(owner) = &np.owner {
        format!("{}.{}", owner, last_segment(&np.path))
    } else {
        np.path.clone()
    }
}

fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}
