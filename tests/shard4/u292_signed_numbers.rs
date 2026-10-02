// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U292 step 2: the signed-number fat token is narrowed into the grammar.
//!
//! A leading `+`/`-` lexes as its own operator token, so a no-space `5-3`
//! reaches the binary subtraction arms, and a bare signed literal (`-3.3V`,
//! uart's `low:-15V ~ -3V`) is re-attached by the unary `mc_literal` arms.
//! The synthesized node keeps the fat-token data string and span, so every
//! data-string reader sees the same bytes the fat lexeme produced.

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

const SPEC: &str = r#"component SIGNEDNUM
{
    pins = [
        in 1 = A @class(digital), "probe"
    ]
    spec = [
ROWS
    ]
}
"#;

fn spec_rows(rows: &str) -> String {
    SPEC.replace("ROWS", rows)
}

/// `5-3` with no spaces is subtraction — the operator token survives the
/// lexer instead of being swallowed into a fat `+3`/`-3` number.
#[test]
fn u292_sign__no_space_subtraction_reaches_the_binary_arm() {
    let _guard = common::lock();
    let (tree, diags) = run_once("a.mc", &spec_rows("        a = 5-3, \"no spaces\"\n        b = 5 - 3, \"spaced\""));
    assert!(
        diags.is_empty(),
        "both subtraction spellings must parse clean: {diags:?}"
    );
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "opd_minus"),
        "`5-3` must build the binary minus node, got {types:?}"
    );
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for operand in ["5", "3"] {
        assert!(
            strings.iter().any(|s| s == operand),
            "subtraction operands must stay plain ints, got {strings:?}"
        );
    }
    assert!(
        !strings.iter().any(|s| s == "-3" || s == "+3"),
        "the sign must not fold into the operand any more, got {strings:?}"
    );
}

/// A bare signed unit value re-attaches through the unary arm with the same
/// data bytes the fat lexeme produced (mcp3204 census form).
#[test]
fn u292_sign__signed_unit_values_keep_fat_token_bytes() {
    let _guard = common::lock();
    let (tree, diags) = run_once(
        "a.mc",
        &spec_rows("        temp = -40°C ~ +85°C, \"industrial\"\n        vneg = -3.3V, \"negative rail\"\n        gain = +6dB, \"explicit plus\""),
    );
    assert!(diags.is_empty(), "signed literals must parse clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "uvalue"),
        "signed unit values ride the UVALUE wrapper like unsigned ones, got {types:?}"
    );
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for lit in ["-40°C", "+85°C", "-3.3V", "+6dB"] {
        assert!(
            strings.iter().any(|s| s == lit),
            "signed literal '{lit}' must keep its sign in the data string, got {strings:?}"
        );
    }
}

/// The uart role-table form: signed windows inside a pin-row tattr list, the
/// corpus shape ~290 live lines lean on.
#[test]
fn u292_sign__negative_window_rows_parse_clean() {
    let _guard = common::lock();
    let src = "component DCE\n{\n    pins = [\n        in 1 = RXD @class(digital), \"Receive\", voltage:[low:-15V ~ -3V, high:+3V ~ +15V]\n    ]\n}\n";
    let (tree, diags) = run_once("a.mc", src);
    assert!(
        diags.is_empty(),
        "the uart negative-window row must parse clean: {diags:?}"
    );
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for lit in ["-15V", "-3V", "+3V", "+15V"] {
        assert!(
            strings.iter().any(|s| s == lit),
            "window bound '{lit}' must survive with its sign, got {strings:?}"
        );
    }
}

/// A plain unsigned literal is untouched by the narrowing.
#[test]
fn u292_sign__unsigned_literals_untouched() {
    let _guard = common::lock();
    let (tree, diags) = run_once(
        "a.mc",
        &spec_rows("        vdd = 2.7V ~ 5.5V, \"range\"\n        n = 12, \"plain int\"\n        f = 29.6mA, \"plain float uv\""),
    );
    assert!(diags.is_empty(), "unsigned forms must stay clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for lit in ["2.7V", "5.5V", "12", "29.6mA"] {
        assert!(
            strings.iter().any(|s| s == lit),
            "unsigned literal '{lit}' must keep its exact spelling, got {strings:?}"
        );
    }
}
