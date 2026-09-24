// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Bare-word members in the `in` member list (U146).
//!
//! `if (sel in [A, B, C])` reached the member collector as bare identifier
//! nodes, but only `MCAST_STRING` items were read — bare members were skipped,
//! `values` stayed empty, and the condition never matched, silently. The
//! collector now takes a bare member's own text, so `sel = B` matches the bare
//! `B` member.
//!
//! Members of the other word family never match (U144, ruling of 2026-09-20):
//! a bare default skips quoted members without a diagnostic, because the `in`
//! face has no single judge to report at.

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

/// A bare default matching a bare member: the then-branch must win. Before
/// the fix this silently took the else branch.
#[test]
fn cond_in__bare_member_hit_takes_then_branch() {
    let result = parse(
        "component SEL(sel = B)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (sel in [A, B, C]) { pins += [2 = Q_IN] }\n    else { pins += [3 = Q_OUT] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_IN".to_string()) && !pins.contains(&"Q_OUT".to_string()),
        "sel = B must match in [A, B, C]; pins: {pins:?}"
    );
}

/// A default outside the bare member list takes the else branch — the
/// condition is evaluated, not discarded.
#[test]
fn cond_in__bare_member_miss_takes_else_branch() {
    let result = parse(
        "component SEL(sel = D)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (sel in [A, B, C]) { pins += [2 = Q_IN] }\n    else { pins += [3 = Q_OUT] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_OUT".to_string()) && !pins.contains(&"Q_IN".to_string()),
        "sel = D must not match in [A, B, C]; pins: {pins:?}"
    );
}

/// A quoted member never matches a bare default (U144, ruling of 2026-09-20):
/// the cross-family members are skipped, so the else branch wins.
#[test]
fn cond_in__quoted_member_of_a_bare_default_never_matches() {
    let result = parse(
        "component SEL(sel = B)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (sel in [\"A\", \"B\", \"C\"]) { pins += [2 = Q_IN] }\n    else { pins += [3 = Q_OUT] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_OUT".to_string()) && !pins.contains(&"Q_IN".to_string()),
        "sel = B must not match the quoted [\"A\", \"B\", \"C\"]; pins: {pins:?}"
    );
}

/// Mixed list: only the members in the default's own family are candidates.
#[test]
fn cond_in__mixed_list_candidates_come_from_one_family_only() {
    let result = parse(
        "component SEL(sel = B)\n{\n    name = \"SEL\"\n    pins = [1 = P]\n    if (sel in [A, \"B\"]) { pins += [2 = Q_IN] }\n    else { pins += [3 = Q_OUT] }\n}\n\nmodule main\n{\n    SEL sel_a\n}\n",
    );
    let pins = instance_pins(&result);
    assert!(
        pins.contains(&"Q_OUT".to_string()) && !pins.contains(&"Q_IN".to_string()),
        "the quoted \"B\" must not match a bare default even next to a bare A; \
         pins: {pins:?}"
    );
}
