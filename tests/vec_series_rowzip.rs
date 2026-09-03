// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Family naming `{family}__{essence}` deliberately uses a doubled underscore to
// separate the grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

//! Topology oracle for vector series statements (vec-dianlu §5.2 / §5.3 / §7.3).
//!
//! **Matrix index** — every legal/illegal shape pair of the vector-series
//! grammar is locked to a point-level net-topology cell below. Shape:
//! S = scalar, A = whole-array node, C = `.k` member column, L = dotted/list
//! column, B = named DC bus `X{a,b}`, G = `(,)` group. The array node is
//! side-asymmetric (its left foot is the members' `.2` column, its right foot
//! the `.1` column). Wl:Wr = left:right member widths.
//!
//! | # | LHS→RHS | Wl:Wr | verdict | cell |
//! |---|---|---|---|---|
//! | 1 | A→L | N:N | equal-row zip, two independent 2-pt nets | `series_eq__array_to_list` |
//! | 2 | A→B | N:N | row-aligned zip, rails stay separate | `series_eq__array_to_dc_bus` |
//! | 3 | L→A | N:N | legal (right foot = `.1` column) | `series_eq__list_to_array` |
//! | 4 | C(.2)→L | N:N | legal, same alignment as the plain array | `series_eq__membercol_dot2_to_list` |
//! | 5 | L→L | N:N | legal | `series_eq__dotted_list_to_list` |
//! | 6/7 | A→S / S→A | N:1 / 1:N | E4007, zero partial nets | `series_illegal__array_vs_scalar` |
//! | 8 | L→A | 3:2 | E4007, zero nets | `series_illegal__n_vs_m` |
//! | 9 | G→S | — | legal fan-in, one 3-pt net | `group_fan__in_shared_multiterminal` |
//! | 10 | G→L | — | branch mismatch E4007, zero nets | `group_fan__mismatch_branch_zero` |
//! | 11 | S→G | — | legal fan-out, shared 3-pt net | `group_fan__out_source_shares_multi_terminal` |
//! | 12 | C(.2)→S | N:1 | E4007, zero nets | `series_illegal__membercol_to_scalar` |
//! | 13 | S→L | 1:N | E4007, zero nets | `series_illegal__scalar_to_dotted_list` |
//! | 14 | A→A | N:N | legal cap↔res zip | `series_eq__array_to_array` |
//! | 15 | C(.1)→C(.1) | N:N | legal member alignment (verdict half of dianlu_core `member_alignment_lane_slice_structure`) | `series_eq__membercol_to_membercol` |
//! | 16 | A(base≠1)→L | 3:3 | legal offset-base zip, ordinal-alias guard | `series_eq__offsetbase_array_to_list` |
//! | 17 | A(base≠1)→S | 3:1 | E4007, zero nets | `series_illegal__offsetbase_array_vs_scalar` |
//! | 18 | A→L | 3:3 | legal, ordinal-alias guard | `series_eq__array3_to_list3` |
//! | 19 | G chain, front & rear share mid | — | legal, no parallel/series shape error (verdict half of dianlu_core `group_chain_connection_count_structure`) | `group_fan__chain_front_rear_share_mid` |
//!
//! Cells are the **judgment half** of the vector grammar: assertions are
//! membership-based (a point path co-resident on its net), never net-count or
//! label-based, so port-ordinal aliasing (e.g. `GND@11`) cannot fake a pass.
//! The purpose is that a silent wrong-wiring regression — the old §5.3.1
//! single-point broadcast collapse — fails hard instead of passing as "zero
//! diagnostics + collapsed net".
//!
//! **Indexed elsewhere (not duplicated here)** — statement-level constructs
//! that keep their diagnostics/inst context in the owning file (matrix §2
//! rows 20–21):
//!
//! - `dispatch__*` (row 20, per-member receiver dispatch §7.6):
//!   `dianlu_core.rs::dispatch__*` (lane/trunk of a vector-receiver call),
//!   `gap1_member_set_alignment.rs::dispatch__*` (slice-arg gap1),
//!   `tablea_dispatch_regression.rs::dispatch__*` (per-member receiver forms).
//! - `member_scalar__*` (row 21, single-member range = scalar, contract E):
//!   `vector_inst_materialize.rs::member_scalar__*`,
//!   `single_member_range.rs::member_scalar__res4_range_is_scalar`.
//!
//! Deliberate non-cells (not invented): a true sub-range slice of a larger
//! declared array, S→A/B legal direct connect, and the `<-` mirror are **not**
//! locked here.

