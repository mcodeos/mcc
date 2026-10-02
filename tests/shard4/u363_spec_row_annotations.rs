// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U363 close-out (b4428): `spec = [...]` block rows.
//!
//! The spec block rides the attribute-value bracket (`mc_attr_lines`), so
//! the per-row tattr mount the instantiation body row got (caller 6t) now
//! has its mirror here: any row — first or continuation — may carry a
//! trailing `@key(...)` run, wrapped in ONE MCAST_SET_ATTRIBUTES bag
//! appended to the row's ATT_VALUES, which every reader unfolds by type
//! with zero Rust changes. Envelope conditions extend per the 2026-10-02
//! ruling: a range spells with `~` (binding above `@`, condition half one
//! raw-text node), a call condition takes raw-text arguments, and a
//! numeric pair still reads `-` as subtraction — including no-space.

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

const SCAFFOLD: &str = r#"abstract component SPECROWS
{
    pins = [out 1 = P @class(analog)]
    spec = [
DRAW_ROWS
    ]
}
"#;

/// The headline form: annotated spec rows, first and continuation, with the
/// second spelling's annotation on its own line (the line-join law makes
/// the two spellings the same tree).
#[test]
fn u363_spec__rows_carry_tattr_bags_first_and_continuation() {
    let _guard = common::lock();
    let rows = "    a = 5V @ds(p=1, trust=max)\n\
                \x20   b = 3V @ds(p=2)";
    let src = SCAFFOLD.replace("DRAW_ROWS", rows);
    let (tree, diags) = run_once("a.mc", &src);
    assert!(diags.is_empty(), "annotated spec rows must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for key in ["a", "b", "5V", "3V", "ds", "trust", "max"] {
        assert!(
            strings.iter().any(|s| s == key),
            "spec row piece '{key}' must be visible in the tree, got {strings:?}"
        );
    }
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "set_attributes"),
        "per-row tattrs ride the SET_ATTRIBUTES bag, got {types:?}"
    );
}

/// The next-line spelling: the annotation below its row joins by the
/// line-join law — same pieces, same bag.
#[test]
fn u363_spec__next_line_annotation_joins_to_the_same_bag() {
    let _guard = common::lock();
    let rows = "    a = 5V\n\
                \x20   @ds(p=1)";
    let src = SCAFFOLD.replace("DRAW_ROWS", rows);
    let (tree, diags) = run_once("b.mc", &src);
    assert!(diags.is_empty(), "the joined row must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "set_attributes"),
        "the joined annotation must ride the bag, got {types:?}"
    );
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for key in ["a", "5V", "ds"] {
        assert!(strings.iter().any(|s| s == key), "'{key}' must survive the join, got {strings:?}");
    }
}

/// Range condition per the 2026-10-02 ruling: the range spells with `~`,
/// binds above `@`, and the condition half stays ONE raw-text node.
#[test]
fn u363_spec__range_condition_joins_raw_whole() {
    let _guard = common::lock();
    let rows = "    a = 80dBm @30MHz ~ 1GHz";
    let src = SCAFFOLD.replace("DRAW_ROWS", rows);
    let (tree, diags) = run_once("c.mc", &src);
    assert!(diags.is_empty(), "the range condition must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    assert!(
        strings.iter().any(|s| s == "30MHz~1GHz"),
        "the range condition must survive as one raw text node, got {strings:?}"
    );
}

/// The numeric pair keeps its subtraction reading — ruling point 3 — and
/// the no-space spelling rides the same tree (b4433 sign-token doctrine).
#[test]
fn u363_spec__numeric_pair_still_subtracts() {
    let _guard = common::lock();
    for spelling in ["a = 100mA @5V - 2mA", "a = 100mA@5V-2mA"] {
        let src = SCAFFOLD.replace("DRAW_ROWS", spelling);
        let (tree, diags) = run_once("d.mc", &src);
        assert!(
            diags.is_empty(),
            "'{spelling}' must stay clean: {diags:?}"
        );
        let tree = tree.expect("must capture a tree");
        let mut strings = Vec::new();
        collect_strings(&tree, &mut strings);
        for key in ["100mA", "5V", "2mA"] {
            assert!(
                strings.iter().any(|s| s == key),
                "'{spelling}': piece '{key}' must be visible, got {strings:?}"
            );
        }
    }
}

/// The @pm(1) shape: the tattr bare-values arm and the envelope call arm
/// accept the same surface; in a spec row the ANNOTATED row wins the
/// %dprec merge — the bag mounts and the value keeps its plain half.
#[test]
fn u363_spec__call_condition_shape_mounts_the_bag_in_rows() {
    let _guard = common::lock();
    let rows = "    a = 5V @pm(1)";
    let src = SCAFFOLD.replace("DRAW_ROWS", rows);
    let (tree, diags) = run_once("e.mc", &src);
    assert!(diags.is_empty(), "the @pm(1) row must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut types = Vec::new();
    collect_types(&tree, &mut types);
    assert!(
        types.iter().any(|t| t == "set_attributes"),
        "the @pm(1) surface must resolve to the annotated row, got {types:?}"
    );
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    for key in ["5V", "pm"] {
        assert!(strings.iter().any(|s| s == key), "'{key}' must be visible, got {strings:?}");
    }
}

/// Comparator-led condition (`<1GHz-fcc`): no other reading starts a
/// condition with a comparator, and the operand continues over the
/// hyphenated word.
#[test]
fn u363_spec__comparator_led_condition_joins_whole() {
    let _guard = common::lock();
    let rows = "    a = 5V @< 1GHz - fcc";
    let src = SCAFFOLD.replace("DRAW_ROWS", rows);
    let (tree, diags) = run_once("f.mc", &src);
    assert!(diags.is_empty(), "the comparator condition must be clean: {diags:?}");
    let tree = tree.expect("must capture a tree");
    let mut strings = Vec::new();
    collect_strings(&tree, &mut strings);
    assert!(
        strings.iter().any(|s| s == "< 1GHz-fcc"),
        "the comparator-led condition must survive as one raw text node, got {strings:?}"
    );
}
