// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Topology oracle for vector series statements (vec-dianlu §5.2 / §5.3).
//!
//! Locks the **point-level net topology** of whole-array / member-column /
//! dotted-list series forms against `[VDD, GND]`, so a silent wrong-wiring
//! regression (the old §5.3.1 single-point broadcast collapse) is caught as a
//! hard failure instead of passing as "zero diagnostics + collapsed net":
//!
//! - Legal equal-row zip (`c[1:2] -> [VDD, GND]`): **two independent 2-point
//!   nets** `{c1.2, VDD}` and `{c2.2, GND}` — never one merged/collapsed net.
//!   The member-col `.2`, dotted `[c1.2, c2.2]`, and DC-bus `PWR{VCC, GND}`
//!   forms must produce the identical row alignment.
//! - Illegal single-point vs row (`c[1:2] -> GND`, `VDD -> c[1:2]`): E4007 and
//!   **zero** c-member points appear on any net (no partial / truncated net).
//! - Group `(,)` fan (`(c1.2, c2.2) -> GND`): one 3-point net (multi-terminal),
//!   the §7.3 legal fan-in; a mismatched group errors with E4007, zero nets.
//!
//! Assertions are **membership-based** (point path in the net), never net-count
//! or label-based, so port-ordinal aliasing (e.g. `GND@11`) cannot fake a pass.

use mcc::{McIds, McURI};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// D8-style CAP: member io bus `io [1,2] = NODE{P, N}` (two-pin, pin names
/// `.1`/`.2`), constructed via the `::CAP()` array form.
const CAP_BUS: &str = "component CAP {\n    pins = [\n        io [1,2] = NODE{P, N}\n    ]\n}\n";

/// Plain two-pin CAP (`1 = 1`, `2 = 2`), constructed via `::CAP()`.
const CAP_PLAIN: &str = "component CAP {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Build `main` and return (diagnostic codes sorted, net-store point lists).
fn build(src: &str, uri: &str) -> (Vec<u32>, Vec<(String, Vec<String>)>) {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    let nets = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(n, pts)| (n.clone(), pts.iter().map(|p| p.path.clone()).collect()))
                .collect()
        })
        .unwrap_or_default();
    (codes, nets)
}

/// Benign build-info codes that may accompany a legal vector build.
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
}

/// The net holding `path`, if any.
fn net_holding<'a>(nets: &'a [(String, Vec<String>)], path: &str) -> Option<&'a Vec<String>> {
    nets.iter()
        .find(|(_, ps)| ps.iter().any(|p| p == path))
        .map(|(_, ps)| ps)
}

/// True when no net in the store carries a `capN.*` point (zero partial net).
fn no_cap_member_on_any_net(nets: &[(String, Vec<String>)]) -> bool {
    nets.iter()
        .all(|(_, ps)| !ps.iter().any(|p| p.starts_with("cap")))
}

// ── Legal: equal-row zip, two independent 2-point nets ──────────────────────

/// `c[1:2] -> [VDD, GND]` — whole declared array node vs a 2-member column.
/// Row-zips `c1.2↔VDD`, `c2.2↔GND` into two separate 2-point nets.
#[test]
fn whole_array_row_zips_two_independent_nets() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    cap[1:2] -> [VDD, GND]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-array.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(rest, Vec::<u32>::new(), "legal zip is quiet; got {codes:?}");

    // Anchor from each cap member and inspect the io point co-resident on its
    // net. The io path may alias (VDD / GND / GND@<ordinal>) — membership only.
    let cap1 = net_holding(&nets, "cap1.2").expect("net carrying cap1.2");
    let cap2 = net_holding(&nets, "cap2.2").expect("net carrying cap2.2");
    assert!(
        cap1.iter().any(|p| p == "VDD"),
        "cap1.2 net carries VDD; got {cap1:?}"
    );
    assert!(
        cap2.iter().any(|p| p == "GND" || p.starts_with("GND@")),
        "cap2.2 net carries GND; got {cap2:?}"
    );
    assert!(
        !cap1.iter().any(|p| p.starts_with("cap2")),
        "cap1.2 net does NOT carry cap2.2 (no collapse); got {cap1:?}"
    );
    assert!(
        !cap2.iter().any(|p| p == "VDD"),
        "GND net does NOT carry VDD; got {cap2:?}"
    );
    assert_eq!(cap1.len(), 2, "VDD net is a 2-point net; got {cap1:?}");
    assert_eq!(cap2.len(), 2, "GND net is a 2-point net; got {cap2:?}");
}

