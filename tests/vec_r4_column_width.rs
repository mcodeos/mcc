// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! R4 column-width consistency in `[...]` connection lists (vec-arch.md
//! §4.1.1).
//!
//! Every element of a connection list must be single-column (point / column
//! vector, left == right) or all double-column (two-pin row / node). A mix —
//! a single-column element silently spanning both columns of the node
//! (`[A, R101]` → `node{[A,R101.1] | [A,R101.2]}`) — is rejected with E2907.
//! The `_` placeholder lead is exempt: it inherits the sibling column width
//! (R4 `_` placeholder carve-out), so `[_, R101]` stays a legal node.
//!
//! The mix is detected per **element**, not from the merged operand shape,
//! because shape-by-use (§8.9.6.3) presents a declared scalar port as empty
//! (unknown width) at Pass1 — in `[A, R101]` the port `A` drops out of the
//! merged `Node` and the mix would be invisible (the current code silently
//! left R101.1 floating). Both the declared-port arm (`A` → empty →
//! single-column by declaration) and the point arm (a 1-pin component →
//! `Point`) are exercised below.

mod common;

use mcc::{McIds, McURI};

const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";
const RES1: &str = "component RES1 {\n    pins = [\n        1 = 1\n    ]\n}\n";

/// Build `top` from `src` and return the emitted diagnostic codes (sorted).
fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/r4-column-width.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// `[A, R101]` (declared scalar port + two-pin) against a 2*2 target: R4
/// forbids the mix — `A` would silently span both columns of R101. `A` is a
/// shape-by-use port, so the mix must be caught per-element.
#[test]
fn mixed_port_two_pin_is_rejected() {
    let src = format!(
        "{RES2}module top {{\n    io A\n    io B\n    io C\n    RES2 R101\n    [A, R101] -> [B, C]\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_COLUMN_WIDTH_MIXED),
        "`[A, R101]` must fire E2907; got codes: {codes:?}"
    );
}

/// `[R201, R101]` (1-pin component point + two-pin): same mix, but the
/// single-column element is a `Point` (not a shape-by-use port) — exercises
/// the point classification arm.
#[test]
fn mixed_single_pin_component_two_pin_is_rejected() {
    let src = format!(
        "{RES1}{RES2}module top {{\n    io B\n    io C\n    RES1 R201\n    RES2 R101\n    [R201, R101] -> [B, C]\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_COLUMN_WIDTH_MIXED),
        "`[R201, R101]` must fire E2907; got codes: {codes:?}"
    );
}

/// `[_, R101]` lead + two-pin: the `_` inherits the sibling column width, so
/// this stays a legal node 2*2 — no E2907.
#[test]
fn lead_plus_two_pin_is_legal() {
    let src = format!(
        "{RES2}module top {{\n    io GND\n    RES2 R101\n    [_, R101] -> [GND, GND]\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHAPE_COLUMN_WIDTH_MIXED),
        "`[_, R101]` must be legal; got codes: {codes:?}"
    );
}

/// `[R101, R102]` all two-pin: an `R ⊕ R` node 2*2 — legal.
#[test]
fn two_two_pin_is_legal() {
    let src = format!(
        "{RES2}module top {{\n    io GND\n    RES2 R101\n    RES2 R102\n    [R101, R102] -> [GND, GND]\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHAPE_COLUMN_WIDTH_MIXED),
        "`[R101, R102]` must be legal; got codes: {codes:?}"
    );
}

/// `[A, B, C]` all points: a column 3*1 — legal.
#[test]
fn all_points_is_legal() {
    let src = "module top {\n    io A\n    io B\n    io C\n    [A, B, C] -> [GND, GND, GND]\n}\n";
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHAPE_COLUMN_WIDTH_MIXED),
        "`[A, B, C]` must be legal; got codes: {codes:?}"
    );
}
