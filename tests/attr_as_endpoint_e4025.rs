// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E4025 `ATTR_VALUE_NOT_A_TERMINAL` — an attribute key standing where a
//! connection endpoint is required (contract-design.md §3.5, G10).
//! E4026 `PIN_VALUE_KEY_NOT_FOUND` — the same reading of a pin row's value key
//! (`uH.A.volt`, U42) when that row declares no such key: the reference names
//! nothing, so it is a typed miss rather than a silent ghost.
//!
//! The name is real and it resolves, but it resolves in the definition space to
//! a *value*, and a value carries no position, so the connection has no
//! endpoint to attach to. The phrase is dropped: no net is built from a value.
//!
//! G10 (terminal first) is the other half and is locked here beside the error:
//! when one name is both a pin and a declared attribute key, the terminal face
//! wins and nothing is reported. The two rows must move together — a check that
//! reported the collision would break live wiring.
//!
//! Family naming `{family}__{essence}` uses a doubled underscore to separate
//! the grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{DiagnosticLevel, McIds};

const CODE: u32 = mcc::errcodes::ATTR_VALUE_NOT_A_TERMINAL;

/// One build of `src`: every `(code, level)` emitted, plus the net partition of
/// `main` (point paths sorted, net names dropped).
fn build_of(src: &str, uri: &str) -> (Vec<(u32, DiagnosticLevel)>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let (_, _, _, net_store) =
        mcc::mcc_build_with_nets(&McIds::from("main"), &uri.to_string()).expect("build");
    let diags: Vec<(u32, DiagnosticLevel)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.level))
        .collect();
    let parts: Vec<Vec<String>> = net_store
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
    (diags, parts)
}

fn count_of(diags: &[(u32, DiagnosticLevel)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// Does any point path hold `needle`?
fn wires(parts: &[Vec<String>], needle: &str) -> bool {
    parts.iter().any(|ps| ps.iter().any(|p| p.contains(needle)))
}

/// The instance-qualified key (`uH.partno`) is the case the check was added
/// for: it used to resolve to nothing and fall through to the ghost path
/// (a phantom net / pin, E3179). It must now be one error-level diagnostic,
/// and the value must not become a net point.
#[test]
fn attr_ep__qualified_key_as_endpoint_errors() {
    let src = "component U {\n    partno = \"ABC\"\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    U uH\n    uH.partno -> N1\n}\n";
    let (diags, parts) = build_of(src, "/mcc/attr-ep-qualified.mc");
    assert_eq!(
        count_of(&diags, CODE),
        1,
        "the key as an endpoint is one fact and one error; got {diags:?}"
    );
    assert!(
        diags.contains(&(CODE, DiagnosticLevel::Error)),
        "the code must be raised at error level; got {diags:?}"
    );
    assert!(
        !wires(&parts, "partno"),
        "a value has no position, so it must not become a net point; got {parts:?}"
    );
}

/// A body's connection line resolves through the component scope chain, where
/// declared attribute names sit beside pin names (G9 position 3). The bare
/// spelling must reach the same verdict as the qualified one.
#[test]
fn attr_ep__bare_key_in_body_errors() {
    let src = "component M {\n    partno = \"ABC\"\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    func Link([x]) {\n        partno -> x\n    }\n}\n\nmodule main {\n    M m1\n    m1.Link(N1)\n}\n";
    let (diags, parts) = build_of(src, "/mcc/attr-ep-bare.mc");
    assert_eq!(
        count_of(&diags, CODE),
        1,
        "the bare spelling must report the same fact once; got {diags:?}"
    );
    assert!(
        !wires(&parts, "partno"),
        "the bare key must not become a net point either; got {parts:?}"
    );
}

/// G10 terminal first: `partno` is both pin 1's name and a declared key, so the
/// terminal face wins, nothing is reported, and the wiring lands on the pin.
#[test]
fn attr_ep__terminal_wins_over_key() {
    let src = "component DUAL {\n    partno = \"ABC\"\n    pins = [\n        1 = partno\n        2 = B\n    ]\n}\n\nmodule main {\n    DUAL d1\n    d1.partno -> N1\n}\n";
    let (diags, parts) = build_of(src, "/mcc/attr-ep-terminal-first.mc");
    assert_eq!(
        count_of(&diags, CODE),
        0,
        "a colliding name resolves to the terminal, so nothing is reported; got {diags:?}"
    );
    assert!(
        wires(&parts, "d1.1"),
        "the colliding name must wire the pin; got {parts:?}"
    );
}

/// Control: an ordinary pin endpoint stays silent — the check cannot rot into a
/// net over legal wiring.
#[test]
fn attr_ep__plain_endpoint_stays_silent() {
    let src = "component U {\n    partno = \"ABC\"\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    U uH\n    uH.1 -> N1\n}\n";
    let (diags, parts) = build_of(src, "/mcc/attr-ep-plain.mc");
    assert_eq!(count_of(&diags, CODE), 0, "got {diags:?}");
    assert!(wires(&parts, "uH.1"), "the pin must wire; got {parts:?}");
}

// E4026: the pin row's value keys (U42)

const KEY_CODE: u32 = mcc::errcodes::PIN_VALUE_KEY_NOT_FOUND;

/// The messages of every diagnostic carrying `code`.
fn msgs_of(src: &str, uri: &str, code: u32) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri.to_string());
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.msg.clone())
        .collect()
}

