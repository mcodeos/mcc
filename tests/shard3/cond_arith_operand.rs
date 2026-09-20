// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Arithmetic expressions as condition comparison operands (U147).
//!
//! `if (count == 2 + 3)` parsed the right side into an `MCAST_EXPRESSION`
//! wrapping an arithmetic node, the operand collector had no arm for it, the
//! operand was silently dropped, the whole condition parsed as `None`, and the
//! if-branch was discarded — always else, with no diagnostic. The collector
//! now carries the expression tree and the value engine folds it at
//! evaluation time (doc/eval V7), so unit families and the `1200mV`/`1.2V`
//! normalization are the engine's, not the collector's.

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

/// `count = 5` against `2 + 3`: the then-branch must win. Before the fix this
/// silently took the else branch.
#[test]
fn cond_arith__sum_operand_hit_takes_then_branch() {
    let result = parse(
        "component SEL(count = 5)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count == 2 + 3) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "count = 5 must satisfy count == 2 + 3; pins: {pins:?}"
    );
}

/// A default that does not sum to the folded value takes the else branch.
#[test]
fn cond_arith__sum_operand_miss_takes_else_branch() {
    let result = parse(
        "component SEL(count = 4)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count == 2 + 3) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_LO".to_string()) && !pins.contains(&"Q_HI".to_string()),
        "count = 4 must not satisfy count == 2 + 3; pins: {pins:?}"
    );
}

/// Operator precedence inside the operand: `2 * 2 + 1` folds as
/// `(2 * 2) + 1` — multiplication binds tighter.
#[test]
fn cond_arith__mixed_operands_fold_with_precedence() {
    let result = parse(
        "component SEL(count = 5)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (count == 2 * 2 + 1) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "count = 5 must satisfy count == 2 * 2 + 1; pins: {pins:?}"
    );
}

/// Unit-valued operands fold through the engine: `1.2V + 0.8V` is `2V`, the
/// same quantity the comparison reads — no suffix stripping anywhere.
#[test]
fn cond_arith__unit_operands_fold_through_engine() {
    let result = parse(
        "component SEL(v = 2V)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (v == 1.2V + 0.8V) { pins += [2 = Q_HI] }\n    else { pins += [3 = Q_LO] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_HI".to_string()) && !pins.contains(&"Q_LO".to_string()),
        "v = 2V must satisfy v == 1.2V + 0.8V; pins: {pins:?}"
    );
}
