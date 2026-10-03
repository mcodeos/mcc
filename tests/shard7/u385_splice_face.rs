// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the `*expr` list-element splice prefix (U385 engine leg 2b,
//! `layer-expansion-law.md` §5):
//!
//! * The marked form parses where the unmarked grammar had no derivation
//!   (E2082 before the leg); `[*a, b]` pairs exactly like the written-out
//!   `[a, b]` — the star is transparent on the connect face, whose list
//!   elements already contribute their expanded members.
//! * The subscript wildcards stay shut without a dedicated arm: `name[*]`
//!   and `S[[*]]` still parse-reject (canon §15.1 covers every spelling).
//!
//! The grammar arm lives in mcast (`mc_list_item: MCOP_MULTI mc_list_item`,
//! node `MCAST_OPD_SPLICE`); the value/Set face dissolves the splice at
//! assembly (`McParamValue::set_from_list`), the connect face unwraps it in
//! `McPhrase::new`'s square-vector arm.

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
fn splice__scalar_items_pair_like_written_out() {
    // The ur-text connect form (nets.mc:478 `-> [*KP4, WPA1]`): splice of
    // scalars is the written-out list — same nets, zero new diagnostics.
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2} -> [*b.P1, b.P2]\n",
        "/mcc/u385/splice-scalars.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert_eq!(
        partition,
        vec![vec!["a.1", "b.1"], vec!["a.2", "b.2"]]
    );
}

#[test]
fn splice__unmarked_control_is_the_same_partition() {
    let (codes, partition) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2} -> [b.P1, b.P2]\n",
        "/mcc/u385/splice-control.mc",
    );
    assert_eq!(codes, Vec::<u32>::new());
    assert_eq!(
        partition,
        vec![vec!["a.1", "b.1"], vec!["a.2", "b.2"]]
    );
}

#[test]
fn splice__wildcard_subscript_stays_banned() {
    // `name[*]` (canon §15.1) — the splice leg must not reopen it.
    let (codes, _) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2} -> b.P[*]\n",
        "/mcc/u385/splice-wildcard-sub.mc",
    );
    assert!(!codes.is_empty(), "name[*] must stay rejected");
}

#[test]
fn splice__doubled_bracket_wildcard_stays_banned() {
    // `[[*]]` — star is not an mc_ida_run_atom; the ban covers the doubled
    // spelling without a dedicated arm.
    let (codes, _) = build(
        "    a::QUAD()\n    b::QUAD()\n    a{P1,P2} -> b.P[[*]]\n",
        "/mcc/u385/splice-wildcard-dbl.mc",
    );
    assert!(!codes.is_empty(), "S[[*]] must stay rejected");
}
