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
