// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The static acceptance engine for the module-body `expects` ledger
//! (circuit-intent-acceptance-design.md §4/§5.1; U298 batch ③).
//!
//! One verdict per row of the top module's ledger, judged on the frozen flat
//! world the caller built. Legality lives in the 4xxx/5xxx/6xxx gates; this
//! engine asks the other question — does the built top *fulfill* what the
//! author asked of it. A structural violation (target absent, class
//! mismatch, not driven) is an error; a value-window conflict is a
//! design-choice report (warning); a row with no declared fact to compare
//! against is DEFER — a verdict, never a diagnostic (§5.1).
//!
//! The diagnostics are returned, never logged: the acceptance layer is a
//! caller-side projection, and its rows must not re-fire from the workspace
//! store on every later reader.

use crate::db::defregistry::{
    adopted_recipes_of, def_id_by_identity, live_entry_by_id, variant_base_of,
};
use crate::db::diagnostic::diagnostic::{Diagnostic, DiagnosticLevel, Location};
use crate::eval::Value;
use crate::instant::insttab::{InstEntry, InstKind, InstTable, NetEntry};
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::module::expects::{Kind, Ledger, Row};
use crate::{IOType, McURI};

/// The verdict of one `expects` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The frozen world fulfills the row.
    Pass,
    /// The frozen world contradicts the row; a diagnostic rides along.
    Fail,
    /// No declared fact on the target to judge — the row waits for sim (§5.1).
    Defer,
}

/// One row's outcome: what was asked, how it landed, and the human sentence.
///
/// `kind` is the projection contract's word (`schema/projection.cddl` §2.6),
/// not a private spelling — the acceptance ledger view carries it verbatim,
/// and the fifth canonical word `presence` stays reserved until a row form
/// asks bare existence.
#[derive(Debug)]
pub struct RowOutcome {
    pub target: String,
    /// The row form: `role-match` | `driven` | `value-bound`.
    pub kind: &'static str,
    pub verdict: Verdict,
    pub detail: String,
    /// The row's own source span (P1a §3 LHS) — the ledger view's `expect`
    /// site. The uri is the ledger's, which the caller already holds.
    pub span: std::ops::Range<usize>,
    /// A FAIL value-bound row's flattened measured side: the declared values
    /// that fell outside the window, in the family's verbatim volt spelling.
    /// `None` on every other verdict.
    pub measured: Option<String>,
}

