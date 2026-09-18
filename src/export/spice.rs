// Copyright (c) 2026 MCode
//! SPICE-style text netlist export -- a human-readable netlist, not a
//! simulation deck (deck responsibility belongs to the sim domain).

use crate::export::NodeArena;
use crate::instant::inststore::InstanceStore;
use crate::instant::insttab::InstTable;
use crate::instant::refdes;
use crate::McModuleInst;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::netlist::{collect_nets, PointNaming};

/// Letter for a component class the refdes table does not register. `X` is a
/// marker, not a guess: a class spelling is no evidence of what a device is
/// (`refdes-design.md` §2).
const UNKNOWN_PREFIX: &str = "X";

pub fn build_spice(
    tree: &McModuleInst,
    table: &InstTable,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
) -> (String, Value, usize) {
    let mut out = String::new();
    out.push_str(&format!("* SPICE netlist: top={}\n", top));
    out.push_str(&format!("* Generated: {}\n\n", super::chrono_like_now()));
    out.push_str(&format!(".SUBCKT {}\n", top));

    let mut name_to_class: HashMap<String, String> = HashMap::new();
    for comp in table.get_components() {
        // A component entry's path is already its own hierarchical path
        // (`main.ldo.C4`), which is the grain the hierarchical net labels use.
        if !comp.path.is_empty() && !comp.class_name.is_empty() {
            name_to_class.insert(comp.path.clone(), comp.class_name.clone());
        }
    }

    let mut netmap: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Phase D: the tree never stores NetPoint — read the frozen per-module
    // string net tables from the flat table's store.
    let store_ref = table.net_table();
    let store_ref = store_ref.borrow();
    // Hierarchical labels: the instance key must join to `name_to_class`, whose
    // keys are flat-table paths.
    collect_nets(
        tree,
        arena,
        inst_store,
        &store_ref,
        PointNaming::Hierarchical,
        &mut netmap,
    );

    // A `BTreeMap`, not a `HashMap`: this map's iteration order *is* the order
    // of the `X<name> <net> <net>` lines below, and a `HashMap` draws its order
    // fresh per process, so exporting one design twice produced two different
    // netlists (build-design §3.7 discipline 4 — a product's order must be
    // determined by the input alone). Every other exporter in this module
    // already keys on a `BTreeMap` (`netmap` above, and all of `bom` / `kicad`).
    let mut inst_nodes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (net_name, points) in &netmap {
        if net_name == "NC" || crate::instant::mc_net::is_anon_net_name(net_name) {
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
        let prefix = name_to_class
            .get(inst)
            .and_then(|class| refdes::prefix_for_class(class))
            .unwrap_or(UNKNOWN_PREFIX);
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
