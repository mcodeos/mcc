// Copyright (c) 2026 MCode
//! KiCad s-expression netlist export (M8)

use crate::instant::insttab::InstTable;
use crate::McModuleInst;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::netlist::collect_nets;

pub fn build_kicad_netlist(
    tree: &McModuleInst,
    table: &InstTable,
    top: &str,
) -> (String, Value, usize) {
    let mut out = String::new();
    out.push_str("(export (version D)\n");
    out.push_str(&format!(
        "  (design\n    (source \"{}\")\n    (date \"{}\"))\n",
        top,
        super::chrono_like_now()
    ));

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
    for conn in &tree.connections {
        for np in &conn.points {
            if let Some((inst, _pin)) = np.path.rsplit_once('.') {
                if !inst.starts_with("__") {
                    inst_set.insert(inst.to_string());
                }
            }
        }
    }
    for sub in &tree.sub_modules {
        collect_instances_from_tree(sub, &mut inst_set);
    }

    for name in &inst_set {
        let class = name_to_class.get(name).map(|c| c.as_str()).unwrap_or("?");
        out.push_str(&format!(
            "    (comp (ref {})\n      (value {})\n      (footprint {}))\n",
            name, class, "?:UNKNOWN"
        ));
    }
    out.push_str("  )\n");

    // Nets
    let mut netmap: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_nets(tree, &mut netmap);
    out.push_str("  (nets\n");
    let mut net_code: u32 = 1;
    for (net_name, points) in &netmap {
        if net_name == "NC" || net_name.starts_with("__net_") {
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

fn collect_instances_from_tree(inst: &McModuleInst, out: &mut BTreeSet<String>) {
    for conn in &inst.connections {
        for np in &conn.points {
            if let Some((inst_name, _pin)) = np.path.rsplit_once('.') {
                if !inst_name.starts_with("__") {
                    out.insert(inst_name.to_string());
                }
            }
        }
    }
    for sub in &inst.sub_modules {
        collect_instances_from_tree(sub, out);
    }
}
