// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The E4 flat-net generic peer sweep (`IFACE_ROLE_PEER_CONFLICT` = 6061,
//! U289 ⑥, replicated-binding-design.md §4 check 4).
//!
//! The statement-level peer judge (E4121) only sees endpoint pairs meeting
//! inside one statement. This sweep walks the finished net map, so it fires
//! where the statement judge structurally cannot:
//! - the `-`/`->` chain split into per-adjacency connects — the chain's outer
//!   endpoints share one flat net but never one adjacency;
//! - the module port passthrough — both device sides of a role-less port are
//!   role-known, the port itself is a conductor (E4184's law).
//!
//! Direct connects and `+` junctions stay visible to BOTH judges by ruling
//! (2026-09-25): 4121 owns the statement walk, 6061 owns the net walk, no
//! dedup carry. The trigger is the declaration, never a name: mutual-peer
//! pairs and cross-family pairs stay silent here (the single-side silence law
//! iface_peer.rs already states).
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries a
//! member, including the silence branches — the mutual pair, the cross-family
//! pair, and the role-less far side.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The family: mutual peers `Mh <-> Sl`. Two endpoints both adopting `Mh` are
/// the conflict shape; `Mh` + `Sl` is the healthy shape.
const FAM: &str = r#"
interface LNKB(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Mh { peer = Sl }
    role Sl { peer = Mh }
}

component HOSTB
{
    pins = [
        [1,2] = IF::LNKB(Mh)
    ]
}

component DEVB
{
    pins = [
        [1,2] = IF::LNKB(Sl)
    ]
}
"#;

/// A second family with the same member names — the cross-family silence
/// branch. Family identity is the interface *name* (the statement-level
/// judge's step 1 owns that axis at meetings; this sweep stays off it).
const FAM2: &str = r#"
interface LNKC(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Mh { peer = Sl }
    role Sl { peer = Mh }
}

component HOSTC
{
    pins = [
        [1,2] = IF::LNKC(Mh)
    ]
}
"#;

/// A plain two-pin part — the role-less wiring mediator (the series R/C shape
/// of the design doc).
const PAD2: &str = r#"
component PAD2
{
    pins = [
        1 = P
        2 = N
    ]
}
"#;

/// A submodule with one role-less bus port — the passthrough conductor. The
/// port is role-less by law (E4184); the device sides attached to it on each
/// level carry the roles.
const SUB: &str = r#"
module mid(io bus[1:2]::LNKB())
{
    HOSTB h
    h.1 -> bus.A
    h.2 -> bus.B
}
"#;

/// Build `main` flat with the body statements and return the sorted
/// diagnostic codes.
fn build(body: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{FAM}{FAM2}{PAD2}{SUB}module main {{\n    HOSTB ha\n    HOSTB hb\n    DEVB da\n    HOSTC hc\n    PAD2 p1\n    mid u1\n{body}\n}}\n"
    );
    let uri: McURI = "/mcc/iface-role-peers.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(code: u32, body: &str) -> usize {
    build(body).iter().filter(|&&c| c == code).count()
}

/// The net partition of the same fixture — the chain probe reads it to pin
/// the fixture's shape (all three endpoints on one net).
fn nets_of(body: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{FAM}{FAM2}{PAD2}{SUB}module main {{\n    HOSTB ha\n    HOSTB hb\n    DEVB da\n    HOSTC hc\n    PAD2 p1\n    mid u1\n{body}\n}}\n"
    );
    let uri: McURI = "/mcc/iface-role-peers-nets.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &uri).expect("build");
    let mut partition: Vec<Vec<String>> = net_store
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
    partition.sort();
    partition
}

/// Hole ② — the chain split: `ha -> da -> hb` is two adjacencies, both
/// mutual-peer pairs (`Mh`-`Sl`, `Sl`-`Mh`) at the statement level, so E4121
/// stays quiet on both — but the chain's outer endpoints (`ha` and `hb`, both
/// `Mh`) share one flat net and are not mutual peers. Exactly one sweep fire,
/// and the statement judge must stay at zero: this conflict exists only on
/// the net walk.
#[test]
fn role_peers__chain_outer_endpoints_fire_once() {
    let body = "    ha.IF -> da.IF -> hb.IF";
    let nets = nets_of(body);
    assert!(
        nets.iter().any(|n| n.len() >= 3 && n.contains(&"ha.1".to_string()) && n.contains(&"hb.1".to_string())),
        "fixture shape: the chain's outer endpoints must share one net; got {nets:?}"
    );
    let codes = build(body);
    assert_eq!(
        codes.iter().filter(|&&c| c == mcc::errcodes::IFACE_ROLE_PEER_CONFLICT).count(),
        2,
        "outer Mh-Mh endpoints of one chain net fire once per physical pin pair (pins 1 and 2); got {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 4121).count(),
        0,
        "each adjacency is a mutual pair; the statement judge must stay quiet; got {codes:?}"
    );
}

