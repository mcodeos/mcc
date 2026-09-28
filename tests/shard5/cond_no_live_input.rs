// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E5463 (COND_NO_LIVE_INPUT): a conditional chain whose judges read no
//! parameter and no definition key, and that names an identifier resolving to
//! no declared name, selects the same branch for every instance — the
//! mistyped name surfaces nowhere else, so the compiler must report it
//! (U344). A judge over literals alone (`if (1 == 1)`) is deliberately
//! supported and stays silent; a live judge over a bare word
//! (`if (sel == FAST)`) stays silent.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashSet;

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    common::reset();
    let uri = "/mcc/cond-no-live-input-test.mc".to_string();
    common::load_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn sem_noLive__dead_judge_over_unresolved_ident_reports() {
    let _lock = common::lock();

    // The judge reads no parameter (`sel` is never named) and
    // `undefined_name` resolves to nothing — the chain folds to the else arm
    // silently unless E5463 fires.
    let src = "component SEL(sel = FAST)\n{\n    pins = [1 = P]\n    if (undefined_name == FAST) pins += [2 = Q_FAST]\n    else pins += [3 = Q_SLOW]\n}\nmodule main { io VA; io VB; SEL a; [VA] -> a{1} -> [VB] }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::COND_NO_LIVE_INPUT),
        "E5463 not emitted for a judge over an unresolved ident; got codes: {codes:?}"
    );
}

#[test]
fn sem_noLive__live_judge_over_bare_word_stays_silent() {
    let _lock = common::lock();

    // `FAST` on the right side is the legitimate bare-word face: the judge
    // reads the parameter `sel`, so the branch varies with the binding.
    let src = "component SEL(sel = FAST)\n{\n    pins = [1 = P]\n    if (sel == FAST) pins += [2 = Q_FAST]\n    else pins += [3 = Q_SLOW]\n}\nmodule main { io VA; io VB; SEL a; [VA] -> a{1, 2} -> [VB] }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::COND_NO_LIVE_INPUT),
        "E5463 false positive on a live bare-word judge; got codes: {codes:?}"
    );
}

#[test]
fn sem_noLive__literal_only_judge_stays_silent() {
    let _lock = common::lock();

    // A judge over literals alone is a legitimate constant condition: a
    // definition with no parameters must still apply its branches.
    let src = "component SEL()\n{\n    pins = [1 = P]\n    if (1 == 1) pins += [2 = Q_FAST]\n    else pins += [3 = Q_SLOW]\n}\nmodule main { io VA; io VB; SEL a; [VA] -> a{1, 2} -> [VB] }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::COND_NO_LIVE_INPUT),
        "E5463 false positive on a literal-only judge; got codes: {codes:?}"
    );
}

#[test]
fn sem_noLive__dead_judge_without_defaults_reports_at_instantiation() {
    let _lock = common::lock();

    // No defaulted formal → the parse fold never selects, the chain is stored
    // for instantiation, and the dead judge reports from there (anchored at
    // the declaration site).
    let src = "component SEL()\n{\n    pins = [1 = P]\n    if (undefined_name == FAST) pins += [2 = Q_FAST]\n    else pins += [3 = Q_SLOW]\n}\nmodule main { io VA; io VB; SEL a; [VA] -> a{1} -> [VB] }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::COND_NO_LIVE_INPUT),
        "E5463 not emitted for a dead judge stored to instantiation; got codes: {codes:?}"
    );
}