/// The whole ledger's outcome: per-row verdicts plus the FAIL diagnostics.
pub struct ExpectationReport {
    pub outcomes: Vec<RowOutcome>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ExpectationReport {
    /// (pass, fail, defer) row counts.
    pub fn counts(&self) -> (usize, usize, usize) {
        let count = |v| self.outcomes.iter().filter(|o| o.verdict == v).count();
        (
            count(Verdict::Pass),
            count(Verdict::Fail),
            count(Verdict::Defer),
        )
    }
}

/// What a row's judgment hands back to the report builder: a PASS/DEFER stays
/// silent; a FAIL names its code and message arguments.
enum Finding {
    Silent,
    Violation(u32, Vec<String>),
}

/// One row's full judgment — the pieces [`run`] folds into the [`RowOutcome`].
struct Judgment {
    verdict: Verdict,
    detail: String,
    finding: Finding,
    /// The measured side of a FAIL value-bound row (see
    /// [`RowOutcome::measured`]); `None` everywhere else.
    measured: Option<String>,
}

/// Judge every row of `module`'s expects ledger against the flat world
/// `table`. `table` is the top module's own frozen world — the caller built it
/// for exactly one entry, which is the engine-side top gate (§3: a module
/// reused as a sub-block is never judged here).
pub fn run(table: &InstTable, ledger: &Ledger, uri: &McURI) -> ExpectationReport {
    let mut report = ExpectationReport {
        outcomes: Vec::new(),
        diagnostics: Vec::new(),
    };
    let Some(root) = root_entry(table) else {
        // No flat world to read: nothing is judgeable, every row defers.
        for row in &ledger.rows {
            report.outcomes.push(RowOutcome {
                target: row.target.clone(),
                kind: kind_label(&row.kind),
                verdict: Verdict::Defer,
                detail: "the top built no flat world — nothing to judge on".to_string(),
                span: row.span.clone(),
                measured: None,
            });
        }
        return report;
    };

    for row in &ledger.rows {
        if row.deferred {
            // The row sits under a condition the static face cannot judge
            // (runtime quantity, unknown name) — §5.1: the whole entry defers,
            // a verdict only, never a diagnostic.
            report.outcomes.push(RowOutcome {
                target: row.target.clone(),
                kind: kind_label(&row.kind),
                verdict: Verdict::Defer,
                detail: "the row waits for a condition static checking cannot judge"
                    .to_string(),
                span: row.span.clone(),
                measured: None,
            });
            continue;
        }
        let judgment = judge(table, root, row);
        report.outcomes.push(RowOutcome {
            target: row.target.clone(),
            kind: kind_label(&row.kind),
            verdict: judgment.verdict,
            detail: judgment.detail,
            span: row.span.clone(),
            measured: judgment.measured,
        });
        if let Finding::Violation(code, args) = judgment.finding {
            let level = if code == crate::errcodes::EXPECTATION_VALUE_OUT_OF_WINDOW {
                DiagnosticLevel::Warning
            } else {
                DiagnosticLevel::Error
            };
            let msg = crate::errcodes::format_msg(code, &args.iter().map(|a| a as &dyn std::fmt::Display).collect::<Vec<_>>());
            report.diagnostics.push(Diagnostic::new(
                code,
                level,
                Location::new(uri.clone(), row.span.start as u32, row.span.len() as u32),
                msg,
            ));
        }
    }
    report
}

/// The projection contract's word for a row form
/// (`schema/projection.cddl` §2.6 `kind`).
fn kind_label(kind: &Kind) -> &'static str {
    match kind {
        Kind::Class(_) => "role-match",
        Kind::Driven => "driven",
        Kind::Window { .. } => "value-bound",
    }
}

fn judge(table: &InstTable, root: u32, row: &Row) -> Judgment {
    match &row.kind {
        Kind::Class(word) => judge_class(table, root, &row.target, word),
        Kind::Driven => judge_driven(table, root, &row.target),
        Kind::Window { low, high } => judge_window(
            table,
            root,
            &row.target,
            low.as_deref(),
            high.as_deref(),
        ),
    }
}

// ── class rows ──

fn judge_class(
    table: &InstTable,
    root: u32,
    target: &str,
    word: &str,
) -> Judgment {
    let Some(inst) = find_instance(table, root, target) else {
        return Judgment {
            verdict: Verdict::Fail,
            detail: format!("no instance named '{target}' in the built top"),
            finding: Finding::Violation(
                crate::errcodes::EXPECTATION_TARGET_MISSING,
                vec![target.to_string(), "instance".to_string()],
            ),
            measured: None,
        };
    };
    let faces = class_faces(inst);
    if faces.iter().any(|f| f == word) {
        Judgment {
            verdict: Verdict::Pass,
            detail: format!(
                "instance '{target}' instantiates '{}' — matches the expected '{word}'",
                inst.class_name
            ),
            finding: Finding::Silent,
            measured: None,
        }
    } else {
        Judgment {
            verdict: Verdict::Fail,
            detail: format!(
                "instance '{target}' instantiates '{}' — no declared face matches '{word}'",
                inst.class_name
            ),
            finding: Finding::Violation(
                crate::errcodes::EXPECTATION_CLASS_MISMATCH,
                vec![
                    target.to_string(),
                    inst.class_name.clone(),
                    word.to_string(),
                    faces.join(", "),
                ],
            ),
            measured: None,
        }
    }
}

/// The instance (component or sub-module) a class row addresses: a top-scope
/// name, or a dotted path under the top.
fn find_instance<'a>(table: &'a InstTable, root: u32, target: &str) -> Option<&'a InstEntry> {
    let root_path = table.get_entry(root)?.path.clone();
    table.iter_entries().find(|e| {
        e.id != root
            && matches!(e.kind, InstKind::Component | InstKind::Module)
            && (is_top_scope_child(e, root, target)
                || e.path == format!("{root_path}.{target}"))
    })
}

