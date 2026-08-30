// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase 1 entry gate (resolve-gate-design.md §1.3/§1.4, §5 items 21-23) —
//! relax-everything: end-to-end behavior of the inlined ghost-bus.
//!
//! pass: the base is a declared instance name in scope — a func-local
//! instance, a FuncCall caller label (B-family, e.g. `dTrigger`), or a module
//! caller label (`PL`) — so the reference keeps its ghost-bus and defers to
//! §3, no warning.
//!
//! true miss (relax-everything): the base is declared nowhere — the ghost-bus is kept
//! and inlined (the statement keeps its net; no E3182). The finish recheck
//! warns E3137 only when the inline net is referenced exactly once; a shared
//! net is left alone, and a module-level `uC.ADC.P -> vdd` plus
//! `uC.ADC.P -> vss` joins vdd~vss through the shared ghost net — netcheck R03
//! reports the short circuit as an ERROR.

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

/// Build flat (pass2 + netcheck) and return the diagnostic codes plus the
/// netcheck report — for asserting net-level findings like the R03 short.
fn build_flat_report(src: &str) -> (Vec<u32>, mcc::instant::netcheck::Report) {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/gate-phase1-flat.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) =
        mcc::mcc_build_flat(&mcc::McIds::from("main"), &uri, 1000).expect("flat build");
    let report = mcc::instant::netcheck::run(&table);
    let codes = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (codes, report)
}

/// R03 short-circuit findings (supply + ground in one net).
fn r03_findings(report: &mcc::instant::netcheck::Report) -> Vec<&mcc::instant::netcheck::Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.rule == "R03" && f.level == mcc::instant::netcheck::Level::Error)
        .collect()
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
fn module_level_true_miss_shorts_via_r03() {
    let lock = lock();

    // §5 item 23 (relax-everything): `uC.ADC.P -> vdd` + `uC.ADC.P -> vss` with `uC`
    // declared nowhere now INLINES the ghost-bus. The shared node joins the
    // vdd supply to the vss ground into one net, so the D1 short must surface
    // as an R03 ERROR at the net layer — never as the old E3182 (the gate is
    // gone) and never as E3137 (the net is referenced twice, so it is not
    // single-use).
    let src = "component B {\n    pins = [\n        1 = VDD\n        2 = VSS\n    ]\n    func G() {}\n}\nmodule main {\n    io vdd\n    io vss\n    B b\n    b.VDD -> vdd\n    uC.ADC.P -> vdd\n    uC.ADC.P -> vss\n}";
    let (codes, report) = build_flat_report(src);
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "no E3182 after relax-everything; got codes: {codes:?}"
    );
    let e3137 = codes
        .iter()
        .filter(|c| **c == mcc::errcodes::SINGLE_USE_INLINE_NET)
        .count();
    assert_eq!(
        e3137, 0,
        "a twice-referenced ghost net is not single-use; got codes: {codes:?}"
    );
    let shorts = r03_findings(&report);
    assert!(
        !shorts.is_empty(),
        "uC.ADC.P joining vdd~vss must be an R03 short-circuit; findings: {:?}",
        report.findings
    );

    drop(lock);
}

#[test]
fn multi_use_ghost_net_series_reuse_is_quiet() {
    let lock = lock();

    // §5 item 23 / uart2rs485.mc shape: `RS485.A` / `RS485.B` are bases declared
    // nowhere, but each is referenced twice across series chains through
    // passive parts (resistors / diodes). This is the legitimate series-reuse
    // the design must NOT flag: multi-use → no E3137; the rails are separated
    // by passives so no single net holds both a supply and a ground → no R03.
    let src = "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VCC\n    io GND\n    R R1\n    R R2\n    R R3\n    R R4\n    R R5\n    VCC - R1 - RS485.A - R3 - RS485.B - R2 - GND\n    RS485.A -> R4\n    RS485.B -> R5\n}";
    let (codes, report) = build_flat_report(src);
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "no E3182 after relax-everything; got codes: {codes:?}"
    );
    let e3137 = codes
        .iter()
        .filter(|c| **c == mcc::errcodes::SINGLE_USE_INLINE_NET)
        .count();
    assert_eq!(
        e3137, 0,
        "series reuse of a ghost net is not single-use; got codes: {codes:?}"
    );
    assert!(
        r03_findings(&report).is_empty(),
        "passive-separated series reuse must not be a rail short; findings: {:?}",
        report.findings
    );

    drop(lock);
}

#[test]
fn single_use_inline_ghost_net_warns_e3137() {
    let lock = lock();

    // A reference to an undeclared base used exactly once keeps its ghost-bus
    // but is almost certainly a typo/forgotten declaration → the dedicated
    // E3137 single-use warning (no E3182, no short).
    let src = "module main {\n    io VDD\n    func main() {\n        uC.ADC.P -> VDD\n    }\n}";
    let (codes, report) = build_flat_report(src);
    let e3137 = codes
        .iter()
        .filter(|c| **c == mcc::errcodes::SINGLE_USE_INLINE_NET)
        .count();
    assert_eq!(
        e3137, 1,
        "single-use inline ghost-net must warn E3137; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "no E3182; got codes: {codes:?}"
    );
    assert!(
        r03_findings(&report).is_empty(),
        "one rail alone cannot short; findings: {:?}",
        report.findings
    );

    drop(lock);
}

#[test]
fn declared_base_member_access_is_untouched() {
    let lock = lock();

    // A base that resolves to a real instance (`b.VDD`) is unaffected by the
    // gate — no E3182, no E3132, no E3137.
    let src = "component B {\n    pins = [ 1 = VDD 2 = VSS ]\n    func G() {}\n}\nmodule main {\n    io vdd\n    io vss\n    B b\n    b.VDD -> vdd\n}";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "declared-base member access must not error; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::SINGLE_USE_INLINE_NET),
        "a resolved base is not an inline ghost-net; got codes: {codes:?}"
    );

    drop(lock);
}

#[test]
fn four_gate_forms_each_warn_single_use() {
    use mcc::ledger::{self, LedgerMode};

    let lock = lock();

    // §1.2② (relax-everything): the four gate-site shapes of an undeclared-base
    // reference all dispatch through the single `resolve_reference` entry, so
    // each inlines exactly one ghost-bus: no E3182, exactly one E3137 (each is
    // single-use), one Fallback ledger row per ghost-bus, and no unresolved_ref
    // rows (the error gate is gone).
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
            e3182, 0,
            "{name}: no E3182 after relax-everything; got codes: {codes:?}"
        );
        let e3137 = codes
            .iter()
            .filter(|c| **c == mcc::errcodes::SINGLE_USE_INLINE_NET)
            .count();
        assert_eq!(
            e3137, 1,
            "{name}: single-use inline ghost-net warns E3137; got codes: {codes:?}"
        );
        let report = ledger::build_report(LedgerMode::Audit);
        let fb: Vec<_> = report
            .detail
            .iter()
            .filter(|r| r.kind == "fallback")
            .collect();
        assert_eq!(
            fb.len(),
            1,
            "{name}: one Fallback row per inlined ghost-bus; got {fb:?}"
        );
        assert!(
            fb[0].site.contains("ghost-bus"),
            "{name}: Fallback site should be an add_bus ghost-bus; got {:?}",
            fb[0].site
        );
        assert!(
            !report.detail.iter().any(|r| r.kind == "unresolved_ref"),
            "{name}: no unresolved_ref rows remain; got {report:?}"
        );
    }

    drop(lock);
}
