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

#[test]
fn sem_attrdotted__dotted_name_prefix_is_silent() {
    // U194: a dotted component's multi-point attributes spell the full name
    // as their leading segments. First-segment equality against the full
    // name string (`TTL` != `TTL.D.SN74LVC1G175`) could never hold, so every
    // such attribute was an error.
    let hits = diags_for(
        r#"
component BUS.DRIVER.DW01
{
    BUS.DRIVER.DW01.doc_features = "octal driver"
    BUS.DRIVER.DW01.doc_overall = "auto grade"
}

module main
{
    io VDD
    BUS.DRIVER.DW01 u1
}
"#,
    );
    assert!(
        hits.is_empty(),
        "attributes prefixed by the component's full dotted name must not report \
         E{CODE}, got: {hits:?}"
    );
}

#[test]
fn sem_attrdotted__partial_name_prefix_still_errors() {
    // The name witness is the *full* segment sequence: a leading run that
    // stops short of the component's own last segment names a family, not
    // this component, and stays an error (nothing else resolves it).
    let hits = diags_for(
        r#"
component BUS.DRIVER.DW01
{
    BUS.DRIVER.doc_features = "octal driver"
}

module main
{
    io VDD
    BUS.DRIVER.DW01 u1
}
"#,
    );
    assert_eq!(
        hits.len(),
        1,
        "a family-prefix attribute on a dotted component must report E{CODE} \
         exactly once, got: {hits:?}"
    );
}

#[test]
fn sem_attrdotted__variant_clone_of_base_attrs_is_silent() {
    // U194 amplification: a materialized variant rides the base's cloned
    // attributes (adoption.rs materialize_variant). The base resolves its own
    // prefixed attrs by name; the clone resolves them through the declared
    // variant base — including when the variant name does not extend the
    // base name (the mcpub `USB.HUM011D_5_S : USB.MINIB` shape). Old
    // behavior: one error on the base plus one per variant clone.
    let hits = diags_for(
        r#"
abstract component DEV.BASE
{
    DEV.BASE.doc_features = "base doc"
}

component DEV.BASE.X1 : DEV.BASE
{
    partno = "X1"
}

component OTHER.SIBLING : DEV.BASE
{
    partno = "S1"
}

module main
{
    io VDD
    DEV.BASE.X1 u1
    OTHER.SIBLING u2
}
"#,
    );
    assert!(
        hits.is_empty(),
        "base name-prefixed attrs must stay silent on the base and on every \
         variant clone, got: {hits:?}"
    );
}

#[test]
fn sem_attrdotted__variant_own_bad_attr_still_errors_once() {
    // The clone witness covers the base's attrs, not the variant's own: a
    // variant-declared unregistered dotted name is its own error, reported
    // once on the variant — not per definition in the binding pair.
    let hits = diags_for(
        r#"
abstract component DEV.BASE
{
    DEV.BASE.doc_features = "base doc"
}

component DEV.BASE.X1 : DEV.BASE
{
    partno = "X1"
    bogus.thing = 7
}

module main
{
    io VDD
    DEV.BASE.X1 u1
}
"#,
    );
    assert_eq!(
        hits.len(),
        1,
        "a variant's own unregistered dotted attr must report E{CODE} exactly \
         once, got: {hits:?}"
    );
}
