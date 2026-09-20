// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E4186 IFACE_VIEW_LANE_MISMATCH (conductor-view-design.md R-CV2, the
//! uniformity law): an interface that declares a role-less conductor view
//! pins every role table carrying its own `pins` list to the same lane
//! count. Unequal counts make role-less bindings and role bindings resolve
//! different shapes from the same interface — the ambiguity is judged at the
//! definition, not at an instantiation point.
//!
//! Same harness as `iface_role_arg_literal.rs`: `mcc parse --code … -f json`,
//! each test asserts only its target code; extra diagnostics tolerated.

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

fn codes_with(value: &Value, code: u64) -> Vec<String> {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
        .iter()
        .filter(|d| d["code"].as_u64() == Some(code))
        .map(|d| d["message"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A 2-lane conductor view with one matching and one mismatching role.
const IFACE: &str = r#"interface CVW(role)
{
    pins = [
        1 = _
        2 = _
    ]
    role Good
    {
        name = "Good role"
        pins = [
            1 = A, "a-side lane 1"
            2 = B, "a-side lane 2"
        ]
        peer = Bad
    }
    role Bad
    {
        name = "Bad role"
        pins = [
            1 = A, "b-side lane 1"
            2 = B, "b-side lane 2"
            3 = C, "b-side lane 3"
        ]
        peer = Good
    }
}
"#;

// The mismatching role fires E4186 and names the interface, the role and
// both counts; the matching role stays silent.
#[test]
fn lock_pp_interface__view_lane_mismatch_4186_fires_per_role() {
    let source = format!(
        "{IFACE}\ncomponent C(w::CVW(Good))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4186);
    assert!(
        hits.len() == 1,
        "expected exactly one E4186 (role Bad only); diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    let m = &hits[0];
    assert!(
        m.contains("CVW") && m.contains("Bad") && m.contains("3") && m.contains("2"),
        "E4186 must name the interface, the role and both lane counts: {}",
        m
    );
    assert!(
        !m.contains("Good"),
        "the matching role must not be named: {}",
        m
    );
}

// The inheritance arm: a role without its own pins table never fires, even
// against a declared view.
#[test]
fn lock_pp_interface__role_inheriting_view_no_4186() {
    let src = r#"interface CVI(role)
{
    pins = [
        1 = _
        2 = _
    ]
    role Master
    {
        name = "Master role"
        peer = Slave
    }
    role Slave
    {
        name = "Slave role"
        peer = Master
    }
}
component C(w::CVI(Master))
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
    let result = parse(src);
    let hits = codes_with(&result, 4186);
    assert!(
        hits.is_empty(),
        "inheriting roles must not fire E4186: {:?}",
        hits
    );
}

// Role-tables-only interface (no view): outside R-CV2's scope until a view
// is declared.
#[test]
fn lock_pp_interface__role_only_no_view_no_4186() {
    let src = r#"interface RVO(role)
{
    role Master
    {
        name = "Master role"
        pins = [
            1 = A, "a"
            2 = B, "b"
        ]
        peer = Slave
    }
    role Slave
    {
        name = "Slave role"
        pins = [
            1 = A, "a"
            2 = B, "b"
        ]
        peer = Master
    }
}
component C(w::RVO(Master))
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
    let result = parse(src);
    let hits = codes_with(&result, 4186);
    assert!(
        hits.is_empty(),
        "role-tables-only interfaces are out of scope: {:?}",
        hits
    );
}
