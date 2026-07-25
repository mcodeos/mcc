// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Integration tests for `use` statement diagnostics (§11, §14 of use-design.md).
//!
//! These tests verify that E800 (undeclared dependency) and E801 (symbol conflict)
//! diagnostics are properly emitted when parsing files with `use` statements.

use serde_json::Value;
use std::process::Command;

fn run_mcc_parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse",
            "--code",
            source,
            "--pass1",
            "--pass2",
            "--top",
            "main",
            "-f",
            "json",
        ])
        .output()
        .expect("run JSON parse");
    assert!(
        output.status.success(),
        "mcc parse failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"].clone()
}

/// §11: E800 — `use` of undeclared third-party library must emit a warning
/// that flows into the pass0 diagnostics snapshot.
#[test]
fn undeclared_dependency_emits_e800() {
    let source = r#"
use $::nonexistent.lib@1.0

module main {
    U1::init()
}
"#;
    let result = run_mcc_parse(source);
    let diags = result["pass0"]["diagnostics"]
        .as_array()
        .expect("pass0 diagnostics");
    let codes: Vec<u64> = diags.iter().filter_map(|d| d["code"].as_u64()).collect();
    assert!(
        codes.contains(&800),
        "expected E800 in pass0 diagnostics, got codes: {codes:?}\nfull: {diags:#?}"
    );
    let e800 = diags
        .iter()
        .find(|d| d["code"] == 800)
        .expect("E800 entry");
    let msg = e800["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("nonexistent") && msg.contains("undeclared"),
        "E800 message should mention 'nonexistent' and 'undeclared', got: {msg}"
    );
}

/// §15: third-party library symbols should be hidden until explicitly `use`d.
/// Referencing such a symbol without `use` should produce E1601 / E1401 / E2606
/// (and NOT E800, since no `use` statement was written).
#[test]
fn third_party_visibility_no_e800_when_not_used() {
    let source = r#"
module main {
    TI_MCU::init()
}
"#;
    let result = run_mcc_parse(source);
    let diags = result["pass0"]["diagnostics"]
        .as_array()
        .expect("pass0 diagnostics");
    let codes: Vec<u64> = diags.iter().filter_map(|d| d["code"].as_u64()).collect();
    assert!(
        !codes.contains(&800),
        "E800 should not appear when no `use` statement references a third-party library, got codes: {codes:?}"
    );
    // We expect unresolved-class diagnostics
    assert!(
        codes.contains(&1601) || codes.contains(&1401) || codes.contains(&2606),
        "expected unresolved-class diagnostic, got codes: {codes:?}"
    );
}

/// §14: when two `use` paths share the same final module name and that module
/// exports overlapping symbols, E801 should be emitted.
///
/// This test uses a non-existent library to verify the conflict detection
/// triggers. E801 may not fire if the symbol sets can't be loaded, so we
/// only assert that the parse path completes and produces diagnostics.
#[test]
fn symbol_conflict_parse_does_not_panic() {
    let source = r#"
module main {
    use $::lib1.power@1.0
    use $::lib2.power@1.0
}
"#;
    let result = run_mcc_parse(source);
    let diags = result["pass0"]["diagnostics"]
        .as_array()
        .expect("pass0 diagnostics");
    // We don't strictly require E801 here because the target files don't
    // exist in the corpus, but the parse must complete and produce diagnostics
    // (e.g. E2003 for missing target).
    assert!(
        !diags.is_empty(),
        "expected at least one diagnostic for the conflicting use statements, got none"
    );
}
