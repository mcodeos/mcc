// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PostParse duplicate-rule positive lock (test-coverage-gap closure).
//!
//! Each test locks one PostParse diagnostic code by asserting the code is
//! emitted for a minimal MCode snippet. Presence only is asserted: unrelated
//! extra diagnostics are acceptable as long as the snippet parses and the
//! targeted code fires.
//!
//! Codes locked here (one test each):
//!
//!   * 5001 DUP_CMIE_CROSS_FILE   — duplicate.rs:134 (cross-URI, same kind)
//!   * 5003 DUP_ENUM_VALUE        — dupwithin.rs:102
//!   * 5401 ENUM_DUPLICATE_VALUE  — enums.rs:61
//!   * 5407 RANGE_REVERSED        — enums.rs:270 (duplicate attribute keys
//!                                  share the code; exprs.rs:287 is the
//!                                  reversed range-literal site)
//!   * 5412 ATTR_SELF_REFERENTIAL — enums.rs:196
//!
//! NOT lockable by a positive fixture today (each is structurally un-fireable
//! from any MCode source; the reasons follow in the two notes below):
//!
//!   * 5002 DUP_WITHIN  — dupwithin.rs:73 iterates `pins.names_to_id`, a
//!     `BTreeMap<String, McPinPort>` whose keys are unique, so the per-name
//!     count is always 1 and `entries.len() > 1` can never hold. Verified by
//!     trying duplicate pin labels on two pin ids (`1 = A` / `2 = A`),
//!     duplicate pin ids with the same label, and pure-number labels.
//!   * 5402 ENUM_MEMBER_DOT / 5403 ENUM_MEMBER_LEADING_DIGIT /
//!     5404 ENUM_MEMBER_RESERVED — enums.rs:125/140/155. The enum value-list
//!     grammar only accepts plain identifiers: `enum E { UV.CAP }`,
//!     `enum E { 1V }`, `enum E { role }`, `enum E { X, this }` and every
//!     other reserved keyword all abort the declaration with E2081 (invalid
//!     top-level declaration) before a member can be registered, so
//!     `def.values` never carries a dotted / digit-leading / reserved name.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
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

fn diagnostics(value: &Value) -> &[Value] {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
}

fn has_code(value: &Value, code: u64) -> bool {
    diagnostics(value)
        .iter()
        .any(|diagnostic| diagnostic["code"].as_u64() == Some(code))
}

// ============================================================================
// 5001 DUP_CMIE_CROSS_FILE — same CMIE name in another workspace file.
// ============================================================================
//
// Cross-file duplicates are a workspace-level check, so this fixture loads two
// on-disk temp files through the library API and collects diagnostics with
// mcc_diagnose_all (the `mcc parse` CLI only ever loads one snippet URI).

#[test]
fn ppdup__cmie_cross_file_duplicate_emits_5001() {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    // Two workspace files each define a component named DUP_CAP: the same CMIE
    // kind (component) collides across two different non-test URIs. File B
    // additionally `use`s file A so the reverse-dependency edge exists: a
    // re-derive of A marks B dirty, so one `mcc_parse_all_modules` round
    // re-derives BOTH files. PostParse validation only emits for re-derived
    // files (pass1.rs `re_derived_set`), and the duplicate sweep attributes
    // E5001 to one of the two URIs in an unordered per-kind set — with both
    // files in the same re-derived round the finding is emitted whichever URI
    // the sweep picks. Files live on disk (virtual URIs cannot resolve `use`).
    let dir = std::env::temp_dir().join(format!("mcc-lock-pp-5001-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("dup_a.mc"),
        "component DUP_CAP\n{\n    pins = [ 1 = A ]\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("dup_b.mc"),
        "use ./dup_a.mc\n\ncomponent DUP_CAP\n{\n    pins = [ 1 = B ]\n}\n\nmodule main\n{\n    io VDD\n}\n",
    )
    .unwrap();

    let uri_a = dir.join("dup_a.mc").canonicalize().unwrap();
    let uri_b = dir.join("dup_b.mc").canonicalize().unwrap();
    let src_a = std::fs::read_to_string(&uri_a).unwrap();
    let src_b = std::fs::read_to_string(&uri_b).unwrap();
    let uri_a = uri_a.to_string_lossy().to_string();
    let uri_b = uri_b.to_string_lossy().to_string();

    mcc::mcc_load_from_string(&uri_a, &src_a);
    mcc::mcc_load_from_string(&uri_b, &src_b);
    // Re-load A: its re-derive dirties B (reverse dep), so this single round
    // re-derives A and B together and the cross-file duplicate sweep fires.
    mcc::mcc_load_from_string(&uri_a, &src_a);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri_b);

    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&5001),
        "E5001 not emitted for a cross-file component duplicate; got codes: {codes:?}"
    );
}

// ============================================================================
// 5003 DUP_ENUM_VALUE / 5401 ENUM_DUPLICATE_VALUE — duplicate enum value.
// ============================================================================
//
// The dupwithin host and the enums host each sweep duplicate enum values, so a
// single snippet fires both codes; each gets its own positive assertion.

#[test]
fn ppdup__dup_enum_value_emits_5003() {
    let source = r#"enum COLOR
{
    RED,
    RED
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5003),
        "E5003 (dupwithin) not emitted for a duplicated enum value: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn ppdup__enum_duplicate_value_emits_5401() {
    let source = r#"enum COLOR
{
    RED,
    RED
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5401),
        "E5401 (enums) not emitted for a duplicated enum value: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// ============================================================================
// 5412 ATTR_SELF_REFERENTIAL — attribute value equals its own key.
// ============================================================================

#[test]
fn ppdup__attr_self_referential_emits_5412() {
    let source = r#"component DUP_ATTR
{
    grade = grade
    pins = [ 1 = SIGNAL ]
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5412),
        "E5412 not emitted for a self-referential attribute: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// ============================================================================
// 5407 RANGE_REVERSED — duplicate attribute keys (enums.rs:270/299 site).
// ============================================================================
//
// The reversed range-literal site (exprs.rs:287) is not reachable through an
// attribute value (a bare Slice value like `{5:2}` does not parse in an attr,
// and pin-name curly templates such as `IO0{7:0}` are stored as name
// templates, not McExpression::Slice). The enums-host duplicate-attribute-key
// sweep emits the same code, which locks the diagnostic end to end.

#[test]
fn ppdup__range_reversed_dup_attr_key_emits_5407() {
    let source = r#"component DUP_ATTRKEY
{
    name = "first"
    name = "second"
    pins = [ 1 = SIGNAL ]
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5407),
        "E5407 not emitted for a duplicated attribute key: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}
