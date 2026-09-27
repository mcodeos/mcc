// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E2096 (PARSER_JUDGE_INVALID, U337 ruling A): a bare literal in the
//! condition position (`if (1)`) has no judge operator — the judge family
//! has no truth-valued arm. The `mc_judge: mc_literal` grammar arm accepts
//! the form so the clause parses, fires E2096 with the literal's own span,
//! and the semantic layer never selects the chain. Before the arm, the
//! GLR error recovery swallowed the whole if/else clause: the author saw
//! only the generic E2082 clause-skipped code plus an empty-body cascade
//! (E2115/E5103/E5252), and on the netlist export face nothing at all.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashSet;

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    common::reset();
    let uri = "/mcc/bare-literal-cond-test.mc".to_string();
    common::load_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn sem_cond_barelit__bare_literal_condition_reports_2096() {
    let _lock = common::lock();

    let src = "component PB(pinno)\n{\n    if (1) pins = [1 = GND]\n    else pins = [1 = VCC]\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::PARSER_JUDGE_INVALID),
        "E2096 not emitted for a bare-literal condition; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&2082),
        "the generic E2082 clause-skipped code must not replace the precise E2096; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&2115),
        "the empty-body E2115 cascade must not fire — the clause now parses; got codes: {codes:?}"
    );
}

#[test]
fn sem_cond_barelit__judge_form_control_stays_clean() {
    let _lock = common::lock();

    // The readable twin of the sick shape: a judge operator present, no
    // bare-literal code — the arm must only reduce when no operator follows.
    let src = "component PC(pinno)\n{\n    if (pinno == 1) pins = [1 = GND]\n    else pins = [1 = VCC]\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::PARSER_JUDGE_INVALID),
        "E2096 false positive on a readable judge form; got codes: {codes:?}"
    );
}

#[test]
fn sem_cond_barelit__tolerance_continuation_keeps_expression_arm() {
    let _lock = common::lock();

    // The R/R fork the arm introduces is on `±`: a literal that continues
    // into a tolerance expression keeps the expression derivation (earlier
    // rule wins), so the ± judge face must stay free of E2096 — the W1102
    // single-arm warning is that form's own code.
    let src = "component PT(pinno)\n{\n    if (1 ± 0.1%) pins = [1 = GND]\n    else pins = [1 = VCC]\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::PARSER_JUDGE_INVALID),
        "E2096 must not fire when the literal continues into a tolerance; got codes: {codes:?}"
    );
    assert!(
        codes.contains(&2112),
        "W1102 expected for the ± judge form; got codes: {codes:?}"
    );
}
