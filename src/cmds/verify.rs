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
use mcc::{InstOrigin, McInstance, McModuleInst, Span};
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
        render_text(&mut text, &top, &summary, &modules);
        let buf = text.trim_end().to_string();
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, buf)?;
        } else {
            println!("{buf}");
        }
    } else {
        let data = json!({ "type": "verify", "top": top, "summary": summary, "modules": modules });
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
    let (inst_report, inst_counts) = compare_instances(inst);
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

fn compare_instances(inst: &McModuleInst) -> (Value, (usize, usize, usize, usize, usize)) {
    // Source-declared physical instance names. Engine-generated pseudo
    // instances (`@RES1` auto ports), non-physical entries (List / BusRef /
    // pins / attrs / funcs / enums) and array-form interfaces
    // (`[VDD_3V3, GND]`, which expand to member labels instead of a bus) are
    // not part of the instance contract. Each entry carries its source line
    // for the `L<n>` column in the report.
    let mut source: Vec<(String, String, u32)> = Vec::new();
    let mut source_names: HashSet<String> = HashSet::new();
    let content = std::fs::read_to_string(&inst.def_uri.to_string()).ok();
    let line_of_span = |sp: &std::ops::Range<usize>| -> u32 {
        content
            .as_ref()
            .map(|c| line_of_byte(c, sp.start))
            .unwrap_or(0)
    };
    // Line fallbacks for entries whose declaration span is not recorded in
    // the Pass1 inst table: module-header parameter ports (`dc{VDD_3V3,GND}`
    // and `in [VDD_3V3, GND]` rows) carry their span in `def.params`, and
    // inline net labels (e.g. `USB_VBUS` inside a connection statement)
    // resolve through the expanded label NetPoint's src_pos.
    let param_line = |name: &str| -> u32 {
        inst.def
            .params
            .iter_defs_with_span()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| line_of_span(&s))
            .unwrap_or(0)
    };
    let label_line = |name: &str| -> u32 {
        inst.get_labels()
            .get(name)
            .and_then(|p| p.src_pos)
            .map(|pos| {
                content
                    .as_ref()
                    .map(|c| line_of_byte(c, pos as usize))
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    };
    // Bracket-vec port members (`in [VDD_3V3, GND]::DC(3.3V)`) are stored
    // under the whole-bracket key (`[VDD_3V3, GND]`) with no per-member
    // span; match the member name against those keys for the declaration
    // line of the header row.
    let member_line = |name: &str| -> u32 {
        inst.def
            .insts
            .port_spans()
            .iter()
            .filter_map(|(key, spans)| {
                if !(key.starts_with('[') || key.starts_with('{')) {
                    return None;
                }
                let mut members = key
                    .trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}')
                    .split(',')
                    .map(str::trim);
                members
                    .any(|m| m == name)
                    .then(|| spans.first().cloned())
                    .flatten()
            })
            .next()
            .map(|sp| line_of_span(&sp))
            .unwrap_or(0)
    };
    for (name, mc_inst) in inst.def.insts.iter() {
        if name.starts_with('@') || name.starts_with('[') {
            continue;
        }
        let kind = match mc_inst {
            McInstance::Component(_) => Some("component"),
            McInstance::Module(_) => Some("module"),
            McInstance::Interface(_) => Some("interface"),
            McInstance::Bus(_) => Some("bus"),
            McInstance::Label(_) => Some("label"),
            _ => None,
        };
        if let Some(kind) = kind {
            let line = inst
                .def
                .insts
                .get_port_span(name)
                .map(|sp| line_of_span(&sp))
                .or_else(|| {
                    let l = param_line(name);
                    if l > 0 {
                        Some(l)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let l = label_line(name);
                    if l > 0 {
                        Some(l)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let l = member_line(name);
                    if l > 0 {
                        Some(l)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            source.push((name.to_string(), kind.to_string(), line));
            source_names.insert(name.to_string());
        }
    }
    // Declareb instances (`C4::CAP()`) bypass `parse_declare`, so their names
    // never enter `insts`; they are recorded in the declareb hint table and
    // expand with a FuncCall origin.
    let mut declareb: Vec<(String, u32)> = Vec::new();
    for (name, (_, span)) in inst.def.insts.iter_declareb_defs() {
        source_names.insert(name.clone());
        declareb.push((name.clone(), line_of_span(&span)));
    }
    declareb.sort();

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
                if source_names.contains(&comp.name) {
                    expanded_declared.insert(comp.name.clone());
                    expanded.push((
                        comp.name.clone(),
                        "component".to_string(),
                        "declareb".to_string(),
                        line,
                    ));
                } else {
                    expanded.push((
                        comp.name.clone(),
                        "component".to_string(),
                        "funcall".to_string(),
                        line,
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

    let mut generated: Vec<(String, u32)> = expanded
        .iter()
        .filter(|(_, _, origin, _)| origin == "funcall")
        .map(|(n, _, _, l)| (n.clone(), *l))
        .collect();
    generated.sort();

    let mut expanded_all: Vec<(String, String, String, u32)> = expanded.clone();
    for n in &net_labels {
        expanded_all.push((n.clone(), "label".to_string(), "derived".to_string(), 0));
    }
    expanded_all.sort_by(|a, b| (a.0.clone(), a.1.clone()).cmp(&(b.0.clone(), b.1.clone())));

    let report = json!({
        "source": source.iter().map(|(n, k, l)| json!({"name": n, "kind": k, "line": l})).collect::<Vec<_>>(),
        "declareb": declareb.iter().map(|(n, l)| json!({"name": n, "line": l})).collect::<Vec<_>>(),
        "expanded": expanded_all.iter().map(|(n, k, o, l)| json!({"name": n, "kind": k, "origin": o, "line": l})).collect::<Vec<_>>(),
        "missing": missing,
        "extra": extra,
        "generated": generated.iter().map(|(n, l)| json!({"name": n, "line": l})).collect::<Vec<_>>(),
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
    // file so expanded connections (tagged by line) can be attributed to the
    // exact source statement that produced them.
    let def_file = inst.def_uri.to_string();
    let content = std::fs::read_to_string(&def_file).ok();
    let mut src_by_line: BTreeMap<u32, usize> = BTreeMap::new();
    if let Some(c) = &content {
        for (i, sp) in spans.iter().enumerate() {
            src_by_line.insert(line_of_byte(c, sp.start as usize), i);
        }
    }

    let mut per_stmt: Vec<Vec<ConnEntry>> = (0..lines.len()).map(|_| Vec::new()).collect();
    let mut untraced: Vec<String> = Vec::new();
    let mut cross_file: Vec<Value> = Vec::new();
    let mut unmatched: Vec<Value> = Vec::new();

    for conn in &inst.connections {
        let entry = ConnEntry {
            net: conn.effective_net_name(),
            points: conn.points.iter().map(|p| p.path.clone()).collect(),
        };
        match &conn.source_span {
            // No source span: engine-internal connection (e.g. auto pullup).
            None => untraced.push(entry.net),
            Some((file, line)) => {
                if *file != def_file {
                    // Connection created while instantiating a function whose
                    // body lives in another file (e.g. `uC.i2c()`); informative.
                    cross_file.push(json!({
                        "net": entry.net,
                        "points": entry.points,
                        "source": format!("{file}:{line}"),
                    }));
                } else if content.is_some() {
                    match src_by_line.get(line) {
                        Some(&idx) => per_stmt[idx].push(entry),
                        None => unmatched.push(json!({
                            "net": entry.net,
                            "points": entry.points,
                            "file": file,
                            "line": line,
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
    // nothing is flagged; statements containing function calls expand inside
    // the callee (whose body lines belong to the callee's file), so they are
    // exempt.
    let mut per_line: Vec<Value> = Vec::with_capacity(lines.len());
    let mut no_expansion = 0usize;
    for (i, phrase) in lines.iter().enumerate() {
        let has_funccall = phrase_contains_funccall(phrase);
        let conns: Vec<Value> = per_stmt[i]
            .iter()
            .map(|c| json!({"net": c.net, "points": c.points}))
            .collect();
        if conns.is_empty() && !has_funccall {
            no_expansion += 1;
        }
        per_line.push(json!({
            "line": line_of_span(&content, spans.get(i)),
            "text": format!("{phrase}"),
            "funcall": has_funccall,
            "connections": conns,
        }));
    }

    let report = json!({
        "statements": lines.len(),
        "expanded": inst.connections.len(),
        "per_line": per_line,
        "untraced": untraced,
        "cross_file": cross_file,
        "unmatched": unmatched,
    });
    let counts = (
        lines.len(),
        inst.connections.len(),
        untraced.len(),
        no_expansion,
    );
    (report, counts)
}

/// 1-based line number of a byte offset within `content`.
fn line_of_byte(content: &str, offset: usize) -> u32 {
    let end = offset.min(content.len());
    content.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
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
        (Some(c), Some(sp)) => Some(line_of_byte(c, sp.start as usize)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Text rendering
// ---------------------------------------------------------------------------

fn render_text(out: &mut String, top: &str, summary: &Value, modules: &[Value]) {
    let inst = &summary["instances"];
    let conn = &summary["connections"];
    let problems = inst["missing"].as_u64().unwrap_or(0)
        + inst["extra"].as_u64().unwrap_or(0)
        + conn["no_expansion"].as_u64().unwrap_or(0);
    let _ = writeln!(
        out,
        "Verify: {top} | modules: {} | instances: source={} expanded={} missing={} extra={} generated={} | connections: statements={} expanded={} untraced={} no_expansion={}",
        summary["modules"],
        inst["source"],
        inst["expanded"],
        inst["missing"],
        inst["extra"],
        inst["generated"],
        conn["statements"],
        conn["expanded"],
        conn["untraced"],
        conn["no_expansion"],
    );
    let _ = writeln!(
        out,
        "Result: {}",
        if problems > 0 { "MISMATCH" } else { "OK" }
    );
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
    if let Some(arr) = inst["source"].as_array() {
        for e in arr {
            let ln = line_col(e);
            let name = e["name"].as_str().unwrap_or("");
            let kind = e["kind"].as_str().unwrap_or("");
            let _ = writeln!(out, "    {ln:<5}[src]  {name:<name_w$} {kind}");
        }
    }
    if let Some(arr) = inst["declareb"].as_array() {
        for e in arr {
            let ln = line_col(e);
            let name = e["name"].as_str().unwrap_or("");
            let _ = writeln!(out, "    {ln:<5}[decl] {name:<name_w$} (declareb)");
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
            let _ = writeln!(out, "    {ln:<5}[gen]  {name:<name_w$} (funcall)");
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
            let flag = if line["funcall"].as_bool().unwrap_or(false) {
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
        }
    }
    if let Some(arr) = conn["untraced"].as_array() {
        for n in arr {
            let _ = writeln!(out, "    [untraced] [{}]", n.as_str().unwrap_or(""));
        }
    }
    if let Some(arr) = conn["cross_file"].as_array() {
        for c in arr {
            let _ = writeln!(
                out,
                "    [cross-file] [{}] from {}",
                c["net"].as_str().unwrap_or(""),
                c["source"].as_str().unwrap_or("")
            );
        }
    }
    if let Some(arr) = conn["unmatched"].as_array() {
        for c in arr {
            let _ = writeln!(
                out,
                "    [unmatched] [{}] line {}",
                c["net"].as_str().unwrap_or(""),
                c["line"]
            );
        }
    }
    let _ = writeln!(out);
}
