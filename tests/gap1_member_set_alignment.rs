// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §11.4 / vector-pipeline Phase 2.1: GAP1 member-set alignment + the
//! positional zip of a vector-slice arg lane against the iterated receiver.
//!
//! `cap[1:2].Cap([XTAL.X[1:2], gnd])` — receiver {cap1,cap2} vs slice
//! {XTAL.X1,XTAL.X2}: one-to-one positional correspondence at equal width is
//! legal and zips c1↔XTAL.X1, c2↔XTAL.X2; scalar lanes (`gnd`) broadcast
//! (§5 item 17). A width mismatch (slice `res[3:5]` = 3 vs receiver 2) is
//! reported once as VECTOR_ZIP_WIDTH_MISMATCH (4181), with the zip clamped so
//! the downstream E4180 (arg-list lane count vs formals) stays quiet — GAP1 is
//! the single precise report.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";
const RES_COMP: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Build `main`, return the diagnostic codes (sorted).
fn codes(src: &str, uri: &str) -> Vec<u32> {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v
}

fn has_code(codes: &[u32], code: u32) -> bool {
    codes.binary_search(&code).is_ok()
}

fn codes_without_benign(codes: &[u32]) -> Vec<u32> {
    // 5641/5642/5054 are unrelated warnings from the probe's short names.
    codes
        .iter()
        .copied()
        .filter(|c| !matches!(c, 5641 | 5642 | 5054))
        .collect()
}

/// Build `main` and return the module instance.
fn build_main(src: &str, uri: &str) -> mcc::McModuleInst {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    mcc::mcc_build(&McIds::from("main"), &u).expect("build")
}

/// ── §11.4 legal case: equal-width slice zips, stays quiet ────────────────
/// `c[1:2].Cap([res[3:4], gnd])` — receiver {c1,c2} (2) vs slice {res3,res4}
/// (2): one-to-one correspondence. No 4181, no E4180, no E4007; the nets show
/// the positional zip c1.1↔res3 / c2.1↔res4 and the scalar broadcast
/// gnd↔c1.2 / gnd↔c2.2.
#[test]
fn aligned_vector_slice_arg_zips_and_is_quiet() {
    let src = format!(
        "{CAP_COMP}{RES_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n    res[3:4]::RES(0)\n    c[1:2].Cap([res[3:4], gnd])\n}}\n"
    );
    let codes = codes(&src, "/mcc/gap1-aligned.mc");
    let codes = codes_without_benign(&codes);
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "aligned slice is quiet; got {codes:?}"
    );

    let inst = build_main(&src, "/mcc/gap1-aligned.mc");
    // Zip: the res3 net carries c1.1, the res4 net carries c2.1 — no
    // cross-pairing, no broadcast of both slice members onto both caps.
    let net_paths = |base: &str| {
        let mut out: Vec<String> = inst
            .nets
            .iter()
            .filter(|(n, _)| n == base)
            .flat_map(|(_, pts)| pts.iter().map(|p| p.path.clone()))
            .collect();
        out.sort();
        out
    };
    let res3 = net_paths("res3");
    assert!(
        res3.iter().any(|p| p == "c1.1"),
        "c1.1 on net res3; got {res3:?}"
    );
    assert!(
        !res3.iter().any(|p| p == "c2.1"),
        "c2.1 NOT on net res3; got {res3:?}"
    );
    let res4 = net_paths("res4");
    assert!(
        res4.iter().any(|p| p == "c2.1"),
        "c2.1 on net res4; got {res4:?}"
    );
    assert!(
        !res4.iter().any(|p| p == "c1.1"),
        "c1.1 NOT on net res4; got {res4:?}"
    );
    // The scalar lane gnd broadcasts to both caps' pin 2.
    let gnd = net_paths("gnd@7");
    assert!(
        gnd.iter().any(|p| p == "c1.2") && gnd.iter().any(|p| p == "c2.2"),
        "gnd net holds both caps' pin 2; got {gnd:?}"
    );
}

