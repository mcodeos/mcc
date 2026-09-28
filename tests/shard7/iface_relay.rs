// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The relay face (iface-peer-cardinality-design.md §4, U352): `role RELAY`
//! is an ordinary role-table member, the relay role's face takes the
//! interface's base pin table (the role-less conductor view), and a relay
//! body is a plain module — `io a::IF(RELAY)` ports joined by explicit body
//! statements, crossing or straight.
//!
//! What the flat checks must read through a relay: the exclusive-peer gate
//! (6054) counts the peer bodies the relay's conductor merges into the
//! group — a device torn across two cables sees both hosts — while a RELAY
//! face itself is never a peer body and never a 6061 pair member. The relay
//! body's own `a - b` join is the crossing law's statement, not a peer
//! pairing, so the statement-level judge (E4121) is silent on RELAY↔RELAY.
//! The carve is module-port-only and declaration-gated: `RELAY` on a module
//! port is legal exactly where the interface's role table declares it
//! (otherwise E4184 keeps its voice), and a component pin adopting an
//! undeclared role is E4104 regardless of the name.
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries a
//! member, including the silence branches — the legal one-cable chain, the
//! dangling relay face (a chain end is legal), and the crossed body.

#![allow(non_snake_case)]

use crate::common;

use std::process::Command;

use mcc::{McIds, McURI};

/// A role-bearing interface with no relay role — the carve's negative twin.
const BARE: &str = r#"
interface BARE(role)
{
    topology = "point to point"
    pins = [
        [1,2,3,4] = [VBUS, DP, DM, GND]
    ]
    role HOST {
        peer = DEVC(1)
    }
    role DEVC {
        peer = HOST(1)
    }
}
"#;

/// The family: HOST ↔ DEVC with one-body windows (the exact-count form),
/// plus the relay role — no pins of its own, so its face is the base
/// interface's conductor view.
const USB: &str = r#"
interface USB(role)
{
    topology = "point to point"
    pins = [
        [1,2,3,4] = [VBUS, DP, DM, GND]
    ]
    role HOST {
        peer = DEVC(1)
    }
    role DEVC {
        peer = HOST(1)
    }
    role RELAY { }
}

component HOSTC
{
    pins = [
        [10,11,12,13] = UBUS::USB(HOST)
    ]
}

component DEVC
{
    pins = [
        [20,21,22,23] = UBUS::USB(DEVC)
    ]
}

module CABLE
{
    io a::USB(RELAY)
    io b::USB(RELAY)
    a - b
}
"#;

/// Build `main` with the body statements and return the sorted diagnostic
/// codes.
fn build(body: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{USB}module main {{\n    HOSTC h1\n    DEVC d1\n    CABLE c1\n{body}\n}}\n"
    );
    let uri: McURI = "/mcc/iface-relay.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(code: u32, body: &str) -> usize {
    build(body).iter().filter(|&&c| c == code).count()
}

/// The healthy chain — one host, one cable, one device. The relay's two
/// faces are conductors: the host lane reaches exactly one device, the
/// device lane exactly one host, and the RELAY faces pair against nothing.
/// All quiet, E4121 included (the body's `a - b` is the crossing law's own
/// statement, not a peer pairing).
#[test]
fn iface_relay__legal_chain_is_quiet() {
    let body = "    h1.UBUS -> c1.a\n    c1.b -> d1.UBUS";
    assert_eq!(
        count(mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT, body),
        0,
        "one host to one device through a cable is quiet; got {:#?}",
        build(body)
    );
    assert_eq!(
        count(mcc::errcodes::IFACE_ROLE_INCOMPATIBLE, body),
        0,
        "the relay body's own join is not a peer pairing; got {:#?}",
        build(body)
    );
    assert_eq!(
        count(mcc::errcodes::IFACE_ROLE_PEER_CONFLICT, body),
        0,
        "a RELAY face pairs against nothing in the flat sweep; got {:#?}",
        build(body)
    );
}

/// The defect the relay traversal exists for: one device torn across TWO
/// cable bodies. Each single connect is a legal mutual-peer pairing and the
/// per-net view is quiet on both sides — only the conductor merge through
/// the relay bodies shows the device lane reaching two host instances.
/// Exactly one 6054, citing both hosts; the host↔host meeting the same
/// merge exposes is the flat sweep's own fact (one 6061 per conductor, the
/// four members); the relay joins themselves stay E4121-silent.
#[test]
fn iface_relay__torn_pairing_through_two_cables_fires_once() {
    let body = "    HOSTC h2\n    CABLE c2\n    h1.UBUS -> c1.a\n    c1.b -> d1.UBUS\n    h2.UBUS -> c2.a\n    c2.b -> d1.UBUS";
    let codes = build(body);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT)
            .count(),
        1,
        "one device lane torn across two cables → exactly one 6054; got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 4121).count(),
        0,
        "the relay joins must not fire the statement-level peer judge; got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 6061).count(),
        4,
        "the merge exposes host↔host on all four conductors — 6061's own facet; got {codes:?}"
    );
}

