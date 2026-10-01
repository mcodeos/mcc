// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U372 acceptance surface: `'` / `^` riding curly member forms
//! (`D2{A, K}'`), bus-table names (`U1.I2C0'`) and keyword base member forms
//! (`this{PA, PK}'`) were silent no-ops; leg1 normalizes them at the
//! statement boundary so every wrapper spelling dissolves to the same
//! member vector the explicit list spelling
//! (`[D2.A, D2.K]'`) always rode (`curly-wrapper-transpose-design.md` §7).
//!
//! **The law under lock is the equivalence**: the curly form and the
//! explicit list form of the same operand produce the same diagnostic set
//! and the same net partition. The E2903 gate narrows to multi-element
//! degenerate columns (ruling 2): single points — labels, `_`, `pins.N`,
//! bare instances at the parse face — are exempt and silent.
//!
//! Three layers, per the filing plan: table-driven golden cells,
//! programmatic equivalence invariants, and explicit golden assertions for
//! the cells a loop could mask. Net *names* are synthesized, so claims are
//! about the **grouping of points** (same normalization as
//! `vec_parallel_transposed_bridge.rs`).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Two-pin diode-shaped body, pins `1 = A` / `2 = K`.
const DI2: &str = "component DI2 {\n    pins = [\n        1 = A\n        2 = K\n    ]\n}\n";

/// Three-pin body, pins `1 = A` / `2 = B` / `3 = C`.
const DI3: &str = "component DI3 {\n    pins = [\n        1 = A\n        2 = B\n        3 = C\n    ]\n}\n";

/// Two-pin body with a named bus table `B0` over pins 1/2 (ruling 2:
/// bus-table names ride the same dissolution), plus the two-pin [`DI2`]
/// body as the zip partner. The interface is declared inline — in-process
/// builds see no system library.
const BUS2: &str = concat!(
    "interface MYBUS(role)\n{\n",
    "    topology = \"multi-point\"\n",
    "    mode = [\"half duplex\"]\n",
    "    pins = [\n",
    "        1 = SCL @class(digital), \"clock\"\n",
    "        2 = SDA @class(digital), \"data\"\n",
    "    ]\n",
    "    role MASTER {\n        name = \"master\"\n        pins = [\n            out 1 = SCL @class(digital)\n            io 2 = SDA @class(digital)\n        ]\n        peer = SLAVE\n    }\n",
    "    role SLAVE {\n        name = \"slave\"\n        pins = [\n            in 1 = SCL @class(digital)\n            io 2 = SDA @class(digital)\n        ]\n        peer = MASTER\n    }\n",
    "}\n",
    "component BUS2 {\n    pins = [\n        io [1,2] = B0::MYBUS(MASTER)\n    ]\n}\n"
);

/// Codes that are build bookkeeping or connectivity bookkeeping, not a
/// verdict about the operator face (same tolerance set as the bridge
/// family, plus the unconnected-pin warnings the negative cells accrue).
fn benign(c: u32) -> bool {
    matches!(c, 4112 | 4116 | 4117 | 4119 | 5054 | 5070 | 5071 | 5072 | 5641 | 5642 | 5643)
}

/// Build `main` from a full source text; returns (non-benign codes sorted,
/// net partition as sorted lists of sorted point paths).
fn build_src(src: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();
    codes.dedup();

    let mut partition: Vec<Vec<String>> = net_store
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
    partition.sort();
    (codes, partition)
}

/// Standard two-label fixture body: instances `D1`/`D2` of [`DI2`] and four
/// `io` labels `L1`..`L4`.
fn di2_module(stmt: &str) -> String {
    format!(
        "{DI2}module main {{\n    DI2 D1\n    DI2 D2\n    io L1\n    io L2\n    io L3\n    io L4\n{stmt}\n}}\n"
    )
}

fn build_di2(stmt: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    build_src(&di2_module(stmt), uri)
}

/// The net that carries `path`, or `None`.
fn net_holding<'a>(nets: &'a [Vec<String>], path: &str) -> Option<&'a Vec<String>> {
    nets.iter().find(|ps| ps.iter().any(|p| p == path))
}

// ── layer 1: table-driven golden cells ─────────────────────────────────────

