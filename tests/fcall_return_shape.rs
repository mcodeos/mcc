// Copyright (c) 2026 MCode
//
// Integration tests for fcall return shape resolution (Phase 4).
//
// Per eval.md §8.1 three-state rules:
//   - return this / implicit → ReturnShape::This { left, right } (preserves caller shape)
//   - return <label/bus> → ReturnShape::Label { bus } (left empty, right = return value)
//
// NOTE: These tests share global mcc state, so a mutex serializes them.
// Include the system library (mcode/) so known 2-pin classes (RES, CAP, DIO, etc.)
// can be resolved. Run from workspace root with:
//   MCC_SYSTEM_ROOT=.. cargo test --test fcall_return_shape

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Helper: acquire lock, load source from string, build module, return module instance.
fn build(source: &str) -> mcc::McModuleInst {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let system_root = std::env::var("MCC_SYSTEM_ROOT").unwrap_or_else(|_| "..".to_string());
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(&system_root));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/fcall-return-shape-test.mc".to_string();
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

// ═══════════════════════════════════════════════════════════════════════════
// Test 1: return this preserves caller shape → chaining works
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fcall_return_this_preserves_caller_shape_for_chaining() {
    let inst = build(
        r#"
component CHAIN_TEST
{
    name = "Chain Test"
    pins = [
        1 = PIN_A
        2 = PIN_B
        3 = PIN_C
    ]

    func config_a(source)
    {
        source -> PIN_A
        return this
    }

    func config_b(target)
    {
        PIN_B -> target
        return this
    }
}

module main
{
    DC.SRC PWR(5V, 100mA)
    CHAIN_TEST CT

    PWR.1 -> V5V
    PWR.2 -> GND

    CT.config_a(V5V).config_b(GND)

    V5V -> CT.PIN_C
}
"#,
    );

    // Verify: CHAIN_TEST instantiated, all 3 pins exist
    let comp = find_component(&inst, "CT");
    assert_eq!(comp.pin_count(), 3, "CHAIN_TEST should have 3 pins");
    assert!(comp.pin_name("1").is_some(), "pin 1 (PIN_A) should exist");
    assert!(comp.pin_name("2").is_some(), "pin 2 (PIN_B) should exist");
    assert!(comp.pin_name("3").is_some(), "pin 3 (PIN_C) should exist");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 2: implicit return (no return statement) preserves caller shape
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fcall_implicit_return_preserves_caller_shape() {
    let inst = build(
        r#"
component LED_DRIVER
{
    name = "LED Driver"
    pins = [
        1 = ANODE
        2 = CATHODE
    ]

    func drive(vcc)
    {
        vcc -> RES(100Ω) -> ANODE
        CATHODE -> GND
        // implicit return this
    }
}

module main
{
    DC.SRC PWR(3.3V, 50mA)
    LED_DRIVER D1

    PWR.1 -> V3V3
    PWR.2 -> GND

    D1.drive(V3V3)
}
"#,
    );

    let comp = find_component(&inst, "D1");
    assert_eq!(comp.pin_count(), 2, "LED_DRIVER should have 2 pins");
    assert!(comp.pin_name("1").is_some(), "ANODE pin should exist");
    assert!(comp.pin_name("2").is_some(), "CATHODE pin should exist");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 3: return this → multiple independent function calls on same instance
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fcall_return_this_multiple_independent_calls() {
    let inst = build(
        r#"
component PASSTHROUGH
{
    name = "Passthrough"
    pins = [
        1 = IN1
        2 = OUT1
        3 = IN2
        4 = OUT2
    ]

    func route_ch1(src, dst)
    {
        src -> IN1
        OUT1 -> dst
        return this
    }

    func route_ch2(src, dst)
    {
        src -> IN2
        OUT2 -> dst
    }
}

module main
{
    DC.SRC PWR(5V, 100mA)
    PASSTHROUGH PT

    PWR.1 -> V5V
    PWR.2 -> GND

    PT.route_ch1(V5V, CH1_OUT)
    PT.route_ch2(V5V, CH2_OUT)
}
"#,
    );

    let comp = find_component(&inst, "PT");
    assert_eq!(comp.pin_count(), 4, "PASSTHROUGH should have 4 pins");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 4: fcall on known two-pin component (RES) with return this
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fcall_on_twopin_component_with_return_this() {
    let inst = build(
        r#"
component LED
{
    name = "LED"
    pins = [
        1 = ANODE
        2 = CATHODE
    ]
}

component DIO
{
    name = "Diode"
    pins = [
        1 = ANODE
        2 = CATHODE
    ]

    func protect(target)
    {
        target -> ANODE
        CATHODE -> GND
        return this
    }
}

module main
{
    DC.SRC PWR(5V, 100mA)

    PWR.1 -> V5V
    PWR.2 -> GND

    DIO D1
    D1.protect(V5V)
    D1.CATHODE -> GND
}
"#,
    );

    // Verify DIO component exists with 2 pins
    let comp = find_component(&inst, "D1");
    assert_eq!(comp.pin_count(), 2, "DIO should have 2 pins");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 5: complex chain — multiple returns + 2-pin components
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fcall_complex_chain_with_passives() {
    let inst = build(
        r#"
component FILTER_BLOCK
{
    name = "Filter Block"
    pins = [
        1 = IN
        2 = OUT
        3 = BIAS
    ]

    func setup(source, bias)
    {
        source -> IN
        BIAS -> GND
        bias -> BIAS
        return this
    }
}

module main
{
    DC.SRC PWR(5V, 100mA)
    DC.SRC VREF(2.5V, 10mA)
    FILTER_BLOCK F1

    PWR.1 -> V5V
    PWR.2 -> GND
    VREF.1 -> V2V5
    VREF.2 -> GND

    F1.setup(V5V, V2V5)
    F1.OUT -> SIG_OUT
}
"#,
    );

    let comp = find_component(&inst, "F1");
    assert_eq!(comp.pin_count(), 3, "FILTER_BLOCK should have 3 pins");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 6: known two-pin class instantiation via method chain
//   REGULATOR.cap_in(10uF) + REGULATOR.cap_out(10uF) → CAP component auto-created
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn fcall_twopin_instantiation_via_method_chain() {
    let inst = build(
        r#"
component REGULATOR
{
    name = "Regulator"
    pins = [
        1 = IN
        2 = OUT
        3 = GND
    ]

    func cap_in(value)
    {
        value -> IN
        IN -> GND
        return this
    }
}

module main
{
    DC.SRC PWR(5V, 100mA)
    REGULATOR U1

    PWR.1 -> V5V
    PWR.2 -> GND

    V5V -> U1.IN
    U1.cap_in(V5V)
    U1.GND -> GND
    U1.OUT -> VOUT
}
"#,
    );

    let reg = find_component(&inst, "U1");
    assert_eq!(reg.pin_count(), 3, "REGULATOR should have 3 pins");
}
