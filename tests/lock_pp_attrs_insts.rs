// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks four PostParse semantic rules from the `attrs` / `insts` validation
//! hosts (src/semantic/validation/attrs.rs, src/semantic/validation/insts.rs):
//!   E5353 ROLE_EMPTY_BODY        - interface role with an empty body
//!   E5354 ROLE_NAME_SHADOWS      - role name shadows an interface pin/param
//!   E5355 ATTR_NESTING_TOO_DEEP  - attribute value nested deeper than 16
//!   E5356 ATTR_PIN_GROUP_UNDEFINED - role keyword used as a param outside an
//!                                   interface (see errcodes.rs doc)
//! Each lock runs `mcc parse --code <src> ... -f json` through the real
//! binary and asserts the presence (or, for E5353, the current absence) of
//! the code in `result.pass0.diagnostics`.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

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

#[test]
fn lock_pp_attrs_insts__role_name_shadows_pin_member() {
    // R2 / insts.rs:329 - role name collides with an interface pin/port name.
    let source = r#"interface PPX.WIRE
{
    role TX
    {
        name = "Transmit role"
    }
    pins = [
        1 = TX
        2 = RX
    ]
}

module main
{
    PPX.WIRE U_BUS
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5354),
        "expected E5354 ROLE_NAME_SHADOWS: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn lock_pp_attrs_insts__deeply_nested_attribute_value() {
    // N4 / attrs.rs:156 - attribute value nested deeper than 16 levels.
    let mut tree = String::from("1");
    for _ in 0..17 {
        tree = format!("[ n = {tree} ]");
    }
    let source = format!(
        r#"component PPX_NEST
{{
    name = "Nested attribute"
    pins = [
        1 = OUT
    ]
    tree = {tree}
}}

module main
{{
    PPX_NEST U_NEST
}}
"#
    );
    let result = parse(&source);
    assert!(
        has_code(&result, 5355),
        "expected E5355 ATTR_NESTING_TOO_DEEP: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn lock_pp_attrs_insts__role_param_outside_interface() {
    // R7 / insts.rs:451,480 - `role` keyword param on a module (interface-only).
    let source = r#"module main(role)
{
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5356),
        "expected E5356 ATTR_PIN_GROUP_UNDEFINED: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn lock_pp_attrs_insts__role_empty_body_is_currently_unreachable() {
    // R1 / insts.rs:286 is the intended emitter for E5353 ROLE_EMPTY_BODY, but
    // with the current role grammar (mca.y `mc_role`) a role node always
    // carries its id child, so `role.body.get_sub_node()` in the check is
    // never empty (`has_body` is always true) and the code cannot fire. The
    // second emitter (attrs.rs:135 unresolvable dotted attribute name) is
    // likewise unsatisfiable: a dotted attribute's own first segment is always
    // collected into `known_keys`, so `!known_keys.contains(first_seg)` never
    // holds. This guard locks today's behavior: the canonical empty-role body
    // snippet parses (the role registers - here proven by E5354 firing) but
    // reports no E5353. Flip this to a presence assertion once the empty-role
    // check becomes reachable.
    let source = r#"interface PPX.WIRE
{
    role TX
    {
    }
    pins = [
        1 = TX
        2 = RX
    ]
}

module main
{
    PPX.WIRE U_BUS
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 5353),
        "E5353 ROLE_EMPTY_BODY unexpectedly fires for an empty role body: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}
