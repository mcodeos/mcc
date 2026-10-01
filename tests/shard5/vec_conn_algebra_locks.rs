// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection-algebra law locks (U372, `curly-wrapper-transpose-design.md` §5).
//!
//! The design doc's law tables (L1–L23) carry a 现状 column: 已对齐 faces are
//! locked here **green**, on the net partition, so every law cell that the
//! engine already honors has an executable anchor. Cells whose current state
//! is a **leg target** (leg1 wrapper dissolution, leg2 label-row face order,
//! leg3 node transpose) are deliberately absent — they land with their leg, so
//! the shard never carries a red that documents a known defect.
//!
//! Probe census source: design doc §2 (56 examples, mcc @ b4377); the `#N`
//! references in the comments point into that table.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The U372 §2 probe fixture, in-process: a two-pin device (`T{A,K}`) twice,
/// plus the io label rail the probes wired against.
const FIXTURE: &str = "component T { pins = [ 1 = A\n    2 = K ] }\nmodule main {\n    io L0\n    io L1\n    io L2\n    io L3\n    io L4\n    io L9\n    io VCC\n    io GND\n    io P\n    io Q\n    io R\n    io S\n    T D1\n    T D2\n{}\n}\n";

/// Build the fixture with `statements` as the module body tail and return
/// (sorted diagnostic codes, normalized net partition) — same normalization as
/// `vec_r0_operator_fidelity`: net names are synthesized, the **grouping of
/// points** is the claim.
fn build(statements: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = FIXTURE.replace("{}", statements);
    let uri = "/mcc/u372-conn-law.mc";
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
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

/// True when some net carries both points.
fn wired(nets: &[Vec<String>], a: &str, b: &str) -> bool {
    nets.iter()
        .any(|n| n.iter().any(|p| p == a) && n.iter().any(|p| p == b))
}

/// L18 zip 位配对律 (#1, #4): a member vector zips **positionally** against the
/// other side — `{A,K}` lanes pair 1:1, they do not merge onto one endpoint.
/// The explicit-list spelling is the same vector (L14 基挂形).
#[test]
fn conn_law__member_vector_zips_positionally() {
    for (tag, stmt) in [
        ("curly", "    D1{A,K} - D2{A,K}"),
        ("list", "    D1{A,K} - [D2.A, D2.K]"),
    ] {
        let (codes, nets) = build(stmt);
        assert!(
            !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
            "{tag}: two lanes vs two lanes must zip clean; got {codes:?}"
        );
        assert!(
            wired(&nets, "D1.1", "D2.1") && wired(&nets, "D1.2", "D2.2"),
            "{tag}: zip must pair near-with-near; got {nets:?}"
        );
        assert!(
            !wired(&nets, "D1.1", "D2.2"),
            "{tag}: lanes must not cross; got {nets:?}"
        );
    }
}

/// L10 基数配对律 (#3): a transposed explicit list is a **row** (1×2); against
/// a two-lane vector the cardinality refuses with E4007 and nothing wires.
#[test]
fn conn_law__transposed_list_row_vs_two_lanes_is_refused() {
    let (codes, nets) = build("    D1{A,K} - [D2.A, D2.K]'");
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "row 1*2 vs column 2*1 must refuse with E4007; got {codes:?}"
    );
    assert!(
        !nets.iter().any(|n| n.iter().any(|p| p.starts_with("D1.")
            || p.starts_with("D2."))),
        "a refused statement wires nothing; got {nets:?}"
    );
}

/// L4 面序律, device side (#22): a row operand is a **directed two-terminal** —
/// series concatenation touches only the near face; the far face continues the
/// chain. `L0 - D1 - L9` wires L0→D1's left face (pin 1) and L9→its right face
/// (pin 2), and the two faces never short together.
#[test]
fn conn_law__device_row_series_touches_only_near_faces() {
    let (codes, nets) = build("    L0 - D1 - L9");
    assert!(
        wired(&nets, "L0", "D1.1") && wired(&nets, "L9", "D1.2"),
        "the chain must touch the near faces; got {nets:?}"
    );
    assert!(
        !nets.iter().any(|n| n.contains(&"L0".to_string())
            && n.contains(&"L9".to_string())),
        "the device's two faces must not short into one net; got {nets:?}"
    );
}

/// L3 单点恒等律, spelling side (#23, #27, #28): the transpose wrapper and the
/// group form on a single-point label are the identity — and (unlike the `^`
/// spellings, whose E2903 noise leg1 narrows) these spellings are already
/// silent.
#[test]
fn conn_law__label_wrapper_identities_are_silent() {
    for (tag, stmt) in [
        ("apos", "    L1' - L2"),
        ("paren", "    (L1) - L2"),
        ("paren_apos", "    (L1)' - L2"),
    ] {
        let (codes, nets) = build(stmt);
        assert!(
            !codes.contains(&mcc::errcodes::SHAPE_REVERSE_NOOP),
            "{tag}: single-point identity must not warn; got {codes:?}"
        );
        assert!(
            wired(&nets, "L1", "L2"),
            "{tag}: the wrapper must be the identity; got {nets:?}"
        );
    }
}

