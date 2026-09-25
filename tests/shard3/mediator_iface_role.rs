// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E3 wiring-mediator role half (`MEDIATOR_IFACE_ROLE` = 4187,
//! replicated-binding-design.md §4 check 3, U289 ⑥).
//!
//! The role is the link identity of an endpoint terminal pin (design R3): it
//! is legal in a binding row's endpoint (the chain's two ENDS adopt their
//! sides there), but a `-`/`->` chain's MIDDLE — the wiring mediator — only
//! conducts. An interface instance standing mid-chain with a role argument is
//! the category error this lock reports; the module-port half of the same
//! check lives in `module_port_role_free.rs` (E4184).
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries a
//! member — including the silence branches (endpoint roles stay legal, a
//! role-less mediator stays legal, `+` junctions are not chains).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The family: mutual peers `Mh <-> Sl`.
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

/// A plain two-pin part — the role-less wiring mediator.
const PAD2: &str = r#"
component PAD2
{
    pins = [
        1 = P
        2 = N
    ]
}
"#;

/// Sorted diagnostic codes for one build of `main` with `body`.
fn codes(body: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{FAM}{PAD2}module main {{\n    HOSTB ha\n    HOSTB hb\n    DEVB da\n    PAD2 p1\n{body}\n}}\n"
    );
    let uri: McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count_4187(body: &str, uri: &str) -> usize {
    codes(body, uri)
        .iter()
        .filter(|&&c| c == 4187)
        .count()
}

/// E3-1: an interface instance with a role argument standing mid-chain —
/// the mediator is a conductor, not a link party. Fires once.
#[test]
fn mediator_role__mid_chain_iface_role_fires() {
    let hits = count_4187(
        "    ha.IF -> m1::LNKB(Mh) -> hb.IF",
        "/mcc/mediator-role-mid.mc",
    );
    assert_eq!(hits, 1, "a role on the chain middle is the E4187 shape");
}

/// E3-2: roles on the chain ENDPOINTS are the legal form (design §2: the
/// terminal pins rows adopt the sides) — silent.
#[test]
fn mediator_role__endpoint_roles_stay_legal() {
    let hits = count_4187("    ha.IF -> hb.IF", "/mcc/mediator-role-ends.mc");
    assert_eq!(hits, 0, "endpoint roles are the legal positions; got");
}

/// E3-3: a role-less two-pin part mid-chain — the canonical mediator —
/// silent.
#[test]
fn mediator_role__roleless_mediator_is_quiet() {
    let hits = count_4187("    ha.IF -> p1 -> hb.IF", "/mcc/mediator-role-less.mc");
    assert_eq!(hits, 0, "a role-less mediator conducts in peace");
}

/// E3-4: `+` joins are not chains — no middle exists, so no E4187 even with
/// a role-bearing iface instance in the join.
#[test]
fn mediator_role__parallel_join_has_no_middle() {
    let hits = count_4187(
        "    ha.IF + m1::LNKB(Mh)",
        "/mcc/mediator-role-join.mc",
    );
    assert_eq!(hits, 0, "a `+` join has no chain middle");
}

/// E3-5: a two-member chain nested in a group — endpoints only, silent.
#[test]
fn mediator_role__grouped_two_member_chain_is_quiet() {
    let hits = count_4187(
        "    (ha.IF -> da.IF) + hb.IF",
        "/mcc/mediator-role-group.mc",
    );
    assert_eq!(hits, 0, "every element of a two-member chain is an end");
}
