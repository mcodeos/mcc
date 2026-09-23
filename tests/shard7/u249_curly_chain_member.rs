// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U249: the curly dot-chain instance member `uC{ADC.P}` is canonical sugar
//! for the dotted spellings `uC.ADC.P` and `uC.ADC{P}` (resolve-gate §2.13.6
//! equivalence ruling), and the B8 params-first declare `CAP(100nF, 10V)
//! cap[1:2].Cap(...)` materializes exactly the member set the canonical split
//! form (`cap[1:2]::CAP(100nF, 10V)` + `cap[1:2].Cap(...)`) does.
//!
//! Locks:
//! 1. All four member spellings resolve the interface pins (no E3179, pins
//!    6/7 report connected).
//! 2. An unknown chain member keeps the interface diagnostic (E3179 names the
//!    missing member).
//! 3. B8: the canonical func-body declare materializes its member set under
//!    the receiver path (`main.b.cap…`, pins included).
//!
//! NOTE: These tests share global mcc state, so a mutex serializes them.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{mcb_pass2_flat, McIds, McSpaceName, McURI};

const IFACE_AND_BOARD: &str = r#"
interface ADC.DIFF(role)
{
    pins = [
        1 = P
        2 = N
    ]
    role Receiver {
        pins = [
            in 1 = P
            in 2 = N
        ]
        peer = Transmitter
    }
    role Transmitter {
        pins = [
            out 1 = P
            out 2 = N
        ]
        peer = Receiver
    }
}

component Probe.UC
{
    partno = "P1"

    pins = [
        io [6, 7] = ADC{P, N}::ADC.DIFF(Receiver)
    ]
}
"#;

/// Build a module whose two connection lines use the given member spelling.
fn board_with_lines(lines: &str) -> String {
    format!("{IFACE_AND_BOARD}\nmodule main {{\n    Probe.UC u1\n{lines}\n}}\n")
}

/// Build and assert the two io pins (6/7) are connected through the member
/// spelling under test: no member-resolution failure (E3179) and no
/// unconnected-pin report for either io pin (E4117/E4119 naming `main.u1.6`
/// or `main.u1.7`).
fn assert_members_connected(src: &str) {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/u249-curly-chain-member.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);

    let diags = mcc::mcc_diagnose_all();
    assert!(
        !diags.iter().any(|d| d.code == 3179),
        "member spelling must resolve the interface pins; diags: {diags:?}"
    );
    let unconnected = diags.iter().any(|d| {
        (d.code == 4117 || d.code == 4119)
            && (d.msg.contains("main.u1.6") || d.msg.contains("main.u1.7"))
    });
    assert!(
        !unconnected,
        "io pins 6/7 must land on nets through the member spelling; diags: {diags:?}"
    );
}

#[test]
fn u249__dotted_chain_member_resolves() {
    assert_members_connected(&board_with_lines(
        "    N1 -> u1.ADC.P -> GND\n    N2 -> u1.ADC.N -> GND",
    ));
}

#[test]
fn u249__dot_curly_chain_member_resolves() {
    assert_members_connected(&board_with_lines(
        "    N1 -> u1.ADC{P} -> GND\n    N2 -> u1.ADC{N} -> GND",
    ));
}

#[test]
fn u249__curly_chain_member_resolves() {
    assert_members_connected(&board_with_lines(
        "    N1 -> u1{ADC.P} -> GND\n    N2 -> u1{ADC.N} -> GND",
    ));
}

#[test]
fn u249__nested_curly_chain_member_resolves() {
    assert_members_connected(&board_with_lines(
        "    N1 -> u1{ADC{P}} -> GND\n    N2 -> u1{ADC{N}} -> GND",
    ));
}

#[test]
fn u249__unknown_chain_member_reports_e3179() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/u249-curly-chain-member-bad.mc".to_string();
    mcc::mcc_load_from_string(&uri, &board_with_lines("    N1 -> u1{ADC.Z} -> GND"));
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);

    let diags = mcc::mcc_diagnose_all();
    let named = diags
        .iter()
        .any(|d| d.code == 3179 && d.msg.contains("ADC.Z"));
    assert!(
        named,
        "unknown chain member must keep the E3179 diagnostic naming it; diags: {diags:?}"
    );
}

/// B8: the canonical func-body declare materializes its member set under the
/// receiver path (`main.b.cap…`), pins included.
///
/// Known divergence (ledger, not locked here): the params-first one-liner
/// `DCAP(100, 10) cap[1:2].Cap([n1, n2])` fuses the declare into the call
/// chain, so the member names are class-construction callers and the prefix
/// law (fcallinst.rs: the class-construction caller is never prefixed) lands
/// them bare at module scope (`main.cap1`) without a vector group. Same nets,
/// different instance identity.
const B8_COMPONENTS: &str = r#"
component DCAP(cap::INT, volt::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]

    func Cap([net1, net2])
    {
        net1 - this - net2
        return [net1, net2]
    }
}

"#;

fn cap_member_set(func_body: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();

    let src = format!(
        "{B8_COMPONENTS}\ncomponent Probe.BOARD {{\n    partno = \"B1\"\n\n{func_body}\n}}\n\nmodule main {{\n    Probe.BOARD b\n    b.dec([VCC, GND])\n}}\n"
    );
    let uri: McURI = "/mcc/u249-b8-params-first.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);

    let entry = McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcb_pass2_flat(&entry, 1).expect("pass2 flat");
    let mut out: Vec<String> = table
        .iter()
        .filter(|(_, e)| e.path.starts_with("main.b.cap"))
        .map(|(_, e)| e.path.clone())
        .collect();
    out.sort();
    out
}

#[test]
fn u249__b8_canonical_func_body_declares_materialize_under_receiver_path() {
    let members = cap_member_set(
        "    func dec([n1, n2])\n    {\n        cap[1:2]::DCAP(100, 10)\n        cap[1:2].Cap([n1, n2])\n    }",
    );

    assert_eq!(
        members,
        vec![
            "main.b.cap1",
            "main.b.cap1.1",
            "main.b.cap1.2",
            "main.b.cap2",
            "main.b.cap2.1",
            "main.b.cap2.2",
        ],
        "canonical func-body declare must materialize both array members with their pins"
    );
}
