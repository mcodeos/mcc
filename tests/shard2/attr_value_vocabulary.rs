// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for the value vocabulary (ATTR_VALUE_NOT_IN_VOCABULARY 5360, parse-time,
// contract-design.md §1.8).
//
// The identity-axis keys (`role`, `class`, `nature`, `noise`, `exposed`,
// `bind_role`), the protection gate (`protect`) and the flag `star` state a
// *classification word*, and the words are the ledger's closed sets (the word
// column of `spec/07-attrs.md` §3.1). Their
// readers see a word or nothing, so a misspelling used to read as "no
// declaration at all" and every rule on that axis went quiet. The check sits in
// `McAttributes::parse`, the funnel every source-fed list goes through, so a
// body sentence and a row's trailing `@attr…` are judged alike.
//
// Every branch below has at least two members, one that fires and one that
// stays silent.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

const CODE: u32 = 5360;

fn diags_for(source: &str) -> Vec<mcc::McDiagnostic> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/attr-value-vocab.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");
    mcc::mcc_diagnose_all()
        .into_iter()
        .filter(|d| d.code == CODE)
        .collect()
}

/// Assert the fixture reports the code exactly once and names `key`.
fn assert_reports(source: &str, key: &str, what: &str) {
    let hits = diags_for(source);
    assert_eq!(
        hits.len(),
        1,
        "{what}: expected exactly one E{CODE} for '{key}', got: {hits:?}"
    );
    assert!(
        hits[0].msg.contains(key),
        "{what}: diagnostic must name '{key}', got: {}",
        hits[0].msg
    );
}

fn assert_silent(source: &str, what: &str) {
    let hits = diags_for(source);
    assert!(
        hits.is_empty(),
        "{what}: expected no E{CODE}, got: {hits:?}"
    );
}

// 1. A word outside the set, on the tail of a row

#[test]
fn sem_attrvocab__unknown_role_word_reports() {
    // `mate` is not one of the five identity words: the conduit would read as
    // having no role at all, which is how the misspelling stays invisible today.
    assert_reports(
        r#"
module main
{
    conduit GND @role(mate)
    io VDD
}
"#,
        "role",
        "conduit tail: @role(mate)",
    );
    // The reference word of the same row is judged too — one report per key.
    assert_reports(
        r#"
module main
{
    out EARTH @bind_role(ground)
    io VDD
}
"#,
        "bind_role",
        "port row: @bind_role(ground) instead of earth",
    );
}

#[test]
fn sem_attrvocab__unknown_face_word_reports() {
    assert_reports(
        r#"
module main
{
    domain DVDD @class(anlog) { rail [VDD_3V3, GND]::DC(3.3V) }
    io VDD
}
"#,
        "class",
        "domain tail: @class(anlog)",
    );
    assert_reports(
        r#"
module main
{
    domain DAUD @noise(noizy) { rail [AVDD, AGND]::DC(3.3V) }
    io VDD
}
"#,
        "noise",
        "domain tail: @noise(noizy)",
    );
}

#[test]
fn sem_attrvocab__words_are_exact_no_case_folding() {
    // §1.8: the words are compared exactly. `AC` is not `ac` — the axis would
    // otherwise read as unnamed.
    assert_reports(
        r#"
module main
{
    domain DMAINS @nature(AC) { rail [L, N]::AC(230V, 50Hz) }
    io VDD
}
"#,
        "nature",
        "domain tail: @nature(AC) in capitals",
    );
    assert_reports(
        r#"
module main
{
    domain DLOW @class(Analog) { rail [AVDD, AGND]::DC(3.3V) }
    io VDD
}
"#,
        "class",
        "domain tail: @class(Analog) in capitals",
    );
}

#[test]
fn sem_attrvocab__exposed_surge_words_report() {
    assert_reports(
        r#"
module main
{
    io USB_DM @exposed(esd_touch)
    io VDD
}
"#,
        "exposed",
        "port row: @exposed(esd_touch)",
    );
    assert_reports(
        r#"
module main
{
    io VBUS @exposed(esd)
    io VDD
}
"#,
        "exposed",
        "port row: @exposed(esd), a prefix rather than a word",
    );
}

// 2. The canon's words stay silent, on both write faces

