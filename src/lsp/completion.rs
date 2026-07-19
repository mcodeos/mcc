// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Code completion — provide completion candidates at a cursor position.
//!
//! To be extracted from `rpc/handlers/`.

use serde_json::Value;

/// Get completion candidates at the given position in a file.
pub fn complete(_uri: &str, _line: u32, _column: u32) -> Vec<Value> {
    // TODO: Implement completion by:
    // 1. Parse the file context around the cursor
    // 2. Determine completion scope (instance body, param list, etc.)
    // 3. Use unified_lookup_all to find visible symbols
    // 4. Filter by prefix and rank by proximity
    vec![]
}
