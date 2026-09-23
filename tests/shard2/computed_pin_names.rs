// Copyright (c) 2026 MCode
//
// U211: computed pin names — an expression in the pin-name slot resolves on
// the value engine instead of reporting "unsupported type". The grammar has
// always accepted the shape (`mc_pins_name: mc_phrase`); the walker now reads
// it. Branches covered:
//   ① a parameter-reading expression resolves per instantiation
//   ② a quantity parameter interpolates with the author's notation kept
//   ③ a parameter-free expression resolves at parse time
//   ④ a name expression whose parameters do not bind reports (E3185),
//     it does not drop the row silently
//   ⑤ the interface body accepts the shape (the parse-time half of ①)
//
// NOTE: these tests share global mcc state, so the common mutex serializes
// them. Run with `cargo test --test computed_pin_names`.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Helper: acquire lock, load source, build module, return instance + arena + store.
fn build(source: &str) -> (mcc::McModuleInst, mcc::NodeArena, mcc::InstanceStore) {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/computed-pin-names.mc".to_string();
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

/// ① The headline case: `1 = "P" + n` materializes pin 1 named by the bound
/// argument, and the computed name answers at the call site's name lookup.
#[test]
fn u211__text_expression_name_resolves_per_instantiation() {
    let (inst, arena, store) = build(
        r#"
component P(n::INT)
{
    pins = [
        1 = "P" + n, "computed supply"
        2 = GND, "return"
    ]
}

module main
{
    P(7) U1
    U1.P7 -> N1
    U1.2 -> N2
}
"#,
    );

    let comp = find_component(&inst, &arena, &store, "U1");
    assert_eq!(comp.pin_name("1").as_deref(), Some("P7"));
    // `U1.P7` netted clean (no pin-not-found diagnostic), so the computed
    // name is a first-class name key, not a display-only alias.
    assert_eq!(comp.pin_count(), 2);
}

/// ② A quantity parameter interpolates with the author's notation kept: the
/// dc.mc motivation shape (`"VCC" + volt` with `volt = 3.3V`).
#[test]
fn u211__quantity_parameter_interpolates_with_notation() {
    let (inst, arena, store) = build(
        r#"
component DCP(volt::UV.VOLT)
{
    pins = [
        1 = "VCC" + volt, "DC power positive", voltage:volt
        2 = GND, "DC power ground", voltage:0.0V
    ]
}

module main
{
    DCP(3.3V) U1
}
"#,
    );

    let comp = find_component(&inst, &arena, &store, "U1");
    assert_eq!(comp.pin_name("1").as_deref(), Some("VCC3.3V"));
    assert_eq!(comp.pin_name("2").as_deref(), Some("GND"));
}

/// ③ No parameter in the expression: it resolves right at parse time and the
/// row stays a static pin.
#[test]
fn u211__static_expression_name_registers() {
    let (inst, arena, store) = build(
        r#"
component STAT
{
    pins = [
        1 = "AB" + "CD", "literal concat"
        2 = GND, "return"
    ]
}

module main
{
    STAT U1
}
"#,
    );

    let comp = find_component(&inst, &arena, &store, "U1");
    assert_eq!(comp.pin_name("1").as_deref(), Some("ABCD"));
}

/// ④ A name expression reading a parameter that is not bound here reports
/// E3185 — the row no longer vanishes without a word.
#[test]
fn u211__unbound_name_expression_reports_not_silent() {
    let _lock = common::lock();
    common::reset();

    let uri = "/mcc/computed-pin-names-unbound.mc".to_string();
    common::load_string(
        &uri,
        r#"
component BADNAME(n::INT)
{
    pins = [
        1 = "P" + missing, "computed"
        2 = GND
    ]
}

module main
{
    BADNAME(7) U1
    U1.2 -> N2
}
"#,
    );

    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let diags = mcc::mcc_diagnose_all();
    assert!(
        diags
            .iter()
            .any(|d| d.code == mcc::errcodes::PIN_NAME_EXPR_UNRESOLVED),
        "an unresolvable name expression reports PIN_NAME_EXPR_UNRESOLVED; \
         got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    // …and the same code no longer fires for the well-formed sibling shape is
    // covered by the tests above (they build clean).
}

/// ⑤ The interface body — where dc.mc wants to converge — accepts the shape:
/// the parse half used to fire E3004 on the name expression.
#[test]
fn u211__interface_body_accepts_expression_name() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/computed-pin-names-iface.mc".to_string();
    mcc::mcc_load_from_string(
        &uri,
        r#"
interface DCP(volt::UV.VOLT)
{
    pins = [
        1 = "VCC" + volt, "DC power positive", voltage:volt
        2 = GND, "DC power ground", voltage:0.0V
    ]
}
"#,
    );

    let diags = mcc::mcc_diagnose(&uri);
    assert!(
        !diags
            .iter()
            .any(|d| d.code == mcc::errcodes::PIN_NAME_TYPE_UNSUPPORTED),
        "the interface body no longer reports E3004 for the name expression; \
         got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
}
