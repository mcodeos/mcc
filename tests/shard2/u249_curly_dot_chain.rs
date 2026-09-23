// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP U249 locks, both halves.
//!
//! ① Curly dot-chain member — resolve-gate §2.13.6 ruling: `uC{ADC.P}` ≡
//!    `uC.ADC.P` ≡ `uC.ADC{P}` (R4 extension: a curly member may itself be a
//!    dot chain). The grammar wraps the chain as one nested `MCAST_IDS`
//!    member, so the member count stays single-member and the expansion is
//!    `uC.ADC.P` — never two members `uC.ADC` / `uC.P`. The reference reads
//!    through the same `validate_component_interface_ref` intercept as the
//!    established 3-segment spelling.
//!
//! ② Params-first declare — resolve-gate §2.9 B8 ruling (2026-09-23):
//!    `CAP(100nF, 10V) cap[1:2]` is canonical sugar for
//!    `cap[1:2]::CAP(100nF, 10V)` — a named array, materialized per member
//!    through the existing ::ctor path. A/B-locked against a use-only
//!    control, because a lone `cap[1:2].Cap(...)` without any declare
//!    materializes nothing (count 0).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Interface + component fixture: interface SPI4 with four members, FLASH
/// adopting it on physical pins `[1,2,5,6]` (member SCLK binds to physical
/// pin 2). Self-contained — no system library needed.
const SPI_FIXTURE: &str = r#"
interface SPI4(role)
{
    pins = [
        1 = CS
        2 = SCLK
        3 = MISO
        4 = MOSI
    ]
    role Slave { name = "Slave" }
}

component FLASH
{
    pins = [
        1 = _CS
        2 = SO
        5 = SI
        6 = SCLK
        [1,2,5,6] = SP::SPI4(Slave)
    ]
}

module main(psnk GND)
{
    FLASH f
    STMT
}
"#;

/// CAP fixture with a real twopin body, so `cap[N].Cap([a, b])` actually
/// wires pins — the same shape as the library CAP, self-contained.
const CAP_FIXTURE: &str = r#"
component CAP(cap::INT, volt::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([net1, net2])
    {
        net1 - this - net2
    }
}

module main
{
    STMT
}
"#;

fn src_of(fixture: &str, stmt: &str) -> String {
    fixture.replace("STMT", stmt)
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped.
fn nets_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut parts: Vec<Vec<String>> = net_store
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
    parts.sort();
    parts
}

/// Every diagnostic code emitted while building `src`, sorted, deduped.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

// ── ① curly dot-chain member ────────────────────────────────────────────────

/// The three spellings of resolve-gate §2.13.6 land the same partition: the
/// dotted chain, the interface-brace form, and the new curly chain member.
#[test]
fn u249_dot_chain__three_spellings_land_the_same_partition() {
    let dotted = nets_of(
        &src_of(SPI_FIXTURE, "f.SP.SCLK -> GND"),
        "/mcc/u249-chain-dotted.mc",
    );
    let iface_brace = nets_of(
        &src_of(SPI_FIXTURE, "f.SP{SCLK} -> GND"),
        "/mcc/u249-chain-brace.mc",
    );
    let curly_chain = nets_of(
        &src_of(SPI_FIXTURE, "f{SP.SCLK} -> GND"),
        "/mcc/u249-chain-curly.mc",
    );

    assert_eq!(
        curly_chain, dotted,
        "`f{{SP.SCLK}}` must land the same partition as `f.SP.SCLK`"
    );
    assert_eq!(
        curly_chain, iface_brace,
        "`f{{SP.SCLK}}` must land the same partition as `f.SP{{SCLK}}`"
    );
    // The chain resolved to the interface member: SCLK is interface pin 2 →
    // physical pin 2, paired with the GND port. A ghost `f.SP` or `f.SCLK`
    // bus here would mean the chain split into two members.
    assert_eq!(
        curly_chain.len(), 1,
        "one net; got {curly_chain:?}"
    );
    assert!(
        curly_chain[0].contains(&"f.2".to_string()) && curly_chain[0].contains(&"GND".to_string()),
        "the net carries f.2 (SCLK's physical pin) and GND; got {curly_chain:?}"
    );
}

/// A multi-member curly chain expands per member: `f{SP.SCLK, SP.MOSI}` is
/// the two-member face, identical to `f.SP{SCLK, MOSI}` — never a single
/// opaque `SP.SCLK,SP.MOSI` member. Both members must land (SCLK on physical
/// pin 2, MOSI on pin 5): a collapse to one member would drop one of them.
#[test]
fn u249_dot_chain__multi_member_chain_expands_per_member() {
    let curly_chain = nets_of(
        &src_of(SPI_FIXTURE, "f{SP.SCLK, SP.MOSI} -> [GND, GND]"),
        "/mcc/u249-chain-multi.mc",
    );
    let iface_brace = nets_of(
        &src_of(SPI_FIXTURE, "f.SP{SCLK, MOSI} -> [GND, GND]"),
        "/mcc/u249-multi-brace.mc",
    );

    assert_eq!(
        curly_chain, iface_brace,
        "the curly chain multi-member face must match the interface-brace face"
    );
    let points: Vec<&String> = curly_chain.iter().flatten().collect();
    // SCLK is interface pin 2 → physical 2; MOSI is interface pin 4 →
    // physical 6 (the [1,2,5,6] adoption list aligns by interface pin number).
    assert!(
        points.iter().any(|p| p.as_str() == "f.2") && points.iter().any(|p| p.as_str() == "f.6"),
        "both chain members resolved to their physical pins; nets={curly_chain:?}"
    );
}