/// Baseline zip and the wrapper families on the device row: each cell is
/// (statement, expected non-benign codes, expected net partition). The net
/// partition claims grouping, never synthesized names.
#[test]
fn golden__device_row_cells() {
    let cells: &[(&str, &str, &[u32], &[&[&str]])] = &[
        // No wrapper: the zip law (§9.6) — lane k pairs with lane k.
        (
            "baseline_zip",
            "    D1{A, K} - D2{A, K}",
            &[],
            &[&["D1.1", "D2.1"], &["D1.2", "D2.2"]],
        ),
        // `'` on the member vector: the transpose is real — a 2-wide row
        // cannot series-pair with the 2-lane zip (E4007), and nothing wires.
        (
            "curly_transpose",
            "    D1{A, K} - D2{A, K}'",
            &[4007],
            &[],
        ),
        (
            "list_transpose",
            "    D1{A, K} - [D2.A, D2.K]'",
            &[4007],
            &[],
        ),
        // `^`: the reverse is real. Both spellings carry the downstream
        // E4007 and the reversed chain wires its own members (the §2.4.5
        // view of `D2.K - D2.A`). The list spelling additionally carries the
        // parse-phase E2903 (multi-element degenerate column); the curly
        // spelling is exempt there — the undissolved bus is a point at the
        // parse face — a recorded phase difference (design §7.2).
        (
            "curly_rev",
            "    D1{A, K} - D2{A, K}^",
            &[4007],
            &[&["D2.1", "D2.2"]],
        ),
        (
            "list_rev",
            "    D1{A, K} - [D2.A, D2.K]^",
            &[2903, 4007],
            &[&["D2.1", "D2.2"]],
        ),
    ];
    for (tag, stmt, want_codes, want_nets) in cells {
        let (codes, nets) = build_di2(stmt, "/mcc/u372-golden.mc");
        let want_codes: Vec<u32> = want_codes.to_vec();
        assert_eq!(codes, want_codes, "{tag}: codes");
        let want_nets: Vec<Vec<String>> = want_nets
            .iter()
            .map(|ps| ps.iter().map(|s| s.to_string()).collect())
            .collect();
        assert_eq!(nets, want_nets, "{tag}: net partition");
    }
}

/// Three-pin member vector under `'`: E2902 (shape-limit law L9 — transpose
/// accepts 1x1 / 1x2 / 2x1 / 2x2 only), same code as the list spelling.
#[test]
fn golden__three_pin_over_limit_reports_e2902() {
    for (tag, stmt) in [
        ("curly", "    DI3 D3\n    L1 - L2 - L3 - D3{A, B, C}'"),
        ("list", "    DI3 D3\n    L1 - L2 - L3 - [D3.A, D3.B, D3.C]'"),
    ] {
        let src = format!("{DI3}module main {{\n    io L1\n    io L2\n    io L3\n{stmt}\n}}\n");
        let (codes, _nets) = build_src(&src, "/mcc/u372-over-limit.mc");
        assert!(
            codes.contains(&2902),
            "{tag}: over-limit transpose must report E2902; got {codes:?}"
        );
    }
}

/// E2903 narrowing (ruling 2): single-point operands are the exempt,
/// silent class; a multi-element degenerate column keeps the warning.
#[test]
fn golden__e2903_narrowing_boundary() {
    // Exempt: single points stay quiet under every `^` spelling.
    for (tag, stmt) in [
        ("label", "    L1^ - L2"),
        ("pin", "    D1.pins.1^ - L1"),
        ("underscore", "    L1 - D1 - _^"),
        ("curly_member_vector", "    D1{A, K} - D2{A, K}^"),
    ] {
        let (codes, _nets) = build_di2(stmt, "/mcc/u372-e2903-exempt.mc");
        assert!(
            !codes.contains(&2903),
            "{tag}: single-point ^ must be exempt from E2903; got {codes:?}"
        );
    }
    // Kept: a two-element degenerate column still warns.
    let (codes, _nets) = build_di2("    L1 - [L3, L4]^", "/mcc/u372-e2903-kept.mc");
    assert!(
        codes.contains(&2903),
        "multi-element degenerate ^ keeps E2903; got {codes:?}"
    );
}

// ── layer 2: programmatic equivalence invariants ───────────────────────────

