// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U356: a whole-foot connection on a single-member interface adoption
//! (`a.SD -> b.SH` where `SD::SPN(DEV)` adopts exactly one pin) resolves to
//! the adopted pin's physical id. Before the fix both gates on the
//! whole-foot expansion chain required `>= 2` members — `find_bus_port_pin_ids`
//! by contract, `declared_pin_id` by its `Single`-only arm — so both
//! endpoints fell to ghost points spelled `a.SD`/`b.SH`: zero produced net
//! between the physical pins, both pins E4119 unconnected, no diagnostic
//! naming the cause.
//!
//! The fix is the identity-resolver arm (user ruling A): `declared_pin_id`
//! reads a single-member interface adoption's unique registered pin, so
//! every consumer of the resolver (pin-path normalization, curly access,
//! ncpin, declaration face) lands on the same physical pin the member
//! spelling (`SD.SB`) already resolved to. The `>= 2` contract of
//! `find_bus_port_pin_ids` is untouched — its eight callers keep their lane
//! semantics.
//!
//! The residue that still resolves to nothing gets a voice: E4217 fires on
//! the whole-foot reference whose port names a real multi-pin-style port
//! but expands to no unique pin — the single-member interface reused
//! across pin groups, where the whole-foot name genuinely names more than
//! one pad. Every verdict branch carries a member: the healthy connect
//! (quiet, and netlist-verified onto the physical pins), the member-spelling
//! control, the multi-member whole-foot control, and the E4217 ambiguity.

#![allow(non_snake_case)]

use crate::common;

use std::process::Command;

use mcc::{McIds, McURI};

/// A single-member interface with a role table — the U351 fixture's
/// minimization (the fixture that first exposed the silent face).
const SPN: &str = r#"
interface SPN(role)
{
    topology = "point to point"
    pins = [
        1 = SB
    ]
    role DEV {
        peer = HOST(1:2)
    }
    role HOST {
        peer = DEV(1)
    }
}

interface DUP(role)
{
    topology = "point to point"
    pins = [
        1 = P
        2 = N
    ]
    role DEV {
        peer = HOST(1)
    }
    role HOST {
        peer = DEV(1)
    }
}

component SDEV
{
    pins = [
        1 = SD::SPN(DEV)
    ]
}

component SHOST
{
    pins = [
        1 = SH::SPN(HOST)
    ]
}

component DDEV
{
    pins = [
        [1,2] = DP::DUP(DEV)
    ]
}

component DHOST
{
    pins = [
        [1,2] = DH::DUP(HOST)
    ]
}
"#;

/// The ambiguous twin: the single-member interface adopted on TWO pins of
/// one component (two rows) — the whole-foot name carries two pads, no
/// unique pin behind it.
const AMBIG: &str = r#"
interface ONE(role)
{
    topology = "point to point"
    pins = [
        1 = SB
    ]
    role DEV {
        peer = HOST(1:2)
    }
    role HOST {
        peer = DEV(1)
    }
}

component PAD2
{
    pins = [
        1 = PA::ONE(DEV)
        2 = PA::ONE(DEV)
    ]
}

component HOST1
{
    pins = [
        1 = PB::ONE(HOST)
    ]
}
"#;

