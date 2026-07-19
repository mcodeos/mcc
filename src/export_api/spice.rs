// Copyright (c) 2026 MCode
//! SPICE netlist export

use crate::instant::inst_table::InstTable;
use crate::McComponentInst;
use crate::McModuleInst;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::netlist::collect_nets;

pub fn build_spice(tree: &McModuleInst, table: &InstTable, top: &str) -> (String, Value, usize) {
    let mut out = String::new();
    out.push_str(&format!("* SPICE netlist: top={}\n", top));
    out.push_str(&format!("* Generated: {}\n\n", super::chrono_like_now()));
    out.push_str(&format!(".SUBCKT {}\n", top));

    let mut name_to_class: HashMap<String, String> = HashMap::new();
    for comp in table.get_components() {
        let inst_name = comp
            .path
            .rsplit_once('.')
            .map(|(i, _)| i)
            .unwrap_or(&comp.path);
        let class = comp.class_name.clone();
        if !inst_name.is_empty() && !class.is_empty() {
            name_to_class.insert(inst_name.to_string(), class);
        }
    }

    let mut netmap: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_nets(tree, &mut netmap);

    let mut inst_nodes: HashMap<String, BTreeSet<String>> = HashMap::new();

    for (net_name, points) in &netmap {
        if net_name == "NC" || net_name.starts_with("__net_") {
            continue;
        }
        let node = net_name.replace('.', "_").replace('-', "_");
        for pt in points {
            if let Some((inst, _pin)) = pt.rsplit_once('.') {
                inst_nodes
                    .entry(inst.to_string())
                    .or_default()
                    .insert(node.clone());
            }
        }
    }

    let mut total: usize = 0;
    for (inst, nodes) in &inst_nodes {
        let node_list: Vec<&String> = nodes.iter().collect();
        let class = name_to_class.get(inst).map(|c| c.as_str()).unwrap_or(inst);
        let prefix = spice_prefix_for_class(class);
        if node_list.len() >= 2 {
            out.push_str(&format!(
                "{}{} {} {}\n",
                prefix, inst, node_list[0], node_list[1]
            ));
            total += 1;
        }
    }

    out.push_str(&format!(".ENDS {}\n\n.END\n", top));
    (out, Value::Null, total)
}

fn spice_prefix_for_class(class: &str) -> String {
    let up = class.to_uppercase();
    if up.starts_with("RES") || up == "R" {
        "R".into()
    } else if up.starts_with("CAP") || up == "C" {
        "C".into()
    } else if up.starts_with("IND") || up == "L" {
        "L".into()
    } else if up.starts_with("DIO")
        || up.starts_with("MOSFET")
        || up.starts_with("MOS")
        || up.starts_with("FET")
        || up == "D"
    {
        "D".into()
    } else {
        let first = class.chars().next().unwrap_or('X').to_ascii_uppercase();
        match first {
            'R' | 'C' | 'L' | 'D' => first.to_string(),
            _ => "X".into(),
        }
    }
}

#[allow(dead_code)]
fn spice_resistor_nodes(c: &McComponentInst) -> (String, String) {
    let mut pins: Vec<String> = Vec::new();
    for (_pin_name, np) in &c.pins {
        pins.push(np.path.clone());
    }
    if pins.len() >= 2 {
        (pins[0].clone(), pins[1].clone())
    } else if pins.len() == 1 {
        (pins[0].clone(), "0".to_string())
    } else {
        ("0".to_string(), "0".to_string())
    }
}
