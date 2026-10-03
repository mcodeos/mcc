// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the formal-parameter faces of func-body substitution (U385 engine
//! leg E3, `layer-expansion-law.md` §6):
//!
//! * **Whole-set face** — a vector formal referenced by its bare base name
//!   (`IN`) or its declared spelling (`kin[4][l, r]`) consumes the whole
//!   bound member_set; the product pairs like the written actual (a
//!   per-member list), not an anonymous merged bus.
//! * **Member-index face** — `IN[2]` in the body selects the n-th lane of
//!   the formal's bound member_set; the remaining lanes stay unconnected
//!   (the caller passes them, the body never consumes them).
//! * **Definition face** — every formal spelling in a body parses without
//!   floating-label (E3136) or dropped-stmt (E3134) noise; the body's shape
//!   verdict happens at the call site.
//! * **Total-count law untouched** — a scalar formal against a wider list
//!   still refuses (E4007); the leg adds no auto-fan.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const QUAD: &str = r#"
component QUAD
{
    pins = [
        1:4 = P[1:4]
    ]
}
"#;

/// Codes that are build-info, not a verdict (the shard7 benign set). 944 is
/// the instance-method try-resolve trace that always precedes a module-level
/// func call binding.
fn benign(c: u32) -> bool {
    // 5641 (UNUSED_PARAM_OR_PORT) is deliberately NOT benign here: the
    // used-count face is part of the leg — a formal consumed through the
    // whole-set or index spelling must not report unused.
    matches!(c, 944 | 5054)
}

/// Build `main` with the given body and return (non-benign codes sorted,
/// net partition as a sorted list of sorted member lists — net names are
/// synthesized, so the claim is about the grouping of points).
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!("{QUAD}module main {{\n{body}\n}}\n");
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

#[test]
fn formal__whole_set_base_name_pairs_like_written_list() {
    // `IN - [SIG, SIG]` with `IN[1:2]` bound to `[a.P1, a.P2]`: the whole-set
    // product is a per-member list pairing lane-wise, exactly the written
    // connect. No shape refusal, no dropped stmt, no floating label — and
    // both lanes join (the single SIG actual is one net shared by both
    // lanes; b.P2 stays unconnected).
    let (codes, partition) = build(
        r#"    func f(IN[1:2], SIG) {
        IN - [SIG, SIG]
    }
    a::QUAD()
    b::QUAD()
    f([a.P1, a.P2], b.P1)
"#,
        "/mcc/u385/formal-whole-set.mc",
    );
    assert_eq!(codes, Vec::<u32>::new(), "whole-set face must connect clean");
    // The written-out control `[a.P1, a.P2] - [b.P1, b.P1]` joins the same
    // single net — the bare-list `-` face at a module's top level is the
    // fan-join, and the product is exactly the written form.
    assert!(partition == vec![vec!["a.1", "a.2", "b.1"]], "actual={partition:?}");
}

#[test]
fn formal__declared_spelling_is_the_whole_set_face() {
    // The compound form `kin[4][l, r]` written in the body is the declared
    // spelling of the formal — a whole-set reference, not an index chain.
    let (codes, partition) = build(
        r#"    func h(kin[4][l,r], SIG) {
        kin[4][l,r] - [SIG, SIG]
    }
    a::QUAD()
    b::QUAD()
    h([a.P1, a.P2], b.P1)
"#,
        "/mcc/u385/formal-declared-spelling.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert!(partition == vec![vec!["a.1", "a.2", "b.1"]], "actual={partition:?}");
}

#[test]
fn formal__member_index_selects_one_lane() {
    // `IN[2] - SIG` consumes only the second lane: a.P2 joins b.P1; the
    // passed but unconsumed a.P1 / a.P3 stay unconnected, and `IN[2]` is
    // not a floating label (E3136) nor a dropped stmt (E3134).
    let (codes, partition) = build(
        r#"    func g(IN[1:3], SIG) {
        IN[2] - SIG
    }
    a::QUAD()
    b::QUAD()
    g([a.P1, a.P2, a.P3], b.P1)
"#,
        "/mcc/u385/formal-member-index.mc",
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "index face must not refuse the pair; got {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "the indexed formal spelling is not a floating label; got {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_STMT_DROPPED),
        "the index-face stmt must not drop; got {codes:?}"
    );
    // (Unconnected instance pins carry no diagnostic in this world — the
    // claim is the partition: only the consumed lane joins.)
    assert!(partition == vec![vec!["a.2", "b.1"]], "actual={partition:?}");
}

#[test]
fn formal__scalar_against_wider_list_still_refuses() {
    // Total-count law untouched: one scalar formal cannot fan into a
    // 2-wide list (no implicit auto-expansion).
    let (codes, partition) = build(
        r#"    func f(A, B) {
        A - [B, B]
    }
    a::QUAD()
    b::QUAD()
    f(a.P1, b.P1)
"#,
        "/mcc/u385/formal-scalar-no-autofan.mc",
    );
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "1-vs-2 must still refuse; got {codes:?}"
    );
    assert_eq!(partition, Vec::<Vec<String>>::new());
}

#[test]
fn formal__plain_scalar_pair_connects() {
    // Control: the pre-existing plain-formal path stays clean.
    let (codes, partition) = build(
        r#"    func g(A, B) {
        A - B
    }
    a::QUAD()
    b::QUAD()
    g(a.P1, b.P1)
"#,
        "/mcc/u385/formal-scalar-control.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert!(partition == vec![vec!["a.1", "b.1"]], "actual={partition:?}");
}

