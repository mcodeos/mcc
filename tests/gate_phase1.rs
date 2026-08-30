// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase 1 entry gate (resolve-gate-design.md §1.3/§1.4, §5 items 21-23):
//! end-to-end behavior of the ghost-bus discriminator.
//!
//! pass: the base is a declared instance name in scope — a func-local
//! instance, a FuncCall caller label (B-family, e.g. `dTrigger`), or a module
//! caller label (`PL`) — so the reference keeps its ghost-bus and defers to
//! §3, no error.
//!
//! true miss (error): the base is declared nowhere — the phantom ghost-bus is
//! suppressed, the statement produces no net, and the component-finish
//! recheck errors E3182. A module-level `uC.ADC.P -> vdd` plus
//! `uC.ADC.P -> vss` must NOT short vdd~vss (both statements drop).

use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Diagnostic codes with duplicates preserved (occurrence counts matter: two
/// identical misses produce two diagnostics).
fn build_codes(src: &str) -> Vec<u32> {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/gate-phase1-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn b_family_same_func_caller_pass_no_error() {
    let lock = lock();

    // `dTrigger` is a FuncCall caller label (never in insts), referenced from
    // the same func's net statements — pass, no E3182, no dropped stmts.
    let src = "component D {\n    pins = [ 1 = VCC 2 = GND ]\n    func Cap() {}\n}\ncomponent T {\n    func main() {\n        D dTrigger.Cap()\n        VDD + dTrigger.VCC + dTrigger.D\n    }\n}\nmodule main {\n    io VDD\n}";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "B-family pass must not error E3182; got codes: {codes:?}"
    );

    drop(lock);
}

#[test]
fn module_level_true_miss_suppresses_short() {
    let lock = lock();

    // §5 item 23: `uC.ADC.P -> vdd` + `uC.ADC.P -> vss` with `uC` declared
    // nowhere must not short vdd~vss — both statements are dropped at parse
    // (E3132) and both error E3182 at the module-finish recheck. A single
    // shared ghost-bus `uC{ADC.P}` would have joined vdd to vss.
    let src = "component B {\n    pins = [\n        1 = VDD\n        2 = VSS\n    ]\n    func G() {}\n}\nmodule main {\n    io vdd\n    io vss\n    B b\n    b.VDD -> vdd\n    uC.ADC.P -> vdd\n    uC.ADC.P -> vss\n}";
    let codes = build_codes(src);
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == mcc::errcodes::INSTANCE_REF_UNDECLARED)
            .count(),
        2,
        "both uC.ADC.P references must error E3182; got codes: {codes:?}"
    );

    drop(lock);
}

#[test]
fn declared_base_member_access_is_untouched() {
    let lock = lock();

    // A base that resolves to a real instance (`b.VDD`) is unaffected by the
    // gate — no E3182, no E3132.
    let src = "component B {\n    pins = [ 1 = VDD 2 = VSS ]\n    func G() {}\n}\nmodule main {\n    io vdd\n    io vss\n    B b\n    b.VDD -> vdd\n}";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "declared-base member access must not error; got codes: {codes:?}"
    );

    drop(lock);
}

#[test]
fn four_gate_forms_each_error_once() {
    use mcc::ledger::{self, LedgerMode};

    let lock = lock();

    // Phase 2 convergence (§1.2②): the four gate-site shapes of an undeclared-base
    // reference all dispatch through the single `resolve_reference` entry, so
    // each produces exactly one E3182 (the finish recheck) plus exactly one
    // UnresolvedRef(error) ledger row at the shared "gate undeclared base"
    // site — and never a silent Fallback row.
    let cases: &[(&str, &str)] = &[
        // Site A: curly member list on an undeclared base (`NOPE{AAA, BBB}`).
        (
            "curly as_bus",
            "module main {\n    io VDD\n    func main() {\n        NOPE{AAA, BBB} -> VDD\n    }\n}",
        ),
        // Site B: two-segment dot chain (`MISSING.PIN`).
        (
            "two-segment dot chain",
            "module main {\n    io VDD\n    func main() {\n        MISSING.PIN -> VDD\n    }\n}",
        ),
        // Site C: single dotted token (`BASE.MEMBER`, MCAST_IDA path).
        (
            "single dotted token",
            "module main {\n    io VDD\n    func main() {\n        BASE.MEMBER -> VDD\n    }\n}",
        ),
        // Site D: multi-item Multiple with one dotted miss alongside a
        // declared item (`M1.PIN + VDD`).
        (
            "multi-item Multiple",
            "module main {\n    io VDD\n    func main() {\n        M1.PIN + VDD -> VDD\n    }\n}",
        ),
    ];
    for (name, src) in cases {
        ledger::clear();
        let codes = build_codes(src);
        let e3182 = codes
            .iter()
            .filter(|c| **c == mcc::errcodes::INSTANCE_REF_UNDECLARED)
            .count();
        assert_eq!(
            e3182, 1,
            "{name}: expected exactly one E3182 for the undeclared base; got codes: {codes:?}"
        );
        let report = ledger::build_report(LedgerMode::Audit);
        let ur: Vec<_> = report
            .detail
            .iter()
            .filter(|r| r.kind == "unresolved_ref")
            .collect();
        assert_eq!(
            ur.len(),
            1,
            "{name}: expected one UnresolvedRef row; got {ur:?}"
        );
        assert_eq!(
            ur[0].action, "error",
            "{name}: true-miss row action should be error"
        );
        assert_eq!(
            ur[0].site, "gate undeclared base (E3182)",
            "{name}: row site should be the shared gate site; got {:?}",
            ur[0].site
        );
        assert!(
            !report.detail.iter().any(|r| r.kind == "fallback"),
            "{name}: a true miss is not a silent Fallback ghost-bus; got {report:?}"
        );
    }

    drop(lock);
}
