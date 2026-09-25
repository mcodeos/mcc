// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U300 grammar-alignment implementation batch — chained locks (CIMP-OPEN U300,
//! audit `mcd/log/9.25.u300-grammar-alignment-audit.md`).
//!
//! Three locks pinned to the batch's user rulings:
//!
//! 1. **I3 / M1** — `FAMILY.INST SW1{COM | NO}` (a named-ctor CURLY_MN used as an
//!    inline instance) is legal: the curly base resolves through the ctor name,
//!    the instance materializes with its faces wired, and E3152 stays silent.
//!    The hs board (`mcs/hs/core/mcu.mc` SW1/SW2 rows) is the ruling's origin.
//!    The presence counterpart keeps the *unresolvable* base reporting E3152.
//! 2. **T3 / M2** — `=>` closures: `|ports|`-headed block bodies are accepted
//!    (the closure formals bind the block's names, so no E3103 "declare"
//!    rejection and no W3136 floating-label false positive), and the two-hop
//!    `S => RES(1k) R1 => { R1.1 -> ... }` chain keeps its wiring.
//! 3. **I1 / M5+M6** — the E4008 operator family (`/ ~ :` binary and binary `*`)
//!    fires on every shape; when the discarded statement carried inline ctors,
//!    the salvage path still materializes them (`A / RES(1k) - B` keeps `_R1`
//!    in the pass-2 table and no secondary E3132 stacks), and shapes without a
//!    ctor keep the E3132 wrapper with its reworded "failed to evaluate" text.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    // SWITCH.BUTTON and RES live in the system library (~/.mcode/mcode).
    mcc::mcc_init();
    let uri: McURI = "/mcc/u300-grammar-alignment.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// (left point path, right point path) pairs of the tree-level connections of
/// module `top`. The inline curly-mn instance's wiring is locked here - the
/// tree connection surface is where the M1 ruling observes it (findable in the
/// instance tree).
fn tree_conn_pairs(src: &str) -> Vec<(String, String)> {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_init();
    let uri: McURI = "/mcc/u300-grammar-alignment.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let tree = mcc::mcc_build(&McIds::from("top"), &uri).expect("build failed");
    tree.connections
        .iter()
        .filter_map(|c| match (c.points.first(), c.points.get(1)) {
            (Some(a), Some(b)) => Some((a.path.clone(), b.path.clone())),
            _ => None,
        })
        .collect()
}

/// (entry path, net name) pairs of the flat pass-2 netlist.
fn net_pairs(src: &str) -> Vec<(String, String)> {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_init();
    let uri: McURI = "/mcc/u300-grammar-alignment.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    let entry = mcc::McSpaceName {
        ident: McIds::from("top"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    let mut pairs = Vec::new();
    for net in table.get_nets() {
        for &point_id in &net.points {
            let Some(entry) = table.get_entry(point_id) else {
                continue;
            };
            pairs.push((entry.path.clone(), net.name.clone()));
        }
    }
    pairs
}


const SRC_I3: &str = r#"
module top
{
    io PWR
    io SIG
    PWR - SWITCH.BUTTON SW1{COM | NO} -> SIG
}
"#;

#[test]
fn u300__named_ctor_curly_mn_is_a_legal_inline_instance() {
    let codes = build_codes(SRC_I3);
    assert!(
        !codes.contains(&mcc::errcodes::CURLY_MN_WRONG_BASE),
        "the hs-board curly-mn ctor shape must not fire E3152; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "the curly-mn ctor statement must not be dropped (E3132); got codes: {codes:?}"
    );
    // The instance materializes with both faces wired: PWR→COM, SIG→NO. The
    // faces resolve through the same def-authoritative expander a declared
    // instance's curly form uses (U304), so the points spell the physical pin
    // ids (COM = 1, NO = 2) — the same spelling the declared control arm
    // produces. The flat-projection parity is locked in shard4
    // u304_flat_curly_pins.rs.
    let pairs = tree_conn_pairs(SRC_I3);
    assert!(
        pairs.contains(&("PWR".to_string(), "SW1.1".to_string())),
        "COM face must connect PWR - SW1.1; got {pairs:?}"
    );
    assert!(
        pairs.contains(&("SW1.2".to_string(), "SIG".to_string())),
        "NO face must connect SW1.2 - SIG; got {pairs:?}"
    );
}

#[test]
fn u300__unresolvable_curly_base_still_reports_3152() {
    // Presence counterpart: an E3152 with no resolvable base must not go quiet.
    let src = SRC_I3.replace("SWITCH.BUTTON", "NOSUCH.FACE");
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::CURLY_MN_WRONG_BASE),
        "an unresolvable curly base must still fire E3152; got codes: {codes:?}"
    );
}


