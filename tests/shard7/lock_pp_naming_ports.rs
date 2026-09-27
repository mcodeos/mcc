// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// PostParse lock guards for the naming/port-instance rule family.
// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix sec. 1 taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

fn parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", source, "--local", "--pass1", "--pass2", "--top", "main", "-f",
            "json",
        ])
        .output()
        .expect("run mcc parse");
    assert!(
        output.status.success(),
        "mcc parse failed for snippet:\n{source}"
    );
    serde_json::from_slice(&output.stdout).expect("parse mcc JSON output")
}

fn diagnostics(value: &Value) -> &[Value] {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
}

fn has_code(value: &Value, code: u64) -> bool {
    diagnostics(value)
        .iter()
        .any(|diagnostic| diagnostic["code"].as_u64() == Some(code))
}

#[test]
fn pp_naming_ports__component_lowercase_fires_5051() {
    // NAME_COMPONENT_LOWERCASE = 5051 (naming.rs J1 / style.rs J1 sweep).
    // A user `component` whose name starts with a lowercase letter must be
    // flagged with 5051. `--code` loads under /mcc/snippet.mc, which is not a
    // test/lab file, so the naming and style sweeps both run.
    let source = r#"component tiny_led
{
    name = "LED"
    pins = [1 = ANODE]
}

module main
{
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5051),
        "expected E5051 component-lowercase diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn pp_naming_ports__pin_mixed_convention_fires_5053() {
    // NAME_PIN_MIXED_CONVENTION = 5053 (naming.rs N9). A component whose pins
    // span >= 3 naming conventions (UPPER_SNAKE + lower_snake + UPPERFLAT
    // here) must be flagged.
    let source = r#"component MIXED_PIN
{
    name = "Mixed pin"
    pins = [
        1 = CHIP_SELECT
        2 = data_ready
        3 = DATA0
    ]
}

module main
{
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5053),
        "expected E5053 mixed-pin-convention diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn pp_naming_ports__duplicate_header_port_never_reports_5152() {
    // PORT_DUPLICATE_NAME = 5152 (ports.rs C2) is currently unreachable:
    // check_duplicate_ports counts occurrences of each name over
    // McInstances::iter_instance_names(), which yields unique BTreeMap keys,
    // so the per-name count can never exceed 1. A duplicated module-header
    // port is silently collapsed to a single registration at parse time (the
    // port is reported once as unconnected, E5162); real duplicate
    // declarations elsewhere surface as E5151 (instance declared multiple
    // times, counted via port_spans) or parse-level E2081. This absence lock
    // documents the observable behavior until the C2 check is reworked to
    // count duplicate declarations (e.g. through port_spans like its D1
    // sibling); when that happens this test must become a presence lock.
    let source = r#"module main(in signal, in signal)
{
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 5152),
        "E5152 is expected to be unreachable, but fired: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// ── mcode style-guide gates (spec/21-mcode-style.md §2/§7, style host) ──

#[test]
fn pp_naming_ports__net_and_port_lowercase_fire_5070() {
    // NAME_NET_NOT_UPPER_SNAKE = 5070 (style.rs §2 #3 sweep). Both faces that
    // name a net are judged: the declared port row (`in vin::DC(5V)`) and the
    // body net labels a connection phrase creates (`gnd`, `vout`).
    let source = r#"module main
{
    in vin::DC(5V)
    gnd -> vout
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5070),
        "expected 5070 net-name diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn pp_naming_ports__instance_name_lowercase_never_fires_5070() {
    // The instance/functional-block split is §2 #2 and stays OUTSIDE the
    // gate (§7: not machine-judgeable). A lowercase functional-block instance name and
    // an uppercase refdes instance in one module must not produce 5070.
    let source = r#"component CAP2
{
    name = "C"
    pins = [1 = P, 2 = N]
}

module main
{
    dc24v = CAP2()
    C1 = CAP2()
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 5070),
        "instance names must not be judged by the net gate: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn pp_naming_ports__role_value_lowercase_fires_5071() {
    // NAME_ROLE_ENUM_NOT_UPPER_SNAKE = 5071 (style.rs §2 #6 sweep). A role
    // value spelled Pascal-case must be flagged.
    let source = r#"interface XTAL(role)
{
    pins = [1 = XIN]

    role Osc
    {
        peer = Res
    }

    role Res
    {
        peer = Osc
    }
}

module main
{
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5071),
        "expected 5071 role-value diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn pp_naming_ports__enum_value_lowercase_fires_5071() {
    // Same code, enum face: value ids an enum declares are §2 #6 too.
    let source = r#"enum dielectric
{
    x7r,
    fast
}

module main
{
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5071),
        "expected 5071 enum-value diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn pp_naming_ports__func_lowercase_initial_fires_5072() {
    // NAME_FUNC_NOT_UPPER_INITIAL = 5072 (style.rs §2 #9 sweep). A func whose
    // name starts with a lowercase letter must be flagged.
    let source = r#"component TINY
{
    name = "T"
    pins = [1 = A]

    func enable([net1, net2])
    {
        net1 - this - net2
    }
}

module main
{
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5072),
        "expected 5072 func-name diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}
