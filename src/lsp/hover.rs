// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Hover information — provide type/definition info for a symbol.
//!
//! Delegates to `lsp::goto_def::resolve` for definition lookup and
//! `lsp::sem::try_lookup_sem` for token-level information.

use serde_json::Value;

/// Get hover information for a symbol name in a file.
/// Returns definition info if found, plus token-level details if available.
pub fn hover(name: &str, uri: &str) -> Option<Value> {
    // First try: resolve as a definition (module, component, interface, enum)
    if let Some(def) = super::goto_def::resolve(name) {
        return Some(def);
    }

    // Second try: look up in semantic tokens
    let candidates = &[crate::McURI::from(uri)];
    if let Some(sem) = super::sem::try_lookup_sem(candidates) {
        // Check if any symbol matches this name
        if let Some(symbols) = sem.get("symbols") {
            if let Some(info) = symbols.get(name) {
                return Some(info.clone());
            }
        }
    }

    None
}
