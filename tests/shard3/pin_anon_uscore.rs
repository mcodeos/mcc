// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `_` as an anonymous pin name in pins rows (U148).
//!
//! A `_` in the pin-name slot used to raise E3004 `PIN_NAME_TYPE_UNSUPPORTED`.
//! The ruling (2026-09-20): `_` means "no name here": any number of pins may be
//! anonymous, and an anonymous pin is addressable only by its pin id. The
//! name never enters `names_to_id`, so name lookup cannot reach it, while the
//! id side of sub-addressing (already dual, `declared_pin_id`) keeps working.
//! The E3179 hint lists ids too (`addressable_members`), otherwise a fully
//! anonymous component would hint "Available pins: []".

use serde_json::Value;
use std::process::Command;

/// Run `mcc parse --code <source> --local --pass1 --pass2 --top main -f json`.
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

/// All (phase, code, message) triples across pass0/pass1/pass2.
fn diagnostics(value: &Value) -> Vec<(String, u64, String)> {
    let mut out = Vec::new();
    for phase in ["pass0", "pass1", "pass2"] {
        for d in value["result"][phase]["diagnostics"]
            .as_array()
            .unwrap_or(&Vec::new())
        {
            out.push((
                phase.to_string(),
                d["code"].as_u64().unwrap_or_default(),
                d["message"].as_str().unwrap_or_default().to_string(),
            ));
        }
    }
    out
}

/// The pin names (id fallback included) of every component instance.
fn instance_pins(value: &Value) -> Vec<String> {
    value["result"]["pass2"]["instances"]["components"]
        .as_array()
        .expect("pass2 components")
        .iter()
        .flat_map(|c| c["pins"].as_array().expect("pins").iter())
        .map(|p| p["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// One anonymous pin: registers nameless, connects by id, no diagnostic.
#[test]
fn anon__single_pin_registers_nameless_and_connects_by_id() {
    let result = parse(
        "component C\n{\n    pins = [\n        in 1 = _\n    ]\n}\n\nmodule main\n{\n    io a\n    C c1()\n    a -> c1.1\n}\n",
    );
    let diags = diagnostics(&result);
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
    assert_eq!(instance_pins(&result), vec!["1".to_string()]);
}

/// No count limit: several `_` rows and one multi-pinid `_` row all register.
#[test]
fn anon__unlimited_count_across_rows() {
    let result = parse(
        "component C\n{\n    pins = [\n        in 1 = _\n        in 2 = _\n        io [3,4] = _\n    ]\n}\n\nmodule main\n{\n    io a\n    C c1()\n    a -> c1.1\n    a -> c1.4\n}\n",
    );
    let diags = diagnostics(&result);
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
    let mut pins = instance_pins(&result);
    pins.sort();
    assert_eq!(pins, vec!["1", "2", "3", "4"]);
}

/// Anonymous and named pins coexist on one component; the named pin resolves
/// by name, the anonymous one by id.
#[test]
fn anon__mixed_named_and_anon_both_addressable() {
    let result = parse(
        "component C\n{\n    pins = [\n        in 1 = P\n        in 2 = _\n    ]\n}\n\nmodule main\n{\n    io a\n    C c1()\n    a -> c1.P\n    a -> c1.2\n}\n",
    );
    let diags = diagnostics(&result);
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
    let mut pins = instance_pins(&result);
    pins.sort();
    // The named pin displays its name; the anonymous one falls back to its id.
    assert_eq!(pins, vec!["2", "P"]);
}

/// The anonymous pin contributes its id to the E3179 hint (`addressable_members`).
#[test]
fn anon__e3179_hint_lists_anon_ids() {
    let result = parse(
        "component C\n{\n    pins = [\n        in 1 = _\n        in 4 = P\n    ]\n}\n\nmodule main\n{\n    io a\n    C c1()\n    a -> c1.99\n}\n",
    );
    let diags = diagnostics(&result);
    assert!(
        diags.iter()
            .any(|(_, code, msg)| *code == 3179 && msg.contains("Available pins: [1, 4, P]")),
        "E3179 hint must list anon ids alongside names: {diags:?}"
    );
}

/// `_` is not a name: `c1._` cannot address the pin (explicit parse error,
/// never a silent match).
#[test]
fn anon__underscore_member_never_resolves() {
    let result = parse(
        "component C\n{\n    pins = [\n        in 1 = _\n    ]\n}\n\nmodule main\n{\n    io a\n    C c1()\n    a -> c1._\n}\n",
    );
    let diags = diagnostics(&result);
    assert!(
        diags.iter().any(|(_, code, _)| *code == 2082),
        "`c1._` must be rejected outright, not resolved: {diags:?}"
    );
}

/// U148 rule 2: an interface member addresses by name AND by its interface pin
/// id: `u1.B0.1` and `u1.B0.CLK` are the same pin.
#[test]
fn anon__iface_member_addressable_by_name_and_id() {
    let result = parse(
        "interface BUS2\n{\n    pins = [\n        io 1 = CLK\n        io 2 = DAT\n    ]\n}\n\ncomponent Host\n{\n    pins = [\n        io [1,2] = B0::BUS2()\n    ]\n}\n\nmodule main\n{\n    io a\n    Host u1()\n    a -> u1.B0.1\n    a -> u1.B0.CLK\n    a -> u1.B0.2\n}\n",
    );
    let diags = diagnostics(&result);
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}

/// Anonymous interface member: the replicated single-pin binding keeps the
/// collapsed member names (`GPIO7`), and the collapsed member by id
/// (`GPIO7.1`) resolves to the same pin.
#[test]
fn anon__anon_iface_member_collapse_and_id_path() {
    let result = parse(
        "interface GP\n{\n    pins = [\n        1 = _\n    ]\n}\n\ncomponent Dev\n{\n    pins = [\n        io [1,2,3] = GPIO[7,8,9]::GP()\n    ]\n}\n\nmodule main\n{\n    io a\n    Dev d1()\n    a -> d1.GPIO7\n    a -> d1.GPIO7.1\n    a -> d1.GPIO9\n}\n",
    );
    let diags = diagnostics(&result);
    assert!(diags.is_empty(), "expected no diagnostics: {diags:?}");
}