mod common;

use mcc::{McIds, McURI};

/// D8-style CAP: member io bus `io [1,2] = NODE{P, N}` (two-pin, pin names
/// `.1`/`.2`), constructed via the `::CAP()` array form.
const CAP_BUS: &str = "component CAP {\n    pins = [\n        io [1,2] = NODE{P, N}\n    ]\n}\n";

/// Plain two-pin CAP (`1 = 1`, `2 = 2`), constructed via `::CAP()`.
const CAP_PLAIN: &str = "component CAP {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Two-pin resistor mirror for the §10.6.3 group-chain cell (row 19): scalar
/// `RES Rn(1)` instances chained with `-` / `+`, pins `1 = 1` / `2 = 2`.
const RES_INT: &str =
    "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Build `main` and return (diagnostic codes sorted, net-store point lists).
fn build(src: &str, uri: &str) -> (Vec<u32>, Vec<(String, Vec<String>)>) {
    let _lock = common::lock();
    common::reset();
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

/// True when no net in the store carries a `<prefix>N.*` point (zero partial
/// net). The prefix is the instance-name stem shared by an array's members
/// (`"cap"` for `cap1`/`cap2`, `"res"` for `res1`/`res2`, ...). Only meaningful
/// against nets that carry member points of that array.
fn no_member_on_any_net(nets: &[(String, Vec<String>)], prefix: &str) -> bool {
    nets.iter()
        .all(|(_, ps)| !ps.iter().any(|p| p.starts_with(prefix)))
}

// ── series_eq__* : legal equal-row zip, two independent N-point nets ─────────

/// `c[1:2] -> [VDD, GND]` — whole declared array node vs a 2-member column.
/// Row-zips `c1.2↔VDD`, `c2.2↔GND` into two separate 2-point nets.
#[test]
fn series_eq__array_to_list() {
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
fn series_eq__membercol_dot2_to_list() {
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
fn series_eq__dotted_list_to_list() {
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
fn series_eq__list_to_array() {
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
fn series_eq__array_to_dc_bus() {
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

/// `cap[1:2] -> res[1:2]` — two whole declared 2-pin arrays (matrix row 14).
/// The left foot `.2` (cap1.2, cap2.2) row-zips the right foot `.1` (res1.1,
/// res2.1): two independent 2-point nets, no cross.
#[test]
fn series_eq__array_to_array() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    cap[1:2]::CAP()\n    res[1:2]::CAP()\n    cap[1:2] -> res[1:2]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-array-array.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "array-to-array zip is quiet; got {codes:?}"
    );

    let c1 = net_holding(&nets, "cap1.2").expect("net carrying cap1.2");
    assert!(
        c1.iter().any(|p| p == "res1.1"),
        "cap1.2 net carries res1.1; got {c1:?}"
    );
    assert!(
        !c1.iter().any(|p| p == "res2.1"),
        "no cross: cap1.2 net has no res2.1; got {c1:?}"
    );
    assert_eq!(c1.len(), 2, "cap1.2 net 2-point; got {c1:?}");
    let c2 = net_holding(&nets, "cap2.2").expect("net carrying cap2.2");
    assert!(
        c2.iter().any(|p| p == "res2.1"),
        "cap2.2 net carries res2.1; got {c2:?}"
    );
    assert!(
        !c2.iter().any(|p| p == "res1.1"),
        "no cross: cap2.2 net has no res1.1; got {c2:?}"
    );
    assert_eq!(c2.len(), 2, "cap2.2 net 2-point; got {c2:?}");
}

/// `cap[1:2].1 -> res[1:2].1` — member-column `.1` against member-column `.1`
/// (matrix row 15). Pass2 wires member-to-member (cap1.1↔res1.1,
/// cap2.1↔res2.1); the net layer zips positionally: two independent 2-point
/// nets, never a cross product. Verdict half of dianlu_core
/// `member_alignment_lane_slice_structure` (conn order + lane Slice→Slice stay
/// there).
#[test]
fn series_eq__membercol_to_membercol() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    cap[1:2]::CAP()\n    res[1:2]::CAP()\n    cap[1:2].1 -> res[1:2].1\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-membercol-membercol.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "member alignment is quiet; got {codes:?}"
    );

    let c1 = net_holding(&nets, "cap1.1").expect("net carrying cap1.1");
    assert!(
        c1.iter().any(|p| p == "res1.1"),
        "cap1.1 net carries res1.1; got {c1:?}"
    );
    assert!(
        !c1.iter().any(|p| p.starts_with("res2")),
        "no cross product; got {c1:?}"
    );
    assert_eq!(c1.len(), 2, "cap1.1 net 2-point; got {c1:?}");
    let c2 = net_holding(&nets, "cap2.1").expect("net carrying cap2.1");
    assert!(
        c2.iter().any(|p| p == "res2.1"),
        "cap2.1 net carries res2.1; got {c2:?}"
    );
    assert!(
        !c2.iter().any(|p| p.starts_with("res1")),
        "no cross product; got {c2:?}"
    );
    assert_eq!(c2.len(), 2, "cap2.1 net 2-point; got {c2:?}");
}

/// `cap[3:5] -> [VDD, GND, A]` — offset-base array node (members cap3/cap4/cap5,
/// matrix row 16). Each member row-zips its rail in written order, so member
/// indices (3,4,5) cannot alias port ordinals 1..3: cap3.2↔VDD, cap4.2↔GND,
/// cap5.2↔A as three independent 2-point nets.
#[test]
fn series_eq__offsetbase_array_to_list() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    io A\n    cap[3:5]::CAP()\n    cap[3:5] -> [VDD, GND, A]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-offset.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "offset-base zip is quiet; got {codes:?}"
    );

    for (m, rail) in [("cap3.2", "VDD"), ("cap4.2", "GND"), ("cap5.2", "A")] {
        let n = net_holding(&nets, m).unwrap_or_else(|| panic!("net carrying {m}"));
        assert!(
            n.iter()
                .any(|p| p == rail || p.starts_with(&format!("{rail}@"))),
            "{m} net carries {rail}; got {n:?}"
        );
        assert_eq!(n.len(), 2, "{m} net is a 2-point net; got {n:?}");
    }
}