/// L3, pin side (#30): `D1.pins.1'` is the same single point.
#[test]
fn conn_law__single_pin_transpose_is_identity() {
    let (_, nets) = build("    D1.pins.1' - L1");
    assert!(
        wired(&nets, "L1", "D1.1"),
        "the transposed pin reference must wire as itself; got {nets:?}"
    );
}

/// L2 对合律, group spelling (#9): `(X')'` composes back to the baseline — the
/// group-machine path is the reference truth for the involution (the adjacent
/// spelling `X''` is leg4's grammar leg).
#[test]
fn conn_law__group_involution_composes_to_baseline() {
    let (baseline_codes, baseline_nets) = build("    D1{A,K} - D2{A,K}");
    let (codes, nets) = build("    D1{A,K} - (D2{A,K}')'");
    assert_eq!(
        codes, baseline_codes,
        "the involution must diagnose exactly like the baseline"
    );
    assert_eq!(
        nets, baseline_nets,
        "(X')' must wire exactly like X; got {nets:?} vs {baseline_nets:?}"
    );
}

/// L22 三端选择律 (#16): the pipe-node form is the node-equation truth —
/// `{L1,L3|L2,L4}` assigns the written row pairs as node memberships.
#[test]
fn conn_law__pipenode_selection_is_the_node_equation_truth() {
    let (_, nets) = build("    D1{A,K} - {L1,L3|L2,L4}");
    assert!(
        wired(&nets, "L1", "D1.1") && wired(&nets, "L3", "D1.2"),
        "the pipe-node rows are the node assignments; got {nets:?}"
    );
}

/// L5 嵌套叠加律 (#17, #18, #19): same-orientation list members stack
/// vertically (column+column → N×1), mixed layers flatten — all three
/// spellings are the same 4×1 vector and zip 1:1 against a 4-wide row.
#[test]
fn conn_law__column_stack_flattens_to_one_vector() {
    let expected = vec![
        vec!["L1".to_string(), "P".to_string()],
        vec!["L2".to_string(), "Q".to_string()],
        vec!["L3".to_string(), "R".to_string()],
        vec!["L4".to_string(), "S".to_string()],
    ];
    for (tag, stmt) in [
        ("colstack", "    [[L1,L2],[L3,L4]] - [P,Q,R,S]"),
        ("flat", "    [L1,L2,L3,L4] - [P,Q,R,S]"),
        ("mixed", "    [L1,[L2,L3],L4] - [P,Q,R,S]"),
    ] {
        let (codes, nets) = build(stmt);
        assert!(
            !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
            "{tag}: 4-wide vs 4-wide must zip clean; got {codes:?}"
        );
        assert_eq!(nets, expected, "{tag}: flatten then zip 1:1; got {nets:?}");
    }
}

/// L10, stacked side (#20): a stacked 4×1 column against a two-lane member
/// vector refuses — the user's original asking cell, confirmed by ruling ⑤.
#[test]
fn conn_law__stacked_column_vs_two_lanes_is_refused() {
    let (codes, nets) = build("    D1{A,K} - [[L1,L2],[L3,L4]]");
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "4*1 vs 2 lanes must refuse with E4007; got {codes:?}"
    );
    assert!(
        !nets.iter().any(|n| n.iter().any(|p| p.starts_with("D1."))),
        "a refused statement wires nothing; got {nets:?}"
    );
}

/// L9 形状超限律 (#45): a transpose over a row-stacked 2×2-of-rows operand
/// exceeds the admitted shapes — E2902 fires (list spelling reports at parse
/// phase; the test asserts the code, not the phase).
#[test]
fn conn_law__overwide_transpose_reports_shape_limit() {
    let (codes, _) = build("    [D1',D2']'");
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_TRANSPOSE_LIMIT),
        "transpose over the row-stacked shape must report E2902; got {codes:?}"
    );
}

/// L8 退化反向律, multi-element side (#48): `^` on a multi-element list is the
/// merge face (并网) and keeps its E2903 — the narrowing that leg1 performs
/// exempts only single points, never this shape.
#[test]
fn conn_law__multi_element_reverse_keeps_warning_and_merges() {
    let (codes, nets) = build("    L1 - [VCC,GND]^");
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_REVERSE_NOOP),
        "multi-element退化 ^ keeps E2903 after the leg1 narrowing; got {codes:?}"
    );
    assert!(
        wired(&nets, "L1", "VCC") && wired(&nets, "L1", "GND"),
        "the merge face is the existing behavior; got {nets:?}"
    );
}
