// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Parallel `+` pairing-side **wiring fence** (S1 of the R0 implementation,
//! `r0-implementation-design.md` §3 / §6).
//!
//! S1 is a pure refactor of the shape layer: the pairing side of `+` becomes
//! derivable from `(lhs, rhs)` (`opcheck::parallel_attaches_right`) instead of
//! being an `align` mode the callers passed in, and the parser's merged call
//! site no longer carries a hand-written face check. The **rule** it must
//! preserve is locked at the unit level in `src/semantic/opcheck.rs`'s `tests`
//! module — including the two §2.2 mis-judgments (`N≠2,M==2` must be legal,
//! `N==2,M≠2` must be illegal), which need an asymmetric operand that the unit
//! cells build directly because the statement grammar barely reaches it.
//!
//! This file is the complementary **characterization fence**: it records the
//! verdict each source-reachable `+` form produced *before* S1, so S2 (the
//! `+ X'` arm removal) and S3 (the engine's `Parallel` fold) cannot change them
//! silently. The assertions are the observed diagnostics, not a restatement of
//! §5.1 — several of these forms do not reach the parallel check at all (a
//! `+` whose operand is an `[X, Y]` list is quiet), and describing *why* is a
//! separate question from fencing the behavior.
//!
//! Verified identical pre- and post-S1 by running this file against the stashed
//! (pre-S1) sources: the same verdicts in both trees.
//!
//! Most cells below read quiet; the three list-operand cells read E4005 — see
//! the correction note at the end of this header. The parallel shape mismatch
//! (E4005) fires for a `+` whose paired faces really disagree in width. The
//! form closest to a mismatch without being one — `R101 - A + R102` — pairs
//! instead, because a `-` chain's written end is a face of the operand
//! (`R101 - A` ends on the net `A`): Pass1 reads that face off the phrase
//! accessors, and Pass2 agrees.
//!
//! **2026-09-17 correction (three cells).** `[A, B] + R101`,
//! `R101 - R102 + [A, B]` and `[A, B] - [A, B] + [B, C, A]` used to read quiet
//! here, and that quiescence was *not* a rule: the list `[A, B]` presented an
//! empty (unknown-width) shape because its elements (`io A`, `io B`) are
//! shape-by-use ports (vec-dianlu.md §8.9.6.3), so the paired sides came out
//! unmeasurable and `check_parallel` wildcard-passed them. R4 forbids reading a
//! list that way — a written element occupies one column whatever its
//! declaration pinned down (vec-arch.md §4.1.1, the declared-scalar element
//! row), and the
//! same three forms written with **bare** names have always reported E4005.
//! The declared spelling now agrees with the bare one: unequal paired rows,
//! illegal, no broadcast carve-out for parallel (§5.1). The cells are updated
//! deliberately, as this fence requires, rather than silently.
//!
//! The real-board no-regression evidence lives in the `pwrint` / `hbl` netdiff
//! goldens (design doc §6 item 5); this file covers the grammar forms those
//! boards do not contain.

// Family naming `fence__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Two-pin resistor, pins `1 = 1` / `2 = 2` (`R101` → row `R101.1 * R101.2`).
const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

fn header(body: &str) -> String {
    format!(
        "{RES2}module main {{\n    io A\n    io B\n    io C\n    RES2 R101\n    RES2 R102\n{body}\n}}\n"
    )
}

/// The same module without the `io A` / `io B` / `io C` declarations: the
/// elements of `[A, B]` are then bare names, i.e. *not* shape-by-use ports.
/// Used to check that a list's verdict does not depend on whether its elements
/// are declared (correction note, 2026-09-17).
fn header_bare(body: &str) -> String {
    format!("{RES2}module main {{\n    RES2 R101\n    RES2 R102\n{body}\n}}\n")
}

/// Build `main` from a full source and return the emitted diagnostic codes,
/// sorted.
fn codes_of_src(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &u);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// Build `main` from `body` (with the declared-port header) and return the
/// emitted diagnostic codes, sorted.
fn codes_of(body: &str, uri: &str) -> Vec<u32> {
    codes_of_src(&header(body), uri)
}

fn has_e4005(codes: &[u32]) -> bool {
    codes.contains(&mcc::errcodes::CONN_PARALLEL_SHAPE_MISMATCH)
}

// quiet cells

/// `R101 - A + B` — 1*1 points on both sides of `+`.
#[test]
fn fence__point_plus_point_quiet() {
    let codes = codes_of("    R101 - A + B", "/mcc/par-pp.mc");
    assert!(!has_e4005(&codes), "got {codes:?}");
}

/// `[A, B] - [A, B] + [B, C]` — 2-row columns on both sides.
#[test]
fn fence__column_plus_column_quiet() {
    let codes = codes_of("    [A, B] - [A, B] + [B, C]", "/mcc/par-cc.mc");
    assert!(!has_e4005(&codes), "got {codes:?}");
}

/// `R101 - R102 + A` — a `-` chain (row) plus a 1*1 point on the written side.
#[test]
fn fence__row_plus_point_quiet() {
    let codes = codes_of("    R101 - R102 + A", "/mcc/par-rp.mc");
    assert!(!has_e4005(&codes), "got {codes:?}");
}

/// `A + R101` — a 1*1 point written on the left of a two-pin row.
#[test]
fn fence__point_plus_row_quiet() {
    let codes = codes_of("    A + R101", "/mcc/par-pr.mc");
    assert!(!has_e4005(&codes), "got {codes:?}");
}

/// `[A, B] + R101` — a list operand on the left of `+`: **E4005**, see the
/// header's note on the 2026-09-17 correction.
#[test]
fn fence__list_plus_row_mismatch() {
    let codes = codes_of("    [A, B] + R101", "/mcc/par-cr.mc");
    assert!(has_e4005(&codes), "got {codes:?}");
}

/// `R101 - R102 + [A, B]` — a list operand on the right of `+`: likewise
/// **E4005**.
#[test]
fn fence__row_plus_list_mismatch() {
    let codes = codes_of("    R101 - R102 + [A, B]", "/mcc/par-rc.mc");
    assert!(has_e4005(&codes), "got {codes:?}");
}

/// `[A, B] - [A, B] + [B, C, A]` — a 2-row and a 3-row column: **E4005**.
#[test]
fn fence__column_plus_wider_column_mismatch() {
    let codes = codes_of("    [A, B] - [A, B] + [B, C, A]", "/mcc/par-c3.mc");
    assert!(has_e4005(&codes), "got {codes:?}");
}

/// A list's verdict may not depend on whether its elements are declared: with
/// **bare** names `[A, B] + R101` has always read E4005, so the declared
/// spelling above must too. This is the assertion the 2026-09-17 correction
/// rests on — the quiescence it removed was the declared/bare divergence
/// itself, not a rule about lists.
#[test]
fn fence__list_plus_row_agrees_with_the_bare_spelling() {
    let codes = codes_of_src(&header_bare("    [A, B] + R101"), "/mcc/par-cr-bare.mc");
    assert!(has_e4005(&codes), "got {codes:?}");
}

/// `R101 - A + R102` — quiet: the two branches pair on both faces. A `-`
/// chain's written end is its right face (`R101 - A` ends on the net `A`), so
/// the two operands are 1-row branches and `+` ties `R101.1` to `R102.1` and
/// `A` to `R102.2`.
#[test]
fn fence__minus_chain_plus_row_quiet() {
    let codes = codes_of("    R101 - A + R102", "/mcc/par-rr.mc");
    assert!(!has_e4005(&codes), "got {codes:?}");
}
