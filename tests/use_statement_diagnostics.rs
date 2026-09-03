// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]
// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Integration tests for `use` statement diagnostics (§11, §14 of use-design.md).
//!
//! These tests verify the use-stage diagnostics USE_LIB_NOT_FOUND (2052,
//! formerly E800 "library not found"), USE_DEP_NOT_DECLARED (2051 "undeclared
//! dependency"), and USE_ALIAS_COLLISION (2005, formerly E2002) are properly
//! emitted when parsing files with `use` statements.

use serde_json::Value;
use std::process::Command;

fn run_mcc_parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", source, "--local", "--pass1", "--pass2", "--top", "main", "-f",
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

/// §19.5 rule 2: in non-project context, `use` of a library that does not
/// exist on disk emits E2052 with a "not found" message (split from the
/// project-mode "undeclared dependency" E2051).
#[test]
fn sem_usediag__lib_not_found_emits_e2052() {
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
        codes.contains(&2052),
        "expected USE_LIB_NOT_FOUND in pass0 diagnostics, got codes: {codes:?}\nfull: {diags:#?}"
    );
    let e800 = diags
        .iter()
        .find(|d| d["code"] == 2052)
        .expect("USE_LIB_NOT_FOUND entry");
    let msg = e800["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("nonexistent") && msg.contains("not found"),
        "non-project USE_LIB_NOT_FOUND message should mention 'nonexistent' and 'not found', got: {msg}"
    );
}

/// §19.5 rule 2: in project context (project.toml present), `use` of a
/// library that is not declared in [dependencies] keeps the strict
/// "undeclared dependency" E2051 message and does NOT lazy-load.
#[test]
fn sem_usediag__undeclared_dependency_project_mode_keeps_undeclared_message() {
    use std::process::Command as StdCommand;
    // Build a throwaway project so the parse runs in project context.
    let dir = std::env::temp_dir().join(format!("mcc-e2051-project-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp project dir");
    std::fs::write(
        dir.join("project.toml"),
        "[project]\nname = \"e2051p\"\nversion = \"1.0.0\"\nentry = \"main.mc\"\ntop_module = \"main\"\n",
    )
    .expect("write project.toml");
    std::fs::write(
        dir.join("main.mc"),
        "use $::nonexistent.lib@1.0\n\nmodule main {\n    U1::init()\n}\n",
    )
    .expect("write main.mc");

    let output = StdCommand::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse",
            dir.to_str().expect("temp dir path"),
            "--local",
            "--pass1",
            "--pass2",
            "--top",
            "main",
            "-f",
            "json",
        ])
        .output()
        .expect("run JSON parse on project dir");
    assert!(
        output.status.success(),
        "mcc parse failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    let diags = envelope["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("pass0 diagnostics");
    let codes: Vec<u64> = diags.iter().filter_map(|d| d["code"].as_u64()).collect();
    assert!(
        codes.contains(&2051),
        "expected USE_DEP_NOT_DECLARED in pass0 diagnostics, got codes: {codes:?}"
    );
    let e800 = diags
        .iter()
        .find(|d| d["code"] == 2051)
        .expect("USE_DEP_NOT_DECLARED entry");
    let msg = e800["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("nonexistent") && msg.contains("undeclared"),
        "project-mode USE_DEP_NOT_DECLARED message should mention 'nonexistent' and 'undeclared', got: {msg}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// §19.5 rule 2: in non-project context, `use` of a library that exists on
/// disk lazily loads it — no E2051, and the library's symbols resolve.
#[test]
fn sem_usediag__non_project_use_lazily_loads_library() {
    use std::process::Command as StdCommand;

    // Build a throwaway system root with a tiny third-party library "acme"
    // exposing the acme-only component ACMERES.
    let root = std::env::temp_dir().join(format!("mcc-lazyroot-{}", std::process::id()));
    let acme = root.join("acme");
    std::fs::create_dir_all(acme.join("res")).expect("create acme dirs");
    std::fs::write(
        acme.join("acme.mc"),
        "// acme library entry: aggregates submodules.\npub use ./res/res.mc\n",
    )
    .expect("write acme.mc");
    std::fs::write(
        acme.join("res/res.mc"),
        "component ACMERES(rs::UV.OHM)\n{\n    name = \"Acme Resistor\"\n    pins = [\n        1 = 1, \"Term 1\"\n        2 = 2, \"Term 2\"\n    ]\n    spec = [\n        resistance = rs\n    ]\n}\n",
    )
    .expect("write res.mc");

    // Standalone file (no project.toml anywhere above it) using the library.
    let standalone =
        std::env::temp_dir().join(format!("mcc-lazy-standalone-{}.mc", std::process::id()));
    std::fs::write(
        &standalone,
        "use $::acme.res\n\nmodule main {\n    VIN -> ACMERES(10kOhm) -> GND\n}\n",
    )
    .expect("write standalone.mc");

    let output = StdCommand::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse",
            standalone.to_str().expect("standalone path"),
            "--local",
            "--pass1",
            "--pass2",
            "--top",
            "main",
            "-f",
            "json",
        ])
        .env("MCC_SYSTEM_ROOT", &root)
        .output()
        .expect("run JSON parse on standalone file");
    assert!(
        output.status.success(),
        "mcc parse failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    let result = &envelope["result"];

    // No E2052/E2051: the library was lazily loaded instead of reported missing.
    let diags = result["pass0"]["diagnostics"]
        .as_array()
        .expect("pass0 diagnostics");
    let codes: Vec<u64> = diags.iter().filter_map(|d| d["code"].as_u64()).collect();
    assert!(
        !codes.contains(&2052) && !codes.contains(&2051),
        "no USE_LIB_NOT_FOUND/USE_DEP_NOT_DECLARED expected after lazy load, got codes: {codes:?}\nfull: {diags:#?}"
    );

    // §15: third-party symbols are removed from the global definitions tables
    // and are only reachable through the explicit `use` path. ACMERES must
    // therefore resolve (no unresolved-class diagnostic) even though it does
    // not appear in pass1.definitions.components. We verify lazy loading
    // succeeded by asserting the whole parse produced zero errors and zero
    // unresolved-class / not-loaded diagnostics.
    let mut all_codes: Vec<u64> = Vec::new();
    for phase in ["pass0", "pass2"] {
        if let Some(arr) = result[phase]["diagnostics"].as_array() {
            all_codes.extend(arr.iter().filter_map(|d| d["code"].as_u64()));
        }
    }
    assert!(
        !all_codes.contains(&3157) && !all_codes.contains(&3154) && !all_codes.contains(&5256),
        "ACMERES should resolve after lazy load, got unresolved-class codes: {all_codes:?}"
    );
    assert_eq!(
        result["summary"]["errors"], 0,
        "no errors expected after lazy load, summary: {:?}",
        result["summary"]
    );

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&standalone);
}

