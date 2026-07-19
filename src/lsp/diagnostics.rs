// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Diagnostic collection and formatting for LSP.
//!
//! Extracted from `rpc/handlers/lsp.rs` (handle_diagnostics).

use crate::builder::diagnostic::Diagnostic;
use crate::McURI;
use serde_json::Value;

/// Collect diagnostics for a file URI.
pub fn collect(uri: &McURI) -> Vec<Value> {
    let diagnostics = crate::mcc_diagnose(uri);
    diagnostics.iter().map(|d| diagnostic_to_json(d)).collect()
}

/// Format a single diagnostic as a JSON value suitable for LSP.
pub fn diagnostic_to_json(d: &Diagnostic) -> Value {
    serde_json::json!({
        "code": d.code,
        "level": format!("{:?}", d.level).to_lowercase(),
        "message": d.msg,
        "location": {
            "pos": d.loc.pos,
            "len": d.loc.len,
            "line": d.loc.row,
            "column": d.loc.col,
        }
    })
}
