// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: an `enum` and a `component` sharing the same base name in
// one file must coexist without E0501 "Definition already exists" (P0-3).
//
// Regression: `parse_cmie_names` collected all declaration names into a single
// list without tracking their types, so `enum CAP` + `component CAP` (as in
// mcode/cap.mc) triggered the duplicate-name error even though the design doc
// (same-name-enum-component.md §2.3) allows enum+component namespace merging.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const SOURCE: &str = r#"
enum CAP { X7R, MLCC, C0G }

component CAP (diel = X7R)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main
{
    io VDD
}
"#;

#[test]
fn enum_and_component_same_name_coexist() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/same-name-cap.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");

    // Both the enum and the component must be registered (no E0501 drop).
    let enum_cmie = mcc::get_def(&McIds::from("CAP"), &uri).expect("CAP definition missing");
    let comp = mcc::get_component_def(&McIds::from("CAP"), &uri)
        .expect("CAP component definition missing (E0501 suppressed both)");
    assert!(
        matches!(enum_cmie, mcc::McCMIE::Enum(_)),
        "CAP should resolve to the enum first"
    );
    assert!(
        matches!(comp, mcc::McCMIE::Component(_)),
        "get_component_def must return the component"
    );

    drop(lock);
}

#[test]
fn component_component_same_name_still_errors() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/dup-component.mc".to_string();
    // Two components with the same name must still trigger E0501 (501).
    let source = r#"
component DUP { pins = [ 1 = 1 ] }
component DUP { pins = [ 1 = 1 ] }

module main
{
    io VDD
}
"#;
    mcc::mcc_load_from_string(&uri, source);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);

    let has_501 = mcc::mcc_diagnose_all().iter().any(|d| d.code == 501);
    assert!(
        has_501,
        "duplicate component definitions must be reported as E0501"
    );

    drop(lock);
}
