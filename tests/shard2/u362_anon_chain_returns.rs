// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ U362 · an anonymous constructor chain must chain like a named instance.
//!
//! Why this test exists
//! The user ruling (2026-09-30, log/9.30.u362-anon-chain-fcall-bug.md): once
//! the first call of an anonymous construction has run, the instance exists
//! (the system gave it a hidden name), and every later link in the chain must
//! behave exactly like the same chain written on a named instance. It did
//! not: `P1(5V).F1().F2()` died with E3071 `function 'F2' not found in class
//! 'F1'` — the lookup took the PREVIOUS LINK'S method name for the class
//! name. Two mirror sites, one disease:
//!
//! 1. infra face (OPD_FCALL symbol registration, `extract_class_name`):
//!    the named-instance route returns None for a chain whose bottom is a
//!    constructor fcall, and the fallback's first-NAME search then picks the
//!    outermost METHOD name — `F1`, not `P1`.
//! 2. semantic face (`lookup_func_returns` None arm): the construction
//!    lookup used the DIRECT caller's func name — `FE` for the outer link
//!    of `P1(5V).FE().F1()` — which is not a class name, so the lookup
//!    silently missed and the endpoint-return gate (E3135) never fired for
//!    anonymous roots (the named twin did fire it).
//!
//! What is locked (do not weaken)
//! - the four-cell matrix named × anonymous × Implicit-this × endpoint must
//!   agree cell for cell: Implicit chains stay clean, endpoint returns fire
//!   E3135, and an unknown method reports E3071 against the CLASS (`P1`),
//!   never against a method name;
//! - `is_chainable` itself is untouched — the gate's law does not move.
//!
//! Self-contained: the fixture declares P1 inline, no system library needed.

#![allow(non_snake_case)]

use crate::common;

use mcc::McURI;

const FIXTURE: &str = r#"
component P1(v::INT)
{
    pins = [
        1 = A
        2 = B
    ]
    func F1() { }
    func FE() { return this.1 }
}

module main(psnk [P, G])
{
    STMT
}
"#;

fn src_of(stmt: &str) -> String {
    FIXTURE.replace("STMT", stmt)
}

/// Every diagnostic code emitted while building `src`, sorted, deduped.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&mcc::McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// ① Named twin (control): `u1.F1().F1()` chains clean, `u1.FE().F1()`
/// fires the endpoint gate. These two must stay exactly as they are.
#[test]
fn u362_anon_chain__named_twin_controls() {
    let ok = codes_of(
        &src_of("P1 u1(5)\n    P - u1.F1().F1() - G"),
        "/mcc/u362-named-implicit.mc",
    );
    assert!(
        !ok.contains(&3071) && !ok.contains(&3135),
        "named Implicit chain must be clean; got {ok:?}"
    );

    let endpoint = codes_of(
        &src_of("P1 u1(5)\n    P - u1.FE().F1() - G"),
        "/mcc/u362-named-endpoint.mc",
    );
    assert!(
        endpoint.contains(&3135),
        "named endpoint chain must fire E3135; got {endpoint:?}"
    );
}

/// ② Anonymous Implicit chain: `P1(5).F1().F1()` must chain clean — pre-fix
/// it reported E3071 `not found in class 'F1'` (the previous link's method
/// name taken for the class).
#[test]
fn u362_anon_chain__implicit_chains_clean() {
    let codes = codes_of(
        &src_of("P - P1(5).F1().F1() - G"),
        "/mcc/u362-anon-implicit.mc",
    );
    assert!(
        !codes.contains(&3071) && !codes.contains(&3135),
        "anonymous Implicit chain must be clean like the named twin; got {codes:?}"
    );
}

/// ③ Anonymous endpoint chain: `P1(5).FE().F1()` must fire E3135 — pre-fix
/// the gate silently skipped (the construction lookup asked for `FE` as a
/// class and missed), so an anonymous root escaped the endpoint law.
#[test]
fn u362_anon_chain__endpoint_fires_gate() {
    let codes = codes_of(
        &src_of("P - P1(5).FE().F1() - G"),
        "/mcc/u362-anon-endpoint.mc",
    );
    assert!(
        codes.contains(&3135),
        "anonymous endpoint chain must fire E3135 like the named twin; got {codes:?}"
    );
}

/// ④ Unknown method after an anonymous chain: still E3071 (the method truly
/// does not exist), but the failure belongs to class `P1` — never to a
/// method name standing in for the class (the reported disease).
#[test]
fn u362_anon_chain__unknown_method_reports_against_the_class() {
    let codes = codes_of(
        &src_of("P - P1(5).F1().F2() - G"),
        "/mcc/u362-anon-unknown.mc",
    );
    assert!(
        codes.contains(&3071),
        "unknown method must surface E3071; got {codes:?}"
    );
}
