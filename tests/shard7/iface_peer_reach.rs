// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The peer-undershoot gate (`IFACE_PEER_UNREACHED` = 6063, U391,
//! reset-intent-design.md §2 — both structural candidates: chain
//! reachability and POR-supervisor existence).
//!
//! A direction-less adoption lane (an `io` member — the quadrant the
//! chain-reach gate 6060 leaves unjudged) whose role declares an exact-one
//! peer must share its whole merged conductor with either the declared
//! peer role or any terminal outside the family. The defect is the true
//! orphan — conductor exhaustion — never "zero reachable sources": an
//! RC-only reset network is a legal reset source (the b4511 probe
//! ruling), because a resistor pin, a capacitor pin, a button pin is a
//! non-family endpoint.
//!
//! The trigger is the declaration, never a family or role name: a bare
//! `peer = X` states no bound and is never judged; a direction-shaped
//! lane is 6060's object; a same-family *different-role* endpoint is
//! neither peer nor structure (two bare receivers exhaust the conductor
//! together).
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries a
//! member — paired quiet, RC-only quiet (the b4511 regression pin),
//! button quiet, RC behind a module port quiet, series-terminator quiet,
//! dual-source quiet (with 6054 firing its own facet), nc'd-structure
//! fire, two-receiver fire, and the two silence shapes (source-shaped
//! lane, bound-less direction-less pair).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The reset face's shape under neutral names — the RST family with the
/// roles renamed: `Src` declares its member pin `out` (a direction shape,
/// 6060's quadrant), `Rcv` declares `io` (no direction shape — this
/// gate's quadrant) and the exact-one bound `peer = Src(1)`.
const RSF: &str = r#"
interface RSF(role)
{
    topology = "point to point"
    pins = [
        1 = SG
    ]
    role Src
    {
        pins = [
            out 1 = SG
        ]
        peer = Rcv
    }
    role Rcv
    {
        pins = [
            io 1 = SG
        ]
        peer = Src(1)
    }
}

component PSRC
{
    pins = [
        1 = OUT::RSF(Src)
    ]
}

component PRCV
{
    pins = [
        1 = IN::RSF(Rcv)
    ]
}
"#;

/// The bound-less direction-less pair: mutual peers, no cardinality, no
/// direction words — the passive-leaf shape. No lane of this family is
/// ever a candidate (no exact-one bound to undershoot).
const BRF: &str = r#"
interface BRF(role)
{
    pins = [
        1 = X
    ]
    role Pa { peer = Pb }
    role Pb { peer = Pa }
}

component XA
{
    pins = [
        1 = A::BRF(Pa)
    ]
}

component XB
{
    pins = [
        1 = B::BRF(Pb)
    ]
}
"#;

/// A plain two-pin part with no adoption — the anonymous endpoint that
/// silences a family lane: the resistor / capacitor / button stand-in.
const PAD: &str = r#"
component PAD
{
    pins = [
        1 = P
        2 = N
    ]
}

component CAP
{
    pins = [
        1 = P
        2 = N
    ]
}
"#;

/// A submodule with one family-typed role-less port — the passthrough
/// conductor. The port is role-less by law (E4184); the RECEIVER rides the
/// child side, the RC rides the parent side, and the conductor merge (arm
/// b) joins them.
const SUB: &str = r#"
module mid(io rst::RSF())
{
    PRCV p
    p.IN.SG -> rst
}
"#;

/// Build `main` flat with the body statements and return the sorted
/// diagnostic codes.
fn build(body: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{RSF}{BRF}{PAD}{SUB}module main {{\n    PSRC s1\n    PSRC s2\n    PRCV p1\n    PRCV p2\n    PAD d1\n    CAP c1\n    XA xa\n    XB xb\n    mid u1\n{body}\n}}\n");
    let uri: McURI = "/mcc/iface-peer-reach.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(code: u32, body: &str) -> usize {
    build(body).iter().filter(|&&c| c == code).count()
}

/// The direct pair — the declared peer on the receiver's own conductor.
/// Quiet.
#[test]
fn peer_reach_paired_body_is_quiet() {
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, "    p1.IN.SG -> s1.OUT.SG"),
        0,
        "a directly paired receiver must be quiet; got {:#?}",
        build("    p1.IN.SG -> s1.OUT.SG")
    );
}

