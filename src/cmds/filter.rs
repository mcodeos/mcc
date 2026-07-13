// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Adapter over `mcc::query_api` for `mcc extract` and `mcc show --list`.
//!
//! The real parser/evaluator lives in `src/query_api.rs` (lib root). This
//! module only:
//!   1. Re-exports the canonical `Query` type as `CompiledFilter` for
//!      back-compat with callers that already use that name.
//!   2. Compiles an optional expression string once.
//!   3. Validates the AST against an `allowed_keys` set per call site, so
//!      e.g. `extract components` (which has no `class=` field on its
//!      output) errors clearly when the user supplies it.
//!   4. Applies the filter to a JSON array of objects (extract) or a plain
//!      `Vec<String>` (show --list).

use anyhow::Result;
use serde_json::Value;

/// Back-compat alias. The real type lives in `mcc::query_api::Query`.
pub type CompiledFilter = mcc::query_api::Query;

/// Parse a `--filter` expression. Returns the canonical AST.
pub fn compile(expr: &str) -> Result<CompiledFilter> {
    mcc::query_api::compile(expr)
}

/// Apply a filter to a JSON array of objects. `allowed_keys` is the set of
/// keys the row shape supports — using a key outside this set errors out.
///
/// Accepts `Option<&str>` (the raw expression) and compiles internally so
/// callers don't have to thread the compiled AST through.
///
/// For `attr(...)` predicates, the row's `name` and `uri` are used to fetch
/// the underlying definition's attributes via `mcc::get_def`.
pub fn apply_to_values(expr: Option<&str>, items: Value, allowed_keys: &[&str]) -> Result<Value> {
    let Some(s) = expr else { return Ok(items) };
    let q = compile(s)?;
    mcc::query_api::validate_allowed_fields(&q, allowed_keys)?;
    let Some(arr) = items.as_array() else {
        return Ok(items);
    };
    let filtered: Vec<Value> = arr
        .iter()
        .filter(|it| {
            let resolver = |name: &str, uri: &str| -> Vec<(String, String)> {
                let _ident = mcc::McIds::from(name);
                let _u = mcc::McURI::from(uri);
                mcc::query_api::attrs_for_def(name, uri, |n, u| {
                    mcc::get_def(&mcc::McIds::from(n), &mcc::McURI::from(u))
                })
            };
            mcc::query_api::matches_json_record_with(&q, it, resolver)
        })
        .cloned()
        .collect();
    Ok(Value::Array(filtered))
}

/// Apply a filter to a plain `Vec<String>` (used by `show --list`).
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
    use serde_json::json;

    #[test]
    fn eq_is_case_insensitive() {
        let items = json!([{"name": "res1"}, {"name": "RES1"}, {"name": "CAP1"}]);
        let out = apply_to_values(Some("name=RES"), items, &["name"]).unwrap();
        let names: Vec<&str> = out
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.get("name").and_then(|n| n.as_str()).unwrap())
            .collect();
        // Exact match (case-insensitive): both "res1" and "RES1" match "RES"
        assert_eq!(names, vec!["res1", "RES1"]);
    }

    #[test]
    fn glob_matches_prefix() {
        let items = json!([{"name": "MCU"}, {"name": "MCU_F4"}, {"name": "RES"}]);
        let out = apply_to_values(Some("name=MCU*"), items, &["name"]).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 2);
    }

    #[test]
    fn and_semantics() {
        let items = json!([
            {"kind": "component", "class": "RES10K"},
            {"kind": "module",    "class": "RES10K"},
            {"kind": "component", "class": "CAP10K"},
        ]);
        let out = apply_to_values(
            Some("kind=component,class=RES*"),
            items,
            &["name", "kind", "class"],
        )
        .unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
    }

    #[test]
    fn unknown_key_errors_with_allowed_set() {
        let items = json!([{"name": "X"}]);
        let err = apply_to_values(Some("name=X*"), items.clone(), &["class"]).unwrap_err();
        assert!(format!("{}", err).contains("unknown key"));

        let ok = apply_to_values(Some("name=X*"), items, &["name"]).unwrap();
        assert_eq!(ok.as_array().unwrap().len(), 1);
    }

    #[test]
    fn glob_special_chars_in_literal_are_escaped() {
        let items = json!([{"name": "v1.0"}, {"name": "v1X0"}]);
        let out = apply_to_values(Some("name=v1.0"), items, &["name"]).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
    }

    #[test]
    fn apply_to_names_only_accepts_name_key() {
        let out = apply_to_names(Some("name=R*"), vec!["RES".into(), "CAP".into()]).unwrap();
        assert_eq!(out, vec!["RES".to_string()]);

        let err = apply_to_names(Some("kind=component"), vec![]).unwrap_err();
        assert!(format!("{}", err).contains("unknown key"));
    }

    #[test]
    fn boolean_ops_in_filter() {
        let items = json!([
            {"kind": "component", "class": "RES10K"},
            {"kind": "module",    "class": "RES10K"},
            {"kind": "component", "class": "CAP10K"},
        ]);
        let out = apply_to_values(
            Some("kind=component AND class=RES*"),
            items,
            &["name", "kind", "class"],
        )
        .unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
    }
}
