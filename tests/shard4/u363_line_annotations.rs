// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U363 gaps 1+2 (b4401): line-level annotations.
//!
//! Gap 1 — an own-line trailing `@attr...` run joins the row. The EL rule
//! emits MC_ENDL for every line break unconditionally, so the join lives in
//! `mca_lex` (the parser's token fetcher): the ENDL run between a token and a
//! line-starting MCPT_AT is skipped, and the parser sees exactly the stream
//! the same-line spelling produces. The acceptance face is the tree
//! equivalence the u261 comparator uses: same-line and own-line variants must
//! build the identical visit JSON with zero diagnostics.
//!
//! Gap 2 — `@key(k=v, ...)` named arguments. Each pair is one
//! clause-3.1-shaped MCAST_ATTRIBUTE (ATT_ID + ATT_VALUES) inside one
//! MCAST_SET_ATTRIBUTES wrapper under the tattr's ATT_VALUES — the node the
//! Rust reader already unfolds into `McAttrVal::Attributes`, so parsing must
//! produce neither 2083 (invalid pin declaration) nor 3022 (attribute value
//! type not supported), and the named keys must be visible in the tree.

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

/// Strip `span` fields recursively: the two spellings cover different byte
/// ranges by construction, so tree equivalence is judged on kinds, values and
/// child shape (the u261 comparator compares parses of the *same* source and
/// keeps spans; here the sources differ on purpose).
fn strip_spans(v: &mut Value) {
    match v {
        Value::Array(items) => items.iter_mut().for_each(strip_spans),
        Value::Object(map) => {
            map.remove("span");
            map.values_mut().for_each(strip_spans);
        }
        _ => {}
    }
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

/// Gap 1: the own-line `@role(quiet)` run must build the identical tree the
/// same-line spelling builds — zero diagnostics on both faces.
#[test]
fn u363_line1__own_line_annotation_builds_the_same_tree_as_the_same_line_form() {
    let _guard = common::lock();
    let same = r#"abstract component MICROPHONE.MEMS
{
    name = "MEMS silicon microphone"

    pins = [
        out 1 = P @class(analog)
        psnk [4, [2, 3]] = [VCC, GND]::DC(3.3V) @role(quiet)
    ]
}
"#;
    let own = r#"abstract component MICROPHONE.MEMS
{
    name = "MEMS silicon microphone"

    pins = [
        out 1 = P @class(analog)
        psnk [4, [2, 3]] = [VCC, GND]::DC(3.3V)
        @role(quiet)
    ]
}
"#;
    let (tree_a, diags_a) = run_once("a.mc", same);
    let (tree_b, diags_b) = run_once("b.mc", own);
    assert!(diags_a.is_empty(), "same-line run must be clean: {diags_a:?}");
    assert!(diags_b.is_empty(), "own-line run must be clean: {diags_b:?}");
    let mut tree_a = tree_a.expect("same-line run must capture a tree");
    let mut tree_b = tree_b.expect("own-line run must capture a tree");
    strip_spans(&mut tree_a);
    strip_spans(&mut tree_b);
    assert_eq!(
        tree_a, tree_b,
        "own-line and same-line spellings must build the same AST"
    );
}

/// Gap 1: a comment line between the row and the `@` run still joins (the
/// comment's MC_ENDL is part of the skipped run), and a two-line `@` run
/// attaches both tattrs to the same row.
#[test]
fn u363_line1__comment_line_and_multi_line_runs_still_join() {
    let _guard = common::lock();
    let src = r#"abstract component MICROPHONE.MEMS
{
    name = "MEMS silicon microphone"

    pins = [
        psnk [4, [2, 3]] = [VCC, GND]::DC(3.3V)
        // a comment line between the row and the run
        @role(quiet)
        @class(analog)
    ]
}
"#;
    let (tree, diags) = run_once("c.mc", src);
    assert!(diags.is_empty(), "comment-joined run must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    let tattrs = types.iter().filter(|t| *t == "attribute").count();
    assert!(
        tattrs >= 2,
        "both own-line tattrs must land in the tree, got {types:?}"
    );
}

/// Gap 2: `@ds(p=4, trust=max)` parses clean — no 2083, no 3022 — and the
/// named keys are visible in the tree (SET_ATTRIBUTES wrapper over the
/// clause-3.1-shaped pairs).
#[test]
fn u363_line2__named_arguments_parse_without_2083_or_3022() {
    let _guard = common::lock();
    let src = r#"abstract component DATASHEET.ROW
{
    name = "named-argument tattr"

    pins = [
        out 5 = S @ds(p=4, trust=max)
    ]
}
"#;
    let (tree, diags) = run_once("d.mc", src);
    assert!(
        !diags.iter().any(|(c, _)| *c == 2083),
        "named-argument tattr must not be E2083: {diags:?}"
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == 3022),
        "the SET_ATTRIBUTES wrapper must have a reader arm (no 3022): {diags:?}"
    );
    assert!(diags.is_empty(), "named-argument row must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "set_attributes"),
        "named args ride the SET_ATTRIBUTES wrapper, got {types:?}"
    );
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for key in ["ds", "p", "trust", "max"] {
        assert!(
            strings.iter().any(|s| s == key),
            "named key/value '{key}' must be visible in the tree, got {strings:?}"
        );
    }
}

/// The bare value form (`@class(analog)`) is untouched by the named arm.
#[test]
fn u363_line2__bare_value_form_is_untouched() {
    let _guard = common::lock();
    let src = r#"abstract component DATASHEET.ROW
{
    name = "bare value tattr"

    pins = [
        in 1 = A @class(analog)
    ]
}
"#;
    let (tree, diags) = run_once("e.mc", src);
    assert!(diags.is_empty(), "bare form must stay clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        !types.iter().any(|t| t == "set_attributes"),
        "the bare form must not grow a SET_ATTRIBUTES wrapper, got {types:?}"
    );
}