/// The brace ≡ list equivalence law (L14 / design §6 ruling 1): for every
/// `'`-family spelling the curly member form and the explicit member list
/// produce the same diagnostic set **and** the same net partition. The
/// `^`-family spellings agree on the net partition but carry the recorded
/// E2903 phase asymmetry (curly exempt at the parse face, list warns), so
/// they are compared net-side only.
#[test]
fn invariant__curly_equals_list_everywhere() {
    for (tag, curly, list) in [
        // Transpose: the batch's core cell.
        ("apos", "D2{A, K}'", "[D2.A, D2.K]'"),
        // Mirrored order.
        ("apos_mirrored", "D2{K, A}'", "[D2.K, D2.A]'"),
    ] {
        let (c_curly, n_curly) =
            build_di2(&format!("    D1{{A, K}} - {curly}"), "/mcc/u372-eq-curly.mc");
        let (c_list, n_list) =
            build_di2(&format!("    D1{{A, K}} - {list}"), "/mcc/u372-eq-list.mc");
        assert_eq!(c_curly, c_list, "{tag}: code sets must match");
        assert_eq!(n_curly, n_list, "{tag}: net partitions must match");
    }
    for (tag, curly, list) in [
        // Reverse: net-equivalent; only the parse-phase E2903 differs.
        ("caret", "D2{A, K}^", "[D2.A, D2.K]^"),
        // Shell-order composite: the `^'` list form is rejected at pass0
        // (row-vs-point, before any wiring) while the curly form reaches
        // pass2 — the recorded open edge (design §10).
        ("caret_apos", "D2{A, K}^'", "[D2.A, D2.K]^'"),
    ] {
        let (c_curly, n_curly) =
            build_di2(&format!("    D1{{A, K}} - {curly}"), "/mcc/u372-eq-curly.mc");
        let (c_list, n_list) =
            build_di2(&format!("    D1{{A, K}} - {list}"), "/mcc/u372-eq-list.mc");
        // Diagnostic phase asymmetry (design §7.2, ruling 2): the list
        // spelling carries the parse-phase E2903; the curly spelling is
        // exempt there because the undissolved bus is a point at the parse
        // face. Both carry the downstream E4007.
        assert!(
            c_list.contains(&2903) && !c_curly.contains(&2903),
            "{tag}: the parse-phase E2903 asymmetry moved; curly {c_curly:?} list {c_list:?}"
        );
        assert_eq!(
            c_curly,
            c_list.iter().copied().filter(|c| *c != 2903).collect::<Vec<_>>(),
            "{tag}: downstream codes must match modulo the parse-phase E2903"
        );
        if tag == "caret" {
            assert_eq!(
                n_curly, n_list,
                "caret: both spellings wire the reversed chain identically"
            );
        } else {
            assert_eq!(
                n_curly,
                vec![vec!["D2.1".to_string(), "D2.2".to_string()]],
                "caret_apos: the curly side leaves the reversed chain wired"
            );
            assert!(
                n_list.is_empty(),
                "caret_apos: the list side is rejected at pass0 before wiring"
            );
        }
    }
}

/// Bus-table names ride the same dissolution (ruling 2): `U1.B0'` and
/// `[U1.B0.SCL, U1.B0.SDA]'` agree, and the transpose is real (≠ the
/// un-wrapped zip baseline).
#[test]
fn invariant__bus_table_name_equals_member_list() {
    let head = format!(
        "{BUS2}{DI2}module main {{\n    BUS2 U1\n    DI2 D1\n"
    );
    let (c_base, n_base) =
        build_src(&format!("{head}    D1{{A, K}} - U1.B0\n}}\n"), "/mcc/u372-bus-base.mc");
    assert_eq!(
        n_base,
        vec![
            vec!["D1.1".to_string(), "U1.1".to_string()],
            vec!["D1.2".to_string(), "U1.2".to_string()]
        ],
        "bus zip baseline pairs lane k with lane k; got {n_base:?}"
    );

    // Pairwise: curly == list, and both differ from the baseline (the
    // transpose took effect).
    let (c_curly, n_curly) =
        build_src(&format!("{head}    D1{{A, K}} - U1.B0'\n}}\n"), "/mcc/u372-bus-curly.mc");
    let (c_list, n_list) = build_src(
        &format!("{head}    D1{{A, K}} - [U1.B0.SCL, U1.B0.SDA]'\n}}\n"),
        "/mcc/u372-bus-list.mc",
    );
    assert_eq!(c_curly, c_list, "bus transpose: code sets must match");
    assert_eq!(n_curly, n_list, "bus transpose: net partitions must match");
    assert!(
        c_curly.contains(&4007),
        "the transposed bus row cannot zip with the member vector; got {c_curly:?}"
    );
    assert_ne!(
        c_curly, c_base,
        "the transpose must not be a silent identity"
    );
}

