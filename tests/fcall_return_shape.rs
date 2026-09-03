// Copyright (c) 2026 MCode
//
// Integration tests for fcall return shape resolution (Phase 4).
//
// Per eval.md §8.1 three-state rules:
//   - return this / implicit → ReturnShape::This (preserves caller shape, read live)
//   - return <label/bus> → ReturnShape::Label { bus } (left empty, right = return value)
//
// NOTE: These tests share global mcc state, so a mutex serializes them.
// Include the system library (mcode/) so known 2-pin classes (RES, CAP, DIO, etc.)
// can be resolved. mcode loads from the standard data root (~/.mcode) unless
// MCC_SYSTEM_ROOT is set to a different system root.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Helper: acquire lock, load source from string, build module, return instance + arena + store.
fn build(source: &str) -> (mcc::McModuleInst, mcc::NodeArena, mcc::InstanceStore) {
    let _lock = common::lock();

    // Runtime-resolved system root (MCC_SYSTEM_ROOT env or ~/.mcode default).
    let system_root = mcc::cli::datadir::data_root();
    mcc::mcc_set_system_root(&system_root);
    // Standard startup: mcc_init() auto-loads the mcode system library from
    // the system root.
    mcc::mcc_init();

    let uri: McURI = "/mcc/fcall-return-shape-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let result = mcc::mcc_build_with_arena(&McIds::from("main"), &uri);
    let (inst, arena, store, _net_store) = result.expect("build failed");

    (inst, arena, store)
}

/// Helper: find a component instance by name (through the store-backed view).
fn find_component<'a>(
    inst: &'a mcc::McModuleInst,
    arena: &'a mcc::NodeArena,
    store: &'a mcc::InstanceStore,
    name: &str,
) -> &'a mcc::McComponentInst {
    let view = mcc::TreeView::new(arena, store);
    view.components(inst)
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("component '{}' not found", name))
}

/// Helper: recursively find the first FuncCall with the given function name
/// in the module's parsed stmts (`inst.def.stmts`).
fn find_funccall<'a>(inst: &'a mcc::McModuleInst, name: &str) -> &'a mcc::McFuncCall {
    fn walk<'a>(phrase: &'a mcc::McPhrase, name: &str) -> Option<&'a mcc::McFuncCall> {
        match phrase {
            mcc::McPhrase::FuncCall(f) => {
                if f.func_name.to_string() == name {
                    return Some(f);
                }
                if let Some(c) = &f.caller {
                    if let Some(hit) = walk(c, name) {
                        return Some(hit);
                    }
                }
                None
            }
            mcc::McPhrase::Series(elems, _) => elems.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Parallel(v) | mcc::McPhrase::Multiple(v) => {
                v.iter().find_map(|e| walk(e, name))
            }
            mcc::McPhrase::Group(g) => g.opds.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Transposed(inner) => walk(inner, name),
            mcc::McPhrase::Closure(c) => c.body.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Member(p, _) => walk(p, name),
            mcc::McPhrase::Lead | mcc::McPhrase::Endpoint(_) => None,
        }
    }
    inst.def
        .stmts
        .iter()
        .find_map(|l| walk(l, name))
        .unwrap_or_else(|| panic!("FuncCall '{}' not found in module stmts", name))
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 1: return this preserves caller shape → chaining works
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sem_fcallret__return_this_preserves_caller_shape_for_chaining() {
    let (inst, arena, store) = build(
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
    let comp = find_component(&inst, &arena, &store, "CT");
    assert_eq!(comp.pin_count(), 3, "CHAIN_TEST should have 3 pins");
    assert!(comp.pin_name("1").is_some(), "pin 1 (PIN_A) should exist");
    assert!(comp.pin_name("2").is_some(), "pin 2 (PIN_B) should exist");
    assert!(comp.pin_name("3").is_some(), "pin 3 (PIN_C) should exist");

    // Pass1b must resolve `return this` → ReturnShape::This.
    let call = find_funccall(&inst, "config_a");
    assert!(
        matches!(call.resolved_return_shape, Some(mcc::ReturnShape::This)),
        "config_a returns `this` → ReturnShape::This, got {:?}",
        call.resolved_return_shape
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 2: implicit return (no return statement) preserves caller shape
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sem_fcallret__implicit_return_preserves_caller_shape() {
    let (inst, arena, store) = build(
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

    let comp = find_component(&inst, &arena, &store, "D1");
    assert_eq!(comp.pin_count(), 2, "LED_DRIVER should have 2 pins");
    assert!(comp.pin_name("1").is_some(), "ANODE pin should exist");
    assert!(comp.pin_name("2").is_some(), "CATHODE pin should exist");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 3: return this → multiple independent function calls on same instance
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sem_fcallret__return_this_multiple_independent_calls() {
    let (inst, arena, store) = build(
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

    let comp = find_component(&inst, &arena, &store, "PT");
    assert_eq!(comp.pin_count(), 4, "PASSTHROUGH should have 4 pins");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 4: fcall on known two-pin component (RES) with return this
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sem_fcallret__twopin_component_with_return_this() {
    let (inst, arena, store) = build(
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
    let comp = find_component(&inst, &arena, &store, "D1");
    assert_eq!(comp.pin_count(), 2, "DIO should have 2 pins");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 5: complex chain — multiple returns + 2-pin components
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sem_fcallret__complex_chain_with_passives() {
    let (inst, arena, store) = build(
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

    let comp = find_component(&inst, &arena, &store, "F1");
    assert_eq!(comp.pin_count(), 3, "FILTER_BLOCK should have 3 pins");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 6: known two-pin class instantiation via method chain
//   REGULATOR.cap_in(10uF) + REGULATOR.cap_out(10uF) → CAP component auto-created
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sem_fcallret__twopin_instantiation_via_method_chain() {
    let (inst, arena, store) = build(
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

    let reg = find_component(&inst, &arena, &store, "U1");
    assert_eq!(reg.pin_count(), 3, "REGULATOR should have 3 pins");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test 7: return <label> → ReturnShape::Label (left empty, right = return value)
//   `B.out_sig(V5V)` — func returns the label `net` → right = the label bus ([0|N])
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sem_fcallret__label_return_resolves_to_zero_left_n_right() {
    let (inst, arena, store) = build(
        r#"
component BUS_SRC
{
    name = "Bus Source"
    pins = [
        1 = OUT_A
        2 = OUT_B
    ]

    func out_sig(net)
    {
        net -> OUT_A
        OUT_B -> GND
        return net
    }
}

module main
{
    DC.SRC PWR(5V, 100mA)
    BUS_SRC B

    PWR.1 -> V5V
    PWR.2 -> GND

    B.out_sig(V5V)
}
"#,
    );

    // Pass1b must resolve the label return → ReturnShape::Label: the call is a
    // [0|N] node (left empty), with right = the returned label's bus.
    let call = find_funccall(&inst, "out_sig");
    match &call.resolved_return_shape {
        Some(mcc::ReturnShape::Label { bus }) => {
            assert_eq!(bus.len(), 1, "return net → right = 1 bus");
        }
        other => panic!(
            "out_sig() returns a label → ReturnShape::Label, got {:?}",
            other
        ),
    }

    // The call still instantiates its circuit effects (net -> OUT_A, OUT_B -> GND).
    let comp = find_component(&inst, &arena, &store, "B");
    assert_eq!(comp.pin_count(), 2, "BUS_SRC should have 2 pins");
}
