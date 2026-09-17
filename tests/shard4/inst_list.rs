// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the `inst-list` projection (build-design §3.7). Every object
// of the frozen circuit that carries an identity must project it — the arena
// `NodeId` used as the build-internal join handle, the canonical def key
// `(uri, ident)` used as the downstream persistence key, and, for a pin / port,
// the `PointId` that joins it to the stage readouts — instead of forcing
// consumers to re-derive identity from the instance path string.
//
// The row set has two blocks (O16): the instance rows the artifact has always
// emitted, then the point-level rows appended after them.

use crate::common;

use std::path::PathBuf;

use mcc::{InstKind, McIds, McURI};
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

/// The instance block, in the order the artifact has always emitted it: the
/// entries of the flat table in id order (the table is a `BTreeMap` keyed by
/// id), filtered to the two kinds that name an arena node.
const INSTANCE_PATHS: [&str; 4] = ["main", "main.R2", "main.U_IN", "main.U_IN.R1"];

/// The point block: one row per pin / port of the board, which are the two
/// kinds that name a physical point (§2.4).
const POINT_PATHS: [&str; 6] = [
    "main.GND",
    "main.R2.1",
    "main.R2.2",
    "main.U_IN.GND",
    "main.U_IN.R1.1",
    "main.U_IN.R1.2",
];

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

/// The 1-based number of the first source line containing `needle`. Expected
/// `loc` values are derived from the fixture text rather than hardcoded, so
/// editing the fixture cannot silently invalidate them.
fn line_of(needle: &str) -> u64 {
    let idx = SOURCE
        .lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("fixture has no line containing '{}'", needle));
    idx as u64 + 1
}

fn text_field<'a>(text: &'a str, path: &str) -> Vec<&'a str> {
    text.lines()
        .find(|l| l.split('\t').nth(1) == Some(path))
        .unwrap_or_else(|| panic!("no text row for path '{}':\n{}", path, text))
        .split('\t')
        .collect()
}

#[test]
fn inst_list_rows_carry_node_id_and_def_key() {
    let _lock = common::lock();
    let (table, root_node) = frozen(SOURCE);
    let (_, items, count) = mcc::export::instlist::build_inst_list(&table, 0);

    // Four instances (`main`, `main.R2`, `main.U_IN`, `main.U_IN.R1`) plus the
    // six points O16 appended (two module ports, four component pins).
    assert_eq!(count, 4 + 6, "unexpected rows: {}", items);

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

    // Every instance row carries the same five fields — a row is a row, so a
    // consumer reads one shape. `point` is the field it does not have.
    for r in rows(&items) {
        let keys: Vec<&str> = r
            .as_object()
            .expect("row is an object")
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(
            keys,
            ["class", "loc", "node", "path", "point"],
            "row shape drifted: {r}"
        );
    }
    for p in INSTANCE_PATHS {
        assert_eq!(row_for(&items, p)["point"], Value::Null, "row {p}");
    }

    // The join handle: every node a row carries is a distinct arena id. The
    // rows that carry one are the instances **plus the two module ports** — a
    // port owns an arena node of its own, so the blanket "pins and ports own no
    // arena node" this test used to assert was true of pins only. The four
    // component pins are the rows that name no node.
    let carried: Vec<&Value> = rows(&items)
        .iter()
        .filter(|r| r["node"] != Value::Null)
        .cloned()
        .collect();
    assert_eq!(
        carried.len(),
        INSTANCE_PATHS.len() + 2,
        "rows carrying a node: {}",
        items
    );
    let mut nodes: Vec<u64> = carried
        .iter()
        .map(|r| r["node"].as_u64().expect("a node row carries a number"))
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    assert_eq!(
        nodes.len(),
        carried.len(),
        "node ids must be distinct: {nodes:?}"
    );
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
            5,
            "text row must be node/path/ident/point/loc: {line}"
        );
    }

    let (csv, _, count) = mcc::export::instlist::build_inst_list(&table, 4);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), count);
    for line in &lines {
        assert_eq!(
            line.split(',').count(),
            5,
            "csv row must be node,path,ident,point,loc: {line}"
        );
    }
}

/// The two blocks, and the promise that appending the second one did not
/// reshuffle the first: the instance rows keep the sequence the artifact had
/// before O16 (the table's id order, filtered).
#[test]
fn point_rows_are_appended_after_the_instance_block() {
    let _lock = common::lock();
    let (table, _) = frozen(SOURCE);
    let (_, items, _) = mcc::export::instlist::build_inst_list(&table, 0);
    let rows = rows(&items);

    let head: Vec<&str> = rows[..INSTANCE_PATHS.len()]
        .iter()
        .map(|r| r["path"].as_str().expect("row has a path"))
        .collect();
    assert_eq!(head, INSTANCE_PATHS, "the instance block moved: {}", items);

    let tail: Vec<&Value> = rows[INSTANCE_PATHS.len()..].to_vec();
    // The tail is every `Pin` / `Port` entry of the table, in the table's own
    // id order. It is *not* "every row that carries a point": a port's
    // aggregate / bus-member spellings name no point and print `null` (locked
    // on the real board in `hbl_point_rows_agree_with_the_stage_view`).
    let want: Vec<String> = table
        .iter()
        .filter(|(_, e)| matches!(e.kind, InstKind::Pin | InstKind::Port))
        .map(|(_, e)| e.path.clone())
        .collect();
    let got: Vec<&str> = tail
        .iter()
        .map(|r| r["path"].as_str().expect("row has a path"))
        .collect();
    assert_eq!(got, want, "point rows: {}", items);
    // The fixture is all scalar, so here every tail row does carry one.
    assert!(tail.iter().all(|r| r["point"] != Value::Null));
}

