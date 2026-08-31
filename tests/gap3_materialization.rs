// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §11.4 / vector-pipeline Phase 2.3: GAP3 — the flat-layer "two different
//! declarations materialize to the same physical pin id" check (E4062
//! PIN_OCCUPIED_BY_DECLARATION, fired from `InstTable::register`).
//!
//! Design §9.3.3 rates GAP3 lowest-priority / deferrable (design "can defer", heavily
//! overlapping 4051); the Phase 2.3 audit confirms that empirically — every
//! well-formed-MCode collision is absorbed by the pass1 declaration layer
//! (E5151 same-scope instance names, `insts` name-keyed dedup) before flatten,
//! so the GAP3 branch is dormant-by-construction for valid syntax. The check
//! converts the silent registration merge into an error should a structural
//! collision ever reach flatten, and the gate (BOTH sides structural, class
//! mismatch) is mathematically disjoint from its neighbors, so no net is
//! double-reported: GAP3 = pin DECLARATION occupancy; E5151 = same-scope
//! instance names; 4051 = per-connection net merge (build side, visit.rs);
//! 4053 = bus pin-group monotonicity (pass1, instref.rs).

use mcc::McIds;
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

/// Reset the mcc_* workspace for one test. The caller must hold `TEST_LOCK`.
fn reset_workspace() {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
}

/// Build `main` (pass1 + pass2 flat) and return all diagnostic codes, sorted.
/// The caller must hold `TEST_LOCK`.
fn build_codes(src: &str) -> Vec<u32> {
    let uri = "/mcc/gap3.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v
}

/// A bare `InstTable` on a fresh workspace with `current_uri` set (so
/// `diagnostic_log` can anchor GAP3's location). The caller must hold
/// `TEST_LOCK`.
fn fresh_table() -> mcc::InstTable {
    let uri = "/mcc/gap3-direct.mc".to_string();
    mcc::mcc_load_from_string(&uri, "module main {}");
    mcc::InstTable::new(1000)
}

// ── Mechanism: the flat-registration collision is the GAP3 trigger ─────────

/// Two Pin declarations claim the same flat path with different classes: the
/// second registration is merged into the first (same id returned) and GAP3
/// reports the physical-position preemption exactly once.
#[test]
fn structural_structural_different_class_reports_gap3_once() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let mut table = fresh_table();
    let first = table.register(
        "main.r1.1".to_string(),
        mcc::InstKind::Pin,
        None,
        "A".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    let second = table.register(
        "main.r1.1".to_string(),
        mcc::InstKind::Pin,
        None,
        "B".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    assert_eq!(
        first, second,
        "second registration is absorbed by the first"
    );
    assert_eq!(
        table.get_entry(first).map(|e| e.class_name.as_str()),
        Some("A"),
        "the first declaration keeps the entry"
    );
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        count(&codes, 4062),
        1,
        "GAP3 reports the preemption once; got {codes:?}"
    );
}

/// Cross-kind structural collision (Component claimed by a Module at the same
/// path) is the same flat-pin-preemption fact — both sides structural, classes
/// differ → GAP3 fires once, the old entry wins (priorities tie).
#[test]
fn cross_kind_structural_collision_reports_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let mut table = fresh_table();
    let first = table.register(
        "main.s".to_string(),
        mcc::InstKind::Component,
        None,
        "R".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    let second = table.register(
        "main.s".to_string(),
        mcc::InstKind::Module,
        None,
        "sub".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    assert_eq!(first, second, "the second structural claim is merged in");
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        count(&codes, 4062),
        1,
        "GAP3 fires for the cross-kind structural claim; got {codes:?}"
    );
}

/// Same-class re-registration is a normal dedup (the same declaration seen
/// twice, e.g. a func-created re-parent or a second net back-fill) — silent.
#[test]
fn same_class_reregistration_is_silent() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let mut table = fresh_table();
    let first = table.register(
        "main.r1.1".to_string(),
        mcc::InstKind::Pin,
        None,
        "A".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    let second = table.register(
        "main.r1.1".to_string(),
        mcc::InstKind::Pin,
        None,
        "A".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    assert_eq!(first, second);
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        count(&codes, 4062),
        0,
        "same declaration re-registered; got {codes:?}"
    );
}

