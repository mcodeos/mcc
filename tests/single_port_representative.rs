// Copyright (c) 2026 MCode
//
// Integration tests for §4 single-port representative rule (eval.md §4 注记).
//
// Rule: `+` / `-` / `<-` take operand 1 as representative, `->` takes operand 2.
// Verified end-to-end through Pass2 connection anchoring:
//   `+`  → wire_parallel_internal anchors opd[0] (op1)
//   `-`  → Series chain head is opd1 (op1)
//   `<-` → Series(RtoL) swaps to [opd2, opd1], op1 lands on the chain tail
//   `->` → Series(LtoR) chain tail (op2) is the output (set_right_out)
//
// NOTE: These tests share global mcc state, so a mutex serializes them.

use mcc::{McIds, McURI};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Helper: acquire lock, load source, build module, return instance.
fn build(source: &str) -> mcc::McModuleInst {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/single-port-representative.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);

    drop(lock);
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

// ── `+` 取 op1：wire_parallel_internal 锚定 opd[0] ─────────────────────────

#[test]
fn plus_anchors_operand_one() {
    // VEXT（op1）作为并联锚，VDD / V5V 都并入 VEXT 网 → 单条多点连接 {VEXT, VDD, V5V}。
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

// ── `-` 取 op1：Series 链首 opd1 ──────────────────────────────────────────

#[test]
fn minus_keeps_operand_one_as_chain_head() {
    // VEXT - R1 - GND：VEXT（op1）在链首，连接 VEXT↔R1.1、R1.2↔GND。
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
    // `-` 链是无向连接
    assert!(
        inst.connections
            .iter()
            .all(|c| c.dir == mcc::ConnDir::Undirected),
        "expected Undirected connections in {got:?}"
    );
}

// ── `->` 取 op2：LtoR 链尾是输出端 ─────────────────────────────────────────

#[test]
fn rightarrow_keeps_operand_two_as_chain_tail() {
    // VEXT -> R1 -> GND：op2（GND）在链尾，连接 VEXT↔R1.1、R1.2↔GND。
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
    // `->` 链必须带 LtoR 方向
    assert!(
        inst.connections.iter().any(|c| c.dir == mcc::ConnDir::LtoR),
        "expected a LtoR connection in {got:?}"
    );
}

// ── `<-` 取 op1：RtoL swap 后 op1 落链尾 ──────────────────────────────────

#[test]
fn leftarrow_keeps_operand_one_as_target() {
    // VEXT <- R1 <- GND：数据从 GND 经 R1 流向 VEXT（op1 目标网）。
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
    // `<-` 链必须带 RtoL 方向
    assert!(
        inst.connections.iter().any(|c| c.dir == mcc::ConnDir::RtoL),
        "expected a RtoL connection in {got:?}"
    );
}