/// Build `main` with the body statements and return the sorted diagnostic
/// codes.
fn build_with(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/u356-whole-foot.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(code: u32, src: &str) -> usize {
    build_with(src).iter().filter(|&&c| c == code).count()
}

/// The defect face: a whole-foot single-member connect must land both
/// adopted pins on the produced net — no E4119 unconnected-pin residue, no
/// E4217, and the whole-foot statement itself drops no error. Before the
/// fix both pins reported E4119 and nothing connected.
#[test]
fn u356__whole_foot_single_member_connects_both_pins() {
    let src = format!(
        "{SPN}module main {{\n    SDEV a\n    SHOST b\n    a.SD -> b.SH\n}}\n"
    );
    let codes = build_with(&src);
    assert_eq!(
        count(mcc::errcodes::NET_PIN_UNWIRED, &src),
        0,
        "both adopted pins must land on the produced net; got {codes:?}"
    );
    assert_eq!(
        count(mcc::errcodes::WHOLE_FOOT_PIN_UNRESOLVED, &src),
        0,
        "a resolvable single-member whole-foot reference stays quiet; got {codes:?}"
    );
    assert_eq!(
        count(mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH, &src),
        0,
        "the whole-foot statement must not degrade to a shape error; got {codes:?}"
    );
}

/// The netlist half of the same face, through the CLI: the net carries the
/// two physical pins (`a.1 b.1`), the identity the member spelling
/// resolves to. Before the fix the produced net was empty between the pins.
#[test]
fn u356__whole_foot_netlist_carries_physical_pins() {
    let dir = std::env::temp_dir().join(format!("u356-net-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let mc = dir.join("disc.mc");
    std::fs::write(
        &mc,
        format!("{SPN}module main {{\n    SDEV a\n    SHOST b\n    a.SD -> b.SH\n}}\n"),
    )
    .expect("write fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args(["export", "netlist", mc.to_str().expect("utf8")])
        .output()
        .expect("run mcc");
    let _ = std::fs::remove_dir_all(&dir);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("a.1 b.1"),
        "the whole-foot net must carry both physical pins; output: {text}"
    );
    assert!(
        !text.contains("E4119"),
        "no adopted pin stays unconnected; output: {text}"
    );
}

/// Control: the member spelling of the same adoption was never broken —
/// it must stay quiet (guards the fix against widening into the spelling
/// faces).
#[test]
fn u356__member_spelling_control_stays_quiet() {
    let src = format!(
        "{SPN}module main {{\n    SDEV a\n    SHOST b\n    a.SD.SB -> b.SH.SB\n}}\n"
    );
    let codes = build_with(&src);
    assert_eq!(
        count(mcc::errcodes::NET_PIN_UNWIRED, &src),
        0,
        "the member spelling keeps connecting; got {codes:?}"
    );
}

/// Control: the multi-member whole-foot (`c.DP -> d.DH`) rides the
/// untouched `>= 2` lane expansion — quiet, and exactly two nets' worth of
/// pins connected (no E4119 on any of the four pads).
#[test]
fn u356__multi_member_whole_foot_control_stays_quiet() {
    let src = format!(
        "{SPN}module main {{\n    DDEV c\n    DHOST d\n    c.DP -> d.DH\n}}\n"
    );
    let codes = build_with(&src);
    assert_eq!(
        count(mcc::errcodes::NET_PIN_UNWIRED, &src),
        0,
        "the multi-member lane expansion keeps connecting all four pads; got {codes:?}"
    );
    assert_eq!(
        count(mcc::errcodes::WHOLE_FOOT_PIN_UNRESOLVED, &src),
        0,
        "a resolvable multi-member whole-foot reference stays quiet; got {codes:?}"
    );
}

/// The residue gets a voice: the single-member interface adopted on TWO
/// pins leaves no unique pin behind the whole-foot name — exactly one
/// E4217 on the reference, and the ambiguous pins keep their E4119 (the
/// diagnostic names the cause; it does not invent a connection).
#[test]
fn u356__ambiguous_whole_foot_reuse_reports_4217() {
    let src = format!(
        "{AMBIG}module main {{\n    PAD2 p2\n    HOST1 h1\n    p2.PA -> h1.PB\n}}\n"
    );
    let codes = build_with(&src);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::WHOLE_FOOT_PIN_UNRESOLVED)
            .count(),
        1,
        "the ambiguous whole-foot reference reports E4217 once; got {codes:?}"
    );
}

/// The peer half of the ambiguity face: the unambiguous host side resolves
/// (`h1.PB` has one pin), so the E4217 anchor is the ambiguous reference
/// only — the connect still lands `h1.1` on the produced net, and no other
/// code family takes over the face.
#[test]
fn u356__ambiguous_whole_foot_peer_side_still_resolves() {
    let src = format!(
        "{AMBIG}module main {{\n    PAD2 p2\n    HOST1 h1\n    p2.PA -> h1.PB\n    p2.1 -> h1.PB.SB\n}}\n"
    );
    let codes = build_with(&src);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::WHOLE_FOOT_PIN_UNRESOLVED)
            .count(),
        1,
        "only the ambiguous side reports; got {codes:?}"
    );
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::NET_PIN_UNWIRED)
            .count(),
        1,
        "only the still-unwired second pad stays E4119 — the peer half resolves; got {codes:?}"
    );
}
