// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks seventeen PostParse semantic rules implemented in
//! `src/semantic/validation/extra.rs` by asserting that each rule fires on a
//! minimal MCode snippet. Each test only asserts the presence of its target
//! diagnostic code; unrelated extra diagnostics are tolerated as long as the
//! targeted code appears in `result.pass0.diagnostics`.
//!
//! Codes locked here (one or more tests each):
//!
//!   * 5201 ENUM_SINGLE_VALUE            - extra.rs:79  enum with one value
//!   * 5202 PARAM_INT_DEFAULT_STRING      - extra.rs:318 ::INT/HEX default is a string (Error)
//!   * 5203 PARAM_STRING_DEFAULT_NUMERIC  - extra.rs:337 ::STRING default looks numeric
//!   * 5204 PARAM_UV_DEFAULT_NO_UNIT      - extra.rs:356 ::UV.<unit> default has no unit suffix
//!   * 5205 PARAM_FLOAT_DEFAULT_INVALID   - extra.rs:623 ::FLOAT default overflows to inf (Error)
//!   * 5206 PARAM_NEGATIVE_DEFAULT        - extra.rs:607 ::INT default is negative
//!   * 5251 PARAM_RESERVED_KEYWORD        - extra.rs:539 currently UNREACHABLE (guarded below)
//!   * 5252 FUNC_EMPTY_BODY               - extra.rs:131 (module func) and :155 (component func)
//!   * 5253 COMPONENT_EMPTY               - extra.rs:258 component with no params/pins/attrs/funcs
//!   * 5254 COMPONENT_NO_PINS             - extra.rs:269 component with content but no pins
//!   * 5255 INTERFACE_EMPTY               - extra.rs:290 interface with no pins or roles
//!   * 5257 COMPONENT_MIXED_CASE          - extra.rs:506 mixed-case component name
//!   * 5258 BUS_DUPLICATE_MEMBER          - extra.rs:469 duplicate bus member in a module
//!   * 5260 DEFINE_NO_ATTRS               - extra.rs:384 define with no attributes
//!   * 5261 DEFINE_NON_ATTR_CLAUSE        - extra.rs:399 define body holds a non-attribute clause
//!   * 5263 FUNC_SHARES_NAME_WITH_PORT    - extra.rs:573 func name equals a port/param name
//!   * 5267 SPEC_KEY_DUPLICATE            - extra.rs:671 duplicate sub-key inside a `spec` value
//!
//! E5251 is structurally unreachable: a component parameter whose name is one
//! of the thirteen reserved keywords in `check_reserved_names` is rejected by
//! the grammar before the semantic model exists, so the guard below locks the
//! current absence.
//!
//! Each snippet runs through the `mcc parse --code` CLI against the same
//! PostParse registry `mcc parse` uses; diagnostics are read from the pass 0
//! bucket (`result.result.pass0.diagnostics`).

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

/// Assert presence of `code` only. Used where the target diagnostic itself is
/// Error severity (E5202, E5205) so the snippet cannot also be error-free, or
/// where extra warnings are an accepted side effect of the minimal trigger.
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

// E5201 ENUM_SINGLE_VALUE (extra.rs:79 check run_post_parse U1 block): an enum
// whose value list holds exactly one value. `enum E_PKG { A }` is the minimal
// single-value enum.
#[test]
fn lock_pp_extra__enum_single_value_5201_fires() {
    let source = r#"enum E_PKG { A }
module main
{
    io VDD
}
"#;
    assert_fires_clean(5201, source);
}

// E5202 PARAM_INT_DEFAULT_STRING (extra.rs:318 check_default_type_mismatch,
// INT/HEX arm): a component ::INT parameter whose default literal is a quoted
// string. The diagnostic is Error severity, so this snippet necessarily
// reports one error.
#[test]
fn lock_pp_extra__param_int_default_string_5202_fires() {
    let source = r#"component C_INT_STR(n::INT = "5")
{
    pins = [ 1 = A ]
}
module main { C_INT_STR u1 }
"#;
    assert_fires(5202, source);
}

