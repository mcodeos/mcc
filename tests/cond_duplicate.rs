// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E5460 (COND_DUPLICATE): a later if/else-if branch repeating an earlier
//! branch's condition verbatim is dead code — the earlier branch already
//! selects that case — so the compiler must warn.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/cond-duplicate-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn duplicate_condition_in_chain_warns() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    // Branch 2 repeats branch 1's condition → dead code.
    let src = "component DUP(partno)\n{\n    if (partno == \"A\") package = \"SOIC8\"\n    else if (partno == \"A\") package = \"MSOP\"\n    else if (partno == \"B\") package = \"TSSOP\"\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::COND_DUPLICATE),
        "E5460 not emitted for a duplicate condition; got codes: {codes:?}"
    );

    drop(lock);
}

#[test]
fn distinct_conditions_do_not_warn() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    // All three branches are mutually exclusive → no dead code.
    let src = "component DUP(partno)\n{\n    if (partno == \"A\") package = \"SOIC8\"\n    else if (partno == \"B\") package = \"MSOP\"\n    else if (partno == \"C\") package = \"TSSOP\"\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::COND_DUPLICATE),
        "E5460 false positive on distinct conditions; got codes: {codes:?}"
    );

    drop(lock);
}
