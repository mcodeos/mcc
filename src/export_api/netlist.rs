// Copyright (c) 2026 MCode
//! Netlist export

use crate::McModuleInst;
use crate::NetPoint;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn build_netlist(tree: &McModuleInst, top: &str, format: u8) -> (String, Value, usize) {
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_nets(tree, &mut nets);
    let nets: BTreeMap<String, Vec<String>> = nets
        .into_iter()
        .filter(|(n, _)| n != "NC" && !n.starts_with("__net_"))
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

pub fn collect_nets(inst: &McModuleInst, out: &mut BTreeMap<String, Vec<String>>) {
    for (name, points) in &inst.nets {
        for np in points {
            let pt = pin_label(np);
            let entry = out.entry(name.clone()).or_default();
            if !entry.contains(&pt) {
                entry.push(pt);
            }
        }
    }
    for sub in &inst.sub_modules {
        collect_nets(sub, out);
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
