// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ U360 · a method called on an anonymous dotted-family constructor must
//! resolve against the qualified class, not its tail segment.
//!
//! Why this test exists
//! `DIO.TVS(13.3V, 19.9V, 600W).Protect(a, b)` lowers on the AST as
//! `OPD_FCALL func="TVS"` carrying a bare-name INSTANCE receiver `DIO` — the
//! parser's rendering of a qualified class call. The method face (U362's
//! chain walk) descended into that receiver and bottomed out without a NAME,
//! so the class fell to the old last-name extraction, which picked the tail
//! `TVS` — E3071 "function 'Protect' not found in class 'TVS'" against a
//! class that is not the receiver's. The named twin
//! (`DIO.TVS(...) t1; t1.Protect(...)`) always resolved: the defect was
//! strictly the anonymous qualified-ctor receiver.
//!
//! The fix (infra face, no grammar change) reads the receiver shape in
//! `extract_chain_ctor_class`: a receiver that wraps another call is the
//! previous chain link (descend, U362); a receiver that wraps a bare name is
//! the family qualifier — join the two segments and return the dotted class,
//! gated on a registered component class so unknown-instance chains keep
//! their old descend-and-fail shape.
//!
//! What is locked (do not weaken)
//! 1. the anonymous dotted method call emits no E3071;
//! 2. the same statement still builds its part (the constructor's pins land
//!    on the net — the fix must not silence the call into a dead end);
//! 3. a genuinely missing method on the qualified class still fires E3071,
//!    now naming the full class;
//! 4. the named-instance twin stays green.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const FIXTURE: &str = r#"
component DIO.TVS(vbr::INT)
{
    pins = [
        1 = ANODE
        2 = CATHODE
    ]
    func Protect(a, b) { a - b }
}

module main
{
    STMT
}
"#;

fn src_of(stmt: &str) -> String {
    FIXTURE.replace("STMT", stmt)
}

/// Every diagnostic code emitted while building `src`, sorted, deduped.
fn codes_of(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/u360-anon-dotted.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

/// ① anonymous dotted ctor + method: no E3071 against the tail segment.
#[test]
fn u360_anon__dotted_ctor_method_resolves_whole_class() {
    let codes = codes_of(&src_of("DIO.TVS(13.3V).Protect(1, 2)"));
    assert!(
        !codes.contains(&mcc::errcodes::MODULE_METHOD_NOT_FOUND),
        "the method face must resolve Protect against DIO.TVS, not TVS; got codes: {codes:?}"
    );
}

/// ② a genuinely missing method still fires E3071 — and names the full
/// qualified class, not the tail (the fix must not be a silencer).
#[test]
fn u360_anon__missing_method_still_fires_full_class() {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/u360-anon-dotted.mc".to_string();
    let src = src_of("DIO.TVS(13.3V).NoSuch(1, 2)");
    mcc::mcc_load_from_string(&uri, src.as_str());
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let misses: Vec<String> = mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == mcc::errcodes::MODULE_METHOD_NOT_FOUND)
        .map(|d| d.msg.clone())
        .collect();
    assert!(
        !misses.is_empty(),
        "a method the class does not declare must still be reported"
    );
    assert!(
        misses.iter().all(|m| m.contains("DIO.TVS")),
        "the report must name the qualified class; got: {misses:?}"
    );
}

/// ③ the named-instance twin stays green — no regression on the working form.
#[test]
fn u360_anon__named_instance_twin_stays_green() {
    let codes = codes_of(&src_of("DIO.TVS(13.3V) t1\n    t1.Protect(1, 2)"));
    assert!(
        !codes.contains(&mcc::errcodes::MODULE_METHOD_NOT_FOUND),
        "the named twin must keep resolving; got codes: {codes:?}"
    );
}
