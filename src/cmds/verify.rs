// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc verify` — check that the Pass2 expansion faithfully reflects the
//! Pass1 source, item by item:
//!
//!   1. Instances: every declared instance (component / module / interface /
//!      bus / label / declareb) must appear in the expanded module tree, and
//!      every expanded declared instance must trace back to a source
//!      declaration. Instances created by function calls (e.g. `.Cap()`) are
//!      reported as `generated`.
//!   2. Connections: every expanded connection carries a `source_span`
//!      (file, line) recorded during Pass2, so each source statement is shown
//!      together with the connections it expanded into, and every expanded
//!      connection is checked for a traceable source line.

use crate::cmds::{common, manifest};
use anyhow::Result;
use mcc::cli::{OutputFormat, VerifyArgs};
use mcc::hierarchy;
use mcc::vector::model::trunk::{TrunkCtx, TrunkKind};
use mcc::{arena_sub_modules, InstOrigin, McModuleInst, NodeArena, Span};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// Controls the returned exit code: 0 = clean, 1 = any mismatch found.
pub struct VerifyOutcome {
    pub exit_code: i32,
}

/// Running totals across all module sections (used for the summary and the
/// process exit code).
#[derive(Default)]
struct VerifyTotals {
    modules: usize,
    source_insts: usize,
    expanded_insts: usize,
    missing: usize,
    extra: usize,
    generated: usize,
    statements: usize,
    expanded_conns: usize,
    untraced: usize,
    no_expansion: usize,
}

/// One expanded connection, shown under the source statement that produced it.
#[derive(Clone)]
struct ConnEntry {
    net: String,
    points: Vec<String>,
    /// Source connector direction, rendered as the separator between points
    /// (`->` for LtoR, `<-` for RtoL, `-` for undirected).
    dir: String,
    /// §8.9.6 structured group context (name/member/kind), decided at the
    /// AST layer; None for plain connections.
    trunk: Option<mcc::vector::model::trunk::TrunkCtx>,
}

/// Join connection endpoints with the separator that reflects the source
/// connector direction: `->` (LtoR), `<-` (RtoL), `-` (undirected).
fn render_conn_points(points: &[&str], dir: &str) -> String {
    let sep = match dir {
        "LtoR" => " -> ",
        "RtoL" => " <- ",
        _ => " - ",
    };
    points.join(sep)
}

pub fn run(args: &VerifyArgs) -> Result<VerifyOutcome> {
    manifest::init_local(args.target.as_deref(), &mcc::cli::globals().lib);

    // Top-module resolution and Pass2 follow the same path as `show dianlu`.
    let (entry_uri, top) = common::load_target(
        args.target.as_deref(),
        mcc::cli::globals().top.as_deref(),
        mcc::cli::globals().entry.as_deref(),
    )?;
    let top = common::resolve_top_module(&entry_uri, top).unwrap_or_else(|| "main".to_string());
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top.clone()));
    let (inst, arena) = common::build_pass2_with_arena(&top, &uri).map_err(anyhow::Error::msg)?;

    let mut totals = VerifyTotals::default();
    let mut modules: Vec<Value> = Vec::new();
    verify_module(&inst, &top, Some(&arena), &mut totals, &mut modules);
    let hierarchy = hierarchy::build_hierarchy(&modules);

    let summary = json!({
        "modules": totals.modules,
        "instances": {
            "source": totals.source_insts,
            "expanded": totals.expanded_insts,
            "missing": totals.missing,
            "extra": totals.extra,
            "generated": totals.generated,
        },
        "connections": {
            "statements": totals.statements,
            "expanded": totals.expanded_conns,
            "untraced": totals.untraced,
            "no_expansion": totals.no_expansion,
        },
    });

    // Untraced connections are engine-generated projection trunks (interface /
    // bus member nets) with no source statement of their own; they are
    // reported for inspection but do not fail the verification. Only real
    // source-vs-expansion mismatches set the exit code.
    let problems = totals.missing + totals.extra + totals.no_expansion;
    let format = mcc::cli::globals().format;
    if matches!(format, OutputFormat::Text) {
        let mut text = String::new();
        render_text(&mut text, &top, &summary, &modules, &hierarchy);
        let buf = text.trim_end().to_string();
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, buf)?;
        } else {
            println!("{buf}");
        }
    } else {
        let data = json!({
            "type": "verify",
            "top": top,
            "summary": summary,
            "hierarchy": hierarchy,
            "modules": modules
        });
        crate::output::emit(
            &data,
            format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        )?;
    }

    Ok(VerifyOutcome {
        exit_code: if problems > 0 { 1 } else { 0 },
    })
}

/// Recurse through one module section: compare instances and connections, then
/// descend into sub-modules.
fn verify_module(
    inst: &McModuleInst,
    path: &str,
    arena: Option<&NodeArena>,
    totals: &mut VerifyTotals,
    out: &mut Vec<Value>,
) {
    let content = std::fs::read_to_string(&inst.def_uri.to_string()).ok();
    let (inst_report, inst_counts) = compare_instances(inst, &content);
    totals.source_insts += inst_counts.0;
    totals.expanded_insts += inst_counts.1;
    totals.missing += inst_counts.2;
    totals.extra += inst_counts.3;
    totals.generated += inst_counts.4;

    let (conn_report, conn_counts) = compare_connections(inst);
    totals.statements += conn_counts.0;
    totals.expanded_conns += conn_counts.1;
    totals.untraced += conn_counts.2;
    totals.no_expansion += conn_counts.3;

    out.push(json!({
        "module": path,
        "uri": inst.def_uri.to_string(),
        "instances": inst_report,
        "connections": conn_report,
    }));

    totals.modules += 1;
    let subs: Vec<&McModuleInst> = match arena {
        Some(a) => arena_sub_modules(a, inst).collect(),
        None => inst.sub_modules.iter().collect(),
    };
    for sub in subs {
        verify_module(sub, &format!("{path}.{}", sub.name), arena, totals, out);
    }
}

// ---------------------------------------------------------------------------
// Instance comparison
// ---------------------------------------------------------------------------