/// A pin names no def of its own, so the def half of its key is the class it
/// was flattened into — and the `PointId` beside it must agree: the node half
/// of the point is the node of that same instance. A `PointId` whose node is
/// not the owning instance names nothing (design §2.1: the member half is an
/// ordinal in that instance's own def member ledger).
#[test]
fn a_point_row_names_the_class_it_was_flattened_into() {
    let _lock = common::lock();
    let (table, _) = frozen(SOURCE);
    let (_, items, _) = mcc::export::instlist::build_inst_list(&table, 0);

    // Same class object as the instance it belongs to, DefId included.
    assert_eq!(
        row_for(&items, "main.R2.1")["class"],
        row_for(&items, "main.R2")["class"]
    );
    assert_eq!(
        row_for(&items, "main.U_IN.R1.2")["class"],
        row_for(&items, "main.U_IN.R1")["class"]
    );
    assert_eq!(
        row_for(&items, "main.U_IN.GND")["class"]["key"]["ident"],
        "inner"
    );

    for path in POINT_PATHS {
        let r = row_for(&items, path);
        let owner = path.rsplit_once('.').expect("a point path is nested").0;
        let owner_row = row_for(&items, owner);
        let point = r["point"].as_str().expect("a point row carries a point");
        let node_half = point
            .split(':')
            .next()
            .expect("a point is spelled node:member");
        assert_eq!(
            node_half,
            format!("N{}", owner_row["node"].as_u64().expect("owner has a node")),
            "row {path}: the point must name the node of the instance it belongs to"
        );
    }

    // A port owns an arena node distinct from the module it is a port of; the
    // old blanket "pins and ports own no arena node" was true of pins only.
    let port = row_for(&items, "main.GND");
    assert!(port["node"].as_u64().is_some(), "port row: {}", port);
    assert_ne!(port["node"], row_for(&items, "main")["node"]);
}

/// The cross-artifact lock O16 exists for: a row of the artifact and a row of
/// the `stage.p2` readout name the same point by the same key. This is the
/// "does not contradict the existing readouts" acceptance, taken directly.
#[test]
fn the_point_column_is_the_key_the_stage_views_read() {
    let _lock = common::lock();
    let (table, _) = frozen(SOURCE);
    let (_, items, _) = mcc::export::instlist::build_inst_list(&table, 0);
    let view = mcc::stages::p2::build_p2(&table, "main", 0);

    let mut compared = 0;
    for row in rows(&items).iter().filter(|r| r["point"] != Value::Null) {
        let path = row["path"].as_str().expect("row has a path");
        let view_row = view
            .items
            .iter()
            .find(|i| i["path"].as_str() == Some(path))
            .unwrap_or_else(|| panic!("stage.p2 has no row for '{}'", path));
        assert_eq!(view_row["class"], "point", "stage.p2 row for {path}");
        assert_eq!(view_row["point"], row["point"], "row {path}");
        assert_eq!(view_row["loc"], row["loc"], "row {path}");
        compared += 1;
    }
    assert_eq!(compared, POINT_PATHS.len());
}

/// `loc` is the site that wrote the object: a point row points at its wiring
/// statement, an instance row at its declaration. Printed as `uri:line` in the
/// text face, and as `-` where there is no position at all.
#[test]
fn the_loc_column_names_the_site_that_wrote_the_object() {
    let _lock = common::lock();
    let (table, _) = frozen(SOURCE);
    let (text, items, _) = mcc::export::instlist::build_inst_list(&table, 0);

    let cases = [
        ("main.R2", line_of("RES R2")),
        ("main.U_IN.R1", line_of("RES R1")),
        ("main.R2.1", line_of("R2.1 -> GND")),
        ("main.U_IN.R1.2", line_of("R1.2 -> GND")),
    ];
    for (path, line) in cases {
        let loc = &row_for(&items, path)["loc"];
        assert_eq!(loc["uri"], URI, "row {path}");
        assert_eq!(loc["line"].as_u64(), Some(line), "row {path}");
        assert_eq!(loc["span"], Value::Null, "a flat position has no extent");
    }

    // The text face: last column is the loc, and a row with no position prints
    // `-` rather than an empty field (an empty field reads as "present and
    // blank", which is a different claim). The root module has no position.
    assert_eq!(text_field(&text, "main")[4], "-");
    assert_eq!(
        text_field(&text, "main.R2.1")[4],
        format!("{URI}:{}", line_of("R2.1 -> GND"))
    );

    // The same five fields in the CSV face, in the same order.
    let (csv, _, _) = mcc::export::instlist::build_inst_list(&table, 4);
    let line = csv
        .lines()
        .find(|l| l.split(',').nth(1) == Some("main.R2.1"))
        .expect("csv row for a component pin");
    let fields: Vec<&str> = line.split(',').collect();
    assert_eq!(fields[0], "-", "a pin owns no arena node: {line}");
    assert_eq!(fields[2], "RES", "the class it was flattened into: {line}");
    assert_eq!(fields[3], row_for(&items, "main.R2.1")["point"]);
    assert_eq!(fields[4], format!("{URI}:{}", line_of("R2.1 -> GND")));
}

