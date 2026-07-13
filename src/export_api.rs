// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M5 export API — netlist / BOM / SPICE builders shared between the CLI
//! (`cmds/export.rs`) and the JSON-RPC handler (`rpc/handlers.rs`).
//!
//! Lives at lib root so both the binary's `cmds` module (private) and the
//! library's `rpc` module can call into it. Pattern mirrors `query_api` and
//! `search_api`.

use crate::{McAttribute, McComponentInst, McIds, McModuleInst, McURI, NetPoint};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::panic;

/// Kind of export. `&str` (not enum) to avoid cross-crate visibility issues
/// with the binary's `pub(crate)` cli module.
pub fn kind_from_str(s: &str) -> u8 {
    // 0 = netlist, 1 = bom, 2 = spice
    match s {
        "bom" => 1,
        "spice" => 2,
        _ => 0, // netlist default
    }
}

pub fn kind_to_str(k: u8) -> &'static str {
    match k {
        1 => "bom",
        2 => "spice",
        _ => "netlist",
    }
}

/// Output format. 0=text, 1=json, 2=json-pretty, 3=yaml, 4=csv
pub fn format_from_str(s: &str) -> u8 {
    match s {
        "json" => 1,
        "json-pretty" | "jsonpretty" => 2,
        "yaml" => 3,
        "csv" => 4,
        _ => 0,
    }
}

/// Load project + libs, resolve top module, run Pass2 (with panic guard).
/// Returns the root `McModuleInst` (== `MccProjectTree`).
///
/// For component enumeration (BOM/SPICE) we walk `inst.connections` and
/// extract unique (name, kind) pairs from `NetPoint.owner`. The
/// `InstTable`-based path doesn't work for us because `inst.components`
/// is empty in Pass2 results (the engine populates `connections` instead).
pub fn build_tree(file: &str, top: Option<&str>, libs: &[String]) -> Result<McModuleInst, String> {
    let _ = libs; // load_libs is bin-private; lib only sees the engine state
    let _ = mcc::mcc_load_project(&McURI::from(file));

    let top = match top {
        Some(t) => t.to_string(),
        None => match mcc::mcb_get_first_module_name() {
            Some(t) => t,
            None => return Err("no module found in file (use --top)".into()),
        },
    };

    let ident = McIds::from(top.as_str());
    let uri = McURI::from(file);
    let built = panic::catch_unwind(panic::AssertUnwindSafe(|| mcc::mcc_build(&ident, &uri)));
    match built {
        Ok(Ok(t)) => Ok(t),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".into()),
    }
}

/// Build the export payload for a single kind. Returns
/// `(raw_text, items_json, count)`. `items_json` is `Value::Null` for
/// text/csv modes (the artifact is in `raw_text`); it's a `Value::Array`
/// for JSON mode.
pub fn build_payload(
    tree: &McModuleInst,
    top: &str,
    kind: u8,
    format: u8,
) -> (String, Value, usize) {
    match kind {
        1 => build_bom(tree, top, format),
        2 => build_spice(tree, top),
        _ => build_netlist(tree, top, format),
    }
}

// ============================================================================
// Netlist
// ============================================================================

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
        out.push_str(&format!("# Generated: {}\n\n", chrono_like_now()));
        for (name, points) in &nets {
            out.push_str(&format!("{}: {}\n", name, points.join(" ")));
        }
        (out, Value::Null, count)
    }
}

