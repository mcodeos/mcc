// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the BOM of the real hbl board. A row entry is a part, named
// by its own canonical instance path (U53), so a short name that two modules
// reuse stays two parts and never lands in two buckets at once. The NC bucket
// rides on that same identity: `Crystal2.DST310S(NC) X6` must mark `X6` and
// everything materialized inside it — the named body product `X6.R442` and the
// auto-named load caps `_C4` / `_C5`, which carry no `X6.` prefix and are
// reached through the expansion's nesting chain — while the fitted rows keep
// their parts and say so.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::path::PathBuf;

use mcc::McIds;
use serde_json::Value;

/// The board's row count: 13 class rows, two of which are the split `_` class.
const EXPECTED_ROWS: usize = 13;

/// The frozen hbl board: the BOM payload, plus the canonical instance paths the
/// §1.4 `inst-list` projection carries, so a lock can hold both projections to
/// the same key.
struct Hbl {
    items: Value,
    inst_paths: Vec<String>,
}

/// Build the fixture board. The caller must hold [`common::lock`]: the registry
/// is process-global, and the tests here load the same project.
fn hbl() -> Hbl {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    let entry_uri = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);
    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let (_, items, _) = mcc::export::bom::build_bom(&tree, &arena, &store, "main", 1);
    let (listing, _, _) = mcc::export::instlist::build_inst_list(&table, 0);
    // The row block only: a blank line ends it and the definition ledger
    // follows (organization-units-design §10.9), whose lines carry an ident in
    // the same column.
    let inst_paths = listing
        .lines()
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split('\t').nth(1))
        .map(|p| p.to_string())
        .collect();
    Hbl { items, inst_paths }
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

fn row_of<'a>(items: &'a Value, class: &str, nc: bool) -> &'a Value {
    rows(items)
        .into_iter()
        .find(|r| r["class"] == class && r["nc"] == nc)
        .unwrap_or_else(|| panic!("no row for class={class} nc={nc}: {items}"))
}

#[test]
fn bom_nc_hbl__not_fitted_parts_get_their_own_rows() {
    let _lock = common::lock();
    let items = hbl().items;

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

    // The microphone and the crystal: `wm7121` is not fitted, and neither is
    // anything their bodies materialize, named or auto-named. The mic's own
    // parts are the `main.MIC` pair; the crystal's are the `main.MCU513` pair.
    assert_eq!(
        marked,
        [
            ("W".to_string(), vec!["main.MIC.wm7121"]),
            (
                "X".to_string(),
                vec!["main.MCU513.X6", "main.MCU513.X6.R442"]
            ),
            (
                "_".to_string(),
                vec![
                    "main.MCU513._C4",
                    "main.MCU513._C5",
                    "main.MIC._C1",
                    "main.MIC._R1"
                ]
            ),
        ],
        "unexpected not-fitted rows: {items}"
    );
    assert_eq!(
        rows(&items).len(),
        EXPECTED_ROWS,
        "unexpected row count: {items}"
    );
}

#[test]
fn bom_nc_hbl__fitted_rows_keep_their_parts() {
    let _lock = common::lock();
    let items = hbl().items;

    // Every row carries the marker, and the fitted classes are untouched.
    assert!(
        rows(&items).iter().all(|r| r["nc"].is_boolean()),
        "every row carries the marker: {items}"
    );
    assert_eq!(
        refdes(row_of(&items, "C", false)),
        [
            "main.MCU513.C4",
            "main.MCU513.C5",
            "main.MIC.C1",
            "main.SPK.C8"
        ]
    );

    // The `_` class is where the per-module names pile up: 38 auto-named parts
    // survive as 38 entries, none of them from the two not-fitted owners above.
    let fitted_underscore = row_of(&items, "_", false);
    assert_eq!(refdes(fitted_underscore).len(), 38);
    assert!(
        !refdes(fitted_underscore)
            .iter()
            .any(|d| d.starts_with("main.MIC.")
                || *d == "main.MCU513._C4"
                || *d == "main.MCU513._C5"),
        "a not-fitted part stays out of the fitted row: {fitted_underscore}"
    );
}

#[test]
fn bom_u53__a_part_is_named_by_its_own_instance_path() {
    let _lock = common::lock();
    let Hbl { items, inst_paths } = hbl();

    // Every entry is a part of the frozen circuit under the canonical instance
    // path the `inst-list` projection uses (§3.7) — the identity downstream
    // joins on, instead of re-deriving one from a path string.
    for r in rows(&items) {
        for d in refdes(r) {
            assert!(
                inst_paths.iter().any(|p| p == d),
                "not an instance path of the board: {d}"
            );
        }
    }

    let fitted: Vec<&str> = rows(&items)
        .iter()
        .filter(|r| r["nc"] == false)
        .flat_map(|r| refdes(r))
        .collect();
    let marked: Vec<&str> = rows(&items)
        .iter()
        .filter(|r| r["nc"] == true)
        .flat_map(|r| refdes(r))
        .collect();

    // A part that is not fitted cannot put a fitted part's name in the NC
    // bucket: no entry sits in a fitted row and an NC row at once.
    for d in &fitted {
        assert!(!marked.contains(d), "{d} sits in both buckets: {items}");
    }

    // `_C4` names two capacitors: the one inside the converter, and the load cap
    // the not-fitted crystal materializes. Two rows, two entries.
    assert!(fitted.contains(&"main.DCDC._C4"), "{items}");
    assert!(marked.contains(&"main.MCU513._C4"), "{items}");

    // A name reused in several modules no longer merges: the five `_C1` parts
    // of the board are five entries.
    assert_eq!(
        fitted.iter().filter(|d| d.ends_with("._C1")).count(),
        5,
        "the fitted `_C1` parts merged: {items}"
    );
}
