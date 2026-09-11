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
//!
//! Two further cells pin the ruling's *boundary*, which is what decides
//! whether a `Group` can ever be a **lane item** in the lane-by-lane wiring
//! (`collect_one_lane_item`):
//!
//! - A **one-element** group is not a statement list at all — it is the
//!   operand it writes, so it is see-through even inside a lane chain.
//! - A **multi-statement** group still expands to statements when it sits in a
//!   lane-triggering chain, so no multi-opd group ever reaches lane wiring.
//!   The per-lane distribution M11.4 sketched for it is therefore dead: there
//!   is no surviving shape for it to distribute.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Two-pin resistor mirror (same declaration the `vec_series_rowzip` group-chain
/// cell uses), pins `1 = 1` / `2 = 2`.
const RES2: &str =
    "component RES2(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

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

/// Unequal branch counts are legal and expand to the **Cartesian product**
/// (vec-dianlu.md §7.3 rule 4): `(A, B) op (C, D, E)` pairs every branch of one
/// group with every branch of the other, N×M = 6 statements here, each judged
/// independently by §5. Locked by equivalence with the handwritten product,
/// mirroring the sibling cell above.
///
/// This path used to fail silently — `infer_shape_and_upgrade` cleared both
/// operand lists when the counts differed, so the expander saw a zero-branch
/// group and wired nothing at all.
#[test]
fn group_expansion__unequal_branch_counts_take_the_cartesian_product() {
    let (group_codes, group_nets) = build(
        "    (R101, R102) - (R103, R104, R105)",
        "/mcc/group-cartesian.mc",
    );
    let (flat_codes, flat_nets) = build(
        "    R101 - R103\n    R101 - R104\n    R101 - R105\n    R102 - R103\n    R102 - R104\n    R102 - R105",
        "/mcc/group-cartesian-flat.mc",
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
        group_nets.iter().flatten().count() >= 5,
        "expected a real net partition, got {group_nets:?}"
    );
}

/// The **boundary** of the statement-list ruling, lane path: a one-element
/// group is not a statement list (`expand_group_statements` returns `None`
/// unless `opds.len() > 1`), so `(R105')` is the operand `R105'` written with
/// redundant parentheses. A `_` sibling forces the lane-by-lane path, which is
/// exactly where a group could have been mistaken for a per-lane item — the
/// grouped and bare forms must stay identical.
#[test]
fn group_expansion__unary_group_is_see_through_in_a_lane_chain() {
    let (grouped, grouped_nets) = build("    (R105') - [R101, _]", "/mcc/group-unary-lane.mc");
    let (plain, plain_nets) = build("    R105' - [R101, _]", "/mcc/group-unary-lane-flat.mc");

    assert_eq!(grouped, plain, "same diagnostics");
    assert_eq!(grouped_nets, plain_nets, "same net partition");
    // Guard against a vacuous pass: the lane chain must really wire.
    assert!(
        grouped_nets.iter().flatten().count() >= 2,
        "expected a real net partition, got {grouped_nets:?}"
    );
}

/// The same boundary on the other side: a **multi-statement** group is a
/// statement list everywhere, including when its branches carry `_` leads that
/// force the lane-by-lane path, so it expands to statements before member
/// flattening and never becomes a lane item. Locked by equivalence with the
/// statements written out — the `_` only forces the lane path for each
/// expanded statement, it does not turn the group into a lane vector.
#[test]
fn group_expansion__multi_statement_group_expands_in_a_lane_chain() {
    let (grouped, grouped_nets) = build(
        "    ([R101, R102] - [R105, _], [R103, R104] - [R106, _])",
        "/mcc/group-multi-lane.mc",
    );
    let (flat, flat_nets) = build(
        "    [R101, R102] - [R105, _]\n    [R103, R104] - [R106, _]",
        "/mcc/group-multi-lane-flat.mc",
    );

    assert_eq!(grouped, flat, "same diagnostics");
    assert_eq!(grouped_nets, flat_nets, "same net partition");
    // Guard against a vacuous pass: both expanded branches must really wire
    // (the lead lane contributes no pin, so each branch wires exactly one).
    assert!(
        grouped_nets.iter().flatten().count() >= 2,
        "expected a real net partition, got {grouped_nets:?}"
    );
}
