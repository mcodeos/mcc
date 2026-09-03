// Copyright (c) 2026 MCode
//
// Integration tests for §4 single-port representative rule (vec-dianlu.md §5.2).
//
// The shape-level `representative` (common.rs) only names the single-point
// label; the physical pairing is done independently by Pass2. These tests
// verify the Pass2 anchoring end-to-end:
//   `+`  → wire_parallel_internal anchors opd[0] (op1)
//   `-`  → Series chain head is opd1 (op1)
//   `<-` → Series(RtoL) swaps to [opd2, opd1], op1 lands on the chain tail
//   `->` → Series(LtoR) chain tail (op2) is the output (set_right_out)
//
// NOTE: These tests share global mcc state, so a mutex serializes them.

mod common;

use mcc::{McIds, McURI};
use std::collections::HashSet;

/// Helper: acquire lock, load source, build module, return instance.
fn build(source: &str) -> mcc::McModuleInst {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/single-port-representative.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);

    result.expect("build failed")
}

/// Collect all 2-point connection pairs as (left.path, right.path).
fn pairs(inst: &mcc::McModuleInst) -> Vec<(String, String)> {
    inst.connections
        .iter()
        .filter(|c| c.points.len() == 2)
        .map(|c| (c.points[0].path.clone(), c.points[1].path.clone()))
        .collect()
}

/// Collect all nets (connections with 2+ points) as point-path lists.
fn nets(inst: &mcc::McModuleInst) -> Vec<Vec<String>> {
    inst.connections
        .iter()
        .filter(|c| c.points.len() >= 2)
        .map(|c| c.points.iter().map(|p| p.path.clone()).collect())
        .collect()
}

/// Assert that a connection pair exists in either order.
fn assert_paired(got: &[(String, String)], a: &str, b: &str) {
    assert!(
        got.iter()
            .any(|(l, r)| { (l == a && r == b) || (l == b && r == a) }),
        "expected connection ({a}, {b}) among:\n  {got:?}"
    );
}

/// Assert that a single net contains all the given members (order-insensitive).
fn assert_net_has(got: &[Vec<String>], members: &[&str]) {
    assert!(
        got.iter().any(|net| {
            let set: HashSet<&str> = net.iter().map(|s| s.as_str()).collect();
            members.iter().all(|m| set.contains(m))
        }),
        "expected a net containing {members:?} among:\n  {got:?}"
    );
}

// ── `+` takes op1: wire_parallel_internal anchors opd[0] ──────────────────

#[test]
fn plus_anchors_operand_one() {
    // VEXT (op1) is the parallel anchor; VDD and V5V both merge into the VEXT
    // net → one multi-point connection {VEXT, VDD, V5V}.
    let inst = build(
        r#"
module main
{
    VEXT + VDD + V5V
}
"#,
    );
    assert_net_has(&nets(&inst), &["VEXT", "VDD", "V5V"]);
}

// ── `-` takes op1: Series chain head opd1 ─────────────────────────────────

#[test]
fn minus_keeps_operand_one_as_chain_head() {
    // VEXT - R1 - GND: VEXT (op1) is at the chain head; connections are
    // VEXT↔R1.1 and R1.2↔GND.
    let inst = build(
        r#"
component RES2()
{
    pins = [
        1 = P1
        2 = P2
    ]
}

module main
{
    VEXT - R1::RES2() - GND
}
"#,
    );
    let got = pairs(&inst);
    assert_paired(&got, "VEXT", "R1.1");
    assert_paired(&got, "R1.2", "GND");
    // `-` chains are undirected connections
    assert!(
        inst.connections
            .iter()
            .all(|c| c.dir == mcc::ConnDir::Undirected),
        "expected Undirected connections in {got:?}"
    );
}

// ── `->` takes op2: LtoR chain tail is the output ─────────────────────────

#[test]
fn rarrow_takes_operand_two_as_output() {
    // VEXT -> R1 -> GND: op2 (GND) is at the chain tail; connections are
    // VEXT↔R1.1 and R1.2↔GND.
    let inst = build(
        r#"
component RES2()
{
    pins = [
        1 = P1
        2 = P2
    ]
}

module main
{
    VEXT -> R1::RES2() -> GND
}
"#,
    );
    let got = pairs(&inst);
    assert_paired(&got, "VEXT", "R1.1");
    assert_paired(&got, "R1.2", "GND");
    // `->` chains must carry the LtoR direction
    assert!(
        inst.connections.iter().any(|c| c.dir == mcc::ConnDir::LtoR),
        "expected a LtoR connection in {got:?}"
    );
}

// ── `<-` takes op1: after the RtoL swap, op1 lands on the chain tail ──────

#[test]
fn leftarrow_keeps_operand_one_as_target() {
    // VEXT <- R1 <- GND: data flows from GND through R1 to VEXT (op1 target net).
    let inst = build(
        r#"
component RES2()
{
    pins = [
        1 = P1
        2 = P2
    ]
}

module main
{
    VEXT <- R1::RES2() <- GND
}
"#,
    );
    let got = pairs(&inst);
    assert_paired(&got, "VEXT", "R1.2");
    assert_paired(&got, "R1.1", "GND");
    // `<-` chains must carry the RtoL direction
    assert!(
        inst.connections.iter().any(|c| c.dir == mcc::ConnDir::RtoL),
        "expected a RtoL connection in {got:?}"
    );
}
