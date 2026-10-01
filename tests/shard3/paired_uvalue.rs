// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The paired unit value keeps both halves through the parameter face
//! (U371; canon §4.5). `10A@5V` is a composite literal: the left half is
//! the recorded quantity, the right half the condition reference — data
//! for later computation, never dropped. Before U371 both drop sites
//! (the call-arg route via `McUnitValue::new`'s first-child read and the
//! expression route's `u.left` fold) kept only `10A`.
//!
//! Locked from the CLI face (`mcc parse --code … -f json`, same harness
//! as the cond_* tests): the whole-pair spelling must reach the unit
//! mismatch echo (E5552) while the unit verdict reads the quantity half
//! (`Amp`), the R05 bind error echoes the argument as written (E4176),
//! a unit-matching pair claims on its quantity half, and the named-arg
//! (expression-route) face binds a pair without degrading.

use serde_json::Value;
use std::process::Command;

/// Run `mcc parse --code <source> --local --pass1 --pass2 --top main -f json`
/// against one component whose sole formal is `formal` and one instance
/// argument list `call`.
fn parse(formal: &str, call: &str) -> Value {
    let source = format!(
        "component CD({formal})\n{{\n    pins = [ 1 = A ]\n}}\nmodule main\n{{\n    io VDD\n    {call} c1\n}}"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", &source, "--local", "--pass1", "--pass2", "--top", "main", "-f",
            "json",
        ])
        .output()
        .expect("run mcc parse");
    assert!(
        output.status.success(),
        "mcc parse exited {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse mcc JSON output")
}

/// Every diagnostic across the pass views, as `(severity, code, message)`.
fn all_diags(value: &Value) -> Vec<(String, u64, String)> {
    ["pass0", "pass1"]
        .iter()
        .flat_map(|phase| {
            value["result"][phase]["diagnostics"]
                .as_array()
                .expect("pass diagnostics")
                .iter()
                .map(|d| {
                    (
                        d["severity"].as_str().unwrap_or_default().to_string(),
                        d["code"].as_u64().unwrap_or_default(),
                        d["message"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn errors(diags: &[(String, u64, String)]) -> Vec<&(String, u64, String)> {
    diags.iter().filter(|(sev, _, _)| sev == "error").collect()
}

#[test]
fn pair_echoes_whole_but_reads_its_quantity_half() {
    // `10A@5V` into a Volt formal: the mismatch echo must name the whole
    // pair (the value is carried, not folded) while the unit verdict reads
    // the quantity half (`Amp`, not the condition's `Volt`).
    let diags = all_diags(&parse("v::UV.VOLT", "CD(10A@5V)"));
    let echo = diags
        .iter()
        .find(|(_, code, _)| *code == 5552)
        .unwrap_or_else(|| panic!("expected E5552; diags: {diags:?}"));
    assert!(
        echo.2.contains("10A@5V"),
        "the whole pair must reach the echo; got: {}",
        echo.2
    );
    assert!(
        echo.2.contains("unit Amp"),
        "the unit verdict must read the quantity half; got: {}",
        echo.2
    );
}

#[test]
fn bind_error_echoes_the_argument_as_written() {
    // The R05 round-2 unit claim fails for `10A@5V` against a Volt-only
    // formal; the error names the argument as written — the whole pair.
    let diags = all_diags(&parse("v::UV.VOLT", "CD(10A@5V)"));
    let bind = diags
        .iter()
        .find(|(_, code, _)| *code == 4176)
        .unwrap_or_else(|| panic!("expected E4176; diags: {diags:?}"));
    assert!(
        bind.2.contains("parameter '10A@5V'"),
        "the bind error must echo the whole pair; got: {}",
        bind.2
    );
}

#[test]
fn unit_matching_pair_claims_on_its_quantity_half() {
    // `10A@5V` into an Amp formal: the claim reads the quantity half, the
    // condition half rides along, and the bind is clean.
    let diags = all_diags(&parse("v::UV.AMP", "CD(10A@5V)"));
    assert!(
        errors(&diags).is_empty(),
        "a unit-matching pair must bind clean; diags: {diags:?}"
    );
}

#[test]
fn named_arg_pair_binds_without_degrading() {
    // The expression route (drop site D2): a bare call-site `k = v` arrives
    // as one attribute whose expression value converts through
    // expr_to_param_value; a pair must reach the binding as a value, not
    // degrade to a family error or a placeholder.
    let diags = all_diags(&parse("v::UV.VOLT", "CD(v = 10A@5V)"));
    assert!(
        errors(&diags).is_empty(),
        "a named-arg pair must bind clean; diags: {diags:?}"
    );
}
