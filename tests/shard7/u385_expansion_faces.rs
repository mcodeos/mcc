// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the `*expr` list-element splice EXPANSION on the connect faces
//! (U385 engine leg 2b, b4510; `layer-expansion-law.md` §5.1 — ruled over
//! b4514's transparent connect-face read):
//!
//! * A range splice item reads exactly as the spelled-out member list:
//!   `A -> (*q.P[1:2], w1)` fans the same three connections as
//!   `A -> (q.P1, q.P2, w1)` — the mark splices the member sequence flat
//!   into the enclosing list, the paren group included.
//! * The bracket zip cell `[*a.P[1:2], *b.P[1:2]] -> [*c.P[1:2], *d.P[1:2]]`
//!   pairs 4v4 positionally (member-access splices flatten instance-major).
//! * The value/Set face (b4514's `set_from_list`, unchanged) flattens the
//!   starred actual into the call's list at its position.
//! * The star stays lawful ONLY in list-element prefix position: a single-item
//!   starred group `(*X)`, a bare statement-level `*X`, and the subscript
//!   wildcard `q.P[*]` all stay parse-rejected (E2082).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const QUAD: &str = r#"
component QUAD
{
    pins = [
        1:2 = P[1:2]
    ]
}
"#;

/// Codes that are build-info, not a verdict (the shard7 benign set). 944 is
/// the instance-method try-resolve trace (`try_resolve_instance_method`) that
/// always precedes a module-level func call binding — info-level, fires for
/// every spelled func call the same way.
fn benign(c: u32) -> bool {
    matches!(c, 944 | 5054 | 5070 | 5071 | 5072 | 5641 | 5642 | 5643 | 5459)
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
fn splice__paren_group_range_fan_matches_the_spelled_form() {
    // `A -> (*q.P[1:2], w1)` must carry exactly the branches the spelled form
    // carries — the splice flattens at the group's operand list (§5.1).
    let (marked_codes, marked) = build(
        "    QUAD k[1:2]\n    QUAD w[1:1]\n    A -> (*k[1:2].1, w1.1)\n",
        "/mcc/u385/fan-marked.mc",
    );
    let (spelled_codes, spelled) = build(
        "    QUAD k[1:2]\n    QUAD w[1:1]\n    A -> (k1.1, k2.1, w1.1)\n",
        "/mcc/u385/fan-spelled.mc",
    );
    assert_eq!(
        marked_codes, spelled_codes,
        "same verdict as the spelled form"
    );
    assert_eq!(marked, spelled, "same three-way fan as the spelled form");
    // The three connections all share the label A: one equipotential holding
    // the label and the three fan sinks.
    assert_eq!(
        marked,
        vec![vec!["A", "k1.1", "k2.1", "w1.1"]],
        "A fans to k1.1, k2.1, w1.1: {marked:?}"
    );
}

#[test]
fn splice__bracket_zip_pairs_positionally() {
    // `[*a.P[1:2], *b.P[1:2]] -> [*c.P[1:2], *d.P[1:2]]`: the member-access
    // splices flatten instance-major, the two lists zip 4v4.
    let (codes, partition) = build(
        "    QUAD s[1:4]\n    QUAD t[1:4]\n    [*s[1:2].1, *s[3:4].1] -> [*t[1:2].1, *t[3:4].1]\n",
        "/mcc/u385/bracket-zip.mc",
    );
    assert_eq!(codes, Vec::<u32>::new(), "the zip must be clean");
    assert_eq!(
        partition,
        vec![
            vec!["s1.1", "t1.1"],
            vec!["s2.1", "t2.1"],
            vec!["s3.1", "t3.1"],
            vec!["s4.1", "t4.1"],
        ]
    );
}

#[test]
fn splice__value_set_face_flattens_into_the_call() {
    // The value/Set face (b4514's set_from_list, unchanged): the starred
    // item's members flatten into the actual list at its position.
    let (codes, partition) = build(
        "    QUAD s[1:2]\n    func drive([a, b]) { a - b }\n    drive([*s[1:2].1])\n",
        "/mcc/u385/set-flatten.mc",
    );
    assert_eq!(codes, Vec::<u32>::new(), "the 2-actual call must bind clean");
    assert_eq!(
        partition,
        vec![vec!["s1.1", "s2.1"]],
        "the flattened actuals bind a, b positionally: {partition:?}"
    );
}

#[test]
fn splice__single_item_starred_group_stays_rejected() {
    // `(*X)` alone: the star is lawful only in list-element PREFIX position
    // (§5.1) — a one-item starred group is the sealed form (E2082).
    let (codes, _) = build(
        "    QUAD s[1:2]\n    (*s[1:2])\n",
        "/mcc/u385/single-star-group.mc",
    );
    assert!(
        codes.contains(&2082),
        "(*X) must stay parse-rejected: {codes:?}"
    );
}

#[test]
fn splice__bare_statement_star_stays_rejected() {
    // Bare statement-level `*X`: not a list-element position — E2082.
    let (codes, _) = build(
        "    QUAD s[1:2]\n    *s[1:2]\n",
        "/mcc/u385/bare-star.mc",
    );
    assert!(
        codes.contains(&2082),
        "bare *X must stay parse-rejected: {codes:?}"
    );
}

#[test]
fn splice__subscript_wildcard_stays_rejected() {
    // `name[*]` (canon §15.1) — the comma-list arms must not reopen it.
    let (codes, _) = build(
        "    QUAD s[1:4]\n    A -> s[*]\n",
        "/mcc/u385/star-subscript.mc",
    );
    assert!(
        codes.contains(&2082),
        "q.P[*] must stay parse-rejected: {codes:?}"
    );
}
