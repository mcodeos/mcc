// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the call-arg lexical-family gate (CIMP U144): a construction argument
//! whose lexical family does not match the formal's declared type is a bind
//! failure (E4176) instead of a silent positional fallback. The gate lives in
//! `McParamBindings::bind_inner_opts` (mc_param.rs, after the enum member
//! check) and mirrors the declare face (E5202 / E5204, locked in
//! `lock_pp_extra.rs`).
//!
//! Probed deformations (U143 audit log 9.20.u143, probes P2–P4, all silent
//! before the gate):
//!
//!   * quoted string → enum / INT / unit-typed formal   (`"X5R"`, `"3"`, `"100nF"`)
//!   * bare number   → enum formal                      (`7` for `diel = E.X7R`)
//!
//! Not judged (locked as clean below): bare number → INT formal (the correct
//! spelling), quoted string → STRING formal.
//!
//! Ruled with the b3643 batch (U144): a bare identifier is an identity
//! reference, never data — into a STRING formal it is a family mismatch and
//! fires E4176 (locked as firing below). Bare identifiers into enum formals
//! are the enum-claiming round's territory and stay untouched.

#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

/// Shared component under test: unit-typed, enum-typed (via default), and INT
/// formals in positional order — the three typed families the gate covers.
const COMPONENT: &str = r#"enum E_DIEL { X7R, X5R }
component CD(cap::UV.CAP, diel = E_DIEL.X7R, n::INT)
{
    pins = [ 1 = A ]
}
component CS(s::STRING)
{
    pins = [ 1 = A ]
}
module main
{
    io VDD
"#;

fn parse(call: &str) -> Value {
    let source = format!("{COMPONENT}    {call} c1\n}}");
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

fn has_4176(value: &Value) -> bool {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
        .iter()
        .any(|d| d["code"].as_u64() == Some(4176))
}

/// The gate must report the mismatched call: E4176 fires with the formal's
/// name and both families in the reason.
fn assert_fires(call: &str) {
    let result = parse(call);
    assert!(
        has_4176(&result),
        "expected E4176 for `{call}`; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

/// The gate must accept the call: E4176 stays silent (other diagnostics, e.g.
/// the unused-parameter hint, are out of scope here).
fn assert_no_4176(call: &str) {
    let result = parse(call);
    assert!(
        !has_4176(&result),
        "E4176 must not fire for `{call}`; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// ── Firing locks ──

// P3: quoted string into a unit-typed formal.
#[test]
fn ctor_arg_family__string_into_uv_formal_4176_fires() {
    assert_fires(r#"CD("100nF", X7R, 3)"#);
}

// P2: quoted string into an enum-typed formal.
#[test]
fn ctor_arg_family__string_into_enum_formal_4176_fires() {
    assert_fires(r#"CD(100nF, "X7R", 3)"#);
}

// P4: quoted string into an INT-typed formal.
#[test]
fn ctor_arg_family__string_into_int_formal_4176_fires() {
    assert_fires(r#"CD(100nF, X7R, "3")"#);
}

// P2': bare number into an enum-typed formal — a number is not a member name.
#[test]
fn ctor_arg_family__number_into_enum_formal_4176_fires() {
    assert_fires(r#"CD(100nF, 7, 3)"#);
}

// ── Clean locks (the gate must not overreach) ──

// Baseline: unit value, bare enum member, bare integer — all correct.
#[test]
fn ctor_arg_family__correct_families_stay_clean() {
    assert_no_4176(r#"CD(100nF, X7R, 3)"#);
}

// A quoted string is the correct spelling for a STRING formal.
#[test]
fn ctor_arg_family__string_into_string_formal_stays_clean() {
    assert_no_4176(r#"CS("X7R")"#);
}

// A bare word as data into a STRING formal is unjudged (ruling pending, U144)
// and must not be rejected by this gate. The word must not be a known enum
// member — a member like `X7R` is already refused by the enum-claiming round
// (it cannot find an enum-class slot), which is a different, older gate.
//
// P5 ruled (b3643): a bare word is an identity reference, not data — the gate
// refuses it with E4176.
#[test]
fn ctor_arg_family__bare_word_into_string_formal_4176_fires() {
    assert_fires(r#"CS(LEFT)"#);
}

// The two-readings seam (U144 residual ⑥): the member check judges the
// member segment of a dotted spelling (`CAP.X5R` judges `X5R`); the class
// half was matched by the claiming round. The corpus-canonical dotted
// spelling stays legal; a misspelled member and a wrong-class dotted value
// both refuse.

#[test]
fn ctor_arg_family__dotted_member_stays_clean() {
    assert_no_4176(r#"CD(100nF, E_DIEL.X5R, 3)"#);
}

#[test]
fn ctor_arg_family__misspelled_dotted_member_4176_fires() {
    assert_fires(r#"CD(100nF, E_DIEL.Y9R, 3)"#);
}

#[test]
fn ctor_arg_family__wrong_class_dotted_member_4176_fires() {
    assert_fires(r#"CD(100nF, CAP.X5R, 3)"#);
}
