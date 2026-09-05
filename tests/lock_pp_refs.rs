// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks three PostParse semantic rules from the `refs` validation host
//! (src/semantic/validation/refs.rs, RefIntegrityCheck::run_post_parse):
//!   E5103 FUNC_PARAMS_NO_BODY       - component func with params but an empty body
//!   E5102 REF_INTEGRITY             - component param left with an unknown type
//!   E5101 SPEC_KEY_UNDECLARED_PARAM - spec key value references an undeclared param
//! Each lock runs `mcc parse --code <src> ... -f json` through the real
//! binary and asserts the presence of the code in `result.pass0.diagnostics`.
//! Presence only is asserted: unrelated extra diagnostics are acceptable as
//! long as the snippet parses and the targeted code fires.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

fn parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args(&[
            "parse", "--code", source, "--local", "--pass1", "--pass2", "--top", "main", "-f",
            "json",
        ])
        .output()
        .expect("run mcc parse");
    assert!(
        output.status.success(),
        "mcc parse exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse mcc JSON output")
}

fn has_code(value: &Value, code: u64) -> bool {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
        .iter()
        .any(|diagnostic| diagnostic["code"].as_u64() == Some(code))
}

#[test]
fn lock_pp_refs__func_params_without_body_emits_5103() {
    // B1 / refs.rs:54 - a component func that declares parameters but has no
    // body (no stmts, no insts) is a stub. `func Wire(anode, cathode)` with an
    // empty block fires it; the func never needs to be called.
    let source = r#"component STUB_FUNC
{
    name = "Stub"
    pins = [ 1 = SIGNAL ]
    func Wire(anode, cathode)
    {
    }
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5103),
        "expected E5103 FUNC_PARAMS_NO_BODY: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn lock_pp_refs__untyped_param_unknown_kind_emits_5102() {
    // I2 / refs.rs:91 - a component param with no type annotation whose type
    // could not be inferred stays Unknown and is flagged. A bare `gain` that
    // is never used in any attribute cannot be inferred, so it fires.
    let source = r#"component AMPLIFIER(gain)
{
    name = "Amplifier"
    pins = [ 1 = SIGNAL ]
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5102),
        "expected E5102 REF_INTEGRITY: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn lock_pp_refs__spec_key_undeclared_param_emits_5101() {
    // I1 / refs.rs:132 - an attr under a `spec.*` key whose value is a bare
    // identifier that names no declared component param is an error.
    // `spec.enable = UNDECLARED_FLAG` references a variable the component
    // never declares (only `threshold` is declared), so it fires.
    let source = r#"component CRITICAL(threshold::INT)
{
    name = "Critical"
    spec.enable = UNDECLARED_FLAG
    pins = [ 1 = SIGNAL ]
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5101),
        "expected E5101 SPEC_KEY_UNDECLARED_PARAM: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}