/// A dangling relay face is a chain end, not a defect: the cable's `b` side
/// wired to nothing leaves the host lane at exactly one peer — quiet.
#[test]
fn iface_relay__dangling_face_is_quiet() {
    let body = "    h1.UBUS -> c1.a";
    assert_eq!(
        count(mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT, body),
        0,
        "a dangling relay face is a chain end; got {:#?}",
        build(body)
    );
}

/// The body's statements are what the merge reads: a crossed cable
/// (`DM ↔ GND`) still shows each lane exactly one peer — the traversal has
/// no straight-through assumption.
#[test]
fn iface_relay__crossed_body_joins_quiet() {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{USB}module XABLE {{\n    io a::USB(RELAY)\n    io b::USB(RELAY)\n    a.VBUS - b.VBUS\n    a.DP - b.DP\n    a.DM - b.GND\n    a.GND - b.DM\n}}\nmodule main {{\n    HOSTC h1\n    DEVC d1\n    XABLE c1\n    h1.UBUS -> c1.a\n    c1.b -> d1.UBUS\n}}\n"
    );
    let uri: McURI = "/mcc/iface-relay-cross.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT)
            .count(),
        0,
        "a crossed cable still pairs one body to one body; got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 4121).count(),
        0,
        "the crossed joins are RELAY↔RELAY meetings — E4121 silent; got {codes:?}"
    );
}

/// The carve is declaration-gated: `RELAY` on a module port is legal exactly
/// where the interface's role table declares it — spelled against an
/// interface without a relay role, the argument selects neither a position
/// nor a relay, so E4184 keeps its voice (the signature-port face; a
/// body-declared `io` port gates through E4104 instead). An endpoint role
/// (`HOST`) on a module port stays an error in both worlds.
#[test]
fn iface_relay__module_port_carve_is_declaration_gated() {
    // E4184 lives on the check face, which the flat build does not run — the
    // same CLI harness `shard3/module_port_role_free.rs` uses drives it.
    let parse_4184 = |source: &str| -> usize {
        let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
            .args([
                "parse",
                "--code",
                source,
                "--local",
                "--pass1",
                "--pass2",
                "--top",
                "main",
                "-f",
                "json",
            ])
            .output()
            .expect("run mcc parse");
        assert!(
            output.status.success() || !String::from_utf8_lossy(&output.stdout).is_empty(),
            "mcc parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("parse mcc JSON output");
        value["result"]["pass0"]["diagnostics"]
            .as_array()
            .expect("pass0 diagnostics")
            .iter()
            .filter(|d| d["code"].as_u64() == Some(4184))
            .count()
    };
    let bare = format!("{BARE}module main(io a::BARE(RELAY), io b::BARE(RELAY)) {{}}\n");
    assert_eq!(
        parse_4184(&bare),
        2,
        "RELAY spelled against an interface without a relay role keeps E4184's voice"
    );
    let carved = format!("{USB}module main(io a::USB(RELAY), io b::USB(RELAY)) {{}}\n");
    assert_eq!(
        parse_4184(&carved),
        0,
        "a declared relay role is the carve — E4184 silent"
    );
}

/// A component pin adopting an undeclared role is E4104 regardless of the
/// name — the relay carve lives on module ports, not on endpoint terminals.
#[test]
fn iface_relay__component_pin_undeclared_relay_is_4104() {
    let _lock = common::lock();
    common::reset();
    let bare = r#"
interface BARE(role)
{
    topology = "point to point"
    pins = [
        [1,2,3,4] = [VBUS, DP, DM, GND]
    ]
    role HOST {
        peer = DEVC(1)
    }
    role DEVC {
        peer = HOST(1)
    }
}
component HOSTX
{
    pins = [
        [10,11,12,13] = UB::BARE(RELAY)
    ]
}
module main
{
    HOSTX h1
}
"#;
    let uri: McURI = "/mcc/iface-relay-comp-pin.mc".to_string();
    mcc::mcc_load_from_string(&uri, bare);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let hits: Vec<_> = mcc::mcc_diagnose_all()
        .into_iter()
        .filter(|d| d.code == 4104)
        .collect();
    assert_eq!(hits.len(), 1, "undeclared role on a component pin → E4104: {hits:?}");
}
