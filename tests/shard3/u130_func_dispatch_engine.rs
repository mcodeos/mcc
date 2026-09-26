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
    build_src(&format!("{FIXTURE}{body}\n}}\n"), uri)
}

/// Build an arbitrary source string. Same claim shape as `build`.
fn build_src(src: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
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
///
/// This is also the behavioral lock for the U308 phase split (the shape-level
/// half is `builder.rs::inst_shape__pass2_context_keeps_the_declared_port_width`).
/// `V5` is a declared port, so a return face that inherits the Pass1 opcheck's
/// declared-port tolerance reads the port's width as empty and publishes no
/// bridge at all — measured, and the failure was exactly this cell with
/// `nets: []`.
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

/// U308 probe fixture: a declared vector slice as the return expression.
/// `c[1:2]` is **one** written reference that resolves to the two lanes `c1`
/// and `c2` (`RefVerdict::ResolvedMany` → a reference-face `Group`), so the
/// number of points the return face publishes is directly readable at the call
/// site: a 2-point face pairs 1:1 with `[X, Y]`, a 1-point face trips E4007.
const VEC_RETURN: &str = "\
component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n\
module main {\n    io X\n    io Y\n    RES2 c[1:2](1)\n    func make() {\n        return c[1:2]\n    }\n";

/// U308: the return face of a func that returns a multi-lane reference.
///
/// The return expression is written once (`c[1:2]`) but names two lanes. A
/// reader that takes the **last** member of a `Group` publishes one point and
/// the `2 -> [X, Y]` bridge is a shape mismatch (E4007, no connection). The
/// value face publishes the operand's own right port — one point per lane,
/// each on the lane's host pin — so the call site is replaced by the operand
/// and the ordinary `->` row pairing connects X to the first lane and Y to the
/// second.
#[test]
fn u308__multi_lane_return_face_publishes_every_lane() {
    let (codes, partition) = build_src(
        &format!("{VEC_RETURN}    make() -> [X, Y]\n}}\n"),
        "/mcc/u308-multi-lane-return.mc",
    );
    assert!(
        !codes.contains(&4007),
        "the return face must be as wide as the operand it was written from \
         (two lanes); got {codes:?}"
    );
    assert!(
        in_same_net(&partition, "X", "c1.2"),
        "the first lane of `c[1:2]` must be published and wired to X; nets: {partition:?}"
    );
    assert!(
        in_same_net(&partition, "Y", "c2.2"),
        "the second lane of `c[1:2]` must be published and wired to Y; nets: {partition:?}"
    );
}
