// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks six PostParse semantic rules implemented in
//! `src/semantic/validation/conds.rs` by asserting that each rule fires on a
//! minimal MCode snippet. Each test only asserts the presence of its target
//! diagnostic code; extra diagnostics are tolerated as long as the snippet
//! parses without errors and the target code appears in
//! `result.pass0.diagnostics`.
// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

/// Run `mcc parse --code <source> --local --pass1 --pass2 --top main -f json`
/// and return the parsed JSON result.
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
        "mcc parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
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

/// Assert that `source` parses without errors and that code `code` fires.
fn assert_fires(code: u64, source: &str) {
    let result = parse(source);
    assert!(
        has_code(&result, code),
        "expected E{} to fire; diagnostics: {}",
        code,
        result["result"]["pass0"]["diagnostics"]
    );
    assert_eq!(
        result["result"]["summary"]["errors"].as_u64(),
        Some(0),
        "snippet for E{} must parse without errors",
        code
    );
}

// E5453 PIN_NC_COMPONENT_LEVEL (conds.rs check_pin_io_context): an `nc`
// iotype keyword on a component pin whose name is not literally "NC"/"nc".
// Here `nc 1 = RESERVED` declares pin 1 as not-connected at the component
// level, which is unusual (NC normally appears at instantiation).
#[test]
fn lock_pp_conds__pin_nc_component_level_5453_fires() {
    let source = r#"component RESERVED_PIN
{
    name = "Reserved pin"
    pins = [
        nc 1 = RESERVED
    ]
}
module main
{
    RESERVED_PIN U1
}
"#;
    assert_fires(5453, source);
}

// E5455 PIN_IO_MIX_IN_OUT (conds.rs check_pin_alt_roles): a shared pin name
// maps to more than one pin ID whose IO types mix In and Out. Pin 1 is `in`
// and pin 2 is `out`, both named SIG, so the shared name resolves to both
// directions.
#[test]
fn lock_pp_conds__pin_io_mix_in_out_5455_fires() {
    let source = r#"component DUAL_ROLE
{
    name = "Dual role"
    pins = [
        in 1 = SIG
        out 2 = SIG
    ]
}
module main
{
    DUAL_ROLE U1
}
"#;
    assert_fires(5455, source);
}

// E5456 PIN_IO_MIX_OUTPUT_POWER (conds.rs check_pin_alt_roles): a shared pin
// name maps to one Output pin and one Power pin (potential backfeed risk).
// Pin 1 is `out` and pin 2 is `ps`, both named PWR.
#[test]
fn lock_pp_conds__pin_io_mix_output_power_5456_fires() {
    let source = r#"component PWR_FB
{
    name = "Power feedback"
    pins = [
        out 1 = PWR
        ps 2 = PWR, voltage:3.3V
    ]
}
module main
{
    PWR_FB U1
}
"#;
    assert_fires(5456, source);
}

// E5457 PIN_IO_MIX_ANALOG_POWER (conds.rs check_pin_alt_roles): a shared pin
// name maps to one Analog pin and one Power pin (unusual combination). Pin 1
// is `anl` and pin 2 is `ps`, both named PWR.
#[test]
fn lock_pp_conds__pin_io_mix_analog_power_5457_fires() {
    let source = r#"component ANL_PWR
{
    name = "Analog power"
    pins = [
        anl 1 = PWR
        ps 2 = PWR, voltage:3.3V
    ]
}
module main
{
    ANL_PWR U1
}
"#;
    assert_fires(5457, source);
}

// E5458 PARAM_PIN_NAME_SHADOW (conds.rs check_param_pin_name_collision): a
// component parameter shares its name with a pin. The parameter `mode`
// collides with the pin named `mode`.
#[test]
fn lock_pp_conds__param_pin_name_shadow_5458_fires() {
    let source = r#"component SHADOW(mode::STRING)
{
    name = "Shadow"
    pins = [
        1 = mode
    ]
}
module main
{
    SHADOW("x") U1
}
"#;
    assert_fires(5458, source);
}

// E5459 MODULE_STUB (conds.rs check_empty_module): a module with no params,
// instances, net statements, or functions is reported as a stub. An empty
// `module main` is the minimal trigger. (The parser also emits E2115 empty
// body for the empty braces; that companion diagnostic is tolerated.)
#[test]
fn lock_pp_conds__module_stub_5459_fires() {
    let source = r#"module main
{
}
"#;
    assert_fires(5459, source);
}
