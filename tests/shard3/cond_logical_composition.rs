// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Composed judges: `&&`/`||` between two comparisons (U145).
//!
//! The lexer had no `&&`/`||` tokens — a doubled `&`/`|` was two bitwise
//! operator tokens and `mc_judge` could not nest a judge, so
//! `if (count == 5 && count > 3)` was a parse error (2082/2081) and the
//! composition could not be written at all. Per the operator ruling, the
//! doubled spellings are the logical operators and the single `&`/`|` remain
//! the bitwise judges.

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

/// The pin names the top module's first component instance ended up with.
fn instance_pins(value: &Value) -> Vec<String> {
    value["result"]["pass2"]["instances"]["components"]
        .as_array()
        .expect("pass2 components")
        .iter()
        .flat_map(|c| c["pins"].as_array().expect("pins").iter())
        .map(|p| p["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// Both conjuncts true: the then-branch must win. Before the fix this did not
/// even parse.
#[test]
fn cond_logic__and_composition_hit_takes_then_branch() {
    let result = parse(
        "component SEL(count = 5)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count == 5 && count > 3) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "count == 5 && count > 3 must hold; pins: {pins:?}"
    );
}

/// One conjunct false: the else branch wins.
#[test]
fn cond_logic__and_composition_miss_takes_else_branch() {
    let result = parse(
        "component SEL(count = 2)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count == 5 && count > 3) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_LO".to_string()) && !pins.contains(&"Q_HI".to_string()),
        "count == 5 && count > 3 must fail for count = 2; pins: {pins:?}"
    );
}

/// Disjunction with one true disjunct: the then-branch wins.
#[test]
fn cond_logic__or_composition_hit_takes_then_branch() {
    let result = parse(
        "component SEL(count = 2)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count < 3 || count > 10) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "count < 3 || count > 10 must hold for count = 2; pins: {pins:?}"
    );
}

/// Both disjuncts false: the else branch wins.
#[test]
fn cond_logic__or_composition_miss_takes_else_branch() {
    let result = parse(
        "component SEL(count = 5)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count < 3 || count > 10) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_LO".to_string()) && !pins.contains(&"Q_HI".to_string()),
        "count < 3 || count > 10 must fail for count = 5; pins: {pins:?}"
    );
}

/// `&&` binds tighter than `||`: `a || b && c` reads as `a || (b && c)`.
#[test]
fn cond_logic__and_binds_tighter_than_or() {
    let result = parse(
        "component SEL(count = 5)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count == 2 || count == 5 && count > 3) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "count == 2 || (count == 5 && count > 3) must hold for count = 5; pins: {pins:?}"
    );
}

/// Parenthesized judges compose unchanged.
#[test]
fn cond_logic__parenthesized_judges_compose() {
    let result = parse(
        "component SEL(count = 5)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if ((count == 5) && (count > 3)) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "(count == 5) && (count > 3) must hold; pins: {pins:?}"
    );
}

/// Bitwise regression: the single `&` stays the bitwise judge — `1 & 0x01`
/// is non-zero, so the then-branch wins.
#[test]
fn cond_logic__single_ampersand_stays_bitwise() {
    let result = parse(
        "component SEL(flag = 1)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (flag & 0x01) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "flag = 1 & 0x01 must be non-zero; pins: {pins:?}"
    );
}

/// Bitwise regression, zero side: `0 & 0x01` is zero — the else branch wins,
/// so the bitwise reading did not collapse into logical-and.
#[test]
fn cond_logic__single_ampersand_zero_takes_else_branch() {
    let result = parse(
        "component SEL(flag = 0)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (flag & 0x01) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_LO".to_string()) && !pins.contains(&"Q_HI".to_string()),
        "flag = 0 & 0x01 must be zero; pins: {pins:?}"
    );
}
