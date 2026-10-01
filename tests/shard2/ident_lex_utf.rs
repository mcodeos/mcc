// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The ID leaf's lexical face (mcode-grammar spec, the ID row): a basic
//! identifier may start with a letter, a digit, or an underscore (`2nd` is
//! legal), and collects UTF spelling characters (`μ`, `µ`, `Ω`, `℃`, `%`,
//! ...) into the name — `μV_in` is one identifier. These locks pin the accept
//! verdict from both directions: the whole spelling must lex as one ID, so the
//! port row stays valid, the declared port and its references meet (no E3136
//! floating label), and the spelling reaches the naming style gate (E5070)
//! unsplit.
//!
//! Same harness as `adopt_dotted_numeric_tail.rs`: `mcc parse --code … -f
//! json`, each test asserts only its target face; the tolerated noise is the
//! E5070 style info and main's unused-VDD warning.

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

/// Every diagnostic across the three pass views, as `(severity, code, message)`.
fn all_diags(value: &Value) -> Vec<(String, u64, String)> {
    ["pass0", "pass1", "pass2"]
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

/// The lexical acceptance face: no error-severity diagnostic at all. The
/// E5070 style info and main's unused-VDD warning are tolerated.
fn assert_no_errors(value: &Value, context: &str) {
    let errors: Vec<(u64, String)> = all_diags(value)
        .into_iter()
        .filter(|(sev, _, _)| sev == "error")
        .map(|(_, code, msg)| (code, msg))
        .collect();
    assert!(
        errors.is_empty(),
        "{context}: expected the ID leaf to accept the spelling, got errors: {errors:?}"
    );
}

fn has_code(value: &Value, code: u64) -> bool {
    all_diags(value).iter().any(|(_, c, _)| *c == code)
}

/// `2nd` starts with a digit. Declared as a port and referenced from both
/// sides of an `A` via label, the spelling must lex as one ID: a split at the
/// digit would invalidate the port row or strand the references as floating
/// labels.
#[test]
fn lex_id__digit_lead_is_one_identifier() {
    let src = "module T()\n{\n    in 2nd\n    A -> 2nd\n    2nd -> A\n}\nmodule main { io VDD }";
    let result = parse(src);
    assert_no_errors(&result, "digit-leading `2nd`");
    assert!(
        !has_code(&result, 3136),
        "references to the digit-leading port must resolve to it; got: {:#?}",
        all_diags(&result)
    );
}

/// UTF characters collect into the name: `μV_in` (Greek mu, U+03BC) starts
/// with a UTF letter, `µF` (micro sign, U+00B5) is the other mu codepoint,
/// `A_Ω℃` carries Ω and ℃ mid-name. Each is declared once and referenced,
/// so a lexer split at any UTF char would show up as a floating label.
#[test]
fn lex_id__utf_chars_collect_into_the_name() {
    let src = "module T()\n{\n    in μV_in\n    in µF\n    in A_Ω℃\n    A_Ω℃ -> μV_in\n    μV_in -> µF\n}\nmodule main { io VDD }";
    let result = parse(src);
    assert_no_errors(&result, "UTF spellings μV_in / µF / A_Ω℃");
    assert!(
        !has_code(&result, 3136),
        "references between the UTF-named ports must resolve; got: {:#?}",
        all_diags(&result)
    );
}

/// `%` collects mid-name (`R_50%`), an underscore may lead (`_x9`), and the
/// mixed shape `R2D2_uF` stays one identifier.
#[test]
fn lex_id__percent_underscore_and_mixed_shapes() {
    let src = "module T()\n{\n    in R_50%\n    in _x9\n    in R2D2_uF\n    R_50% -> _x9\n    _x9 -> R2D2_uF\n}\nmodule main { io VDD }";
    let result = parse(src);
    assert_no_errors(&result, "`R_50%` / `_x9` / `R2D2_uF`");
    assert!(
        !has_code(&result, 3136),
        "references between the mixed-shape ports must resolve; got: {:#?}",
        all_diags(&result)
    );
}

/// The whole-spelling witness: the style gate (E5070) must name the full
/// `μV_in` spelling. Had the lexer split the name at `μ`, the gate would see
/// fragments (`V` or `_in`), never the whole word.
#[test]
fn lex_id__style_gate_sees_the_unsplit_spelling() {
    let src = "module T()\n{\n    in μV_in\n}\nmodule main { io VDD }";
    let result = parse(src);
    let style_hits: Vec<String> = all_diags(&result)
        .into_iter()
        .filter(|(_, c, _)| *c == 5070)
        .map(|(_, _, m)| m)
        .collect();
    assert!(
        style_hits.iter().any(|m| m.contains("μV_in")),
        "E5070 must quote the whole `μV_in` spelling; got: {style_hits:?}"
    );
}
