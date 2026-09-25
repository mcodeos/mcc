// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E1 width binding + E2 three-way width consistency
//! (replicated-binding-design.md §4 checks 1–2, U289 ⑥).
//!
//! The judgment is the declared-formal discriminator (design §3): a dynamic
//! range name **not** declared in the formal table is a *width binder* — it
//! binds the binding row's instance subscript width (no subscript → 1); a
//! name inside arithmetic never back-solves and must be given explicitly
//! (E3186). When the caller DOES pass an explicit width, the subscript member
//! count, the expansion count, and the physical pin count must agree (E3187
//! guards the explicit leg; E3111 alone guards the no-parameter case).
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries a
//! member — including the silence branches (binder resolves quietly,
//! declared-formal arithmetic keeps working, the three-way-agrees case).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// A replicated interface with the width-echo formal REMOVED (the gpio.mc
/// migration shape): `n` is undeclared, so it is a width binder.
const XA: &str = r#"
interface XA()
{
    pins = [
        1:n = 1:n
    ]
}
"#;

/// A replicated interface that KEEPS the formal — the explicit-width leg of
/// E2 lives here, because `n` declared means `n` participates in computation.
const XB: &str = r#"
interface XB(n::INT)
{
    pins = [
        1:n = 1:n
    ]
}
"#;

/// Sorted diagnostic codes for one build.
fn codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/width-binding.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// Build and return the named component's pin count.
fn pin_count(src: &str, name: &str) -> usize {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/width-binding-count.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let result = mcc::mcc_build_with_arena(&McIds::from("main"), &uri);
    let (inst, arena, store, _net_store) = result.expect("build failed");
    let view = mcc::TreeView::new(&arena, &store);
    let count = view
        .components(&inst)
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("component '{name}' not found"))
        .pin_count();
    count
}

fn no_width_diags(codes: &[u32]) {
    assert!(
        !codes.contains(&3185) && !codes.contains(&3186) && !codes.contains(&3187),
        "the binder must resolve silently; got {codes:?}"
    );
}

// ── E1: the width binder ──

/// E1-1: `io [1,2] = XA[3,4]::XA()` — the subscript names 2 members, the
/// undeclared `n` binds that width, two members land, silent.
#[test]
fn width_binding__binder_binds_subscript_width() {
    let src = format!(
        "{XA}component BOARD\n{{\n    pins = [\n        io [1,2] = XA[3,4]::XA()\n    ]\n}}\n\nmodule main\n{{\n    BOARD b\n}}\n"
    );
    no_width_diags(&codes(&src));
}

/// E1-2: no subscript → the binder defaults to 1.
#[test]
fn width_binding__no_subscript_defaults_to_one() {
    let src = format!(
        "{XA}component BOARD\n{{\n    pins = [\n        io [1] = XA::XA()\n    ]\n}}\n\nmodule main\n{{\n    BOARD b\n}}\n"
    );
    no_width_diags(&codes(&src));
}

/// E1-3: `n` inside arithmetic never back-solves — the component face
/// reports E3186 (needs an explicit parameter), not the raw E3185.
#[test]
fn width_binding__expression_width_needs_explicit_param() {
    let src = r#"
component EXPRW()
{
    pins = [
        1:n*2 = 1:n*2
    ]
}

module main
{
    EXPRW u1
}
"#;
    let cs = codes(src);
    assert!(
        cs.contains(&3186),
        "an undeclared name inside arithmetic must ask for an explicit parameter; got {cs:?}"
    );
    assert!(
        !cs.contains(&3185),
        "the classified E3186 replaces the raw unbound-name diagnostic; got {cs:?}"
    );
}

/// E1-3 control: the bare-name binder at the component face (no subscript in
/// a module body → width 1) resolves quietly — one pin per instance.
#[test]
fn width_binding__bare_binder_component_face_resolves() {
    let src = r#"
component TP()
{
    pins = [
        1:n = 1:n
    ]
}

module main
{
    TP u1
}
"#;
    let cs = codes(src);
    no_width_diags(&cs);
    assert_eq!(pin_count(src, "u1"), 1, "no subscript → the binder binds 1");
}

/// E1-4: declared formals in arithmetic keep working (the HDR_MULTI shape) —
/// the binder machinery must not touch them (regression).
#[test]
fn width_binding__declared_formal_arithmetic_unaffected() {
    let src = r#"
component MULTI(rows::INT, cols::INT)
{
    pins = [
        1:rows*cols = 1:rows*cols
    ]
}

module main
{
    MULTI(2, 3) u1
}
"#;
    let cs = codes(src);
    no_width_diags(&cs);
    assert_eq!(pin_count(src, "u1"), 6, "rows*cols = 2*3");
}

/// E1-5: `TP[1:8]::TP()` — instance replication in the module body; each
/// replicated instance binds its own width 1 (the parse-time array is
/// flattened before instantiation), so no E3185 flood.
#[test]
fn width_binding__replicated_instances_bind_one_each() {
    let src = r#"
component TP()
{
    pins = [
        1:n = 1:n
    ]
}

module main
{
    TP[1:8]::TP()
}
"#;
    let cs = codes(src);
    no_width_diags(&cs);
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/width-binding-repl.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let result = mcc::mcc_build_with_arena(&McIds::from("main"), &uri);
    let (inst, arena, store, _net_store) = result.expect("build failed");
    let view = mcc::TreeView::new(&arena, &store);
    let comps: Vec<&mcc::McComponentInst> = view.components(&inst).collect();
    assert_eq!(comps.len(), 8, "8 replicated test points");
    for c in &comps {
        assert_eq!(c.pin_count(), 1, "each replicated instance carries 1 pin");
    }
}

// ── E2: three-way width consistency ──

/// E2-1: explicit parameter 2 vs 3 subscript members — the explicit leg
/// disagrees → E3187.
#[test]
fn width_binding__explicit_param_subscript_mismatch_fires() {
    let src = format!(
        "{XB}component BOARD\n{{\n    pins = [\n        io [1,2] = XB[3,4,5]::XB(2)\n    ]\n}}\n\nmodule main\n{{\n    BOARD b\n}}\n"
    );
    let cs = codes(&src);
    assert!(
        cs.contains(&3187),
        "explicit param 2 vs 3 subscript members must fire E3187; got {cs:?}"
    );
}

/// E2-2: 2 = 2 = 2 — the three-way-agrees silence branch.
#[test]
fn width_binding__three_way_agreement_is_silent() {
    let src = format!(
        "{XB}component BOARD\n{{\n    pins = [\n        io [1,2] = XB[3,4]::XB(2)\n    ]\n}}\n\nmodule main\n{{\n    BOARD b\n}}\n"
    );
    no_width_diags(&codes(&src));
}

/// E2-3: no explicit parameter → the binder makes the two legs constructively
/// equal, so only E3111 (LHS count vs member pool) judges — no E3187 double
/// report.
#[test]
fn width_binding__no_param_leg_owned_by_e3111_alone() {
    let src = format!(
        "{XA}component BOARD\n{{\n    pins = [\n        io [1] = XA[3,4]::XA()\n    ]\n}}\n\nmodule main\n{{\n    BOARD b\n}}\n"
    );
    let cs = codes(&src);
    assert!(
        cs.contains(&3111),
        "one LHS member vs two subscript members fires E3111; got {cs:?}"
    );
    assert!(
        !cs.contains(&3187),
        "without an explicit parameter E3187 stays out (the legs agree by construction); got {cs:?}"
    );
}