/// `cap[1:3] -> [VDD, GND, A]` — a >2-row array-to-list zip (matrix row 18).
/// Every row is its own independent 2-point net (cap1.2↔VDD, cap2.2↔GND,
/// cap3.2↔A); with three rails the pairing cannot be faked by a port-ordinal
/// shortcut.
#[test]
fn series_eq__array3_to_list3() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    io A\n    cap[1:3]::CAP()\n    cap[1:3] -> [VDD, GND, A]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-array3.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(rest, Vec::<u32>::new(), "3-row zip is quiet; got {codes:?}");

    for (m, rail) in [("cap1.2", "VDD"), ("cap2.2", "GND"), ("cap3.2", "A")] {
        let n = net_holding(&nets, m).unwrap_or_else(|| panic!("net carrying {m}"));
        assert!(
            n.iter()
                .any(|p| p == rail || p.starts_with(&format!("{rail}@"))),
            "{m} net carries {rail}; got {n:?}"
        );
        assert_eq!(n.len(), 2, "{m} net is a 2-point net; got {n:?}");
        assert!(
            !n.iter().any(|p| p.starts_with("cap") && p != m),
            "{m} net has no sibling member (no collapse); got {n:?}"
        );
    }
}

// ── series_illegal__* : single-point / width mismatch → E4007, zero partial ──

