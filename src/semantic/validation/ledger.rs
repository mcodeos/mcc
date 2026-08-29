// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Failure ledger — cross-pass unified record of "non-clean" parses.
//!
//! Resolve-gate architecture §1 (mcd/doc/resolve-gate-design.md): a compiler
//! that silently resolves a miss (a bare identifier → floating wire, a literal
//! net point → quarantine, a structured miss → fallback bus) produces zero
//! errors while the netlist is broken. The ledger records every such moment —
//! observation only, never changing semantics — and reports a kind×form
//! summary plus (under `--ledger`) the per-row detail, so silent breakage is
//! attributable to a concrete site.
//!
//! Kinds (§1.3):
//!   - [`LedgerKind::UnresolvedRef`]  — structured miss → will error (Phase 1 gate)
//!   - [`LedgerKind::Wire`]           — bare miss → floating net label (survivors)
//!   - [`LedgerKind::Deferred`]       — name deferred to component-finish recheck
//!   - [`LedgerKind::ResolvedMany`]   — a name that resolved ambiguously (2+ candidates)
//!   - [`LedgerKind::Phantom`]        — quarantined literal / phantom instance (pass2)
//!   - [`LedgerKind::Fallback`]       — shape mismatch / member fall-through
//!
//! The store is a process-global following the `WORKSPACE` LazyLock pattern
//! (db/cmie/tables.rs). Recording is a pure `push` on a `Mutex<Vec>` — no
//! control-flow change, so instrumentation cannot alter resolution.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

// ============================================================================
// Kinds & actions
// ============================================================================

/// What kind of miss was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LedgerKind {
    /// Structured miss (dotted / array / group that failed to resolve): the
    /// resolve-gate will turn this into an error (Phase 1).
    UnresolvedRef,
    /// Bare single-segment miss resolved to a floating net label.
    Wire,
    /// Resolution was postponed to component-finish (late-declared names).
    Deferred,
    /// A name matched more than one candidate at finish.
    ResolvedMany,
    /// Pass2 quarantine / phantom instance (`@_phantom_…`).
    Phantom,
    /// Shape mismatch / member fall-through (silent fallback shape).
    Fallback,
}

impl LedgerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnresolvedRef => "unresolved_ref",
            Self::Wire => "wire",
            Self::Deferred => "deferred",
            Self::ResolvedMany => "resolved_many",
            Self::Phantom => "phantom",
            Self::Fallback => "fallback",
        }
    }

    /// Stable ordered list of all kinds — the `by_kind_form` map always emits
    /// every key (complete form set), so consumers can rely on the shape.
    pub const ALL: [LedgerKind; 6] = [
        Self::UnresolvedRef,
        Self::Wire,
        Self::Deferred,
        Self::ResolvedMany,
        Self::Phantom,
        Self::Fallback,
    ];
}

/// The action the compiler actually took at this site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerAction {
    Error,
    Warning,
    /// No diagnostic was emitted — the miss was silently absorbed.
    Silent,
}

impl LedgerAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Silent => "silent",
        }
    }
}

// ============================================================================
// Modes
// ============================================================================

/// Detail scope for [`build_report`] (resolve-gate-design.md §7.1-4): the
/// daemon/CLI envelope always carries the summary; per-row detail is opt-in.
/// `Deferred` / `ResolvedMany` are **successful** resolutions (resolved to a
/// named sub-instance / per-member array), so their rows are audit-only and
/// excluded from the default detail mode — only `--ledger=audit` lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LedgerMode {
    /// Summary counts only (always emitted).
    #[default]
    Summary,
    /// `--ledger`: per-row detail for every kind except Deferred/ResolvedMany.
    Detail,
    /// `--ledger=audit`: per-row detail for every kind, including the
    /// Deferred/ResolvedMany audit rows.
    Audit,
}

