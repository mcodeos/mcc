// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `core-erc` projection — the extension-tool snapshot, read as data.
//!
//! The fourth carried canonical `view-name` word (`schema/projection.cddl`
//! §2.3): the check-model the ERC extension tool consumes, demoted from a
//! schema of its own to one projection of the family
//! (`projection-schema-design.md` §2.3). One item carries the whole top's
//! snapshot — the same one-root-item spelling the `project-model` tree uses —
//! with its three pieces spelled verbatim:
//!
//! * `nets` is the §2.2 `net` group, built by **the netlist view's own
//!   builder** ([`netlistview::net_items`]) — the snapshot can no more spell
//!   the connectivity a second way than the export face can;
//! * `findings` is the flat electrical net checks in the result form the
//!   build envelope carries (`pass2.net_checks`), a parallel array that
//!   names its object verbatim and never joins it on the wire — a check may
//!   name a pin path or an instance path rather than a net, so joining is
//!   the consumer's move, not the producer's;
//! * `loc` — the object→source-site side table — is reserved: the flat
//!   table carries no source site for a net today, and every finding
//!   carries its own site inline, which is the only consultation the v1
//!   items need. The member stays in the group because the contract
//!   reserves it; the fixture exercises it by constructing the item
//!   directly.
//!
//! The findings are the raw aggregate, before any override-store
//! suppression: the snapshot is the object of study, not a display layer.
//! A readout, not a verdict (law C): a findings array full of errors never
//! flips an exit code.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::diagview::{DiagLoc, Level};
use super::netlistview;
use super::StageView;
use crate::db::diagnostic::diagnostic::Location;
use crate::semantic::validation::nets::NetCheckResult;

/// The canonical word this face publishes (CDDL `view-name`).
pub const CORE_ERC_VIEW: &str = "core-erc";

/// CDDL `core-erc-finding` — one flat electrical net check, member names
/// verbatim from `schema/projection.cddl`. `fix_hint` is reserved (zero
/// producer today, exactly as on `diag`); the other six members always
/// serialize.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// The code as every human face spells it: `E{:04}` (the same rendering
    /// `diag.code` uses).
    pub code: String,
    /// The check's own severity word, mapped onto the contract's level
    /// vocabulary in [`level_of`].
    pub level: Level,
    /// The check's label, the `rules.rs` registry's own spelling.
    pub check: String,
    pub msg: String,
    /// The object the check named — a net, a pin path or an instance path,
    /// verbatim from the check.
    pub net: String,
    pub loc: DiagLoc,
    /// Mechanically executable action (M4 5W). Reserved: no producer yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
}

/// The check vocabulary is closed at the `rules.rs` catalog (`"error"` /
/// `"warning"` / `"info"`); a fourth word would be a catalog change and will
/// surface in the golden before it can pass unnoticed. `info` is the honest
/// reading of "not an error, not a warning".
fn level_of(severity: &str) -> Level {
    match severity {
        "error" => Level::Error,
        "warning" => Level::Warning,
        _ => Level::Info,
    }
}

/// The code spelling, in exactly one place per module (the same format line
/// `diagview` carries).
fn code_string(code: u32) -> String {
    format!("E{code:04}")
}

/// Adapt one flat net check to the contract finding. The site's line number
/// is derived from the check's byte offset through the same
/// [`Location::new`] the diagnostics use — one derivation, deterministic in
/// the loaded world. A check with no site serializes the empty uri at line 1
/// rather than borrowing the caller's current file.
fn finding(r: &NetCheckResult) -> Finding {
    let loc = Location::new(r.uri.clone(), r.pos, 0);
    Finding {
        code: code_string(r.code),
        level: level_of(r.severity),
        check: r.check.to_string(),
        msg: r.message.clone(),
        net: r.net_name.clone(),
        loc: DiagLoc {
            uri: r.uri.clone(),
            line: loc.row,
            span: None,
        },
        fix_hint: None,
    }
}

