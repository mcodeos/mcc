// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Failure ledger integration assertions (resolve-gate-design.md §5.32-33,
//! §7.1/§7.3): every silent-fallback recording point produces a ledger row of
//! the right kind, and legitimate (non-miss) constructs record nothing. The
//! ledger is observation-only — these tests never change resolution semantics.
//!
//! Fixtures mirror the recording sites added for the §1.2③ Fallback cut:
//! two-segment dot ghost-bus / member fall-through, `this.y.N` D9, group
//! shape-mismatch `<error:shape_mismatch>`, and the bare-miss Wire row.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use mcc::ledger::{self, LedgerMode};

/// Global mutex to serialize tests that share mcc's global workspace state
/// (the ledger is process-global too — each test clears it before building).
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Acquire the shared test lock, recovering from a prior test's panic (a
/// panicked assert poisons the mutex while unwinding).
fn lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Build `src` in a fresh workspace and return the emitted diagnostic codes,
/// leaving the failure ledger populated (cleared first) for the caller to
/// inspect.
fn build_codes(src: &str) -> HashSet<u32> {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    ledger::clear();
    let uri = "/mcc/failure-ledger-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// Rows of one kind in the ledger (audit mode — every kind listed).
fn rows(kind: &str) -> Vec<(String, String)> {
    let report = ledger::build_report(LedgerMode::Audit);
    report
        .detail
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| (r.form.clone(), r.site.clone()))
        .collect()
}

/// Rows of one kind with their action (audit mode) — for asserting the
/// error/warning/silent action of UnresolvedRef rows.
fn rows_with_action(kind: &str) -> Vec<(String, String, String)> {
    let report = ledger::build_report(LedgerMode::Audit);
    report
        .detail
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| (r.form.clone(), r.site.clone(), r.action.clone()))
        .collect()
}

#[test]
fn two_segment_dot_undeclared_base_inlines_ghost_bus() {
    let lock = lock();

    // `MISSING.PIN` — a two-segment dot access on a base declared nowhere.
    // relax-everything: the ghost-bus is kept and inlined (no E3182); the finish
    // recheck warns E3137 because the inline net is referenced exactly once.
    // One Fallback row records the ghost-bus; no UnresolvedRef rows remain.
    let src = "module main {\n    io VDD\n    func main() {\n        MISSING.PIN -> VDD\n    }\n}";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "E3182 is gone after relax-everything; got codes: {codes:?}"
    );
    assert!(
        codes.contains(&mcc::errcodes::SINGLE_USE_INLINE_NET),
        "single-use inline net warns E3137; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "a structured miss is a ghost-bus, not a floating Wire; got codes: {codes:?}"
    );
    let fb = rows("fallback");
    assert_eq!(
        fb.len(),
        1,
        "one Fallback row for the inlined ghost-bus, got {fb:?}"
    );
    assert!(
        fb[0].0.contains("MISSING"),
        "the Fallback form should name the reference, got {:?}",
        fb[0].0
    );
    let ur = rows_with_action("unresolved_ref");
    assert_eq!(ur.len(), 0, "no UnresolvedRef rows remain, got {ur:?}");

    drop(lock);
}

#[test]
fn late_declared_base_resolves_at_finish_no_error() {
    let lock = lock();

    // A forward reference to a func-call caller declared later in the same
    // component (`dTrigger.VCC` in func A, `dTrigger.Cap()` in func B) is a
    // true miss at parse time (the caller name is not yet visible in A's scope),
    // but it resolves at component-finish → §1.3 `resolved_late`, balanced in
    // the ledger, no E3182. The forward-reference statement itself is dropped
    // (produces no net) — that is the designed §1.3 parse-time suppression.
    let src = "component D {\n    pins = [ 1 = VCC 2 = GND ]\n    func Cap() {}\n}\ncomponent T {\n    func A() {\n        VDD + dTrigger.VCC\n    }\n    func B() {\n        D dTrigger.Cap()\n    }\n}\nmodule main {\n    io VDD\n}";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::INSTANCE_REF_UNDECLARED),
        "late-declared base must not error E3182; got codes: {codes:?}"
    );
    let report = ledger::build_report(LedgerMode::Audit);
    assert!(
        report.resolved_late > 0,
        "late-declared candidate must be balanced via resolved_late"
    );

    drop(lock);
}

