// Copyright (c) 2026 MCode
//
// Integration tests for the authoritative-declared-shape rule (the replacement
// of the former scalar -> bus "usage auto-expansion" mechanism). A module port
// declared without members (`out spi1`) is a scalar and stays scalar: body
// member/lane access against it is an E3183 error (BUS_MEMBER_ON_SCALAR_PORT),
// never an implicit widening of the port. Membered ports (`io SPI{...}` /
// `io X[...]` / typed) declare their own authoritative member set; internal
// undeclared nets remain usage-defined and are unaffected.
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

/// Codes of all diagnostics currently in the global workspace.
fn diag_codes() -> Vec<u32> {
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

// ── Form 1: curly multi-member use no longer widens a scalar port ──────────

#[test]
fn curly_use_does_not_upgrade_scalar_port() {
    // `spi1` is declared scalar; the curly multi-member use is an E3183 error
    // and must NOT back-fill the port's bus_members.
    let inst = build(
        r#"
module main
{
    out spi1

    spi1{CS, SCLK, MOSI, MISO} -> [GND, GND, GND, GND]
}
"#,
    );
    let codes = diag_codes();
    assert!(
        codes.contains(&mcc::errcodes::BUS_MEMBER_ON_SCALAR_PORT),
        "E3183 not emitted for curly use of a scalar port; codes: {codes:?}"
    );
    assert_eq!(
        port_members(&inst, "spi1"),
        Vec::<String>::new(),
        "scalar port must not be widened by body usage"
    );
}

// ── Form 2: dotted member access no longer widens a scalar port ────────────

#[test]
fn dotted_use_does_not_upgrade_scalar_port() {
    // Four single-level dotted accesses each report E3183 (one per offending
    // reference) and never widen the port.
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
    let e3183: Vec<u32> = diag_codes()
        .iter()
        .copied()
        .filter(|&c| c == mcc::errcodes::BUS_MEMBER_ON_SCALAR_PORT)
        .collect();
    assert_eq!(
        e3183.len(),
        4,
        "expected one E3183 per dotted member reference; got {e3183:?}"
    );
    assert_eq!(
        port_members(&inst, "spi1"),
        Vec::<String>::new(),
        "scalar port must not be widened by body usage"
    );
}

// ── Form 3: vector connection no longer widens a scalar port ───────────────

#[test]
fn vector_connection_does_not_upgrade_scalar_port() {
    // `spi1` is a plain scalar operand; its sibling `spi{...}` is a
    // multi-member use of the scalar-declared `spi`, which is itself an
    // E3183. `spi1` must stay scalar (no back-prop from the sibling).
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
    let codes = diag_codes();
    assert!(
        codes.contains(&mcc::errcodes::BUS_MEMBER_ON_SCALAR_PORT),
        "E3183 not emitted for multi-member sibling of a scalar-declared port; codes: {codes:?}"
    );
    assert_eq!(
        port_members(&inst, "spi1"),
        Vec::<String>::new(),
        "scalar port must not be widened by the sibling vector"
    );
}

// ── Negative: scalar usage keeps the port scalar ──────────────────────────

#[test]
fn scalar_use_keeps_port_scalar() {
    // No member/lane-shaped usage in the body, so `spi1` stays a bare scalar
    // port and no E3183 fires.
    let inst = build(
        r#"
module main
{
    out spi1

    spi1 -> GND
}
"#,
    );
    let codes = diag_codes();
    assert!(
        !codes.contains(&mcc::errcodes::BUS_MEMBER_ON_SCALAR_PORT),
        "whole-port scalar use must not report E3183; codes: {codes:?}"
    );
    assert!(
        port_members(&inst, "spi1").is_empty(),
        "scalar port must not be upgraded"
    );
}
