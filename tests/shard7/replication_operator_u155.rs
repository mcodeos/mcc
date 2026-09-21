// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U155 — anonymous replication `Phrase * N` / `Phrase × N`.
//!
//! Both spellings must produce the **same** `MCAST_OPD_MULTI` parse (the `×`
//! lexer rule exists purely as a typographic alias), the engine must expand
//! an int literal N >= 2 into `McPhrase::Multiple` of N fresh copies, and the
//! two diagnostic lanes must hold: int < 2 → `E4216
//! CONN_REPLICATION_COUNT`, any other right operand → the pre-existing
//! `E4008 CONN_OPERATOR_UNSUPPORTED`.
//!
//! Same extraction path as `vec_r0_operator_encoding.rs` (func body via
//! `McFunction.stmts`), so the encoding under test is the parser product
//! itself.

// Family naming `{family}__{essence}` uses a doubled underscore (matrix §1).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Plain two-pin resistor, pins `1 = 1` / `2 = 2`.
const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Parse one statement in a func body; return `(stmt count, stmt 0 text,
/// non-benign diagnostic codes)`.
fn encoded(stmt: &str) -> (usize, String, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{RES2}module main {{\n    func M() {{\n        {stmt}\n    }}\n}}\n"
    );
    let u = McURI::from("/mcc/u155-replication.mc");
    mcc::mcc_load_from_string(&u, &src);
    let inst = mcc::mcc_build(&McIds::from("main"), &u).expect("build");
    let stmts = inst
        .def
        .funcs
        .find("M")
        .map(|f| f.stmts.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        // Benign naming/noise family from the minimal harness: 5052/5056
        // (CMIE-shadow naming from the synthetic names), 5054 (single-char
        // func name `M`), 505x port/instance convention, 5641-5643 unused
        // declaration advisories.
        .filter(|c| !matches!(c, 5641 | 5642 | 5643 | 5051..=5057))
        .collect();
    codes.sort_unstable();
    codes.dedup();
    (stmts.len(), stmts.first().cloned().unwrap_or_default(), codes)
}

#[test]
fn replication__star_and_times_spellings_encode_identically() {
    let star = encoded("RES2*2");
    let times = encoded("RES2 × 2");
    assert_eq!(star.2, Vec::<u32>::new(), "`*2` must be quiet; got {:?}", star.2);
    assert_eq!(times.2, Vec::<u32>::new(), "`× 2` must be quiet; got {:?}", times.2);
    assert_eq!(star.0, 1, "`*2` must be one stmt");
    assert_eq!(times.0, 1, "`× 2` must be one stmt");
    // The two spellings land on the same engine node, so the parser product
    // is byte-identical (same auto names, same order).
    assert_eq!(star.1, times.1, "`*` and `×` must encode identically");
    // Two fresh copies of the same anonymous phrase.
    assert_eq!(star.1.matches("Multiple").count(), 1, "expected a Multiple node: {}", star.1);
    let copies = star.1.matches("Resistor").count() + star.1.matches("RES2").count();
    assert!(copies >= 2, "expected 2 copies in {}", star.1);
}

#[test]
fn replication__count_below_two_is_E4216() {
    for stmt in ["RES2*1", "RES2*0"] {
        let (_, _, codes) = encoded(stmt);
        assert!(
            codes.contains(&4216),
            "`{stmt}` must report E4216; got {codes:?}"
        );
    }
}

#[test]
fn replication__non_int_right_keeps_E4008() {
    let (_, _, codes) = encoded("RES2*R101");
    assert!(
        codes.contains(&4008),
        "`*R101` must keep the operator-unsupported fallthrough; got {codes:?}"
    );
}