/// `c[1:2] -> GND` (2x1 node column vs a 1-row scalar) and `VDD -> c[1:2]`
/// (1-row scalar vs 2x1): §5.3.1 single-point broadcast, illegal. E4007, and
/// **no** cap member point may appear on any net (no pair-by-min recovery).
#[test]
fn series_illegal__array_vs_scalar() {
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
            no_member_on_any_net(&nets, "cap"),
            "zero partial net for `{stmt}`; got nets {nets:?}"
        );
    }
}

/// `[VDD, GND, A] -> c[1:2]` — 3x1 vs 2x1 mismatched rows: E4007, zero nets.
#[test]
fn series_illegal__n_vs_m() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    io A\n    cap[1:2]::CAP()\n    [VDD, GND, A] -> cap[1:2]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-mismatch.mc");
    assert!(codes.contains(&4007), "E4007 for 3x1 vs 2x1; got {codes:?}");
    assert!(
        no_member_on_any_net(&nets, "cap"),
        "zero partial net; got nets {nets:?}"
    );
}

/// `cap[1:2].2 -> GND` — member-column `.2` (2 rows) against a scalar
/// (matrix row 12). 2:1 single-point broadcast: E4007, and no cap member point
/// appears on any net.
#[test]
fn series_illegal__membercol_to_scalar() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    cap[1:2].2 -> GND\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/illegal-membercol-scalar.mc");
    assert!(
        codes.contains(&4007),
        "E4007 for member-col vs scalar; got {codes:?}"
    );
    assert!(
        no_member_on_any_net(&nets, "cap"),
        "zero partial net; got nets {nets:?}"
    );
}

/// `VDD -> [cap1.2, cap2.2]` — a scalar against a 2-column dotted list
/// (matrix row 13). 1:2 single-point broadcast: E4007, zero partial nets.
#[test]
fn series_illegal__scalar_to_dotted_list() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    VDD -> [cap1.2, cap2.2]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/illegal-scalar-dotted.mc");
    assert!(
        codes.contains(&4007),
        "E4007 for scalar vs dotted list; got {codes:?}"
    );
    assert!(
        no_member_on_any_net(&nets, "cap"),
        "zero partial net; got nets {nets:?}"
    );
}

/// `cap[3:5] -> GND` — offset-base 3-row array against a scalar (matrix
/// row 17). 3:1 single-point broadcast: E4007, zero partial nets.
#[test]
fn series_illegal__offsetbase_array_vs_scalar() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[3:5]::CAP()\n    cap[3:5] -> GND\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/illegal-offset-scalar.mc");
    assert!(
        codes.contains(&4007),
        "E4007 for 3x1 offset array vs scalar; got {codes:?}"
    );
    assert!(
        no_member_on_any_net(&nets, "cap"),
        "zero partial net; got nets {nets:?}"
    );
}

// ── group_fan__* : (,) group fan-in / fan-out / chain — multi-terminal nets ──

