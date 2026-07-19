// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Hover information — provide type/definition info for a symbol at a position.
//!
//! To be extracted from `rpc/handlers/` and `db/infra/mc_code.rs`.

use serde_json::Value;

/// Get hover information for a symbol at the given position in a file.
/// Returns `None` if no symbol is found at the position.
pub fn hover(_uri: &str, _line: u32, _column: u32) -> Option<Value> {
    // TODO: Implement hover by:
    // 1. Look up the McCode for the URI
    // 2. Use the lapper to find the symbol at (line, column)
    // 3. Resolve the symbol definition via unified_lookup
    // 4. Format as hover response
    None
}
