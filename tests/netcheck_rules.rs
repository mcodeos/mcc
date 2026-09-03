// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! netcheck R-rule coverage locks (reorg doc §8.3/§8.5, netcheck R batch) —
//! one firing test per reachable rule of the `instant::netcheck` rule set,
//! built through `mcc_build_flat` (pass1 + pass2 + flatten) and asserted on
//! the returned `netcheck::Report` findings.
//!
//! Reachable from a self-contained top-level module and locked here:
//!
//! | rule | level | fixture essence | behavior |
//! |---|---|---|---|
//! | R02 | Error | two-terminal device with both pins on one rail | short circuit |
//! | R04 | Error | two bus members land on the same net | lane short |
//! | R05 | Error | unit-typed argument claims no formal slot | unresolved unit |
//! | R06 | Warn  | 10-point / 9-device non-power net | meganet |
//! | R07 | Error | net references a device under a real sub-module that is not registered | ghost device |
//! | R09 | Warn  | power pin left unconnected | floating power |
//! | R10 | Error | `run_with_expectation` device count mismatch | conservation |
//! | R14 | Warn  | instance registered but in no net | orphan |
//!
//! R03 (supply+ground short via an inlined ghost-bus) is covered by
//! `dlu_gate__module_level_true_miss_shorts_via_r03` in gate_phase1.rs and is
//! not duplicated here.
//!
//! Rules that cannot be fired from a self-contained top-level flatten are
//! recorded as **context-gated** in the reorg doc §8.3 rather than forced
//! fixtures: R01 (literal curly-bus point — the reference is rewritten before
//! netcheck sees a single-point literal), R08 (numeric-leaf phantom path under
//! a registered grandparent is reported as R07 unregistered-device, so R08 is
//! subsumed), R11 (split power rail — needs a module-boundary rail topology;
//! single-module rail joins collapse to R02 shorts or dissolve), R12
//! (dangling single-point port — same one-point-net precondition as E4115,
//! not producible from top-level wiring), R15 (visual/rendering context).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeMap;

use mcc::instant::netcheck::{Finding, Level, Report};

/// Build `main` flat and return the netcheck report (codes are not asserted
/// here — the R-rules live on the report, not in the diagnostic channel).
fn build_report(src: &str) -> Report {
    let _lock = common::lock();
    common::reset();
    mcc::instant::reset_r05_counter();
    let uri = "/mcc/netcheck-rules.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) =
        mcc::mcc_build_flat(&mcc::McIds::from("main"), &uri, 1000).expect("flat build");
    mcc::instant::netcheck::run(&table)
}

/// Same as [`build_report`] but with an explicit device-count expectation, so
/// R10 computes a real mismatch instead of reporting itself void.
fn build_report_expect(src: &str, expected: usize) -> Report {
    let _lock = common::lock();
    common::reset();
    mcc::instant::reset_r05_counter();
    let uri = "/mcc/netcheck-rules.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) =
        mcc::mcc_build_flat(&mcc::McIds::from("main"), &uri, 1000).expect("flat build");
    mcc::instant::netcheck::run_with_expectation(
        &table,
        &BTreeMap::from([("main".to_string(), expected)]),
    )
}

/// Findings of one rule at one level.
fn rule_findings<'a>(report: &'a Report, rule: &str, level: Level) -> Vec<&'a Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.rule == rule && f.level == level)
        .collect()
}

fn assert_fires(report: &Report, rule: &str, level: Level, what: &str) {
    let hits = rule_findings(report, rule, level);
    assert!(
        !hits.is_empty(),
        "{what}: expected {rule}:{level:?} to fire; findings: {:?}",
        report
            .findings
            .iter()
            .map(|f| format!("{}:{:?} {}", f.rule, f.level, f.detail))
            .collect::<Vec<_>>()
    );
}

const TWO_PIN: &str = "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n";
const ONE_IN: &str = "component C {\n    pins = [\n        in 1 = A\n    ]\n}\n";

/// ── R02 short circuit ───────────────────────────────────────────────────────
/// A two-terminal device with both pins on the `VDD` rail is a dead short.
#[test]
fn dlu_netcheck__r02_two_terminal_short_fires() {
    let src = format!(
        "{TWO_PIN}module main {{\n    io VDD\n    R r1\n    r1.1 -> VDD\n    r1.2 -> VDD\n}}"
    );
    let report = build_report(&src);
    let hits = rule_findings(&report, "R02", Level::Error);
    assert_eq!(
        hits.len(),
        1,
        "exactly one shorted device; findings: {:?}",
        report.findings
    );
    assert!(
        hits[0].detail.contains("short circuit"),
        "detail should say short circuit; got: {}",
        hits[0].detail
    );
}