/// `(cap1.2, cap2.2) -> GND` — the `(,)` group is the legal fan-in form: both
/// members share the GND multi-terminal net (one 3-point net), NOT a broadcast
/// per-member collapse and NOT an error.
#[test]
fn group_fan__in_shared_multiterminal() {
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

/// `VDD -> (cap1.2, cap2.2)` — the `(,)` group as the legal fan-out form
/// (matrix row 11, mirror of row 9): a scalar source drives every member, so
/// `{VDD, cap1.2, cap2.2}` is one 3-point multi-terminal net — quiet, not an
/// error and not a per-member broadcast.
#[test]
fn group_fan__out_source_shares_multi_terminal() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    cap[1:2]::CAP()\n    VDD -> (cap1.2, cap2.2)\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-group-out.mc");
    let rest: Vec<u32> = codes.iter().copied().filter(|c| !benign(*c)).collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "group fan-out is quiet; got {codes:?}"
    );

    let vdd = net_holding(&nets, "VDD").expect("net carrying VDD");
    assert!(
        vdd.iter().any(|p| p == "cap1.2") && vdd.iter().any(|p| p == "cap2.2"),
        "VDD net carries both group members; got {vdd:?}"
    );
    assert_eq!(vdd.len(), 3, "one 3-point multiterminal net; got {vdd:?}");
}

/// `(cap1.2, cap2.2) -> [VDD, GND, A]` — group branch vs a 3-column target:
/// each branch's scalar is 1x1 against a 1x3 list → mismatch → E4007, zero nets.
#[test]
fn group_fan__mismatch_branch_zero() {
    let src = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    io A\n    cap[1:2]::CAP()\n    (cap1.2, cap2.2) -> [VDD, GND, A]\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-group-mismatch.mc");
    assert!(
        codes.contains(&4007),
        "E4007 for mismatched group branch; got {codes:?}"
    );
    assert!(
        no_member_on_any_net(&nets, "cap"),
        "zero partial net; got nets {nets:?}"
    );
}

/// mcrule.md §10.6.3: a group `(,)` allows front AND rear operands — the rule
/// `opd1 op1 (s1, .., sN) op2 opd2` expands to per-branch statements that share
/// opd2 (matrix row 19). `R101 - (R102 - R103, R104 - R105) + R106` must build
/// without a shape error, and the net layer joins R106.1 with the shared left
/// net (R101.1), R106.2 with the shared right net (both group exits R103.2,
/// R105.2), and R101.2 with both branch heads (R102.1, R104.1). Verdict half of
/// dianlu_core `group_chain_connection_count_structure` (the exact 7-join conn
/// count stays there).
#[test]
fn group_fan__chain_front_rear_share_mid() {
    let src = format!(
        "{RES_INT}module main {{\n    RES R101(1), R102(1), R103(1), R104(1), R105(1), R106(1)\n    R101 - (R102 - R103, R104 - R105) + R106\n}}"
    );
    let (codes, nets) = build(&src, "/mcc/rz-gchain.mc");
    assert!(
        !codes.contains(&4005),
        "rule §10.6.3 allows a rear operand around a group; got {codes:?}"
    );
    assert!(
        !codes.contains(&4007),
        "no series shape error; got {codes:?}"
    );

    // Shared left net: R101.1 and R106.1 join.
    let left = net_holding(&nets, "R101.1").expect("left net holds R101.1");
    assert!(
        left.iter().any(|p| p == "R106.1"),
        "R106.1 joins the shared left net; got {left:?}"
    );
    // Shared right net: BOTH group exits and R106.2 join.
    let right = net_holding(&nets, "R103.2").expect("right net holds R103.2");
    assert!(
        right.iter().any(|p| p == "R105.2"),
        "R105.2 joins the shared right net; got {right:?}"
    );
    assert!(
        right.iter().any(|p| p == "R106.2"),
        "R106.2 joins the shared right net; got {right:?}"
    );
    // Series fan-out: R101.2 reaches both branch heads.
    let fan = net_holding(&nets, "R101.2").expect("fan-out net holds R101.2");
    assert!(
        fan.iter().any(|p| p == "R102.1"),
        "R101.2 joins branch head R102.1; got {fan:?}"
    );
    assert!(
        fan.iter().any(|p| p == "R104.1"),
        "R101.2 joins branch head R104.1; got {fan:?}"
    );
}
