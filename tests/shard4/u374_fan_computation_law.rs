// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U374 fan computation law golden locks: the probe matrix P1-P8 (design doc
//! `doc/vector/fan-computation-law-design.md` v0.3, matrix in its §4) frozen
//! as regression guards. The law: the statement face has no legal fan -- every
//! unequal-width shape is rejected by Pass1 with E4007 (F3); the only legal
//! fan source is the dispatch-face per-member share, in exactly two shapes
//! (F1): a scalar actual shared across all lanes, and a net-name group that
//! every member re-obtains whole. Equal-width rows pair positionally and are
//! not a fan (F2). P9 (order_members member drift) has no source-level entry
//! today (U373 b4405 verdict: three upstream seals), so the lane gates
//! E4189/E4190 stay defense in depth locked by the U373 crate-internal
//! fixtures, not here.

use crate::common;

use mcc::{McIds, McURI};

const CAP_COMP: &str = "component CAP {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";
const RP_COMP: &str = "component RP {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1) {\n        n1 - this.1\n    }\n}\n";
const T_COMP: &str = "component T {\n    pins = [\n        1 = A\n        2 = K\n    ]\n}\n";

/// Build `main`, return the diagnostic codes (sorted).
fn codes(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v
}

/// Build `main` with nets, returning the frozen net-table store.
fn build_net_store(src: &str, uri: &str) -> Vec<(String, Vec<String>)> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(n, pts)| (n.clone(), pts.iter().map(|p| p.path.clone()).collect()))
                .collect()
        })
        .unwrap_or_default()
}

/// The net that holds `path`, if any.
fn net_holding<'a>(nets: &'a [(String, Vec<String>)], path: &str) -> Option<&'a Vec<String>> {
    nets.iter()
        .find(|(_, ps)| ps.iter().any(|p| p == path))
        .map(|(_, ps)| ps)
}

/// F3 (matrix ①) / P1 -- `c[1:2].1 -> VDD`: one column against a single
/// point. Pass1 rejects the broadcast with E4007; nothing reaches the lane
/// face.
#[test]
fn p1_stmt_single_point_broadcast_banned() {
    let src = format!("{T_COMP}module main {{\n    io VDD\n    T c[1:2]\n    c[1:2].1 -> VDD\n}}\n");
    let codes = codes(&src, "/mcc/u374-p1.mc");
    assert!(codes.contains(&4007), "E4007 expected; got {codes:?}");
}

/// F3 (matrix ②) / P2 -- `c[1:3].1 -> [VDD, GND]`: two unequal columns
/// (N-vs-M, both >= 2). Banned with E4007.
#[test]
fn p2_stmt_unequal_columns_banned() {
    let src = format!(
        "{T_COMP}module main {{\n    io VDD\n    io GND\n    T c[1:3]\n    c[1:3].1 -> [VDD, GND]\n}}\n"
    );
    let codes = codes(&src, "/mcc/u374-p2.mc");
    assert!(codes.contains(&4007), "E4007 expected; got {codes:?}");
}

/// F3 (matrix ③) / P5 -- `c[1:2].1 -> d[1:3].1`: unequal slice pair. Banned
/// with E4007.
#[test]
fn p5_stmt_unequal_slice_pair_banned() {
    let src = format!(
        "{T_COMP}module main {{\n    T c[1:2]\n    T d[1:3]\n    c[1:2].1 -> d[1:3].1\n}}\n"
    );
    let codes = codes(&src, "/mcc/u374-p5.mc");
    assert!(codes.contains(&4007), "E4007 expected; got {codes:?}");
}

/// F2 (matrix, P3) -- `c[1:2].1 -> [VDD, GND]`: equal columns are not a fan;
/// they pair per row: c1.1 joins VDD, c2.1 joins GND, and neither net sees
/// the other lane.
#[test]
fn p3_stmt_equal_columns_pair_per_row() {
    let src = format!(
        "{T_COMP}module main {{\n    io VDD\n    io GND\n    T c[1:2]\n    c[1:2].1 -> [VDD, GND]\n}}\n"
    );
    let nets = build_net_store(&src, "/mcc/u374-p3.mc");
    let up = net_holding(&nets, "c1.1").expect("net carrying c1.1");
    assert!(
        up.iter().any(|p| p == "VDD") && !up.iter().any(|p| p == "GND"),
        "c1.1 pairs with VDD only; got {up:?}"
    );
    let down = net_holding(&nets, "c2.1").expect("net carrying c2.1");
    assert!(
        down.iter().any(|p| p == "GND") && !down.iter().any(|p| p == "VDD"),
        "c2.1 pairs with GND only; got {down:?}"
    );
}

