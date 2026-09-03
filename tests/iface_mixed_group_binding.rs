// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Regression: a mixed scalar + nested-group pin list such as
// `[1, [5,6,7]] = [VBUS, GND]::DC(5V)` must bind pin 1 -> VBUS and pins 5,6,7
// -> GND. parse_pinid used to drop the scalar `1` when a nested group was
// present, leaving only one group `[5,6,7]` and raising E3111 (declares 2
// pins but 1 group given).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

fn build(lhs: &str) -> (mcc::MccProjectTree, mcc::NodeArena, mcc::InstanceStore) {
    let source = format!(
        r#"
interface DC(volt)
{{
    pins = [
        1 = VCC5V0
        2 = GND
    ]
}}

component CONN
{{
    pins = [
        {lhs} = [VBUS, GND]::DC(5V)
    ]
}}

module main(ps GND)
{{
    CONN sock
    sock.VBUS -> GND
    sock.GND -> GND
}}
"#
    );

    let uri: McURI = "/mcc/iface-mixed-group.mc".to_string();
    mcc::mcc_load_from_string(&uri, &source);
    let result = mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build failed");
    let (instance, arena, store, _net_store) = result;
    (instance, arena, store)
}

fn assert_binding(lhs: &str, expected: &[(&str, &str)]) {
    let (instance, arena, store) = build(lhs);

    let diagnostics = mcc::mcc_diagnose_all();
    let e3111: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == mcc::errcodes::PARAM_DECLARE_IFACE_PINS)
        .collect();
    assert!(
        e3111.is_empty(),
        "E3111 must not fire for `{lhs} = [VBUS, GND]`: {:?}",
        e3111
            .iter()
            .map(|d| (&d.msg, d.loc.pos))
            .collect::<Vec<_>>()
    );

    let view = mcc::TreeView::new(&arena, &store);
    let component = view
        .components(&instance)
        .find(|component| component.name == "sock")
        .expect("CONN instance");
    for (pin, name) in expected {
        assert_eq!(
            component.pin_name(pin).as_deref(),
            Some(*name),
            "`{lhs}`: pin {pin} must bind to {name}"
        );
    }
}

#[test]
fn mat_ifacebind__scalar_then_group_binds_positionally() {
    let _lock = common::lock();
    common::reset();

    assert_binding(
        "[1, [5,6,7]]",
        &[("1", "VBUS"), ("5", "GND"), ("6", "GND"), ("7", "GND")],
    );
}

#[test]
fn mat_ifacebind__group_then_scalar_preserves_source_order() {
    let _lock = common::lock();
    common::reset();

    assert_binding(
        "[[5,6,7], 1]",
        &[("5", "VBUS"), ("6", "VBUS"), ("7", "VBUS"), ("1", "GND")],
    );
}
