// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U304 — the flat pass-2 projection resolves an inline CURLY_MN instance.
//!
//! The M1 ruling (CIMP §1 U300, b4006) legalized `FAMILY INST{COM | NO}` as a
//! named-ctor inline instance, but its phrase arm rewrote the statement into a
//! Ports of dotted bus spellings: the instance never entered the module's
//! component table, the faces fell into the bus-definition fallback, and the
//! flat projection silently resolved zero nets (CIMP §1 U304). The fix keeps
//! the FuncCall alive with the faces stashed on its interface buses, so the
//! named-ctor machinery materializes the instance and the face resolver
//! expands the selection exactly as it does for a declared instance. These
//! locks pin the flat face to parity with the declared-instance control arm.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const SRC_INLINE: &str = r#"
module top
{
    io PWR
    io SIG
    PWR - SWITCH.BUTTON SW1{COM | NO} -> SIG
}
"#;

const SRC_DECLARED: &str = r#"
module top
{
    io PWR
    io SIG
    SWITCH.BUTTON SW1
    PWR - SW1{COM | NO} -> SIG
}
"#;

const SRC_PLAIN: &str = r#"
module top
{
    io PWR
    io SIG
    PWR - RES(1k) R1 -> SIG
}
"#;

fn build_flat(src: &str) -> mcc::InstTable {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_init();
    let uri: McURI = "/mcc/u304-flat-curly-pins.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: McIds::from("top"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    table
}

/// (entry path, net name) pairs of the flat pass-2 netlist.
fn net_pairs(table: &mcc::InstTable) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for net in table.get_nets() {
        for &point_id in &net.points {
            let Some(entry) = table.get_entry(point_id) else {
                continue;
            };
            pairs.push((entry.path.clone(), net.name.clone()));
        }
    }
    pairs
}

/// (entry path, kind word) pairs — the structural shape of the flat table.
fn entry_kinds(table: &mcc::InstTable) -> Vec<(String, &'static str)> {
    let mut rows: Vec<(String, &'static str)> = table
        .iter()
        .map(|(_, e)| (e.path.clone(), e.kind.word()))
        .collect();
    rows.sort();
    rows
}

#[test]
fn u304__inline_curly_mn_flat_face_matches_declared_instance() {
    let table = build_flat(SRC_INLINE);

    // The instance materializes with its physical pins, not as a bus with
    // member-spelled labels (the old silent shape).
    let kinds = entry_kinds(&table);
    assert!(
        kinds.contains(&("top.SW1".to_string(), "component")),
        "SW1 must be a component in the flat table; got {kinds:?}"
    );
    assert!(
        kinds.contains(&("top.SW1.1".to_string(), "pin"))
            && kinds.contains(&("top.SW1.2".to_string(), "pin")),
        "SW1's COM/NO faces must register as physical pins; got {kinds:?}"
    );
    assert!(
        !kinds.iter().any(|(p, _)| p.contains("SW1.COM") || p.contains("SW1.NO")),
        "no member-spelled label residue may remain; got {kinds:?}"
    );

    // Both faces wire: PWR→COM (pin 1), NO (pin 2)→SIG — the flat face sees
    // the same two nets the tree connection surface always did.
    let pairs = net_pairs(&table);
    assert!(
        pairs.contains(&("top.SW1.1".to_string(), "PWR".to_string())),
        "COM (pin 1) must sit on net PWR; got {pairs:?}"
    );
    assert!(
        pairs.contains(&("top.SW1.2".to_string(), "SIG".to_string())),
        "NO (pin 2) must sit on net SIG; got {pairs:?}"
    );
}

#[test]
fn u304__inline_form_matches_declared_form_byte_for_byte() {
    // The declared-instance control arm (`SWITCH.BUTTON SW1` + `SW1{COM |
    // NO}`) is the canonical spelling the inline form must converge to: the
    // flat table kinds and the net membership come out identical (instance
    // name aside, the sources are the same modulo the declare row).
    let inline = build_flat(SRC_INLINE);
    let declared = build_flat(SRC_DECLARED);
    let inline_pairs = net_pairs(&inline);
    let declared_pairs = net_pairs(&declared);
    assert_eq!(
        inline_pairs, declared_pairs,
        "the inline and declared curly forms must project identical nets"
    );
}

#[test]
fn u304__plain_inline_ctor_flat_face_unchanged() {
    // Presence control: the plain named-ctor projection keeps its nets.
    let table = build_flat(SRC_PLAIN);
    let pairs = net_pairs(&table);
    assert!(
        pairs.contains(&("top.R1.1".to_string(), "PWR".to_string()))
            && pairs.contains(&("top.R1.2".to_string(), "SIG".to_string())),
        "the plain inline ctor control must keep both nets; got {pairs:?}"
    );
}
