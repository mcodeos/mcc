// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the BOM's NC bucket. A part designed in but not placed
// (the `NC` construction argument = DNP) must be reported in its own row
// instead of looking like a fitted one, and a not-fitted part is a row even
// when nothing connects to it. Nothing is dropped: the fitted rows keep their
// parts, and the class grouping survives.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};
use serde_json::Value;

const URI: &str = "/mcc/bom-nc.mc";

/// One class with a wired fitted part and a wired not-fitted part, plus a
/// fitted part and a not-fitted part that nothing connects to.
const SPLIT: &str = r#"
component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}
module main(psnk GND)
{
    RES R1
    R1.1 -> GND
    R1.2 -> GND
    RES(NC) R9
    R9.1 -> GND
    R9.2 -> GND
    RES R8
    RES(NC) R7
}
"#;

/// The same shape with every `NC` argument removed: no row may be marked.
const FITTED: &str = r#"
component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}
module main(psnk GND)
{
    RES R1
    R1.1 -> GND
    R1.2 -> GND
}
"#;

/// Freeze the circuit and build the BOM. The caller must hold [`common::lock`]
/// for its whole body: the registry is global state, so a sibling `reset()`
/// landing between the load and the build would strip the class defs.
fn bom(source: &str, format: u8) -> (String, Value, usize) {
    common::reset();
    let uri: McURI = URI.to_string();
    common::load_string(URI, source);
    let ident = McIds::from("main");
    let (tree, _table, arena, store) =
        mcc::mcc_build_flat_with_arena(&ident, &uri, 1).expect("pass2_flat failed");
    mcc::export::bom::build_bom(&tree, &arena, &store, "main", format)
}

fn rows<'a>(items: &'a Value) -> Vec<&'a Value> {
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

fn row<'a>(items: &'a Value, class: &str, nc: bool) -> &'a Value {
    rows(items)
        .into_iter()
        .find(|r| r["class"] == class && r["nc"] == nc)
        .unwrap_or_else(|| panic!("no row for class={class} nc={nc}: {items}"))
}

#[test]
fn bom_nc__one_class_splits_into_a_fitted_and_a_not_fitted_row() {
    let _lock = common::lock();
    let (_, items, count) = bom(SPLIT, 1);

    assert_eq!(count, 2, "one fitted row and one NC row: {items}");
    assert_eq!(refdes(row(&items, "R", false)), ["main.R1"]);
    assert_eq!(refdes(row(&items, "R", true)), ["main.R7", "main.R9"]);
}

#[test]
fn bom_nc__not_fitted_part_is_a_row_even_without_a_connection() {
    let _lock = common::lock();
    let (_, items, _) = bom(SPLIT, 1);

    // R7 is declared NC and wired to nothing. R8 is an ordinary part that is
    // equally unwired: it stays out, as before.
    assert_eq!(refdes(row(&items, "R", true)), ["main.R7", "main.R9"]);
    assert!(
        !refdes(row(&items, "R", false)).contains(&"R8"),
        "an unwired fitted part must stay absent: {items}"
    );
    assert!(
        rows(&items).iter().all(|r| !refdes(r).contains(&"R8")),
        "R8 must not appear in any row: {items}"
    );
}

#[test]
fn bom_nc__a_fully_fitted_board_marks_every_row_fitted() {
    let _lock = common::lock();
    let (_, items, count) = bom(FITTED, 1);

    assert_eq!(count, 1, "one class, one row: {items}");
    assert_eq!(refdes(row(&items, "R", false)), ["main.R1"]);
    assert!(
        rows(&items).iter().all(|r| r["nc"] == false),
        "no row may be marked NC: {items}"
    );
}

#[test]
fn bom_nc__the_nc_row_sorts_after_the_fitted_row_of_its_class() {
    let _lock = common::lock();
    let (_, items, _) = bom(SPLIT, 1);
    let order: Vec<(String, bool)> = rows(&items)
        .iter()
        .map(|r| {
            (
                r["class"].as_str().unwrap_or_default().to_string(),
                r["nc"] == true,
            )
        })
        .collect();

    assert_eq!(
        order,
        [("R".to_string(), false), ("R".to_string(), true)],
        "the NC row stays beside its class, right after the fitted one: {items}"
    );
}

#[test]
fn bom_nc__text_and_csv_carry_the_marker() {
    let _lock = common::lock();
    let (text, _, _) = bom(SPLIT, 0);
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines[0], "# BOM: top=main");
    // Found by content first, then pinned by position: the frame is two lines
    // (title, column header), and an index alone would report the frame's shape
    // while saying nothing about the nc column.
    let header = lines
        .iter()
        .position(|l| l.contains("class") && l.contains("nc") && l.contains("count"))
        .unwrap_or_else(|| panic!("no line names the nc column: {text}"));
    assert_eq!(
        header, 1,
        "the header line is the one right after the title: {text}"
    );
    let fitted = lines
        .iter()
        .find(|l| l.contains("R1"))
        .expect("a fitted row");
    assert!(
        !fitted.contains("NC"),
        "a fitted row carries no NC marker: {fitted}"
    );
    let marked = lines
        .iter()
        .find(|l| l.contains("NC"))
        .expect("a marked row");
    assert!(
        marked.contains("main.R7, main.R9"),
        "the marked row lists both not-fitted parts: {marked}"
    );

    let (csv, _, _) = bom(SPLIT, 4);
    assert_eq!(
        csv.lines().next(),
        Some("class,nc,value,description,package,count,refdes")
    );
    assert!(
        csv.contains("R,false,,,,1,main.R1\n"),
        "fitted csv row: {csv}"
    );
    assert!(
        csv.contains("R,true,,,,2,\"main.R7,main.R9\"\n"),
        "not-fitted csv row: {csv}"
    );
}