#[test]
fn sem_attrvocab__canonical_words_are_silent() {
    assert_silent(
        r#"
module main
{
    conduit GND @role(main)
    conduit CHASSIS @role(earth) @star
    conduit ISO @role(isolated) @bind_role(isolated)
    domain DVDD @class(digital) @noise(noisy) { rail [VDD_3V3, GND]::DC(3.3V) }
    domain DAUD @class(analog) @noise(sensitive) { rail [AVDD, AGND]::DC(3.3V) }
    domain DMAINS @nature(ac) { rail [L, N]::AC(230V, 50Hz) }
    io USB_DM @exposed(esd_contact)
    io VBUS @exposed(esd_air)
    out EARTH @bind_role(earth)
    io VDD
}
"#,
        "the canon's own words",
    );
}

#[test]
fn sem_attrvocab__body_sentence_face_is_judged_too() {
    // Ruling ① of the power-quality axis writes the same key in a definition
    // body (`noise = quiet`), so the two faces share one vocabulary.
    assert_silent(
        r#"
component QUIETPART
{
    noise = quiet
}

module main
{
    io VDD
    QUIETPART q1
}
"#,
        "component body: noise = quiet",
    );
    assert_reports(
        r#"
component QUIETPART2
{
    noise = queit
}

module main
{
    io VDD
    QUIETPART2 q2
}
"#,
        "noise",
        "component body: noise = queit",
    );
}

#[test]
fn sem_attrvocab__protect_gate_words() {
    assert_silent(
        r#"
component FUSE
{
    protect = series
    pins = [ 1 = A, 2 = B ]
    spec.resistance = 1mOhm
}

module main
{
    io VDD
    FUSE f1
}
"#,
        "component body: protect = series",
    );
    assert_reports(
        r#"
component FUSE2
{
    protect = serise
    pins = [ 1 = A, 2 = B ]
    spec.resistance = 1mOhm
}

module main
{
    io VDD
    FUSE2 f2
}
"#,
        "protect",
        "component body: protect = serise",
    );
}

// 3. A set-valued key with no value at all

#[test]
fn sem_attrvocab__valueless_words_key_reports() {
    // A bare `@role` claims nothing: the declaration is there, the word is not.
    assert_reports(
        r#"
module main
{
    conduit GND @role
    io VDD
}
"#,
        "role",
        "conduit tail: @role with no value",
    );
    assert_reports(
        r#"
module main
{
    domain DVDD @class { rail [VDD_3V3, GND]::DC(3.3V) }
    io VDD
}
"#,
        "class",
        "domain tail: @class with no value",
    );
}

// 4. The flag: presence is the declaration

#[test]
fn sem_attrvocab__flag_takes_no_value() {
    // `@star` is live by being written; a value on it is the error.
    assert_reports(
        r#"
module main
{
    conduit GND @role(main) @star(true)
    io VDD
}
"#,
        "star",
        "conduit tail: @star(true)",
    );
    assert_silent(
        r#"
module main
{
    conduit GND @role(main) @star
    io VDD
}
"#,
        "conduit tail: @star",
    );
}

// 5. Boundaries: a key with no closed set is not judged

#[test]
fn sem_attrvocab__open_vocabulary_keys_are_silent() {
    // `@return` names a conduit — a reference, not a word from a set.
    assert_silent(
        r#"
module main
{
    out MIC @return(GNDA)
    io VDD
}
"#,
        "port row: @return(GNDA)",
    );
    // A key the ledger does not register carries no registered word set, so no
    // word is outside anything (the ledger's open vocabulary).
    assert_silent(
        r#"
component COIL
{
    coil_voltage = 5V
    spec.Rdson = 20mOhm
}

module main
{
    io VDD
    COIL c1
}
"#,
        "component body: unregistered keys",
    );
    assert_silent(
        r#"
component TOPO
{
    topology = "point-to-point"
}

module main
{
    io VDD
    TOPO t1
}
"#,
        "component body: an unregistered word key",
    );
}

#[test]
fn sem_attrvocab__dotted_key_is_not_the_registered_key() {
    // Keys are looked up whole: `class.foo` is not `class`, so the row's
    // vocabulary does not answer for it (the same read as every other column).
    assert_silent(
        r#"
component DOTTED
{
    class.foo = 1
}

module main
{
    io VDD
    DOTTED d1
}
"#,
        "component body: class.foo",
    );
    // The pi-row face is judged by the same key: a pin row's `@class` is one
    // declaration of the same `class` key (U42).
    assert_reports(
        r#"
component PINCLASS
{
    pins = [ 1 = A @class(anlog) ]
}

module main
{
    io VDD
    PINCLASS p1
}
"#,
        "class",
        "pin row: @class(anlog)",
    );
}