#[test]
fn multi_segment_this_miss_records_fallback() {
    let lock = lock();

    // `this.y.2` — a multi-segment `this` access (2+ dot siblings) is the D9
    // silent fallback: the tail is dropped and `this.y` becomes a literal
    // label. Must record one Fallback row.
    let src = "component T {\n    pins = [ 1 = A ]\n    func main() {\n        this.y.2 -> A\n    }\n}\nmodule main { io VDD }";
    build_codes(src);
    let fb = rows("fallback");
    assert_eq!(fb.len(), 1, "expected one D9 this.y.N row, got {fb:?}");
    assert_eq!(fb[0].0, "this.y.2");
    assert!(
        fb[0].1.contains("this.y.N"),
        "site should name the D9 site, got {:?}",
        fb[0].1
    );

    drop(lock);
}

#[test]
fn single_segment_this_pin_transparency_does_not_record() {
    let lock = lock();

    // `this.ANODE` — a single-member `this` access is the legitimate
    // pin-transparency path (a component's own pin, resolved later). It must
    // NOT be recorded as a D9 failure.
    let src = "module main {\n    io VDD\n    func main() {\n        this.ANODE -> VDD\n    }\n}";
    build_codes(src);
    let fb = rows("fallback");
    assert!(
        !fb.iter()
            .any(|(f, s)| f.contains("this.") && s.contains("this.y.N")),
        "single-segment this must not record D9; got {fb:?}"
    );

    drop(lock);
}

#[test]
fn group_shape_mismatch_records_fallback() {
    let lock = lock();

    // `([GND, X], r1)` — a group whose branches have unequal widths (2×1 list
    // vs 1×1 module-level component instance) is silently absorbed into an
    // `<error:shape_mismatch>` placeholder. Must record Fallback rows.
    let src = "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io GND\n    R r1;\n    func main() {\n        ([GND, X], r1) -> GND\n    }\n}";
    build_codes(src);
    let fb = rows("fallback");
    assert!(
        fb.iter().any(|(_, s)| s.contains("shape_mismatch")),
        "expected a shape_mismatch Fallback row, got {fb:?}"
    );
    assert!(
        fb.iter().any(|(f, _)| f.contains("r1")),
        "the mismatch form should name the group, got {fb:?}"
    );

    drop(lock);
}

#[test]
fn bare_miss_records_wire_row() {
    let lock = lock();

    // A bare identifier referenced exactly once resolves to a floating net
    // label — the Wire kind (E3136 twin), not a Fallback.
    let src = "component FLT(pwr) {\n    func F(pwr) {\n        pwr -> DC\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 expected for a once-referenced dangling name; got codes: {codes:?}"
    );
    let wires = rows("wire");
    assert!(
        wires.iter().any(|(f, _)| f == "DC"),
        "expected a Wire row for DC, got {wires:?}"
    );

    drop(lock);
}

#[test]
fn sibling_func_declare_is_not_late_resolved() {
    let lock = lock();

    // `RES ra(10k)` in func `declare`, referenced as `ra` in sibling func
    // `setup` — the §7.1-1 "sibling-func late resolution" shape. The design
    // claimed the component-finish recheck (`floating.rs comp.find_inst`)
    // suppresses E3136 for this, but the recheck resolves component scope only
    // (params → enum → attrs → pins → insts → funcs): func-local declares live
    // in `func.insts`, invisible to it, so the name is NOT late-resolved. It is
    // recorded as one Wire row (refs=1) and E3136 still fires. `resolved_late`
    // therefore has no trigger in current code — it needs the §3 deferral infra.
    let src = "component RES {\n    pins = [\n        1 = A\n    ]\n    func declare() {\n        RES ra(10k)\n    }\n    func setup() {\n        ra -> A\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 expected: the recheck cannot resolve a sibling-func func-local declare; got codes: {codes:?}"
    );
    let wires = rows("wire");
    assert_eq!(
        wires.len(),
        1,
        "expected one Wire row for the sibling-func ref, got {wires:?}"
    );
    assert_eq!(wires[0].0, "ra");
    let report = ledger::build_report(LedgerMode::Audit);
    assert_eq!(
        report.resolved_late, 0,
        "no row may be marked resolved_late — the component-finish recheck never fires in current code"
    );

    drop(lock);
}