impl LedgerMode {
    /// Map the CLI flag value (`--ledger` → `"detail"` default, `--ledger=audit`
    /// → `"audit"`, absent → `None`) onto a mode.
    pub fn from_flag(flag: Option<&str>) -> Self {
        match flag {
            Some("audit") => Self::Audit,
            Some(_) => Self::Detail,
            None => Self::Summary,
        }
    }
}

// ============================================================================
// Entry
// ============================================================================

/// One recorded non-clean moment. `uri`/`pos`/`len` are best-effort: sites deep
/// in instantiation (e.g. `NetPoint` quarantine) have no source node, so they
/// record `uri: None` and only contribute counts + form to the report.
#[derive(Debug, Clone)]
pub struct LedgerEntry {
    pub kind: LedgerKind,
    /// The unresolved text as written (label name, literal path, type name).
    pub form: String,
    /// Where it occurred: component / module / func / phase-stage name.
    pub site: String,
    pub action: LedgerAction,
    pub uri: Option<String>,
    pub pos: u32,
    pub len: u32,
    /// Wire-only: how many net endpoints reference the name (denoise — a name
    /// referenced twice or more is a shared floating net, not a single typo).
    pub refs: Option<u32>,
}

impl LedgerEntry {
    pub fn new(kind: LedgerKind, form: impl Into<String>, site: impl Into<String>) -> Self {
        Self {
            kind,
            form: form.into(),
            site: site.into(),
            action: LedgerAction::Silent,
            uri: None,
            pos: 0,
            len: 0,
            refs: None,
        }
    }

    pub fn with_action(mut self, action: LedgerAction) -> Self {
        self.action = action;
        self
    }

    pub fn with_uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    pub fn with_span(mut self, pos: u32, len: u32) -> Self {
        self.pos = pos;
        self.len = len;
        self
    }

    pub fn with_refs(mut self, refs: u32) -> Self {
        self.refs = Some(refs);
        self
    }
}

// ============================================================================
// Global store
// ============================================================================

/// Process-global ledger, following the `WORKSPACE` LazyLock pattern
/// (db/cmie/tables.rs). Cleared at the start of each build/check entry point.
pub(crate) static LEDGER: LazyLock<Mutex<Ledger>> = LazyLock::new(|| Mutex::new(Ledger::default()));

/// Identity of a recorded miss. Same (kind, form, site, uri, pos, len) = the
/// same source-level failure; a whole-workspace re-parse (e.g. the synthetic
/// `VIRT_<T>` module install in Pass 2's virtual build re-runs
/// `mcb_parse_all_modules`) must not double-book it. `clear()` resets the set
/// per request, so dedup is scoped to one build — never across requests.
type LedgerKey = (LedgerKind, String, String, Option<String>, u32, u32);

#[derive(Default)]
pub struct Ledger {
    entries: Vec<LedgerEntry>,
    /// Seen keys for this request; keeps `record` idempotent for a re-parse.
    seen: std::collections::HashSet<LedgerKey>,
    /// Number of Deferred entries that resolved at component-finish. Not yet
    /// exercised by any recording point; kept for the stable protocol shape.
    resolved_late: usize,
}

