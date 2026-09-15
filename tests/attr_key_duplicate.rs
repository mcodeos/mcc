// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for U43 — one attribute key declared twice in one attribute list
// (ATTR_KEY_DUPLICATE 5359, parse-time).
//
// One site is one attribute list: the trailing `@attr…` run of a row, or a
// definition body (which is a single list). The predicate sits in
// `McAttributes::parse`, the funnel every source-fed list goes through, so a
// row, a port, a connection tail and a body are all judged alike. The read is
// value-blind (a key written twice is reported even when both values agree)
// and it compares the whole dotted key, so `spec.sub1` is not a repetition of
// `spec` (U43 rulings 3, 4, 7).
//
// This file also locks the two boundaries the rule deliberately does not
// cross: nested `[...]` value tables (5267 owns those) and the same pin
// carrying a key on two different rows (`attach_row_attrs` is first-wins, not
// an error). Every branch below has at least two members, one that fires and
// one that stays silent.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::McIds;

const CODE: u32 = 5359;

fn diags_for(source: &str) -> Vec<mcc::McDiagnostic> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/attr-key-dup.mc".to_string();
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

// 1. Unregistered key, conflicting values

#[test]
fn sem_attrdup__unregistered_key_conflicting_values_reports() {
    // An unregistered key defaults to single-valued, so two declarations of it
    // in one list are a duplicate whatever the values say.
    assert_reports(
        r#"
component CLASSY
{
    pins = [ 1 = A @class(analog) @class(digital) ]
}

module main
{
    io VDD
    CLASSY c1
}
"#,
        "class",
        "pin row: @class twice with different values",
    );
    assert_reports(
        r#"
component NOISY
{
    pins = [ 1 = A @noise(flicker) @noise(burst) ]
}

module main
{
    io VDD
    NOISY n1
}
"#,
        "noise",
        "pin row: @noise twice with different values",
    );
}

// 2. Unregistered key, same value twice

#[test]
fn sem_attrdup__same_value_twice_still_reports() {
    // Value-blind (ruling 4, borrowing 5267's reading): the second declaration
    // is a second declaration even when it repeats the first — a reader that
    // takes the last one and a reader that takes the first must not be able to
    // disagree silently.
    assert_reports(
        r#"
component CLASSY2
{
    pins = [ 1 = A @class(analog) @class(analog) ]
}

module main
{
    io VDD
    CLASSY2 c1
}
"#,
        "class",
        "pin row: @class twice with the same value",
    );
    assert_reports(
        r#"
component NOISY2
{
    pins = [ 1 = A @noise(sensitive) @noise(sensitive) ]
}

module main
{
    io VDD
    NOISY2 n1
}
"#,
        "noise",
        "pin row: @noise twice with the same value",
    );
}

// 3. Registered keys are not exempt

#[test]
fn sem_attrdup__registered_keys_report_too() {
    // The registry's third column is the only exemption point, and today no
    // row claims the repeating arity — a reserved key (`role`) and a
    // registered voltage key (`vdd`) are both single-valued.
    assert_reports(
        r#"
module main
{
    conduit GND @role(main) @role(quiet)
    io VDD
}
"#,
        "role",
        "conduit row: @role twice",
    );
    assert_reports(
        r#"
component SUPPLIED
{
    vdd = 3.3V
    vdd = 5V
}

module main
{
    io VDD
    SUPPLIED s1
}
"#,
        "vdd",
        "body: a registered voltage key twice",
    );
}

// 4. Dotted keys are compared whole (ruling 7)

#[test]
fn sem_attrdup__dotted_keys_are_distinct() {
    // `spec.a` and `spec.b` are two keys, not two declarations of `spec`.
    assert_silent(
        r#"
component SPECS
{
    spec.a = 1
    spec.b = 2
}

module main
{
    io VDD
    SPECS s1
}
"#,
        "body: spec.a and spec.b",
    );
    // Nor is a sub-key a repetition of its namespace table.
    assert_silent(
        r#"
component SPECS2
{
    spec = [ voltage = 5V ]
    spec.sub1 = 1
}

module main
{
    io VDD
    SPECS2 s2
}
"#,
        "body: the spec table plus spec.sub1",
    );
}

