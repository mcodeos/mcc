// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The general `/`-composite unit face (U370): a number followed by
//! slash-joined stems (`100ppm/℃`, `1mV/°C`, `5%/°C`) lexes as ONE unit
//! value and reaches the parameter face as a quantity — not as a division
//! phrase whose right half is an identifier (`℃` used to fall to ID, spec
//! §4.3 point 3). The seam rules lock in both directions:
//!
//!   * a composite default fits a leaf `::UV.<head-family>` declaration
//!     (head-family equivalence, `100ppm/°C` on `::UV.PPM` is clean);
//!   * a composite default in ANOTHER family still fires E5207;
//!   * an unknown stem is a real E3046 anchored diagnostic, not silence;
//!   * plain arithmetic (`10/3`) is untouched by the composite rule.
//!
//! Same harness as `lock_pp_extra.rs`: `mcc parse --code … -f json`, each
//! test asserts only its target face.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

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
        "mcc parse exited {:?}; stderr: {}",
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

fn error_count(value: &Value) -> u64 {
    value["result"]["summary"]["errors"].as_u64().unwrap_or(0)
}

/// The corpus form: a temp-co default on a `::UV.PPM` parameter must arrive
/// as one composite unit value — zero errors, and no E5207 (the head family
/// is `PPM`, so the head-family equivalence seam accepts it). All three
/// degree spellings are the same value.
#[test]
fn u370_composite__tempco_default_fits_ppm_declaration() {
    for spelling in ["100ppm/℃", "100ppm/°C", "100ppm/degC"] {
        let source = format!(
            "component C_TEMPCO(tc::UV.PPM = {spelling})\n{{\n    pins = [ 1 = A ]\n}}\nmodule main {{ C_TEMPCO u1 }}\n"
        );
        let result = parse(&source);
        assert_eq!(
            error_count(&result),
            0,
            "{spelling}: composite default must parse clean; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
        assert!(
            !has_code(&result, 5207),
            "{spelling}: head family PPM matches ::UV.PPM, no E5207; got: {}",
            result["result"]["pass0"]["diagnostics"]
        );
        assert!(
            !has_code(&result, 5357),
            "{spelling}: a composite unit literal is a constant, not a non-constant default; got: {}",
            result["result"]["pass0"]["diagnostics"]
        );
    }
}

/// The 5357 exemption must not become a silencer: a slash-text whose stems
/// do NOT all resolve (`100ppm/parsec` — refused by the table, E3046) is not
/// exempted, so the pre-existing non-constant-default warning still fires.
#[test]
fn u370_composite__unresolvable_slash_default_still_warns_5357() {
    let source = r#"component C_BADSTEM2(tc::UV.PPM = 100ppm/parsec)
{
    pins = [ 1 = A ]
}
module main { C_BADSTEM2 u1 }
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5357),
        "100ppm/parsec is not exempted from the non-constant-default heuristic; got: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

/// The other temp-co shapes ride the same rule: numerator scales through
/// its own family, the degree is the denominator label.
#[test]
fn u370_composite__mv_and_percent_tempco_shapes() {
    for (default, head) in [("1mV/°C", "VOLT"), ("5%/°C", "PERCENT")] {
        let source = format!(
            "component C_DRIFT(d::UV.{head} = {default})\n{{\n    pins = [ 1 = A ]\n}}\nmodule main {{ C_DRIFT u1 }}\n"
        );
        let result = parse(&source);
        assert_eq!(
            error_count(&result),
            0,
            "{default}: must parse clean; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
        assert!(!has_code(&result, 5207), "{default}: no E5207");
    }
}

/// The seam must not become a silencer: a composite default whose head
/// family does NOT match the declaration still fires E5207.
#[test]
fn u370_composite__cross_family_composite_still_fires_5207() {
    let source = r#"component C_TEMPCO_WRONG(tc::UV.TEMP = 100ppm/°C)
{
    pins = [ 1 = A ]
}
module main { C_TEMPCO_WRONG u1 }
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5207),
        "ppm/°C on ::UV.TEMP is another family and must fire E5207; got: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

/// An unknown stem is refused with E3046 (UVAL_UNIT_UNSUPPORTED), anchored
/// — never silently accepted, never guessed.
#[test]
fn u370_composite__unknown_stem_is_unsupported_3046() {
    let source = r#"component C_BADSTEM(tc::UV.PPM = 100ppm/parsec)
{
    pins = [ 1 = A ]
}
module main { C_BADSTEM u1 }
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 3046),
        "unknown stem `parsec` must fire E3046; got: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

/// Generic composites (non-temperature denominator) parse as unit values
/// too — `9m/s` on `::UV.LEN` rides the head-family seam.
#[test]
fn u370_composite__generic_composite_parses_clean() {
    let source = r#"component C_GENERIC(v::UV.LEN = 9m/s)
{
    pins = [ 1 = A ]
}
module main { C_GENERIC u1 }
"#;
    let result = parse(source);
    assert_eq!(
        error_count(&result),
        0,
        "9m/s must parse as one composite unit value; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(!has_code(&result, 5207));
}

/// Arithmetic is untouched: `10/3` in a value position keeps the division
/// phrase path (no unit diagnostic fires, no capture into a composite).
#[test]
fn u370_composite__plain_division_is_not_captured() {
    let source = r#"component C_DIV(r::UV.VOLT)
{
    pins = [ 1 = A ]
}
module main
{
    io VDD
    C_DIV u1(r = 3.3V)
    attr = [ ratio = 10/3 ]
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 3046) && !has_code(&result, 3044) && !has_code(&result, 3049),
        "10/3 must keep the arithmetic path (no unit-family diagnostic); got: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

/// The compound declaration side pairs with the composite value side:
/// `::UV.PPM/UV.TEMP` declared, `100ppm/℃` written — the structural type
/// and the structural value meet with zero diagnostics.
#[test]
fn u370_composite__compound_declared_meets_composite_default() {
    let source = r#"component C_PAIR(tc::UV.PPM/UV.TEMP = 100ppm/℃)
{
    pins = [ 1 = A ]
}
module main { C_PAIR u1 }
"#;
    let result = parse(source);
    assert_eq!(
        error_count(&result),
        0,
        "compound declared + composite default must meet clean; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(!has_code(&result, 5207));
    assert!(!has_code(&result, 5204));
}
