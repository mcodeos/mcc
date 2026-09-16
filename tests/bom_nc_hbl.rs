// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the BOM's NC bucket on the real hbl board. `Crystal2.DST310S(NC) X6`
// must mark `X6` and everything materialized inside it: the named body product
// `X6.R442` and the auto-named load caps `_C4` / `_C5`, which carry no `X6.` prefix
// and are reached through the expansion's nesting chain. The fitted rows keep their
// parts and say so.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::path::PathBuf;

use mcc::McIds;
use serde_json::Value;

const EXPECTED_ROWS: usize = 15;

/// Build the BOM of the fixture board. The caller must hold [`common::lock`]:
/// the registry is process-global, and the two tests here load the same project.
fn hbl_bom() -> Value {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    let entry_uri = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);
    let (tree, _table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let (_, items, _) = mcc::export::bom::build_bom(&tree, &arena, &store, "main", 1);
    items
}

fn rows(items: &Value) -> Vec<&Value> {
    items
        .as_array()
        .expect("items must be an array")
        .iter()
        .collect()
}

fn refdes<'a>(row: &'a Value) -> Vec<&'a str> {
    row["refdes"]
        .as_array()
        .expect("refdes must be an array")
        .iter()
        .map(|s| s.as_str().expect("refdes entries are strings"))
        .collect()
}

#[test]
fn bom_nc_hbl__not_fitted_parts_get_their_own_rows() {
    let _lock = common::lock();
    let items = hbl_bom();

    let marked: Vec<(String, Vec<&str>)> = rows(&items)
        .iter()
        .filter(|r| r["nc"] == true)
        .map(|r| {
            (
                r["class"].as_str().unwrap_or_default().to_string(),
                refdes(r),
            )
        })
        .collect();

    // The microphone module and the crystal: `wm7121` is not fitted, and so is
    // everything the crystal's body materializes, named or auto-named.
    assert_eq!(
        marked,
        [
            ("W".to_string(), vec!["wm7121"]),
            ("X".to_string(), vec!["X6", "X6.R442"]),
            ("_".to_string(), vec!["_C1", "_C4", "_C5", "_R1"]),
        ],
        "unexpected not-fitted rows: {items}"
    );
    assert_eq!(
        rows(&items).len(),
        EXPECTED_ROWS,
        "one more row than the all-fitted baseline: {items}"
    );
}

#[test]
fn bom_nc_hbl__fitted_rows_keep_their_parts() {
    let _lock = common::lock();
    let items = hbl_bom();

    // Every row carries the marker, and the fitted classes are untouched.
    assert!(
        rows(&items).iter().all(|r| r["nc"].is_boolean()),
        "every row carries the marker: {items}"
    );
    let c = rows(&items)
        .into_iter()
        .find(|r| r["class"] == "C" && r["nc"] == false)
        .expect("a fitted C row");
    assert_eq!(refdes(c), ["C1", "C4", "C5", "C8"]);

    // A designator is module-relative, so a name that is not fitted in one
    // module and fitted in another shows up in both buckets. The fitted `_`
    // row still lists every name it listed before.
    let fitted_underscore = rows(&items)
        .into_iter()
        .find(|r| r["class"] == "_" && r["nc"] == false)
        .expect("a fitted underscore row");
    assert_eq!(refdes(fitted_underscore).len(), 19);
}