#[test]
fn sem_attrdup__repeated_subkey_reports() {
    // The same sub-key twice *is* a duplicate, and the message names the whole
    // dotted key rather than its first segment.
    assert_reports(
        r#"
component SPECS3
{
    spec.sub1 = 1
    spec.sub1 = 2
}

module main
{
    io VDD
    SPECS3 s3
}
"#,
        "spec.sub1",
        "body: spec.sub1 twice",
    );
    assert_reports(
        r#"
component SPECS4
{
    spec.a = 1
    spec.a = 1
}

module main
{
    io VDD
    SPECS4 s4
}
"#,
        "spec.a",
        "body: spec.a twice with the same value",
    );
}

// 5. Distinct keys on one row stay silent

#[test]
fn sem_attrdup__distinct_keys_on_one_row_are_silent() {
    assert_silent(
        r#"
component MIXED
{
    pins = [ 1 = A @class(analog) @noise(sensitive) ]
}

module main
{
    io VDD
    MIXED m1
}
"#,
        "pin row: class + noise",
    );
    assert_silent(
        r#"
module main
{
    out EARTH @nature(ac) @bind_role(earth)
    io VDD
}
"#,
        "port row: nature + bind_role",
    );
}

// 6. One rule over every source-fed list

#[test]
fn sem_attrdup__component_body() {
    assert_reports(
        r#"
component BODY1
{
    name = "first"
    name = "second"
}

module main
{
    io VDD
    BODY1 b1
}
"#,
        "name",
        "component body: name twice",
    );
    assert_silent(
        r#"
component BODY2
{
    name = "first"
    description = "second"
}

module main
{
    io VDD
    BODY2 b2
}
"#,
        "component body: name + description",
    );
}

#[test]
fn sem_attrdup__interface_body() {
    assert_reports(
        r#"
interface IFDUPD(role)
{
    topology = "point-to-point"
    topology = "multi-point"
}

module main
{
    io VDD
}
"#,
        "topology",
        "interface body: topology twice",
    );
    assert_silent(
        r#"
interface IFUPCLEAN(role)
{
    topology = "multi-point"
    mode = ["half duplex"]
}

module main
{
    io VDD
}
"#,
        "interface body: topology + mode",
    );
}

#[test]
fn sem_attrdup__static_pin_row() {
    assert_reports(
        r#"
component PINROW1
{
    pins = [ 1 = A @class(analog) @class(digital) ]
}

module main
{
    io VDD
    PINROW1 p1
}
"#,
        "class",
        "static pin row: @class twice",
    );
    assert_silent(
        r#"
component PINROW2
{
    pins = [ 1 = A @class(analog) @noise(sensitive) ]
}

module main
{
    io VDD
    PINROW2 p2
}
"#,
        "static pin row: @class + @noise",
    );
}

#[test]
fn sem_attrdup__dynamic_pin_row() {
    // A bank row (`out [1:N] = D[1:N]`) is the same row grammar, so the same
    // predicate covers it.
    assert_reports(
        r#"
component DYNROW1
{
    pins = [ out [1:2] = D[1:2] @class(digital) @class(analog) ]
}

module main
{
    io VDD
    DYNROW1 d1
}
"#,
        "class",
        "dynamic pin row: @class twice",
    );
    assert_silent(
        r#"
component DYNROW2
{
    pins = [ out [1:2] = D[1:2] @class(digital) @noise(clean) ]
}

module main
{
    io VDD
    DYNROW2 d2
}
"#,
        "dynamic pin row: @class + @noise",
    );
}

#[test]
fn sem_attrdup__power_row() {
    assert_reports(
        r#"
component PWRROW1
{
    pins = [ psnk [1,2] = [VCC, GND] @class(supply) @class(signal) ]
}

module main
{
    io VDD
    PWRROW1 w1
}
"#,
        "class",
        "power row: @class twice",
    );
    assert_silent(
        r#"
component PWRROW2
{
    pins = [ psnk [1,2] = [VCC, GND] @class(supply) @noise(clean) ]
}

module main
{
    io VDD
    PWRROW2 w2
}
"#,
        "power row: @class + @noise",
    );
}

#[test]
fn sem_attrdup__module_port_row() {
    assert_reports(
        r#"
module PORTDUP
{
    io VDD @class(power) @class(signal)
}

module main
{
    io VDD
    PORTDUP pd1
}
"#,
        "class",
        "module port row: @class twice",
    );
    assert_silent(
        r#"
module PORTOK
{
    io VDD @class(power) @noise(clean)
}

module main
{
    io VDD
    PORTOK po1
}
"#,
        "module port row: @class + @noise",
    );
}

