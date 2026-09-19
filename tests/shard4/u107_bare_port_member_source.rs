// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP §1 U107 ③: a bare container port's member table comes from the port it
//! is **paired with in the body**.
//!
//! `io SPI` declares no members, and the reference side reads members by name
//! (①, `b3545`), so nothing could compare the two faces and a body statement
//! folded four conductors into one net in silence. The ruling taken for ③ is
//! "complete the bare port from its paired port", and the pair is the same in
//! both directions a port can face: `io [8:11] = SPI{SCLK, …}` on a device and
//! `io SPI{SCLK, …}` on a module both declare their members the same way, so
//! one rule reads both (the boundary-formal block, `fcallinst.rs`).
//!
//! What this lock reads is the **pairing**, member by member: with a declaring
//! peer each of the four lanes joins the peer's own member to the container
//! port's member of the same name. A count would not distinguish the states --
//! the collapsed reading also has four conductor-level nets. Nor would a net
//! *name*, since the container side is spelled by whatever its declaration
//! gives (`SPI/SCLK` when the port declares nothing, `SPI.SCLK` when it does);
//! which of those two spellings a completed bare port takes is U107 ①/②, still
//! open, and this lock is deliberately blind to it.
//!
//! The negative twin keeps the lock from passing on "any bare port with a peer
//! zips": a peer that declares **no** members (and one that declares a single
//! member, below the two-member floor a bus needs) must leave the statement
//! exactly as it was -- one net between the two bare ports.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::{BTreeSet, HashSet};

use mcc::{DiagnosticLevel, McIds};

/// The four member names the peer module declares, in declaration order.
const MEMBERS: [&str; 4] = ["SCLK", "MOSI", "CSN", "MISO"];

/// The container port as written (`io SPI` vs `io SPI{…}`).
fn bare_or_declared(declared: bool) -> String {
    if declared {
        format!("io SPI{{{}}}", MEMBERS.join(", "))
    } else {
        "io SPI".to_string()
    }
}

/// A board whose sub-module owns a wire-to-flash macro `func wire(SPI)`, and
/// whose peer `p` declares the SPI members the container may or may not repeat.
///
/// `main` hands the formal a **four-member** actual (`BUS`), so the boundary is
/// four lanes wide in every variant and the only variable left is what `SUBM`'s
/// own port says.
fn board(container_declared: bool, peer: &str) -> String {
    format!(
        r#"module PEERM
{{
    {peer}
}}

module SUBM
{{
    {container}
    PEERM p
    func wire(SPI)
    {{
        SPI + p.SPI
    }}
}}

module main
{{
    io BUS{{{members}}}
    SUBM s
    s.wire(BUS)
}}
"#,
        container = bare_or_declared(container_declared),
        members = MEMBERS.join(", "),
    )
}

/// The two bare ports of the body statement, in the spelling the engine gives
/// them when neither side declares members.
const COLLAPSED: [&str; 2] = ["main.s.SPI", "main.s.p.SPI"];

struct Built {
    /// Every net's member paths, as a set per net: order within a net is not a
    /// reading, and comparing sets is what keeps this from pinning a walk.
    nets: Vec<BTreeSet<String>>,
    errors: Vec<(u32, String)>,
}

impl Built {
    /// The one net whose members include `path`, or a panic naming what the
    /// board had instead -- a second net under one member is itself a failure.
    fn net_of(&self, path: &str) -> &BTreeSet<String> {
        let hits: Vec<&BTreeSet<String>> = self
            .nets
            .iter()
            .filter(|n| n.iter().any(|m| m == path))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "`{path}` must be on exactly one net: {hits:?}"
        );
        hits[0]
    }

    /// Is there a net joining exactly these members?
    fn has_net(&self, members: &[&str]) -> bool {
        let want: BTreeSet<String> = members.iter().map(|m| (*m).to_string()).collect();
        self.nets.iter().any(|n| *n == want)
    }
}

/// The member key of a port path -- the text after its last `.` or `/`, which
/// is the spelling difference this lock refuses to depend on.
fn member_key(path: &str) -> &str {
    let cut = path.rfind(['.', '/']).map(|i| i + 1).unwrap_or(0);
    &path[cut..]
}

fn build(tag: &str, source: &str) -> Built {
    let _lock = common::lock();
    common::reset();

    let uri = format!("/mcc/u107-{tag}.mc");
    mcc::mcc_load_from_string(&uri, source);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000)
        .unwrap_or_else(|e| panic!("flat build failed for {tag}: {e:?}"));

    let nets: Vec<BTreeSet<String>> = table
        .get_nets()
        .iter()
        .map(|net| {
            net.points
                .iter()
                .filter_map(|p| table.get_entry(*p).map(|e| e.path.clone()))
                .collect()
        })
        .filter(|n: &BTreeSet<String>| !n.is_empty())
        .collect();

    let errors: Vec<(u32, String)> = mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .map(|d| (d.code, d.msg.clone()))
        .collect();

    Built { nets, errors }
}

