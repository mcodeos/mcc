// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U363 gap 3b (b4421): value-face `[...]` lists.
//!
//! A value list keeps the author's item layout — items separate on a comma
//! OR a line break (`mc_list_items`) — and a direct-call item may carry a
//! trailing `@attr(...)` run: the chain wraps in one MCAST_SET_ATTRIBUTES
//! bag riding as the fcall's third linked child (the slot the net arm
//! uses), which every fcall reader ignores because they pick children up by
//! type. An envelope point may also ride a hyphen-joined bareword condition
//! (`10mA@periph-clk-off`); the condition half is one MCAST_ID data node
//! with the joined text, and the rule carries IDA_BASE_PREC so the word
//! chain continues greedily over the minus (U292 doctrine).

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

const SCAFFOLD: &str = r#"abstract component LISTROWS
{
    pins = [
        out 1 = P @class(analog)
        psnk [4, [2, 3]] = [VCC, GND]::DC(
            vin = supply_range(2V ~ 3.6V)
DRAW_ROWS
        )
    ]
}
"#;

/// The headline form: a multi-line list of annotated call rows — the shape
/// the cc2530/esp32h2 census blocks write.
#[test]
fn u363_list__multiline_rows_with_tattr_parse_clean() {
    let _guard = common::lock();
    let draw = "            draw = [\n\
                \x20               current_draw(mode = rx, value = 29.6mA) @ds(p=4, trust=max)\n\
                \x20               current_draw(mode = sleep, value = 9uA) @ds(p=5)\n\
                \x20           ]";
    let src = SCAFFOLD.replace("DRAW_ROWS", draw);
    let (tree, diags) = run_once("a.mc", &src);
    assert!(
        !diags.iter().any(|(c, _)| *c == 2083),
        "annotated list rows must not die E2083: {diags:?}"
    );
    assert!(diags.is_empty(), "the list block must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for key in ["draw", "current_draw", "mode", "rx", "sleep", "29.6mA", "9uA", "ds", "trust"] {
        assert!(
            strings.iter().any(|s| s == key),
            "list row piece '{key}' must be visible in the tree, got {strings:?}"
        );
    }
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "set_attributes"),
        "per-item tattrs ride the SET_ATTRIBUTES bag, got {types:?}"
    );
}

/// The comma spelling of the same list builds the same tree pieces and
/// carries no annotation bag.
#[test]
fn u363_list__comma_form_is_untouched() {
    let _guard = common::lock();
    let draw = "            draw = [current_draw(mode = rx, value = 29.6mA), current_draw(mode = sleep, value = 9uA)]";
    let src = SCAFFOLD.replace("DRAW_ROWS", draw);
    let (tree, diags) = run_once("b.mc", &src);
    assert!(diags.is_empty(), "the comma list must stay clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        !types.iter().any(|t| t == "set_attributes"),
        "the comma form must not grow a SET_ATTRIBUTES bag, got {types:?}"
    );
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    assert!(
        strings.iter().any(|s| s == "current_draw"),
        "both call rows must be visible, got {strings:?}"
    );
}

/// The envelope point over a hyphen-joined bareword condition: the joined
/// word survives the tree whole, and the unit-value spelling beside it is
/// untouched.
#[test]
fn u363_list__bareword_envelope_point_joins_whole() {
    let _guard = common::lock();
    let draw = "            envelope = [10mA@periph-clk-off, 17mA@periph-clk-on]";
    let src = SCAFFOLD.replace("DRAW_ROWS", draw);
    let (tree, diags) = run_once("c.mc", &src);
    assert!(
        !diags.iter().any(|(c, _)| *c == 2082),
        "the bareword condition must not die E2082: {diags:?}"
    );
    assert!(diags.is_empty(), "the envelope list must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for word in ["periph-clk-off", "periph-clk-on", "10mA", "17mA"] {
        assert!(
            strings.iter().any(|s| s == word),
            "envelope piece '{word}' must be visible in the tree, got {strings:?}"
        );
    }
    assert!(
        !strings.iter().any(|s| s == "clk"),
        "the condition word must not be split at the hyphen, got {strings:?}"
    );
}
