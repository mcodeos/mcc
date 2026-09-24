// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The chain-level source-reach gate (`IFACE_CHAIN_SOURCE_UNREACHED` = 6060,
//! U112 ②, clock-intent-design.md §2.2).
//!
//! A sink-shaped adoption lane — a role whose member pins all declare `in` —
//! must reach a source of its own family along the adoption chain. The walk
//! crosses whole nets and distributor instances (a sink lane whose owner also
//! declares a same-family source pin elsewhere hands the walk over to its
//! source nets); the defect is the orphan, zero reachable sources.
//!
//! Reachability, not uniqueness: a sink fed from several sources stays quiet.
//! The trigger is the role's declared pin-direction shape, never a family or
//! role name: a mixed role (`out` TX beside `in` RX) and a direction-less
//! role (the passive-leaf pair) are never judged, and a source-shaped lane is
//! never judged at all — an unconnected source drives nothing, the load law.
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries a
//! member — direct feed, distributor feed, orphan alone, orphan behind a
//! dead-end distributor, multi-source quiet, and the three silence shapes
//! (mixed role, direction-less role, unwired source lane).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The unidirectional family: the CLK face's shape under neutral names —
/// `Src` declares every member pin `out`, `Snk` every member pin `in`.
const CKF: &str = r#"
interface CKF(role)
{
    topology = "point to point"
    mode = ["unidirectional"]
    pins = [
        1 = CK
    ]
    role Src
    {
        pins = [
            out 1 = CK
        ]
        peer = Snk
    }
    role Snk
    {
        pins = [
            in 1 = CK
        ]
        peer = Src
    }
}

component OSC
{
    pins = [
        1 = DRV::CKF(Src)
    ]
}

component SINK
{
    pins = [
        1 = RCV::CKF(Snk)
    ]
}

component RPT
{
    pins = [
        1 = RIN::CKF(Snk)
        2 = ROUT::CKF(Src)
    ]
}
"#;

/// The mixed family: one role declaring `out` TX beside `in` RX — no uniform
/// direction shape, so no chain gate may judge its lanes.
const MBF: &str = r#"
interface MBF(role)
{
    pins = [
        [1,2] = [TX, RX]
    ]
    role DCE
    {
        pins = [
            out 1 = TX
            in 2 = RX
        ]
    }
}

component DCE1
{
    pins = [
        [1,2] = LINK::MBF(DCE)
    ]
}
"#;

/// The direction-less family: the XTAL face's shape — mutual roles with no
/// direction words anywhere (the passive-leaf law).
const XT2: &str = r#"
interface XT2(role)
{
    topology = "point to point"
    pins = [
        [1,2] = [X1, X2]
    ]
    role Osc { peer = Res }
    role Res { peer = Osc }
}

component XTA
{
    pins = [
        [1,2] = XT::XT2(Osc)
    ]
}

component XTB
{
    pins = [
        [1,2] = XT::XT2(Res)
    ]
}
"#;

/// A plain two-pin part with no adoption — the anonymous endpoint a family
/// lane may legally dangle toward.
const PAD: &str = r#"
component PAD
{
    pins = [
        1 = A
        2 = B
    ]
}
"#;

/// Build `main` with the body statements and return the sorted
/// `(code, message)` pairs.
fn build_diag(body: &str) -> Vec<(u32, String)> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{CKF}{MBF}{XT2}{PAD}module main {{\n    OSC o1\n    OSC o2\n    SINK s1\n    SINK s2\n    RPT r1\n    DCE1 d1\n    XTA x1\n    XTB x2\n    PAD p1\n{body}\n}}\n");
    let uri: McURI = "/mcc/iface-chain.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut diag: Vec<(u32, String)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    diag.sort_unstable();
    diag
}

fn build(body: &str) -> Vec<u32> {
    build_diag(body).into_iter().map(|(c, _)| c).collect()
}

fn count(code: u32, body: &str) -> usize {
    build(body).iter().filter(|&&c| c == code).count()
}

/// The direct feed — a source of the family on the sink's own net. Quiet.
#[test]
fn chain_source_on_the_sink_net_is_quiet() {
    assert_eq!(
        count(mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED, "    s1.RCV.CK -> o1.DRV.CK"),
        0,
        "a directly fed sink must be quiet; got {:#?}",
        build("    s1.RCV.CK -> o1.DRV.CK")
    );
}

/// The buffer chain — the classic clock-distributor shape. Each sink's own
/// net carries its family's source (the oscillator feeds the buffer's input,
/// the buffer's output lane feeds the sink): quiet on every lane.
#[test]
fn chain_buffer_topology_is_quiet() {
    let body = "    o1.DRV.CK -> r1.RIN.CK\n    r1.ROUT.CK -> s1.RCV.CK";
    assert_eq!(
        count(mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED, body),
        0,
        "every sink under an oscillator through a buffer must be quiet; got {:#?}",
        build(body)
    );
}