impl Ledger {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.seen.clear();
        self.resolved_late = 0;
    }

    pub fn record(&mut self, entry: LedgerEntry) {
        let key = (
            entry.kind,
            entry.form.clone(),
            entry.site.clone(),
            entry.uri.clone(),
            entry.pos,
            entry.len,
        );
        if !self.seen.insert(key) {
            return;
        }
        self.entries.push(entry);
    }

    /// A Deferred/UnresolvedRef candidate that the component-finish recheck
    /// resolved to a late-declared instance (resolve-gate-design.md §1.3): the
    /// gate does not error and the candidate is counted as late-resolved. The
    /// parse-time row is kept for audit value; `survived` never counts it.
    pub fn mark_resolved_late(&mut self) {
        self.resolved_late += 1;
    }

    /// Snapshot into a serializable report. Summary counts (kind×form) are
    /// always produced; the per-row list is gated by `mode` (resolve-gate
    /// §7.1-4). `survived` (§7.1-3) counts the entries that are genuine
    /// problems at end of compile: Phantom/Fallback/UnresolvedRef always, and
    /// a Wire only when its `refs == 1` (exactly-once bare reference — the
    /// E3136 twin); Deferred/ResolvedMany never survive (successful resolution).
    pub fn build_report(&self, mode: LedgerMode) -> LedgerReport {
        let mut by_kind_form: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for kind in LedgerKind::ALL {
            by_kind_form.insert(kind.as_str().to_string(), BTreeMap::new());
        }
        for e in &self.entries {
            let forms = by_kind_form.entry(e.kind.as_str().to_string()).or_default();
            *forms.entry(e.form.clone()).or_insert(0) += 1;
        }

        let detail_rows = match mode {
            LedgerMode::Summary => Vec::new(),
            // Deferred/ResolvedMany are successful resolutions — audit-only,
            // excluded from the default detail mode.
            LedgerMode::Detail => self
                .entries
                .iter()
                .filter(|e| !matches!(e.kind, LedgerKind::Deferred | LedgerKind::ResolvedMany))
                .map(|e| e.to_row())
                .collect(),
            LedgerMode::Audit => self.entries.iter().map(|e| e.to_row()).collect(),
        };

        let survived = self.entries.iter().filter(|e| Self::survives(e)).count();

        // Deferred entries that later resolved are removed from the ledger (see
        // `resolve`); the count is carried here so the protocol stays stable.
        let resolved_late = self.resolved_late;

        LedgerReport {
            total: self.entries.len(),
            by_kind_form,
            survived,
            resolved_late,
            detail: detail_rows,
        }
    }

    /// Whether a recorded miss is still a genuine problem at end of compile
    /// (§7.1-3): a floating candidate, quarantined phantom, or silent fallback —
    /// never a successfully-resolved Deferred/ResolvedMany or a shared rail
    /// (Wire with `refs >= 2`).
    fn survives(e: &LedgerEntry) -> bool {
        match e.kind {
            LedgerKind::Deferred | LedgerKind::ResolvedMany => false,
            LedgerKind::Wire => e.refs == Some(1),
            // UnresolvedRef (Phase-1-gated), Phantom (quarantined), Fallback
            // (silent placeholder) all represent real breakage.
            _ => true,
        }
    }
}

impl LedgerEntry {
    fn to_row(&self) -> LedgerDetailRow {
        let uri = self.uri.as_deref();
        let (file, line, column) = match uri {
            Some(u) => {
                // Reuse the diagnostic location resolver: row/col come free from
                // the workspace source map.
                let loc = crate::db::diagnostic::diagnostic::Location::new(
                    crate::McURI::from(u.to_string()),
                    self.pos,
                    self.len,
                );
                (Some(u.to_string()), Some(loc.row), Some(loc.col))
            }
            None => (None, None, None),
        };
        LedgerDetailRow {
            kind: self.kind.as_str().to_string(),
            form: self.form.clone(),
            site: self.site.clone(),
            action: self.action.as_str().to_string(),
            refs: self.refs,
            file,
            line,
            column,
            pos: if self.uri.is_some() {
                Some(self.pos)
            } else {
                None
            },
            len: if self.uri.is_some() {
                Some(self.len)
            } else {
                None
            },
        }
    }
}

// ============================================================================
// Report
// ============================================================================

