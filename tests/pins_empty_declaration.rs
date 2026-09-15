// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test for the empty-pins cascade (status-design.md §2.7): an empty
// `pins` list is one self-sufficient diagnostic. The grammar reports
// PARSER_EMPTY_PINS (2116) at the opening bracket, and the semantic layer must
// not repeat the same fact through its childless-node fallback (1054, whose
// raise site is gone).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{DiagnosticLevel, McIds};

/// The semantic fallback code, kept numeric: it must never appear in output.
const RETIRED_FALLBACK: u32 = 1054;

fn diagnose(src: &str, uri: &str) -> Vec<(u32, DiagnosticLevel)> {
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri.to_string());
    mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.level))
        .collect()
}

#[test]
fn sem_pinsempty__one_error_without_cascade() {
    let _lock = common::lock();

    let diags = diagnose(
        "component C { pins = [] }\nmodule main { io VDD }",
        "/mcc/pins-empty.mc",
    );
    assert!(
        diags.contains(&(mcc::errcodes::PARSER_EMPTY_PINS, DiagnosticLevel::Error)),
        "an empty pins list must be one error-level diagnostic; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == RETIRED_FALLBACK),
        "the childless-node fallback must not repeat the parser's fact; got {diags:?}"
    );
}

#[test]
fn sem_pinsempty__plus_equal_keeps_only_specific_codes() {
    let _lock = common::lock();

    let diags = diagnose(
        "component C { pins += [] }\nmodule main { io VDD }",
        "/mcc/pins-empty-add.mc",
    );
    assert!(
        diags.contains(&(
            mcc::errcodes::PINS_PLUS_WITHOUT_BASE,
            DiagnosticLevel::Error
        )),
        "`pins += []` without a base must still report PINS_PLUS_WITHOUT_BASE; got {diags:?}"
    );
    assert!(
        diags.contains(&(mcc::errcodes::PARSER_EMPTY_PINS, DiagnosticLevel::Error)),
        "`pins += []` must report the empty list once; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == RETIRED_FALLBACK),
        "the childless-node fallback must not repeat the parser's fact; got {diags:?}"
    );
}