/// The hand-off walk — no source on the sink's net at all: only another
/// sink lane whose owner drives the family from a different net. The sink
/// stays quiet, which only the cross-net walk can explain; the
/// distributor's own input lane, whose net nothing drives, is the one true
/// orphan here and fires alone.
#[test]
fn chain_hand_off_walk_feeds_the_sink_and_names_the_true_orphan() {
    let body = "    s1.RCV.CK -> r1.RIN.CK\n    r1.ROUT.CK -> o1.DRV.CK";
    let fires: Vec<String> = build_diag(body)
        .iter()
        .filter(|(c, _)| c == &mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED)
        .map(|(_, m)| m.clone())
        .collect();
    assert_eq!(
        fires.len(),
        1,
        "s1 rides the hand-off walk; r1's undriven input is the one orphan; got {fires:?}"
    );
    assert!(
        fires[0].contains("lane 'RIN'"),
        "the fire must anchor at the distributor's own input lane, not at the fed sink; got {fires:?}"
    );
}

/// The orphan — the defect the gate exists for. The sink's net carries no
/// family source and no distributor: exactly one fire, anchored at the sink.
#[test]
fn chain_orphan_sink_fires_once() {
    let body = "    s1.RCV.CK -> p1.1";
    assert_eq!(
        count(mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED, body),
        1,
        "a sink with no reachable source → exactly one 6060; got {:#?}",
        build(body)
    );
}

/// The dead-end distributor: its source lane is unwired — an unconnected
/// source drives nothing (the load law), so the hand-off face is empty. Both
/// sink lanes on the shared net are orphans now — the sink and the
/// distributor's own input — and the unwired source lane itself never
/// fires: two fires, both at sink lanes.
#[test]
fn chain_dead_end_distributor_leaves_both_sinks_orphan() {
    let body = "    s1.RCV.CK -> r1.RIN.CK";
    let fires: Vec<String> = build_diag(body)
        .iter()
        .filter(|(c, _)| c == &mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED)
        .map(|(_, m)| m.clone())
        .collect();
    assert_eq!(
        fires.len(),
        2,
        "an undriven net behind an empty hand-off → both sink lanes orphan; got {fires:?}"
    );
    assert!(fires.iter().all(|m| m.contains("lane 'RCV'") || m.contains("lane 'RIN'")),
        "only sink lanes fire — the unwired source lane is the load law's object; got {fires:?}");
}

/// Two sinks share one source-less net: each lane is its own orphan — two
/// fires, one per sink endpoint.
#[test]
fn chain_two_orphan_sinks_fire_once_each() {
    let body = "    s1.RCV.CK + s2.RCV.CK + p1.1";
    assert_eq!(
        count(mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED, body),
        2,
        "two orphan sink lanes → two 6060; got {:#?}",
        build(body)
    );
}

/// Reachability, not uniqueness: two sources land on the sink's net. The
/// point-to-point count (E4122) fires its own per-net facet, but the chain
/// gate must stay quiet — several reachable sources is a legal shape.
#[test]
fn chain_multi_source_reach_stays_quiet() {
    let body = "    o1.DRV.CK + o2.DRV.CK + s1.RCV.CK";
    let codes = build(body);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED)
            .count(),
        0,
        "several reachable sources must be quiet; got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 4122).count(),
        1,
        "three family endpoints on one net → E4122's own facet; got {codes:?}"
    );
}

/// The mixed role: `out` TX beside `in` RX states no uniform direction
/// shape, so the DCE lane is never judged — neither member fires, wired or
/// not.
#[test]
fn chain_mixed_direction_role_is_never_judged() {
    let body = "    d1.LINK.TX -> p1.1\n    d1.LINK.RX -> p1.2";
    assert_eq!(
        count(mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED, body),
        0,
        "a mixed-direction role must be invisible to the gate; got {:#?}",
        build(body)
    );
}

/// The direction-less pair: the passive-leaf shape declares no direction
/// words, so no lane is sink- or source-shaped and nothing is judged.
#[test]
fn chain_direction_less_roles_are_never_judged() {
    let body = "    x1.XT.X1 -> x2.XT.X1\n    x1.XT.X2 -> x2.XT.X2";
    assert_eq!(
        count(mcc::errcodes::IFACE_CHAIN_SOURCE_UNREACHED, body),
        0,
        "a direction-less role must be invisible to the gate; got {:#?}",
        build(body)
    );
}