/// F2 (matrix, P3') -- `c[1:2].1 -> d[1:2].1`: equal slice pairs pair per row
/// with no cross-pairing.
#[test]
fn p3_stmt_equal_slice_pair_per_row() {
    let src = format!(
        "{T_COMP}module main {{\n    T c[1:2]\n    T d[1:2]\n    c[1:2].1 -> d[1:2].1\n}}\n"
    );
    let nets = build_net_store(&src, "/mcc/u374-p3s.mc");
    let first = net_holding(&nets, "c1.1").expect("net carrying c1.1");
    assert!(
        first.iter().any(|p| p == "d1.1") && !first.iter().any(|p| p.starts_with("d2.")),
        "c1.1 pairs with d1.1 only; got {first:?}"
    );
    let second = net_holding(&nets, "c2.1").expect("net carrying c2.1");
    assert!(
        second.iter().any(|p| p == "d2.1") && !second.iter().any(|p| p.starts_with("d1.")),
        "c2.1 pairs with d2.1 only; got {second:?}"
    );
}

/// F1 shape 1 (matrix, P4) -- `c[1:2].Pullup(VDD)`: the single scalar actual is
/// shared across all lanes, so every pulled pin lands on the VDD net.
#[test]
fn p4_dispatch_scalar_shared_fan() {
    let src = format!(
        "{RP_COMP}module main {{\n    io VDD\n    RP c[1:2]()\n    c[1:2].Pullup(VDD)\n}}\n"
    );
    let nets = build_net_store(&src, "/mcc/u374-p4.mc");
    let shared = net_holding(&nets, "c1.1").expect("net carrying c1.1");
    assert!(
        shared.iter().any(|p| p == "VDD") && shared.iter().any(|p| p == "c2.1"),
        "the scalar actual fans to every lane; got {shared:?}"
    );
}

/// F1 shape 2 (matrix, P2') -- `c[1:2].Cap([VDD, GND])`: the net-name group
/// is re-obtained whole by every member (not row-paired): VDD carries both
/// pin-1 leads, GND carries both pin-2 leads.
#[test]
fn p2_dispatch_group_share_two_members() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2]()\n    c[1:2].Cap([VDD, GND])\n}}\n"
    );
    let nets = build_net_store(&src, "/mcc/u374-p2p.mc");
    let up = net_holding(&nets, "c1.1").expect("net carrying c1.1");
    assert!(
        up.iter().any(|p| p == "VDD")
            && up.iter().any(|p| p == "c2.1")
            && !up.iter().any(|p| p.ends_with(".2")),
        "every member bridges the whole group: VDD side; got {up:?}"
    );
    let down = net_holding(&nets, "c1.2").expect("net carrying c1.2");
    assert!(
        down.iter().any(|p| p == "GND")
            && down.iter().any(|p| p == "c2.2")
            && !down.iter().any(|p| p == "VDD"),
        "every member bridges the whole group: GND side; got {down:?}"
    );
}

/// F1 shape 2, three lanes (matrix, P8) -- `c[1:3].Cap([VDD, GND])`: the
/// count coincidence does not trigger row pairing; all three caps bridge the
/// whole group.
#[test]
fn p8_dispatch_group_share_three_members() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:3]()\n    c[1:3].Cap([VDD, GND])\n}}\n"
    );
    let nets = build_net_store(&src, "/mcc/u374-p8.mc");
    let up = net_holding(&nets, "c1.1").expect("net carrying c1.1");
    assert!(
        up.iter().any(|p| p == "VDD")
            && up.iter().any(|p| p == "c2.1")
            && up.iter().any(|p| p == "c3.1"),
        "all three lanes share the VDD side; got {up:?}"
    );
    let down = net_holding(&nets, "c3.2").expect("net carrying c3.2");
    assert!(
        down.iter().any(|p| p == "GND")
            && down.iter().any(|p| p == "c1.2")
            && down.iter().any(|p| p == "c2.2"),
        "all three lanes share the GND side; got {down:?}"
    );
}

/// F3 (matrix ⑤) / P6 -- `c[1:2].Cap(VDD)`: a scalar actual against a vector
/// formal is banned with E4180 on the dispatch face.
#[test]
fn p6_dispatch_scalar_to_vector_formal_banned() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    CAP c[1:2]()\n    c[1:2].Cap(VDD)\n}}\n"
    );
    let codes = codes(&src, "/mcc/u374-p6.mc");
    assert!(codes.contains(&4180), "E4180 expected; got {codes:?}");
}

/// F3 (matrix ④) / P7 -- `a[1:2].Cap(b[1:3].1)`: a slice actual narrower than
/// its peers is banned with E4181 on the dispatch face.
#[test]
fn p7_dispatch_slice_width_mismatch_banned() {
    let src = format!(
        "{CAP_COMP}{T_COMP}module main {{\n    T a[1:2]\n    T b[1:3]\n    a[1:2].Cap(b[1:3].1)\n}}\n"
    );
    let codes = codes(&src, "/mcc/u374-p7.mc");
    assert!(codes.contains(&4181), "E4181 expected; got {codes:?}");
}
