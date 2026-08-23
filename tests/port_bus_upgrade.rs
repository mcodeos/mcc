// Copyright (c) 2026 MCode
//
// Integration tests for the scalar -> bus upgrade mechanism (shape by use,
// vec-dianlu.md §8.9.6.3 / §8.9.6.6 step 2). A module port declared as a
// single point (`out spi1`) is upgraded to a bus before instantiation when
// the module body uses it as one:
//   1. curly multi-member   `spi1{CS, SCLK, MOSI, MISO}`
//   2. dotted member access `spi1.CS`
//   5. vector connection    `spi1` as scalar operand with a >1-member sibling
//
// NOTE: These tests share global mcc state, so a mutex serializes them.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Helper: acquire lock, load source, build module, return instance.
fn build(source: &str) -> mcc::McModuleInst {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/port-bus-upgrade.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);

    drop(lock);
    result.expect("build failed")
}

/// bus_members of the named module port.
fn port_members(inst: &mcc::McModuleInst, name: &str) -> Vec<String> {
    inst.ports
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("port {name} not found"))
        .bus_members
        .clone()
}

// ── Form 1: curly multi-member use ────────────────────────────────────────

#[test]
fn curly_use_upgrades_scalar_port() {
    // `spi1` is declared as a single point; the body uses it as a 4-member
    // bus, so the port is upgraded with the curly members before
    // instantiation. The right side must be 4-wide too: `spi1{...} -> GND`
    // (4x1 vs 1x1) is intentionally rejected by the strict opcheck as a
    // single-point broadcast (no carve-out), so the fan-in shape is written
    // as an explicit 4-wide vector.
    let inst = build(
        r#"
module main
{
    out spi1

    spi1{CS, SCLK, MOSI, MISO} -> [GND, GND, GND, GND]
}
"#,
    );
    assert_eq!(
        port_members(&inst, "spi1"),
        vec!["CS", "SCLK", "MOSI", "MISO"]
    );
}

// ── Form 2: dotted member access ──────────────────────────────────────────

#[test]
fn dotted_use_upgrades_scalar_port() {
    // Four single-level dotted accesses upgrade the port; the union keeps
    // first-appearance order.
    let inst = build(
        r#"
module main
{
    out spi1

    spi1.CS -> GND
    spi1.SCLK -> GND
    spi1.MOSI -> GND
    spi1.MISO -> GND
}
"#,
    );
    assert_eq!(
        port_members(&inst, "spi1"),
        vec!["CS", "SCLK", "MOSI", "MISO"]
    );
}

// ── Form 5: vector connection with a >1-member sibling ────────────────────

#[test]
fn vector_connection_upgrades_scalar_port() {
    // `spi1` is a plain scalar operand whose sibling `spi{...}` is a
    // 4-member vector; the port is upgraded with the sibling's members.
    let inst = build(
        r#"
module main
{
    out spi1
    out spi

    spi1 -> spi{CS, SCLK, MOSI, MISO}
}
"#,
    );
    assert_eq!(
        port_members(&inst, "spi1"),
        vec!["CS", "SCLK", "MOSI", "MISO"]
    );
}

// ── Negative: scalar usage keeps the port scalar ──────────────────────────

#[test]
fn scalar_use_keeps_port_scalar() {
    // No bus-shaped usage in the body, so `spi1` stays a bare scalar port.
    let inst = build(
        r#"
module main
{
    out spi1

    spi1 -> GND
}
"#,
    );
    assert!(
        port_members(&inst, "spi1").is_empty(),
        "scalar port must not be upgraded"
    );
}
