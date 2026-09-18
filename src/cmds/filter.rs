// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Adapter over `mcc::query_api` for `mcc list --filter`.
//!
//! The real parser/evaluator lives in `src/query/search/dsl.rs` (lib root).
//! This module only:
//!   1. Re-exports the canonical `Query` type as `CompiledFilter` for
//!      back-compat with callers that already use that name.
//!   2. Compiles an optional expression string once.
//!   3. Validates the AST against the `name` key, the only key a name-only
//!      list can carry, so `list --filter kind=...` errors clearly.

use anyhow::Result;

/// Back-compat alias. The real type lives in `mcc::query_api::Query`.
pub type CompiledFilter = mcc::query_api::Query;

/// Parse a `--filter` expression. Returns the canonical AST.
pub fn compile(expr: &str) -> Result<CompiledFilter> {
    mcc::query_api::compile(expr)
}

/// Apply a filter to a plain `Vec<String>` (used by `mcc list --filter`).
/// Only the `name=` key is meaningful for name-only lists; other keys error.
pub fn apply_to_names(expr: Option<&str>, names: Vec<String>) -> Result<Vec<String>> {
    let Some(s) = expr else { return Ok(names) };
    let q = compile(s)?;
    mcc::query_api::validate_allowed_fields(&q, &["name"])?;
    Ok(names
        .into_iter()
        .filter(|n| {
            let item = serde_json::json!({ "name": n });
            mcc::query_api::matches_json_record(&q, &item)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_filter__apply_to_names_only_accepts_name_key() {
        let out = apply_to_names(Some("name=R*"), vec!["RES".into(), "CAP".into()]).unwrap();
        assert_eq!(out, vec!["RES".to_string()]);

        let err = apply_to_names(Some("kind=component"), vec![]).unwrap_err();
        assert!(format!("{}", err).contains("unknown key"));
    }
}
