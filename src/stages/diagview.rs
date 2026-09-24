// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `diagnostics` projection — every collected diagnostic, read as data.
//!
//! The first of the six canonical `view-name` words
//! (`schema/projection.cddl`) to carry its serde payload group: the `diag`
//! item group below spells the CDDL members verbatim (`code` / `level` / `msg`
//! / `loc` / `fix_hint` / `net` / `pin`), so the CDDL↔serde drift the schema
//! family guards against dies here. The lock
//! `tests/shard7/view_vocabulary.rs` holds the member sets equal on both
//! sides, and `tests/shard7/diag_view_golden.rs` locks the serialized bytes.
//!
//! The source of the items is the workspace diagnostic store as a whole —
//! pass1 parse/semantic findings, pass2 instantiation findings, and the flat
//! net/ERC checks `mcb_pass2_flat` logs — the same aggregate `mcc check`
//! reports. The view is a **readout, not a verdict** (law C): a full error
//! count never flips an exit code.
//!
//! Two known v1 gaps, both named rather than papered over: `fix_hint` has no
//! producer yet (the command envelope's suggestions are likewise
//! zero-producer), and `net` / `pin` cannot be recovered from the store (the
//! net check's `net_name` is dropped when a finding is logged as a plain
//! diagnostic). The members stay in the item because the contract reserves
//! them; the fixture exercises them by constructing items directly.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::StageView;
use crate::db::diagnostic::diagnostic::{Diagnostic, DiagnosticLevel};

/// The canonical word this face publishes (CDDL `view-name`).
pub const DIAGNOSTICS_VIEW: &str = "diagnostics";

/// CDDL `diag.level`: `"error" / "warning" / "advisory" / "info"`.
///
/// The internal [`DiagnosticLevel`] has `Hint` where the contract spells
/// `advisory`; the mapping lives in [`level_of`] and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
    Advisory,
    Info,
}

/// The one place the internal level vocabulary meets the contract's: `Hint`
/// reads as `advisory` on this face. The command envelope's `Severity` keeps
/// its own `hint` spelling — that face is not this one.
fn level_of(level: DiagnosticLevel) -> Level {
    match level {
        DiagnosticLevel::Error => Level::Error,
        DiagnosticLevel::Warning => Level::Warning,
        DiagnosticLevel::Info => Level::Info,
        DiagnosticLevel::Hint => Level::Advisory,
    }
}

/// CDDL `loc = { uri, line, span }`. All three members are required by the
/// group, so none is skipped: a diagnostic with no extent serializes `span`
/// as `null`, the same discipline as [`super::loc_value`] — filling it with a
/// made-up end would be inventing data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagLoc {
    pub uri: String,
    pub line: u32,
    pub span: Option<u32>,
}

/// One `diag` item, member names verbatim from `schema/projection.cddl`
/// (`diag = { code, level, msg, loc, ?fix_hint, ?net, ?pin }`). The four
/// required members always serialize; the three optional ones are omitted
/// when absent, which is what makes the minimal key set the contract's
/// optional-member set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagItem {
    /// The code as every human face spells it: `E{:04}` (the same rendering
    /// the text diagnostics print, `src/output/mod.rs`). One function so a
    /// re-spelling is one line.
    pub code: String,
    pub level: Level,
    pub msg: String,
    pub loc: DiagLoc,
    /// Mechanically executable action (M4 5W). Reserved: no producer yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
    /// Associated net, when the finding has one. Reserved: dropped at
    /// store-logging time today.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
    /// Associated pin. Reserved, same as [`DiagItem::net`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

/// The code spelling, in exactly one place.
fn code_string(code: u32) -> String {
    format!("E{code:04}")
}

/// Adapt one store diagnostic to the contract item.
///
/// The future seams are functions, not inlined `None`s: when a producer for
/// suggestions or for the net-check's `net_name` lands, only these bodies
/// change — the item shape and the callers stay.
fn fix_hint_of(_d: &Diagnostic) -> Option<String> {
    None
}

fn net_of(_d: &Diagnostic) -> Option<String> {
    None
}

fn pin_of(_d: &Diagnostic) -> Option<String> {
    None
}

