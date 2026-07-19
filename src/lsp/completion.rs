// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Code completion — provide completion candidates at a cursor position.
//!
//! Uses `unified_lookup_all` to find visible symbols in the current scope.

use crate::{McURI, ScopeFilter, ScopePath};
use serde_json::{json, Value};

/// Get completion candidates for a prefix at a given file location.
/// Filters visible symbols by the optional prefix string.
pub fn complete(uri: &str, prefix: Option<&str>, scope: Option<&str>) -> Vec<Value> {
    let mc_uri = McURI::from(uri);
    let scope_path = if let Some(s) = scope {
        crate::builder::mc_code::McCode::scope_path_from_scope_str_public(&mc_uri, s)
    } else {
        ScopePath::file_level(&mc_uri)
    };

    let mut filter = ScopeFilter::new();
    if let Some(pref) = prefix {
        filter = filter.with_prefix(pref);
    }
    filter = filter.with_limit(50);

    let results = crate::unified_lookup_all(&scope_path, &filter);
    results
        .iter()
        .map(|r| {
            json!({
                "label": r.name,
                "kind": format!("{:?}", r.kind).to_lowercase(),
                "detail": r.container.as_ref().map(|c| format!("{:?} {}", c.kind, c.name)),
                "uri": r.uri,
                "span": { "start": r.span.start, "end": r.span.end },
            })
        })
        .collect()
}
