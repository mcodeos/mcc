// Copyright (c) 2026 MCode
//
// Integration tests for P5.1: `_` wire three-usage classification (eval.md §1).
//
//   - Placeholder  `[_, R101]` : keeps position inside a vector; not a wire.
//   - Passthrough  `VEXT - _ - GND` : bridges a series chain (3-point net).
//   - PrefixId     `_OPEN` : a member name inside IDA indexes like
//                            `M[1:4][_LEFT,_RIGHT]`, NOT the wire `_`. Using it
//                            as a standalone operand warns (E4058).
//
// NOTE: These tests share global mcc state, so a mutex serializes them.

use mcc::errcodes::{FLOATING_PLACEHOLDER, LEAD_PREFIX_ID_AS_WIRE};
use mcc::{McDiagnostic, McIds, McModuleInst, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Acquire the global test lock.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Load source, build module `top`, return the module instance.
fn build(source: &str) -> McModuleInst {
    let _lock = lock();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/lead-classification.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    mcc::mcc_build(&McIds::from("top"), &uri).expect("build failed")
}

/// Load source, build module `top`, return all diagnostics (build errors tolerated).
fn build_diags(source: &str) -> Vec<McDiagnostic> {
    let _lock = lock();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/lead-classification.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    mcc::mcc_diagnose_all()
}

fn has_code(diags: &[McDiagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// Assert that a connection pair exists in either order.
fn assert_paired(got: &[(String, String)], a: &str, b: &str) {
    assert!(
        got.iter()
            .any(|(l, r)| { (l == a && r == b) || (l == b && r == a) }),
        "expected connection ({a}, {b}) among:\n  {got:?}"
    );
}

/// Collect all 2-point connection pairs as (left.path, right.path).
fn pairs(inst: &McModuleInst) -> Vec<(String, String)> {
    inst.connections
        .iter()
        .filter(|c| c.points.len() == 2)
        .map(|c| (c.points[0].path.clone(), c.points[1].path.clone()))
        .collect()
}

/// Find the first `(lead)_...` point path appearing in the given pairs.
fn lead_point(got: &[(String, String)]) -> Option<String> {
    got.iter()
        .flat_map(|(l, r)| [l.as_str(), r.as_str()])
        .find(|p| p.starts_with("(lead)_"))
        .map(|s| s.to_string())
}

// ── PrefixId: `_OPEN` used as a standalone operand → E4058 ────────────────

#[test]
fn prefix_id_as_wire_warns_on_first_use() {
    // `_OPEN` is a member-name style prefix identifier (like `M[1:4][_OPEN,...]`),
    // not the wire `_`. Using it as a standalone operand must warn.
    let diags = build_diags(
        r#"
module top
{
    _OPEN -> GND
}
"#,
    );
    assert!(
        has_code(&diags, LEAD_PREFIX_ID_AS_WIRE),
        "expected E4058 for `_OPEN` used as a wire, got: {:?}",
        diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
    );
}

#[test]
fn prefix_id_in_ida_index_members_does_not_warn() {
    // `_LEFT` / `_RIGHT` are legit member names inside `[1:4][_LEFT,_RIGHT]`
    // IDA indexes — no E4058 (the auto-created member labels are legal).
    let diags = build_diags(
        r#"
component HDR(n: Int)
{
}

module top
{
    HDR(4) SP[1:4] <- S[1:4][_LEFT,_RIGHT]
}
"#,
    );
    assert!(
        !has_code(&diags, LEAD_PREFIX_ID_AS_WIRE),
        "unexpected E4058 for IDA index member names, got: {:?}",
        diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
    );
}

#[test]
fn declared_prefix_id_label_does_not_warn() {
    // Once `_CLR` exists as a declared label, using it as an endpoint is legal.
    let diags = build_diags(
        r#"
module top
{
    Label _CLR
    _CLR -> GND
}
"#,
    );
    assert!(
        !has_code(&diags, LEAD_PREFIX_ID_AS_WIRE),
        "unexpected E4058 for declared label `_CLR`, got: {:?}",
        diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
    );
}

// ── Passthrough: `VEXT - _ - GND` bridges a series chain ──────────────────

#[test]
fn passthrough_lead_bridges_series_net() {
    // `_` as a standalone operand is a passthrough: the series chain becomes
    // VEXT ~ (lead)_x and (lead)_x ~ GND (the CLI merges these into a single
    // 3-point net view), with no floating-placeholder (E4054) and no
    // prefix-id (E4058) diagnostics.
    let inst = build(
        r#"
module top
{
    VEXT - _ - GND
}
"#,
    );
    let got = pairs(&inst);
    let lp = lead_point(&got).expect("expected a (lead)_ point in {got:?}");
    assert_paired(&got, "VEXT", &lp);
    assert_paired(&got, &lp, "GND");
    assert!(
        !has_code(&mcc::mcc_diagnose_all(), FLOATING_PLACEHOLDER),
        "passthrough `_` must not be reported as a floating placeholder"
    );
}

// ── Placeholder: `[_, ...]` vector member ─────────────────────────────────

#[test]
fn placeholder_lead_in_vector_does_not_warn() {
    // `_` inside `[...]` is a placeholder, not a wire — no E4058 and no
    // E4054 even when the surrounding net is valid.
    let diags = build_diags(
        r#"
module top
{
    VEXT -> [_, GND]
}
"#,
    );
    assert!(
        !has_code(&diags, LEAD_PREFIX_ID_AS_WIRE),
        "unexpected E4058 for placeholder `_`, got: {:?}",
        diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
    );
}
