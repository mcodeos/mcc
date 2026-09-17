// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks three PostParse semantic rules implemented in
//! `src/semantic/validation/interface.rs` by asserting that each rule fires on
//! a minimal MCode snippet:
//!   E4104 IFACE_ROLE_NOT_FOUND      - param references a role the interface lacks
//!   E4106 IFACE_NOT_LOADED          - param references an interface that is not loaded
//!   E4107 IFACE_DEPRECATED_CMIE     - deprecated component/interface used
//! Each test only asserts the presence of its target diagnostic code; extra
//! diagnostics are tolerated as long as the snippet parses without errors and
//! the target code appears in `result.pass0.diagnostics`.
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

// E4104 IFACE_ROLE_NOT_FOUND (interface.rs check_iface_role_exists): a
// component param of the form `name::IFACE(ROLE)` selects interface `WIDGET.BUS`
// (which only defines role `Host`) with role `Slave`, so the referenced role is
// not defined in the loaded interface.
#[test]
fn lock_pp_interface__iface_role_not_found_4104_fires() {
    let source = r#"interface WIDGET.BUS
{
    pins = [
        1 = D, "Data"
    ]
    role Host
    {
        name = "Host role"
    }
}

component C(u::WIDGET.BUS(Slave))
{
    name = "C"
    pins = [
        1 = X, "x"
    ]
}

module main
{
    io VDD
}
"#;
    assert_fires(4104, source);
}

// E4106 IFACE_NOT_LOADED (interface.rs check_iface_role_exists): a component
// param of the form `name::IFACE(ROLE)` references interface `PHANTOM.BUS`,
// which is not defined anywhere in the workspace, so no matching interface is
// loaded. (E5302 DEF_REF_NOT_LOADED fires alongside; it is a warning and is
// tolerated.)
#[test]
fn lock_pp_interface__iface_not_loaded_4106_fires() {
    let source = r#"component C(u::PHANTOM.BUS(DCE))
{
    name = "C"
    pins = [
        1 = X, "x"
    ]
}

module main
{
    io VDD
}
"#;
    assert_fires(4106, source);
}

// E4107 IFACE_DEPRECATED_CMIE (interface.rs check_deprecated_cmie_usage): a
// component carrying a `deprecated` attribute is instantiated from `module
// main`, and the module-instance sweep reports the deprecated usage.
#[test]
fn lock_pp_interface__iface_deprecated_cmie_4107_fires() {
    let source = r#"component LEGACY
{
    name = "Legacy part"
    deprecated = "yes"
    pins = [
        1 = A, "a"
        2 = B, "b"
    ]
}

module main
{
    LEGACY U1
}
"#;
    assert_fires(4107, source);
}