/// One lane of the fixed reading: the peer's member `N` shares a two-member net
/// with the container port's member `N` -- whichever way that side is spelled.
fn assert_lane_joins_the_same_member(b: &Built, member: &str) {
    let peer_path = format!("main.s.p.SPI.{member}");
    let net = b.net_of(&peer_path);
    assert_eq!(
        net.len(),
        2,
        "`{peer_path}` must pair with one member, not {net:?}"
    );
    let other = net
        .iter()
        .find(|m| *m != &peer_path)
        .expect("the net has two members");
    assert!(
        other.starts_with("main.s.SPI"),
        "`{peer_path}` paired with `{other}`, which is not the container's own port member"
    );
    assert_eq!(
        member_key(other),
        member,
        "`{peer_path}` paired with `{other}` -- a member of the same name was expected"
    );
}

// -- 1. the fixed branch: a bare port paired with a declaring module peer --

/// `io SPI` in `SUBM`, paired with `p.SPI` (a module port declaring the four
/// names) and handed a four-member actual. Each of the four conductors keeps
/// its own net, and the peer's member is on the net of the container's member
/// of the same name -- not on one net with all four.
#[test]
fn u107__bare_container_port_takes_its_members_from_the_paired_peer_port() {
    let b = build("bare-module-peer", &board(false, &bare_or_declared(true)));

    assert!(
        b.errors.is_empty(),
        "a shape the peer declares must not be an error: {:?}",
        b.errors
    );
    for member in MEMBERS {
        assert_lane_joins_the_same_member(&b, member);
    }
    // The reading this rules out: all four conductors on one net between the
    // two bare ports, which is what "no member table" produced in silence.
    assert!(
        !b.has_net(&COLLAPSED),
        "the four conductors were folded together again: {:?}",
        b.nets
    );
}

// -- 2. the twin: the same board with the port's members written out --

/// The same board, the container port **explicitly** declaring the four names.
/// The pairing must be identical member for member, so the test above cannot be
/// satisfied by a rule that zips whenever a peer exists.
#[test]
fn u107__a_declared_container_port_pairs_the_same_way() {
    let b = build(
        "declared-module-peer",
        &board(true, &bare_or_declared(true)),
    );

    assert!(b.errors.is_empty(), "unexpected error: {:?}", b.errors);
    for member in MEMBERS {
        assert_lane_joins_the_same_member(&b, member);
    }
    assert!(
        !b.has_net(&COLLAPSED),
        "the four conductors were folded together: {:?}",
        b.nets
    );
}

// -- 3. the negative twin: a peer that declares no members --

/// A peer port that declares nothing has no member table to give, so the bare
/// container port gets none either and the statement stays what it was: one net
/// between the two bare ports. Without this the fix above would also pass on an
/// engine that minted members from thin air.
#[test]
fn u107__a_peer_declaring_no_members_completes_nothing() {
    let b = build("bare-bare-peer", &board(false, "io SPI"));

    assert!(b.errors.is_empty(), "unexpected error: {:?}", b.errors);
    assert!(
        b.has_net(&COLLAPSED),
        "the statement must stay one net between the two bare ports: {:?}",
        b.nets
    );
}

/// …and neither does a peer declaring a **single** member: a bus needs two, so
/// the floor that the component branch already applied is applied here too.
#[test]
fn u107__a_single_member_peer_is_below_the_floor() {
    let b = build("bare-one-member-peer", &board(false, "io SPI{SCLK}"));

    assert!(b.errors.is_empty(), "unexpected error: {:?}", b.errors);
    assert!(
        b.has_net(&COLLAPSED),
        "a one-member peer must not shape the statement: {:?}",
        b.nets
    );
}

/// Guard against the whole file passing vacuously: the boards must actually
/// reach the four-member boundary, i.e. `main.BUS` must be spelled out member
/// by member somewhere. If a fixture edit flattened the actual to a scalar,
/// every assertion above would hold for the wrong reason.
#[test]
fn u107__the_actual_still_reaches_the_boundary_member_by_member() {
    let b = build("boundary-witness", &board(false, &bare_or_declared(true)));

    let container_paths: HashSet<&str> = b
        .nets
        .iter()
        .flatten()
        .map(|p| p.as_str())
        .filter(|p| p.starts_with("main.s.SPI"))
        .collect();
    assert_eq!(
        container_paths.len(),
        MEMBERS.len(),
        "the container port must carry one endpoint per member: {container_paths:?}"
    );
    for member in MEMBERS {
        assert!(
            container_paths
                .iter()
                .any(|p| member_key(p) == member && p.contains("SPI")),
            "no container endpoint names `{member}`: {container_paths:?}"
        );
    }
}
