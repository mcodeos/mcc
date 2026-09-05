// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PostParse defs-rule positive lock (test-coverage-gap closure).
//!
//! Each test locks one `defs`-host PostParse diagnostic code by asserting the
//! code is emitted for a minimal MCode snippet. Presence only is asserted:
//! unrelated extra diagnostics are acceptable as long as the snippet parses
//! and the targeted code fires.
//!
//! Codes locked here (one test each):
//!
//!   * 5301 DEF_AMBIGUOUS_NAME  - defs.rs:78 (interface x enum collision,
//!                                 Warning) and :103 (component x module
//!                                 collision, Info). The component x interface
//!                                 pair is intentionally NOT reported.
//!   * 5302 DEF_REF_NOT_LOADED  - defs.rs:208 (component param declare-class
//!                                 expression). The pin-binding arm (:180) and
//!                                 the module-instance arm (:242) are pre-empted
//!                                 by earlier pipeline stages: an unknown
//!                                 `::IFACE()` pin binding is downgraded with
//!                                 E3110 and an unknown module-instance class
//!                                 becomes `McInstance::Unresolved` (E5256 /
//!                                 E3157), so neither arm ever sees a typed
//!                                 reference whose class is absent. The param
//!                                 arm is syntactic (`name::CLASS()`), so it
//!                                 reaches the check with the class name intact.
//!   * 5303 COMPONENT_INT_SUFFIX - defs.rs:273 (component named `*.int`).
//!   * 5304 ENUM_INT_SUFFIX      - defs.rs:298 (enum named `*.int`) and :323
//!                                 (interface named `*.int`).
//!
//! Each snippet runs through the `mcc parse --code` CLI against the same
//! PostParse registry `mcc parse` uses; diagnostics are read from the pass 0
//! bucket (`result.result.pass0.diagnostics`).

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
// 5301 DEF_AMBIGUOUS_NAME - interface and enum share a name.
// ============================================================================

#[test]
fn ppdefs__iface_enum_same_name_emits_5301() {
    let source = r#"interface X
{
    pins = [ 1 = A ]
}

enum X { B, C }

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5301),
        "E5301 not emitted when an interface and an enum share a name: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// ============================================================================
// 5302 DEF_REF_NOT_LOADED - component param declares an unloaded class.
// ============================================================================

#[test]
fn ppdefs__param_declares_unloaded_class_emits_5302() {
    // `u::GHOST_IF()` is a declare-style typed param (MCAST_DECLARE): the class
    // name is recorded syntactically, so the PostParse check sees `GHOST_IF`
    // absent from every loaded table and reports it.
    let source = r#"component C(u::GHOST_IF())
{
    pins = [ 1 = X, "x" ]
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5302),
        "E5302 not emitted when a component param declares an unloaded class: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// ============================================================================
// 5303 COMPONENT_INT_SUFFIX - component name ends with `.int`.
// ============================================================================

#[test]
fn ppdefs__component_int_suffix_emits_5303() {
    let source = r#"component BAD.int
{
    pins = [ 1 = X, "x" ]
}

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5303),
        "E5303 not emitted for a component named '*.int': {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// ============================================================================
// 5304 ENUM_INT_SUFFIX - enum name ends with `.int`.
// ============================================================================

#[test]
fn ppdefs__enum_int_suffix_emits_5304() {
    // The `.int` suffix survives default hygiene in the McIds display name.
    // The enum value list must stay on one line: the multi-line enum form does
    // not parse.
    let source = r#"enum PKG.int { A, B }

module main
{
    io VDD
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5304),
        "E5304 not emitted for an enum named '*.int': {}",
        result["result"]["pass0"]["diagnostics"]
    );
}
