// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Find references — locate all usages of a symbol.
//!
//! Extracted from `rpc/handlers/defs.rs` (handle_refs).

use serde_json::{json, Value};

/// Find all references to a named symbol across the workspace.
pub fn find(name: &str) -> Vec<Value> {
    let refs = crate::mcb_get_refs(name);
    refs.iter()
        .map(|(uri, scope, span)| {
            json!({
                "uri": uri,
                "scope": scope,
                "pos": span.start,
                "end": span.end,
            })
        })
        .collect()
}
