// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! R3 domain-level `@bridge` — the license, its member take, and its four
//! static codes (intent-reference-layer-design.md §10.4, U79 ① item 3).
//!
//! A statement carrying `@bridge(domA, domB)` with both arguments
//! whole-referenceable domains (exactly one `::DC` rail) is *licensed*: each
//! domain word on its chain resolves to the single directed rail member its
//! own arrow direction names (`->` takes the hot member, `<-` the return
//! member) instead of the whole `[hot, ret]` pair, and a one-lane licensed
//! operand facing a wider one stretches to the member column. The four codes:
//!
//! * 6046 `DOMAIN_NET_MIXED_BRIDGE` — one argument names a domain, the other
//!   a plain endpoint: no single crossing. Reported, and the statement keeps
//!   today's (unlicensed, net-level) reading.
//! * 6047 `DOMAIN_BRIDGE_DIRECTION_REVERSED` — argument order disagrees with
//!   the written left-to-right order of the domain words. Reported *and* the
//!   statement stays licensed: member resolution is per word, so re-reading
//!   the chain from the other end would be the silent rewrite this layer
//!   forbids.
//! * 6048 `DOMAIN_BRIDGE_LEG_INCONSISTENT` — the chain's two end words land
//!   on opposite sides of the pair (a hot leg carrying a return-copper end).
//! * 6049 `DOMAIN_BRIDGE_DANGLING` — the named pair is witnessed by no leg
//!   anywhere in the module.
//!
//! The member take is asserted through the real net partition
//! (`export::netlist::collect_nets`), not through the absence of codes — a
//! silent mis-wire (the pre-R3 defect: return-leg members landing on the
//! hot plane) also reads as zero codes.

use crate::common;

use mcc::export::netlist::{collect_nets, PointNaming};
use mcc::{McIds, McURI};
use std::collections::BTreeMap;

/// A two-pin ferrite-like device, declared in-file so the tests don't depend
/// on the installed mcode library.
const FB: &str = "component FB {\n    pins = [\n        io [1,2] = [X, Y]\n    ]\n}\n";

/// One hot domain + one return-leg domain, the R3 anchor pair — declared in
/// the module body, where the whole-referenceable pair is scoped (the probe
/// boards keep them there too).
const DOMAINS: &str =
    "    domain DVDD { rail [V3, G]::DC(3.3V) }\n    domain AVDD { rail [VA, GA]::DC(3.3V) }\n";

fn load(uri: &str, src: &str) {
    common::reset();
    let owned: McURI = uri.to_string();
    mcc::mcc_load_from_string(&owned, src);
}

/// Every diagnostic code (sorted, deduped).
fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    load("/mcc/u79-r3.mc", src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &McURI::from("/mcc/u79-r3.mc"), 1000)
        .expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

/// The flat net partition as `net → sorted pad list` (local naming).
fn net_partition(src: &str) -> BTreeMap<String, Vec<String>> {
    let _lock = common::lock();
    load("/mcc/u79-r3.mc", src);
    let mut dl = mcc::mcc_build_dianlu(&McIds::from("main"), &McURI::from("/mcc/u79-r3.mc"), 0)
        .expect("dianlu build");
    let _ = dl.flatten_with_prefix(None);
    let arena = dl.arena().clone();
    let store = dl.store().clone();
    let (tree, table) = dl.into_parts();
    let net_store = table.net_table();
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_nets(
        &tree,
        &arena,
        &store,
        &net_store.borrow(),
        PointNaming::Local,
        &mut nets,
    );
    nets.into_iter()
        .map(|(n, mut pts)| {
            pts.sort();
            (n, pts)
        })
        .collect()
}

/// `component FB` + `module main` whose body is `DOMAINS` followed by `stmt`.
fn board(stmt: &str) -> String {
    format!("{FB}module main {{\n{DOMAINS}{stmt}\n}}\n")
}

/// The hot-leg anchor (srcv3 main.mc :58 form): a licensed `->` chain takes
/// the hot members — zero codes, and the pads land on V3/VA, not on the pair.
#[test]
fn u79_r3_hot_leg_member_take() {
    let src = board("    DVDD -> fb1::FB() -> AVDD @bridge(DVDD, AVDD)");
    let codes = build_codes(&src);
    let family = [
        mcc::errcodes::DOMAIN_NET_MIXED_BRIDGE,
        mcc::errcodes::DOMAIN_BRIDGE_DIRECTION_REVERSED,
        mcc::errcodes::DOMAIN_BRIDGE_LEG_INCONSISTENT,
        mcc::errcodes::DOMAIN_BRIDGE_DANGLING,
        mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH,
        mcc::errcodes::CONN_LEFT_ARROW_SHAPE_MISMATCH,
    ];
    for code in family {
        assert!(
            !codes.contains(&code),
            "licensed hot leg must be clean of code {code}; got: {codes:?}"
        );
    }
    let nets = net_partition(&src);
    assert_eq!(
        nets.get("V3").map(|v| v.as_slice()),
        Some(vec!["V3".to_string(), "fb1.1".to_string()].as_slice()),
        "hot member take must wire fb1.1 to V3; got: {nets:?}"
    );
    assert_eq!(
        nets.get("VA").map(|v| v.as_slice()),
        Some(vec!["VA".to_string(), "fb1.2".to_string()].as_slice()),
        "hot member take must wire fb1.2 to VA; got: {nets:?}"
    );
}

