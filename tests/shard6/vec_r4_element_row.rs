// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! One written element of an `[...]` list is one row (vec-arch.md §4.1.1).
//!
//! A scalar-declared port presents an empty (unknown-width) port view at
//! Pass1, which is what lets shape-by-use (vec-dianlu.md §8.9.6.3) survive to
//! Pass2. That is the right answer for a **whole operand** — `X -> UART0` must
//! stay open until Pass2 upgrades the port — but not for a **list element**:
//! R4 stacks one row per written element, whatever its declaration pinned
//! down. Letting the empty element drop out of the merge made the row count
//! depend on how many elements happened to have a *visible* width, so a lone
//! declared `V5V` turned the legal `[V5V, GND] -> LDO{vin | vout}` into a 1-row
//! vs 2-row `SeriesRowsMismatch` (E4007 + E3132), while the very same
//! statement with an undeclared `V5V` passed.
//!
//! The R4 width gate already read such an element as single-column by
//! declaration (`column_kind` → E2907 gate); these tests hold the row-count
//! view to the same element table.

use crate::common;

use mcc::{McIds, McURI};

/// Build `top` from `src` and return the emitted diagnostic codes (sorted).
fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/r4-element-row.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn rejects_shape(codes: &[u32]) -> bool {
    codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH)
        || codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED)
}

/// A declared `A` + `[A, B] -> [C, D]`: both sides stack two rows, so the
/// pair is legal. Declaring `A` must not shrink its side to one row.
#[test]
fn declared_element_keeps_its_row_on_the_left() {
    let codes = build_codes("module top(A) {\n    [A, B] -> [C, D]\n}\n");
    assert!(
        !rejects_shape(&codes),
        "a declared `A` + `[A, B] -> [C, D]` is a legal 2-row pair; got codes: {codes:?}"
    );
}

/// The symmetric case — the declared name written on the **right** list.
#[test]
fn declared_element_keeps_its_row_on_the_right() {
    let codes = build_codes("module top(C) {\n    [A, B] -> [C, D]\n}\n");
    assert!(
        !rejects_shape(&codes),
        "a declared `C` + `[A, B] -> [C, D]` is a legal 2-row pair; got codes: {codes:?}"
    );
}

/// All four declared: the rows must still be counted (2 vs 2), not dropped to
/// an unknown-width wildcard on both sides.
#[test]
fn all_declared_elements_are_still_counted() {
    let codes = build_codes("module top(A, B, C, D) {\n    [A, B] -> [C, D]\n}\n");
    assert!(
        !rejects_shape(&codes),
        "four declared names still stack to a legal 2-row pair; got codes: {codes:?}"
    );
}

/// A declared element reads exactly like a bare name on the shape face:
/// `[A, B]` is two rows against a single point, so the mismatch is reported —
/// the same E4007 the undeclared spelling has always produced. Declaration is
/// not a shape.
#[test]
fn declared_element_reads_like_a_bare_name() {
    let declared = build_codes("module top(A) {\n    [A, B] -> C\n}\n");
    let bare = build_codes("module top {\n    [A, B] -> C\n}\n");
    assert!(
        rejects_shape(&declared),
        "a declared `A` + `[A, B] -> C` is 2 rows against 1 and must be rejected; got: {declared:?}"
    );
    assert!(
        rejects_shape(&bare),
        "the undeclared spelling has always been rejected; got: {bare:?}"
    );
    // The declared spelling is a header row, so `top` also carries an unused
    // port (E5162) — a module-port face diagnostic, orthogonal to the list's
    // row counting. Beyond that one extra the two spellings agree.
    let mut extra = declared.clone();
    for code in &bare {
        if let Some(i) = extra.iter().position(|c| c == code) {
            extra.remove(i);
        }
    }
    assert_eq!(
        extra,
        vec![mcc::errcodes::MODULE_PORT_UNUSED],
        "declaring `A` must not change the list's diagnostics beyond the unused-port face"
    );
}

/// Two declared names against one declared name: 2 rows vs 1 — rejected, and
/// not silently passed as an unmeasurable wildcard.
#[test]
fn declared_lists_of_unequal_width_are_rejected() {
    let codes = build_codes("module top(A, B, C) {\n    [A, B] -> [C]\n}\n");
    assert!(
        rejects_shape(&codes),
        "`[A, B] -> [C]` is 2 rows against 1 and must be rejected; got: {codes:?}"
    );
}
