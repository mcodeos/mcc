// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E5460 (COND_DUPLICATE): a later if/else-if branch repeating an earlier
//! branch's condition verbatim is dead code — the earlier branch already
//! selects that case — so the compiler must warn.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::collections::HashSet;

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    common::reset();
    let uri = "/mcc/cond-duplicate-test.mc".to_string();
    common::load_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn sem_dupcond__duplicate_condition_in_chain_warns() {
    let _lock = common::lock();

    // Branch 2 repeats branch 1's condition → dead code.
    let src = "component DUP(partno)\n{\n    if (partno == \"A\") package = \"SOIC8\"\n    else if (partno == \"A\") package = \"MSOP\"\n    else if (partno == \"B\") package = \"TSSOP\"\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::COND_DUPLICATE),
        "E5460 not emitted for a duplicate condition; got codes: {codes:?}"
    );
}

#[test]
fn sem_dupcond__distinct_conditions_do_not_warn() {
    let _lock = common::lock();

    // All three branches are mutually exclusive → no dead code.
    let src = "component DUP(partno)\n{\n    if (partno == \"A\") package = \"SOIC8\"\n    else if (partno == \"B\") package = \"MSOP\"\n    else if (partno == \"C\") package = \"TSSOP\"\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::COND_DUPLICATE),
        "E5460 false positive on distinct conditions; got codes: {codes:?}"
    );
}