/// The return-leg anchor (srcv3 main.mc :59 form): a licensed `<-` chain with
/// a list stretches the single licensed operand to the member column and
/// takes the RETURN members — the pads land on G/GA, not on the hot planes.
/// Pre-R3 this statement silently mis-wired FB2/FB3 onto the hot side (and
/// fired the 6022 plane-link audit against the wrong copper).
#[test]
fn u79_r3_return_leg_member_take() {
    let src = board("    DVDD <- [fb2::FB(), fb3::FB()] <- AVDD @bridge(DVDD, AVDD)");
    let codes = build_codes(&src);
    for code in [
        mcc::errcodes::CONN_LEFT_ARROW_SHAPE_MISMATCH,
        mcc::errcodes::DOMAIN_BRIDGE_LEG_INCONSISTENT,
        mcc::errcodes::DOMAIN_BRIDGE_DANGLING,
    ] {
        assert!(
            !codes.contains(&code),
            "licensed return leg must be clean of code {code}; got: {codes:?}"
        );
    }
    let nets = net_partition(&src);
    assert_eq!(
        nets.get("G").map(|v| v.as_slice()),
        Some(vec!["G".to_string(), "fb2.1".to_string(), "fb3.1".to_string()].as_slice()),
        "return member take must wire fb2.1/fb3.1 to G; got: {nets:?}"
    );
    assert_eq!(
        nets.get("GA").map(|v| v.as_slice()),
        Some(vec!["GA".to_string(), "fb2.2".to_string(), "fb3.2".to_string()].as_slice()),
        "return member take must wire fb2.2/fb3.2 to GA; got: {nets:?}"
    );
}

/// Same chain shape without `@bridge` stays unlicensed and shape-checked
/// (the whole-pair reading cannot fit a single two-pin device).
#[test]
fn u79_r3_unlicensed_control_stays_shape_checked() {
    let src = board("    DVDD -> fb1::FB() -> AVDD");
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "the unlicensed whole-pair chain is shape-rejected; got: {codes:?}"
    );
}

/// 6046: one domain argument + one plain endpoint names no single crossing —
/// reported, and the statement keeps today's unlicensed reading (the shape
/// defect of the raw pair is not suppressed by the failed license).
#[test]
fn u79_r3_mixed_bridge_reported_and_keeps_reading() {
    let src = board("    DVDD -> fb1::FB() -> G @bridge(DVDD, G)");
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::DOMAIN_NET_MIXED_BRIDGE),
        "a mixed domain/endpoint @bridge is 6046; got: {codes:?}"
    );
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "a mixed @bridge must not license the chain; got: {codes:?}"
    );
}

/// 6047: reversed argument order is reported *and* the statement stays
/// licensed — the wiring equals the correctly-ordered control exactly.
#[test]
fn u79_r3_reversed_args_reported_and_stay_licensed() {
    let reversed = board("    DVDD <- fb1::FB() <- AVDD @bridge(AVDD, DVDD)");
    let control = board("    DVDD <- fb1::FB() <- AVDD @bridge(DVDD, AVDD)");
    let codes = build_codes(&reversed);
    assert!(
        codes.contains(&mcc::errcodes::DOMAIN_BRIDGE_DIRECTION_REVERSED),
        "reversed @bridge argument order is 6047; got: {codes:?}"
    );
    for code in [
        mcc::errcodes::DOMAIN_BRIDGE_LEG_INCONSISTENT,
        mcc::errcodes::DOMAIN_BRIDGE_DANGLING,
        mcc::errcodes::CONN_LEFT_ARROW_SHAPE_MISMATCH,
    ] {
        assert!(
            !codes.contains(&code),
            "a reversed but licensed leg stays clean of code {code}; got: {codes:?}"
        );
    }
    assert_eq!(
        net_partition(&reversed),
        net_partition(&control),
        "reversed arguments must wire exactly like the ordered control"
    );
}

/// 6048: the chain's ends land on opposite sides of the pair — a hot domain
/// word against a return-copper member name.
#[test]
fn u79_r3_leg_inconsistent_reported() {
    let src = board("    DVDD -> fb1::FB() <- GA @bridge(DVDD, AVDD)");
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::DOMAIN_BRIDGE_LEG_INCONSISTENT),
        "ends on opposite sides of the pair are 6048; got: {codes:?}"
    );
}

/// 6049: the pair is named but no chain in the module witnesses a leg —
/// the one-leg chain here ends on a non-member word, so the declared
/// crossing is realized by nothing.
#[test]
fn u79_r3_dangling_bridge_reported() {
    let src = board("    DVDD -> fb1::FB() @bridge(DVDD, AVDD)");
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::DOMAIN_BRIDGE_DANGLING),
        "a named pair with zero witnessed legs is 6049; got: {codes:?}"
    );
}

/// Two legs of one pair — one hot, one return — witness both sides, so the
/// pair is not dangling. (The parallel-leg loop itself is pre-existing
/// PWR-2/6007 territory and is not re-judged here.)
#[test]
fn u79_r3_both_legs_witness_no_dangling() {
    let src = board(
        "    DVDD -> fb1::FB() -> AVDD @bridge(DVDD, AVDD)\n    \
         DVDD <- [fb2::FB(), fb3::FB()] <- AVDD @bridge(DVDD, AVDD)",
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::DOMAIN_BRIDGE_DANGLING),
        "two witnessed legs keep the pair non-dangling; got: {codes:?}"
    );
}
