// Copyright (c) 2026 MCode
//
// Integration tests for param-domain pin rows inside conditional branches
// (CIMP U230). A `1:cols = 1:cols` row inside an `if`/`else` branch of the
// component body must expand at instantiation when that branch is selected —
// the same semantics the row already has at the top level of the `pins`
// table (dynamic_pins path). A row the selected branch cannot resolve must
// report `PIN_NAME_EXPR_UNRESOLVED`, not drop silently.
//
// NOTE: These tests share global mcc state, so a mutex serializes them.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Helper: acquire lock, load source, build module, return instance + arena + store.
fn build(source: &str) -> (mcc::McModuleInst, mcc::NodeArena, mcc::InstanceStore) {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/cond-branch-dynamic-pins.mc".to_string();
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

/// U230 — the `if` branch is selected (`cols = 5`); its param-domain row must
/// expand to 5 pins exactly as the same row does at the pins-table top level.
#[test]
fn mat_condyn__selected_if_branch_param_row_expands() {
    let (inst, arena, store) = build(
        r#"
component HDR_COND(cols::INT)
{
    if cols == 5
        pins = [ 1:cols = 1:cols ]
    else
        pins = [ 1 = ONLY ]
}

module main
{
    HDR_COND(5) J1
    J1.1 -> NET_1
    J1.5 -> NET_5
}
"#,
    );

    let comp = find_component(&inst, &arena, &store, "J1");
    assert_eq!(
        comp.pin_count(),
        5,
        "selected branch's `1:cols` row must expand to 5 pins"
    );
    assert_eq!(comp.pin_name("1").as_deref(), Some("1"));
    assert_eq!(comp.pin_name("5").as_deref(), Some("5"));
}

/// U230 — the `else` branch is selected (`cols = 2`); same expansion duty.
#[test]
fn mat_condyn__selected_else_branch_param_row_expands() {
    let (inst, arena, store) = build(
        r#"
component HDR_COND(cols::INT)
{
    if cols == 5
        pins = [ 1 = ONLY ]
    else
        pins = [ 1:cols = 1:cols ]
}

module main
{
    HDR_COND(2) J1
    J1.1 -> NET_1
    J1.2 -> NET_2
}
"#,
    );

    let comp = find_component(&inst, &arena, &store, "J1");
    assert_eq!(
        comp.pin_count(),
        2,
        "selected else branch's `1:cols` row must expand to 2 pins"
    );
}

/// Control: a literal condition folds at the definition, so the branch's
/// param-domain row lands in the def's dynamic_pins and already expands.
/// The component carries a written default — a paramless chain never folds
/// (`foldable` requires defaults to evaluate against). This must keep
/// passing — the fix is scoped to the runtime-conditional path only.
#[test]
fn mat_condyn__folded_literal_branch_param_row_expands() {
    let (inst, arena, store) = build(
        r#"
component HDR_FOLDED(cols::INT = 3)
{
    if 1 == 1
        pins = [ 1:cols = 1:cols ]
}

module main
{
    HDR_FOLDED J1
    J1.1 -> NET_1
    J1.3 -> NET_3
}
"#,
    );

    let comp = find_component(&inst, &arena, &store, "J1");
    assert_eq!(
        comp.pin_count(),
        3,
        "folded branch's `1:cols` row must expand to 3 pins"
    );
}

/// U230 ruling ③ — a row the *selected* branch cannot resolve reports
/// `PIN_NAME_EXPR_UNRESOLVED` at instantiation, the same diagnostic the
/// top-level dynamic row raises (computed_pin_names' unbound name
/// expression); it must not vanish without a word.
#[test]
fn mat_condyn__unresolved_row_in_selected_branch_reports() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/cond-branch-dynamic-pins-unresolved.mc".to_string();
    mcc::mcc_load_from_string(
        &uri,
        r#"
component HDR_COND(sel::INT = 1)
{
    if sel == 1
        pins = [ 1 = "P" + missing, "computed" ]
}

module main
{
    HDR_COND(1) J1
    J1.1 -> NET_1
}
"#,
    );
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");

    let diags = mcc::mcc_diagnose_all();
    let hits: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::PIN_NAME_EXPR_UNRESOLVED)
        .collect();
    assert!(
        !hits.is_empty(),
        "selected branch's unresolvable name expression must report PIN_NAME_EXPR_UNRESOLVED; got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