/// Unknown chain member: the curly chain face matches the established
/// interface-brace face — the same E3179 (COMPONENT_PIN_NOT_FOUND), no
/// connection-parse fallout. (The bare dotted spelling reports through the
/// plain pin reader with a wider available-pin list — a pre-existing
/// curly-vs-dotted divergence, out of U249 scope.)
#[test]
fn u249_dot_chain__unknown_member_matches_iface_brace_face() {
    let curly_chain = codes_of(
        &src_of(SPI_FIXTURE, "f{SP.XX} -> GND"),
        "/mcc/u249-chain-unknown.mc",
    );
    let iface_brace = codes_of(
        &src_of(SPI_FIXTURE, "f.SP{XX} -> GND"),
        "/mcc/u249-unknown-brace.mc",
    );

    assert!(
        curly_chain.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "an unknown chain member reports E3179; codes: {curly_chain:?}"
    );
    assert_eq!(
        curly_chain, iface_brace,
        "the unknown-member face must match `f.SP{{XX}}`"
    );
}

// ── ② params-first declare (B8 canonical sugar) ─────────────────────────────

/// The sugar face ≡ the canonical ::ctor face, same fixture, same statement
/// slot: identical partition.
#[test]
fn u249_params_first__sugar_equals_canon_ctor_form() {
    let sugar = nets_of(
        &src_of(
            CAP_FIXTURE,
            "CAP(100, 10) cap[1:2].Cap([DC1, DC2])",
        ),
        "/mcc/u249-b8-sugar.mc",
    );
    let canon = nets_of(
        &src_of(
            CAP_FIXTURE,
            "cap[1:2]::CAP(100, 10).Cap([DC1, DC2])",
        ),
        "/mcc/u249-b8-canon.mc",
    );

    assert_eq!(
        sugar, canon,
        "`CAP(100, 10) cap[1:2].Cap(..)` must land the same partition as the ::ctor form"
    );
    assert_eq!(sugar.len(), 2, "one net per Cap arg; got {sugar:?}");
}

/// A/B: the params-first declare itself registers the named array. With the
/// declare, a separate later `cap[1:2].Cap(...)` use wires both members;
/// without it the same use materializes nothing (count 0) — a ghost array
/// never auto-creates.
#[test]
fn u249_params_first__bare_declare_registers_the_named_array() {
    let fixture_with_use = r#"
component CAP(cap::INT, volt::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([net1, net2])
    {
        net1 - this - net2
    }
}

module main
{
    CAP(100, 10) cap[1:2]
    cap[1:2].Cap([DC1, DC2])
}
"#;
    let with_declare = nets_of(fixture_with_use, "/mcc/u249-b8-declare.mc");
    let use_only = nets_of(
        &src_of(CAP_FIXTURE, "cap[1:2].Cap([DC1, DC2])"),
        "/mcc/u249-b8-use-only.mc",
    );

    assert_eq!(use_only, Vec::<Vec<String>>::new(), "use without declare: nothing");
    assert_eq!(
        with_declare.len(), 2,
        "with the declare the named array is live: two nets; got {with_declare:?}"
    );
    assert!(
        with_declare
            .iter()
            .flatten()
            .any(|p| p == "cap1.1")
            && with_declare.iter().flatten().any(|p| p == "cap2.1"),
        "both members materialized; got {with_declare:?}"
    );
}

/// Per-member materialization: `cap[1:3]` produces three distinct capacitors
/// (cap1/cap2/cap3) — the named-array range expands member-wise through the
/// ::ctor path (the B8 ruling's named-array reading, not one anonymous ctor
/// materialized once).
#[test]
fn u249_params_first__range_materializes_per_member() {
    let sugar = nets_of(
        &src_of(
            CAP_FIXTURE,
            "CAP(100, 10) cap[1:3].Cap([DC1, DC2, DC3])",
        ),
        "/mcc/u249-b8-range.mc",
    );

    assert_eq!(sugar.len(), 2, "the twopin Cap body makes two nets; got {sugar:?}");
    for member in ["cap1", "cap2", "cap3"] {
        assert!(
            sugar
                .iter()
                .flatten()
                .any(|p| p.starts_with(&format!("{member}."))),
            "{member} materialized and wired; nets={sugar:?}"
        );
    }
}

/// The func-body face: the sugar declare + use inside a func materializes
/// exactly like the module top (the deferred sub-instance path). Locked
/// because this face was the historical silent no-op the B8 ruling closed.
#[test]
fn u249_params_first__func_body_declare_and_use_materialize() {
    let func_body = r#"
component CAP(cap::INT, volt::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([net1, net2])
    {
        net1 - this - net2
    }
}

module main
{
    func addcaps()
    {
        CAP(100, 10) cap[1:2]
        cap[1:2].Cap([DC1, DC2])
    }
}
"#;
    let parts = nets_of(func_body, "/mcc/u249-b8-func.mc");
    assert_eq!(parts.len(), 2, "two nets; got {parts:?}");
    assert!(
        parts.iter().flatten().any(|p| p == "cap1.1")
            && parts.iter().flatten().any(|p| p == "cap2.1"),
        "the func-body declare materialized both members; got {parts:?}"
    );
}
