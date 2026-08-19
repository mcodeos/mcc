// Copyright (c) 2026 MCode
//
// Integration tests for dynamic pin expansion (design doc §2.20).
//
// Covers:
//   §2.20.1 — parameter reference form:   `1:cols = 1:cols`
//   §2.20.2 — expression evaluation:       `1 : rows*cols = ...`
//   §2.20.3 — nested range expansion:      `R[1:rows]C[1:cols]`
//   §2.20.5 — static degenerate form:      `1:6 = R[1:2]C[1:3]`
//
// NOTE: These tests share global mcc state, so a mutex serializes them.
// Run with `cargo test --test dynamic_pin_expansion` (no special flags needed).

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

    let uri: McURI = "/mcc/dynamic-pin-expansion.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);

    drop(lock);
    result.expect("build failed")
}

/// Helper: find a component instance by name.
fn find_component<'a>(inst: &'a mcc::McModuleInst, name: &str) -> &'a mcc::McComponentInst {
    inst.components
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("component '{}' not found", name))
}

// ── §2.20.1: Parameter reference form ──────────────────────────────────────

#[test]
fn dynamic_pin_parameter_reference_expands() {
    let inst = build(
        r#"
component HDR_SINGLE(cols::INT)
{
    pins = [
        1:cols = 1:cols
    ]
}

module main
{
    HDR_SINGLE(5) J1
    J1.1 -> NET_1
    J1.5 -> NET_5
}
"#,
    );

    let comp = find_component(&inst, "J1");
    // cols=5 → pins 1..5 should exist
    assert_eq!(comp.pin_count(), 5, "expected 5 pins for cols=5");
    assert_eq!(comp.pin_name("1").as_deref(), Some("1"));
    assert_eq!(comp.pin_name("5").as_deref(), Some("5"));
}

// ── §2.20.5: Static degenerate form ────────────────────────────────────────

#[test]
fn static_nested_range_expands() {
    let inst = build(
        r#"
component HDR_2x3()
{
    pins = [
        1:6 = R[1:2]C[1:3]
    ]
}

module main
{
    HDR_2x3 J1
    J1.1 -> NET_R1C1
    J1.6 -> NET_R2C3
}
"#,
    );

    let comp = find_component(&inst, "J1");
    assert_eq!(comp.pin_count(), 6, "expected 6 pins for 2x3 header");
    // Pin names should follow R<row>C<col> pattern
    assert_eq!(comp.pin_name("1").as_deref(), Some("R1C1"));
    assert_eq!(comp.pin_name("2").as_deref(), Some("R1C2"));
    assert_eq!(comp.pin_name("3").as_deref(), Some("R1C3"));
    assert_eq!(comp.pin_name("4").as_deref(), Some("R2C1"));
    assert_eq!(comp.pin_name("5").as_deref(), Some("R2C2"));
    assert_eq!(comp.pin_name("6").as_deref(), Some("R2C3"));
}

// ── §2.20.2 + §2.20.3: Expression evaluation + nested range ───────────────

#[test]
fn dynamic_pin_expression_and_nested_range_expands() {
    let inst = build(
        r#"
component HDR_MULTI(rows::INT, cols::INT)
{
    pins = [
        1 : rows*cols = R[1:rows]C[1:cols]
    ]
}

module main
{
    HDR_MULTI(2, 3) J1
    J1.1 -> NET_R1C1
    J1.6 -> NET_R2C3
}
"#,
    );

    let comp = find_component(&inst, "J1");
    // rows=2, cols=3 → rows*cols=6 → pins 1..6
    assert_eq!(
        comp.pin_count(),
        6,
        "expected 6 pins for rows=2, cols=3 (rows*cols=6)"
    );
    // Nested range expansion: R1C1, R1C2, R1C3, R2C1, R2C2, R2C3
    assert_eq!(comp.pin_name("1").as_deref(), Some("R1C1"));
    assert_eq!(comp.pin_name("3").as_deref(), Some("R1C3"));
    assert_eq!(comp.pin_name("4").as_deref(), Some("R2C1"));
    assert_eq!(comp.pin_name("6").as_deref(), Some("R2C3"));
}

