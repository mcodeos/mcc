// Copyright (c) 2026 MCode
//! BOM (Bill of Materials) export

use crate::McModuleInst;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn build_bom(tree: &McModuleInst, top: &str, format: u8) -> (String, Value, usize) {
    let mut comps = collect_component_instances(tree);
    comps.sort();
    comps.dedup();

    let agg: BTreeMap<String, Vec<String>> = {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (kind, name) in &comps {
            m.entry(kind.clone()).or_default().push(name.clone());
        }
        m
    };

    let count = agg.len();
    match format {
        1 => {
            let items: Vec<Value> = agg
                .iter()
                .map(|(c, names)| {
                    json!({
                        "class": c,
                        "value": "",
                        "description": "",
                        "package": "",
                        "count": names.len(),
                        "refdes": names,
                    })
                })
                .collect();
            (String::new(), Value::Array(items), count)
        }
        4 => {
            let mut out = String::new();
            out.push_str("class,value,description,package,count,refdes\n");
            for (c, names) in &agg {
                let refdes = names.join(",");
                out.push_str(&format!(
                    "{},,,,{},{}\n",
                    super::csv_escape(c),
                    names.len(),
                    super::csv_escape(&refdes),
                ));
            }
            (out, Value::Null, count)
        }
        _ => {
            let mut out = String::new();
            out.push_str(&format!("# BOM: top={}\n", top));
            out.push_str(&format!("# Generated: {}\n", super::chrono_like_now()));
            let w_class = agg.keys().map(|c| c.len()).max().unwrap_or(5).max(5);
            out.push_str(&format!(
                "{:<w_c$}  {:>5}  refdes\n",
                "class",
                "count",
                w_c = w_class,
            ));
            for (c, names) in &agg {
                out.push_str(&format!(
                    "{:<w_c$}  {:>5}  {}\n",
                    c,
                    names.len(),
                    names.join(", "),
                    w_c = w_class,
                ));
            }
            (out, Value::Null, count)
        }
    }
}

fn collect_component_instances(inst: &McModuleInst) -> Vec<(String, String)> {
    let mut out: BTreeSet<(String, String)> = BTreeSet::new();
    collect_components_in_inst(inst, &mut out);
    out.into_iter().collect()
}

fn collect_components_in_inst(inst: &McModuleInst, out: &mut BTreeSet<(String, String)>) {
    for conn in &inst.connections {
        for np in &conn.points {
            if let Some((instance, _pin)) = np.path.rsplit_once('.') {
                if !instance.is_empty() && !instance.starts_with("__") {
                    let prefix = instance
                        .chars()
                        .next()
                        .map(|c| c.to_ascii_uppercase().to_string())
                        .unwrap_or_else(|| "?".into());
                    out.insert((prefix, instance.to_string()));
                }
            }
        }
    }
    for sub in &inst.sub_modules {
        collect_components_in_inst(sub, out);
    }
}
