// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The role-anchored exclusive-peer gate (`IFACE_EXCLUSIVE_PEER_CONFLICT` =
//! 6054, U201 ①②, xtal-oscillator-design.md §2).
//!
//! One adoption lane of a role declaring `exclusive = true` must reach **one**
//! peer-role instance across its terminals. The defect the gate exists for is
//! torn across nets — a resonator body wired `X1` onto one MCU and `X2` onto
//! another puts exactly two family endpoints on each net, so E4122 is quiet
//! on both and each single pairing is a legal mutual-peer connect. Only a
//! whole-net judge over the body (the adoption lane) sees it.
//!
//! The trigger is the role's own declaration, never a family or role name:
//! the XTAL-shaped family below declares `exclusive = true` on both roles,
//! the CLK-shaped one declares nothing — a multi-input receiver lane fed from
//! two transmitters is a legal shape and must stay silent.
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries a
//! member, including the silence branches — the single-side pairing (one
//! terminal with no peer: the analog-peer §1 silence law, not this gate's
//! defect) and the declaration-gated multi-peer lane.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The exclusive family: mutual peers, both roles `exclusive = true` — the
/// XTAL face's shape (Oscillator ↔ Resonator), under neutral names: the gate
/// must read the declaration, not the vocabulary.
const XRES: &str = r#"
interface XTL(role)
{
    topology = "point to point"
    pins = [
        [1,2] = [X1, X2]
    ]
    role Osc {
        peer = Res
        exclusive = true
    }
    role Res {
        peer = Osc
        exclusive = true
    }
}

component CRY
{
    pins = [
        [1,2] = XT::XTL(Res)
    ]
}

component MCU
{
    pins = [
        [3,4] = XTAL::XTL(Osc)
    ]
}
"#;

/// The unrestricted family: same topology, mutual peers, **no** `exclusive`
/// declaration anywhere.
const CLKF: &str = r#"
interface CLKF(role)
{
    topology = "point to point"
    pins = [
        1 = CK
    ]
    role Tx { peer = Rx }
    role Rx { peer = Tx }
}

component CLKTX
{
    pins = [
        [1,2] = TXA::CLKF(Tx)
    ]
}

component CLKRX
{
    pins = [
        [1,2] = CLKIN::CLKF(Rx)
    ]
}
"#;

/// A plain two-pin part with no adoption — the load-capacitor shape of the
/// design doc: it must stay invisible to the family judging.
const PAD: &str = r#"
component PAD
{
    pins = [
        1 = A
        2 = B
    ]
}
"#;

/// Build `main` with the body statements and return the sorted diagnostic
/// codes.
fn build(body: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{XRES}{CLKF}{PAD}module main {{\n    CRY c1\n    MCU m1\n    MCU m2\n    CLKTX t1\n    CLKTX t2\n    CLKRX r1\n    PAD p1\n{body}\n}}\n");
    let uri: McURI = "/mcc/iface-exclusive-peer.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(code: u32, body: &str) -> usize {
    build(body).iter().filter(|&&c| c == code).count()
}

fn gate_body() -> &'static str {
    "    c1.XT -> m1.XTAL"
}

/// The healthy pairing, both directions of the declaration: the resonator
/// lane sees one oscillator instance, the oscillator lane sees one resonator
/// instance. Quiet — and the point-to-point count agrees (two endpoints per
/// net).
#[test]
fn exclusive_peer__one_body_pairing_is_quiet() {
    assert_eq!(
        count(mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT, gate_body()),
        0,
        "one crystal to one MCU must be quiet; got {:#?}",
        build(gate_body())
    );
}

/// The torn pairing — the defect the gate exists for. `c1.XT.X1 -> m1.XTAL.X1`
/// and `c1.XT.X2 -> m2.XTAL.X1`: each net holds exactly two family endpoints
/// (E4122 must stay quiet on both), each single connect is a legal mutual-peer
/// pairing, and only the lane fact — the resonator body reaches two
/// oscillator instances — is the fault. Exactly one fire, from the
/// resonator's lane.
#[test]
fn exclusive_peer__torn_pairing_across_nets_fires_once() {
    let body = "    c1.XT.X1 -> m1.XTAL.X1\n    c1.XT.X2 -> m2.XTAL.X1";
    let codes = build(body);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT)
            .count(),
        1,
        "one torn resonator lane → exactly one 6054; got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 4122).count(),
        0,
        "each net holds exactly two family endpoints — E4122 must be quiet; got {codes:?}"
    );
}

/// The uniqueness law on one net: two oscillator instances land on the
/// resonator's X1 net. The point-to-point count (E4122) fires its own
/// per-net fact; the gate fires the lane fact. Two codes, one root cause —
/// each names a different facet (the net's endpoint count vs the body's peer
/// set), so both stay.
#[test]
fn exclusive_peer__two_peers_on_one_net_fires_both_facets() {
    let body = "    m1.XTAL.X1 + c1.XT.X1 + m2.XTAL.X1";
    let codes = build(body);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT)
            .count(),
        1,
        "the resonator lane reaches two oscillator instances → 6054; got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 4122).count(),
        1,
        "three family endpoints on one net → E4122's own facet; got {codes:?}"
    );
}

/// The silence branch the doc names: one terminal paired, the other on a
/// part that adopts nothing (the load-capacitor shape). The single-side
/// silence law (analog-peer design §1: only both-sides-declared pairs are
/// judged) keeps the gate quiet — one peer instance is not a conflict.
#[test]
fn exclusive_peer__single_side_pairing_stays_silent() {
    let body = "    c1.XT.X1 -> m1.XTAL.X1\n    c1.XT.X2 -> p1.1";
    assert_eq!(
        count(mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT, body),
        0,
        "one paired terminal plus one unadopted one is not a torn pairing; got {:#?}",
        build(body)
    );
}

/// The declaration gate: the unrestricted family's receiver lane spans two
/// terminals fed from **two different transmitter instances** — exactly the
/// torn shape, but the roles declare no `exclusive`, so multi-peer pairing
/// is a legal shape and the gate must say nothing.
#[test]
fn exclusive_peer__undeclared_role_pairs_unrestricted() {
    let body = "    t1.TXA.CKA -> r1.CLKIN.CKA\n    t2.TXA.CKA -> r1.CLKIN.CKB";
    assert_eq!(
        count(mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT, body),
        0,
        "a role without `exclusive` pairs unrestricted; got {:#?}",
        build(body)
    );
}

/// The message names the body and both peers: family, lane, owner, role,
/// count, peer instance paths.
#[test]
fn exclusive_peer__message_names_lane_and_peers() {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{XRES}{CLKF}{PAD}module main {{\n    CRY c1\n    MCU m1\n    MCU m2\n    CLKTX t1\n    CLKTX t2\n    CLKRX r1\n    PAD p1\n    c1.XT.X1 -> m1.XTAL.X1\n    c1.XT.X2 -> m2.XTAL.X1\n}}\n"
    );
    let uri: McURI = "/mcc/iface-exclusive-peer-msg.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    let rows: Vec<_> = mcc::check::nets::run_net_checks(&table)
        .into_iter()
        .filter(|r| r.code == mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT)
        .collect();
    assert_eq!(rows.len(), 1, "one torn lane, one row: {rows:?}");
    let m = &rows[0].message;
    assert!(
        m.contains("XTL") && m.contains("XT") && m.contains("Res"),
        "message must name the family, the lane and the role: {m}"
    );
    assert!(
        m.contains("main.m1") && m.contains("main.m2"),
        "message must name both peer instances: {m}"
    );
}
