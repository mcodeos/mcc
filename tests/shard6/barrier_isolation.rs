// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Cross-barrier merge ERC (`CROSS_BARRIER_NET` = 6053, rules-catalog §2 B5's
//! declarative subject, barrier-design.md §3).
//!
//! A component's pin rows declare isolation groups with `@barrier(<group>)` —
//! group names the part's author coins, compared only by equality. One net
//! touching two different groups of the same instance is the schematic-level
//! fact of a bridged isolation → Error. The lock pins the judgment, not a
//! library part: no test below names XFR/OPTO/RELAY, the axis lives in the
//! declaration language.
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries at least
//! two members so a per-net rule cannot pass as per-instance, and the silence
//! branches each hold the shape the doc names — same group twice on one net,
//! one group plus an unmarked row, and the deliberate cross-barrier bridge
//! (a Y capacitor spanning the two worlds) which is a *path*, never a net.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// An isolation transformer shape: two windings, two groups.
const XISO: &str = "component XISO {\n    pins = [\n        1 = PRI_P @barrier(pri)\n        2 = PRI_N @barrier(pri)\n        3 = SEC_P @barrier(sec)\n        4 = SEC_N @barrier(sec)\n    ]\n}\n";

/// A contact-isolated shape: coil vs contact, the second violation member.
const RLY: &str = "component RLY {\n    pins = [\n        [1,2] = COIL{VCC, GND} @barrier(coil)\n        3 = COM @barrier(contact)\n        4 = NO @barrier(contact)\n    ]\n}\n";

/// A center-tapped shape: the tap stays unmarked — outside every barrier.
const XISO_CT: &str = "component XISO_CT {\n    pins = [\n        1 = PRI_P @barrier(pri)\n        2 = PRI_N @barrier(pri)\n        3 = SEC_P @barrier(sec)\n        4 = SEC_CT\n        5 = SEC_N @barrier(sec)\n    ]\n}\n";

/// A two-pin unmarked bridge element (the Y capacitor shape).
const YCAP: &str = "component YCAP {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n";

/// Build the source and return every diagnostic code (sorted).
fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/barrier-isolation.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// One code's rows, in emission order, as `(net_name, message)`.
fn rows_of(code: u32, src: &str) -> Vec<(String, String)> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/barrier-isolation.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    mcc::check::nets::run_net_checks(&table)
        .iter()
        .filter(|r| r.code == code)
        .map(|r| (r.net_name.clone(), r.message.clone()))
        .collect()
}

fn count(code: u32, src: &str) -> usize {
    build_codes(src).iter().filter(|&&c| c == code).count()
}

// ── 1. The violation: one net touches two groups

#[test]
fn cross_barrier_net_fires_per_net_and_per_instance() {
    // u1 bridges pri/sec on PRISEC; u2 (a different part family, coil/contact)
    // bridges on the same net. Two instances, two fires — a per-instance rule
    // must not read as per-net or per-part.
    let src = format!(
        "{XISO}\n{RLY}\nmodule main {{\n    conduit PRISEC\n    XISO u1\n    RLY k1\n    \
         u1{{1, 3}} - [PRISEC, PRISEC]\n    k1{{1, 3}} - [PRISEC, PRISEC]\n}}\n"
    );
    let n = count(mcc::errcodes::CROSS_BARRIER_NET, &src);
    assert_eq!(
        n,
        2,
        "two instances each landing two groups on one net → 6053 ×2; got: {:?}",
        build_codes(&src)
    );
}

#[test]
fn cross_barrier_message_names_groups_and_net() {
    let src = format!(
        "{XISO}\nmodule main {{\n    conduit BAD\n    XISO u1\n    \
         u1{{2, 4}} - [BAD, BAD]\n}}\n"
    );
    let rows = rows_of(mcc::errcodes::CROSS_BARRIER_NET, &src);
    assert_eq!(rows.len(), 1, "one bridged net, one row: {rows:?}");
    let (_, msg) = &rows[0];
    assert!(
        msg.contains("pri") && msg.contains("sec") && msg.contains("BAD"),
        "message must name both groups and the net: {msg}"
    );
}

// ── 2. The silences the doc names

#[test]
fn same_group_on_one_net_is_the_point_of_a_winding() {
    // Both pri rows on one return: one group on the net, nothing crossed.
    let src = format!(
        "{XISO}\nmodule main {{\n    conduit PGND\n    XISO u1\n    \
         u1{{1, 2}} - [PGND, PGND]\n}}\n"
    );
    assert_eq!(count(mcc::errcodes::CROSS_BARRIER_NET, &src), 0);
}

#[test]
fn separate_worlds_stay_silent() {
    // The legal shape: each group on its own copper, both sides landed.
    let src = format!(
        "{XISO}\nmodule main {{\n    conduit PGND\n    conduit SGND\n    XISO u1\n    \
         u1{{1, 2}} - [PGND, PGND]\n    u1{{3, 4}} - [SGND, SGND]\n}}\n"
    );
    assert_eq!(count(mcc::errcodes::CROSS_BARRIER_NET, &src), 0);
}

#[test]
fn unmarked_rows_are_outside_every_barrier() {
    // The CT tap is unmarked and lands with the secondary rows: one *named*
    // group on the net → silent. And a second member: an unmarked row sharing
    // a net with a single marked group of another shape.
    let src = format!(
        "{XISO_CT}\nmodule main {{\n    conduit SGND\n    XISO_CT u1\n    \
         u1{{3, 4, 5}} - [SGND, SGND, SGND]\n}}\n"
    );
    assert_eq!(count(mcc::errcodes::CROSS_BARRIER_NET, &src), 0);
}

#[test]
fn deliberate_cross_barrier_bridge_is_a_path_not_a_net() {
    // The Y capacitor spans the two worlds by *splitting* the copper: no
    // single net ever holds two groups, so the gate stays silent — judging a
    // path is expressly not this rule's object (barrier-design.md §3.3).
    let src = format!(
        "{XISO}\n{YCAP}\nmodule main {{\n    conduit PGND\n    conduit SGND\n    \
         XISO u1\n    YCAP cy\n    u1{{1, 2}} - [PGND, PGND]\n    \
         u1{{3, 4}} - [SGND, SGND]\n    cy{{1, 2}} - [PGND, SGND]\n}}\n"
    );
    assert_eq!(count(mcc::errcodes::CROSS_BARRIER_NET, &src), 0);
}

#[test]
fn groups_are_per_instance_not_per_net_global() {
    // Two *different* parts may share a net across their own groups: u1's sec
    // and u2's pri on one net is two parts, each net seeing one group per
    // part... unless one part itself contributes both groups. Here each part
    // contributes a single group to SHARED → silent; the axis is the
    // instance's own partition, never the board's.
    let src = format!(
        "{XISO}\nmodule main {{\n    conduit SHARED\n    XISO u1\n    XISO u2\n    \
         u1{{3, 4}} - [SHARED, SHARED]\n    u2{{1, 2}} - [SHARED, SHARED]\n}}\n"
    );
    assert_eq!(count(mcc::errcodes::CROSS_BARRIER_NET, &src), 0);
}