// ============================================================================
// BOM
// ============================================================================

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
                    csv_escape(c),
                    names.len(),
                    csv_escape(&refdes),
                ));
            }
            (out, Value::Null, count)
        }
        _ => {
            // text (0) — aligned table.
            let mut out = String::new();
            out.push_str(&format!("# BOM: top={}\n", top));
            out.push_str(&format!("# Generated: {}\n", chrono_like_now()));
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

// ============================================================================
// SPICE
// ============================================================================

pub fn build_spice(tree: &McModuleInst, top: &str) -> (String, Value, usize) {
    let mut out = String::new();
    out.push_str(&format!("* SPICE netlist: top={}\n", top));
    out.push_str(&format!("* Generated: {}\n\n", chrono_like_now()));
    out.push_str(&format!(".SUBCKT {}\n", top));
    let mut total: usize = 0;
    // Walk connections to discover component instances.
    for (kind, name) in collect_component_instances(tree) {
        let mut sorted = (kind.clone(), name.clone());
        sorted.1 = name;
        let prefix = spice_prefix_for_class(&kind);
        out.push_str(&format!(
            "* {} {} (class={}; pin-level SPICE model is v2)\n",
            prefix, sorted.1, kind
        ));
        total += 1;
    }
    out.push_str(&format!(".ENDS {}\n\n.END\n", top));
    (out, Value::Null, total)
}

/// Walk inst.connections (recursively) and collect unique (component_name, kind)
/// pairs from NetPoint.owner + the connection's points.
fn collect_component_instances(inst: &McModuleInst) -> Vec<(String, String)> {
    let mut out: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    collect_components_in_inst(inst, &mut out);
    out.into_iter().collect()
}

fn collect_components_in_inst(
    inst: &McModuleInst,
    out: &mut std::collections::BTreeSet<(String, String)>,
) {
    for conn in &inst.connections {
        for np in &conn.points {
            // Derive instance name from np.path. Two patterns:
            //   - "<instance>.<pin_id>" → instance = "<instance>"
            //   - "<instance>" (label/port) → instance = "<instance>"
            // Owner is None in the engine's Pass2 result; the instance
            // name is encoded in the path.
            let instance = np
                .path
                .rsplit_once('.')
                .map(|(inst, _pin)| inst.to_string())
                .unwrap_or_else(|| np.path.clone());
            if !instance.is_empty() && !instance.starts_with("__") {
                out.insert(("instance".to_string(), instance));
            }
        }
    }
    for sub in &inst.sub_modules {
        collect_components_in_inst(sub, out);
    }
}

fn spice_prefix_for_class(class: &str) -> String {
    let up = class.to_uppercase();
    if up.starts_with("RES") || up == "R" {
        "R".into()
    } else if up.starts_with("CAP") || up == "C" {
        "C".into()
    } else if up.starts_with("IND") || up == "L" {
        "L".into()
    } else if up.starts_with("DIO") || up.starts_with("MOS") {
        "D".into()
    } else {
        "X".into()
    }
}

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

// ============================================================================
// Recursive walkers
// ============================================================================

fn collect_nets(inst: &McModuleInst, out: &mut BTreeMap<String, Vec<String>>) {
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

fn collect_components(
    _inst: &McModuleInst,
    _out: &mut Vec<(String, String, String, String, String)>,
) {
    // Deprecated: BOM now uses InstTable directly. Kept as a stub.
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

// ============================================================================
// Helpers
// ============================================================================

pub fn attr_value(attrs: &[McAttribute], name: &str) -> Option<String> {
    let id = McIds::from(name);
    for a in attrs {
        if a.id == id {
            for v in &a.values {
                if let mcc::McAttrVal::AttrLiteral(mcc::McLiteral::String(s)) = v {
                    return Some(s.value.clone());
                }
                if let mcc::McAttrVal::AttrLiteral(mcc::McLiteral::Int(i)) = v {
                    return Some(i.to_string());
                }
                if let mcc::McAttrVal::AttrLiteral(mcc::McLiteral::Uval(u)) = v {
                    return Some(u.value().to_string());
                }
            }
        }
    }
    None
}

pub fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        field.to_string()
    }
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch={}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escape_plain() {
        assert_eq!(csv_escape("RES"), "RES");
        assert_eq!(csv_escape(""), "");
    }

    #[test]
    fn csv_escape_comma() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn csv_escape_quote() {
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn csv_escape_newline() {
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }
}
