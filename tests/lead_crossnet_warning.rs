// Copyright (c) 2026 MCode
//
// vec-dianlu §5.4 — the `_` lead is an **ideal wire**, i.e. a body, not an
// empty width slot. Its two ends are meant to be the same net: that is what
// makes a lane's left and right ends one member (`pwr{V1,V2} -> [_, RES] ->
// out{V1,V2}`). When a lane's two ends are **two different bare nets**, the
// lead joins them at zero impedance and must warn (4182), never error — the
// author may have written the jumper on purpose.
//
// The lane loop drops the `_` placeholder (it holds a width slot, not an
// endpoint), which leaves the lane's neighbours adjacent and wires them
// straight through; the warning recovers *why* they met.
//
// NOTE: These tests share global mcc state, so a mutex serializes them.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::errcodes::CONN_LEAD_CROSSNET;
use mcc::{McIds, McModuleInst, McURI};

const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Load source, build module `top`, return the module instance.
fn build(source: &str) -> McModuleInst {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/lead-crossnet.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    mcc::mcc_build(&McIds::from("top"), &uri).expect("build failed")
}

/// How many CONN_LEAD_CROSSNET warnings the build of `top` recorded.
fn crossnet_count(inst: &McModuleInst) -> usize {
    inst.diagnostics
        .iter()
        .filter(|d| d.code == CONN_LEAD_CROSSNET)
        .count()
}

/// Collect all 2-point connection pairs as (left.path, right.path).
fn pairs(inst: &McModuleInst) -> Vec<(String, String)> {
    inst.connections
        .iter()
        .filter(|c| c.points.len() == 2)
        .map(|c| (c.points[0].path.clone(), c.points[1].path.clone()))
        .collect()
}

/// Two named nets under one lead: the lead is an ideal wire between them, so
/// the lane is shorted at zero impedance → exactly one warning, and the
/// connection is still built (warning level, not error).
#[test]
fn crl__lead_joining_two_named_nets_warns_once() {
    let src =
        format!("{RES2}module top {{\n    RES2 R101\n    [V1, V2] -> [_, R101] -> [V3, V4]\n}}\n");
    let inst = build(&src);
    assert_eq!(
        crossnet_count(&inst),
        1,
        "expected exactly one CONN_LEAD_CROSSNET, got:\n{:#?}",
        inst.diagnostics
    );
    let got = pairs(&inst);
    assert!(
        got.iter().any(|(l, r)| l == "V1" && r == "V3"),
        "the lead's two ends must still be wired (warning, not error):\n  {got:?}"
    );
}

/// The lead's two ends are the same net: an ordinary matched pass-through, no
/// merge happens and nothing is reported.
#[test]
fn crl__lead_joining_one_net_twice_stays_quiet() {
    let src =
        format!("{RES2}module top {{\n    RES2 R101\n    [V1, V2] -> [_, R101] -> [V1, V4]\n}}\n");
    let inst = build(&src);
    assert_eq!(
        crossnet_count(&inst),
        0,
        "a lead whose ends are the same net is a matched pass-through:\n{:#?}",
        inst.diagnostics
    );
}

/// A pin on either side carries an owner: a lead between a bare net and a pin
/// is ordinary wiring, not a named-net merge — no warning.
#[test]
fn crl__lead_touching_a_pin_stays_quiet() {
    let src = format!(
        "{RES2}module top {{\n    RES2 R101\n    RES2 R102\n    [V1, V2] -> [_, R101] -> [R102, V4]\n}}\n"
    );
    let inst = build(&src);
    assert_eq!(
        crossnet_count(&inst),
        0,
        "a pin end is not a named net:\n{:#?}",
        inst.diagnostics
    );
}
