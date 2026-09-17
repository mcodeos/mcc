// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks five PostParse semantic rules implemented in
//! `src/semantic/validation/hw.rs` by asserting that each rule fires on a
//! minimal MCode snippet:
//!   E5502 HW_PIN_NUMBER_GAP         - component pin numbers skip a value
//!   E5503 HW_PIN_COUNT_HIGH         - component with an implausibly high pin count
//!   E5504 HW_ZERO_PINS_WITH_PARAMS  - component with parameters but zero pins
//!   E5507 HW_ALL_SAME_IO_TYPE       - all pins share one IO type
//!   E5510 HW_FUNC_PARAM_SHADOWS_PIN - a func parameter shadows a pin name
//! Each test runs `mcc parse --code <src> --local --pass1 --pass2 --top main
//! -f json` through the real binary and asserts the presence of its target
//! diagnostic code in `result.pass0.diagnostics`. Each snippet defines the
//! offending component and instantiates it inside `module main` so the
//! component enters the workspace definition space; extra diagnostics are
//! tolerated as long as the snippet parses without errors.
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

// E5502 HW_PIN_NUMBER_GAP (hw.rs check_pin_id_gaps): component pin IDs with a
// skip that exceeds the noise threshold (gaps / total pins > 5%). Pins
// 1,2,3,5 omit pin 4: one gap among four pins (25%) is reported.
#[test]
fn lock_pp_hw__pin_number_gap_5502_fires() {
    let source = r#"component GAP_PINS
{
    name = "Gap pins"
    pins = [
        io 1 = P1
        io 2 = P2
        io 3 = P3
        io 5 = P5
    ]
}
module main
{
    GAP_PINS U1
}
"#;
    assert_fires(5502, source);
}

// E5503 HW_PIN_COUNT_HIGH (hw.rs check_pin_count_extremes): a component with
// more than 300 pins is flagged as a likely data-entry error. The indexed
// range declaration `io [1:301] = P[0:300]` expands to 301 single-typed IO
// pins, which keeps the snippet minimal (the same expansion also avoids the
// all-one-IO-type rule, which skips IO-type-uniform InOut pins).
#[test]
fn lock_pp_hw__pin_count_high_5503_fires() {
    let source = r#"component BIG
{
    name = "Big"
    pins = [
        io [1:301] = P[0:300]
    ]
}
module main
{
    BIG U1
}
"#;
    assert_fires(5503, source);
}

// E5504 HW_ZERO_PINS_WITH_PARAMS (hw.rs check_pin_count_extremes): a
// component with parameters and attributes but zero pins (and no dynamic pin
// definitions or funcs) looks like an abstract component that forgot its pin
// definition. `ZERO(foo::STRING)` carries the parameter `foo` and the `name`
// attribute but no `pins` rows. Companion warnings are tolerated: E5254
// COMPONENT_NO_PINS and E5641 UNUSED_PARAM_OR_PORT also fire on this shape.
#[test]
fn lock_pp_hw__zero_pins_with_params_5504_fires() {
    let source = r#"component ZERO(foo::STRING)
{
    name = "Zero"
}
module main
{
    ZERO("x") U1
}
"#;
    assert_fires(5504, source);
}

// E5507 HW_ALL_SAME_IO_TYPE (hw.rs check_single_ioc_type_component): a
// component of at least four pins whose every active pin shares one IO type
// (here four `out` pins) suggests incomplete pin definitions. Power-only
// components and IO-type-uniform InOut pins are exempt; `out` is not, so the
// rule fires.
#[test]
fn lock_pp_hw__all_same_io_type_5507_fires() {
    let source = r#"component ALL_OUT
{
    name = "All out"
    pins = [
        out 1 = A
        out 2 = B
        out 3 = C
        out 4 = D
    ]
}
module main
{
    ALL_OUT U1
}
"#;
    assert_fires(5507, source);
}

// E5510 HW_FUNC_PARAM_SHADOWS_PIN (hw.rs check_func_param_pin_shadow): a
// component func parameter whose primary name matches a pin name (here the
// `Connect` parameter `SIG` and pin `1 = SIG`) makes net references inside
// the func body ambiguous. The func body `SIG -> SIG` demonstrates exactly
// that ambiguity; the func is never invoked, so the body is only parsed.
#[test]
fn lock_pp_hw__func_param_shadows_pin_5510_fires() {
    let source = r#"component SHADOW_F
{
    name = "Shadow func"
    pins = [
        1 = SIG
        2 = GNDP
    ]
    func Connect(SIG)
    {
        SIG -> SIG
    }
}
module main
{
    SHADOW_F U1
}
"#;
    assert_fires(5510, source);
}