fn compare_instances(
    inst: &McModuleInst,
    content: &Option<String>,
) -> (Value, (usize, usize, usize, usize, usize)) {
    // Declared / declareb / funcall-generated families are extracted by the
    // shared hierarchy module; the comparison below adds the expanded side
    // (component / sub-module / label / bus instances Pass2 produced) and
    // computes missing / extra / generated.
    let fam = hierarchy::extract_instance_families(inst, content);
    let source = fam.source;
    let declareb = fam.declareb;
    let source_names = fam.source_names;
    let generated = fam.generated;

    // Expanded physical instance names. Function-generated components whose
    // name matches a source declaration are declareb instances (treated as
    // declared); the rest are reported as `generated`. The last tuple element
    // is the 1-based construction line (0 when unknown / declared).
    let mut expanded: Vec<(String, String, String, u32)> = Vec::new();
    let mut expanded_declared: HashSet<String> = HashSet::new();
    for comp in &inst.components {
        match comp.origin {
            InstOrigin::Declared => {
                expanded_declared.insert(comp.name.clone());
                expanded.push((
                    comp.name.clone(),
                    "component".to_string(),
                    "declared".to_string(),
                    0,
                ));
            }
            InstOrigin::FuncCall { line, .. } => {
                // Decision A (§7.1): `line` is a byte offset; convert to a
                // 1-based line for display (best effort against this module's
                // own file; func-body offsets into other files fall back to 0
                // — the accurate line comes from the expansion record).
                let ln = content.as_ref().map_or(0, |c| {
                    if (line as usize) < c.len() {
                        hierarchy::line_of_byte(c, line as usize)
                    } else {
                        0
                    }
                });
                if source_names.contains(&comp.name) {
                    expanded_declared.insert(comp.name.clone());
                    expanded.push((
                        comp.name.clone(),
                        "component".to_string(),
                        "declareb".to_string(),
                        ln,
                    ));
                } else {
                    expanded.push((
                        comp.name.clone(),
                        "component".to_string(),
                        "funcall".to_string(),
                        ln,
                    ));
                }
            }
        }
    }
    for sub in &inst.sub_modules {
        expanded_declared.insert(sub.name.clone());
        expanded.push((
            sub.name.clone(),
            "module".to_string(),
            "declared".to_string(),
            0,
        ));
    }
    for name in inst.get_labels().keys() {
        if !name.contains('.') {
            expanded.push((name.clone(), "label".to_string(), "derived".to_string(), 0));
        }
    }
    for bus in inst.get_buses().values() {
        if !bus.name.contains('.') {
            expanded.push((
                bus.name.clone(),
                "bus".to_string(),
                "derived".to_string(),
                0,
            ));
        }
    }
    // Labels that only exist as a connection net name (e.g. module port
    // labels like `DAC_OUT`) are also expanded instances.
    let mut net_labels: HashSet<String> = HashSet::new();
    for conn in &inst.connections {
        if let Some(n) = &conn.net_name {
            if !n.contains('.') {
                net_labels.insert(n.clone());
            }
        }
    }
    // Bare connection endpoints that are ports/labels (owner-less points whose
    // path has no instance prefix, e.g. the `VBUS` side of `DAP_USB_Vbus ->
    // VBUS`) are real netlist nodes too. A net may be named after one label
    // while other declared ports/labels join it as points; they must still
    // count as present in the expansion.
    let mut point_labels: HashSet<String> = HashSet::new();
    for conn in &inst.connections {
        for p in &conn.points {
            if p.owner.is_none() && !p.path.contains('.') {
                point_labels.insert(p.path.clone());
            }
        }
    }

    // Every source name must appear somewhere in the expansion. Components and
    // modules must be strictly declared-expanded; interfaces, buses and labels
    // may also match derived names (bus projections, connection net labels).
    let mut expanded_any: HashSet<String> = expanded_declared.clone();
    for name in inst.get_labels().keys() {
        if !name.contains('.') {
            expanded_any.insert(name.clone());
        }
    }
    for bus in inst.get_buses().values() {
        if !bus.name.contains('.') {
            expanded_any.insert(bus.name.clone());
        }
    }
    for n in &net_labels {
        expanded_any.insert(n.clone());
    }
    for n in &point_labels {
        expanded_any.insert(n.clone());
    }

    let mut missing: Vec<String> = source_names.difference(&expanded_any).cloned().collect();
    missing.sort();

    let mut extra: Vec<String> = expanded_declared
        .difference(&source_names)
        .cloned()
        .collect();
    extra.sort();

    let mut expanded_all: Vec<(String, String, String, u32)> = expanded.clone();
    for n in &net_labels {
        expanded_all.push((n.clone(), "label".to_string(), "derived".to_string(), 0));
    }
    for n in &point_labels {
        expanded_all.push((n.clone(), "label".to_string(), "derived".to_string(), 0));
    }
    expanded_all.sort_by(|a, b| (a.0.clone(), a.1.clone()).cmp(&(b.0.clone(), b.1.clone())));

    let report = json!({
        "source": source.iter().map(|(n, k, l, cl, o)| json!({
            "name": n, "kind": k, "line": l, "class": cl, "origin": o,
        })).collect::<Vec<_>>(),
        "declareb": declareb.iter().map(|(n, l, cl)| json!({"name": n, "line": l, "class": cl})).collect::<Vec<_>>(),
        "expanded": expanded_all.iter().map(|(n, k, o, l)| json!({"name": n, "kind": k, "origin": o, "line": l})).collect::<Vec<_>>(),
        "missing": missing,
        "extra": extra,
        "generated": generated.iter().map(|(n, l, cl, caller)| json!({"name": n, "line": l, "class": cl, "caller": caller})).collect::<Vec<_>>(),
    });
    let counts = (
        source.len() + declareb.len(),
        expanded_all.len(),
        missing.len(),
        extra.len(),
        generated.len(),
    );
    (report, counts)
}

// ---------------------------------------------------------------------------
// Connection comparison
// ---------------------------------------------------------------------------

