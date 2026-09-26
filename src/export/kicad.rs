// Copyright (c) 2026 MCode
//! KiCad s-expression netlist export (M8)
//!
//! Components come from the module tree's own instance collections — every
//! placed part, connected or not — never from the net endpoints (an
//! endpoint-derived set drops unconnected parts and cannot see a part's
//! attributes). A part is named by its hierarchical instance path, the same
//! identity the BOM uses, so two modules reusing one designator stay two
//! parts; the emitted `ref` disambiguates the reused spelling with a `_2`,
//! `_3`, … suffix, and every net node reads through that same map.

use crate::export::NodeArena;
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::insttab::InstTable;
use crate::instant::mc_comp::McComponentInst;
use crate::McModuleInst;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::netlist::PointNaming;

/// One component instance of the tree, resolved against its own module scope.
struct CompInfo {
    /// The component's class (definition name).
    class: String,
    /// The value text: the first positional parameter as written, else the
    /// `partno` metadata attribute, else the class.
    value: String,
    /// The `package` metadata attribute — the closest mcc concept to a KiCad
    /// footprint; empty when undeclared (never guessed).
    footprint: String,
    /// Not-fitted marker (DNP downstream).
    dnp: bool,
    /// The canonical path of the module the part lives in (`main.ldo`), for
    /// the sheetpath row.
    module_path: String,
}

pub fn build_kicad_netlist(
    tree: &McModuleInst,
    table: &InstTable,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
) -> (String, Value, usize) {
    let comps = collect_components(tree, arena, inst_store);

    let mut out = String::new();
    out.push_str("(export (version D)\n");
    out.push_str(&format!("  (design\n    (source \"{}\"))\n", top));

    // Components, with globally unique refs in walk order.
    let mut used_refs: BTreeSet<String> = BTreeSet::new();
    let mut ref_of: HashMap<String, String> = HashMap::new();
    out.push_str("  (components\n");
    for (hier, info) in &comps {
        let reference = unique_ref(&last_segment(hier), &mut used_refs);
        ref_of.insert(hier.clone(), reference.clone());
        out.push_str(&format!(
            "    (comp (ref {})\n      (value {})\n      (footprint {})\n      (libsource (part {}) (description \"\"))\n      (sheetpath (names \"{}\") (tstamps \"\"))\n",
            reference,
            escape_sexpr(&info.value),
            escape_sexpr(&info.footprint),
            escape_sexpr(&info.class),
            escape_sexpr(&sheetpath_names(&info.module_path)),
        ));
        if info.dnp {
            out.push_str("      (property (name \"DNP\") (value \"yes\"))\n");
        }
        out.push_str("    )\n");
    }
    out.push_str("  )\n");

    // Nets: copper islands from the flat table (U158) — an island carries the
    // whole node, so no connection point vanishes for being on the far side of
    // a module boundary or on copper no statement ever named. Hierarchical
    // point naming is what lets each node join back to its component ref
    // through `ref_of`, whose keys are the same full instance paths.
    let netmap: BTreeMap<String, Vec<String>> =
        super::netlist::island_nets(table, PointNaming::Hierarchical);
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
            net_code,
            escape_sexpr(net_name)
        ));
        for pt in points {
            if let Some((inst, pin)) = split_point(pt) {
                if let Some(reference) = ref_of.get(inst) {
                    out.push_str(&format!("      (node (ref {}) (pin {}))\n", reference, pin));
                }
            }
        }
        out.push_str("    )\n");
        net_code += 1;
    }
    out.push_str("  )\n");

    out.push_str(")\n");
    (out, Value::Null, netmap.len())
}

/// Every component instance of the tree, keyed by hierarchical instance path
/// (`main.ldo.C4`) — the same grain the BOM rows use, so a name two modules
/// reuse stays two parts. Engine-generated names (`__`-prefixed) are not parts.
fn collect_components(
    tree: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
) -> BTreeMap<String, CompInfo> {
    let view = TreeView::new(arena, inst_store);
    let mut out: BTreeMap<String, CompInfo> = BTreeMap::new();
    let mut walk = |m: &McModuleInst, path: &str| {
        for c in view.components(m) {
            if c.name.starts_with("__") {
                continue;
            }
            let first_value = c
                .raw_params
                .iter()
                .map(|p| p.to_string())
                .find(|t| !t.is_empty() && t != "_" && t != "NC");
            let value = first_value
                .or_else(|| attr_text(c, "partno"))
                .unwrap_or_else(|| c.def.name.to_string());
            out.insert(
                format!("{path}.{}", c.name),
                CompInfo {
                    class: c.def.name.to_string(),
                    value,
                    footprint: attr_text(c, "package").unwrap_or_default(),
                    dnp: c.not_fitted(),
                    module_path: path.to_string(),
                },
            );
        }
    };
    walk_modules(tree, &view, &tree.name.clone(), &mut walk);
    out
}

fn walk_modules(
    inst: &McModuleInst,
    view: &TreeView,
    path: &str,
    f: &mut impl FnMut(&McModuleInst, &str),
) {
    f(inst, path);
    for sub in view.sub_modules(inst) {
        walk_modules(sub, view, &format!("{path}.{}", sub.name), f);
    }
}

/// One plain metadata attribute of an instance, rendered as text.
fn attr_text(c: &McComponentInst, key: &str) -> Option<String> {
    use crate::semantic::component::mc_attr::attr_values_text;
    for attr in &c.resolved_attrs {
        if attr.id.segments.len() == 1 && attr.id.segments[0].to_string() == key {
            if let Some(t) = attr_values_text(attr.values.iter()) {
                return Some(t);
            }
        }
    }
    None
}

/// A repeated designator gets `_2`, `_3`, … in walk order — deterministic,
/// and the map the net nodes read through, so refs and nodes cannot disagree.
fn unique_ref(base: &str, used: &mut BTreeSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// `"main.ldo.C1" -> ("main.ldo.C1", "1")`.
fn split_point(pt: &str) -> Option<(&str, &str)> {
    pt.rsplit_once('.')
}

fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// KiCad sheetpath spelling of a module path: `/ldo/` for `main.ldo`, `/` for
/// the top module itself.
fn sheetpath_names(module_path: &str) -> String {
    let mut parts: Vec<&str> = module_path.split('.').collect();
    if !parts.is_empty() {
        parts.remove(0);
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}/", parts.join("/"))
    }
}

fn escape_sexpr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_designators_get_numbered_suffixes() {
        let mut used = BTreeSet::new();
        assert_eq!(unique_ref("R1", &mut used), "R1");
        assert_eq!(unique_ref("C1", &mut used), "C1");
        assert_eq!(unique_ref("R1", &mut used), "R1_2");
        assert_eq!(unique_ref("R1", &mut used), "R1_3");
        assert_eq!(unique_ref("C1", &mut used), "C1_2");
    }

    #[test]
    fn point_splits_into_instance_and_pin() {
        assert_eq!(split_point("main.ldo.C1.1"), Some(("main.ldo.C1", "1")));
        assert_eq!(split_point("V5V"), None);
    }

    #[test]
    fn sheetpath_drops_the_top_module() {
        assert_eq!(sheetpath_names("main"), "/");
        assert_eq!(sheetpath_names("main.ldo"), "/ldo/");
        assert_eq!(sheetpath_names("main.ldo.core"), "/ldo/core/");
    }

    #[test]
    fn sexpr_strings_escape_quotes() {
        assert_eq!(escape_sexpr("10k"), "10k");
        assert_eq!(escape_sexpr("a\"b"), "a\\\"b");
    }
}
