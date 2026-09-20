// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Keyword constants (`HIGH`/`LOW`) as condition operands.
//!
//! `HIGH`/`LOW` lex as `MCONST_*` macros and parse to `MCAST_CONST`. The
//! condition operand collector had no `MCAST_CONST` arm, so the operand was
//! silently dropped, the whole condition parsed as `None`, and the if-branch
//! was discarded — `if (sel == HIGH)` always took the else branch, with no
//! diagnostic. The collector now reads the const's raw text (the same face
//! the default-value extractor reads), so `sel = HIGH` compares equal to
//! `== HIGH`.
//!
//! Evidence and the full usability matrix:
//! `mcd/doc/grammar/keyword-constants.md` §3–4.

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

/// `sel = HIGH` compared against the keyword spelling: the then-branch must
/// win. Before the fix this silently took the else branch.
#[test]
fn kw_const__eq_keyword_operand_takes_then_branch() {
    let result = parse(
        "component SEL(sel = HIGH)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (sel == HIGH) { pins += [2 = Q_H] }\n    else { pins += [3 = Q_L] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_H".to_string()) && !pins.contains(&"Q_L".to_string()),
        "sel == HIGH must take the then branch; pins: {pins:?}"
    );
}

/// `!=` with a keyword operand: `sel = LOW` does not differ from `LOW`, so
/// the else branch wins. The negation must not change the operand's fate.
#[test]
fn kw_const__neq_keyword_operand_takes_else_branch() {
    let result = parse(
        "component SEL(sel = LOW)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (sel != LOW) { pins += [2 = Q_H] }\n    else { pins += [3 = Q_L] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_L".to_string()) && !pins.contains(&"Q_H".to_string()),
        "sel != LOW must take the else branch; pins: {pins:?}"
    );
}

/// A non-matching keyword operand takes the else branch — the condition is
/// now evaluated, not discarded.
#[test]
fn kw_const__nonmatching_keyword_operand_takes_else_branch() {
    let result = parse(
        "component SEL(sel = LOW)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (sel == HIGH) { pins += [2 = Q_H] }\n    else { pins += [3 = Q_L] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_L".to_string()) && !pins.contains(&"Q_H".to_string()),
        "sel = LOW must not satisfy == HIGH; pins: {pins:?}"
    );
}