/// The faces a class row can expect of an instance (§3): the class itself,
/// its variant base chain, and its adopted recipes — exact def idents
/// read from the registry, the same keys `::` binding reads. Never derived
/// from names.
fn class_faces(entry: &InstEntry) -> Vec<String> {
    let mut faces = vec![entry.class_name.clone()];
    let Some(sn) = &entry.class_def else {
        return faces;
    };
    let Some(host) = def_id_by_identity(sn) else {
        return faces;
    };
    let mut cur = host;
    for _ in 0..16 {
        match variant_base_of(cur) {
            Some(base) => {
                cur = base;
                if let Some((bsn, _)) = live_entry_by_id(cur) {
                    faces.push(bsn.ident.to_string());
                }
            }
            None => break,
        }
    }
    for cap in adopted_recipes_of(host) {
        if let Some((csn, _)) = live_entry_by_id(cap) {
            faces.push(csn.ident.to_string());
        }
    }
    faces.sort();
    faces.dedup();
    faces
}

// ── driven rows ──

fn judge_driven(table: &InstTable, root: u32, target: &str) -> Judgment {
    let Some(net) = find_net(table, root, target) else {
        return Judgment {
            verdict: Verdict::Fail,
            detail: format!("no net named '{target}' in the built top"),
            finding: Finding::Violation(
                crate::errcodes::EXPECTATION_TARGET_MISSING,
                vec![target.to_string(), "net".to_string()],
            ),
            measured: None,
        };
    };
    if net_has_driver(table, net) {
        Judgment {
            verdict: Verdict::Pass,
            detail: format!("net '{target}' carries a declared driver"),
            finding: Finding::Silent,
            measured: None,
        }
    } else {
        Judgment {
            verdict: Verdict::Fail,
            detail: format!("net '{target}' carries no declared driver"),
            finding: Finding::Violation(
                crate::errcodes::EXPECTATION_NOT_DRIVEN,
                vec![target.to_string()],
            ),
            measured: None,
        }
    }
}

/// The top-scope net a driven/window row addresses: the net of that name the
/// top module's own net table produced, or — when the target spells a terminal
/// (`u2.vout`) — the top-scope net that terminal lands on.
fn find_net<'a>(table: &'a InstTable, root: u32, target: &str) -> Option<&'a NetEntry> {
    if let Some(net) = table
        .get_nets()
        .into_iter()
        .find(|n| n.module == Some(root) && n.name == target)
    {
        return Some(net);
    }
    let root_path = table.get_entry(root)?.path.clone();
    let terminal = table.iter_entries().find(|e| {
        matches!(e.kind, InstKind::Pin | InstKind::Port)
            && (is_top_scope_child(e, root, target)
                || e.path == format!("{root_path}.{target}"))
    })?;
    table
        .get_nets()
        .into_iter()
        .find(|n| n.module == Some(root) && n.points.contains(&terminal.id))
}

/// The declared-face rule `check_undriven_nets` reads: an endpoint is a driver
/// when its declared io type is `Out` or `Power`. No name table, and no
/// boundary-port exemption — at the top scope a port is the drive's own face.
fn net_has_driver(table: &InstTable, net: &NetEntry) -> bool {
    net.points.iter().any(|p| {
        table
            .get_entry(*p)
            .is_some_and(|e| matches!(e.io_type, IOType::Out | IOType::Power))
    })
}

// ── window rows ──

