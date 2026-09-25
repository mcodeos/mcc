// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance-engine locks (U298 batch ③): one fixture per verdict branch of
//! [`super::run`], all judged on the real flat world (pass2, not a mock).

use super::{run, ExpectationReport, Verdict};
use crate::db::infra::init::MCC_TEST_PARSE_LOCK;
use crate::{definition_space, mcb_pass2_flat, mcc_load_from_string, McIds, McSpaceName, McURI};

/// The declared DC fixture: a sink pair carrying 3.3V, wired to a top port.
/// The dotted row `u1.1 = [low:..]` names one terminal of `u1`; its window is
/// judged on that terminal's net (the `::DC` declared value rides the same
/// `pin_declared_voltages` read the E4105 gate uses).
const GREEN: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    io RAW
    B u1
    RAW -> u1.1
    expects = [
        u1 = B
        RAW = driven
        RAW = [low:3.0V, high:3.5V]
        u1.1 = [low:3.0V, high:3.5V]
    ]
}
"#;

const MISMATCH: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    B u1
    expects = [
        u1 = LDO
    ]
}
"#;

const MISSING: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    B u1
    expects = [
        ghost = B
        nosuch = driven
    ]
}
"#;

const UNDRIVEN: &str = r#"
component R {
    pins = [
        1 = P
        2 = G
    ]
}
module main {
    io DEAD
    R r1
    r1.1 -> DEAD
    expects = [
        DEAD = driven
    ]
}
"#;

const OUT_OF_WINDOW: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    io RAW
    B u1
    RAW -> u1.1
    expects = [
        RAW = [low:3.4V, high:3.5V]
    ]
}
"#;

/// Parse `src`, build the flat world of `main`, judge its ledger.
fn judge(src: &str, uri_path: &str) -> ExpectationReport {
    // The C parser / workspace tables are process-global — hold the suite-wide
    // parse lock (init.rs), not a private one, or this races other tests.
    let _guard = MCC_TEST_PARSE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = crate::cli::datadir::data_root();
    crate::mcc_set_system_root(&root);
    crate::mcc_init();
    let uri: McURI = uri_path.to_string();
    mcc_load_from_string(&uri, src);
    let entry = McSpaceName {
        ident: McIds::from("main"),
        uri: crate::uri_intern(&uri),
    };
    let (_tree, table) = mcb_pass2_flat(&entry, 1).expect("pass2 flat failed");
    let (_, module) = definition_space()
        .workspace_modules()
        .into_iter()
        .find(|(sn, _)| sn.ident.to_string() == "main")
        .expect("module 'main' not registered");
    run(&table, &module.expects, &module.uri)
}

fn verdict_of(report: &ExpectationReport, target: &str) -> Verdict {
    report
        .outcomes
        .iter()
        .find(|o| o.target == target)
        .unwrap_or_else(|| panic!("no outcome for '{target}': {:?}", report.outcomes))
        .verdict
}

#[test]
fn green_rows_all_pass() {
    let report = judge(GREEN, "/mcc/expects-green-test.mc");
    assert_eq!(report.counts(), (4, 0, 0), "outcomes: {:?}", report.outcomes);
    assert_eq!(verdict_of(&report, "u1"), Verdict::Pass);
    assert_eq!(verdict_of(&report, "RAW"), Verdict::Pass);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn runtime_condition_rows_defer_without_diagnostics() {
    // `partno` names nothing here: the branch cannot be judged statically, so
    // its rows land deferred and the engine reports DEFER for them — never a
    // diagnostic (§5.1), and never E9001 for the target it cannot see.
    const CONDITIONAL: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    B u1
    if (partno == "X") {
        expects += [
            u1 = NOT_B
            ghost = B
        ]
    }
}
"#;
    let report = judge(CONDITIONAL, "/mcc/expects-conditional-test.mc");
    assert_eq!(report.counts(), (0, 0, 2), "outcomes: {:?}", report.outcomes);
    assert_eq!(verdict_of(&report, "u1"), Verdict::Defer);
    assert_eq!(verdict_of(&report, "ghost"), Verdict::Defer);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn missing_targets_report_9001() {
    let report = judge(MISSING, "/mcc/expects-missing-test.mc");
    assert_eq!(verdict_of(&report, "ghost"), Verdict::Fail);
    assert_eq!(verdict_of(&report, "nosuch"), Verdict::Fail);
    let codes: Vec<u32> = report.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![
            crate::errcodes::EXPECTATION_TARGET_MISSING,
            crate::errcodes::EXPECTATION_TARGET_MISSING
        ]
    );
}

#[test]
fn class_mismatch_reports_9002() {
    let report = judge(MISMATCH, "/mcc/expects-mismatch-test.mc");
    assert_eq!(verdict_of(&report, "u1"), Verdict::Fail);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == crate::errcodes::EXPECTATION_CLASS_MISMATCH));
}

#[test]
fn undriven_reports_9003() {
    let report = judge(UNDRIVEN, "/mcc/expects-undriven-test.mc");
    assert_eq!(verdict_of(&report, "DEAD"), Verdict::Fail);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == crate::errcodes::EXPECTATION_NOT_DRIVEN));
}

#[test]
fn out_of_window_reports_9004_warning() {
    let report = judge(OUT_OF_WINDOW, "/mcc/expects-window-test.mc");
    assert_eq!(verdict_of(&report, "RAW"), Verdict::Fail);
    let diag = report
        .diagnostics
        .iter()
        .find(|d| d.code == crate::errcodes::EXPECTATION_VALUE_OUT_OF_WINDOW)
        .expect("E9004 not emitted");
    assert!(matches!(diag.level, crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning));
}

#[test]
fn no_declared_value_defers_without_diagnostic() {
    // The `DEAD` net carries no declared DC fact; a window row on it must
    // defer, not fail — DEFER is a verdict, never a diagnostic (§5.1).
    const DEFER: &str = r#"
component R {
    pins = [
        1 = P
        2 = G
    ]
}
module main {
    io DEAD
    R r1
    r1.1 -> DEAD
    expects = [
        DEAD = [low:3.0V, high:3.5V]
    ]
}
"#;
    let report = judge(DEFER, "/mcc/expects-defer-window-test.mc");
    assert_eq!(verdict_of(&report, "DEAD"), Verdict::Defer);
    assert!(report.diagnostics.is_empty());
}
