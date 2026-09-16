// Copyright (c) 2026 MCode
//! BOM (Bill of Materials) export
//!
//! Rows come from the module tree's connection points plus every not-fitted
//! (NC / DNP) part, and every row carries an `nc` marker, so a part designed in
//! but not placed is reported in its own bucket instead of looking like a
//! fitted one. Nothing is dropped: the reader can still see "this part exists,
//! it is not fitted".

use crate::export::{for_each_module_with_arena, NodeArena};
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::mc_comp::McComponentInst;
use crate::instant::provenance::ExpansionRecord;
use crate::McModuleInst;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn build_bom(
    tree: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
    format: u8,
) -> (String, Value, usize) {
    let comps = collect_component_instances(tree, arena, inst_store);

    // Keyed by (class, nc): the class grouping survives and a not-fitted part
    // sits in its own row right after the fitted ones of the same class.
    let agg: BTreeMap<(String, bool), Vec<String>> = {
        let mut m: BTreeMap<(String, bool), Vec<String>> = BTreeMap::new();
        for (kind, nc, name) in comps {
            m.entry((kind, nc)).or_default().push(name);
        }
        m
    };

    let count = agg.len();
    match format {
        1 => {
            let items: Vec<Value> = agg
                .iter()
                .map(|((c, nc), names)| {
                    json!({
                        "class": c,
                        "nc": nc,
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
            out.push_str("class,nc,value,description,package,count,refdes\n");
            for ((c, nc), names) in &agg {
                let refdes = names.join(",");
                out.push_str(&format!(
                    "{},{},,,,{},{}\n",
                    super::csv_escape(c),
                    if *nc { "true" } else { "false" },
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
            let w_class = agg.keys().map(|(c, _)| c.len()).max().unwrap_or(5).max(5);
            out.push_str(&format!(
                "{:<w_c$}  {:<2}  {:>5}  refdes\n",
                "class",
                "nc",
                "count",
                w_c = w_class,
            ));
            for ((c, nc), names) in &agg {
                out.push_str(&format!(
                    "{:<w_c$}  {:<2}  {:>5}  {}\n",
                    c,
                    if *nc { "NC" } else { "-" },
                    names.len(),
                    names.join(", "),
                    w_c = w_class,
                ));
            }
            (out, Value::Null, count)
        }
    }
}

/// Every BOM row of the circuit: one `(class, nc, designator)` entry per
/// component the module tree mentions, plus every not-fitted component even
/// when nothing connects to it.
fn collect_component_instances(
    inst: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
) -> Vec<(String, bool, String)> {
    let view = TreeView::new(arena, inst_store);
    let mut out: BTreeSet<(String, bool, String)> = BTreeSet::new();
    for_each_module_with_arena(inst, arena, inst_store, &mut |m| {
        // The designators this module's net points use are the names of the
        // module's own component children (`McComponentInst::name`), so the
        // status of a designator resolves against its own module scope.
        let comps: Vec<&McComponentInst> = view.components(m).collect();
        let by_name: BTreeMap<&str, &McComponentInst> =
            comps.iter().map(|c| (c.name.as_str(), *c)).collect();
        let recs: &[ExpansionRecord] = m.expansion.records.as_slice();

        for conn in &m.connections {
            for np in &conn.points {
                if let Some((name, _pin)) = np.path.rsplit_once('.') {
                    if !name.is_empty() && !name.starts_with("__") {
                        out.insert((class_of(name), nc_status(name, &by_name, recs), name.into()));
                    }
                }
            }
        }

        // A not-fitted part is a row even when nothing connects to it: "designed
        // in, not placed" is exactly what a downstream reader has to see. A
        // fitted part with no connection stays out, as before.
        for c in &comps {
            let name = c.name.as_str();
            if !name.starts_with("__") && nc_status(name, &by_name, recs) {
                out.insert((class_of(name), true, name.to_string()));
            }
        }
    });
    out.into_iter().collect()
}

/// The instance a designator was materialized by: the caller of the enclosing
/// expansion record.
///
/// A product's own record carries no usable caller — a plain construction
/// records the instance itself (`X6.R442`), an auto-named one records nothing
/// (`_C4`) — so the nesting chain is walked up until a record names a caller
/// other than the designator itself. That is why `X6.R442`, `_C4` and `_C5`
/// all resolve to `X6` through the `X6.setup(...)` record that enclose them.
fn owning_instance<'a>(c: &'a McComponentInst, recs: &'a [ExpansionRecord]) -> Option<&'a str> {
    let mut idx = c.expansion_id?;
    loop {
        let rec = recs.get(idx)?;
        if let Some(caller) = rec.caller_inst.as_deref() {
            if !caller.is_empty() && caller != c.name.as_str() {
                return Some(caller);
            }
        }
        idx = rec.parent?;
    }
}

/// Is this designator not fitted? True when the instance says so itself, or
/// when it was materialized by an instance that is not fitted: every product of
/// `X6.setup(...)` inherits the DNP status of `X6`, whether it is named after
/// it (`X6.R442`) or auto-named (`_C4`).
///
/// The ownership link is the module's own expansion log, so the rule reads
/// provenance rather than parsing the dotted name. A name that is not a
/// component of this module (a sub-module or a module port) is never NC, and an
/// owner that is not a component of this module does not propagate.
fn nc_status<'a>(
    name: &'a str,
    by_name: &BTreeMap<&'a str, &'a McComponentInst>,
    recs: &'a [ExpansionRecord],
) -> bool {
    let mut cur: &'a str = name;
    let mut seen: BTreeSet<&'a str> = BTreeSet::new();
    loop {
        if !seen.insert(cur) {
            return false;
        }
        let Some(&c) = by_name.get(cur) else {
            return false;
        };
        if c.nc {
            return true;
        }
        match owning_instance(c, recs) {
            Some(owner) => cur = owner,
            None => return false,
        }
    }
}

/// The class bucket of a designator: its first character, uppercased.
fn class_of(designator: &str) -> String {
    designator
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}