/// Net side (Label/Bus/Port) claimed by a structural entity is the in-place
/// priority upgrade (Port→Component, "bug ①") — the pass1 declaration layer
/// owns that case (E5151 same-scope instance names), never GAP3.
#[test]
fn net_side_to_structural_seizure_is_not_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let mut table = fresh_table();
    let label_id = table.register(
        "main.r1".to_string(),
        mcc::InstKind::Label,
        None,
        String::new(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    let comp_id = table.register(
        "main.r1".to_string(),
        mcc::InstKind::Component,
        None,
        "R".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    assert_eq!(label_id, comp_id, "structural upgrade reuses the entry");
    assert_eq!(
        table.get_entry(comp_id).map(|e| e.kind.clone()),
        Some(mcc::InstKind::Component),
        "structural priority (2 > 1) upgrades the entry"
    );
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        count(&codes, 4062),
        0,
        "GAP3 gate requires both sides structural; got {codes:?}"
    );
}

/// Structural entity claimed by a net-side registration keeps the structural
/// entry (priority) — a normal no-op, silent.
#[test]
fn structural_keeps_entry_against_net_side_claim() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let mut table = fresh_table();
    let comp_id = table.register(
        "main.r1".to_string(),
        mcc::InstKind::Component,
        None,
        "R".to_string(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    let label_id = table.register(
        "main.r1".to_string(),
        mcc::InstKind::Label,
        None,
        String::new(),
        mcc::IOType::None,
        None,
        String::new(),
    );
    assert_eq!(comp_id, label_id);
    assert_eq!(
        table.get_entry(comp_id).map(|e| e.kind.clone()),
        Some(mcc::InstKind::Component),
        "old structural entry is kept"
    );
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        count(&codes, 4062),
        0,
        "net-side claim is discarded; got {codes:?}"
    );
}

// ── Domain split: GAP3 stays quiet where the neighbors own the fact ─────────

/// `io r1` + `R r1`: the flat registration upgrades Port→Component (not both
/// structural → GAP3 gate closed); E5151 owns the same-scope instance-name
/// duplicate. No double-report.
#[test]
fn same_scope_instance_name_collision_is_e5151_not_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "component R {\n    pins = [\n        1 = 1\n    ]\n}\nmodule main {\n    io r1\n    R r1\n    r1.1 -> VDD\n}";
    let codes = build_codes(src);
    assert_eq!(
        count(&codes, 5151),
        1,
        "E5151 owns the same-scope instance-name duplicate; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 4062),
        0,
        "GAP3 stays quiet (not both sides structural); got {codes:?}"
    );
}

/// `R r1` + `CAP r1` (different classes, same instance name): the pass1
/// `insts` name-keyed map dedups to one component, so flatten sees a single
/// registration; E5151 reports the duplicate. No GAP3.
#[test]
fn different_class_same_inst_name_is_e5151_not_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "component CAP {\n    pins = [\n        1 = 1\n    ]\n}\ncomponent R {\n    pins = [\n        1 = 1\n    ]\n}\nmodule main {\n    R r1\n    CAP r1\n    r1.1 -> VDD\n}";
    let codes = build_codes(src);
    assert_eq!(count(&codes, 5151), 1, "E5151 fires; got {codes:?}");
    assert_eq!(
        count(&codes, 4062),
        0,
        "flatten sees one registration; got {codes:?}"
    );
}

/// `[A, A] -> [GND, GND]`: two points in one connection resolve to the same
/// port id — a NET-layer merge (build side, visit.rs), reported as 4051. GAP3
/// is the pin DECLARATION occupancy, a disjoint fact → stays quiet.
#[test]
fn net_merge_is_4051_not_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "module main {\n    io A\n    io GND\n    [A, A] -> [GND, GND]\n}";
    let codes = build_codes(src);
    assert!(
        count(&codes, 4051) >= 1,
        "4051 reports the per-connection net merge; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 4062),
        0,
        "GAP3 stays quiet for a net-layer merge; got {codes:?}"
    );
}

/// Non-monotonic bus pin numbers (`io [5,2] = BUS{CLK, DATA}`) are the pass1
/// SORT_HAZARD (instref.rs) fact — member→pin mapping hazard, not a pin
/// preemption. No GAP3.
#[test]
fn sort_hazard_is_4053_not_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "component MyChip {\n    pins = [\n        io [5,2] = BUS{CLK, DATA}\n    ]\n}\nmodule main {\n    io CLK, DATA\n    MyChip chip\n    chip{CLK, DATA} -> (CLK, DATA)\n}";
    let codes = build_codes(src);
    assert_eq!(
        count(&codes, 4053),
        1,
        "4053 reports the non-monotonic bus pins; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 4062),
        0,
        "GAP3 stays quiet for the monotonicity hazard; got {codes:?}"
    );
}

/// A 0-pin materialization is GAP2 (E4057 NET_DROPPED_STATEMENT), never GAP3.
#[test]
fn zero_pin_net_is_4057_not_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "module main {\n    func main() {\n        res[1:2] -> led[3:4]\n    }\n}";
    let codes = build_codes(src);
    assert_eq!(
        count(&codes, 4057),
        1,
        "GAP2 reports the 0-pin drop; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 4062),
        0,
        "GAP3 stays quiet for a dropped statement; got {codes:?}"
    );
}

/// A legit net registers no structural collision and no net merge — quiet.
#[test]
fn legit_net_is_quiet_for_gap3() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "module main {\n    io A\n    io GND\n    A -> GND\n}";
    let codes = build_codes(src);
    assert_eq!(count(&codes, 4062), 0, "got {codes:?}");
    assert_eq!(count(&codes, 4051), 0, "got {codes:?}");
}
