// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the grouped structural consumption face of the `[[layer]]` marker
//! (U385 engine
//! leg 2c, `layer-expansion-law.md` §3.3):
//!
//! * A marked reference (`name[[..]]`) pairs **per group**: group k of the
//!   accumulator's right face against group k of the next operand's left
//!   face. The §5.2 row gate counts **groups**, not members.
//! * The flat identity holds: the same statements written unmarked produce
//!   the same net partition — the mark is a toggle for the structural read,
//!   never a shape change.
//!
//! The grouped read lives in the engine (`McIda::expand_grouped` /
//! `marked_group`, `ConcreteOpd::group`, the decomposition arm in
//! `vexpr_step`); the grammar has no new arm (the `[[..]]` layer grammar is
//! leg G1, b4500).

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
fn layer_group__marked_sides_pair_per_group() {
    // Both sides marked, two groups of one: group k pairs group k, zero
    // diagnostics — the structural read runs and stays inside §5.2.
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P[[1:2]]} -> b{P[[3:4]]}\n",
        "/mcc/u385/group-marked-pair.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert_eq!(partition, vec![vec!["a.1", "b.3"], vec!["a.2", "b.4"]]);
}

#[test]
fn layer_group__unmarked_control_is_the_same_partition() {
    // Flat identity (law §3.1): dropping the marks leaves the same nets.
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P[1:2]} -> b{P[3:4]}\n",
        "/mcc/u385/group-flat-control.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert_eq!(partition, vec![vec!["a.1", "b.3"], vec!["a.2", "b.4"]]);
}

#[test]
fn layer_group__marked_against_ungrouped_pairs_group_to_row() {
    // One side marked, the other flat: the grouped side contributes its group
    // count as rows, the flat side one element per group.
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P[[1:2]]} -> b{P3, P4}\n",
        "/mcc/u385/group-vs-flat.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert_eq!(partition, vec![vec!["a.1", "b.3"], vec!["a.2", "b.4"]]);
}

#[test]
fn layer_group__group_rows_gate_refuses_member_count_mismatch() {
    // The row gate counts groups: two groups against a flat single point is
    // not silently broadcast — E4007, no connection (the implicit
    // no-auto-expansion law is unchanged by the mark).
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P[[1:2]]} -> b.P3\n",
        "/mcc/u385/group-rows-gate.mc",
    );
    assert!(
        codes.contains(&4007),
        "2 groups vs 1 flat point must stay refused, got {codes:?}"
    );
    assert!(partition.is_empty(), "no net may be emitted");
}
