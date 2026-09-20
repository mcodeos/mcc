// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ U105 · top-level C5 uncluttering may not cost a connection.
//!
//! Why this test exists
//! `drop_top_passives` (`src/viz/layout/rails.rs`) removed *every* two-pin
//! passive from the top layer as soon as a block shared that layer, then deleted
//! the nets it had drained. A passive in **series** between two blocks is the
//! only thing joining them, so `A.OP -> RES(10k) -> B.IP` came out of the
//! drawing as two boxes with **dangling pins**, while the netlist stayed
//! connected: the drawing could no longer be returned to electrical truth
//! (`doc/viz/requirement-design.md` D1) and one drawing was no longer one
//! netlist (E1). Nothing was red — C5 is silent about what it removed, and no
//! gate looked at the root layer's items.
//!
//! What is compared (do not weaken)
//! The **same two blocks twice** — the fixture's `series` top and its `direct`
//! control — read from the drawing face itself (`stage.viz`, the sole producer
//! of it, ruling b3477):
//!
//! 1. the control's two pins and its edge must be drawn (if this ever goes
//!    away, the assertions below are measuring nothing);
//! 2. the series layer must draw both blocks' pins and both edges, and the
//!    edges' own endpoints must be the real pins — not one edge per block with
//!    a dangling end: the connection survives, and the passive that carries it
//!    is drawn too (`series._R1`, kind `two_pin`);
//! 3. the axis is **the connection, not the symbol**: the passive is kept
//!    because its nets would fall below two endpoints without it — a shunt
//!    whose removal keeps every net at ≥2 endpoints is still dropped, and that
//!    half is read off the hbl board (`C5: dropped 2 … kept 2`, see the batch
//!    log), not from this fixture.
//!
//! Keys are compared as canonical paths, **qualified by the top module of the
//! load** (`direct.A.OP`, `series._R1.1`), never as refdes or drawn labels:
//! `_R1` is an allocation-order name and would pin the test to the source order.

#![allow(non_snake_case)]

use std::path::PathBuf;

use crate::common;
use mcc::stages::{read, StageSeg};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/top_series_passive")
}

/// One drawn item of the root layer. A drawn object without a canonical path of
/// its own (an edge carries its endpoints instead, design §11.1 / M5) keeps an
/// empty `path` rather than being dropped by the reader.
#[derive(Debug, PartialEq, Eq)]
struct Item {
    class: String,
    path: String,
    kind: String,
    /// For an edge: the canonical paths of the two endpoints it joins.
    ends: Vec<String>,
}

/// Read the drawing face for one top module of the fixture.
fn root_layer(top: &str) -> Vec<Item> {
    // The mcc_* workspace is global state; hold the shard-wide lock for the
    // whole init+load+build (U136).
    let _guard = common::lock();
    let root = fixture_dir();
    let entry = root.join("src/main.mc");
    let entry_uri = entry.to_string_lossy().into_owned();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&root);
    let loaded = read::load(&entry_uri, Some(top), &[]).expect("load top_series_passive");
    let view = read::build_segment(StageSeg::Viz, &loaded);
    view.items
        .iter()
        .filter_map(|it| {
            let class = it.get("class")?.as_str()?.to_string();
            let path = it
                .get("path")
                .and_then(|p| p.as_str())
                .or_else(|| it.pointer("/canon_key/path").and_then(|p| p.as_str()))
                .unwrap_or_default()
                .to_string();
            let kind = it
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or_default()
                .to_string();
            let mut ends: Vec<String> = ["from", "to"]
                .iter()
                .filter_map(|slot| it.get(slot))
                .filter_map(|a| a.as_array())
                .flatten()
                .filter_map(|e| e.get("path").and_then(|p| p.as_str()))
                .map(str::to_string)
                .collect();
            ends.sort();
            Some(Item {
                class,
                path,
                kind,
                ends,
            })
        })
        .collect()
}

/// Every path of one class, in the root layer, sorted.
fn paths_of_class(items: &[Item], class: &str) -> Vec<String> {
    let mut v: Vec<String> = items
        .iter()
        .filter(|i| i.class == class)
        .map(|i| i.path.clone())
        .collect();
    v.sort();
    v
}

/// Every drawn edge as its two endpoint paths (each edge sorted, list sorted).
fn edges(items: &[Item]) -> Vec<Vec<String>> {
    let mut v: Vec<Vec<String>> = items
        .iter()
        .filter(|i| i.class == "segment")
        .map(|i| i.ends.clone())
        .collect();
    v.sort();
    v
}

/// One test body: the mcc workspace is global state, so both tops are read in
/// the same test rather than in two tests that could interleave.
#[test]
fn top_series_passive__connection_survives_the_top_level_uncluttering() {
    // Control: two blocks joined directly. C5 never touches it.
    let direct = root_layer("direct");
    assert_eq!(
        paths_of_class(&direct, "pin"),
        vec!["direct.A.OP".to_string(), "direct.B.IP".to_string()],
        "control: the direct connection must draw both pins"
    );
    assert_eq!(
        edges(&direct),
        vec![vec!["direct.A.OP".to_string(), "direct.B.IP".to_string()]],
        "control: the direct connection must draw one edge between the two pins"
    );

    // Under test: the same connection through a series passive.
    let series = root_layer("series");
    assert_eq!(
        paths_of_class(&series, "pin"),
        vec![
            "series.A.OP".to_string(),
            "series.B.IP".to_string(),
            "series._R1.1".to_string(),
            "series._R1.2".to_string(),
        ],
        "U105: the series passive carries the connection — its nets and both \
         blocks' pins must be drawn, not the pins left dangling"
    );
    assert_eq!(
        edges(&series),
        vec![
            vec!["series.A.OP".to_string(), "series._R1.1".to_string()],
            vec!["series.B.IP".to_string(), "series._R1.2".to_string()],
        ],
        "U105: the series connection is two edges (A.OP~_R1, _R1~B.IP), each \
         ending on the pin it is electrically joined to"
    );
    assert!(
        series
            .iter()
            .any(|i| i.class == "box" && i.kind == "two_pin"),
        "U105: the passive that carries the connection must be drawn itself: {series:?}"
    );
}
