// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `expectation` projection — the acceptance ledger, read as data.
//!
//! The fifth carried canonical `view-name` word (`schema/projection.cddl`
//! §2.6): the per-top verdict ledger for a declared `expects` block
//! (circuit-intent-acceptance-design.md §4), one projection of the family
//! (`projection-schema-design.md` §2.6). The diagnostics view carries only a
//! violation row's FAIL diagnostic; this view carries the **whole** account —
//! PASS and DEFER rows included — which is what an AI consumer and the
//! functional-alignment diff both read.
//!
//! The items are built from one acceptance run ([`crate::check::expectation::
//! run`]) — the same engine the `check` gate judges with, so the two faces
//! cannot spell the verdicts two ways. The rows' order is the ledger's own
//! source order, which is a function of the world. Two v1 contract notes,
//! both recorded on the CDDL group itself:
//!
//! * `level` is present on FAIL items only — a PASS/DEFER row has no
//!   violation to grade;
//! * `bound`'s sides are each optional, the `ratings-bound` precedent: a
//!   side the row does not state is absent, never null.
//!
//! `presence` (the fourth `kind` word) and `fix_hint` stay reserved — no row
//! form asks bare existence, and no producer computes a mechanical fix.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::diagview::DiagLoc;
use super::StageView;
use crate::db::diagnostic::diagnostic::Location;
use crate::semantic::module::expects::{Kind, Ledger, Row};
use crate::semantic::validation::expectation::{ExpectationReport, Verdict};
use crate::McURI;

/// The canonical word this face publishes (CDDL `view-name`).
pub const EXPECTATION_VIEW: &str = "expectation";

/// CDDL `expectation-verdict` — one row's account, member names verbatim
/// from `schema/projection.cddl`. The optional members are omitted when
/// absent, never serialized as null.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectationVerdict {
    /// The row's source site (the `loc` group): the ledger row's own span.
    pub expect: DiagLoc,
    /// The referenced symbol, as the row spells it (P1a §3 LHS).
    pub target: String,
    /// `role-match` | `driven` | `value-bound` (`presence` reserved).
    pub kind: String,
    /// The value-bound row's window, the author's notation verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound: Option<Bound>,
    /// `PASS` | `FAIL` | `DEFER` (the CDDL `verdict` rule's words).
    pub verdict: String,
    /// The FAIL diagnostic's code, `E{:04}` — the spelling every face uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The FAIL's report level: `error` (structural) or `warning`
    /// (value-bound overrun). FAIL items only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// A FAIL value-bound row's flattened measured side — the declared
    /// values outside the window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured: Option<String>,
    /// Mechanically executable action (M4 5W). Reserved: no producer yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
}

/// CDDL `bound` — a value-bound row's sides, each optional (the
/// `ratings-bound` precedent: absent, not null).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bound {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
}

fn verdict_word(v: Verdict) -> &'static str {
    match v {
        Verdict::Pass => "PASS",
        Verdict::Fail => "FAIL",
        Verdict::Defer => "DEFER",
    }
}

/// Adapt one ledger row plus its outcome to the contract item. `diag` is the
/// row's own FAIL diagnostic when the row has one.
fn item(row: &Row, outcome: &crate::semantic::validation::expectation::RowOutcome, diag: Option<&crate::db::diagnostic::diagnostic::Diagnostic>, uri: &McURI) -> ExpectationVerdict {
    let loc = Location::new(uri.clone(), row.span.start as u32, row.span.len() as u32);
    let bound = match &row.kind {
        Kind::Window { low, high } => Some(Bound {
            low: low.clone(),
            high: high.clone(),
        }),
        _ => None,
    };
    ExpectationVerdict {
        expect: DiagLoc {
            uri: uri.to_string(),
            line: loc.row,
            span: Some(row.span.len() as u32),
        },
        target: outcome.target.clone(),
        kind: outcome.kind.to_string(),
        bound,
        verdict: verdict_word(outcome.verdict).to_string(),
        code: diag.map(|d| format!("E{:04}", d.code)),
        level: diag.map(|d| match d.code {
            crate::errcodes::EXPECTATION_VALUE_OUT_OF_WINDOW => "warning".to_string(),
            _ => "error".to_string(),
        }),
        measured: outcome.measured.clone(),
        fix_hint: None,
    }
}

