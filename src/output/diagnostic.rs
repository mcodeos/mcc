// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc::Diagnostic` -> our [`Diagnostic`] adapter.
//!
//! The library's `Diagnostic` looks like:
//!
//! ```ignore
//! pub struct Diagnostic {
//!     pub code: u32,
//!     pub level: DiagnosticLevel,  // Error | Warning | Info | Hint
//!     pub loc: Location { uri, pos, len, row, col },
//!     pub msg: String,
//!     pub other: Vec<RelatedInformation>,
//! }
//! ```
//!
//! We convert it to a flat [`super::envelope::Diagnostic`], carrying the [`Phase`] tag and
//! mapping `other` → `related` (M6). Suggestions are not yet populated by the builder; the
//! field is reserved for future use.

use super::envelope::{DiagLocation, Diagnostic, DiagnosticRelated, Phase, Severity};
use mcc::{DiagnosticLevel, McDiagnostic};

/// Single mcc::Diagnostic -> our Diagnostic
pub fn from_mcc(d: &McDiagnostic, phase: Phase) -> Diagnostic {
    let related: Vec<DiagnosticRelated> = d
        .other
        .iter()
        .map(|ri| DiagnosticRelated {
            message: ri.get_formatted_message(),
            location: DiagLocation {
                file: ri.location.uri.clone(),
                line: ri.location.row,
                column: ri.location.col,
                end_line: None,
                end_column: None,
                pos: ri.location.pos,
                len: ri.location.len,
            },
        })
        .collect();

    Diagnostic {
        phase,
        severity: severity_from(d.level),
        code: d.code,
        message: d.msg.clone(),
        location: Some(DiagLocation {
            file: d.loc.uri.clone(),
            line: d.loc.row,
            column: d.loc.col,
            end_line: None,
            end_column: None,
            pos: d.loc.pos,
            len: d.loc.len,
        }),
        suggestions: vec![],
        related,
    }
}

/// Batch convert + tag phase. Use snapshot mode (before/after) to pick out a vec slice.
///
/// ```ignore
/// let before = mcc::mcc_diagnose_all().len();
/// run_pass2();
/// let all = mcc::mcc_diagnose_all();
/// let pass2_diags = batch_from_mcc(&all[before..], Phase::Pass2);
/// ```
pub fn batch_from_mcc(slice: &[McDiagnostic], phase: Phase) -> Vec<Diagnostic> {
    slice.iter().map(|d| from_mcc(d, phase)).collect()
}

/// Counter: count errors / warnings for a given set of Diagnostics.
pub fn count_severity(diags: &[Diagnostic]) -> (usize, usize) {
    let mut errs = 0usize;
    let mut warns = 0usize;
    for d in diags {
        match d.severity {
            Severity::Error => errs += 1,
            Severity::Warning => warns += 1,
            _ => {}
        }
    }
    (errs, warns)
}

fn severity_from(l: DiagnosticLevel) -> Severity {
    match l {
        DiagnosticLevel::Error => Severity::Error,
        DiagnosticLevel::Warning => Severity::Warning,
        DiagnosticLevel::Info => Severity::Info,
        DiagnosticLevel::Hint => Severity::Hint,
    }
}

// ============================================================================
// PhaseTracker - used to split diagnostics between pass1/pass2
// ============================================================================

/// Tool for tagging diagnostics with phase between pass1 and pass2.
///
/// Usage:
///
/// ```ignore
/// let mut tracker = PhaseTracker::new();
/// // ... pass1 runs once ...
/// let pass1_diags = tracker.collect(Phase::Pass1);
/// // ... pass2 runs once ...
/// let pass2_diags = tracker.collect(Phase::Pass2);
/// ```
///
/// Internally maintains a cursor; each collect slices `[cursor..end]`, converts, and advances the cursor.
pub struct PhaseTracker {
    cursor: usize,
}

impl PhaseTracker {
    pub fn new() -> Self {
        Self {
            cursor: mcc::mcc_diagnose_all().len(),
        }
    }

    /// Capture the new diagnostics added since the last [`collect`](Self::collect) (or [`new`](Self::new)),
    /// tag them with `phase` and return; cursor advances.
    pub fn collect(&mut self, phase: Phase) -> Vec<Diagnostic> {
        let all = mcc::mcc_diagnose_all();
        let new_slice = if self.cursor <= all.len() {
            &all[self.cursor..]
        } else {
            &[][..]
        };
        let result = batch_from_mcc(new_slice, phase);
        self.cursor = all.len();
        result
    }

    /// Reset the cursor to the latest position (returns no diagnostics). Suitable for discarding intermediate phase diagnostics.
    pub fn skip(&mut self) {
        self.cursor = mcc::mcc_diagnose_all().len();
    }
}

impl Default for PhaseTracker {
    fn default() -> Self {
        Self::new()
    }
}