// E5203 PARAM_STRING_DEFAULT_NUMERIC (extra.rs:337 check_default_type_mismatch,
// STRING arm): a component ::STRING parameter whose default literal starts with
// an ASCII digit (here `123`).
#[test]
fn lock_pp_extra__param_string_default_numeric_5203_fires() {
    let source = r#"component C_STR_NUM(s::STRING = 123)
{
    pins = [ 1 = A ]
}
module main { C_STR_NUM u1 }
"#;
    assert_fires_clean(5203, source);
}

// E5204 PARAM_UV_DEFAULT_NO_UNIT (extra.rs:356 check_default_type_mismatch,
// unit-value arm): a ::UV.<unit> parameter whose default literal is a plain
// number with no unit suffix (`5` instead of `5V`).
#[test]
fn lock_pp_extra__param_uv_default_no_unit_5204_fires() {
    let source = r#"component C_UV_NOUNIT(v::UV.VOLT = 5)
{
    pins = [ 1 = A ]
}
module main { C_UV_NOUNIT u1 }
"#;
    assert_fires_clean(5204, source);
}

// E5205 PARAM_FLOAT_DEFAULT_INVALID (extra.rs:623 check_default_value_range,
// float arm): a ::FLOAT parameter whose default overflows f64. The literal
// `1e999` survives lexing and then parses as +inf, tripping the range check.
// The diagnostic is Error severity, so this snippet necessarily reports one
// error.
#[test]
fn lock_pp_extra__param_float_default_invalid_5205_fires() {
    let source = r#"component C_FLT_INF(f::FLOAT = 1e999)
{
    pins = [ 1 = A ]
}
module main { C_FLT_INF u1 }
"#;
    assert_fires(5205, source);
}

// E5206 PARAM_NEGATIVE_DEFAULT (extra.rs:607 check_default_value_range, INT/HEX
// arm): a ::INT parameter whose default literal starts with `-`.
#[test]
fn lock_pp_extra__param_negative_default_5206_fires() {
    let source = r#"component C_INT_NEG(n::INT = -5)
{
    pins = [ 1 = A ]
}
module main { C_INT_NEG u1 }
"#;
    assert_fires_clean(5206, source);
}

// E5251 PARAM_RESERVED_KEYWORD (extra.rs:539 check_reserved_names) is
// currently UNREACHABLE. The check iterates component parameters whose primary
// name is one of the thirteen reserved words {"this", "pins", "role", "func",
// "return", "in", "out", "io", "ps", "anl", "nc", "if", "else"}, but the
// grammar treats every one of those words as a hard keyword: using any of them
// as a component parameter name (typed `nc::STRING` or bare `nc`) fails to
// parse with E2081 "Invalid top-level declaration" before the semantic model
// exists, so no such parameter ever reaches check_reserved_names. All thirteen
// spellings were probed against the CLI and each yields E2081. This guard
// locks today's behavior: the canonical reserved-word parameter snippet
// reports E2081 (proving the keyword-as-param path is exercised) but never
// E5251. Flip to a presence assertion only if a future grammar change admits
// reserved keywords as parameter names.
#[test]
fn lock_pp_extra__param_reserved_keyword_5251_is_currently_unreachable() {
    let source = r#"component C_RES(nc)
{
    pins = [ 1 = A ]
}
module main { C_RES u1 }
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 2081),
        "reserved keyword `nc` as a parameter name should be rejected with E2081: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        !has_code(&result, 5251),
        "E5251 PARAM_RESERVED_KEYWORD unexpectedly fires for a keyword parameter name: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// E5252 FUNC_EMPTY_BODY, component arm (extra.rs:155 check_empty_functions): a
// component func whose body holds neither statements nor instances.
#[test]
fn lock_pp_extra__func_empty_body_component_5252_fires() {
    let source = r#"component C_EMPTY_FUNC
{
    pins = [ 1 = A ]
    func do_nothing()
    {
    }
}
module main { C_EMPTY_FUNC u1 }
"#;
    assert_fires_clean(5252, source);
}

// E5252 FUNC_EMPTY_BODY, module arm (extra.rs:131 check_empty_functions): a
// module func whose body is empty. `module main` itself is fine here because
// the func keeps it from being a stub.
#[test]
fn lock_pp_extra__func_empty_body_module_5252_fires() {
    let source = r#"module main
{
    func empty_func()
    {
    }
}
"#;
    assert_fires_clean(5252, source);
}

