// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the postfix `{{order}}` member-sequence reorder (U385 engine leg 2,
//! `layer-expansion-law.md` §4):
//!
//! * `{{4:1}}` on four members is the reverse — the spec keeps its WRITTEN
//!   spelling order, a range expands descending.
//! * The spec must be a 1-based permutation of the operand's expanded member
//!   count: a short spec (E2911), a repeated position (E2911) and a
//!   non-positive position (E2910) are rejected, and the rejected spec keeps
//!   the operand's written order (identity pairing) — a bad spec neither
//!   silently reorders nor silently drops the face.
//!
//! The grammar arm lives in mcast (MCAST_OPD_REORDER / _RANGE); the judgement
//! runs at Pass2 (`points.rs::judge_reorder`), where the member count is real.

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

/// Codes that are build-info, not a verdict (the shard7 benign set).
fn benign(c: u32) -> bool {
    matches!(c, 5054 | 5070 | 5071 | 5072 | 5641 | 5642 | 5643 | 5459)
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
fn reorder__identity_control_pairs_in_written_order() {
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2,P3,P4} -> b{P1,P2,P3,P4}\n",
        "/mcc/u385/reorder-control.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert_eq!(
        partition,
        vec![
            vec!["a.1", "b.1"],
            vec!["a.2", "b.2"],
            vec!["a.3", "b.3"],
            vec!["a.4", "b.4"]
        ]
    );
}

#[test]
fn reorder__descending_range_is_the_reverse() {
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2,P3,P4}{{4:1}} -> b{P1,P2,P3,P4}\n",
        "/mcc/u385/reorder-reverse.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    // {{4:1}} is the reverse: a_i pairs b_(5-i).
    assert_eq!(
        partition,
        vec![
            vec!["a.1", "b.4"],
            vec!["a.2", "b.3"],
            vec!["a.3", "b.2"],
            vec!["a.4", "b.1"]
        ]
    );
}

#[test]
fn reorder__identity_spelling_pairs_in_written_order() {
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2,P3,P4}{{1,2,3,4}} -> b{P1,P2,P3,P4}\n",
        "/mcc/u385/reorder-identity.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert_eq!(
        partition,
        vec![
            vec!["a.1", "b.1"],
            vec!["a.2", "b.2"],
            vec!["a.3", "b.3"],
            vec!["a.4", "b.4"]
        ]
    );
}

#[test]
fn reorder__short_spec_is_rejected_and_keeps_written_order() {
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2,P3,P4}{{1,3}} -> b{P1,P2,P3,P4}\n",
        "/mcc/u385/reorder-short.mc",
    );
    assert_eq!(codes, vec![2911]);
    assert_eq!(
        partition,
        vec![
            vec!["a.1", "b.1"],
            vec!["a.2", "b.2"],
            vec!["a.3", "b.3"],
            vec!["a.4", "b.4"]
        ]
    );
}

#[test]
fn reorder__repeated_position_is_rejected() {
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2,P3,P4}{{1,1,3,4}} -> b{P1,P2,P3,P4}\n",
        "/mcc/u385/reorder-repeat.mc",
    );
    assert_eq!(codes, vec![2911]);
    assert_eq!(
        partition,
        vec![
            vec!["a.1", "b.1"],
            vec!["a.2", "b.2"],
            vec!["a.3", "b.3"],
            vec!["a.4", "b.4"]
        ]
    );
}

#[test]
fn reorder__zero_position_is_rejected() {
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2,P3,P4}{{0,2,3,4}} -> b{P1,P2,P3,P4}\n",
        "/mcc/u385/reorder-zero.mc",
    );
    assert_eq!(codes, vec![2910]);
    assert_eq!(
        partition,
        vec![
            vec!["a.1", "b.1"],
            vec!["a.2", "b.2"],
            vec!["a.3", "b.3"],
            vec!["a.4", "b.4"]
        ]
    );
}