/// Keyword base member forms (`this{PA, PK}`) are standard member vectors:
/// the wrapper agrees with the explicit dotted list (codes; the net store is
/// not consulted because the top-face wiring of `this` faces is judged by
/// the code set here).
#[test]
fn invariant__this_member_form_equals_dotted_list() {
    let head = "module main {\n    io PA\n    io PK\n    io L1\n    io L2\n";
    let (c_curly, _) =
        build_src(&format!("{head}    this{{PA, PK}}' - [L1, L2]\n}}\n"), "/mcc/u372-this-curly.mc");
    let (c_list, _) = build_src(
        &format!("{head}    [this.PA, this.PK]' - [L1, L2]\n}}\n"),
        "/mcc/u372-this-list.mc",
    );
    assert_eq!(c_curly, c_list, "this-member forms must agree with the dotted list");
    assert!(
        c_curly.contains(&4007),
        "the transposed row cannot series-pair with the column; got {c_curly:?}"
    );
}

/// Soundness: every over-limit or mismatched composite carries a non-empty
/// non-benign code set — silence would be the pre-U372 regression.
#[test]
fn invariant__composite_spellings_stay_audible() {
    // Three-member vector: over the 2-wide transpose domain (E2902), same
    // code as the list spelling.
    let src = format!(
        "{DI3}module main {{\n    DI3 D3\n    io L1\n    io L2\n    io L3\n    \
         L1 - L2 - L3 - D3{{A, B, C}}'\n}}\n"
    );
    let (codes, _nets) = build_src(&src, "/mcc/u372-audible-3pin.mc");
    assert!(
        codes.contains(&2902),
        "three_pin_curly: expected E2902 in the code set; got {codes:?}"
    );

    let (codes, _nets) = build_di2("    D1{A, K} - D2{A, K}'", "/mcc/u372-audible.mc");
    assert!(
        codes.contains(&4007),
        "transpose_mismatch: expected E4007 in the code set; got {codes:?}"
    );
}

// ── layer 3: explicit golden assertions ────────────────────────────────────

/// `L1 - D1 - _'` ≡ `L1 - D1 - _`: the placeholder transposes to itself
/// (L20 — a placeholder has no faces to swap). The wrapper must be the exact
/// identity of the bare placeholder — same codes, same partition, and
/// whatever ideal-lead element the bare `_` semantics mint, the wrapper must
/// not mint one more.
#[test]
fn golden__placeholder_transpose_is_the_bare_lead() {
    let (c0, n0) = build_di2("    L1 - D1 - _", "/mcc/u372-lead-base.mc");
    let (codes, nets) = build_di2("    L1 - D1 - _'", "/mcc/u372-lead.mc");
    assert_eq!(codes, c0, "wrapper on the placeholder must not add diagnostics");
    assert_eq!(nets, n0, "wrapper on the placeholder must be the exact identity");
    let l1 = net_holding(&nets, "L1").expect("net holding L1");
    assert!(
        l1.contains(&"D1.1".to_string()),
        "L1 wires to D1's near pin; got {l1:?}"
    );
}

/// Label wrapper identities: every `'` / `^` / `()` spelling on a bare label
/// is the identity — quiet and net-identical to the unwrapped baseline
/// (ruling 3: no error, no warning). The group form wraps the operand, so
/// the paren cells are written as `(L1)` standing in the operand slot.
#[test]
fn golden__label_wrapper_matrix_is_identity() {
    let (c0, n0) = build_di2("    L1 - L2", "/mcc/u372-label-base.mc");
    assert!(c0.is_empty(), "baseline must be quiet; got {c0:?}");
    for w in [
        "L1'", "L1^", "L1'^", "L1^'", "(L1)", "(L1)'", "(L1)^", "L1^^",
    ] {
        let stmt = format!("    {w} - L2");
        let (codes, nets) = build_di2(&stmt, "/mcc/u372-label-wrap.mc");
        assert_eq!(codes, c0, "wrapper `{w}` on a label must stay quiet; got {codes:?}");
        assert_eq!(nets, n0, "wrapper `{w}` on a label must be the identity; got {nets:?}");
    }
}

/// `D2{A,K}'^`: the `'` then `^` composite stays audible (the row-reversed
/// form is not the baseline) — locks the shell-order family against a
/// silent collapse.
#[test]
fn golden__apos_caret_composite_is_not_the_baseline() {
    let (c_base, n_base) = build_di2("    D1{A, K} - D2{A, K}", "/mcc/u372-ac-base.mc");
    let (codes, nets) = build_di2("    D1{A, K} - D2{A, K}'^", "/mcc/u372-ac.mc");
    assert!(
        codes.contains(&4007),
        "the composite must stay audible; got {codes:?}"
    );
    assert_ne!(nets, n_base, "the composite must not collapse to the baseline zip");
}