/// ── R04 bus lane short ──────────────────────────────────────────────────────
/// Two members of the same declared bus (`MIC{P, N}`) land on one net `SIG`.
#[test]
fn dlu_netcheck__r04_bus_members_share_net_fires() {
    let src = format!("{ONE_IN}module main {{\n    io SIG\n    io MIC{{P, N}}\n    C c1\n    c1.A -> SIG\n    MIC.P -> SIG\n    MIC.N -> SIG\n}}");
    let report = build_report(&src);
    let hits = rule_findings(&report, "R04", Level::Error);
    assert_eq!(
        hits.len(),
        1,
        "exactly one shorted bus; findings: {:?}",
        report.findings
    );
    assert!(
        hits[0].detail.contains("same net"),
        "detail should name the shared net; got: {}",
        hits[0].detail
    );
}

/// ── R05 unresolved unit ──────────────────────────────────────────────────────
/// A positional unit-typed argument that claims no formal parameter slot. The
/// argument counter must be reset before the build so other modules in the
/// same process cannot pre-claim slots.
#[test]
fn dlu_netcheck__r05_unit_argument_claims_no_slot_fires() {
    let src = "component L(V::UV.VOLT) {\n    pins = [\n        1 = 1\n    ]\n}\nmodule main {\n    L l1(10mA)\n}\n";
    let report = build_report(&src);
    let hits = rule_findings(&report, "R05", Level::Error);
    assert!(
        !hits.is_empty(),
        "wrong-unit positional arg must fail slot claiming; findings: {:?}",
        report
            .findings
            .iter()
            .map(|f| format!("{}:{:?} {}", f.rule, f.level, f.detail))
            .collect::<Vec<_>>()
    );
}

/// ── R06 meganet ─────────────────────────────────────────────────────────────
/// Ten points spanning nine devices on one non-power net trips the size rule.
#[test]
fn dlu_netcheck__r06_large_non_power_net_fires() {
    let mut src = String::from(format!("{TWO_PIN}module main {{\n    io SIG\n"));
    for i in 1..=9 {
        src.push_str(&format!("    R r{i}\n"));
    }
    for i in 1..=9 {
        src.push_str(&format!("    r{i}.2 -> SIG\n"));
    }
    src.push_str("}\n");
    let report = build_report(&src);
    assert_fires(&report, "R06", Level::Warn, "10-point non-power net");
}

/// ── R07 unregistered device under a real sub-module ─────────────────────────
/// `sub1` resolves to a real module instance, but `sub1.missing.1` reaches a
/// device that was never registered under it.
#[test]
fn dlu_netcheck__r07_ghost_device_under_submodule_fires() {
    let src = "module sub {\n    io X\n}\nmodule main {\n    io VDD\n    sub sub1\n    sub1.missing.1 -> VDD\n}\n";
    let report = build_report(&src);
    let hits = rule_findings(&report, "R07", Level::Error);
    assert_eq!(
        hits.len(),
        1,
        "exactly one unregistered device; findings: {:?}",
        report.findings
    );
    assert!(
        hits[0].detail.contains("unregistered device"),
        "detail should name the missing device; got: {}",
        hits[0].detail
    );
}

/// ── R09 floating power pin ──────────────────────────────────────────────────
/// The device is in a net (its `SIG` pin is wired) but its `VDD` power pin is
/// not connected anywhere.
#[test]
fn dlu_netcheck__r09_unconnected_power_pin_fires() {
    let src = "component R {\n    pins = [\n        1 = VDD\n        2 = SIG\n    ]\n}\nmodule main {\n    io SIG\n    R r1\n    r1.2 -> SIG\n}\n";
    let report = build_report(&src);
    let hits = rule_findings(&report, "R09", Level::Warn);
    assert_eq!(
        hits.len(),
        1,
        "exactly one floating power pin; findings: {:?}",
        report.findings
    );
}

/// ── R10 device-count conservation ───────────────────────────────────────────
/// With an explicit expectation, the pass1/pass2 count mismatch is an Error.
/// (Without the expectation R10 only reports itself void — the Info finding
/// on every other fixture in this file.)
#[test]
fn dlu_netcheck__r10_conservation_mismatch_fires() {
    let src = format!("{ONE_IN}module main {{\n    C c1\n    C c2\n}}");
    let report = build_report_expect(&src, 9);
    assert_fires(
        &report,
        "R10",
        Level::Error,
        "device-count conservation mismatch",
    );
}

/// ── R14 orphan instance ─────────────────────────────────────────────────────
/// An instance that is registered but appears in no net.
#[test]
fn dlu_netcheck__r14_orphan_instance_fires() {
    let src = "component C {\n    pins = [\n        in 1 = A\n        in 2 = B\n    ]\n}\nmodule main {\n    C c1\n}\n";
    let report = build_report(&src);
    let hits = rule_findings(&report, "R14", Level::Warn);
    assert_eq!(
        hits.len(),
        1,
        "exactly one orphan; findings: {:?}",
        report.findings
    );
    assert!(
        hits[0].detail.contains("c1"),
        "detail should name the orphan; got: {}",
        hits[0].detail
    );
}
