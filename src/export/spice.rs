// Copyright (c) 2026 MCode
//! SPICE-style text netlist export -- a human-readable netlist, not a
//! simulation deck (deck responsibility belongs to the sim domain).

use crate::instant::insttab::InstTable;
use crate::instant::refdes;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::netlist::PointNaming;

/// Letter for a component class the refdes table does not register. `X` is a
/// marker, not a guess: a class spelling is no evidence of what a device is
/// (`refdes-design.md` §2).
const UNKNOWN_PREFIX: &str = "X";

pub fn build_spice(table: &InstTable, top: &str) -> (String, Value, usize) {
    let mut out = String::new();
    out.push_str(&format!("* SPICE netlist: top={}\n", top));
    out.push_str(&format!(".SUBCKT {}\n", top));

    let mut name_to_class: HashMap<String, String> = HashMap::new();
    for comp in table.get_components() {
        // A component entry's path is already its own hierarchical path
        // (`main.ldo.C4`), which is the grain the hierarchical net labels use.
        if !comp.path.is_empty() && !comp.class_name.is_empty() {
            name_to_class.insert(comp.path.clone(), comp.class_name.clone());
        }
    }

    // Copper islands from the flat table (U158): a merged island carries the
    // whole node, so a part's pin list is complete even where the copper is
    // anonymous — dropping those islands dropped the device's own pins.
    let netmap: BTreeMap<String, Vec<String>> =
        super::netlist::island_nets(table, PointNaming::Hierarchical);

    // A `BTreeMap`, not a `HashMap`: this map's iteration order *is* the order
    // of the `X<name> <net> <net>` lines below, and a `HashMap` draws its order
    // fresh per process, so exporting one design twice produced two different
    // netlists (build-design §3.7 discipline 4 — a product's order must be
    // determined by the input alone). Every other exporter in this module
    // already keys on a `BTreeMap` (`netmap` above, and all of `bom` / `kicad`).
    let mut inst_nodes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (net_name, points) in &netmap {
        if net_name == "NC"
            || net_name.starts_with(crate::semantic::basic::mc_bus::McBus::ERROR_PREFIX)
        {
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
        // An island carries its boundary port points too (U158), so a port
        // group (`main.LDO.vin`) and a module instance arrive here beside the
        // real parts. The flat table's component registry is the divider: no
        // component row, no device line.
        let Some(class) = name_to_class.get(inst) else {
            continue;
        };
        let node_list: Vec<&String> = nodes.iter().collect();
        let prefix = refdes::prefix_for_class(class).unwrap_or(UNKNOWN_PREFIX);
        // `inst` is the hierarchical path; the netlist reader is told the name
        // the module itself uses.
        let name = inst.rsplit('.').next().unwrap_or(inst);
        if node_list.len() >= 2 {
            out.push_str(&format!(
                "{}{} {} {}\n",
                prefix,
                strip_anon_line(name),
                node_list[0],
                node_list[1]
            ));
            total += 1;
        }
    }

    out.push_str(&format!(".ENDS {}\n\n.END\n", top));
    (out, Value::Null, total)
}

/// Strip a legacy `@line` provenance suffix from an anonymous instance name
/// (`_C1@62` -> `_C1`) for robustness; engine-generated names no longer carry
/// it, but an `@` that is not followed by digits (e.g. the internal
/// `@_phantom_1` isolation node) is left untouched.
fn strip_anon_line(name: &str) -> &str {
    if let Some(idx) = name.find('@') {
        let rest = &name[idx + 1..];
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return &name[..idx];
        }
    }
    name
}