/// ── §11.4 mismatch: receiver 2 vs slice 3 reports GAP1 once ──────────────
/// `c[1:2].Cap([res[3:5], gnd])` — {c1,c2} vs {res3,res4,res5}: 4181 fires
/// exactly once; the zip clamps to the receiver width so E4180/E4007 stay
/// silent (GAP1 is the single report).
#[test]
fn mismatched_vector_slice_arg_reports_gap1_once() {
    let src = format!(
        "{CAP_COMP}{RES_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n    res[3:5]::RES(0)\n    c[1:2].Cap([res[3:5], gnd])\n}}\n"
    );
    let codes = codes(&src, "/mcc/gap1-mismatch.mc");
    let codes = codes_without_benign(&codes);
    let count = codes.iter().filter(|c| **c == 4181).count();
    assert_eq!(count, 1, "GAP1 fires exactly once; got {codes:?}");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| *c != 4181).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "no E4180/E4007 noise alongside GAP1; got {codes:?}"
    );
}

/// ── §11.4: receiver 3 vs slice 2 (narrower slice) also reports GAP1 ──────
/// The clamp repeats the slice's last member for the overflow; GAP1 still
/// flags the width mismatch.
#[test]
fn narrower_slice_reports_gap1() {
    let src = format!(
        "{CAP_COMP}{RES_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:3](1)\n    res[3:4]::RES(0)\n    c[1:3].Cap([res[3:4], gnd])\n}}\n"
    );
    let codes = codes(&src, "/mcc/gap1-narrower.mc");
    let codes = codes_without_benign(&codes);
    let count = codes.iter().filter(|c| **c == 4181).count();
    assert_eq!(count, 1, "GAP1 fires for narrower slice; got {codes:?}");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| *c != 4181).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "no E4180/E4007 noise; got {codes:?}"
    );
}

/// ── §11.4: scalar lanes broadcast, no GAP1 ───────────────────────────────
/// `c[1:2].Cap([VDD, GND])` — both lanes scalar; every member gets the full
/// arg list (§5 item 17). No slice → no member-set comparison.
#[test]
fn scalar_lanes_broadcast_no_gap1() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n    c[1:2].Cap([VDD, GND])\n}}\n"
    );
    let codes = codes(&src, "/mcc/gap1-scalar.mc");
    let codes = codes_without_benign(&codes);
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "scalar broadcast stays quiet; got {codes:?}"
    );

    // Both caps' pin 1 on the VDD net (broadcast, not zip).
    let inst = build_main(&src, "/mcc/gap1-scalar.mc");
    let vdd_net = inst
        .nets
        .iter()
        .find(|(n, _)| n.starts_with("VDD"))
        .unwrap_or_else(|| panic!("no VDD net; nets={:?}", inst.nets));
    let paths: Vec<&String> = vdd_net.1.iter().map(|p| &p.path).collect();
    assert!(
        paths.iter().any(|p| p.contains("c1.1")),
        "c1.1 on VDD; got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.contains("c2.1")),
        "c2.1 on VDD; got {paths:?}"
    );
}

/// ── §11.3 ③ (b): the slice member set is structural, not re-parsed ──────
/// GAP1's within-lane comparison reads the member set from `McIds::expand` on
/// the structured id — the same mechanism serves a bus/interface member slice
/// (`XTAL.X[1:2]` → {XTAL.X1, XTAL.X2}) and a declared-vector slice. Lock the
/// member-set producer here so the check layer never string-re-parses.
#[test]
fn slice_member_set_comes_from_expand() {
    for (display, expected) in [
        ("res[3:4]", vec!["res3", "res4"]),
        ("XTAL.X[1:2]", vec!["XTAL.X1", "XTAL.X2"]),
        ("h.XTAL.X[1:2]", vec!["h.XTAL.X1", "h.XTAL.X2"]),
    ] {
        let members = McIds::from(display).expand();
        assert_eq!(members, expected, "expand({display})");
    }
}