/// The ledger's items, in the ledger's own source order — the engine pushes
/// exactly one outcome per row in ledger order, and exactly one diagnostic
/// per FAIL row in that same order, so the two zips line every row up with
/// its own verdict and its own diagnostic. The assertion guards that
/// invariant instead of trusting it.
pub fn expectation_items(ledger: &Ledger, report: &ExpectationReport, uri: &McURI) -> Vec<Value> {
    assert_eq!(
        ledger.rows.len(),
        report.outcomes.len(),
        "the engine owes one outcome per ledger row"
    );
    let mut diags = report.diagnostics.iter();
    ledger
        .rows
        .iter()
        .zip(report.outcomes.iter())
        .map(|(row, outcome)| {
            let diag = if outcome.verdict == Verdict::Fail {
                Some(diags.next().expect("a FAIL row carries its diagnostic"))
            } else {
                None
            };
            serde_json::to_value(item(row, outcome, diag, uri)).unwrap_or(Value::Null)
        })
        .collect()
}

/// Per-face counts: the verdict words — computed from the **items** so the
/// header and the rows cannot disagree. Every word is printed even when zero.
pub fn expectation_counts(items: &[Value]) -> Value {
    let count = |word: &str| {
        items
            .iter()
            .filter(|i| i["verdict"].as_str() == Some(word))
            .count()
    };
    serde_json::json!({
        "pass": count("PASS"),
        "fail": count("FAIL"),
        "defer": count("DEFER"),
    })
}

/// Assemble the projection. Goes through [`StageView::with_view`] — this view
/// publishes its own vocabulary and its own count words, not a pipeline
/// segment's.
pub fn expectation_view(top: &str, ledger: &Ledger, report: &ExpectationReport, uri: &McURI) -> StageView {
    let items = expectation_items(ledger, report, uri);
    let counts = expectation_counts(&items);
    StageView::with_view(EXPECTATION_VIEW, top, items, counts)
}

/// The text face, rendered from the **same** items the envelope carries:
/// header, the counts, then one row per entry — `verdict`, `kind`, `target`,
/// the bound when the row states one, and for a FAIL its code, level,
/// measured side and site. A DEFER row says what DEFER means here: the row
/// waits for sim.
pub fn render_expectation_text(view: &StageView) -> String {
    let mut lines = vec![view.header_line()];
    let words: Vec<String> = view
        .counts
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k} {}", v.as_u64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    lines.push(format!("# {}", words.join("  ")));
    let items: Vec<&Value> = view.items.iter().collect();
    let widest = |f: fn(&Value) -> String| -> usize {
        items.iter().map(|i| f(i).len()).max().unwrap_or(0)
    };
    let verdict_col = widest(|i| i["verdict"].as_str().unwrap_or("-").to_string()).max(7);
    let kind_col = widest(|i| i["kind"].as_str().unwrap_or("-").to_string()).max(4);
    let target_col = widest(|i| i["target"].as_str().unwrap_or("-").to_string()).max(6);
    lines.push(format!(
        "{:<verdict_col$}  {:<kind_col$}  {:<target_col$}  {}",
        "verdict", "kind", "target", "bound / code / measured / loc"
    ));
    for it in &items {
        let mut tail = String::new();
        if let Some(b) = it["bound"].as_object() {
            let low = b.get("low").and_then(|v| v.as_str()).unwrap_or("-");
            let high = b.get("high").and_then(|v| v.as_str()).unwrap_or("-");
            tail.push_str(&format!("window [{low}, {high}]"));
        }
        if it["verdict"].as_str() == Some("FAIL") {
            if !tail.is_empty() {
                tail.push_str("  ");
            }
            tail.push_str(&format!(
                "{} {} measured {}  {}",
                it["code"].as_str().unwrap_or("-"),
                it["level"].as_str().unwrap_or("-"),
                it["measured"].as_str().unwrap_or("-"),
                super::loc_cell(&it["expect"]),
            ));
        }
        if it["verdict"].as_str() == Some("DEFER") {
            tail.push_str("awaits sim (no declared fact to judge)");
        }
        lines.push(format!(
            "{:<verdict_col$}  {:<kind_col$}  {:<target_col$}  {}",
            it["verdict"].as_str().unwrap_or("-"),
            it["kind"].as_str().unwrap_or("-"),
            it["target"].as_str().unwrap_or("-"),
            tail,
        ));
    }
    lines.join("\n")
}
