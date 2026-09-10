// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance surface for the `+ Transposed` bridge (S2+S3 of the R0
//! implementation, `mcd/doc/vector-conn-r0-implementation-design.md` §3 / §7.1).
//!
//! `X + Y'` is a **parallel with a transposed operand** — the shunt/bridge form
//! of the face-side law (vec-dianlu.md §1.4 / §5.1). Today the parser folds it
//! into a `Series` (one of the three specialized `+` arms), so the bridge is
//! produced by the lane engine's transposed-series path. S2 removes those arms
//! and S3 gives the engine a real `Parallel` fold — the operator encoding
//! becomes faithful (R0) while the **wiring must not move**.
//!
//! This file exists **before** that work, precisely so the work has an
//! acceptance line: it is green today, and it must still be green afterwards.
//! It is deliberately *not* the `periph.mc:38` net-table check the design draft
//! originally proposed — that line is inside a `//` comment and no live source
//! uses `+ X'` at all, so it is green either way and would judge nothing
//! (see the draft's §3.1 / §6 errata).
//!
//! **What is asserted** is the law, not the current implementation: a
//! degenerate right operand attaches to the face on its **written** side, so
//! writing the cap after the branches bridges their *right* faces, and writing
//! it before bridges their *left* faces. `Unknown`-width operands (`[A, B]` on
//! undeclared `io` ports) never reach the bridge at all — the width gate
//! rejects them — which is why the cells below use declared two-column
//! operands (`[R101, R102]`, two-pin instances).

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Plain two-pin capacitor, pins `1 = 1` / `2 = 2`.
const CAP2: &str = "component CAP2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Plain two-pin resistor, pins `1 = 1` / `2 = 2`.
const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Codes that are build-info, not a verdict (same set the vector-oracle
/// family tolerates).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
}

/// Shape/transpose verdicts a bridge statement may be rejected with.
fn shape_error(c: u32) -> bool {
    matches!(
        c,
        mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH
            | mcc::errcodes::CONN_PARALLEL_SHAPE_MISMATCH
            | mcc::errcodes::CONN_TRANSPOSE_SIZE_MISMATCH
    )
}

/// Build `main` and return (non-benign codes sorted, net partition).
///
/// The partition is normalized to a sorted list of sorted member lists: net
/// *names* are synthesized, so the claim is about the **grouping of points**.
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{CAP2}{RES2}module main {{\n    RES2 R101\n    RES2 R102\n    CAP2 C1\n{body}\n}}\n"
    );
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();

    let mut partition: Vec<Vec<String>> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(_, pts)| {
                    let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
                    ps.sort();
                    ps
                })
                .filter(|ps| !ps.is_empty())
                .collect()
        })
        .unwrap_or_default();
    partition.sort();
    (codes, partition)
}

/// The net that carries `path`, or `None`.
fn net_holding<'a>(nets: &'a [Vec<String>], path: &str) -> Option<&'a Vec<String>> {
    nets.iter().find(|ps| ps.iter().any(|p| p == path))
}

// ── the bridge, on the written side ─────────────────────────────────────────

/// `[R101, R102] + C1'` — the cap is written **after** the branches, so a
/// degenerate (point/column) operand attaches to their **right** faces: the
/// bridge lands on `R101.2` / `R102.2`, and the two cap pins land on two
/// *different* nets (a shunt across the pair, not a short).
#[test]
fn bridge__written_right_lands_on_right_faces() {
    let (codes, nets) = build("    [R101, R102] + C1'", "/mcc/bridge-right.mc");
    assert_eq!(codes, Vec::<u32>::new(), "bridge is quiet; got {codes:?}");

    let c1 = net_holding(&nets, "C1.1").expect("net holding C1.1");
    assert_eq!(
        c1,
        &vec!["C1.1".to_string(), "R101.2".to_string()],
        "C1.1 bridges to the first branch's right face"
    );
    let c2 = net_holding(&nets, "C1.2").expect("net holding C1.2");
    assert_eq!(
        c2,
        &vec!["C1.2".to_string(), "R102.2".to_string()],
        "C1.2 bridges to the second branch's right face"
    );
    assert_ne!(c1, c2, "the two cap pins must not share one net");
}

/// `C1' + [R101, R102]` — same operands, cap written **before** the branches:
/// now the bridge attaches to their **left** faces (`R101.1` / `R102.1`).
/// This mirror is what makes the cell a law check rather than a snapshot.
#[test]
fn bridge__written_left_lands_on_left_faces() {
    let (codes, nets) = build("    C1' + [R101, R102]", "/mcc/bridge-left.mc");
    assert_eq!(codes, Vec::<u32>::new(), "bridge is quiet; got {codes:?}");

    let c1 = net_holding(&nets, "C1.1").expect("net holding C1.1");
    assert_eq!(
        c1,
        &vec!["C1.1".to_string(), "R101.1".to_string()],
        "C1.1 bridges to the first branch's left face"
    );
    let c2 = net_holding(&nets, "C1.2").expect("net holding C1.2");
    assert_eq!(
        c2,
        &vec!["C1.2".to_string(), "R102.1".to_string()],
        "C1.2 bridges to the second branch's left face"
    );
    assert_ne!(c1, c2, "the two cap pins must not share one net");
}

/// `[R101, _] + C1'` — the `_` placeholder lead: the branch column is still two
/// wide (the placeholder inherits the sibling width), so the bridge runs, but
/// only the real branch's pin is available to land on.
#[test]
fn bridge__placeholder_lead_keeps_the_bridge() {
    let (codes, nets) = build("    [R101, _] + C1'", "/mcc/bridge-placeholder.mc");
    assert_eq!(codes, Vec::<u32>::new(), "bridge is quiet; got {codes:?}");
    assert_eq!(
        net_holding(&nets, "C1.1").map(|v| v.as_slice()),
        Some(["C1.1".to_string(), "R101.2".to_string()].as_slice()),
        "C1.1 still bridges to the real branch's right face; got {nets:?}"
    );
}

/// A left operand that is **not** two wide cannot take a bridge: the width gate
/// rejects the statement instead of silently wiring a partial one.
#[test]
fn bridge__narrow_left_operand_is_rejected() {
    let (codes, nets) = build("    R101 - R102 + C1'", "/mcc/bridge-narrow.mc");
    assert!(
        codes.iter().copied().any(shape_error),
        "a non-2-wide left operand must be rejected; got {codes:?}"
    );
    assert!(
        nets.is_empty(),
        "a rejected bridge wires nothing; got {nets:?}"
    );
}