#[test]
fn sem_attrdup__connection_line_tail() {
    assert_reports(
        r#"
component SRC1
{
    pins = [ 1 = A ]
}

module main
{
    io VDD
    SRC1 s1
    s1.A -> VDD @class(analog) @class(digital)
}
"#,
        "class",
        "connection tail: @class twice",
    );
    assert_silent(
        r#"
component SRC2
{
    pins = [ 1 = A ]
}

module main
{
    io VDD
    SRC2 s2
    s2.A -> VDD @class(analog) @noise(clean)
}
"#,
        "connection tail: @class + @noise",
    );
}

#[test]
fn sem_attrdup__conduit_and_domain_row_tails() {
    assert_reports(
        r#"
module main
{
    conduit GND @role(main) @role(quiet)
    io VDD
}
"#,
        "role",
        "conduit tail: @role twice",
    );
    assert_silent(
        r#"
module main
{
    conduit GND @role(main)
    io VDD
}
"#,
        "conduit tail: @role once",
    );
    assert_reports(
        r#"
module main
{
    domain DVDD @class(digital) @class(analog) { rail [VDD_3V3, GND]::DC(3.3V) }
    io VDD
}
"#,
        "class",
        "domain tail: @class twice",
    );
    assert_silent(
        r#"
module main
{
    domain DCORE @class(digital) { rail [VCC_1V2, GND]::DC(1.2V) }
    io VDD
}
"#,
        "domain tail: @class once",
    );
}

// 7. Boundary: the same pin on two rows is not one list (ruling 3)

#[test]
fn sem_attrdup__same_pin_on_two_rows_is_silent() {
    // A row is a list; a pin is not. Two rows each declaring the key give one
    // declaration per list, and `attach_row_attrs` keeps the first on the pin
    // (identity is declared once) — silence, not an error.
    assert_silent(
        r#"
component TWOROWS1
{
    pins = [ 1 = A @class(analog) ]
    pins = [ 1 = A @class(digital) ]
}

module main
{
    io VDD
    TWOROWS1 t1
}
"#,
        "same pin, @class on two rows",
    );
    assert_silent(
        r#"
component TWOROWS2
{
    pins = [ 1 = A @class(analog) ]
    pins = [ 1 = A, 2 = B @noise(sensitive) ]
}

module main
{
    io VDD
    TWOROWS2 t2
}
"#,
        "same pin, a key carried on two rows",
    );
}

// 8. The projection: `show pins -f json` carries the row attrs

#[test]
fn sem_attrdup__show_pins_json_projects_attrs() {
    // The declaration is nowhere else in a dump: `show attrs` reads the
    // component-level list, not the per-pin one. A pin whose rows carried no
    // identity words must not gain an empty field. (Rows are newline
    // separated — a comma between two pin rows is E2083 + E2116, a declared
    // error, so the rows here are written the way they parse.)
    let src = r#"component WITHATTR
{
    pins = [ in 1 = A @class(analog)
             in 2 = B ]
}

module main
{
    io VDD
    WITHATTR w1
}
"#;
    let path = std::env::temp_dir().join("mcc-attr-key-dup-showpins.mc");
    std::fs::write(&path, src).expect("write fixture");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "show",
            "pins",
            "WITHATTR",
            "-F",
            path.to_str().expect("utf-8 path"),
            "-f",
            "json",
        ])
        .output()
        .expect("run mcc show pins");
    assert!(
        output.status.success(),
        "mcc show pins exited {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("show pins JSON output");
    let pins = value["pins"].as_array().expect("pins array");
    let with: Vec<&serde_json::Value> = pins.iter().filter(|p| p["id"] == "1").collect();
    let without: Vec<&serde_json::Value> = pins.iter().filter(|p| p["id"] == "2").collect();
    assert_eq!(with.len(), 1, "pin 1 must be in the dump: {value}");
    assert_eq!(without.len(), 1, "pin 2 must be in the dump: {value}");
    let attrs = with[0]["attrs"]
        .as_array()
        .unwrap_or_else(|| panic!("pin 1 must carry attrs: {}", with[0]));
    assert!(
        attrs.iter().any(|a| a.as_str() == Some("class = analog")),
        "pin 1 attrs must show the row's @class: {attrs:?}"
    );
    assert!(
        without[0].get("attrs").is_none(),
        "pin 2 declared no attrs and must not gain the field: {}",
        without[0]
    );
}