fn judge_window(
    table: &InstTable,
    root: u32,
    target: &str,
    low: Option<&str>,
    high: Option<&str>,
) -> Judgment {
    let Some(net) = find_net(table, root, target) else {
        return Judgment {
            verdict: Verdict::Fail,
            detail: format!("no net named '{target}' in the built top"),
            finding: Finding::Violation(
                crate::errcodes::EXPECTATION_TARGET_MISSING,
                vec![target.to_string(), "net".to_string()],
            ),
            measured: None,
        };
    };
    let declared = declared_voltages(table, net);
    if declared.is_empty() {
        // §5.1: no declared DC fact to compare — the row waits for sim.
        return Judgment {
            verdict: Verdict::Defer,
            detail: format!(
                "net '{target}' declares no DC rail value — the row waits for sim"
            ),
            finding: Finding::Silent,
            measured: None,
        };
    }
    // Both sides must bind as volt quantities through the eval text path; a
    // side that cannot is outside the static gate's reach, not a violation.
    let low_v = low.map(parse_volt);
    let high_v = high.map(parse_volt);
    if low.is_some_and(|_| low_v.is_none()) || high.is_some_and(|_| high_v.is_none()) {
        return Judgment {
            verdict: Verdict::Defer,
            detail: format!(
                "the window of the row on '{target}' is not a volt window — outside the static gate's reach"
            ),
            finding: Finding::Silent,
            measured: None,
        };
    }
    let outside: Vec<f64> = declared
        .iter()
        .copied()
        .filter(|v| !in_window(*v, low_v.flatten(), high_v.flatten()))
        .collect();
    if outside.is_empty() {
        Judgment {
            verdict: Verdict::Pass,
            detail: format!(
                "net '{target}' declares {} — inside the window",
                fmt_volts(&declared)
            ),
            finding: Finding::Silent,
            measured: None,
        }
    } else {
        let measured = fmt_volts(&outside);
        Judgment {
            verdict: Verdict::Fail,
            detail: format!("net '{target}' declares {measured} — outside the window"),
            finding: Finding::Violation(
                crate::errcodes::EXPECTATION_VALUE_OUT_OF_WINDOW,
                vec![
                    target.to_string(),
                    measured.clone(),
                    fmt_window(low, high),
                ],
            ),
            measured: Some(measured),
        }
    }
}

/// Every declared DC voltage fact on the net (§5.1): the union of the declared
/// supply values its pins carry — the same set the 4105 voltage comparison
/// reads.
fn declared_voltages(table: &InstTable, net: &NetEntry) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for pid in &net.points {
        let Some(e) = table.get_entry(*pid) else {
            continue;
        };
        if !matches!(e.kind, InstKind::Pin) {
            continue;
        }
        if let Some(vs) = super::nets::pin_declared_voltages(table, e) {
            out.extend(vs);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup();
    out
}

/// Bind one author spelling to a volt quantity through the eval text path —
/// the same suffix table the AST path reads. `None` when the text is not a
/// volt quantity (the gate stays silent on it rather than guess).
fn parse_volt(text: &str) -> Option<f64> {
    match Value::from_text(text) {
        Value::Quantity(uv) if matches!(uv.unit(), McUnit::Volt) => Some(uv.value()),
        _ => None,
    }
}

/// Closed bounds, matching the ratings family's interval reading; an absent
/// side is open.
fn in_window(v: f64, low: Option<f64>, high: Option<f64>) -> bool {
    low.map_or(true, |l| v >= l) && high.map_or(true, |h| v <= h)
}

fn fmt_volts(vs: &[f64]) -> String {
    vs.iter()
        .map(|v| format!("{v}V"))
        .collect::<Vec<_>>()
        .join("/")
}

fn fmt_window(low: Option<&str>, high: Option<&str>) -> String {
    let mut sides: Vec<String> = Vec::new();
    if let Some(l) = low {
        sides.push(format!("low:{l}"));
    }
    if let Some(h) = high {
        sides.push(format!("high:{h}"));
    }
    sides.join(", ")
}

// ── shared lookups ──

/// The flat world's root: the one module entry with no parent.
fn root_entry(table: &InstTable) -> Option<u32> {
    table
        .iter_entries()
        .find(|e| e.kind == InstKind::Module && e.parent_id.is_none())
        .map(|e| e.id)
}

fn is_top_scope_child(e: &InstEntry, root: u32, target: &str) -> bool {
    e.parent_id == Some(root)
        && e.path.rsplit('.').next() == Some(target)
}

#[cfg(test)]
mod tests;