fn diag_item(d: &Diagnostic) -> DiagItem {
    DiagItem {
        code: code_string(d.code),
        level: level_of(d.level),
        msg: d.msg.clone(),
        loc: DiagLoc {
            uri: d.loc.uri.as_str().to_string(),
            line: d.loc.row,
            span: (d.loc.len > 0).then_some(d.loc.len),
        },
        fix_hint: fix_hint_of(d),
        net: net_of(d),
        pin: pin_of(d),
    }
}

/// Drop diagnostics a later entry reported again — a file reached by two
/// entries' `use` closures is parsed once per world, so its diagnostics
/// arrive once per entry that reaches it. Same key `mcc check` dedups by
/// (`code`, `msg`, `uri`, `pos`): the view reads the aggregate a check
/// reports, not the store's raw append log.
pub fn dedup_diags(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    for d in diags {
        let seen = out.iter().any(|e| {
            e.code == d.code && e.msg == d.msg && e.loc.uri == d.loc.uri && e.loc.pos == d.loc.pos
        });
        if !seen {
            out.push(d);
        }
    }
    out
}

/// The items, in the view's stated order: `(uri, line, pos, code, msg)`, the
/// code compared numerically (`E{code:04}` is a display spelling — sorting on
/// it would collate `E9xxx` after `E1xxx` within one namespace digit).
///
/// The order is a function of the world, never of the store's insertion
/// sequence: the store appends in parse order over tables that include hash
/// maps, so without this sort two reads of one world could serialize alike
/// only by luck.
pub fn diag_items(diags: &[Diagnostic]) -> Vec<Value> {
    let deduped = dedup_diags(diags.to_vec());
    let mut keyed: Vec<(&Diagnostic, DiagItem)> =
        deduped.iter().map(|d| (d, diag_item(d))).collect();
    keyed.sort_by(|(a, _), (b, _)| {
        (
            a.loc.uri.as_str(),
            a.loc.row,
            a.loc.pos,
            a.code,
            a.msg.as_str(),
        )
            .cmp(&(
                b.loc.uri.as_str(),
                b.loc.row,
                b.loc.pos,
                b.code,
                b.msg.as_str(),
            ))
    });
    keyed
        .into_iter()
        .map(|(_, item)| serde_json::to_value(item).unwrap_or(Value::Null))
        .collect()
}

/// Per-level counts, computed from the **sorted** items so the header and the
/// rows cannot disagree. Every word is printed even when zero — an absent
/// line reads as "not implemented" rather than "none".
pub fn diag_counts(items: &[Value]) -> Value {
    let n_of = |level: &str| {
        items
            .iter()
            .filter(|i| i["level"].as_str() == Some(level))
            .count()
    };
    json!({
        "errors": n_of("error"),
        "warnings": n_of("warning"),
        "advisories": n_of("advisory"),
        "infos": n_of("info"),
    })
}

/// Assemble the projection. Goes through [`StageView::with_view`] — this view
/// publishes its own vocabulary and its own count words, not a pipeline
/// segment's.
pub fn diagnostics_view(top: &str, diags: &[Diagnostic]) -> StageView {
    let items = diag_items(diags);
    let counts = diag_counts(&items);
    StageView::with_view(DIAGNOSTICS_VIEW, top, items, counts)
}

/// The text face, rendered from the **same** items the envelope carries:
/// header, the level counts, then one row per item — `level`, `code`, `msg`,
/// `loc`. A missing value prints as `-`.
pub fn render_diag_text(view: &StageView) -> String {
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
    let widest = |f: fn(&Value) -> String| -> usize {
        view.items.iter().map(|i| f(i).len()).max().unwrap_or(0)
    };
    let level_col = widest(|i| i["level"].as_str().unwrap_or("-").to_string()).max(5);
    let code_col = widest(|i| i["code"].as_str().unwrap_or("-").to_string()).max(4);
    lines.push(format!(
        "{:<level_col$}  {:<code_col$}  {}",
        "level", "code", "msg / loc"
    ));
    for it in &view.items {
        let msg = it["msg"].as_str().unwrap_or("-");
        let loc = super::loc_cell(&it["loc"]);
        lines.push(format!(
            "{:<level_col$}  {:<code_col$}  {}  {}",
            it["level"].as_str().unwrap_or("-"),
            it["code"].as_str().unwrap_or("-"),
            msg,
            loc,
        ));
    }
    lines.join("\n")
}