/// §15: third-party library symbols should be hidden until explicitly `use`d.
/// Referencing such a symbol without `use` should produce unresolved-class
/// diagnostics (3154 / 3157 / 5256) and NOT USE_LIB_NOT_FOUND (2052) or
/// USE_DEP_NOT_DECLARED (2051), since no `use` statement was written.
#[test]
fn sem_usediag__third_party_visibility_no_e800_when_not_used() {
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
        !codes.contains(&2052) && !codes.contains(&2051),
        "USE_LIB_NOT_FOUND/USE_DEP_NOT_DECLARED should not appear when no `use` statement references a third-party library, got codes: {codes:?}"
    );
    // We expect unresolved-class diagnostics (INST_CLASS_UNRESOLVED /
    // INST_NODE_MISSING / INST_CLASS_NOT_LOADED)
    assert!(
        codes.contains(&3157) || codes.contains(&3154) || codes.contains(&5256),
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
fn sem_usediag__symbol_conflict_parse_does_not_panic() {
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

/// §6.3: `as` alias should register the module under the alias name
/// without triggering E2002 (alias collision with itself).
/// The test also verifies that the original module name is NOT leaked
/// into the importing file's spacenames.
#[test]
fn sem_usediag__alias_registers_spacename_without_collision() {
    // Run `mcc parse` on the alias_user.mc corpus file with --dlog
    // (--dlog bypasses RPC, which may have a stale PID file)
    let corpus = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let alias_user = corpus.join("alias_user.mc");
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse",
            alias_user.to_str().expect("alias_user.mc path"),
            "--local",
            "--pass1",
            "--pass2",
            "--top",
            "main",
            "-f",
            "json",
            "--dlog",
        ])
        .env(
            "MCC_SYSTEM_ROOT",
            std::env::temp_dir().join("mcc-test-root"),
        )
        .output()
        .expect("run JSON parse on alias_user.mc");
    assert!(
        output.status.success(),
        "mcc parse failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    // With --dlog, output is text diagnostics on stdout. E2002 must NOT appear.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("E2002"),
        "E2002 should not appear for a valid `as` alias. stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("collides with an existing name"),
        "alias collision message should not appear. stdout:\n{stdout}"
    );
}

/// The C lexer only treats `[A-Za-z0-9_.]` as URI characters, so a file name
/// containing a hyphen (`use ./comp-cap.mc`) is tokenized as `comp` `-` `cap`
/// `.` `mc`: the parser drops everything after the first `-`, resolves the use
/// to the wrong file, and reports a spurious PARSER_TOP_INVALID (2081). The
/// path must be recovered from the raw source text and the use must resolve to
/// the real file with no E2081 and no "use target not found" (E2003).
#[test]
fn sem_usediag__hyphenated_file_name_in_relative_use_resolves() {
    use std::process::Command as StdCommand;
    let dir = std::env::temp_dir().join(format!("mcc-hyphen-use-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(
        dir.join("comp-cap.mc"),
        "component comp_cap {\n  pin +VIN\n  pin -GND\n}\n",
    )
    .expect("write comp-cap.mc");
    std::fs::write(
        dir.join("main.mc"),
        "use ./comp-cap.mc\n\nmodule main {\n    comp_cap c1\n}\n",
    )
    .expect("write main.mc");

    let output = StdCommand::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse",
            dir.join("main.mc").to_str().expect("main.mc path"),
            "--local",
            "--pass1",
            "--pass2",
            "--top",
            "main",
            "-f",
            "json",
        ])
        .env(
            "MCC_SYSTEM_ROOT",
            std::env::temp_dir().join("mcc-hyphen-use-root"),
        )
        .output()
        .expect("run JSON parse on hyphen use file");
    assert!(
        output.status.success(),
        "mcc parse failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    let diags = envelope["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("pass0 diagnostics");
    let codes: Vec<u64> = diags.iter().filter_map(|d| d["code"].as_u64()).collect();
    assert!(
        !codes.contains(&2081),
        "no spurious top-level error for the hyphenated file name, got codes: {codes:?}\nfull: {diags:#?}"
    );
    assert!(
        !codes.contains(&2003),
        "no 'use target not found' for the hyphenated file name, got codes: {codes:?}\nfull: {diags:#?}"
    );
    // The used component must resolve (no unresolved-class diagnostics).
    assert!(
        !codes.contains(&3157) && !codes.contains(&3154) && !codes.contains(&5256),
        "hyphenated use must resolve its component, got codes: {codes:?}\nfull: {diags:#?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
