// Copyright (c) 2026 MCode
//
// Integration tests for physical pin index access (`uC.pins[N]`), design gap P0.
//
// Covers:
//   §1 — single index:   `uC.pins[18] -> VDD` resolves to the physical pin `uC.18`
//   §2 — range index:    `uC.pins[3:5]` expands to pins `uC.3..uC.5`
//
// NOTE: bare dot forms (`uC.pins.1`, `uC.pins.VDD_3V3`) are rejected at parse
// time (E2082) — numeric index access is a documented bracket-only idiom
// (MCODE-AI-RULES.md §5.3), so they are out of scope here.
//
// These tests share global mcc state, so a mutex serializes them.
// Run with `cargo test --test pins_index_access` (no special flags needed).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Helper: acquire lock, load source, build module, return instance + arena + store.
fn build(source: &str) -> (mcc::McModuleInst, mcc::NodeArena, mcc::InstanceStore) {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/pins-index-access.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build_with_arena(&McIds::from("main"), &uri);
    let (inst, arena, store, _net_store) = result.expect("build failed");

    (inst, arena, store)
}

/// All net-point paths of the built module.
fn net_paths(inst: &mcc::McModuleInst) -> Vec<&str> {
    inst.connections
        .iter()
        .flat_map(|connection| connection.points.iter().map(|point| point.path.as_str()))
        .collect()
}

/// §1: single index — `uC.pins[18]` must resolve to the physical pin `uC.18`,
/// not a phantom `uC.pins18` (E3179).
#[test]
fn p0_pins_idx__single_index_resolves_to_physical_pin() {
    let (inst, _arena, _store) = build(
        r#"
component MCU
{
    pins = [
        1 = VDD_3V3
        2 = GND
        18 = VDD_CORE
        21 = GND_CORE
    ]
}

module main
{
    MCU uC
    VDD_CORE -> uC.pins[18]
    GND_CORE -> uC.pins[21]
}
"#,
    );

    let diags = mcc::mcc_diagnose_all();
    let e3179: Vec<_> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::COMPONENT_PIN_NOT_FOUND)
        .collect();
    assert!(
        e3179.is_empty(),
        "E3179 must not fire for `pins[N]` access: {:?}",
        e3179.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
    );

    let paths = net_paths(&inst);
    assert!(
        paths.contains(&"uC.18"),
        "`uC.pins[18]` must resolve to path `uC.18`, got: {paths:?}"
    );
    assert!(
        paths.contains(&"uC.21"),
        "`uC.pins[21]` must resolve to path `uC.21`, got: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.contains("pins")),
        "no phantom `pins`-prefixed paths allowed, got: {paths:?}"
    );
}

/// §2: range index — `uC.pins[3:5]` expands to the physical pins `uC.3..uC.5`.
#[test]
fn p0_pins_idx__range_index_expands_to_pins() {
    let (inst, _arena, _store) = build(
        r#"
component HDR
{
    pins = [
        1 = A1
        2 = A2
        3 = B1
        4 = B2
        5 = C1
        6 = C2
    ]
}

module main
{
    HDR uH
    uH.pins[3:5] -> [NET_B1, NET_B2, NET_C1]
}
"#,
    );

    let diags = mcc::mcc_diagnose_all();
    let e3179: Vec<_> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::COMPONENT_PIN_NOT_FOUND)
        .collect();
    assert!(
        e3179.is_empty(),
        "E3179 must not fire for `pins[N:M]` range: {:?}",
        e3179.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
    );

    let paths = net_paths(&inst);
    for pin in ["3", "4", "5"] {
        assert!(
            paths.contains(&format!("uH.{pin}").as_str()),
            "`uH.pins[3:5]` must produce path `uH.{pin}`, got: {paths:?}"
        );
    }
    assert!(
        !paths.iter().any(|p| p.contains("pins")),
        "no phantom `pins`-prefixed paths allowed, got: {paths:?}"
    );
}