#[test]
fn dynamic_pin_expression_different_dimensions() {
    let inst = build(
        r#"
component HDR_MULTI(rows::INT, cols::INT)
{
    pins = [
        1 : rows*cols = R[1:rows]C[1:cols]
    ]
}

module main
{
    HDR_MULTI(3, 2) J1
    J1.1 -> NET_A
    J1.6 -> NET_B
}
"#,
    );

    let comp = find_component(&inst, "J1");
    // rows=3, cols=2 → rows*cols=6 → pins 1..6
    assert_eq!(comp.pin_count(), 6, "expected 6 pins for rows=3, cols=2");
    // R1C1, R1C2, R2C1, R2C2, R3C1, R3C2
    assert_eq!(comp.pin_name("1").as_deref(), Some("R1C1"));
    assert_eq!(comp.pin_name("2").as_deref(), Some("R1C2"));
    assert_eq!(comp.pin_name("3").as_deref(), Some("R2C1"));
    assert_eq!(comp.pin_name("4").as_deref(), Some("R2C2"));
    assert_eq!(comp.pin_name("5").as_deref(), Some("R3C1"));
    assert_eq!(comp.pin_name("6").as_deref(), Some("R3C2"));
}

// ─- §2.20.1: Parameter reference with different values ─────────────────────

#[test]
fn dynamic_pin_parameter_reference_single_pin() {
    let inst = build(
        r#"
component HDR_SINGLE(cols::INT)
{
    pins = [
        1:cols = 1:cols
    ]
}

module main
{
    HDR_SINGLE(1) J1
    J1.1 -> NET_1
}
"#,
    );

    let comp = find_component(&inst, "J1");
    assert_eq!(comp.pin_count(), 1, "expected 1 pin for cols=1");
    assert_eq!(comp.pin_name("1").as_deref(), Some("1"));
}

#[test]
fn dynamic_pin_parameter_reference_large() {
    let inst = build(
        r#"
component HDR_SINGLE(cols::INT)
{
    pins = [
        1:cols = 1:cols
    ]
}

module main
{
    HDR_SINGLE(20) J1
    J1.1 -> NET_1
    J1.20 -> NET_20
}
"#,
    );

    let comp = find_component(&inst, "J1");
    assert_eq!(comp.pin_count(), 20, "expected 20 pins for cols=20");
    assert_eq!(comp.pin_name("1").as_deref(), Some("1"));
    assert_eq!(comp.pin_name("20").as_deref(), Some("20"));
}

// ── Pin usage check: unused dynamic pins detected ──────────────────────────