/// Serializable report shape (mcd/doc/resolve-gate-design.md §7.1-2):
/// `{total, by_kind_form, survived, resolved_late, detail}`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LedgerReport {
    pub total: usize,
    /// kind → form → count. All six kinds are always present (complete form
    /// set); empty inner maps mean "no entries of this kind".
    pub by_kind_form: BTreeMap<String, BTreeMap<String, usize>>,
    /// Entries that are still genuine problems at end of compile (§7.1-3):
    /// the "true problem" headline.
    pub survived: usize,
    pub resolved_late: usize,
    /// Per-row detail, only under `--ledger`. Rows that had no source node
    /// omit `file`/`line`/`column`/`pos`/`len`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub detail: Vec<LedgerDetailRow>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LedgerDetailRow {
    pub kind: String,
    pub form: String,
    pub site: String,
    pub action: String,
    /// Wire-only reference count (denoise); omitted for other kinds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<u32>,
}

// ============================================================================
// Record / clear / report entry points (called from every recording site)
// ============================================================================

/// Append an entry. Pure push — must never change control flow.
pub fn record(entry: LedgerEntry) {
    LEDGER.lock().unwrap().record(entry);
}

/// Drop all entries. Call once at the entry of each build/check path so a
/// long-lived server (RPC) does not accumulate stale rows across requests.
pub fn clear() {
    LEDGER.lock().unwrap().clear();
}

/// Count one Deferred/UnresolvedRef candidate that the component-finish
/// recheck resolved to a late-declared instance (no error emitted).
pub fn mark_resolved_late() {
    LEDGER.lock().unwrap().mark_resolved_late();
}

