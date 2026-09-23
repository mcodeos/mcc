// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! EVAL_ERROR_EXPRESSION (E5416): the library author's `error(msg)` clause is a
//! real diagnostic. A fired error does not block instantiation — the instance
//! is still built, only the diagnostic is reported.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McDiagnostic;

/// Build `src` in a fresh workspace and return every emitted diagnostic.
fn build_diags(src: &str) -> Vec<McDiagnostic> {
    common::reset();
    let uri = "/mcc/error-expression-test.mc".to_string();
    common::load_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all()
}

fn codes(diags: &[McDiagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn sem_errexpr__instance_time_error_fires_5416_with_interpolation() {
    let _lock = common::lock();

    let src = "component ERF1(pin_count::INT)\n{\n    if pin_count in [3, 4]\n        pins = [ 1 = GND, \"a\" ]\n    else\n        error(\"ERF1 unknown \" + pin_count)\n}\nmodule main\n{\n    A1::ERF1(3)\n    A2::ERF1(7)\n}";
    let diags = build_diags(src);
    let codes = codes(&diags);
    let hits: Vec<&McDiagnostic> = diags.iter().filter(|d| d.code == mcc::errcodes::EVAL_ERROR_EXPRESSION).collect();
    assert_eq!(hits.len(), 1, "exactly one fired error expected; got codes: {codes:?}");
    assert!(
        hits[0].msg.contains("ERF1 unknown 7"),
        "message must interpolate the instance argument; got: {}",
        hits[0].msg
    );
}

#[test]
fn sem_errexpr__non_text_message_falls_back() {
    let _lock = common::lock();

    // An operand that resolves to no string value cannot carry a message.
    let src = "component FB(n::INT)\n{\n    if n == 1\n        pins = [ 1 = GND, \"one\" ]\n    else\n        error(HIGH)\n}\nmodule main\n{\n    Z1::FB(5)\n}";
    let diags = build_diags(src);
    let codes = codes(&diags);
    let hits: Vec<&McDiagnostic> = diags.iter().filter(|d| d.code == mcc::errcodes::EVAL_ERROR_EXPRESSION).collect();
    assert_eq!(hits.len(), 1, "the fallback must still fire; got codes: {codes:?}");
    assert_eq!(
        hits[0].msg, "error() clause: message is not a text expression",
        "fallback message expected; got: {}",
        hits[0].msg
    );
}

#[test]
fn sem_errexpr__bare_error_branch_body_parses_without_E2082() {
    let _lock = common::lock();

    // A bare error(...) clause as a branch body is legal; the old grammar
    // rejected it with E2082 (Invalid clause in a body).
    let src = "component ERF2(pin_count::INT)\n{\n    if pin_count == 2\n        pins = [ 1 = GND, \"a\" ]\n    else\n        error(\"plain \" + pin_count)\n}\nmodule main\n{\n    A4::ERF2(9)\n}";
    let diags = build_diags(src);
    let codes = codes(&diags);
    assert!(
        !codes.contains(&2082),
        "bare error branch must not be E2082; got: {codes:?}"
    );
    assert!(
        codes.contains(&mcc::errcodes::EVAL_ERROR_EXPRESSION),
        "the else branch must fire for pin_count=9; got: {codes:?}"
    );
}

#[test]
fn sem_errexpr__guard_branch_does_not_fire_and_no_E5451_noise() {
    let _lock = common::lock();

    // When the selected branch is a pins branch, no error fires and the
    // error-carrying branches are skipped by the pin coverage warnings.
    let src = "component ERF1(pin_count::INT)\n{\n    if pin_count in [3, 4]\n        pins = [ 1 = GND, \"a\" ]\n    else\n        error(\"ERF1 unknown \" + pin_count)\n}\nmodule main\n{\n    A1::ERF1(3)\n}";
    let diags = build_diags(src);
    let codes = codes(&diags);
    assert!(
        !codes.contains(&mcc::errcodes::EVAL_ERROR_EXPRESSION),
        "no error for a legal argument; got: {codes:?}"
    );
    assert!(
        !codes.contains(&5451),
        "no E5451 on an error-carrying chain; got: {codes:?}"
    );
}