/// Build one expansion-record tree node for the in-place funcall expansion:
/// the record's label, its direct products (connections + generated
/// components + sub-modules), the merged sub-module body connections (when
/// this record is a sub-target call), and its nested child records.
///
/// Returns `None` when the record produced nothing (no products, no merged
/// sub-module body, no children).
fn record_tree_node(
    idx: usize,
    groups: &mcc::ProductGroups,
    inst: &McModuleInst,
    sub_conns: Vec<Value>,
) -> Option<Value> {
    let r = &inst.expansion.records[idx];
    let conns: Vec<Value> = groups.by_record[idx]
        .connections
        .iter()
        .map(|&ci| {
            let c = &inst.connections[ci];
            json!({
                "net": c.effective_net_name(),
                "points": c.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
                "dir": format!("{:?}", c.dir),
                "trunk": c.trunk.as_ref().map(|pg| pg.to_json_value()),
            })
        })
        .collect();
    let comps: Vec<Value> = groups.by_record[idx]
        .components
        .iter()
        .map(|&ci| {
            let c = &inst.components[ci];
            json!({
                "name": c.name,
                "class": hierarchy::comp_class_raw(&c.def.name.to_string(), &c.raw_params),
            })
        })
        .collect();
    let subs: Vec<Value> = groups.by_record[idx]
        .sub_modules
        .iter()
        .map(|&si| json!({"name": inst.sub_modules[si].name}))
        .collect();
    let children: Vec<Value> = r
        .children
        .iter()
        .filter_map(|&ci| record_tree_node(ci, groups, inst, Vec::new()))
        .collect();
    if conns.is_empty()
        && comps.is_empty()
        && subs.is_empty()
        && sub_conns.is_empty()
        && children.is_empty()
    {
        return None;
    }
    Some(json!({
        "label": r.func_name,
        "kind": r.kind.name(),
        "connections": conns,
        "components": comps,
        "sub_modules": subs,
        "sub_connections": sub_conns,
        "children": children,
    }))
}

