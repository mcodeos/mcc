// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U130 ①③ (ruled 2026-09-20): bare user-func dispatch order + Endpoint-
//! return bridges through the unified engine.
//!
//! ① A bare func call (`make() -> X`) is not a class name, so the CMIE
//! lookup in `instantiate_funccall` misses; the CMIE-miss early return
//! (`failed_classes` + PassThrough) used to sit between that miss and the
//! user-func table lookup, making the whole bare-call face — and with it the
//! Endpoint-return bridge emission — unreachable. The func table is now
//! consulted before the early return.
//!
//! ③ `emit_endpoint_return_bridges` used to hand-build
//! `make_conn_with_provenance` pairs with a pair-by-min common-prefix
//! truncation — a literal R8 violation (connection computation outside the
//! engine). It is retired: the Endpoint-return arm now publishes the
//! substituted return face through the LAST_RETURN_ENDPOINT side channel
//! (the same path instance methods use), and the connection operation
//! happens only at the final connection statement, through the unified
//! engine — so the engine's pairing, shape judgment (count mismatch →
//! E4007, no connection) and interface connect-rule check all apply.
//!
//! Probes run through the test harness (the CLI does not parse func bodies).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const FIXTURE: &str =
    "module main {\n    io V5\n    io X\n    io Y\n    func make() {\n        return V5\n    }\n";

/// Build `main` with `body` appended and return (sorted codes, normalized net
/// partition). Same partition shape as `iface_connect_rule.rs`: net names are
/// synthesized, so the claim is about the grouping of points.
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!("{FIXTURE}{body}\n}}\n");
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

fn in_same_net(partition: &[Vec<String>], a: &str, b: &str) -> bool {
    partition
        .iter()
        .any(|net| net.iter().any(|p| p.ends_with(a)) && net.iter().any(|p| p.ends_with(b)))
}

/// ①+③ together: the bare call must reach the func table (dispatch order),
/// and the Endpoint-return bridge must exist as a real engine connection —
/// `X` and the body's `return V5` land in ONE net. Before the fix the call
/// died at the CMIE-miss early return: PassThrough, no bridge, X floating
/// alone.
#[test]
fn u130__bare_call_bridges_return_endpoint_through_engine() {
    let (codes, partition) = build("    make() -> X", "/mcc/u130-bare-call.mc");
    assert!(
        in_same_net(&partition, "X", "V5"),
        "the bare call must bridge X to the func's return endpoint V5; nets: {partition:?}"
    );
    assert!(
        !codes.contains(&4007),
        "a clean 1:1 bridge must not raise the shape family; got {codes:?}"
    );
}

/// ③'s engine face: a 2-vs-1 bridge is a shape mismatch, judged BY THE
/// ENGINE. The retired hand-built path silently wired the common prefix
/// (X ~ V5) and dropped Y with no code; the unified law reports E4007 and
/// generates no connection.
#[test]
fn u130__endpoint_bridge_count_mismatch_is_e4007_not_silent_truncation() {
    let (codes, partition) = build("    make() -> [X, Y]", "/mcc/u130-bridge-mismatch.mc");
    assert!(
        codes.contains(&4007),
        "the 2-vs-1 bridge must surface the engine's shape judgment (E4007); got {codes:?}"
    );
    assert!(
        !in_same_net(&partition, "X", "V5"),
        "no connection may be generated on a shape-mismatched bridge; nets: {partition:?}"
    );
}
