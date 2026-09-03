// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test for the P1-3 top-level declaration fix (B5):
// `pins.subcls = [...]` is parsed by mca.y (MCAST_ATTRIBUTE_PIN with an
// mc_id sub-class child) but was silently dropped by McPins::parse. It must
// now report NOT_SUPPORTED_YET (2171, formerly E1107) instead of silently
// ignoring the sub-class name.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::McIds;

#[test]
fn sem_pinsubcls__reports_unsupported() {
    let _lock = common::lock();
    common::reset();

    let uri: mcc::McURI = "/mcc/pins-subcls.mc".to_string();
    let source = r#"
component SUBPINS
{
    pins = [ 1 = 1 ]
    pins.subcls = [ 2 = 2 ]
}

module main
{
    io VDD
    SUBPINS s1
}
"#;
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");

    let diags = mcc::mcc_diagnose_all();
    let has_2171 = diags
        .iter()
        .any(|d| d.code == 2171 && d.msg.contains("pins.subcls"));
    assert!(
        has_2171,
        "pins.subcls must report NOT_SUPPORTED_YET (not silently drop), got: {:?}",
        diags.iter().filter(|d| d.code == 2171).collect::<Vec<_>>()
    );
}
