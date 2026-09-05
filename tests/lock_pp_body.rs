// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks three PostParse semantic rules implemented in
//! `src/semantic/validation/body.rs` by asserting that each rule fires on a
//! minimal MCode snippet (or, for E5163, that it is currently unreachable):
//!   E5160 INST_THIS_TYPE     - `this` used as a new instance name on the
//!                              LHS of an inline `::` construction
//!   E5162 MODULE_PORT_UNUSED - a module formal port declared but never
//!                              referenced by any net line
//!   E5163 COND_SINGLE_BINARY - an `In` condition whose left operand is the
//!                              literal `0` or `1` (currently unreachable)
//! Each test runs `mcc parse --code <src> --local --pass1 --pass2 --top main
//! -f json` through the real binary and asserts the presence (or, for E5163,
//! the current absence) of the code in `result.pass0.diagnostics`.
// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix taxonomy).
#![allow(non_snake_case)]

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
        "mcc parse exited {:?}: {}",
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

/// Assert presence of `code` only. Used where the snippet cannot also be
/// error-free (E5160 itself has Error severity).
fn assert_fires(code: u64, source: &str) {
    let result = parse(source);
    assert!(
        has_code(&result, code),
        "expected E{code} to fire; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

/// Assert that the snippet parses without errors and that `code` fires.
fn assert_fires_clean(code: u64, source: &str) {
    let result = parse(source);
    assert_eq!(
        result["result"]["summary"]["errors"].as_u64(),
        Some(0),
        "snippet for E{code} must parse without errors; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        has_code(&result, code),
        "expected E{code} to fire; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// E5160 INST_THIS_TYPE (body.rs:168 check_this_lhs_declaration): `this` on the
// LHS of an inline `::` construction. `this :: TYPE` is invalid because `this`
// refers to the current instance and cannot be a new instance name. The C
// parser still accepts the text and registers a module instance named `this`;
// the check detects it structurally (name `this`, non-`Label` instance kind).
// A bare `this` as a net endpoint (e.g. `A -> this`) parses to
// `McInstance::Label("this")` and does NOT fire. E5160 is Error severity, so
// this snippet necessarily reports one error.
#[test]
fn lock_pp_body__inst_this_type_5160_fires() {
    let source = r#"component C
{
    name = "C"
    pins = [
        1 = A
    ]
}
module main
{
    this::C()
}
"#;
    assert_fires(5160, source);
}

// E5162 MODULE_PORT_UNUSED (body.rs:330 check_unconnected_module_ports): a
// module formal parameter port that appears in `insts` but is never referenced
// by any `->` net line or function body. Sub-module `BLOCK` declares `in
// signal` and its body is empty, so `signal` is never referenced. Synthetic
// `VIRT_<T>` wrappers (standalone component/interface views) are exempt, and
// non-parameter instances are filtered out by `module.params.is_defined`, so a
// real user module with an unused formal port is the shape that still fires.
// `module main` must exist for `--top main`; its empty body adds the tolerated
// E2115/E5459 warnings (zero errors).
#[test]
fn lock_pp_body__module_port_unused_5162_fires() {
    let source = r#"module BLOCK(in signal)
{
}
module main
{
}
"#;
    assert_fires_clean(5162, source);
}

// E5163 COND_SINGLE_BINARY (body.rs:256 push_single_binary_diag) cannot fire
// with the current grammar/AST classification. The check fires only for
// McCondition::In whose left operand is McCondOperand::Literal("0"/"1")
// (body.rs:228 is_single_binary_in). McCondition::In is produced only by the
// condition parser in mc_conds.rs: the dedicated `in` path (parse_in_condition,
// mc_conds.rs:425) maps the left token to McCondOperand::Ident, and the
// equality-with-square-vec fallback (mc_conds.rs:373) never materializes for
// any text tried below. In MCode source, numeric/string/hex literals directly
// on the left of `in` (`0 in ["A"]`, `1 in ["A"]`, `"0" in [A]`, `0x1 in
// ["A"]`) are grammar errors (E2081/E2082), so a Literal-left In never reaches
// the validator. The A/B control proves the cond_pins and cond_attrs walkers
// (body.rs:195/210) do run: `x in [...]` with an identifier left parses and
// stores In conditions (the E5452 missing-else infos below fire for both
// cond_pins[0] and cond_attrs[0]), yet 5163 is absent because the left operand
// is an Ident. This guard locks today's behavior: the canonical snippet parses
// cleanly and reports no E5163. Flip to a presence assertion once a
// Literal-left In condition becomes reachable.
#[test]
fn lock_pp_body__cond_single_binary_5163_currently_unreachable() {
    let source = r#"component C(x::STRING)
{
    name = "C"
    pins = [
        1 = A
    ]
    if (x in ["0", "1"])
    {
        pins += [
            io 2 = B
        ]
    }
    if (x in ["1"])
    {
        package = "P"
    }
}
module main
{
    C("x") U1
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 5163),
        "E5163 COND_SINGLE_BINARY unexpectedly fires for an identifier-left \
         `in` condition: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert_eq!(
        result["result"]["summary"]["errors"].as_u64(),
        Some(0),
        "absence snippet for E5163 must parse without errors"
    );
    assert!(
        has_code(&result, 5452),
        "absence snippet for E5163 must store In conditions (E5452 fires for \
         cond_pins/cond_attrs without else)"
    );
}