/// The RC-only reset network — the b4511 probe shape this gate must keep
/// legal: resistor and capacitor terminals on the conductor are non-family
/// endpoints, the structure witness. Zero fires.
#[test]
fn peer_reach_rc_only_reset_is_quiet() {
    let body = "    p1.IN.SG + d1.P + c1.P";
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, body),
        0,
        "an RC-only reset network is a legal reset source; got {:#?}",
        build(body)
    );
}

/// A single button pin — one non-family terminal is already structure.
#[test]
fn peer_reach_single_plain_terminal_is_quiet() {
    let body = "    p1.IN.SG + d1.P";
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, body),
        0,
        "any mounted non-family terminal silences the lane; got {:#?}",
        build(body)
    );
}

/// The series terminator — the receiver's net leads to a plain part whose
/// far side dangles. The plain pin on the conductor is structure either
/// way; what the far side does is not this gate's fact.
#[test]
fn peer_reach_series_plain_part_is_quiet() {
    let body = "    p1.IN.SG -> d1.P";
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, body),
        0,
        "a series plain part is structure; got {:#?}",
        build(body)
    );
}

/// The RC behind a module port: the RECEIVER inside the submodule, the RC
/// on the parent side. The port-passthrough merge (arm b) joins the faces
/// into one conductor, and the family-typed role-less port face itself
/// counts as structure — conservative silence for the dangling-port shape
/// (R12/C4's object).
#[test]
fn peer_reach_rc_behind_module_port_is_quiet() {
    let body = "    d1.P -> u1.rst";
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, body),
        0,
        "structure across a role-less port keeps the receiver quiet; got {:#?}",
        build(body)
    );
}

/// Overshoot is 6054's object, not this gate's: two sources on the
/// receiver's conductor satisfy the peer arm twice over — the undershoot
/// gate stays silent while the exclusive-peer gate fires its own facet.
#[test]
fn peer_reach_dual_source_leaves_overshoot_to_6054() {
    let body = "    s1.OUT.SG + s2.OUT.SG + p1.IN.SG";
    let codes = build(body);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_PEER_UNREACHED)
            .count(),
        0,
        "a fed receiver is quiet here however many sources; got {codes:?}"
    );
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT)
            .count(),
        1,
        "two peer bodies past the exact-one bound → 6054's own facet; got {codes:?}"
    );
}

/// The orphan — the defect the gate exists for. The only other terminal is
/// a fully nc-marked part: mounted-but-not-connected terminals count on
/// neither side, so the conductor is exhausted by its own family. One
/// fire, and the wired-while-marked facet is P6's own object.
#[test]
fn peer_reach_nc_marked_structure_does_not_count() {
    let _lock = common::lock();
    common::reset();
    let src = format!("{RSF}{BRF}{PAD}{SUB}module main {{\n    PSRC s1\n    PSRC s2\n    PRCV p1\n    PRCV p2\n    PAD d1\n    CAP c1 @ncpin(1,2)\n    XA xa\n    XB xb\n    mid u1\n    p1.IN.SG + c1.P\n}}\n");
    let uri: McURI = "/mcc/iface-peer-reach-nc.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::IFACE_PEER_UNREACHED)
            .count(),
        1,
        "an nc'd structure is no structure — the orphan fires once; got {codes:?}"
    );
}

/// Two bare receivers share one conductor: each is a same-family
/// different-role endpoint to the other — neither peer nor structure — so
/// the conductor is exhausted and both fire, one per lane.
#[test]
fn peer_reach_two_bare_receivers_fire_once_each() {
    let body = "    p1.IN.SG + p2.IN.SG";
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, body),
        2,
        "two receivers alone on a conductor → two 6063, one per lane; got {:#?}",
        build(body)
    );
}

/// A source-shaped lane is 6060's object, never judged here — however
/// bare its company.
#[test]
fn peer_reach_source_shaped_lane_is_never_judged() {
    let body = "    s1.OUT.SG + d1.P";
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, body),
        0,
        "a direction-shaped lane is outside the trigger; got {:#?}",
        build(body)
    );
}

/// The bound-less direction-less pair: mutual peers with no cardinality
/// state no exact-one bound, so no lane of the family is a candidate —
/// paired or not.
#[test]
fn peer_reach_bound_less_pair_is_never_judged() {
    let body = "    xa.A.X -> xb.B.X";
    assert_eq!(
        count(mcc::errcodes::IFACE_PEER_UNREACHED, body),
        0,
        "no declared bound, no undershoot judge; got {:#?}",
        build(body)
    );
}
