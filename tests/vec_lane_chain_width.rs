// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Lane-chain reduction width (unified-core §5 fixtures ② and ③).
//!
//! Both cases were covered only by the real-board hbl golden; these are the
//! synthetic minimal spellings. Their claim is a **reduction width** at Pass 1,
//! observed here end-to-end as the net partition: a dropped or truncated lane
//! shows up as a missing (or short-circuited) member.
//!
//! * **② bare (ghost) label lane chain** -- `dc{VDD_3V3, GND} -> sink{A, B}`
//!   with `dc` *undeclared*, the real-board `MIC.dc` / `dc{...}` shape
//!   (`periph.mc:39`, `hbl.mc:38`). The curly face must reduce to **width 2**
//!   and must not mirror: the hot member `VDD_3V3` stays on the sink's hot
//!   member (`A`) and `GND` on the return (`B`). A mirroring reduction would
//!   swap the two rails.
//! * **③ bracket-range bus lane chain** -- a bus declared over a pin range
//!   (`io [8:11] = SPI{SCLK, MOSI, CSN, MISO}`, `us513.mc:30`) used as a
//!   multi-lane series operand. The reduction must keep **width = N**, i.e.
//!   every lane including the middle ones (lanes 2..3, pins 9/10) must wire --
//!   the truncation the fixture guards against drops the middle lanes.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// A sink whose two members are a directional `[hot, ret]` pair.
const SINK2: &str =
    "component SINK2 {\n    pins = [\n        psnk [1,2] = [A, B]::DC(3.3V)\n    ]\n}\n";

/// A four-member bus declared over the pin range `[8:11]` -- the bracket bus.
const SPI4: &str =
    "component SPI4 {\n    pins = [\n        io [8:11] = SPI{SCLK, MOSI, CSN, MISO}\n    ]\n}\n";

/// The four-member counterpart the bracket bus is chained to.
const LOAD4: &str =
    "component LOAD4 {\n    pins = [\n        io [8:11] = SPI{SCLK, MOSI, CSN, MISO}\n    ]\n}\n";

/// Return every (entry path, net name) pair of the flat pass-2 netlist.
fn net_pairs(src: &str, uri: &str) -> Vec<(String, String)> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) =
        mcc::mcc_build_with_nets(&McIds::from("top"), &u).expect("build top");

    let mut pairs = Vec::new();
    if let Some(table) = net_store.get("top") {
        for (name, pts) in table.iter() {
            for p in pts {
                pairs.push((p.path.clone(), name.clone()));
            }
        }
    }
    pairs
}

/// Diagnostic codes emitted for `src` (sorted, deduped).
fn codes(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &u);
    let mut c: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    c.sort_unstable();
    c.dedup();
    c
}

/// Net-name of the single entry whose path ends with `suffix` (panics if not
/// exactly one -- a dropped/mis-expanded member fails loudly).
fn net_of(pairs: &[(String, String)], suffix: &str) -> String {
    let mut hits: Vec<&str> = pairs
        .iter()
        .filter(|(p, _)| p.ends_with(suffix))
        .map(|(_, n)| n.as_str())
        .collect();
    hits.sort_unstable();
    hits.dedup();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one entry ending {suffix:?}; got {hits:?} in {pairs:?}"
    );
    hits[0].to_string()
}