fn compare_connections(inst: &McModuleInst) -> (Value, (usize, usize, usize, usize)) {
    let stmts = &inst.def.stmts;
    let stmt_spans = &inst.def.stmt_spans;

    // Build a byte-offset -> line-number map from the module's own source
    // file so expanded connections (tagged by `source_span`, decision A §7.1
    // byte offset) can be attributed to the exact source statement that
    // produced them.
    let def_file = inst.def_uri.to_string();
    let content = std::fs::read_to_string(&def_file).ok();
    let mut src_by_line: BTreeMap<u32, usize> = BTreeMap::new();
    if let Some(c) = &content {
        for (i, sp) in stmt_spans.iter().enumerate() {
            src_by_line.insert(hierarchy::line_of_byte(c, sp.start as usize), i);
        }
    }

    // Body-expansion records issued by each module statement, keyed by the
    // statement byte offset (call_site, §4.1). Used to upgrade the `(funcall)`
    // exemption into a real "did the call expand anything" check (§5.1).
    // §7.10: built via the shared `build_tree` read API instead of a
    // hand-rolled statement aggregation.
    let groups =
        inst.expansion
            .group_products(&inst.components, &inst.sub_modules, &inst.connections);
    let mut stmt_records: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for node in inst.expansion.build_tree() {
        if node.call_site.uri == def_file {
            stmt_records
                .entry(node.call_site.offset)
                .or_default()
                .extend(node.expansions);
        }
    }

    // §7.3 cross-module merged display: when a top-level record of this module
    // is a sub-module method / inline-module call (`sub_target` set), the
    // callee body expands inside the sub-module instance. Merge that
    // sub-module body expansion (its record carries the same call_site as the
    // parent record) into this statement so the call site shows what the call
    // expanded — boundary connections still belong to the parent record
    // (expansion_id), body products are listed here as auxiliary context.
    let mut sub_expansions: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
    // Same merged body connections keyed by the parent record index, so the
    // nested expansion tree can attach them under the sub-target record.
    let mut sub_by_record: HashMap<usize, Vec<Value>> = HashMap::new();
    let mut seen_sub_conns: HashSet<String> = HashSet::new();
    for (ri, r) in inst.expansion.records.iter().enumerate() {
        if r.parent.is_some() {
            continue;
        }
        let Some(sub_path) = &r.sub_target else {
            continue;
        };
        let Some(pos) = &r.call_site else { continue };
        if pos.uri != def_file {
            continue;
        }
        let Some(sub) = inst
            .sub_modules
            .iter()
            .find(|s| s.name.as_str() == sub_path.as_str())
        else {
            continue;
        };
        let sub_groups =
            sub.expansion
                .group_products(&sub.components, &sub.sub_modules, &sub.connections);
        for (si, sr) in sub.expansion.records.iter().enumerate() {
            if sr.parent.is_some() {
                continue;
            }
            let same_site = sr
                .call_site
                .as_ref()
                .map(|sp| sp.uri == pos.uri && sp.offset == pos.offset)
                .unwrap_or(false);
            if !same_site {
                continue;
            }
            let g = &sub_groups.by_record[si];
            for &ci in &g.connections {
                let c = &sub.connections[ci];
                // Same call site may map to several records (e.g. chained
                // `mcu513.i2c().loadFlash(...)` expands both in the sub-module);
                // dedupe identical body connections within the statement.
                let key = format!(
                    "{}|{}",
                    c.effective_net_name(),
                    c.points
                        .iter()
                        .map(|p| p.path.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                if !seen_sub_conns.contains(&key) {
                    seen_sub_conns.insert(key);
                    let entry = json!({
                        "sub": sub_path,
                        "net": c.effective_net_name(),
                        "points": c.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
                        "dir": format!("{:?}", c.dir),
                        "trunk": c.trunk.as_ref().map(|pg| pg.to_json_value()),
                    });
                    sub_expansions
                        .entry(pos.offset)
                        .or_default()
                        .push(entry.clone());
                    sub_by_record.entry(ri).or_default().push(entry);
                }
            }
        }
    }

    let mut conns_by_stmt: Vec<Vec<ConnEntry>> = (0..stmts.len()).map(|_| Vec::new()).collect();
    let mut untraced: Vec<String> = Vec::new();
    let mut cross_file: Vec<Value> = Vec::new();
    // Cross-file body connections attributed to a declaration line (e.g. a
    // component constructor body defined in another file): grouped by the
    // declaration line, rendered under that statement (§5.1 declare view).
    let mut declare_conns: BTreeMap<u32, Value> = BTreeMap::new();
    let mut unattributed: Vec<Value> = Vec::new();

    for conn in &inst.connections {
        let entry = ConnEntry {
            net: conn.effective_net_name(),
            points: conn.points.iter().map(|p| p.path.clone()).collect(),
            dir: format!("{:?}", conn.dir),
            trunk: conn.trunk.clone(),
        };
        match &conn.source_span {
            // No source span: engine-internal projection trunk (interface / bus
            // member net), legal (§5.4).
            None => untraced.push(entry.net),
            Some(pos) => {
                if pos.uri != def_file {
                    // Connection created while instantiating a function whose
                    // body lives in another file (e.g. `uC.i2c()`). Attribute
                    // it to the statement that triggered the call: walk the
                    // expansion record parent chain to the top-level record
                    // and use its call site (the declaring statement in this
                    // module). When that statement is a declaration line (not
                    // a connection statement, so absent from `src_by_line`),
                    // the connection is collected per declaration line for
                    // the `declare_conns` block; otherwise it falls back to
                    // the cross-file section (§5.2).
                    let mut cur = conn.expansion_id;
                    let mut top_call: Option<&mcc::SourcePos> = None;
                    while let Some(c) = cur {
                        if let Some(r) = inst.expansion.records.get(c) {
                            if r.parent.is_none() {
                                top_call = r.call_site.as_ref();
                                break;
                            }
                            cur = r.parent;
                        } else {
                            break;
                        }
                    }
                    let mut attributed = false;
                    if let Some(tc) = top_call {
                        if tc.uri == def_file {
                            if let Some(c) = &content {
                                let ln = hierarchy::line_of_byte(c, tc.offset as usize);
                                if let Some(&idx) = src_by_line.get(&ln) {
                                    conns_by_stmt[idx].push(entry.clone());
                                    attributed = true;
                                } else {
                                    declare_conns.entry(ln).or_insert_with(|| {
                                        let text = c
                                            .lines()
                                            .nth(ln.saturating_sub(1) as usize)
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                        json!({"line": ln, "text": text, "conns": []})
                                    })["conns"]
                                        .as_array_mut()
                                        .unwrap()
                                        .push(json!({
                                            "net": entry.net.clone(),
                                            "points": entry.points.clone(),
                                            "dir": entry.dir.clone(),
                                            "trunk": entry
                                                .trunk
                                                .as_ref()
                                                .map(|pg| pg.to_json_value()),
                                        }));
                                    attributed = true;
                                }
                            }
                        }
                    }
                    if attributed {
                        continue;
                    }
                    let line = std::fs::read_to_string(&pos.uri)
                        .ok()
                        .map(|c| hierarchy::line_of_byte(&c, pos.offset as usize))
                        .unwrap_or(0);
                    cross_file.push(json!({
                        "net": entry.net,
                        "points": entry.points,
                        "dir": entry.dir,
                        "trunk": entry.trunk.as_ref().map(|pg| pg.to_json_value()),
                        "source": format!("{}:{}", pos.uri, line),
                    }));
                } else if content.is_some() {
                    // `source_span` carries a byte offset (decision A);
                    // convert to a line number to attribute the statement.
                    let ln =
                        hierarchy::line_of_byte(content.as_ref().unwrap(), pos.offset as usize);
                    match src_by_line.get(&ln) {
                        Some(&idx) => conns_by_stmt[idx].push(entry),
                        None if conn.expansion_id.is_some() => {
                            // Attributed to an expansion record (its
                            // call_site / def_site locates it); not a module
                            // statement product, so not unattributed.
                        }
                        None => unattributed.push(json!({
                            "net": entry.net,
                            "points": entry.points,
                            "dir": entry.dir,
                            "trunk": entry.trunk.as_ref().map(|pg| pg.to_json_value()),
                            "line": ln,
                        })),
                    }
                } else {
                    // Source file unreadable: cannot attribute.
                    untraced.push(entry.net);
                }
            }
        }
    }

    // Per-statement report. A real connection statement that expanded to
    // nothing is flagged; statements containing function calls are checked
    // against their body-expansion records instead of being blanket-exempt
    // (§5.1): a body-expanding record with no products is a genuine empty
    // expansion (P2-8 `skipped` records are deliberate).
    let mut per_stmt: Vec<Value> = Vec::with_capacity(stmts.len());
    let mut no_expansion = 0usize;
    for (i, phrase) in stmts.iter().enumerate() {
        let has_funccall = phrase_contains_funccall(phrase);
        let conns: Vec<Value> = conns_by_stmt[i]
            .iter()
            .map(|c| {
                json!({
                    "net": c.net,
                    "points": c.points,
                    "dir": c.dir,
                    "trunk": c.trunk.as_ref().map(|pg| pg.to_json_value()),
                })
            })
            .collect();
        let stmt_off = stmt_spans.get(i).map(|sp| sp.start as u32);
        let empty_expansion = stmt_off
            .and_then(|off| stmt_records.get(&off))
            .map(|recs| {
                let body_kinds = [
                    mcc::ExpansionKind::InstanceMethod,
                    mcc::ExpansionKind::UserFunc,
                    mcc::ExpansionKind::ModuleCall,
                ];
                let body: Vec<usize> = recs
                    .iter()
                    .copied()
                    .filter(|&k| body_kinds.contains(&inst.expansion.records[k].kind))
                    .collect();
                if body.is_empty() {
                    // Only leaf records (declare / ctor / iterated): products
                    // exist by construction.
                    return false;
                }
                // P2-8 (`skipped`) records are deliberate — the module method
                // was already auto-invoked during the sub-module's own
                // instantiation, so the explicit call correctly expands
                // nothing here (design §5.1). They must not count toward a
                // "surprise empty" flag; a statement whose only body records
                // are skipped is intentional, not a mismatch.
                let active: Vec<usize> = body
                    .iter()
                    .copied()
                    .filter(|&k| !inst.expansion.records[k].skipped)
                    .collect();
                if active.is_empty() {
                    return false;
                }
                active.iter().all(|&k| {
                    let g = &groups.by_record[k];
                    g.components.is_empty() && g.sub_modules.is_empty() && g.connections.is_empty()
                })
            })
            .unwrap_or(false);
        let funcall_empty = has_funccall && empty_expansion;
        if conns.is_empty() && (!has_funccall || funcall_empty) {
            no_expansion += 1;
        }
        // §7.3: sub-module body expansion merged into this statement (the
        // `sub_expansions` table is keyed by the parent call site offset).
        let sub_conns: Vec<Value> = stmt_off
            .and_then(|off| sub_expansions.get(&off))
            .cloned()
            .unwrap_or_default();
        // Nested in-place expansion tree for funcall statements: products are
        // grouped by expansion record (not by source line), so func-body
        // connections whose span falls on non-statement lines (e.g. a
        // component method body in the definition file) show up at the call
        // site instead of being dropped. The merged sub-module body
        // connections attach under the first sub-target record.
        let tree: Vec<Value> = if has_funccall {
            stmt_off
                .and_then(|off| stmt_records.get(&off))
                .map(|recs| {
                    let mut used_sub = false;
                    recs.iter()
                        .filter_map(|&idx| {
                            let attach_sub =
                                !used_sub && inst.expansion.records[idx].sub_target.is_some();
                            if attach_sub {
                                used_sub = true;
                            }
                            let sub = if attach_sub {
                                sub_by_record.get(&idx).cloned().unwrap_or_default()
                            } else {
                                Vec::new()
                            };
                            record_tree_node(idx, &groups, inst, sub)
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        per_stmt.push(json!({
            "line": line_of_span(&content, stmt_spans.get(i)),
            "text": format!("{phrase}"),
            "funcall": has_funccall,
            "funcall_empty": funcall_empty,
            "connections": conns,
            "sub_expansions": sub_conns,
            "tree": tree,
        }));
    }

    let report = json!({
        "statements": stmts.len(),
        "expanded": inst.connections.len(),
        "per_stmt": per_stmt,
        "untraced": untraced,
        "cross_file": cross_file,
        "declare_conns": declare_conns.into_values().collect::<Vec<_>>(),
        "unattributed": unattributed,
    });
    let counts = (
        stmts.len(),
        inst.connections.len(),
        untraced.len(),
        no_expansion,
    );
    (report, counts)
}

/// `L<n>` column text for an instance report entry; blank when the source
/// line is unknown (0) so the instance list stays readable.
fn line_col(e: &Value) -> String {
    match e["line"].as_u64() {
        Some(0) | None => String::new(),
        Some(l) => format!("L{l}"),
    }
}

/// Whether a phrase tree contains a function call anywhere. Used to exempt
/// statements whose expansion happens inside a callee body.
fn phrase_contains_funccall(p: &mcc::McPhrase) -> bool {
    match p {
        mcc::McPhrase::FuncCall(_) => true,
        mcc::McPhrase::Series(ps, _)
        | mcc::McPhrase::Parallel(ps)
        | mcc::McPhrase::Multiple(ps) => ps.iter().any(phrase_contains_funccall),
        mcc::McPhrase::Group(g) => g.opds.iter().any(phrase_contains_funccall),
        mcc::McPhrase::Closure(c) => c.body.iter().any(phrase_contains_funccall),
        mcc::McPhrase::Transposed(inner) | mcc::McPhrase::Member(inner, _) => {
            phrase_contains_funccall(inner)
        }
        mcc::McPhrase::Lead | mcc::McPhrase::Endpoint(_) => false,
    }
}

/// Line number for a source span, None when the file is unreadable.
fn line_of_span(content: &Option<String>, sp: Option<&Span>) -> Option<u32> {
    match (content, sp) {
        (Some(c), Some(sp)) => Some(hierarchy::line_of_byte(c, sp.start as usize)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Text rendering
// ---------------------------------------------------------------------------

fn render_text(out: &mut String, top: &str, summary: &Value, modules: &[Value], hierarchy: &Value) {
    let inst = &summary["instances"];
    let conn = &summary["connections"];
    let problems = inst["missing"].as_u64().unwrap_or(0)
        + inst["extra"].as_u64().unwrap_or(0)
        + conn["no_expansion"].as_u64().unwrap_or(0);
    let _ = writeln!(
        out,
        "Verify: {top} | modules: {} | instances: source={} expanded={} missing={} extra={} generated={} | connections: statements={} expanded={} no_expansion={}",
        summary["modules"],
        inst["source"],
        inst["expanded"],
        inst["missing"],
        inst["extra"],
        inst["generated"],
        conn["statements"],
        conn["expanded"],
        conn["no_expansion"],
    );
    let _ = writeln!(
        out,
        "Result: {}",
        if problems > 0 { "MISMATCH" } else { "OK" }
    );
    let _ = writeln!(out);

    // Global module-nesting overview first, so the whole instance structure
    // is visible before the per-module detail sections.
    let _ = writeln!(out, "===== Hierarchy: {top} =====");
    hierarchy::render_hierarchy_text(out, hierarchy);
    let _ = writeln!(out);

    // Per-module instance report right after the hierarchy tree: for every
    // level, the expected count (declared source + declareb) against the
    // actual expanded count, so a layer missing instances stands out before
    // the per-module detail sections.
    render_instance_report(out, modules);

    // Blank line between the report and the first per-module section.
    let _ = writeln!(out);

    for m in modules {
        render_module_text(out, m);
    }
}

/// Per-module counts, all in the same "visible in the hierarchy tree" terms:
/// `expected` = declared source + declareb, `expanded` = expected plus
/// funcall-generated anonymous components (every row the tree shows for that
/// module). Derived labels / buses and connection net names are connection
/// projections, not instances, so they are not counted. `missing` / `extra`
/// are the actual mismatch lists (`extra` only counts components/modules that
/// appear without a declaration). Per-kind columns (`comp`/`mod`/`ifs`/`bus`/
/// `lbl`) show `expected/expanded` as a pair, so the source of any gap is
/// visible on the module row itself.
fn render_instance_report(out: &mut String, modules: &[Value]) {
    let _ = writeln!(out);
    let _ = writeln!(out, "===== Instance Report =====");
    let _ = writeln!(
        out,
        "  {:<28} {:>9} {:>9} {:>8} {:>6}  {:>6} {:>6} {:>6} {:>6} {:>6}",
        "module", "expected", "expanded", "missing", "extra", "comp", "mod", "ifs", "bus", "lbl"
    );
    let mut total_expected = 0usize;
    let mut total_expanded = 0usize;
    let mut total_missing = 0usize;
    let mut total_extra = 0usize;
    let mut total_kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total_kinds_exp: BTreeMap<&str, usize> = BTreeMap::new();
    for m in modules {
        let path = m["module"].as_str().unwrap_or("");
        let depth = path.matches('.').count();
        let src = m["instances"]["source"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let decl = m["instances"]["declareb"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let gen = m["instances"]["generated"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let missing = m["instances"]["missing"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let extra = m["instances"]["extra"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let label = format!("{}{path}", "  ".repeat(depth));
        // Per-kind counts in tree terms: declared (source + declareb,
        // declareb is always a component) against expanded (declared plus
        // funcall-generated anonymous components).
        let mut expected_by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        if let Some(arr) = m["instances"]["source"].as_array() {
            for e in arr {
                let k = e["kind"].as_str().unwrap_or("");
                *expected_by_kind.entry(k).or_default() += 1;
            }
        }
        *expected_by_kind.entry("component").or_default() += decl;
        let mut expanded_by_kind = expected_by_kind.clone();
        *expanded_by_kind.entry("component").or_default() += gen;
        let pair = |k: &str| -> String {
            let exp = expected_by_kind.get(k).copied().unwrap_or(0);
            let act = expanded_by_kind.get(k).copied().unwrap_or(0);
            if exp == 0 && act == 0 {
                "-".to_string()
            } else {
                format!("{exp}/{act}")
            }
        };
        for (k, v) in &expected_by_kind {
            *total_kinds.entry(k).or_default() += *v;
        }
        for (k, v) in &expanded_by_kind {
            *total_kinds_exp.entry(k).or_default() += *v;
        }
        let _ = writeln!(
            out,
            "  {label:<28} {:>9} {:>9} {:>8} {:>6}  {:>6} {:>6} {:>6} {:>6} {:>6}",
            src + decl,
            src + decl + gen,
            missing,
            extra,
            pair("component"),
            pair("module"),
            pair("interface"),
            pair("bus"),
            pair("label"),
        );
        total_expected += src + decl;
        total_expanded += src + decl + gen;
        total_missing += missing;
        total_extra += extra;
    }
    let total_pair = |k: &str| -> String {
        let exp = total_kinds.get(k).copied().unwrap_or(0);
        let act = total_kinds_exp.get(k).copied().unwrap_or(0);
        if exp == 0 && act == 0 {
            "-".to_string()
        } else {
            format!("{exp}/{act}")
        }
    };
    let _ = writeln!(
        out,
        "  {:<28} {:>9} {:>9} {:>8} {:>6}  {:>6} {:>6} {:>6} {:>6} {:>6}",
        "TOTAL",
        total_expected,
        total_expanded,
        total_missing,
        total_extra,
        total_pair("component"),
        total_pair("module"),
        total_pair("interface"),
        total_pair("bus"),
        total_pair("label"),
    );
}

/// One branch of the in-place funcall expansion tree: a plain leaf line or a
/// nested record node whose own products / children render recursively.
enum Branch<'a> {
    Leaf(String),
    Node { label: String, node: &'a Value },
}

/// Net + points joined key used to dedupe a connection across the record
/// tree and the flat call-site product list.
fn conn_key(c: &Value) -> String {
    let pts: Vec<&str> = c["points"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p.as_str()).collect())
        .unwrap_or_default();
    format!("{}|{}", c["net"].as_str().unwrap_or(""), pts.join(","))
}

/// Convert a connection JSON value to the shared §8.9.5 view. The JSON
/// `trunk` object (`{"name", "member", "kind", "iface_class"}`) is decoded
/// back into a structured [`TrunkCtx`]; missing / malformed → None.
fn conn_view(c: &Value) -> common::ConnView {
    let trunk = c["trunk"].as_object().map(|o| TrunkCtx {
        name: o.get("name").and_then(|v| v.as_str()).map(str::to_string),
        member: o.get("member").and_then(|v| v.as_str()).map(str::to_string),
        kind: o
            .get("kind")
            .and_then(|v| v.as_str())
            .map(kind_from_label)
            .unwrap_or(TrunkKind::Plain),
        iface_class: o
            .get("iface_class")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    });
    common::ConnView {
        net: c["net"].as_str().unwrap_or("").to_string(),
        points: c["points"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        dir: c["dir"].as_str().unwrap_or("").to_string(),
        trunk,
    }
}

/// Reverse of [`TrunkKind::label`]; unknown labels map to `Plain`.
fn kind_from_label(s: &str) -> TrunkKind {
    match s {
        "bus" => TrunkKind::Bus,
        "ifs" => TrunkKind::Interface,
        "list" => TrunkKind::List,
        _ => TrunkKind::Plain,
    }
}

/// Collect every connection key inside a record node (its own products plus
/// all nested children), so flat call-site products are not duplicated.
fn collect_conn_keys(node: &Value, out: &mut HashSet<String>) {
    if let Some(conns) = node["connections"].as_array() {
        for c in conns {
            out.insert(conn_key(c));
        }
    }
    if let Some(subs) = node["sub_connections"].as_array() {
        for c in subs {
            out.insert(conn_key(c));
        }
    }
    if let Some(children) = node["children"].as_array() {
        for c in children {
            collect_conn_keys(c, out);
        }
    }
}

/// Render a list of tree branches with `|--` / `|` / `` `--`` connectors,
/// matching the hierarchy tree style. `prefix` is the indentation before the
/// branch stems (the statement line's continuation column).
fn render_branches(out: &mut String, prefix: &str, branches: &[Branch]) {
    let n = branches.len();
    for (i, b) in branches.iter().enumerate() {
        let last = i + 1 == n;
        let (stem, cont) = if last {
            ("`-- ", "    ")
        } else {
            ("|-- ", "|   ")
        };
        match b {
            Branch::Leaf(text) => {
                let _ = writeln!(out, "{prefix}{stem}{text}");
            }
            Branch::Node { label, node } => {
                // Single-product leaf record (e.g. `RES` creating only `_R1`):
                // collapse the record label and its component into one line
                // instead of a two-level stub. The record label is dropped:
                // for a construction leaf it always equals the component
                // class, so `|-- RES _R1 RES(10kΩ)` reads as the product
                // `|-- _R1 RES(10kΩ)`.
                let single_comp = node["components"]
                    .as_array()
                    .map(|a| a.len() == 1)
                    .unwrap_or(false)
                    && !node["connections"]
                        .as_array()
                        .map_or(false, |a| !a.is_empty())
                    && !node["sub_connections"]
                        .as_array()
                        .map_or(false, |a| !a.is_empty())
                    && !node["children"].as_array().map_or(false, |a| !a.is_empty());
                if single_comp {
                    let c = &node["components"][0];
                    let class = c["class"].as_str().unwrap_or("");
                    let _ = writeln!(
                        out,
                        "{prefix}{stem}{}{}",
                        c["name"].as_str().unwrap_or(""),
                        if class.is_empty() {
                            String::new()
                        } else {
                            format!(" {class}")
                        },
                    );
                    continue;
                }
                let _ = writeln!(out, "{prefix}{stem}{label}");
                let mut children: Vec<Branch> = Vec::new();
                // Nested expansion records first (what the call created), then
                // the record's own components, then its wiring — matches the
                // top level where tree nodes precede leftover flat products.
                for c in node["children"].as_array().into_iter().flatten() {
                    let label = c["label"].as_str().unwrap_or("");
                    children.push(Branch::Node {
                        label: if label.is_empty() {
                            c["kind"].as_str().unwrap_or("").to_string()
                        } else {
                            label.to_string()
                        },
                        node: c,
                    });
                }
                if let Some(comps) = node["components"].as_array() {
                    for c in comps {
                        children.push(Branch::Leaf(format!(
                            "{} {}",
                            c["name"].as_str().unwrap_or(""),
                            c["class"].as_str().unwrap_or("")
                        )));
                    }
                }
                if let Some(conns) = node["connections"].as_array() {
                    // §8.9.5 layered rendering (trunks for bus/interface
                    // groups, flat lines otherwise).
                    let views: Vec<common::ConnView> = conns.iter().map(conn_view).collect();
                    for t in common::render_layered_conns(&views, "") {
                        children.push(Branch::Leaf(t));
                    }
                }
                if let Some(subs) = node["sub_connections"].as_array() {
                    for c in subs {
                        let sub = c["sub"].as_str().unwrap_or("");
                        let mut lines =
                            common::render_layered_conns(std::slice::from_ref(&conn_view(c)), "");
                        // mark as merged sub-module body (auxiliary context)
                        if let Some(first) = lines.first_mut() {
                            *first = format!("{first} (sub {sub})");
                        }
                        for t in lines {
                            children.push(Branch::Leaf(t));
                        }
                    }
                }
                let child_prefix = format!("{prefix}{cont}");
                render_branches(out, &child_prefix, &children);
            }
        }
    }
}

fn render_module_text(out: &mut String, m: &Value) {
    let _ = writeln!(out, "===== Section: {} =====", m["module"]);
    let inst = &m["instances"];
    let conn = &m["connections"];

    let _ = writeln!(out, "  Instances:");
    // Pad every instance name to the same width so the type / annotation
    // column (kind, "(declareb)", "(funcall)") lines up vertically.
    let name_w = ["source", "declareb", "generated"]
        .iter()
        .filter_map(|k| inst.get(k).and_then(|v| v.as_array()))
        .flatten()
        .filter_map(|e| e["name"].as_str())
        .map(|n| n.chars().count())
        .max()
        .map(|w| w + 1)
        .unwrap_or(13);
    // The class column (same rendering as the Hierarchy tree) is appended
    // after the kind marker; marker and class widths keep all rows aligned.
    // Markers carry the origin suffix (`comp.s` / `comp.b` / `comp.f`).
    let mark_w = {
        let kind = inst["source"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        e["kind"].as_str().map(|k| {
                            hierarchy::kind_text(k, e["origin"].as_str().unwrap_or("src")).len()
                        })
                    })
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        kind.max(hierarchy::kind_text("declareb", "decl").len())
            .max(hierarchy::kind_text("component", "gen").len())
            + 1
    };
    let class_w = ["source", "declareb", "generated"]
        .iter()
        .filter_map(|k| inst.get(k).and_then(|v| v.as_array()))
        .flatten()
        .filter_map(|e| e["class"].as_str())
        .map(|c| c.chars().count())
        .max()
        .map(|w| w + 1)
        .unwrap_or(0);
    if let Some(arr) = inst["source"].as_array() {
        for e in arr {
            let ln = line_col(e);
            let name = e["name"].as_str().unwrap_or("");
            let kind = hierarchy::kind_text(
                e["kind"].as_str().unwrap_or(""),
                e["origin"].as_str().unwrap_or("src"),
            );
            let class = e["class"].as_str().unwrap_or("");
            let _ = writeln!(
                out,
                "    {ln:<5}[src]  {name:<name_w$}{kind:<mark_w$}{class:<class_w$}"
            );
        }
    }
    if let Some(arr) = inst["declareb"].as_array() {
        for e in arr {
            let ln = line_col(e);
            let name = e["name"].as_str().unwrap_or("");
            let class = e["class"].as_str().unwrap_or("");
            let mark = hierarchy::kind_text("declareb", "decl");
            let _ = writeln!(
                out,
                "    {ln:<5}[decl] {name:<name_w$}{mark:<mark_w$}{class:<class_w$}"
            );
        }
    }
    if let Some(arr) = inst["missing"].as_array() {
        for name in arr {
            let _ = writeln!(out, "    [missing] {}", name.as_str().unwrap_or(""));
        }
    }
    if let Some(arr) = inst["extra"].as_array() {
        for name in arr {
            let _ = writeln!(out, "    [extra] {}", name.as_str().unwrap_or(""));
        }
    }
    if let Some(arr) = inst["generated"].as_array() {
        for e in arr {
            let ln = line_col(e);
            let name = e["name"].as_str().unwrap_or("");
            let class = e["class"].as_str().unwrap_or("");
            let mark = hierarchy::kind_text("component", "gen");
            let _ = writeln!(
                out,
                "    {ln:<5}[gen]  {name:<name_w$}{mark:<mark_w$}{class:<class_w$}"
            );
        }
    }

    let _ = writeln!(
        out,
        "  Connections ({} statements -> {} expanded):",
        conn["statements"], conn["expanded"]
    );
    if let Some(arr) = conn["per_stmt"].as_array() {
        for line in arr {
            let ln = line["line"]
                .as_u64()
                .map(|l| format!("L{l}"))
                .unwrap_or_else(|| "?".to_string());
            let count = line["connections"].as_array().map(|a| a.len()).unwrap_or(0);
            // Function-call statements carry no marker: a visible tree is the
            // expansion itself, and a funcall without a tree may still expand
            // into the cross-file section (the no_expansion counter already
            // exempts funcalls unless the body genuinely produced nothing).
            let flag = if line["funcall_empty"].as_bool().unwrap_or(false) {
                "  <<< EMPTY EXPANSION"
            } else if count == 0 && !line["funcall"].as_bool().unwrap_or(false) {
                "  <<< NO EXPANSION"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "    {ln:<6} {}{}",
                line["text"].as_str().unwrap_or(""),
                flag
            );
            // In-place expansion tree (funcall statements): record nodes carry
            // the nested expansion (func bodies, generated components, merged
            // sub-module bodies); call-site boundary products not covered by a
            // record are appended as first-level branches.
            if let Some(tree) = line["tree"].as_array().filter(|t| !t.is_empty()) {
                let mut used: HashSet<String> = HashSet::new();
                for node in tree {
                    collect_conn_keys(node, &mut used);
                }
                let mut branches: Vec<Branch> = Vec::new();
                for node in tree {
                    let label = node["label"].as_str().unwrap_or("");
                    branches.push(Branch::Node {
                        label: if label.is_empty() {
                            node["kind"].as_str().unwrap_or("").to_string()
                        } else {
                            label.to_string()
                        },
                        node,
                    });
                }
                if let Some(conns) = line["connections"].as_array() {
                    // §8.9.5: group bus/interface connections into trunks so a
                    // tree leaf can be a coarse header or a member pin2pin row.
                    let views: Vec<common::ConnView> = conns
                        .iter()
                        .filter(|c| !used.contains(&conn_key(c)))
                        .map(conn_view)
                        .collect();
                    for text in common::render_layered_conns(&views, "") {
                        branches.push(Branch::Leaf(text));
                    }
                }
                render_branches(out, "           ", &branches);
                continue;
            }
            if let Some(conns) = line["connections"].as_array() {
                // §8.9.5 layered rendering (trunks for bus/interface groups,
                // flat lines otherwise).
                let views: Vec<common::ConnView> = conns.iter().map(conn_view).collect();
                for text in common::render_layered_conns(&views, "           ") {
                    let _ = writeln!(out, "{text}");
                }
            }
            // §7.3: sub-module body expansion merged into this call site
            // (the callee body lives inside the sub-module instance, §7.3).
            if let Some(subs) = line["sub_expansions"].as_array() {
                for c in subs {
                    let pts = c["points"]
                        .as_array()
                        .map(|p| {
                            render_conn_points(
                                &p.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>(),
                                c["dir"].as_str().unwrap_or(""),
                            )
                        })
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "           [{}] (sub {}) : {}",
                        c["net"].as_str().unwrap_or(""),
                        c["sub"].as_str().unwrap_or(""),
                        pts
                    );
                }
            }
        }
    }
    if let Some(arr) = conn["declare_conns"].as_array() {
        // Cross-file body connections attributed to a declaration statement
        // (e.g. a component constructor body defined in another file):
        // rendered under that declaration line like a normal statement, so
        // `FLASH.GD25Q32E flash(V3V3)` shows the connections its constructor
        // func produced.
        for e in arr {
            let ln = e["line"]
                .as_u64()
                .map(|l| format!("L{l}"))
                .unwrap_or_default();
            let _ = writeln!(out, "    {ln:<6} {}", e["text"].as_str().unwrap_or(""));
            if let Some(conns) = e["conns"].as_array() {
                let views: Vec<common::ConnView> = conns.iter().map(conn_view).collect();
                for text in common::render_layered_conns(&views, "           ") {
                    let _ = writeln!(out, "{text}");
                }
            }
        }
    }
    if let Some(arr) = conn["cross_file"].as_array() {
        // Auxiliary information only (design §5.2): the connection is fully
        // visible under its call-site statement in the hierarchy tree; the
        // definition file + line is context.
        for c in arr {
            let _ = writeln!(
                out,
                "    [cross-file] [{}] def {}",
                c["net"].as_str().unwrap_or(""),
                c["source"].as_str().unwrap_or("")
            );
        }
    }
    if let Some(arr) = conn["unattributed"].as_array() {
        // expansion_id = None and no source statement to attribute to: should
        // be zero; any hit is a Pass2 attribution bug (design §5.1).
        for c in arr {
            let _ = writeln!(
                out,
                "    [unattributed] [{}] line {}",
                c["net"].as_str().unwrap_or(""),
                c["line"]
            );
        }
    }
    let _ = writeln!(out);
}
