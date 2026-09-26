// Copyright (c) 2026 MCode
//! BOM (Bill of Materials) export
//!
//! Rows come from the circuit's **instances**: every component or sub-module
//! instance a module mentions through its net points, plus every not-fitted
//! (NC / DNP) part even when nothing connects to it. A row entry is named by
//! the part's canonical instance path (`main.MCU513._C4`), the same identity
//! the `inst-list` projection carries (§3.7), so a name that two modules reuse
//! stays two parts and never lands in two buckets at once.
//!
//! Rows carry an `nc` marker, so a part designed in but not placed is reported
//! in its own bucket instead of looking like a fitted one. Nothing is dropped:
//! the reader can still see "this part exists, it is not fitted".

use crate::export::NodeArena;
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
    let comps = collect_part_instances(tree, arena, inst_store);

    // Keyed by (class, nc): the class grouping survives and a not-fitted part
    // sits in its own row right after the fitted ones of the same class.
    let agg: BTreeMap<(String, bool), Vec<String>> = {
        let mut m: BTreeMap<(String, bool), Vec<String>> = BTreeMap::new();
        for (kind, nc, path) in comps {
            m.entry((kind, nc)).or_default().push(path);
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

/// Every BOM row of the circuit: one `(class, nc, path)` entry per placed part
/// — a component or sub-module instance of some module of the tree — plus
/// every not-fitted component even when nothing connects to it.
///
/// The module path carried down the walk is the canonical instance path of the
/// module the part lives in, so a part is named by where it actually is:
/// `main.DCDC._C4` and `main.MCU513._C4` are two different capacitors, and one
/// of them being not fitted cannot put the other one's name in the NC bucket.
fn collect_part_instances(
    inst: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
) -> Vec<(String, bool, String)> {
    let view = TreeView::new(arena, inst_store);
    let mut out: BTreeSet<(String, bool, String)> = BTreeSet::new();
    let root = inst.name.clone();
    collect_parts_impl(inst, &view, &root, &mut out, false);
    out.into_iter().collect()
}

fn collect_parts_impl(
    m: &McModuleInst,
    view: &TreeView,
    path: &str,
    out: &mut BTreeSet<(String, bool, String)>,
    // ★ U305⑤: a `@dnp` ancestor scope — every part inside a not-fitted
    // assembly is not fitted with it.
    inherited_dnp: bool,
) {
    // The parts this module puts on the board: its own component and
    // sub-module children, each under the name its own scope gives it. A
    // module port, a bus member or a net label is not a part and never enters
    // the BOM, so membership is decided by the instance collections the arena
    // groups by node kind — never by the shape of a path string.
    let comps: Vec<&McComponentInst> = view.components(m).collect();
    let subs: Vec<&McModuleInst> = view.sub_modules(m).collect();
    let mut parts: BTreeMap<&str, String> = BTreeMap::new();
    for c in &comps {
        if !c.name.starts_with("__") {
            parts.insert(c.name.as_str(), format!("{path}.{}", c.name));
        }
    }
    for sub in &subs {
        if !sub.name.starts_with("__") {
            parts.insert(sub.name.as_str(), format!("{path}.{}", sub.name));
        }
    }

    // The status of a part resolves against its own module scope, through the
    // module's own expansion log (see [`nc_status`]).
    let by_name: BTreeMap<&str, &McComponentInst> =
        comps.iter().map(|c| (c.name.as_str(), *c)).collect();
    let recs: &[ExpansionRecord] = m.expansion.records.as_slice();

    // A part this module's net points name is a row: the point's `owner` is
    // the instance that owns the pin, so the row is the instance's own path.
    for conn in &m.connections {
        for np in &conn.points {
            let Some(owner) = np.owner.as_deref() else {
                continue;
            };
            let Some(full) = parts.get(owner) else {
                continue;
            };
            out.insert((
                class_of(owner),
                nc_status(owner, &by_name, recs) || inherited_dnp,
                full.clone(),
            ));
        }
    }

    // A not-fitted part is a row even when nothing connects to it: "designed
    // in, not placed" is exactly what a downstream reader has to see. A
    // fitted part with no connection stays out, as before.
    for c in &comps {
        let name = c.name.as_str();
        if !name.starts_with("__") && (nc_status(name, &by_name, recs) || inherited_dnp) {
            out.insert((class_of(name), true, format!("{path}.{}", name)));
        }
    }

    for sub in &subs {
        if !sub.name.starts_with("__") {
            let sub_path = format!("{path}.{}", sub.name);
            // ★ U305⑤: a `@dnp` sub-module is itself a row — "designed in,
            // not placed" is exactly what a downstream reader has to see —
            // and its whole subtree inherits the status.
            if sub.dnp {
                out.insert((class_of(sub.name.as_str()), true, sub_path.clone()));
            }
            collect_parts_impl(sub, view, &sub_path, out, inherited_dnp || sub.dnp);
        }
    }
}

/// The instance a part was materialized by: the caller of the enclosing
/// expansion record.
///
/// A product's own record carries no usable caller — a plain construction
/// records the instance itself (`X6.R442`), an auto-named one records nothing
/// (`_C4`) — so the nesting chain is walked up until a record names a caller
/// other than the part itself. That is why `X6.R442`, `_C4` and `_C5` all
/// resolve to `X6` through the `X6.setup(...)` record that enclose them.
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

/// Is this part not fitted? True when the instance says so itself, or when it
/// was materialized by an instance that is not fitted: every product of
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
        // ★ U305⑤: the constructor `NC` argument and the statement-line
        // `@dnp` flag say the same thing here — the part is not fitted.
        if c.nc || c.dnp {
            return true;
        }
        match owning_instance(c, recs) {
            Some(owner) => cur = owner,
            None => return false,
        }
    }
}

/// The class bucket of a part: the first character of the name its own scope
/// gave the instance, uppercased.
fn class_of(designator: &str) -> String {
    designator
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}
