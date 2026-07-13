// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared `--filter <EXPR>` helper used by `mcc extract`, `mcc show --list`,
//! and (later) `mcc query`.
//!
//! **v1 syntax**: comma-separated `key=value` predicates joined with AND.
//! RHS supports `*` / `?` wildcards (compiled to regex via `regex` crate).
//!
//! Examples:
//!   `--filter 'name=RES*'`
//!   `--filter 'kind=component,class=MCU*'`
//!
//! Unknown keys return an error pointing at the allowed keyset, so callers
//! like `extract components` (which has only `name` + `class`) error if the
//! user supplies `kind=` — preventing silent over-matching.

use anyhow::{anyhow, Result};
use regex::Regex;
use serde_json::Value;

/// One `key=value` predicate after parsing.
#[derive(Debug, Clone)]
pub struct CompiledFilter {
    pub preds: Vec<(String, FilterOp)>,
}

#[derive(Debug, Clone)]
pub enum FilterOp {
    /// Case-insensitive equality on the trimmed lowercased value.
    Eq(String),
    /// Wildcard RHS, compiled to an anchored regex (`^...$`).
    Glob(Regex),
}

/// Parse `key=value,key=value...` into a [`CompiledFilter`].
pub fn compile(expr: &str) -> Result<CompiledFilter> {
    let mut preds = Vec::new();
    for piece in expr.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (k, v) = piece
            .split_once('=')
            .ok_or_else(|| anyhow!("filter: expected key=value, got '{}'", piece))?;
        let key = k.trim().to_lowercase();
        let rhs = v.trim().to_string();
        let op = if rhs.contains('*') || rhs.contains('?') {
            // Glob → anchored regex. Escape regex metachars in the literal segments.
            let pattern = glob_to_regex(&rhs);
            FilterOp::Glob(Regex::new(&pattern)?)
        } else {
            FilterOp::Eq(rhs.to_lowercase())
        };
        preds.push((key, op));
    }
    Ok(CompiledFilter { preds })
}

/// Apply a filter to a JSON array of objects.
///
/// `allowed_keys` is the set of keys the caller knows about — using a key
/// outside this set returns an error so `extract components` (which doesn't
/// emit `kind`) doesn't silently accept `kind=component`.
pub fn apply_to_values(filter: Option<&str>, items: Value, allowed_keys: &[&str]) -> Result<Value> {
    let Some(expr) = filter else {
        return Ok(items);
    };
    let compiled = compile(expr)?;
    for (k, _) in &compiled.preds {
        if !allowed_keys.iter().any(|a| a.eq_ignore_ascii_case(k)) {
            return Err(anyhow!(
                "filter: unknown key '{}', expected one of {:?}",
                k,
                allowed_keys
            ));
        }
    }
    let Some(arr) = items.as_array() else {
        return Ok(items);
    };
    let filtered: Vec<Value> = arr
        .iter()
        .filter(|it| matches_item(&compiled, it))
        .cloned()
        .collect();
    Ok(Value::Array(filtered))
}

/// Apply a filter to a plain `Vec<String>` (used by `show --list`).
/// Only the `name=` key is meaningful for name-only lists; other keys error.
pub fn apply_to_names(filter: Option<&str>, names: Vec<String>) -> Result<Vec<String>> {
    let Some(expr) = filter else {
        return Ok(names);
    };
    let compiled = compile(expr)?;
    for (k, _) in &compiled.preds {
        if !k.eq_ignore_ascii_case("name") {
            return Err(anyhow!(
                "filter: unknown key '{}', expected 'name' for --list targets",
                k
            ));
        }
    }
    Ok(names
        .into_iter()
        .filter(|n| {
            compiled.preds.iter().all(|(k, op)| {
                if k.eq_ignore_ascii_case("name") {
                    matches_value(op, n)
                } else {
                    true
                }
            })
        })
        .collect())
}

fn matches_item(filter: &CompiledFilter, item: &Value) -> bool {
    filter
        .preds
        .iter()
        .all(|(k, op)| match item.get(k).and_then(|v| v.as_str()) {
            Some(s) => matches_value(op, s),
            None => false,
        })
}

fn matches_value(op: &FilterOp, s: &str) -> bool {
    match op {
        FilterOp::Eq(needle) => s.to_lowercase() == *needle,
        FilterOp::Glob(re) => re.is_match(s),
    }
}

/// Convert a glob (`*`/`?`) to a regex. Everything else is regex-escaped so
/// the user's literal text stays literal.
fn glob_to_regex(glob: &str) -> String {
    let mut out = String::from("^");
    for ch in glob.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            // Regex metacharacters to escape
            '.' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '|' | '^' | '$' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn eq_is_case_insensitive() {
        let f = compile("name=RES").unwrap();
        assert!(matches_item(&f, &json!({"name": "res1"})));
        assert!(matches_item(&f, &json!({"name": "RES1"})));
        assert!(!matches_item(&f, &json!({"name": "CAP1"})));
    }

    #[test]
    fn glob_matches_prefix() {
        let f = compile("class=MCU*").unwrap();
        assert!(matches_item(&f, &json!({"class": "MCU"})));
        assert!(matches_item(&f, &json!({"class": "MCU_F4"})));
        assert!(!matches_item(&f, &json!({"class": "RES"})));
    }

    #[test]
    fn glob_matches_suffix_and_middle() {
        let f = compile("name=*_vcc").unwrap();
        assert!(matches_item(&f, &json!({"name": "vcc"})));
        assert!(!matches_item(&f, &json!({"name": "vcc1"})));
        assert!(matches_item(&f, &json!({"name": "core_vcc"})));

        let f = compile("name=R?S").unwrap();
        assert!(matches_item(&f, &json!({"name": "RES"})));
        assert!(matches_item(&f, &json!({"name": "R1S"})));
        assert!(!matches_item(&f, &json!({"name": "RXS"})));
    }

    #[test]
    fn and_semantics() {
        let f = compile("kind=component,class=RES*").unwrap();
        assert!(matches_item(
            &f,
            &json!({"kind": "component", "class": "RES10K"})
        ));
        assert!(!matches_item(
            &f,
            &json!({"kind": "module", "class": "RES10K"})
        ));
        assert!(!matches_item(
            &f,
            &json!({"kind": "component", "class": "CAP10K"})
        ));
    }

    #[test]
    fn unknown_key_errors_with_allowed_set() {
        let items = json!([{"name": "X"}]);
        let err = apply_to_values(Some("foo=bar"), items.clone(), &["name"]).unwrap_err();
        assert!(format!("{}", err).contains("unknown key 'foo'"));

        let ok = apply_to_values(Some("name=X*"), items, &["name"]).unwrap();
        assert_eq!(ok.as_array().unwrap().len(), 1);
    }

    #[test]
    fn glob_special_chars_in_literal_are_escaped() {
        // `.` is a regex metachar; the literal must NOT match across the dot.
        let f = compile("name=v1.0").unwrap();
        assert!(matches_item(&f, &json!({"name": "v1.0"})));
        assert!(!matches_item(&f, &json!({"name": "v1X0"})));
    }

    #[test]
    fn apply_to_names_only_accepts_name_key() {
        assert!(
            apply_to_names(Some("name=R*"), vec!["RES".into(), "CAP".into()])
                .unwrap()
                .contains(&"RES".to_string())
        );

        let err = apply_to_names(Some("kind=component"), vec![]).unwrap_err();
        assert!(format!("{}", err).contains("unknown key"));
    }
}
