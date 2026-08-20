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
use mcc::{InstOrigin, McModuleInst, Span};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
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
struct ConnEntry {
    net: String,
    points: Vec<String>,
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
    let inst = common::build_pass2(&top, &uri).map_err(anyhow::Error::msg)?;

    let mut totals = VerifyTotals::default();
    let mut modules: Vec<Value> = Vec::new();
    verify_module(&inst, &top, &mut totals, &mut modules);
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

    // Untraced connections are engine-generated projection links (interface /
    // bus member nets) with no source statement of their own; they are
    // reported for inspection but do not fail the verification. Only real
    // source-vs-expansion mismatches set the exit code.
    let problems = totals.missing + totals.extra + totals.no_expansion;
    let format = mcc::cli::globals().format;
    if format == OutputFormat::Text {
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
fn verify_module(inst: &McModuleInst, path: &str, totals: &mut VerifyTotals, out: &mut Vec<Value>) {
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
    for sub in &inst.sub_modules {
        verify_module(sub, &format!("{path}.{}", sub.name), totals, out);
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
    expanded_all.sort_by(|a, b| (a.0.clone(), a.1.clone()).cmp(&(b.0.clone(), b.1.clone())));

    let report = json!({
        "source": source.iter().map(|(n, k, l, cl)| json!({"name": n, "kind": k, "line": l, "class": cl})).collect::<Vec<_>>(),
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

fn compare_connections(inst: &McModuleInst) -> (Value, (usize, usize, usize, usize)) {
    let lines = &inst.def.lines;
    let spans = &inst.def.line_spans;

    // Build a byte-offset -> line-number map from the module's own source
    // file so expanded connections (tagged by `source_span`, decision A §7.1
    // byte offset) can be attributed to the exact source statement that
    // produced them.
    let def_file = inst.def_uri.to_string();
    let content = std::fs::read_to_string(&def_file).ok();
    let mut src_by_line: BTreeMap<u32, usize> = BTreeMap::new();
    if let Some(c) = &content {
        for (i, sp) in spans.iter().enumerate() {
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
    let mut seen_sub_conns: HashSet<String> = HashSet::new();
    for r in inst.expansion.records.iter() {
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
                    sub_expansions.entry(pos.offset).or_default().push(json!({
                        "sub": sub_path,
                        "net": c.effective_net_name(),
                        "points": c.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
                    }));
                }
            }
        }
    }

    let mut per_stmt: Vec<Vec<ConnEntry>> = (0..lines.len()).map(|_| Vec::new()).collect();
    let mut untraced: Vec<String> = Vec::new();
    let mut cross_file: Vec<Value> = Vec::new();
    let mut unattributed: Vec<Value> = Vec::new();

    for conn in &inst.connections {
        let entry = ConnEntry {
            net: conn.effective_net_name(),
            points: conn.points.iter().map(|p| p.path.clone()).collect(),
        };
        match &conn.source_span {
            // No source span: engine-internal projection link (interface / bus
            // member net), legal (§5.4).
            None => untraced.push(entry.net),
            Some(pos) => {
                if pos.uri != def_file {
                    // Connection created while instantiating a function whose
                    // body lives in another file (e.g. `uC.i2c()`). The call
                    // site is visible in the hierarchy tree; the definition
                    // file + line here is auxiliary info only (§5.2).
                    let line = std::fs::read_to_string(&pos.uri)
                        .ok()
                        .map(|c| hierarchy::line_of_byte(&c, pos.offset as usize))
                        .unwrap_or(0);
                    cross_file.push(json!({
                        "net": entry.net,
                        "points": entry.points,
                        "source": format!("{}:{}", pos.uri, line),
                    }));
                } else if content.is_some() {
                    // `source_span` carries a byte offset (decision A);
                    // convert to a line number to attribute the statement.
                    let ln =
                        hierarchy::line_of_byte(content.as_ref().unwrap(), pos.offset as usize);
                    match src_by_line.get(&ln) {
                        Some(&idx) => per_stmt[idx].push(entry),
                        None if conn.expansion_id.is_some() => {
                            // Attributed to an expansion record (its
                            // call_site / def_site locates it); not a module
                            // statement product, so not unattributed.
                        }
                        None => unattributed.push(json!({
                            "net": entry.net,
                            "points": entry.points,
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
    let mut per_line: Vec<Value> = Vec::with_capacity(lines.len());
    let mut no_expansion = 0usize;
    for (i, phrase) in lines.iter().enumerate() {
        let has_funccall = phrase_contains_funccall(phrase);
        let conns: Vec<Value> = per_stmt[i]
            .iter()
            .map(|c| json!({"net": c.net, "points": c.points}))
            .collect();
        let stmt_off = spans.get(i).map(|sp| sp.start as u32);
        let empty_expansion = stmt_off
            .and_then(|off| stmt_records.get(&off))
            .map(|recs| {
                let body_kinds = [
                    mcc::ExpansionKind::InstanceMethod,
                    mcc::ExpansionKind::UserFunc,
                    mcc::ExpansionKind::ModuleCall,
                    mcc::ExpansionKind::BuiltinTwopin,
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
                body.iter().all(|&k| {
                    let r = &inst.expansion.records[k];
                    if r.skipped {
                        return true; // deliberate empty expansion (P2-8)
                    }
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
        per_line.push(json!({
            "line": line_of_span(&content, spans.get(i)),
            "text": format!("{phrase}"),
            "funcall": has_funccall,
            "funcall_empty": funcall_empty,
            "connections": conns,
            "sub_expansions": sub_conns,
        }));
    }

    let report = json!({
        "statements": lines.len(),
        "expanded": inst.connections.len(),
        "per_line": per_line,
        "untraced": untraced,
        "cross_file": cross_file,
        "unattributed": unattributed,
    });
    let counts = (
        lines.len(),
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

    for m in modules {
        render_module_text(out, m);
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
    let mark_w = {
        let kind = inst["source"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| e["kind"].as_str().map(|k| k.chars().count()))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        kind.max("(declareb)".len()).max("(funcall)".len()) + 1
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
            let kind = e["kind"].as_str().unwrap_or("");
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
            let mark = "(declareb)";
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
            let mark = "(funcall)";
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
    if let Some(arr) = conn["per_line"].as_array() {
        for line in arr {
            let ln = line["line"]
                .as_u64()
                .map(|l| format!("L{l}"))
                .unwrap_or_else(|| "?".to_string());
            let count = line["connections"].as_array().map(|a| a.len()).unwrap_or(0);
            let flag = if line["funcall_empty"].as_bool().unwrap_or(false) {
                "  <<< EMPTY EXPANSION"
            } else if line["funcall"].as_bool().unwrap_or(false) {
                " (funcall)"
            } else if count == 0 {
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
            if let Some(conns) = line["connections"].as_array() {
                for c in conns {
                    let pts = c["points"]
                        .as_array()
                        .map(|p| {
                            p.iter()
                                .filter_map(|x| x.as_str())
                                .collect::<Vec<_>>()
                                .join(" - ")
                        })
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "           [{}] : {}",
                        c["net"].as_str().unwrap_or(""),
                        pts
                    );
                }
            }
            // §7.3: sub-module body expansion merged into this call site
            // (the callee body lives inside the sub-module instance, §7.3).
            if let Some(subs) = line["sub_expansions"].as_array() {
                for c in subs {
                    let pts = c["points"]
                        .as_array()
                        .map(|p| {
                            p.iter()
                                .filter_map(|x| x.as_str())
                                .collect::<Vec<_>>()
                                .join(" - ")
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