#[test]
fn clean_declared_net_records_nothing() {
    let lock = lock();

    // Everything resolves to something declared: no non-clean parse, no ledger
    // rows at all.
    let src = "component R {\n    pins = [\n        1 = A\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1 -> VDD\n    }\n}";
    let codes = build_codes(src);
    let report = ledger::build_report(LedgerMode::Audit);
    assert_eq!(
        report.total, 0,
        "clean project must record nothing; got total={} (codes {codes:?})",
        report.total
    );

    drop(lock);
}

#[test]
fn survived_counts_true_problems_only() {
    let lock = lock();

    // Two ghost-bus fallbacks (both survive) — survived equals total for
    // Fallback-only runs.
    let src = "module main {\n    io VDD\n    func main() {\n        MISSING.PIN -> VDD\n        OTHER.WIRE -> VDD\n    }\n}";
    build_codes(src);
    let report = ledger::build_report(LedgerMode::Audit);
    assert_eq!(report.total, 2);
    assert_eq!(report.survived, 2);
    assert_eq!(report.resolved_late, 0);

    drop(lock);
}

#[test]
fn component_pin_miss_records_unresolved_ref_error() {
    let lock = lock();

    // `r1.NOPIN` — a two-segment dot on a *declared* component whose member is
    // not a pin is a loud E3179 at parse time: base hit, member fails → the
    // statement is dropped and one UnresolvedRef row (action=error) records it.
    let src = "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1.NOPIN -> VDD\n    }\n}";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "E3179 expected for a member miss on a declared component; got codes: {codes:?}"
    );
    let ur = rows_with_action("unresolved_ref");
    assert_eq!(ur.len(), 1, "expected one unresolved_ref row, got {ur:?}");
    assert_eq!(ur[0].0, "r1.NOPIN");
    assert_eq!(
        ur[0].2, "error",
        "parse-time E3179 drops the statement → error action"
    );
    assert!(
        ur[0].1.contains("component pin not found"),
        "site should name the component-pin site, got {:?}",
        ur[0].1
    );

    drop(lock);
}

#[test]
fn curly_net_point_pin_miss_records_unresolved_ref_warning() {
    let lock = lock();

    // `r1.A{BAD}` — a curly member on a declared component that survives parse
    // and fails only at pass2 net-point validation is a *warning* E3179 (the
    // connection was kept). One UnresolvedRef row (action=warning) records it.
    let src = "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1.A{BAD} -> VDD\n    }\n}";
    build_codes(src);
    let ur = rows_with_action("unresolved_ref");
    assert_eq!(ur.len(), 1, "expected one unresolved_ref row, got {ur:?}");
    assert_eq!(ur[0].0, "r1.A.BAD");
    assert_eq!(
        ur[0].2, "warning",
        "pass2 net-point miss keeps the connection → warning"
    );
    assert!(
        ur[0].1.contains("pass2 net-point"),
        "site should name the pass2 net-point site, got {:?}",
        ur[0].1
    );

    drop(lock);
}

#[test]
fn undeclared_curly_iface_member_records_unresolved_ref_error() {
    let lock = lock();

    // `MISSING.IF{A, B}` — a curly interface-member access whose component base
    // is undeclared is a loud IFACE_CURLY_MEMBER_INVALID at parse time: base
    // miss → the statement is dropped and one UnresolvedRef row (action=error)
    // records it.
    let src =
        "module main {\n    io VDD\n    func main() {\n        MISSING.IF{A, B} -> VDD\n    }\n}";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::IFACE_CURLY_MEMBER_INVALID),
        "IFACE_CURLY_MEMBER_INVALID expected for an undeclared curly base; got codes: {codes:?}"
    );
    let ur = rows_with_action("unresolved_ref");
    assert_eq!(ur.len(), 1, "expected one unresolved_ref row, got {ur:?}");
    assert_eq!(ur[0].0, "MISSING.IF{A, B}");
    assert_eq!(
        ur[0].2, "error",
        "parse-time IFACE error drops the statement → error action"
    );
    assert!(
        ur[0].1.contains("interface curly member invalid"),
        "site should name the interface curly site, got {:?}",
        ur[0].1
    );

    drop(lock);
}
