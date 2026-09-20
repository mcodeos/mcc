// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP §1 U127 on the pin-backed boundary shape: each side of a func-call
//! boundary keeps its own spelling, and the parent never names the peer's lane.
//!
//! The b3559-era reading of the disease: the parent wrote the sub-module-side
//! boundary point with the **actual's** member name (`main.MCU513.SPI._CS`
//! under a container whose own side spelled the lane differently), and every
//! spelling that matched nothing came back as `warning[E3175]`
//! (`MODULE_PORT_NOT_FOUND`) -- a naming axis judged with a matching rule.
//!
//! The b3569-era residual this lock closes: the boundary spelled the
//! container's pin `main.s.8` (component segment dropped) while the body
//! statement spelled the same pin `main.s.c.8` -- one pin, two identities,
//! four disjoint lanes. The law locked here, per the platform's junction
//! model (insttab A′ 3: a boundary junction id lands on both the child
//! internal net and the parent net):
//!
//! * the boundary pairs by position (written order), not by name: the actual's
//!   member at position k reaches the pin the sub-module's declaration gives
//!   the k-th member, even though **no name agrees** across the boundary;
//! * one pin carries **one identity**: the entry lives at the full instance
//!   path (`main.s.c.<pin>`, the body statement's spelling) and no orphan
//!   entry is minted at the dropped spelling (`main.s.<pin>`);
//! * that one id is the junction: the parent's boundary segment and the
//!   body statement's segment both carry it, so the conductor is continuous;
//! * `E3175` stays silent -- a naming difference is not a missing port;
//! * the matching-name control pairs identically, so the lock cannot pass on
//!   "the names happened to agree".
//!
//! Board is self-contained: the `SPI` interface is declared inline (role
//! tables included) so the flat harness needs no library.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{DiagnosticLevel, InstTable, McIds};

/// The sub-module side's member names, in declaration order.
const SUB_MEMBERS: [&str; 4] = ["SCLK", "MOSI", "CSN", "MISO"];

/// The actual's member names, deliberately **all different** from
/// `SUB_MEMBERS` (the disease's shape: no name agrees across the boundary).
const ARG_MEMBERS: [&str; 4] = ["_CS", "DO", "DI", "SCK"];

/// The pins the container's component declares for the port, in the same
/// order as `SUB_MEMBERS` (its declaration maps member k to pin `8 + k`).
const PINS: [&str; 4] = ["8", "9", "10", "11"];

fn board(arg_members: &[&str]) -> String {
    format!(
        r#"interface SPI(role)
{{
    role Master
    {{
        pins = [
            1 = CS
            2 = SCLK
            3 = MISO
            4 = MOSI
        ]
        peer = Slave
    }}
    role Slave
    {{
        pins = [
            1 = CS
            2 = SCLK
            3 = SO
            4 = SI
        ]
        peer = Master
    }}
}}

component CHIP
{{
    pins = [
        io [8:11] = SPI{{{members}}}::SPI(Master)
    ]
}}

module SUBM
{{
    io SPI{{{members}}}
    CHIP c
    func wire(SPI)
    {{
        SPI + c.SPI
    }}
}}

module main
{{
    io BUS{{{arg_members}}}
    SUBM s
    s.wire(BUS)
}}
"#,
        members = SUB_MEMBERS.join(", "),
        arg_members = arg_members.join(", "),
    )
}

struct Built {
    table: InstTable,
    errors: Vec<(u32, String)>,
    warnings: Vec<(u32, String)>,
}

fn build(tag: &str, source: &str) -> Built {
    let _lock = common::lock();
    common::reset();

    let uri = format!("/mcc/u127-{tag}.mc");
    mcc::mcc_load_from_string(&uri, source);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000)
        .unwrap_or_else(|e| panic!("flat build failed for {tag}: {e:?}"));

    let all = mcc::mcc_diagnose_all();
    let errors: Vec<(u32, String)> = all
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    let warnings: Vec<(u32, String)> = all
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Warning)
        .map(|d| (d.code, d.msg.clone()))
        .collect();

    Built {
        table,
        errors,
        warnings,
    }
}

/// Every path an entry is registered under that ends in `suffix` -- the
/// spelling-difference lens this lock exists to judge.
fn paths_ending_with<'a>(b: &'a Built, suffix: &str) -> Vec<&'a str> {
    b.table
        .iter()
        .map(|(_, e)| e.path.as_str())
        .filter(|p| p.ends_with(suffix))
        .collect()
}

