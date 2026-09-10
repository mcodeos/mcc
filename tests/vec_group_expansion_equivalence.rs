// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! C1 lock — a `(,)` group expands into **statements**, not into a new
//! association (R0 ruling C1, `mcd/doc/vector-conn-r0-implementation-design.md`
//! §4.C1; `mcrule.md` §10.6).
//!
//! R0 (source order and operator fidelity) governs the connection-phrase
//! representation. The `(,)` group is a **statement-separator** construct, so
//! expanding `opd1 op1 (s1, .., sN) op2 opd2` into N statements that share
//! `opd2` is not a re-association and not a member insertion — it is outside
//! R0's jurisdiction. The companion half of the ruling is that
//! `flatten_series_dir` is an *idempotent restoration* of the form the parser
//! already produces for a same-direction chain, not a new transform.
//!
//! Both halves are locked here by **equivalence**: the group form and its
//! handwritten expansion must produce the same diagnostics and the same net
//! partition. That is what it means for the expansion to be "the statements
//! written out" — no more and no less.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Two-pin resistor mirror (same declaration the `vec_series_rowzip` group-chain
/// cell uses), pins `1 = 1` / `2 = 2`.
const RES2: &str = "component RES2(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Build `main` and return (diagnostic codes sorted, net partition).
///
/// The net partition is normalized to a sorted list of sorted member lists:
/// net *names* are not part of the claim (they are synthesized), the
/// **grouping of points** is.
fn build(statements: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{RES2}module main {{\n    RES2 R101(1), R102(1), R103(1), R104(1), R105(1), R106(1)\n{statements}\n}}\n"
    );
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

/// `opd1 - (s1, s2) + opd2` must equal the two statements written out: the
/// shared `opd2` is the same instance on both, so its pins land on the same
/// nets either way. Verified on diagnostics *and* net partition.
#[test]
fn group_expansion__equals_handwritten_statements() {
    let (group_codes, group_nets) = build(
        "    R101 - (R102 - R103, R104 - R105) + R106",
        "/mcc/group-expansion.mc",
    );
    let (flat_codes, flat_nets) = build(
        "    R101 - R102 - R103 + R106\n    R101 - R104 - R105 + R106",
        "/mcc/group-expansion-flat.mc",
    );

    assert_eq!(
        group_codes, flat_codes,
        "group form and handwritten form must report the same diagnostics"
    );
    assert_eq!(
        group_nets, flat_nets,
        "group form and handwritten form must produce the same net partition"
    );
    // Guard against a vacuous pass: the equivalence must not be "both empty".
    assert!(
        group_nets.iter().flatten().count() >= 8,
        "expected a real net partition, got {group_nets:?}"
    );
}

/// The same-direction half of the ruling: a chain spliced through a group
/// (`R101 - (R102 - R103)`) behaves identically to the chain written without
/// the group. `flatten_series_dir` re-absorbs a same-direction `Series` that
/// the splice re-exposed; it must not change the wiring.
#[test]
fn group_expansion__same_direction_chain_is_restored() {
    let (grouped, grouped_nets) = build("    R101 - (R102 - R103)", "/mcc/group-samedir.mc");
    let (plain, plain_nets) = build("    R101 - R102 - R103", "/mcc/group-samedir-flat.mc");

    assert_eq!(grouped, plain, "same diagnostics");
    assert_eq!(grouped_nets, plain_nets, "same net partition");
    assert!(
        grouped_nets.iter().flatten().count() >= 3,
        "expected a real net partition, got {grouped_nets:?}"
    );
}

/// A group whose branches disagree on shape must not be silent — the expansion
/// is per-branch, so a divergent branch is still judged (and rejected) rather
/// than quietly dropped in favour of its well-formed sibling.
///
/// Note this is a **characterization** cell: the shape divergence here is
/// reported as `E4005` (the branches converge on a `+`), and the branches are
/// not equivalent to either handwritten half. What C1 requires is only that the
/// failure is *reported*; how the rejected branch is recovered is not part of
/// the ruling.
#[test]
fn group_expansion__branch_mismatch_still_reports() {
    let (codes, _) = build(
        "    io A\n    io B\n    R101 - (R102 - R103, [A, B] - [A, B]) + R106",
        "/mcc/group-branch-mismatch.mc",
    );
    let shape_errors: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|c| {
            *c == mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH
                || *c == mcc::errcodes::CONN_PARALLEL_SHAPE_MISMATCH
        })
        .collect();
    assert!(
        !shape_errors.is_empty(),
        "a divergent branch must still be reported; got {codes:?}"
    );
}
