// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Diagnostic collection and formatting for LSP and AI consumers.
//!
//! ## Phase 8.2 — unified serialization
//!
//! Both `handle_diagnostics` (LSP) and `handle_check` (AI) route through
//! these functions. Handlers are **forbidden** from hand-assembling
//! diagnostic JSON — that way the fields stay consistent across consumers.

use crate::db::diagnostic::diagnostic::{Diagnostic, DiagnosticLevel, Location};
use crate::McURI;
use serde_json::{json, Value};

/// Collect diagnostics for a file URI (LSP format).
pub fn collect(uri: &McURI) -> Vec<Value> {
    let diagnostics = crate::mcc_diagnose(uri);
    diagnostics.iter().map(|d| diagnostic_to_json(d)).collect()
}

/// Collect all diagnostics across all files (AI check format, with
/// `end_line`/`end_column`/`suggestions`/`related`).
pub fn collect_all_full() -> Vec<Value> {
    crate::mcc_diagnose_all()
        .iter()
        .map(|d| diagnostic_to_json_full(d))
        .collect()
}

// ── Single diagnostic formatters ──

/// Format a diagnostic for LSP (compact format: code, level, message, location).
pub fn diagnostic_to_json(d: &Diagnostic) -> Value {
    json!({
        "code": d.code,
        "level": level_str(&d.level),
        "message": d.msg,
        "location": {
            "pos": d.loc.pos,
            "len": d.loc.len,
            "line": d.loc.row,
            "column": d.loc.col,
        }
    })
}

/// Format a diagnostic for AI consumers (full format with
/// `end_line`, `end_column`, `suggestions`, `related`).
pub fn diagnostic_to_json_full(d: &Diagnostic) -> Value {
    let mut v = json!({
        "code": d.code,
        "severity": level_str(&d.level),
        "message": d.msg,
        "location": {
            "file": d.loc.uri,
            "line": d.loc.row,
            "column": d.loc.col,
            "pos": d.loc.pos,
            "len": d.loc.len,
        },
        "end_line": d.loc.row,
        "end_column": d.loc.col,
        "suggestions": [],
        "related": [],
    });

    // Fill suggestions from RelatedInformation
    if !d.other.is_empty() {
        let suggestions: Vec<Value> = d
            .other
            .iter()
            .map(|ri| {
                json!({
                    "message": ri.message_template,
                    "location": location_to_json(&ri.location),
                })
            })
            .collect();
        v["suggestions"] = json!(suggestions);
        v["related"] = json!(suggestions);
    }

    v
}

// ── Helpers ──

fn level_str(level: &DiagnosticLevel) -> &'static str {
    match level {
        DiagnosticLevel::Error => "error",
        DiagnosticLevel::Warning => "warning",
        DiagnosticLevel::Info => "info",
        DiagnosticLevel::Hint => "hint",
    }
}

fn location_to_json(loc: &Location) -> Value {
    json!({
        "file": loc.uri,
        "line": loc.row,
        "column": loc.col,
        "pos": loc.pos,
        "len": loc.len,
    })
}
