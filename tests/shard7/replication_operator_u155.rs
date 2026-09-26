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

// Shape law (ruled 2026-09-21): P*N stacks N copies of P in the y direction —
// rows multiply by N, columns unchanged. 1×1→N×1, 1×2→N×2, 1×3 impossible,
// M×1→MN×1 in block order, 2-pin default 1×2 so `P'*N` = 2N×1, and
// `(A+B)*N` evaluates the group before replicating. The chain law judges
// rows after stacking: every mismatch below must be E4007, every exact
// match must be free of it.

/// Ports give the shape probes real 1×1 labels without floating-label noise
/// (unused-port advisories are part of the benign 56xx/50xx family already
/// filtered by `codes_in_module`).
const PORTS: &str = "module main {\n    in T1, T2, T3, T4, T5, T6, R1, R2, R3\n    func M() {\n";

fn codes_in_module(stmt: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{RES2}{PORTS}        {stmt}\n    }}\n}}\n");
    let u = McURI::from("/mcc/u155-shape.mc");
    mcc::mcc_load_from_string(&u, &src);
    mcc::mcc_build(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !matches!(c, 5641 | 5642 | 5643 | 5051..=5057))
        .collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

/// Exact row match: the statement must be free of shape/count/drop errors.
fn assert_quiet(stmt: &str) {
    let codes = codes_in_module(stmt);
    assert!(
        !codes.contains(&4007) && !codes.contains(&3132) && !codes.contains(&4216),
        "`{stmt}` must stack quietly; got {codes:?}"
    );
}

/// Row mismatch: the equal-width chain law must fire E4007 after stacking.
fn assert_E4007(stmt: &str) {
    let codes = codes_in_module(stmt);
    assert!(
        codes.contains(&4007),
        "`{stmt}` must report E4007 after stacking; got {codes:?}"
    );
}

#[test]
fn replication__shape_1x1_stacks_to_Nx1() {
    assert_quiet("[T1, T2] -> R1*2");
    assert_E4007("[T1] -> R1*2");
    assert_E4007("[T1, T2, T3] -> R1*2");
}

#[test]
fn replication__shape_1x2_stacks_to_Nx2() {
    // 2×1 column against a 2×2 two-pin block: the hbl DIO pattern.
    assert_quiet("[T1, T2] -> RES2*2");
    // Rows are precise: 4 rows against a 2-row block is a mismatch.
    assert_E4007("[T1, T2, T3, T4] -> RES2*2");
}

#[test]
fn replication__shape_column_stacks_to_MNx1_in_block_order() {
    assert_quiet("[T1, T2, T3, T4] -> [R1, R2]*2");
    assert_E4007("[T1, T2] -> [R1, R2]*2");
    // M=3, N=2 → 6 rows.
    assert_quiet("[T1, T2, T3, T4, T5, T6] -> [R1, R2, R3]*2");
    // M=3, N=3 → 9 rows, not 5.
    assert_E4007("[T1, T2, T3, T4, T5] -> [R1, R2, R3]*3");
}

/// Render a phrase as `Kind[operand, ..]` with endpoint names inline (the
/// r0 encoding-anchor style), so the block order of the copies is readable
/// straight off the parse product.
fn shape(p: &mcc::McPhrase) -> String {
    match p {
        mcc::McPhrase::Endpoint(ep) => match ep {
            mcc::McRef::Name(r) => r.base.get_name().to_string(),
            other => format!("{other:?}"),
        },
        mcc::McPhrase::Multiple(v) => {
            format!("Multiple[{}]", v.iter().map(shape).collect::<Vec<_>>().join(", "))
        }
        mcc::McPhrase::Series(v, d) => {
            format!("Series({d:?})[{}]", v.iter().map(shape).collect::<Vec<_>>().join(", "))
        }
        mcc::McPhrase::Parallel(v) => {
            format!("Parallel[{}]", v.iter().map(shape).collect::<Vec<_>>().join(", "))
        }
        other => format!("{other:?}"),
    }
}

#[test]
fn replication__column_base_stacks_in_block_order_not_interleaved() {
    let _lock = common::lock();
    common::reset();
    let src = format!("{RES2}{PORTS}        [T1, T2, T3, T4] -> [R1, R2]*2\n    }}\n}}\n");
    let u = McURI::from("/mcc/u155-order.mc");
    mcc::mcc_load_from_string(&u, &src);
    let inst = mcc::mcc_build(&McIds::from("main"), &u).expect("build");
    let rendered = inst
        .def
        .funcs
        .find("M")
        .map(|f| f.stmts.iter().map(shape).collect::<Vec<_>>().join("; "))
        .unwrap_or_default();
    // Block order: [R1, R2] stacked after [R1, R2] — never R1, R1, R2, R2.
    assert!(
        rendered.contains("Multiple[Multiple[R1, R2], Multiple[R1, R2]]"),
        "copies must stack in block order; got: {rendered}"
    );
}

#[test]
fn replication__transposed_two_pin_stacks_to_2Nx1() {
    // A two-pin creation is 1×2 by default; transposed it is 2×1, so *2
    // stacks to 2N×1 = 4×1 (lib `RES(10)`, the vec_p25 creation form — a
    // bare class-name creation does not carry a transposable face).
    assert_quiet("[T1, T2, T3, T4] -> RES(10)'*2");
    assert_E4007("[T1, T2] -> RES(10)'*2");
}

#[test]
fn replication__paren_group_evaluates_before_replicating() {
    // (A+B) folds to a 1×2 parallel row first, then *2 stacks to 2×2.
    assert_quiet("[T1, T2] -> (RES2 + RES2)*2");
    assert_E4007("[T1, T2, T3, T4] -> (RES2 + RES2)*2");
}
