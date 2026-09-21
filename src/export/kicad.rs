// Copyright (c) 2026 MCode
//! KiCad s-expression netlist export (M8)

use crate::export::{for_each_module_with_arena, NodeArena};
use crate::instant::inststore::InstanceStore;
use crate::instant::insttab::InstTable;
use crate::McModuleInst;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::netlist::PointNaming;

pub fn build_kicad_netlist(
    tree: &McModuleInst,
    table: &InstTable,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
) -> (String, Value, usize) {
    let mut out = String::new();
    out.push_str("(export (version D)\n");
    out.push_str(&format!("  (design\n    (source \"{}\"))\n", top));

    // Components
    out.push_str("  (components\n");
    let mut name_to_class: HashMap<String, String> = HashMap::new();
    for comp in table.get_components() {
        let inst = comp
            .path
            .rsplit_once('.')
            .map(|(i, _)| i)
            .unwrap_or(&comp.path);
        if !inst.is_empty() && !comp.class_name.is_empty() {
            name_to_class.insert(inst.to_string(), comp.class_name.clone());
        }
    }

    let mut inst_set: BTreeSet<String> = BTreeSet::new();
    collect_instances_from_tree(tree, arena, inst_store, &mut inst_set);

    for name in &inst_set {
        let class = name_to_class.get(name).map(|c| c.as_str()).unwrap_or("?");
        out.push_str(&format!(
            "    (comp (ref {})\n      (value {})\n      (footprint {}))\n",
            name, class, "?:UNKNOWN"
        ));
    }
    out.push_str("  )\n");

    // Nets: copper islands from the flat table (U158) — an island carries the
    // whole node, so no connection point vanishes for being on the far side of
    // a module boundary or on copper no statement ever named.
    let netmap: BTreeMap<String, Vec<String>> =
        super::netlist::island_nets(table, PointNaming::Local);
    out.push_str("  (nets\n");
    let mut net_code: u32 = 1;
    for (net_name, points) in &netmap {
        if net_name == "NC"
            || net_name.starts_with(crate::semantic::basic::mc_bus::McBus::ERROR_PREFIX)
        {
            continue;
        }
        out.push_str(&format!(
            "    (net (code {}) (name \"{}\")\n",
            net_code, net_name
        ));
        for pt in points {
            if let Some((inst, pin)) = pt.rsplit_once('.') {
                out.push_str(&format!("      (node (ref {}) (pin {}))\n", inst, pin));
            }
        }
        out.push_str("    )\n");
        net_code += 1;
    }
    out.push_str("  )\n");

    out.push_str(")\n");
    (out, Value::Null, netmap.len())
}

fn collect_instances_from_tree(
    inst: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    out: &mut BTreeSet<String>,
) {
    for_each_module_with_arena(inst, arena, inst_store, &mut |m| {
        for conn in &m.connections {
            for np in &conn.points {
                if let Some((inst_name, _pin)) = np.path.rsplit_once('.') {
                    if !inst_name.starts_with("__") {
                        out.insert(inst_name.to_string());
                    }
                }
            }
        }
    });
}