/// Freeze the real hbl board and hand back its flat table.
///
/// The caller must hold [`common::lock`]: the registry is process-global, and
/// every test in this file builds a project into it.
fn frozen_hbl() -> mcc::InstTable {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    let entry_uri = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);
    let (_inst, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    table
}

// The real board, measured after the batch landed: a synthetic fixture cannot
// exercise the aggregate port spellings below, so the counts are pinned here
// rather than inferred from the toy one (§2.4: an endpoint is not necessarily a
// point).
const HBL_ROWS: usize = 299;
const HBL_INSTANCES: usize = 68;
const HBL_POINTS: usize = 231;
/// Point-class rows that name no physical point: a port's aggregate / bus-member
/// spellings (`main.V1V2.VCC`, `main.DCDC.GND`, …).
const HBL_POINTLESS: usize = 42;

/// The two blocks on a real board, and the promise that no row invents a key it
/// does not have.
#[test]
fn hbl_point_rows_are_appended_and_never_invent_a_key() {
    let _lock = common::lock();
    let table = frozen_hbl();
    let (_, items, count) = mcc::export::instlist::build_inst_list(&table, 0);
    let rows = rows(&items);
    assert_eq!(count, HBL_ROWS);

    // Block boundary: every instance row precedes every point row, and the
    // instance block is exactly the table's module / component rows, in order.
    let boundary = rows
        .iter()
        .position(|r| r["point"] != Value::Null)
        .expect("the board has points");
    let want: Vec<String> = table
        .iter()
        .filter(|(_, e)| matches!(e.kind, InstKind::Module | InstKind::Component))
        .map(|(_, e)| e.path.clone())
        .collect();
    assert_eq!(boundary, want.len(), "instance block size: {}", items);
    let head: Vec<&str> = rows[..boundary]
        .iter()
        .map(|r| r["path"].as_str().expect("row has a path"))
        .collect();
    assert_eq!(head, want, "the instance block moved: {}", items);

    // Rows carrying a point are exactly the point block minus the spellings
    // that name none; nothing outside the point block carries one.
    let tail: Vec<&Value> = rows[boundary..].to_vec();
    let pointless = tail.iter().filter(|r| r["point"] == Value::Null).count();
    assert!(pointless >= 2, "the point-less branch must be exercised");
    assert_eq!(
        (boundary, tail.len() - pointless, pointless),
        (HBL_INSTANCES, HBL_POINTS - HBL_POINTLESS, HBL_POINTLESS),
        "board split changed: {}",
        items
    );
    // An instance row carries no point, and a point row's class is the instance
    // it was flattened into — never a class of its own (there is none).
    assert!(rows[..boundary].iter().all(|r| r["point"] == Value::Null));
    for r in &tail {
        let path = r["path"].as_str().expect("row has a path");
        let owner = path.rsplit_once('.').expect("a point path is nested").0;
        assert_eq!(
            r["class"],
            row_for(&items, owner)["class"],
            "row {path}: the class half must be the owning instance's"
        );
    }
}

/// The cross-artifact lock on the real board, taken row by row rather than by
/// hand: the artifact and the `stage.p2` readout name the same point with the
/// same in-domain key and the same source site (O16's "does not contradict the
/// existing readouts").
#[test]
fn hbl_point_rows_agree_with_the_stage_view() {
    let _lock = common::lock();
    let table = frozen_hbl();
    let (_, items, _) = mcc::export::instlist::build_inst_list(&table, 0);
    let view = mcc::stages::p2::build_p2(&table, "main", 0);

    let mut compared = 0;
    for row in rows(&items) {
        let path = row["path"].as_str().expect("row has a path");
        let Some(view_row) = view.items.iter().find(|i| i["path"].as_str() == Some(path)) else {
            // An entry the readout folds away (a non-physical alias) is not a
            // disagreement: the two projections do not have to carry the same
            // row *set*, only the same key for a row they both carry.
            continue;
        };
        assert_eq!(view_row["point"], row["point"], "row {path}");
        assert_eq!(view_row["loc"], row["loc"], "row {path}");
        if view_row["class"] == "point" {
            compared += 1;
        }
    }
    assert_eq!(compared, HBL_POINTS, "point-class rows compared");

    // And the artifact is stable across two builds in one process, which is the
    // weakest form of the acceptance the views hold to byte for byte.
    let (first, _, _) = mcc::export::instlist::build_inst_list(&table, 0);
    let (second, _, _) = mcc::export::instlist::build_inst_list(&table, 0);
    assert_eq!(first, second, "the artifact is not reproducible");
}
