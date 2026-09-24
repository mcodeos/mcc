// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The word-family matrix lock (U144, ruling of 2026-09-20).
//!
//! A condition judge between two word families is decided by family first:
//! bare meets bare, quoted meets quoted, and a bare×quoted pair is a
//! diagnostic (5462), never a silent text equality. The `in` face has no
//! single judge to report at, so its cross-family members are skipped in
//! silence. Numeric operands keep the engine's own gate.

use crate::common;

use mcc::{McIds, McURI};

/// One component per matrix cell; `main` instantiates the one under test.
fn source(comp: &str) -> String {
    format!("{comp}\nmodule main\n{{\n    io VDD\n    SEL a\n}}\n")
}

/// Build and return the sorted, deduplicated diagnostic codes.
fn codes(comp: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/cond-family-matrix.mc".to_string();
    mcc::mcc_load_from_string(&uri, &source(comp));
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &uri);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The bare×bare cell: the judge is a text equality, and it is quiet.
#[test]
fn matrix__bare_meets_bare_is_quiet() {
    let comp = "component SEL(sel = FAST)\n{\n    pins = [1 = P]\n    if (sel == FAST) { pins += [2 = Q_FAST] }\n    else { pins += [3 = Q_SLOW] }\n}\n";
    assert!(
        !codes(comp).contains(&mcc::errcodes::COND_FAMILY_MISMATCH),
        "bare against bare is the same family: no family diagnostic"
    );
}

/// The quoted×quoted cell: equally quiet.
#[test]
fn matrix__quoted_meets_quoted_is_quiet() {
    let comp = "component SEL(sel = \"FAST\")\n{\n    pins = [1 = P]\n    if (sel == \"FAST\") { pins += [2 = Q_FAST] }\n    else { pins += [3 = Q_SLOW] }\n}\n";
    assert!(
        !codes(comp).contains(&mcc::errcodes::COND_FAMILY_MISMATCH),
        "quoted against quoted is the same family: no family diagnostic"
    );
}

/// The bare default against the quoted condition: the judge is reported.
#[test]
fn matrix__bare_default_against_quoted_condition_is_reported() {
    let comp = "component SEL(sel = FAST)\n{\n    pins = [1 = P]\n    if (sel == \"FAST\") { pins += [2 = Q_FAST] }\n    else { pins += [3 = Q_SLOW] }\n}\n";
    assert!(
        codes(comp).contains(&mcc::errcodes::COND_FAMILY_MISMATCH),
        "the cross-family judge is a diagnostic, not a silent miss"
    );
}

/// The mirror cell: a quoted default against the bare condition, also reported.
#[test]
fn matrix__quoted_default_against_bare_condition_is_reported() {
    let comp = "component SEL(sel = \"FAST\")\n{\n    pins = [1 = P]\n    if (sel == FAST) { pins += [2 = Q_FAST] }\n    else { pins += [3 = Q_SLOW] }\n}\n";
    assert!(
        codes(comp).contains(&mcc::errcodes::COND_FAMILY_MISMATCH),
        "the mirror cross-family pair is reported the same way"
    );
}

/// The `in` face has no single judge, so a cross-family member is skipped in
/// silence — the miss takes the else branch without a diagnostic.
#[test]
fn matrix__in_face_cross_family_members_stay_silent() {
    let comp = "component SEL(sel = B)\n{\n    pins = [1 = P]\n    if (sel in [\"A\", \"B\", \"C\"]) { pins += [2 = Q_IN] }\n    else { pins += [3 = Q_OUT] }\n}\n";
    assert!(
        !codes(comp).contains(&mcc::errcodes::COND_FAMILY_MISMATCH),
        "the in face skips the other family's members without a diagnostic"
    );
}

/// The numeric face keeps the engine's own gate: a numeric default against a
/// numeric condition is quiet, whatever the unit spelling.
#[test]
fn matrix__numeric_face_keeps_the_engine_gate() {
    let comp = "component SEL(count = 5)\n{\n    pins = [1 = P]\n    if (count == 5) { pins += [2 = Q_FIVE] }\n    else { pins += [3 = Q_OTHER] }\n}\n";
    assert!(
        !codes(comp).contains(&mcc::errcodes::COND_FAMILY_MISMATCH),
        "numeric against numeric never raises the word-family diagnostic"
    );
}
