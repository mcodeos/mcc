// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Dotted-numeric CMIE names (`component TTL.7400`) must register under their
// full name, not collapse to the identifier prefix (`TTL`).
//
// The grammar emits `mc_ids MCPT_DOT mc_int` with the `.int` as a SIBLING of
// the ids node, so `McIds::new` alone drops it. CMIE name extraction uses
// `McIds::new_with_dot` to append the sibling. Before the fix, every
// `TTL.NNNN` collapsed to `TTL`, so the first registered and the rest fired
// E1002 "Duplicate component" / E1051 "Definition already exists".

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

fn setup(uri: &str, source: &str) {
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), source);
}

#[test]
fn def_dotname__int_component_names_register_fully() {
    let _lock = common::lock();

    let uri = "/mcc/dotted-int-comp.mc";
    let source = r#"
component A.1
{
    pins = [ 1 = X, "x" ]
}
component A.2
{
    pins = [ 1 = Y, "y" ]
}
"#;
    setup(uri, source);

    // Neither A.1 nor A.2 may be reported as a duplicate.
    let diags = mcc::mcc_diagnose_all();
    assert!(
        !diags.iter().any(|d| d.code == 1002),
        "unexpected E1002 duplicate component: {:?}",
        diags
    );
    assert!(
        !diags.iter().any(|d| d.code == 1051),
        "unexpected E1051 definition already exists: {:?}",
        diags
    );

    // Both register under their full dotted-numeric names.
    let mut comps = mcc::mcc_get_components_in_file(&uri.to_string());
    comps.sort();
    assert_eq!(comps, vec!["A.1".to_string(), "A.2".to_string()]);
}

#[test]
fn def_dotname__ident_component_names_still_register_fully() {
    let _lock = common::lock();

    let uri = "/mcc/dotted-id-comp.mc";
    let source = r#"
component USB.MINI_B
{
    pins = [ 1 = X, "x" ]
}
"#;
    setup(uri, source);

    let mut comps = mcc::mcc_get_components_in_file(&uri.to_string());
    comps.sort();
    assert_eq!(comps, vec!["USB.MINI_B".to_string()]);
}
