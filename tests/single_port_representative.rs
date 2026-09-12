// Copyright (c) 2026 MCode
//
// Integration tests for §4 single-port representative rule (vec-dianlu.md §5.2).
//
// The shape-level `representative` (common.rs) only names the single-point
// label; the physical pairing is done independently by Pass2. These tests
// verify the Pass2 anchoring end-to-end.
//
// Per vec-dianlu.md §1.4 a series junction is **positional**: the written-left
// operand's right face meets the written-right operand's left face, and that
// is identical for `-`, `->` and `<-`. The direction word never enters the
// wiring — it is carried by `ConnDir` alone (§2.4.5). So all three pair the
// same way and differ only in the direction they record, and in which single
// label names the resulting net (`representative`, §5.2):
//   `+`  → Parallel, vexpr_wire_parallel anchors opd[0] (op1)
//   `-`  → Series(Undirected), connections Undirected
//   `->` → Series(LtoR), representative is op2 (the chain tail)
//   `<-` → Series(RtoL), representative is op1 (the chain head); the arrow is
//          drawn leftward over the same written left-to-right layout (§1.1)
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

// `+` takes op1: vexpr_wire_parallel anchors opd[0]

#[test]
fn plus_anchors_operand_one() {
    // VEXT (op1) is the parallel anchor; TP1 and TP2 both merge into the VEXT
    // net → one multi-point connection {VEXT, TP1.1, TP2.1}.
    //
    // The other operands must be *bodies*. The original fixture chained three
    // bare labels (`VEXT + VDD + V5V`), which is now rejected at Pass1: two
    // distinct names are two distinct potentials, so `+` between them fuses two
    // nets (CONN_NET_CROSSNET, vec-dianlu §1.4/§5.4). One-pin bodies keep the
    // subject -- opd[0] is what the merged net is anchored on.
    let inst = build(
        r#"
component TESTPOINT()
{
    pins = [
        1 = P1
    ]
}

module main
{
    VEXT + TP1::TESTPOINT() + TP2::TESTPOINT()
}
"#,
    );
    assert_net_has(&nets(&inst), &["VEXT", "TP1.1", "TP2.1"]);
}

// `-` takes op1: Series chain head opd1

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

// `->` takes op2: LtoR chain tail is the output

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

// `<-` pairs positionally like every other series

#[test]
fn leftarrow_pairs_by_written_position() {
    // VEXT <- R1 <- GND is laid out left-to-right and wired by position, so the
    // pairs are the same as for `-` and `->`: VEXT↔R1.1 and R1.2↔GND. The
    // leftward arrow is carried by `ConnDir::RtoL`, not by reordering operands.
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
    assert_paired(&got, "VEXT", "R1.1");
    assert_paired(&got, "R1.2", "GND");
    // `<-` chains must carry the RtoL direction
    assert!(
        inst.connections.iter().any(|c| c.dir == mcc::ConnDir::RtoL),
        "expected a RtoL connection in {got:?}"
    );
}