/// `c[1:2].2 -> [VDD, GND]` — explicit member-column `.2` on the whole array.
/// Reference form: identical row-zip to the plain array form (§5.2: the member
/// column `.2` IS the array node's right column).
#[test]
fn member_col_dot2_row_zips_like_plain_array() {
    let plain = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    cap[1:2] -> [VDD, GND]\n}}"
    );
    let dot2 = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    cap[1:2].2 -> [VDD, GND]\n}}"
    );
    let (_, nets_a) = build(&plain, "/mcc/rz-a.mc");
    let (_, nets_b) = build(&dot2, "/mcc/rz-b.mc");
    // Same row membership on both sides.
    for nets in [&nets_a, &nets_b] {
        let cap1 = net_holding(nets, "cap1.2").expect("net carrying cap1.2");
        assert!(
            cap1.iter().any(|p| p == "VDD"),
            "cap1.2 net carries VDD; got {cap1:?}"
        );
        let cap2 = net_holding(nets, "cap2.2").expect("net carrying cap2.2");
        assert!(
            cap2.iter().any(|p| p == "GND" || p.starts_with("GND@")),
            "cap2.2 net carries GND; got {cap2:?}"
        );
        assert_eq!(cap1.len(), 2, "cap1.2 net 2-point; got {cap1:?}");
        assert_eq!(cap2.len(), 2, "cap2.2 net 2-point; got {cap2:?}");
    }
}

/// `[c1.2, c2.2] -> [VDD, GND]` — explicit dotted member list on the left.
/// Reference form: positional 1:1 row zip, identical to the array forms.
#[test]
fn dotted_member_list_row_zips_like_plain_array() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    [cap1.2, cap2.2] -> [VDD, GND]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-dotted.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "dotted list zip is quiet; got {codes:?}"
    );

    let cap1 = net_holding(&nets, "cap1.2").expect("net carrying cap1.2");
    assert!(
        cap1.iter().any(|p| p == "VDD"),
        "cap1.2 net carries VDD; got {cap1:?}"
    );
    let cap2 = net_holding(&nets, "cap2.2").expect("net carrying cap2.2");
    assert!(
        cap2.iter().any(|p| p == "GND" || p.starts_with("GND@")),
        "cap2.2 net carries GND; got {cap2:?}"
    );
    assert!(
        !cap1.iter().any(|p| p.starts_with("cap2")),
        "no collapse: cap1.2 net has no cap2; got {cap1:?}"
    );
}

/// `[VDD, GND] -> c[1:2]` — 2-member column on the left against the whole
/// array node on the right. The array node's LEFT column is its 2 members'
/// `.1` pins, so this is an equal **2x1 vs 2x1** row zip (VDD↔cap1.1,
/// GND↔cap2.1) — legal, NOT a broadcast.
#[test]
fn two_member_list_against_array_node_is_equal_row_zip() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    [VDD, GND] -> cap[1:2]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-left.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "2x1 vs 2x1 zip is quiet; got {codes:?}"
    );

    let cap1 = net_holding(&nets, "cap1.1").expect("net carrying cap1.1");
    assert!(
        cap1.iter().any(|p| p == "VDD"),
        "cap1.1 net carries VDD; got {cap1:?}"
    );
    let cap2 = net_holding(&nets, "cap2.1").expect("net carrying cap2.1");
    assert!(
        cap2.iter().any(|p| p == "GND" || p.starts_with("GND@")),
        "cap2.1 net carries GND; got {cap2:?}"
    );
    assert!(
        !cap1.iter().any(|p| p.starts_with("cap2")),
        "no collapse; got {cap1:?}"
    );
}

