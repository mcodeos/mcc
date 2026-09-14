// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the `inst-list` projection (build-design §3.7). Every module
// / component of the frozen circuit must carry two-space identity — the arena
// `NodeId` used as the build-internal join handle, and the canonical def key
// `(uri, ident)` of its class used as the downstream persistence key — instead
// of forcing consumers to re-derive identity from the instance path string.

mod common;

use mcc::{McIds, McURI};
use serde_json::Value;

const SOURCE: &str = r#"
component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}
module inner(psnk GND)
{
    RES R1
    R1.1 -> GND
    R1.2 -> GND
}
module main(psnk GND)
{
    inner U_IN
    RES R2
    R2.1 -> GND
    R2.2 -> GND
    U_IN.GND -> GND
}
"#;

const URI: &str = "/mcc/inst-list.mc";

/// Freeze the circuit once and hand back the flat table plus the arena root, so
/// the test can cross-check the projected `node` against the arena's own id.
///
/// The caller must hold [`common::lock`] for its whole body: the registry the
/// projection reads is global state, so a sibling test's `reset()` landing
/// between this call and `build_inst_list` would strip the class defs.
fn frozen(source: &str) -> (mcc::InstTable, u32) {
    common::reset();
    let uri: McURI = URI.to_string();
    common::load_string(URI, source);
    let ident = McIds::from("main");
    let (_inst, table, arena, _store) =
        mcc::mcc_build_flat_with_arena(&ident, &uri, 1).expect("pass2_flat failed");
    (table, arena.root().0)
}

fn rows(items: &Value) -> Vec<&Value> {
    items
        .as_array()
        .expect("items must be an array")
        .iter()
        .collect()
}

fn row_for<'a>(items: &'a Value, path: &str) -> &'a Value {
    rows(items)
        .into_iter()
        .find(|r| r["path"] == path)
        .unwrap_or_else(|| panic!("no inst-list row for path '{}': {}", path, items))
}

#[test]
fn inst_list_rows_carry_node_id_and_def_key() {
    let _lock = common::lock();
    let (table, root_node) = frozen(SOURCE);
    let (_, items, count) = mcc::export::instlist::build_inst_list(&table, 0);

    // Module + component instances only: `main`, `main.U_IN`, `main.U_IN.R1`,
    // `main.R2`. Pins / ports / labels own no arena node and must not appear.
    assert_eq!(count, 4, "unexpected rows: {}", items);

    let main = row_for(&items, "main");
    assert_eq!(main["class"]["key"]["ident"], "main");
    assert_eq!(main["class"]["key"]["uri"], URI);
    assert_eq!(
        main["node"].as_u64(),
        Some(u64::from(root_node)),
        "the built module's node must be the arena root"
    );

    let sub = row_for(&items, "main.U_IN");
    assert_eq!(sub["class"]["key"]["ident"], "inner");
    assert_eq!(sub["class"]["key"]["uri"], URI);

    for path in ["main.U_IN.R1", "main.R2"] {
        let r = row_for(&items, path);
        assert_eq!(r["class"]["key"]["ident"], "RES", "row {path}");
        assert_eq!(r["class"]["key"]["uri"], URI, "row {path}");
    }

    // `class.def` is the world-local DefId: present (the def registry holds
    // every class of the build) and equal for two instances of the same class.
    let def_of = |p: &str| row_for(&items, p)["class"]["def"].as_u64();
    assert!(def_of("main").is_some(), "module def id missing: {}", main);
    assert!(def_of("main.U_IN.R1").is_some());
    assert_eq!(def_of("main.U_IN.R1"), def_of("main.R2"));

    // The join handle: every emitted node is a distinct arena id.
    let mut nodes: Vec<u64> = rows(&items)
        .iter()
        .map(|r| r["node"].as_u64().expect("every row carries a node id"))
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    assert_eq!(nodes.len(), count, "node ids must be distinct: {nodes:?}");
}

#[test]
fn inst_list_text_and_csv_serialization() {
    let _lock = common::lock();
    let (table, _) = frozen(SOURCE);

    let (text, _, count) = mcc::export::instlist::build_inst_list(&table, 0);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), count);
    for line in &lines {
        assert_eq!(
            line.split('\t').count(),
            3,
            "text row must be node/path/ident: {line}"
        );
    }

    let (csv, _, count) = mcc::export::instlist::build_inst_list(&table, 4);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), count);
    for line in &lines {
        assert_eq!(
            line.split(',').count(),
            3,
            "csv row must be node,path,ident: {line}"
        );
    }
}