/// One lane of the law. `arg_member` (the actual's spelling), `sub_member`
/// (the sub-module port's member) and `pin` (the container pin the
/// declaration maps that member to) are three different names for the same
/// conductor -- which is the point.
fn assert_lane(b: &Built, arg_member: &str, sub_member: &str, pin: &str) {
    let arg_path = format!("main.BUS.{arg_member}");
    let pin_path = format!("main.s.c.{pin}");
    let sub_path = format!("main.s.SPI.{sub_member}");

    // One pin, one identity: the entry lives at the full instance path (the
    // body statement's spelling), and the boundary's dropped-spelling orphan
    // (`main.s.<pin>`) does not exist.
    let pin_id = b
        .table
        .get_id_by_path(&pin_path)
        .unwrap_or_else(|| panic!("pin `{pin}` must be registered at `{pin_path}`"));
    let orphans = paths_ending_with(b, &format!("s.{pin}"));
    assert!(
        orphans.is_empty(),
        "the boundary's dropped-spelling orphan must not exist: {orphans:?}"
    );

    // The boundary segment: the actual's member sits on one segment, and that
    // segment carries the pin's own id (not a copy, not a minted orphan).
    let arg_id = b
        .table
        .get_id_by_path(&arg_path)
        .unwrap_or_else(|| panic!("`{arg_path}` must be a registered entry"));
    let arg_segs = b.table.nets_of(arg_id);
    assert_eq!(
        arg_segs.len(),
        1,
        "`{arg_path}` must sit on exactly one net segment: {arg_segs:?}"
    );
    let boundary_seg = b
        .table
        .get_net(arg_segs[0])
        .expect("segment id from nets_of must resolve");
    assert!(
        boundary_seg.points.contains(&pin_id),
        "the boundary segment must carry the pin's own id ({pin_path}): {:?}",
        boundary_seg
            .points
            .iter()
            .filter_map(|p| b.table.get_entry(*p).map(|e| e.path.clone()))
            .collect::<Vec<_>>()
    );

    // The body segment: the sub-module port member sits on one segment, and
    // it carries the SAME id -- the junction that makes the conductor
    // continuous across the boundary.
    let sub_id = b
        .table
        .get_id_by_path(&sub_path)
        .unwrap_or_else(|| panic!("`{sub_path}` must be a registered entry"));
    let sub_segs = b.table.nets_of(sub_id);
    assert_eq!(
        sub_segs.len(),
        1,
        "`{sub_path}` must sit on exactly one net segment: {sub_segs:?}"
    );
    let body_seg = b
        .table
        .get_net(sub_segs[0])
        .expect("segment id from nets_of must resolve");
    assert!(
        body_seg.points.contains(&pin_id),
        "the body segment must carry the same pin id ({pin_path}): {:?}",
        body_seg
            .points
            .iter()
            .filter_map(|p| b.table.get_entry(*p).map(|e| e.path.clone()))
            .collect::<Vec<_>>()
    );

    // The parent never names the peer's lane: no container-side entry is
    // spelled with the actual's member name.
    for foreign in ARG_MEMBERS {
        let offenders = paths_ending_with(b, &format!("s.{foreign}"));
        assert!(
            offenders.is_empty(),
            "a container-side entry is named by the actual's spelling ({foreign}): {offenders:?}"
        );
    }
}

/// `E3175` (`MODULE_PORT_NOT_FOUND`) must not fire on a boundary whose names do
/// not agree: the disease reported a naming difference as a missing port.
fn assert_no_3175(b: &Built, tag: &str) {
    for (code, msg) in b.errors.iter().chain(b.warnings.iter()) {
        assert_ne!(*code, 3175, "{tag}: E3175 fired at the boundary: {msg}");
    }
}

/// The disease's shape, on the pin-backed boundary: every actual member name
/// differs from the sub-module's, the pairing is still positional (written
/// order), and the boundary junction carries the container's pin identity.
#[test]
fn u127__mismatched_names_still_pair_positionally_on_pin_backed_boundary() {
    let b = build("mismatched", &board(&ARG_MEMBERS));

    assert!(
        b.errors.is_empty(),
        "a naming difference is not an error: {:?}",
        b.errors
    );
    for (i, arg_member) in ARG_MEMBERS.iter().enumerate() {
        assert_lane(&b, arg_member, SUB_MEMBERS[i], PINS[i]);
    }
    assert_no_3175(&b, "mismatched");
}

/// The control: matching names pair identically. Without it the test above
/// could pass on "the only boundary the engine handles is the agreeing one".
#[test]
fn u127__a_matching_name_boundary_pairs_the_same_way() {
    let b = build("matching", &board(&SUB_MEMBERS));

    assert!(b.errors.is_empty(), "unexpected error: {:?}", b.errors);
    for (i, _) in SUB_MEMBERS.iter().enumerate() {
        assert_lane(&b, SUB_MEMBERS[i], SUB_MEMBERS[i], PINS[i]);
    }
    assert_no_3175(&b, "matching");
}
