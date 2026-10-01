// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U363 gap 3a (b4406): instantiation body rows.
//!
//! An instantiation body (`::DC( ... )`) separates rows on a line break
//! beside the comma, and both layouts build the same MCAST_PARAM chain —
//! the argument reader cannot tell them apart (`mc_body_params`).
//!
//! A row may also carry a trailing `@attr(...)` run (`vin = f(2V) @ds(p=4)`).
//! The tattr chain wraps in one MCAST_SET_ATTRIBUTES bag appended to the row
//! attribute's ATT_VALUES — the same wrapper the tattr named-argument arm
//! uses — and the reader splits every bag that *follows* a real value out of
//! `values` into `McAttribute::annotations`, so value-position readers see
//! the stream an unannotated row produces.

#![allow(non_snake_case)]

use crate::common;

use serde_json::Value;

/// One parse run: fresh workspace, visit capture on, take the tree for the
/// URI, then the diagnostics as `(code, message)` pairs.
fn run_once(uri: &str, src: &str) -> (Option<Value>, Vec<(u32, String)>) {
    common::reset();
    mcc::set_ast_visit_json(true);
    mcc::clear_ast_visit_json();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let tree = mcc::take_ast_visit_json_for(uri);
    let diags = mcc::mcc_diagnose_all()
        .into_iter()
        .map(|d| (d.code, d.msg))
        .collect();
    (tree, diags)
}

/// Recursively collect every node kind name present in a visit JSON tree.
fn collect_types(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| collect_types(i, out)),
        Value::Object(map) => {
            if let Some(Value::String(t)) = map.get("kind") {
                out.push(t.clone());
            }
            map.values().for_each(|c| collect_types(c, out));
        }
        _ => {}
    }
}

/// Recursively collect every string payload in a visit JSON tree.
fn collect_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| collect_strings(i, out)),
        Value::Object(map) => {
            map.values().for_each(|c| collect_strings(c, out));
            if let Some(Value::String(s)) = map.get("value") {
                out.push(s.clone());
            }
        }
        _ => {}
    }
}

/// Gap 3a: newline-separated body rows parse clean and both row keys are
/// visible in the tree.
#[test]
fn u363_body__newline_separated_rows_parse_clean() {
    let _guard = common::lock();
    let src = r#"abstract component BODYROWS
{
    pins = [
        out 1 = P @class(analog)
        psnk [4, [2, 3]] = [VCC, GND]::DC(
            vin = supply_range(2V ~ 3.6V)
            vmax = absmax(-0.3V ~ 3.9V)
        )
    ]
}
"#;
    let (tree, diags) = run_once("a.mc", src);
    assert!(
        !diags.iter().any(|(c, _)| *c == 2083),
        "the second body row must not die E2083: {diags:?}"
    );
    assert!(diags.is_empty(), "newline-separated rows must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for key in ["vin", "vmax", "supply_range", "absmax"] {
        assert!(
            strings.iter().any(|s| s == key),
            "body row '{key}' must be visible in the tree, got {strings:?}"
        );
    }
}

/// Gap 3a: a row with a trailing tattr run parses clean — no 2083 (the old
/// second-row death) and no 3022 (the SET_ATTRIBUTES bag has a reader arm) —
/// and the tattr keys ride the tree under the row.
#[test]
fn u363_body__row_tattr_parses_without_2083_or_3022() {
    let _guard = common::lock();
    let src = r#"abstract component BODYROWS
{
    pins = [
        out 1 = P @class(analog)
        psnk [4, [2, 3]] = [VCC, GND]::DC(
            vin = supply_range(2V ~ 3.6V) @ds(p=4, trust=max)
            vmax = absmax(-0.3V ~ 3.9V)   @ds(p=4)
        )
    ]
}
"#;
    let (tree, diags) = run_once("b.mc", src);
    assert!(
        !diags.iter().any(|(c, _)| *c == 2083),
        "annotated body rows must not die E2083: {diags:?}"
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == 3022),
        "the SET_ATTRIBUTES bag must have a reader arm (no 3022): {diags:?}"
    );
    assert!(diags.is_empty(), "annotated body rows must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "set_attributes"),
        "row tattrs ride the SET_ATTRIBUTES wrapper, got {types:?}"
    );
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for key in ["ds", "trust", "max"] {
        assert!(
            strings.iter().any(|s| s == key),
            "row tattr key/value '{key}' must be visible in the tree, got {strings:?}"
        );
    }
}

/// The comma form is untouched: rows separated on commas still parse clean
/// and carry no SET_ATTRIBUTES bag.
#[test]
fn u363_body__comma_form_is_untouched() {
    let _guard = common::lock();
    let src = r#"abstract component BODYROWS
{
    pins = [
        out 1 = P @class(analog)
        psnk [4, [2, 3]] = [VCC, GND]::DC(
            vin = supply_range(2V ~ 3.6V),
            vmax = absmax(-0.3V ~ 3.9V)
        )
    ]
}
"#;
    let (tree, diags) = run_once("c.mc", src);
    assert!(diags.is_empty(), "comma-separated rows must stay clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        !types.iter().any(|t| t == "set_attributes"),
        "the comma form must not grow a SET_ATTRIBUTES bag, got {types:?}"
    );
}
