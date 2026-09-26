// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Module-body interface chain endpoints reach the connect rule (U307).
//!
//! `n1::LNKB(Mh) -> n2::LNKB(...)` downgrades each endpoint to an owner-less
//! member point inside `phrase_to_members`; the family/role now survive via
//! the pre-flatten record (`chain_iface_endpoints`), so the statement-level
//! judge sees what the component-pin half always saw: family equality
//! (E4120), role mutual-peer (E4121), roleless pairing stays quiet, and a
//! Series middle stays the E4187 category error without leaking its role
//! into the peer judgment.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The family: mutual peers `Mh <-> Sl`, plus two adopters.
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

interface LNK2(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Mh { peer = Sl }
    role Sl { peer = Mh }
}
"#;

/// Sorted diagnostic codes for one build of `main` with `body`.
fn codes(body: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{FAM}module main {{\n{body}\n}}\n");
    let uri: McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(body: &str, uri: &str, code: u32) -> usize {
    codes(body, uri).iter().filter(|&&c| c == code).count()
}

/// U307-1: two module-body endpoints with non-peer roles report E4121 —
/// the pair is judged although neither side owns a component pin.
#[test]
fn u307__module_pair_role_peer_mismatch_fires() {
    let hits = count(
        "    n1::LNKB(Mh) -> n2::LNKB(Mh)\n",
        "/mcc/u307-pair-mismatch.mc",
        4121,
    );
    assert_eq!(hits, 1, "non-peer module-body endpoint roles report E4121");
}

/// U307-2: mutual peers `Mh <-> Sl` are the legal form — silent.
#[test]
fn u307__module_pair_mutual_peers_stay_silent() {
    let hits = count(
        "    n1::LNKB(Mh) -> n2::LNKB(Sl)\n",
        "/mcc/u307-pair-mutual.mc",
        4121,
    );
    assert_eq!(hits, 0, "mutual peers pair in peace");
}

/// U307-3: cross-family module-body endpoints report E4120.
#[test]
fn u307__module_pair_cross_family_fires() {
    let hits = count(
        "    n1::LNKB(Mh) -> n2::LNK2(Sl)\n",
        "/mcc/u307-cross-family.mc",
        4120,
    );
    assert_eq!(hits, 1, "different families cannot pair");
}

/// U307-4: a role-less side pairs positionally — no role check, silent.
#[test]
fn u307__module_endpoint_roleless_side_stays_silent() {
    let hits = count(
        "    n1::LNKB(Mh) -> n2::LNKB()\n",
        "/mcc/u307-roleless.mc",
        4121,
    );
    assert_eq!(hits, 0, "a role-less side pairs without the role check");
}

/// U307-5: a module-body endpoint meets a component adoption pin — the pair
/// is judged across the two halves; both writing `Mh` reports E4121.
#[test]
fn u307__module_endpoint_vs_component_pin_mismatch_fires() {
    let hits = count(
        "    HOSTB ha\n    n1::LNKB(Mh) -> ha.IF\n",
        "/mcc/u307-vs-comp-mismatch.mc",
        4121,
    );
    assert_eq!(hits, 1, "module endpoint and adoption pin are one pair");
}

/// U307-6: the same shape with mutual peers (`Mh` vs `Sl`) — silent.
#[test]
fn u307__module_endpoint_vs_component_pin_mutual_silent() {
    let hits = count(
        "    DEVB da\n    n1::LNKB(Mh) -> da.IF\n",
        "/mcc/u307-vs-comp-mutual.mc",
        4121,
    );
    assert_eq!(hits, 0, "mutual peers across the two halves stay legal");
}

/// U307-7: a role on a Series middle stays the E4187 category error and does
/// NOT leak into the peer judgment (no E4121 on top of it).
#[test]
fn u307__mediator_role_reports_4187_only() {
    let body = "    HOSTB ha\n    HOSTB hb\n    ha.IF -> m1::LNKB(Mh) -> hb.IF\n";
    assert_eq!(
        count(body, "/mcc/u307-mediator.mc", 4187),
        1,
        "a role on the chain middle is the E4187 shape"
    );
    assert_eq!(
        count(body, "/mcc/u307-mediator.mc", 4121),
        0,
        "the mediator judges roleless; no peer diagnostic leaks"
    );
}

/// U307-8: `+` junction operands are all endpoint positions — the pair is
/// judged there too.
#[test]
fn u307__parallel_junction_endpoints_are_judged() {
    let hits = count(
        "    n1::LNKB(Mh) + n2::LNKB(Mh)\n",
        "/mcc/u307-junction.mc",
        4121,
    );
    assert_eq!(hits, 1, "a `+` junction has no middle; every operand is an end");
}
