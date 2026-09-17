// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks six PostParse semantic rules implemented in
//! `src/semantic/validation/exprs.rs` by asserting that each rule fires on a
//! minimal MCode snippet:
//!   E5405 ATTR_INFINITE_FLOAT    - infinite float attribute value (unreachable)
//!   E5406 ATTR_LARGE_INT         - oversized integer attribute value (unreachable)
//!   E5408 RANGE_SINGLE_ELEMENT   - single-element range/slice `3:3`
//!   E5409 IDX_MULTIPLE_SLICE_SPEC - two inst names sharing one base key
//!   E5410 EXPR_THIS_TOP_LEVEL    - `this` in a top-level net line
//!   E5411 EXPR_PLACEHOLDER_ONLY  - a net connecting only to `_`
//! Each test runs `mcc parse --code <src> --local --pass1 --pass2 --top main
//! -f json` through the real binary and asserts the presence (or, for E5405
//! and E5406, the current absence) of the code in
//! `result.pass0.diagnostics`.
// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

/// Run `mcc parse --code <source> --local --pass1 --pass2 --top main -f json`
/// and return the parsed JSON result.
fn parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args(&[
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
/// error-free (E5410 itself has Error severity) or where extra warnings are
/// an accepted side effect of the minimal trigger.
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

// E5410 EXPR_THIS_TOP_LEVEL (exprs.rs:71 check_this_outside_instance): `this`
// referenced from a top-level net line of a module. `this` parses to an
// endpoint label that `collect_referenced_names` picks up; the check fires
// for every module top-level stmt naming `this`. E5410 itself is Error
// severity, so this snippet necessarily reports one error.
#[test]
fn lock_pp_exprs__this_top_level_5410_fires() {
    let source = r#"module main
{
    io A
    A -> this
}
"#;
    assert_fires(5410, source);
}

// E5411 EXPR_PLACEHOLDER_ONLY (exprs.rs:104 check_uscore_sole_endpoint):
// a net whose series consists only of `_` (NC placeholder) endpoints.
// `_` parses to McPhrase::Lead, so an all-Lead series is detected
// structurally. `_ -> _` has no real endpoint at all.
#[test]
fn lock_pp_exprs__placeholder_only_5411_fires() {
    let source = r#"module main
{
    _ -> _
}
"#;
    assert_fires_clean(5411, source);
}

// E5408 RANGE_SINGLE_ELEMENT (exprs.rs:300 check_expr_range): a Slice
// expression whose integer bounds are equal (`3:3`) expands to one element.
// Attribute values that are colon expressions parse as
// McAttrVal::AttrExpr(McExpression::Slice) and are visited by
// check_val_for_reversed_range. Component attribute keys are free-form here;
// the same walker also feeds the overflow check below, which makes this the
// A/B control proving attribute values are visited at all.
#[test]
fn lock_pp_exprs__range_single_element_5408_fires() {
    let source = r#"component C
{
    name = "C"
    pins = [
        1 = A
    ]
    foo = 3:3
}
module main
{
    C U1
}
"#;
    assert_fires_clean(5408, source);
}

// E5409 IDX_MULTIPLE_SLICE_SPEC (exprs.rs:342 check_idx_key_collision): two
// module inst names share the same base key before `[` with different slice
// specs. Sliced inst statements such as `c[1:2] -> c[3:4]` register both
// `c[1:2]` and `c[3:4]` in module.insts, so the base key `c` accumulates two
// full names. The statements live in a non-`main` submodule: inside `main`
// itself the same text is dropped as an invalid top-level statement.
#[test]
fn lock_pp_exprs__idx_multiple_slice_spec_5409_fires() {
    let source = r#"module SM
{
    c[1:2] -> c[3:4]
}

module main
{
}
"#;
    assert_fires_clean(5409, source);
}

// E5406 ATTR_LARGE_INT (exprs.rs:190 check_expr_overflow) cannot fire with the
// current grammar/AST classification. check_constant_overflow walks component
// attribute values through check_val_for_overflow, which visits only
// McAttrVal::AttrExpr and nested McAttrVal::Attributes; bare number values
// such as `foo = 2000000000` are classified as McAttrVal::AttrLiteral by
// McAttribute::new_attr_values (src/semantic/component/mc_attr.rs) and hit
// the `_ => {}` arm, so the oversized literal never reaches
// McExpression::Int. Parenthesized or arithmetic forms that would classify as
// AttrExpr are grammar errors (E2082), and numbers inside `pins = [...]`
// rows are pin declarations, not attribute values. The A/B control above
// (`foo = 3:3`) fires E5408 through the identical walker, proving the walker
// runs but literal values are not AttrExpr. This guard locks today's
// behavior: the canonical snippet parses cleanly and reports no E5406. Flip
// to a presence assertion once number literals become reachable.
#[test]
fn lock_pp_exprs__attr_large_int_5406_currently_unreachable() {
    let source = r#"component C
{
    name = "C"
    pins = [
        1 = A
    ]
    foo = 2000000000
}
module main
{
    C U1
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 5406),
        "E5406 ATTR_LARGE_INT unexpectedly fires for an oversized integer literal: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert_eq!(
        result["result"]["summary"]["errors"].as_u64(),
        Some(0),
        "absence snippet for E5406 must parse without errors"
    );
}

// E5405 ATTR_INFINITE_FLOAT (exprs.rs:205 check_expr_overflow) cannot fire for
// the same structural reason as E5406: the float `1e999` parses as f64 to
// +inf and is stored by McAttribute::new_attr_values as
// McAttrVal::AttrLiteral, which check_val_for_overflow skips, so the
// infinite McExpression::Float never reaches check_expr_overflow. Floating
// values cannot be forced into AttrExpr through the current grammar (see the
// E5406 note). This guard locks today's behavior: the canonical snippet
// parses cleanly and reports no E5405. Flip to a presence assertion once
// float literals become reachable.
#[test]
fn lock_pp_exprs__attr_infinite_float_5405_currently_unreachable() {
    let source = r#"component C
{
    name = "C"
    pins = [
        1 = A
    ]
    foo = 1e999
}
module main
{
    C U1
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 5405),
        "E5405 ATTR_INFINITE_FLOAT unexpectedly fires for an infinite float literal: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert_eq!(
        result["result"]["summary"]["errors"].as_u64(),
        Some(0),
        "absence snippet for E5405 must parse without errors"
    );
}