/// Snapshot the current ledger into a report.
pub fn build_report(mode: LedgerMode) -> LedgerReport {
    LEDGER.lock().unwrap().build_report(mode)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_has_complete_kind_form_map() {
        // Use a local instance — the global LEDGER is shared by parallel tests.
        let mut ledger = Ledger::default();
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "PWR", "COMP")
                .with_action(LedgerAction::Warning)
                .with_refs(1),
        );
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "PWR", "COMP")
                .with_action(LedgerAction::Warning)
                .with_refs(1),
        );
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "VCC", "COMP")
                .with_action(LedgerAction::Warning)
                .with_refs(1),
        );
        ledger.record(
            LedgerEntry::new(LedgerKind::Phantom, "clk", "net-point")
                .with_action(LedgerAction::Silent),
        );

        let r = ledger.build_report(LedgerMode::Summary);
        assert_eq!(r.total, 3);
        // All six kinds present, even empty ones.
        assert_eq!(r.by_kind_form.len(), LedgerKind::ALL.len());
        // Identical re-record (a whole-workspace re-parse) collapses to one row.
        assert_eq!(r.by_kind_form["wire"]["PWR"], 1);
        assert_eq!(r.by_kind_form["wire"]["VCC"], 1);
        assert_eq!(r.by_kind_form["phantom"]["clk"], 1);
        assert!(r.by_kind_form["unresolved_ref"].is_empty());
        // Detail suppressed when detail=false.
        assert!(r.detail.is_empty());
        assert_eq!(r.resolved_late, 0);
    }

    #[test]
    fn detail_rows_carry_span_and_refs() {
        let mut ledger = Ledger::default();
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "RAIL", "BOARD")
                .with_action(LedgerAction::Warning)
                .with_uri("proj/main.mc")
                .with_span(100, 4)
                .with_refs(2),
        );
        let r = ledger.build_report(LedgerMode::Detail);
        assert_eq!(r.detail.len(), 1);
        let row = &r.detail[0];
        assert_eq!(row.kind, "wire");
        assert_eq!(row.form, "RAIL");
        assert_eq!(row.action, "warning");
        assert_eq!(row.file.as_deref(), Some("proj/main.mc"));
        assert_eq!(row.refs, Some(2));
        assert!(row.line.is_some());
        assert_eq!(row.pos, Some(100));
        assert_eq!(row.len, Some(4));
    }

    #[test]
    fn uri_less_entries_omit_location_fields() {
        let mut ledger = Ledger::default();
        ledger.record(LedgerEntry::new(LedgerKind::Phantom, "x[1]", "net-point"));
        let r = ledger.build_report(LedgerMode::Detail);
        let row = &r.detail[0];
        assert!(row.file.is_none());
        assert!(row.line.is_none());
        assert!(row.pos.is_none());
    }

    #[test]
    fn record_dedupes_identical_span_but_keeps_distinct_sites() {
        // Same failure re-fired by a whole-workspace re-parse collapses to one
        // row (virtual-build VIRT_<T> install re-runs mcb_parse_all_modules).
        let mut ledger = Ledger::default();
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "pwr", "T")
                .with_uri("/tmp/ledger_wire.mc")
                .with_span(73, 3)
                .with_refs(1),
        );
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "pwr", "T")
                .with_uri("/tmp/ledger_wire.mc")
                .with_span(73, 3)
                .with_refs(1),
        );
        // Same form/span at a different site is a genuinely different failure.
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "pwr", "U1")
                .with_uri("/tmp/ledger_wire.mc")
                .with_span(73, 3)
                .with_refs(1),
        );
        let r = ledger.build_report(LedgerMode::Detail);
        assert_eq!(r.total, 2);
        assert_eq!(r.detail.len(), 2);
    }

    #[test]
    fn audit_mode_includes_deferred_and_resolved_many_rows() {
        // §7.1-4: Deferred/ResolvedMany are successful resolutions — summary
        // always counts them, `--ledger` detail excludes their rows, and only
        // `--ledger=audit` lists them.
        let mut ledger = Ledger::default();
        ledger.record(LedgerEntry::new(LedgerKind::Deferred, "y", "T"));
        ledger.record(LedgerEntry::new(LedgerKind::ResolvedMany, "U1.y", "T"));
        ledger.record(LedgerEntry::new(LedgerKind::Fallback, "X.A", "T"));
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "PWR", "T").with_refs(2), // shared rail — does not survive
        );
        ledger.record(
            LedgerEntry::new(LedgerKind::Wire, "FLO", "T").with_refs(1), // exactly-once — survives (E3136 twin)
        );

        // Summary: counts every kind, no detail rows.
        let s = ledger.build_report(LedgerMode::Summary);
        assert_eq!(s.total, 5);
        assert!(s.detail.is_empty());
        assert_eq!(s.by_kind_form["deferred"]["y"], 1);
        assert_eq!(s.by_kind_form["resolved_many"]["U1.y"], 1);

        // Detail (`--ledger`): Deferred/ResolvedMany rows excluded.
        let d = ledger.build_report(LedgerMode::Detail);
        let kinds: Vec<&str> = d.detail.iter().map(|r| r.kind.as_str()).collect();
        assert!(kinds.contains(&"fallback"));
        assert!(kinds.contains(&"wire"));
        assert!(!kinds.contains(&"deferred"));
        assert!(!kinds.contains(&"resolved_many"));
        assert_eq!(d.detail.len(), 3);

        // Audit (`--ledger=audit`): all rows listed.
        let a = ledger.build_report(LedgerMode::Audit);
        assert_eq!(a.detail.len(), 5);
        assert!(a.detail.iter().any(|r| r.kind == "deferred"));
        assert!(a.detail.iter().any(|r| r.kind == "resolved_many"));

        // Survived (§7.1-3): fallback + exactly-once wire only. Deferred /
        // ResolvedMany and the shared-rail wire (refs=2) never survive.
        for r in [&s, &d, &a] {
            assert_eq!(r.survived, 2, "survived must be mode-independent");
        }
    }

    #[test]
    fn from_flag_maps_cli_values() {
        assert_eq!(LedgerMode::from_flag(None), LedgerMode::Summary);
        assert_eq!(LedgerMode::from_flag(Some("detail")), LedgerMode::Detail);
        assert_eq!(LedgerMode::from_flag(Some("")), LedgerMode::Detail);
        assert_eq!(LedgerMode::from_flag(Some("audit")), LedgerMode::Audit);
        assert_eq!(LedgerMode::from_flag(Some("unknown")), LedgerMode::Detail);
    }
}
