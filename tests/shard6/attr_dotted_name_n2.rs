// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for N2 (dotted attribute name resolution, ATTR_DOTTED_NAME_UNRESOLVED
// 5358) against the key registry (`semantic::basic::attr_keys`, the data
// dictionary of first-level attribute keys — contract-design.md §1.7 G6).
//
// N2 used to collect the component's own attribute first-segments as its
// "known keys" set, which always contained the segment under test: the
// predicate was a tautology and the check could never fire. It also reported
// the wrong code (5353 ROLE_EMPTY_BODY). Both are locked here — every branch
// of the predicate is exercised, including the two that must stay silent.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

const CODE: u32 = 5358;

fn diags_for(source: &str) -> Vec<mcc::McDiagnostic> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/attr-dotted-n2.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");
    mcc::mcc_diagnose_all()
        .into_iter()
        .filter(|d| d.code == CODE)
        .collect()
}

#[test]
fn sem_attrdotted__unregistered_key_errors() {
    // The segment under test is the *unknown* one: this is the case the old
    // tautological predicate could not reach.
    let hits = diags_for(
        r#"
component NEEDS
{
    bogus.thing = 7
}

module main
{
    io VDD
    NEEDS n1
}
"#,
    );
    assert_eq!(
        hits.len(),
        1,
        "an unregistered first segment must report E{CODE} exactly once, got: {hits:?}"
    );
    assert!(
        hits[0].msg.contains("bogus"),
        "diagnostic must name the offending key, got: {}",
        hits[0].msg
    );
}

#[test]
fn sem_attrdotted__registered_key_is_silent() {
    // `spec` is a row of the dictionary.
    let hits = diags_for(
        r#"
component LDO(vout::UV.VOLT = 3.3V)
{
    spec.Vout = vout
    spec.Iout_max = 500mA
}

module main
{
    io VDD
    LDO l1
}
"#,
    );
    assert!(
        hits.is_empty(),
        "registered key 'spec' must not report E{CODE}, got: {hits:?}"
    );
}

#[test]
fn sem_attrdotted__component_name_is_silent() {
    // First segment naming the enclosing definition is the other resolution.
    let hits = diags_for(
        r#"
component MODA
{
    MODA.thing = 7
}

module main
{
    io VDD
    MODA m1
}
"#,
    );
    assert!(
        hits.is_empty(),
        "the component's own name must resolve, got: {hits:?}"
    );
}

#[test]
fn sem_attrdotted__reserved_segment_is_silent() {
    // `pins.X` belongs to N7 / N1, not to N2 — reserved words are rows of the
    // same dictionary and must not be re-reported here.
    let hits = diags_for(
        r#"
component SUBPINS
{
    pins = [ 1 = 1 ]
    pins.subcls = [ 2 = 2 ]
}

module main
{
    io VDD
    SUBPINS s1
}
"#,
    );
    assert!(
        hits.is_empty(),
        "a reserved first segment must not report E{CODE}, got: {hits:?}"
    );
}

#[test]
fn sem_attrdotted__single_segment_is_silent() {
    // A plain `key = value` is not a dotted name at all.
    let hits = diags_for(
        r#"
component PLAIN
{
    bogus = 7
}

module main
{
    io VDD
    PLAIN p1
}
"#,
    );
    assert!(
        hits.is_empty(),
        "single-segment attribute names must not report E{CODE}, got: {hits:?}"
    );
}