/// A key the row does not declare is one error-level diagnostic, and the
/// reference stays out of the netlist — it used to reach neither the key table
/// nor the pin table and fell through to a phantom endpoint.
#[test]
fn attr_ep__pin_value_missing_key_errors() {
    let src = "component W {\n    pins = [\n        1 = A, \"L1\"\n        2 = B, voltage:3V3\n    ]\n}\n\nmodule main {\n    W uH\n    uH.B.volt -> N1\n}\n";
    let (diags, parts) = build_of(src, "/mcc/attr-ep-pin-key-miss.mc");
    assert_eq!(
        count_of(&diags, KEY_CODE),
        1,
        "one missing key is one error; got {diags:?}"
    );
    assert!(
        diags.contains(&(KEY_CODE, DiagnosticLevel::Error)),
        "the code must be raised at error level; got {diags:?}"
    );
    assert!(
        !wires(&parts, "volt"),
        "a value carries no position, so it must not become a net point; got {parts:?}"
    );
}

/// The message names the keys the row *does* answer to — the bare values under
/// `desc`, and each `KVS` key by its whole dotted spelling, so `spec.vout` never
/// answers to `spec`.
#[test]
fn attr_ep__pin_value_miss_lists_declared_keys() {
    let src = "component W {\n    pins = [\n        1 = A, \"L1\"\n        3 = C, spec.vout:2V\n    ]\n}\n\nmodule main {\n    W uH\n    uH.A.nope -> N1\n    uH.C.spec -> N2\n}\n";
    let msgs = msgs_of(src, "/mcc/attr-ep-pin-key-list.mc", KEY_CODE);
    assert!(
        msgs.iter()
            .any(|m| m.contains("nope") && m.contains("[desc]")),
        "a bare-valued row answers to `desc`; got: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("spec.vout")),
        "a dotted key is addressed whole; got: {msgs:?}"
    );
}

/// A declared key is still a value, not a terminal: the verdict is E4025 and
/// the new key check must stay silent — it reports `key` misses only.
#[test]
fn attr_ep__declared_pin_value_key_is_not_a_key_miss() {
    let src = "component W {\n    pins = [\n        2 = B, voltage:3V3\n    ]\n}\n\nmodule main {\n    W uH\n    uH.B.voltage -> N1\n}\n";
    let (diags, _parts) = build_of(src, "/mcc/attr-ep-pin-key-hit.mc");
    assert_eq!(count_of(&diags, KEY_CODE), 0, "got {diags:?}");
    assert_eq!(count_of(&diags, CODE), 1, "got {diags:?}");
}