/// Hole ① — the port passthrough: `ha(Mh)` on the parent side of `u1`'s
/// role-less port, `hb(Mh)` inside the submodule on the other side. Each
/// statement-level judge sees a role-less far end and skips; the sweep
/// unions the two faces of the shared port entry and collides the roles.
#[test]
fn role_peers__module_port_passthrough_fires() {
    let body = "    ha.IF -> u1.bus";
    let codes = build(body);
    assert_eq!(
        codes.iter().filter(|&&c| c == mcc::errcodes::IFACE_ROLE_PEER_CONFLICT).count(),
        2,
        "Mh-port-Mh through a role-less port fires once per physical pin pair; got {codes:?}"
    );
}

/// The direct connect is visible to BOTH judges by ruling (2026-09-25): 4121
/// owns the statement walk, 6061 owns the net walk, no dedup carry.
#[test]
fn role_peers__direct_connect_reports_both_codes() {
    let body = "    ha.IF -> hb.IF";
    assert_eq!(
        count(4121, body),
        1,
        "the statement-level judge owns the meeting; got {:?}",
        build(body)
    );
    assert_eq!(
        count(mcc::errcodes::IFACE_ROLE_PEER_CONFLICT, body),
        2,
        "the net walk reports the same conflict again by ruling, once per physical pin pair; got {:?}",
        build(body)
    );
}

/// The `+` junction shape: same double reporting by ruling.
#[test]
fn role_peers__parallel_junction_reports_both_codes() {
    let body = "    ha.IF + hb.IF";
    assert_eq!(
        count(4121, body),
        1,
        "the junction judge must fire; got {:?}",
        build(body)
    );
    assert_eq!(
        count(mcc::errcodes::IFACE_ROLE_PEER_CONFLICT, body),
        2,
        "the net walk sees the same junction net, once per physical pin pair; got {:?}",
        build(body)
    );
}

/// The healthy pairing: `Mh` + `Sl`, mutual peers, direct — silent here
/// (E4121's own battery owns the healthy-connect claim; this is the sweep's
/// silence branch).
#[test]
fn role_peers__mutual_pair_is_quiet() {
    let body = "    ha.IF -> da.IF";
    assert_eq!(
        count(mcc::errcodes::IFACE_ROLE_PEER_CONFLICT, body),
        0,
        "mutual peers must stay silent on the net walk; got {:?}",
        build(body)
    );
}

/// Cross-family pairs stay silent in the sweep: the family axis is the
/// statement-level judge's step 1 (E4120) object at meetings.
#[test]
fn role_peers__cross_family_pair_is_quiet_here() {
    let body = "    ha.IF -> hc.IF";
    let codes = build(body);
    assert_eq!(
        codes.iter().filter(|&&c| c == mcc::errcodes::IFACE_ROLE_PEER_CONFLICT).count(),
        0,
        "the sweep judges the peer table, not the family axis; got {codes:?}"
    );
}

/// A role-less far side stays silent — the single-side law the exclusive-peer
/// gate already states: one known role is not a collision.
#[test]
fn role_peers__single_role_side_is_quiet() {
    let body = "    ha.IF -> p1";
    assert_eq!(
        count(mcc::errcodes::IFACE_ROLE_PEER_CONFLICT, body),
        0,
        "one role-bearing endpoint is no collision; got {:?}",
        build(body)
    );
}

/// The message names the net, both endpoints with their roles, and the
/// family — the shape lock of the 6xxx family convention.
#[test]
fn role_peers__message_names_both_sides() {
    let uri_msg = build("    ha.IF -> hb.IF")
        .iter()
        .filter(|&&c| c == mcc::errcodes::IFACE_ROLE_PEER_CONFLICT)
        .count();
    assert_eq!(uri_msg, 2, "one conflict per physical pin pair expected");
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{FAM}{FAM2}{PAD2}{SUB}module main {{\n    HOSTB ha\n    HOSTB hb\n    DEVB da\n    HOSTC hc\n    PAD2 p1\n    mid u1\n    ha.IF -> hb.IF\n}}\n"
    );
    let uri: McURI = "/mcc/iface-role-peers-msg.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let msg = mcc::mcc_diagnose_all()
        .into_iter()
        .find(|d| d.code == mcc::errcodes::IFACE_ROLE_PEER_CONFLICT)
        .map(|d| d.msg.clone())
        .expect("conflict message present");
    assert!(msg.contains("Mh"), "both roles named: {msg}");
    assert!(msg.contains("LNKB"), "family named: {msg}");
    assert!(msg.contains("ha"), "first endpoint named: {msg}");
    assert!(msg.contains("hb"), "second endpoint named: {msg}");
}

