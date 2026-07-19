// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Document and workspace symbols.
//!
//! Extracted from `rpc/handlers/lsp.rs` (handle_project_symbols).

use crate::query::iterators::{
    mcb_iter_components_with_span, mcb_iter_enum_values, mcb_iter_enums_with_span,
    mcb_iter_interfaces_with_span, mcb_iter_modules_with_span,
};
use serde_json::Value;

/// Collect project-wide symbols (components, interfaces, enums, modules, enum_values).
pub fn project_symbols() -> Vec<Value> {
    let mut symbols = Vec::new();

    for (name, uri, span) in mcb_iter_components_with_span() {
        symbols.push(
            serde_json::json!({ "name": name, "kind": "component", "uri": uri, "span": span }),
        );
    }
    for (name, uri, span) in mcb_iter_interfaces_with_span() {
        symbols.push(
            serde_json::json!({ "name": name, "kind": "interface", "uri": uri, "span": span }),
        );
    }
    for (name, uri, span) in mcb_iter_enums_with_span() {
        symbols.push(serde_json::json!({ "name": name, "kind": "enum", "uri": uri, "span": span }));
    }
    for (name, uri, span) in mcb_iter_modules_with_span() {
        symbols
            .push(serde_json::json!({ "name": name, "kind": "module", "uri": uri, "span": span }));
    }
    for (name, enum_name, uri, span) in mcb_iter_enum_values() {
        symbols.push(serde_json::json!({ "name": name, "kind": "enum_value", "enum": enum_name, "uri": uri, "span": span }));
    }

    symbols
}