/// ② -- a bare (undeclared) curly label carrying a two-member DC face reduces
/// to **width 2** with no mirror: the hot member lands on the sink hot member
/// `A` (pin 1) and the return on `B` (pin 2). Both rails reach the sink.
#[test]
fn bare_label__ghost_curly_dc_pair_keeps_two_lanes_without_mirror() {
    let src = format!(
        "{SINK2}module top {{\n    SINK2 sink\n    dc{{VDD_3V3, GND}} -> sink{{A, B}}\n}}\n"
    );
    let c = codes(&src, "/mcc/lcw-bare-label.mc");
    assert!(
        !c.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH)
            && !c.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "the bare-label DC pair must be a legal 2-lane chain; got codes: {c:?}"
    );

    let pairs = net_pairs(&src, "/mcc/lcw-bare-label.mc");
    // No mirror: hot -> A, return -> B. A mirrored reduction would put
    // VDD_3V3 on B and GND on A.
    assert_eq!(
        net_of(&pairs, "sink.1"),
        "dc.VDD_3V3",
        "the hot member must stay hot (no mirror); pairs: {pairs:?}"
    );
    assert_eq!(
        net_of(&pairs, "sink.2"),
        "dc.GND",
        "the return member must stay the return (no mirror); pairs: {pairs:?}"
    );
    // Width 2: the two members are two distinct nets, neither dropped.
    assert_ne!(
        net_of(&pairs, "sink.1"),
        net_of(&pairs, "sink.2"),
        "the pair must reduce to two distinct lanes; pairs: {pairs:?}"
    );
}

/// ③ -- a bracket-range bus (`io [8:11]`) used as a four-lane series operand
/// keeps **width = N**: all four lanes wire, so the middle lanes 2..3 (pins 9
/// and 10) are present and distinct. A truncating reduction would drop them.
#[test]
fn bracket_bus__four_lane_series_keeps_middle_lanes() {
    let src = format!(
        "{SPI4}{LOAD4}module top {{\n    SPI4 driver\n    LOAD4 load\n    \
         driver.SPI{{SCLK, MOSI, CSN, MISO}} -> load.SPI{{SCLK, MOSI, CSN, MISO}}\n}}\n"
    );
    let c = codes(&src, "/mcc/lcw-bracket-bus.mc");
    assert!(
        !c.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH)
            && !c.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "the four-lane bracket bus must be a legal series; got codes: {c:?}"
    );

    let pairs = net_pairs(&src, "/mcc/lcw-bracket-bus.mc");
    let mut nets = Vec::new();
    for pin in ["8", "9", "10", "11"] {
        let l = net_of(&pairs, &format!("driver.{pin}"));
        let r = net_of(&pairs, &format!("load.{pin}"));
        assert_eq!(
            l, r,
            "bracket-bus pin {pin} must wire across the series; pairs: {pairs:?}"
        );
        nets.push(l);
    }
    // Width = N = 4: every lane is its own net (lanes 2..3 included).
    nets.sort();
    let before = nets.len();
    nets.dedup();
    assert_eq!(
        nets.len(),
        before,
        "all four lanes must be distinct (no truncation/gluing); pairs: {pairs:?}"
    );
}

/// ③ (middle-lane focus) -- selecting only the two middle members (`MOSI`,
/// `CSN` = pins 9 and 10) as a series keeps both of them: neither middle lane
/// is dropped.
#[test]
fn bracket_bus__middle_two_members_keep_their_lanes() {
    let src = format!(
        "{SPI4}{LOAD4}module top {{\n    SPI4 driver\n    LOAD4 load\n    \
         driver.SPI{{MOSI, CSN}} -> load.SPI{{MOSI, CSN}}\n}}\n"
    );
    let c = codes(&src, "/mcc/lcw-bracket-middle.mc");
    assert!(
        !c.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH)
            && !c.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "the middle-lane series must be legal; got codes: {c:?}"
    );

    let pairs = net_pairs(&src, "/mcc/lcw-bracket-middle.mc");
    let n9 = net_of(&pairs, "driver.9");
    let n10 = net_of(&pairs, "driver.10");
    assert_eq!(
        net_of(&pairs, "load.9"),
        n9,
        "middle lane 2 (MOSI, pin 9) must wire; pairs: {pairs:?}"
    );
    assert_eq!(
        net_of(&pairs, "load.10"),
        n10,
        "middle lane 3 (CSN, pin 10) must wire; pairs: {pairs:?}"
    );
    assert_ne!(
        n9, n10,
        "the two middle lanes must stay distinct; pairs: {pairs:?}"
    );
}