/// `cap[4:5] -> PWR{VCC, GND}` — DC-bus member group on the right (2 members).
/// Row-zips `cap4.2↔PWR.VCC`, `cap5.2↔PWR.GND`; never collapses both members
/// onto one rail (the old §5.3.1 re-link broadcast).
#[test]
fn dc_bus_rail_zips_row_aligned() {
    let src = format!(
        "{CAP_BUS}module main {{\n    io PWR{{VCC, GND}}\n    cap[4:5]::CAP()\n    cap[4:5] -> PWR{{VCC, GND}}\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-dc.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "DC-bus zip is quiet; got {codes:?}"
    );

    let vcc = net_holding(&nets, "PWR.VCC").expect("net carrying PWR.VCC");
    let gnd = net_holding(&nets, "PWR.GND").expect("net carrying PWR.GND");
    assert!(
        vcc.iter().any(|p| p == "cap4.2"),
        "PWR.VCC carries cap4.2; got {vcc:?}"
    );
    assert!(
        !vcc.iter().any(|p| p.starts_with("cap5")),
        "broadcast abolished: cap5.2 NOT on PWR.VCC; got {vcc:?}"
    );
    assert!(
        gnd.iter().any(|p| p == "cap5.2"),
        "PWR.GND carries cap5.2; got {gnd:?}"
    );
    assert_eq!(vcc.len(), 2, "PWR.VCC 2-point; got {vcc:?}");
    assert_eq!(gnd.len(), 2, "PWR.GND 2-point; got {gnd:?}");
}

// ── Illegal: single-point vs row → E4007, zero partial nets ─────────────────

/// `c[1:2] -> GND` (2x1 node column vs a 1-row scalar) and `VDD -> c[1:2]`
/// (1-row scalar vs 2x1): §5.3.1 single-point broadcast, illegal. E4007, and
/// **no** cap member point may appear on any net (no pair-by-min recovery).
#[test]
fn single_point_vs_row_is_illegal_with_zero_partial_net() {
    for (stmt, uri) in [
        ("cap[1:2] -> GND", "/mcc/illegal-r.mc"),
        ("VDD -> cap[1:2]", "/mcc/illegal-l.mc"),
    ] {
        let src = format!(
            "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    {stmt}\n}}"
        );
        let (codes, nets) = build(&src, uri);
        assert!(
            codes.contains(&4007),
            "E4007 for single-point broadcast `{stmt}`; got {codes:?}"
        );
        assert!(
            no_cap_member_on_any_net(&nets),
            "zero partial net for `{stmt}`; got nets {nets:?}"
        );
    }
}

/// `[VDD, GND, A] -> c[1:2]` — 3x1 vs 2x1 mismatched rows: E4007, zero nets.
#[test]
fn mismatched_row_counts_error_with_zero_nets() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    io A\n    cap[1:2]::CAP()\n    [VDD, GND, A] -> cap[1:2]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-mismatch.mc");
    assert!(codes.contains(&4007), "E4007 for 3x1 vs 2x1; got {codes:?}");
    assert!(
        no_cap_member_on_any_net(&nets),
        "zero partial net; got nets {nets:?}"
    );
}

// ── Group (,) fan: §7.3 legal fan-in, mismatch → E4007 zero nets ────────────

/// `(cap1.2, cap2.2) -> GND` — the `(,)` group is the legal fan-in form: both
/// members share the GND multi-terminal net (one 3-point net), NOT a broadcast
/// per-member collapse and NOT an error.
#[test]
fn group_fan_in_shares_one_multiterminal_net() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io GND\n    cap[1:2]::CAP()\n    (cap1.2, cap2.2) -> GND\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-group.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "group fan-in is quiet; got {codes:?}"
    );

    let gnd = net_holding(&nets, "GND").expect("net carrying GND");
    assert!(
        gnd.iter().any(|p| p == "cap1.2") && gnd.iter().any(|p| p == "cap2.2"),
        "GND net carries both group members; got {gnd:?}"
    );
    assert_eq!(gnd.len(), 3, "one 3-point multiterminal net; got {gnd:?}");
}

/// `(cap1.2, cap2.2) -> [VDD, GND, A]` — group branch vs a 3-column target:
/// each branch's scalar is 1x1 against a 1x3 list → mismatch → E4007, zero nets.
#[test]
fn group_mismatched_branch_errors_with_zero_nets() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    io A\n    cap[1:2]::CAP()\n    (cap1.2, cap2.2) -> [VDD, GND, A]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-group-mismatch.mc");
    assert!(
        codes.contains(&4007),
        "E4007 for mismatched group branch; got {codes:?}"
    );
    assert!(
        no_cap_member_on_any_net(&nets),
        "zero partial net; got nets {nets:?}"
    );
}