// E5253 COMPONENT_EMPTY (extra.rs:258 check_component_structure M1): a
// component with no params, pins, attributes, or funcs.
#[test]
fn lock_pp_extra__component_empty_5253_fires() {
    let source = r#"component EMPTY_COMP
{
}
module main
{
    io VDD
}
"#;
    assert_fires_clean(5253, source);
}

// E5254 COMPONENT_NO_PINS (extra.rs:269 check_component_structure M3): a
// component that has content (an attribute) but no pin definitions.
#[test]
fn lock_pp_extra__component_no_pins_5254_fires() {
    let source = r#"component PASSIVE_NO_PINS
{
    name = "passive"
}
module main { PASSIVE_NO_PINS u1 }
"#;
    assert_fires_clean(5254, source);
}

// E5255 INTERFACE_EMPTY (extra.rs:290 check_interface_structure M4): an
// interface with neither pins nor roles.
#[test]
fn lock_pp_extra__interface_empty_5255_fires() {
    let source = r#"interface IF_EMPTY
{
}
module main
{
    io VDD
}
"#;
    assert_fires_clean(5255, source);
}

// E5257 COMPONENT_MIXED_CASE (extra.rs:506 check_naming_convention F2): a
// component name whose first segment starts lowercase and contains an
// uppercase letter (`ledDriver`). Companion E5051 (lowercase start, Info) also
// fires; both are tolerated.
#[test]
fn lock_pp_extra__component_mixed_case_5257_fires() {
    let source = r#"component ledDriver
{
    pins = [ 1 = A ]
}
module main { ledDriver u1 }
"#;
    assert_fires_clean(5257, source);
}

// E5258 BUS_DUPLICATE_MEMBER (extra.rs:469 check_bus_member_collision D3): a
// module port declared as a curly bus with the same member listed twice.
// `io MIC{1, 1}` registers a Bus instance whose member list contains `1`
// twice, so the module scan reports the duplicate.
#[test]
fn lock_pp_extra__bus_duplicate_member_5258_fires() {
    let source = r#"module SUB_BUS
{
    io MIC{1, 1}
}
module main { SUB_BUS u1 }
"#;
    assert_fires_clean(5258, source);
}

// E5260 DEFINE_NO_ATTRS (extra.rs:384 check_empty_defines U5): a define whose
// attribute list is empty.
#[test]
fn lock_pp_extra__define_no_attrs_5260_fires() {
    let source = r#"define D_NOATTR
{
}
module main
{
    io VDD
}
"#;
    assert_fires_clean(5260, source);
}

// E5261 DEFINE_NON_ATTR_CLAUSE (extra.rs:399 check_empty_defines U4): a define
// whose body mixes a real attribute (`partno`) with a net clause. Net clauses
// are syntactically accepted inside define bodies but are flagged because a
// define should only hold attributes. The present attribute keeps E5260 from
// firing, isolating E5261.
#[test]
fn lock_pp_extra__define_non_attr_clause_5261_fires() {
    let source = r#"define D_MIXED
{
    partno = "abc"
    gnd - g0
}
module main
{
    io VDD
}
"#;
    assert_fires_clean(5261, source);
}

// E5263 FUNC_SHARES_NAME_WITH_PORT (extra.rs:573 check_func_name_conflict R5):
// a module func whose name equals a module parameter name in the same module.
// The empty func body additionally trips E5252 (Warning); tolerated here.
#[test]
fn lock_pp_extra__func_shares_name_with_port_5263_fires() {
    let source = r#"module SUB_PORT_FUNC(foo)
{
    func foo()
    {
    }
}
module main { SUB_PORT_FUNC u1 }
"#;
    assert_fires_clean(5263, source);
}

// E5267 SPEC_KEY_DUPLICATE (extra.rs:671 check_duplicate_spec_keys): a
// component `spec` value whose sub-attribute list repeats a sub-key. Only the
// `;`-separated pair form parses (`spec = [ v = 5V, v = 12V ]` is rejected
// with E2082); two sub-attributes sharing the key `voltage` reach the check.
#[test]
fn lock_pp_extra__spec_key_duplicate_5267_fires() {
    let source = r#"component C_SPEC_DUP
{
    pins = [ 1 = A ]
    spec = [ voltage = 5V; voltage = 12V ]
}
module main { C_SPEC_DUP u1 }
"#;
    assert_fires_clean(5267, source);
}
