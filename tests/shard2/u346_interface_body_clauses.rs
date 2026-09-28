// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U346 — interface body build/check face locks.
//!
//! ① A clause kind that carries no interface semantics (the body grammar is
//! shared with component/module bodies, so `func`, … all parse) is reported
//! `INTERFACE_CLAUSE_UNSUPPORTED` (3083) instead of being dropped silently;
//! attrs/pins/conditional chains stay silent, and a `block` partition stays
//! transparent (its inner clauses are judged as if written in the body).
//!
//! ② Conditional pin chains materialize EVERY branch into the declaration
//! pin table (the union shape): the adoption site picks the branch its
//! parameter bindings answer, so the declaration must have registered them
//! all. The pre-fix face read a single branch and this fixture family
//! contributed zero pins.
//!
//! Like every shard2 file these tests share global mcc state;
//! `common::lock()` serializes them.

use crate::common;

const CODE: u32 = 3083;

const SUPPORTED_BODY: &str = r#"
component LDO2 {
    pins = [
        1 = VIN, "input"
        2 = VOUT, "output"
        3 = GND, "ground"
    ]
}

interface PWR_IF(sel) {
    topology = "point to point"
    pins = [
        1 = VIN, "input"
        2 = VOUT, "output"
    ]
    if (sel == 1) { pins += [3 = GND] } else { pins += [3 = PGND] }
    if (sel == 2) { pins += [4 = NC1] }
}

module main {
    ldo::LDO2()
}
"#;

fn unsupported_clause_diags(source: &str) -> Vec<mcc::McDiagnostic> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/u346-interface-body.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    mcc::mcc_diagnose_all()
        .into_iter()
        .filter(|d| d.code == CODE)
        .collect()
}

/// ①: a `func` clause inside an interface body reports 3083 exactly once.
#[test]
fn foreign_func_clause_reports_e3083() {
    let source = SUPPORTED_BODY.replace(
        "    if (sel == 1) { pins += [3 = GND] } else { pins += [3 = PGND] }",
        "    func drive(x) { }\n    if (sel == 1) { pins += [3 = GND] } else { pins += [3 = PGND] }",
    );
    let hits = unsupported_clause_diags(&source);
    assert_eq!(hits.len(), 1, "expected exactly one E{CODE}, got: {hits:?}");
    assert!(
        hits[0].msg.contains("interface body"),
        "diagnostic must name the interface body, got: {}",
        hits[0].msg
    );
}

/// ①: a partition is transparent grouping — a foreign clause inside it is
/// still a foreign clause of the interface body.
#[test]
fn partition_wrapped_foreign_clause_still_reports() {
    let source = SUPPORTED_BODY.replace(
        "    if (sel == 1) { pins += [3 = GND] } else { pins += [3 = PGND] }",
        "    block aux { func drive(x) { } }\n    if (sel == 1) { pins += [3 = GND] } else { pins += [3 = PGND] }",
    );
    let hits = unsupported_clause_diags(&source);
    assert_eq!(hits.len(), 1, "expected exactly one E{CODE}, got: {hits:?}");
}

/// ①: attrs/pins/conditional chains (the supported kinds) stay silent.
#[test]
fn supported_kinds_stay_silent() {
    let hits = unsupported_clause_diags(SUPPORTED_BODY);
    assert_eq!(hits.len(), 0, "supported kinds must not report: {hits:?}");
}

/// ②: every branch of every chain lands in the declaration pin table.
#[test]
fn cond_chains_materialize_all_branches() {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/u346-interface-body.mc".to_string();
    mcc::mcc_load_from_string(&uri, SUPPORTED_BODY);
    let def = mcc::get_kind_def(2, &mcc::McIds::from("PWR_IF"), &uri);
    let Some(mcc::McCMIE::Interface(iface)) = def else {
        panic!("interface def 'PWR_IF' not found");
    };
    let mut rows: Vec<(String, Vec<String>)> = iface
        .pins
        .pins
        .iter()
        .map(|(pid, pin)| {
            let mut names = pin.names.clone();
            names.sort();
            (pid.clone(), names)
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let want: Vec<(String, Vec<String>)> = vec![
        ("1".into(), vec!["VIN".into()]),
        ("2".into(), vec!["VOUT".into()]),
        // both arms of the first chain (the else branch included), and the
        // second chain's if-only branch
        ("3".into(), vec!["GND".into(), "PGND".into()]),
        ("4".into(), vec!["NC1".into()]),
    ];
    assert_eq!(rows, want, "interface pin table must be the branch union");
}