/// Helper: build module, flatten to InstTable, run pin checks.
fn build_and_check_pins(source: &str) -> Vec<mcc::check::pins::PinCheckResult> {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/dynamic-pin-check.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let mod_name = mcc::mcb_get_module_name_by_uri(&uri)
        .or_else(|| mcc::mcb_get_first_module_name())
        .unwrap_or_else(|| "main".to_string());
    let entry = mcc::McSpaceName {
        ident: McIds::from(mod_name.as_str()),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");

    let results = mcc::check::pins::run_pin_checks(&table);
    drop(lock);
    results
}

#[test]
fn pin_check_detects_unused_dynamic_pins() {
    let results = build_and_check_pins(
        r#"
component HDR_SINGLE(cols::INT)
{
    pins = [
        1:cols = 1:cols
    ]
}

module main
{
    HDR_SINGLE(5) J1
    J1.1 -> NET_1
    // Pins 2-5 intentionally unconnected
}
"#,
    );

    let unused: Vec<&mcc::check::pins::PinCheckResult> =
        results.iter().filter(|r| r.check == "unused-pin").collect();
    assert_eq!(
        unused.len(),
        4,
        "expected 4 unused pins, got {}",
        unused.len()
    );
    let pinids: Vec<&str> = unused
        .iter()
        .map(|r| {
            // Extract pinid from message "Pin 'N' on ..."
            r.message.split('\'').nth(1).unwrap_or("")
        })
        .collect();
    assert!(
        pinids.contains(&"2"),
        "pin 2 should be unused: {:?}",
        pinids
    );
    assert!(
        pinids.contains(&"5"),
        "pin 5 should be unused: {:?}",
        pinids
    );
}

#[test]
fn pin_check_no_false_positives_when_all_connected() {
    let results = build_and_check_pins(
        r#"
component HDR_MULTI(rows::INT, cols::INT)
{
    pins = [
        1 : rows*cols = R[1:rows]C[1:cols]
    ]
}

module main
{
    HDR_MULTI(2, 2) J1
    J1.1 -> NET_R1C1
    J1.2 -> NET_R1C2
    J1.3 -> NET_R2C1
    J1.4 -> NET_R2C2
}
"#,
    );

    let unused: Vec<&mcc::check::pins::PinCheckResult> =
        results.iter().filter(|r| r.check == "unused-pin").collect();
    assert_eq!(unused.len(), 0, "expected 0 unused pins, got: {:?}", unused);
}

#[test]
fn pin_check_detects_unused_static_nested_range() {
    let results = build_and_check_pins(
        r#"
component HDR_2x3()
{
    pins = [
        1:6 = R[1:2]C[1:3]
    ]
}

module main
{
    HDR_2x3 J1
    J1.1 -> NET_R1C1
    J1.6 -> NET_R2C3
    // Pins 2-5 unconnected
}
"#,
    );

    let unused: Vec<&mcc::check::pins::PinCheckResult> =
        results.iter().filter(|r| r.check == "unused-pin").collect();
    assert_eq!(
        unused.len(),
        4,
        "expected 4 unused pins, got {}",
        unused.len()
    );
    // Verify pin names appear in messages for static nested range
    let has_r1c2 = results.iter().any(|r| r.message.contains("R1C2"));
    assert!(
        has_r1c2,
        "expected R1C2 in unused pin messages: {:?}",
        results
    );
}

// ── Interface dynamic pins with default parameters (§2.20 + IFACE) ─────────
//
// An interface whose only pins are dynamic (`1:count = 1:count`) with a
// default parameter (`count::INT = 1`) must resolve via the default when the
// call site passes no arguments. Previously `IO::IF_GPIO()` left `count`
// unbound, so `1:count` could not expand and the interface was treated as
// having no top-level pins (E3180 IFACE_NO_TOPLEVEL_PINS), even though the
// definition declared a usable default.

#[test]
fn interface_dynamic_pins_resolve_with_default_param() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/iface-default-dynamic-pins.mc".to_string();
    mcc::mcc_load_from_string(
        &uri,
        r#"
interface IF_GPIO(count::INT = 1, role = Controller)
{
    pins = [
        1:count = 1:count
    ]

    role Controller {
        name = "GPIO Controller"
        peer = Peripheral
    }

    role Peripheral {
        name = "GPIO Peripheral"
        peer = Controller
    }
}

component IF1_GPIO
{
    pins = [
        1 = IO::IF_GPIO()
    ]
}

module main
{
    IF1_GPIO u1
}
"#,
    );
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");

    let diags = mcc::mcc_diagnose_all();
    let iface_warnings: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::IFACE_NO_TOPLEVEL_PINS)
        .collect();
    assert!(
        iface_warnings.is_empty(),
        "E3180 fired for interface with default count: {:?}",
        iface_warnings
    );

    let cmie = mcc::get_def(&McIds::from("IF1_GPIO"), &uri).expect("IF1_GPIO not found");
    let mcc::McCMIE::Component(comp) = cmie else {
        panic!("IF1_GPIO is not a Component");
    };
    // count defaults to 1 -> pin 1 registered, member name from the dynamic line
    assert_eq!(comp.pins.count(), 1, "expected 1 pin for default count=1");
    assert!(
        comp.pins.pins.contains_key("1"),
        "pin 1 should be registered, got pins: {:?}",
        comp.pins.pins.keys().collect::<Vec<_>>()
    );
    drop(lock);
}
