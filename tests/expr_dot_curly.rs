// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration tests for the pass1 AST coverage fixes (P1-1 / P1-2):
//
// P1-1: arithmetic operators (`*` / `/` / `~` / `:`) on connection lines used
//       to fall into the generic E4008 "Unexpected AST node type" message.
//       Now they report an operator-specific "not supported in connection
//       statements" diagnostic instead of being mistaken for an AST shape bug.
//
// P1-2: `McOpd::new` used to fall through to `_ => None` for direct
//       MCAST_OPD_DOT / MCAST_OPD_CURLY / MCAST_OPD_CURLY_MN nodes, silently
//       dropping expressions routed through `McExpression::new`. The top-level
//       DOT/CURLY branch keeps dot (`a.b`) and curly (`a{b,c}`) accesses from
//       silently failing in expression / parameter / attribute contexts.

use mcc::McIds;
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn setup(uri: &str, source: &str) {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    mcc::mcc_load_from_string(&uri.to_string(), source);
}

fn has_code(code: u32) -> bool {
    mcc::mcc_diagnose_all().iter().any(|d| d.code == code)
}

#[test]
fn connection_line_arithmetic_reports_unsupported() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let uri = "/mcc/arith-line.mc";
    let source = r#"
module main
{
    io VIN
    io VOUT
    VIN * VOUT
}
"#;
    setup(uri, source);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri.to_string());

    // P1-1: `*` must produce the operator-specific E4008 message, not the
    // generic "Unexpected AST node type" text.
    let diags = mcc::mcc_diagnose_all();
    let has_specific = diags
        .iter()
        .any(|d| d.code == 4008 && d.msg.contains("not supported in connection statements"));
    assert!(
        has_specific,
        "expected operator-specific E4008, got: {:?}",
        diags.iter().filter(|d| d.code == 4008).collect::<Vec<_>>()
    );

    drop(lock);
}

#[test]
fn dot_in_param_value_is_parsed() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let uri = "/mcc/dot-param.mc";
    let source = r#"
component DOTPARAM (note = VIN.VOUT)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main
{
    io VIN
    io VOUT
    DOTPARAM d1
}
"#;
    setup(uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri.to_string());
    result.expect("build failed");

    let comp = mcc::get_component_def(&McIds::from("DOTPARAM"), &uri.to_string())
        .expect("DOTPARAM definition missing");
    let mcc::McCMIE::Component(comp) = comp else {
        panic!("DOTPARAM should resolve to a component");
    };
    assert!(
        comp.params.names().iter().any(|n| n == "note"),
        "param 'note' must be registered (dot value must not silently drop), got names: {:?}",
        comp.params.names()
    );

    // No parse-stage E4008/E2121 style errors from the dot access.
    assert!(
        !has_code(4008),
        "dot access in param value must not report E4008: {:?}",
        mcc::mcc_diagnose_all()
            .iter()
            .filter(|d| d.code == 4008)
            .collect::<Vec<_>>()
    );

    drop(lock);
}

#[test]
fn dot_in_attr_value_is_parsed() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let uri = "/mcc/dot-attr.mc";
    let source = r#"
component DOTATTR
{
    note = VIN.VOUT
    pins = [
        1 = 1
    ]
}

module main
{
    io VIN
    io VOUT
    DOTATTR a1
}
"#;
    setup(uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri.to_string());
    result.expect("build failed");

    let comp = mcc::get_component_def(&McIds::from("DOTATTR"), &uri.to_string())
        .expect("DOTATTR definition missing");
    let mcc::McCMIE::Component(comp) = comp else {
        panic!("DOTATTR should resolve to a component");
    };
    assert_eq!(
        comp.attrs.len(),
        1,
        "attribute 'note' must be registered (dot value must not silently drop)"
    );

    drop(lock);
}

#[test]
fn dot_in_condition_is_parsed() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let uri = "/mcc/dot-cond.mc";
    let source = r#"
component DOTCOND
{
    if (VIN.VOUT == 1) pins = [
        1 = 1
    ]
}

module main
{
    io VIN
    io VOUT
    DOTCOND c1
}
"#;
    setup(uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri.to_string());
    result.expect("build failed");

    let comp = mcc::get_component_def(&McIds::from("DOTCOND"), &uri.to_string())
        .expect("DOTCOND definition missing");
    let mcc::McCMIE::Component(comp) = comp else {
        panic!("DOTCOND should resolve to a component");
    };
    assert!(
        !comp.cond_pins.is_empty(),
        "conditional pins must survive a dot access in the condition"
    );

    drop(lock);
}
