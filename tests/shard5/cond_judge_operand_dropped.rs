// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E5461 (COND_JUDGE_OPERAND_DROPPED): a judge operand the condition
//! collector cannot name (a call, a form with no reading) used to vanish
//! silently — with fewer than two operands the whole judge was discarded and
//! the if-branch never selected, with no diagnostic explaining why. The drop
//! is now audible (U144 condition face; keyword-constant behavior is locked
//! separately in shard3 `kw_const_condition`).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashSet;

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    common::reset();
    let uri = "/mcc/cond-operand-dropped-test.mc".to_string();
    common::load_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn sem_cond_drop__call_operand_reports_5461() {
    let _lock = common::lock();

    // `F(1)` has no collector reading — the judge is discarded, and the drop
    // must be reported instead of the branch silently going dead.
    let src = "component DROPPED(partno)\n{\n    if (partno == F(1)) package = \"A\"\n    else package = \"B\"\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::COND_JUDGE_OPERAND_DROPPED),
        "E5461 not emitted for an unrecognized judge operand; got codes: {codes:?}"
    );
}

#[test]
fn sem_cond_drop__valid_operands_stay_clean() {
    let _lock = common::lock();

    // Parameter vs literal on both comparison arms — the fixed form.
    let src = "component CLEAN(partno)\n{\n    if (partno == \"A\") package = \"SOIC8\"\n    else if (partno == \"B\") package = \"MSOP\"\n    else package = \"TSSOP\"\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::COND_JUDGE_OPERAND_DROPPED),
        "E5461 false positive on recognized operands; got codes: {codes:?}"
    );
}

#[test]
fn sem_cond_drop__keyword_constant_operand_stays_clean() {
    let _lock = common::lock();

    // A keyword constant (MCAST_CONST) is a recognized operand — the CONST
    // arm sits next to the collector fallthrough and must not be mistaken
    // for a drop (shard3 `kw_const_condition` locks the branch selection).
    let src = "component CONSTC(partno)\n{\n    if (partno == HIGH) package = \"A\"\n    else package = \"B\"\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::COND_JUDGE_OPERAND_DROPPED),
        "E5461 false positive on a keyword-constant operand; got codes: {codes:?}"
    );
}