/// CDDL `core-erc-snapshot` — the three §2.3 pieces. `loc` is `None` until a
/// producer for the net side table exists (see the module header).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreErcSnapshot {
    /// The top's copper islands, the `netlist` view's own items.
    pub nets: Vec<Value>,
    /// The flat net checks, ordered by [`finding_order`].
    pub findings: Vec<Finding>,
    /// The object→site side table. Reserved, no producer yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc: Option<std::collections::BTreeMap<String, DiagLoc>>,
}

/// Assemble the snapshot item: the netlist view's connectivity plus the flat
/// checks, sorted so two reads of one world serialize alike. The findings'
/// order is `(uri, code, message)`; codes are all `E{:04}`, so the string
/// order is the numeric one. The order is a function of the world, never of
/// the checks' run order.
pub fn core_erc_items(table: &crate::InstTable, results: &[NetCheckResult]) -> Vec<Value> {
    let mut findings: Vec<Finding> = results.iter().map(finding).collect();
    findings.sort_by_cached_key(|f| (f.loc.uri.clone(), f.code.clone(), f.msg.clone()));
    let snapshot = CoreErcSnapshot {
        nets: netlistview::net_items(table),
        findings,
        loc: None,
    };
    vec![serde_json::to_value(&snapshot).unwrap_or(Value::Null)]
}

/// Per-face counts: the islands, their members and the findings — computed
/// from the **items** so the header and the rows cannot disagree. Every word
/// is printed even when zero.
pub fn core_erc_counts(items: &[Value]) -> Value {
    let snap = items.first();
    let nets = snap
        .and_then(|s| s["nets"].as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let points: usize = snap
        .and_then(|s| s["nets"].as_array())
        .map(|a| {
            a.iter()
                .map(|n| n["points"].as_array().map(|p| p.len()).unwrap_or(0))
                .sum()
        })
        .unwrap_or(0);
    let findings = snap
        .and_then(|s| s["findings"].as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    serde_json::json!({
        "nets": nets,
        "points": points,
        "findings": findings,
    })
}

/// Assemble the projection. Goes through [`StageView::with_view`] — this view
/// publishes its own vocabulary and its own count words, not a pipeline
/// segment's.
pub fn core_erc_view(top: &str, table: &crate::InstTable, results: &[NetCheckResult]) -> StageView {
    let items = core_erc_items(table, results);
    let counts = core_erc_counts(&items);
    StageView::with_view(CORE_ERC_VIEW, top, items, counts)
}

/// The text face, rendered from the **same** items the envelope carries:
/// header, the counts, then one row per finding — `level`, `code`, `check`,
/// `net`, `msg`, `loc`. The connectivity half is the `netlist` face's to
/// show; the snapshot's headline is what the checks said.
pub fn render_core_erc_text(view: &StageView) -> String {
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
    let snap = view.items.first();
    let findings = snap
        .and_then(|s| s["findings"].as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    let widest = |f: fn(&Value) -> String| -> usize {
        findings.iter().map(|i| f(i).len()).max().unwrap_or(0)
    };
    let level_col = widest(|i| i["level"].as_str().unwrap_or("-").to_string()).max(5);
    let code_col = widest(|i| i["code"].as_str().unwrap_or("-").to_string()).max(4);
    let check_col = widest(|i| i["check"].as_str().unwrap_or("-").to_string()).max(5);
    let net_col = widest(|i| i["net"].as_str().unwrap_or("-").to_string()).max(3);
    lines.push(format!(
        "{:<level_col$}  {:<code_col$}  {:<check_col$}  {:<net_col$}  {}",
        "level", "code", "check", "net", "msg / loc"
    ));
    for it in findings {
        lines.push(format!(
            "{:<level_col$}  {:<code_col$}  {:<check_col$}  {:<net_col$}  {}  {}",
            it["level"].as_str().unwrap_or("-"),
            it["code"].as_str().unwrap_or("-"),
            it["check"].as_str().unwrap_or("-"),
            it["net"].as_str().unwrap_or("-"),
            it["msg"].as_str().unwrap_or("-"),
            super::loc_cell(&it["loc"]),
        ));
    }
    lines.join("\n")
}