const SRC_T3_CLOSURE: &str = r#"
component DEV
{
    pins = [
        1 = IO0
        2 = IO1
    ]
}

module top
{
    io GND
    DEV D
    D => |ports| { ports -> GND }
}
"#;

#[test]
fn u300__closure_named_ports_bind_without_3103_or_w3136() {
    let codes = build_codes(SRC_T3_CLOSURE);
    assert!(
        !codes.contains(&mcc::errcodes::PARAM_DECLARE_INVALID),
        "|ports| closure formals must not fire E3103; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "closure formals must not fire W3136 on the block body; got codes: {codes:?}"
    );
}

const SRC_T3_CHAIN: &str = r#"
module top
{
    io SIG
    io CLK
    SIG => RES(1k) R1 => { R1.1 -> CLK }
}
"#;

#[test]
fn u300__two_hop_arrow_chain_keeps_its_wiring() {
    let codes = build_codes(SRC_T3_CHAIN);
    assert!(
        !codes.contains(&mcc::errcodes::PARAM_DECLARE_INVALID),
        "the two-hop chain must not fire E3103; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "the two-hop chain must not fire W3136; got codes: {codes:?}"
    );
    // W3137 on `R1.1` is the pre-existing structured-reference naming face
    // (m2c comparison: fires identically without any closure) — out of M2
    // scope, so it is tolerated here, but the wiring itself must survive.
    // (Tree connection surface — same InstTable caveat as the I3 lock above.)
    let pairs = tree_conn_pairs(SRC_T3_CHAIN);
    assert!(
        pairs.iter().any(|(a, b)| (a == "R1.1" && b == "CLK") || (a == "CLK" && b == "R1.1")),
        "the chained body's R1.1 must connect to CLK; got {pairs:?}"
    );
}


fn conn_codes(stmt: &str) -> Vec<u32> {
    let src = format!(
        "module top\n{{\n    io A\n    io B\n    {stmt}\n}}\n"
    );
    build_codes(&src)
}

#[test]
fn u300__e4008_fires_on_every_unsupported_operator_shape() {
    for op in ["/", "~", ":", "*"] {
        let codes = conn_codes(&format!("A {op} B"));
        assert!(
            codes.contains(&mcc::errcodes::CONN_OPERATOR_UNSUPPORTED),
            "binary `{op}` must fire E4008; got codes: {codes:?}"
        );
    }
}

#[test]
fn u300__e4008_statement_with_inline_ctor_still_materializes_it() {
    // M5 ruling: the error does not block instantiation. The `/` shape fires
    // E4008, the salvage path keeps the inline RES in the pass-2 table, and —
    // because salvage succeeds — no secondary E3132 stacks on top.
    let codes = conn_codes("A / RES(1k) - B");
    assert!(
        codes.contains(&mcc::errcodes::CONN_OPERATOR_UNSUPPORTED),
        "the `/` shape must still fire E4008; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "a salvaged statement must not stack E3132; got codes: {codes:?}"
    );
    let pairs = net_pairs("module top\n{\n    io A\n    io B\n    A / RES(1k) - B\n}\n");
    assert!(
        pairs.iter().any(|(p, _)| p.ends_with("_R1.1") || p.ends_with("_R1.2")),
        "the salvaged inline RES must materialize in the pass-2 table; got {pairs:?}"
    );
}

#[test]
fn u300__ctorless_e4008_keeps_the_reworded_e3132_wrapper() {
    // Without an inline ctor there is nothing to salvage, so the wrapper
    // E3132 still fires (M6 rewording carries it — content is locked by the
    // errcodes entry, here we lock the pairing).
    let codes = conn_codes("A ~ B");
    assert!(
        codes.contains(&mcc::errcodes::CONN_OPERATOR_UNSUPPORTED),
        "`~` must fire E4008; got codes: {codes:?}"
    );
    assert!(
        codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "a ctorless dropped statement must keep the E3132 wrapper; got codes: {codes:?}"
    );
}

/// Net-name of the single entry whose path ends with `suffix`.
fn net_of(pairs: &[(String, String)], suffix: &str) -> String {
    let mut hits: Vec<&str> = pairs
        .iter()
        .filter(|(p, _)| p.ends_with(suffix))
        .map(|(_, n)| n.as_str())
        .collect();
    hits.sort_unstable();
    hits.dedup();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one entry ending {suffix:?}; got {hits:?} in {pairs:?}"
    );
    hits[0].to_string()
}
