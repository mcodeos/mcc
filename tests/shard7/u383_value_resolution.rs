// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U383 leg3 — spec-row value-face resolution at the instantiation door.
//!
//! The engine (b4474) evaluates quantity arithmetic; this locks the door that
//! carries it into `resolved_attrs` (value-computation-design.md §3 layer 3a):
//!
//! - a quantity result lands **typed** — `AttrLiteral(Uval)`, the exact shape
//!   a direct `36W` row already stores — so `pmax = vbus * imax` resolves to
//!   what `pmax = 36W` holds;
//! - a dimensionless ratio (W/W → bare scalar, ruling ①) lands as a float
//!   literal;
//! - sibling spec rows are names (`pmax` reads `vbus` and `imax` from the rows
//!   above it); instance params shadow;
//! - a refused expression (family law, no registered derived product) keeps
//!   its written form **and reports** — the survey §D face, where silence was
//!   the defect;
//! - an unbound name leaves the row as written, silently — an unbound `VCC`
//!   is not a refusal;
//! - the legacy text results (whole-number arithmetic, `+` concat) are
//!   byte-identical with the pre-leg3 door.
#![allow(non_snake_case)]

use crate::common;

const KEY: &str = "
component C {
    pins = [
        1 = A
        2 = B
    ]
";

const MAIN: &str = "
module main {
    C c1()
}
";

/// One build of `src`: every diagnostic code, plus each instance's resolved
/// attributes rendered the way the renderers print them.
fn probe(src: &str, uri: &str) -> (Vec<u32>, Vec<(String, Vec<String>)>) {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (tree, arena, store, _) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build");
    let diags: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    let view = mcc::TreeView::new(&arena, &store);
    let attrs: Vec<(String, Vec<String>)> = view
        .components(&tree)
        .map(|c| {
            (
                c.name.clone(),
                c.resolved_attrs.iter().map(|a| a.to_string()).collect(),
            )
        })
        .collect();
    (diags, attrs)
}

use mcc::McIds;

fn attrs_of(attrs: &[(String, Vec<String>)], inst: &str) -> Vec<String> {
    attrs
        .iter()
        .find(|(name, _)| name == inst)
        .map(|(_, a)| a.clone())
        .unwrap_or_default()
}

/// The quantity chain: `pmax = vbus * imax` reads its siblings and lands
/// typed — the same text a direct `36W` row produces.
#[test]
fn u383__quantity_chain_resolves_typed() {
    let src = format!(
        "{KEY}    spec = [\n        vbus = 12V\n        imax = 3A\n        pmax = vbus * imax\n        direct = 36W\n    ]\n}}\n{MAIN}\n"
    );
    let (diags, attrs) = probe(&src, "/mcc/u383-quantity-chain.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    // Both rows are the same typed storage; the derived row displays in the
    // canonical scale (no author raw to echo), the direct row echoes `36W`.
    assert!(rows.contains("pmax = 36.00W"), "{rows}");
    assert!(rows.contains("direct = 36W"), "{rows}");
}

/// Same-family division is the bare scalar (ruling ①): `W/W` lands as a
/// float literal, and the paren chain resolves through it. The rows use
/// binary-exact operands so the f64 text is clean; operands like `11.4`
/// (not representable) display their honest float digits.
#[test]
fn u383__ratio_is_a_bare_scalar() {
    let src = format!(
        "{KEY}    spec = [\n        vbus = 12V\n        pmax = 36W\n        margin = (12V - 11V) / 1V\n        margin2 = pmax / 40W\n    ]\n}}\n{MAIN}\n"
    );
    let (diags, attrs) = probe(&src, "/mcc/u383-ratio.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    assert!(rows.contains("margin = 1"), "{rows}");
    assert!(rows.contains("margin2 = 0.9"), "{rows}");
}

/// The composite chain mirrors the U370 parse law: `50mV/A * 3A` cancels the
/// denominator and yields the numerator family.
#[test]
fn u383__composite_cancels_to_the_numerator() {
    let src = format!(
        "{KEY}    spec = [\n        imax = 3A\n        idroop = 50mV/A\n        vdroop = idroop * imax\n    ]\n}}\n{MAIN}\n"
    );
    let (diags, attrs) = probe(&src, "/mcc/u383-composite.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    // The computed value carries no author raw, so it displays in the
    // numerator family's base scale: 150mV canonicalizes to 0.150V.
    assert!(rows.contains("vdroop = 0.150V"), "{rows}");
}

/// The §D refusals report and keep their written form: cross-family add,
/// affine-Temp add, dB scaling, and the unregistered product.
#[test]
fn u383__section_D_refusals_report() {
    let src = format!(
        "{KEY}    spec = [\n        a1 = 12V + 3A\n        a2 = 25°C + 5°C\n        a3 = 10dB * 2\n        a4 = 3Hz * 1V\n    ]\n}}\n{MAIN}\n"
    );
    let (diags, attrs) = probe(&src, "/mcc/u383-refusals.mc");
    assert!(
        diags.contains(&mcc::errcodes::EVAL_OPERAND_NOT_NUMERIC),
        "family-law rows must report: {diags:?}"
    );
    assert!(
        diags.contains(&mcc::errcodes::EVAL_NO_DERIVED_FAMILY),
        "the unregistered product must report: {diags:?}"
    );
    // Every refused row keeps the form it was written in.
    let rows = attrs_of(&attrs, "c1").join("\n");
    assert!(rows.contains("a1 = 12V + 3A"), "{rows}");
    assert!(rows.contains("a4 = 3Hz * 1V"), "{rows}");
}

/// An unbound name is not a refusal: the row stays as written, silently.
#[test]
fn u383__unbound_name_stays_silent_raw() {
    let src = format!(
        "{KEY}    spec = [\n        w = 0.3 * VCC\n    ]\n}}\n{MAIN}\n"
    );
    let (diags, attrs) = probe(&src, "/mcc/u383-unbound.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    assert!(rows.contains("w = 0.3 * VCC"), "{rows}");
}

/// The legacy text results are byte-identical: whole-number arithmetic reads
/// back as its quoted integer text (the string literal it always was), `+`
/// still concatenates without a separator, and a call-site-bound number
/// scales a quantity.
#[test]
fn u383__legacy_text_faces_unchanged() {
    // `gain` must be a declared parameter (an undeclared call-site key is
    // E4176), so the header carries it with a default.
    let src = format!(
        "component C (gain = 1.0) {{\n    pins = [\n        1 = A\n        2 = B\n    ]\n    spec = [\n        n = 2 * 3\n        label = \"cols: \" + \"unit\"\n        scaled = 10V * gain\n    ]\n}}\nmodule main {{\n    C c1( gain = 2.5 )\n}}\n"
    );
    let (diags, attrs) = probe(&src, "/mcc/u383-legacy.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    // The pre-leg3 door stored these as string literals, quotes included —
    // that is the byte-identical promise.
    assert!(rows.contains("n = \"6\""), "{rows}");
    assert!(rows.contains("label = \"cols: unit\""), "{rows}");
    // `gain` binds as text "2.5"; 10V * 2.5 is a quantity times a scalar → 25V.
    assert!(rows.contains("scaled = 25.00V"), "{rows}");
}

// ---------------------------------------------------------------------------
// U383 leg4b — body-local `let` bindings and per-instance `require` judgements
// (b4483 rulings ① and ②). The judgement rides the conds chain with the U39
// deferral: undecided is not violated.

/// Diagnostics only — the module-body face has no resolved-attrs render to
/// read; the judgement products are diagnostics (or their absence).
fn probe_diags(src: &str, uri: &str, top: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (_, arena, store, _) =
        mcc::mcc_build_with_arena(&McIds::from(top), &uri).expect("build");
    let _ = (&arena, &store);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// The exemplar §B chain: lets bind in written order, a later let reads an
/// earlier one, and the satisfied require stays silent.
#[test]
fn u383__let_chain_satisfied_require_is_silent() {
    let src = "module main {\n\
               \x20   let v      = 3.3V\n\
               \x20   let i_load = 500mA\n\
               \x20   let p      = v * i_load\n\
               \x20   let p_peak = p * 2\n\
               \x20   require p_peak <= 5W\n\
               }\n";
    let diags = probe_diags(src, "/mcc/u383-let-chain.mc", "main");
    assert!(diags.is_empty(), "a satisfied require must stay silent: {diags:?}");
}

/// The violated require reports E5463 at its row.
#[test]
fn u383__violated_require_reports() {
    let src = "module main {\n\
               \x20   let p = 9.9W\n\
               \x20   require p <= 5W\n\
               }\n";
    let diags = probe_diags(src, "/mcc/u383-require-fail.mc", "main");
    assert!(
        diags.contains(&mcc::errcodes::MODULE_REQUIRE_UNSATISFIED),
        "a false judge must report: {diags:?}"
    );
}

/// Undecided is not violated (the U39 deferral): a `_`-valued binding keeps
/// the require silent, and so does an unbound header formal.
#[test]
fn u383__undecided_requires_stay_silent() {
    let src = "module main (src) {\n\
               \x20   let mystery = _\n\
               \x20   let p = mystery * 3A\n\
               \x20   require p <= 10W\n\
               \x20   require src <= 3\n\
               }\n";
    let diags = probe_diags(src, "/mcc/u383-require-undef.mc", "main");
    assert!(diags.is_empty(), "undecided judges must defer silently: {diags:?}");
}

/// The judgement is per instance (ruling ②): two instances of the same
/// module, one inside and one outside the bound — the report is E5463 and
/// the same written row does not duplicate it.
#[test]
fn u383__judgement_is_per_instance() {
    let pass = "module TOP {\n    LIM(lim = 0.5A)\n}\n\
                module LIM (lim) {\n    let p = lim * 5V\n    require p <= 5W\n}\n";
    let diags = probe_diags(pass, "/mcc/u383-per-inst-pass.mc", "TOP");
    assert!(diags.is_empty(), "0.5A * 5V = 2.5W satisfies: {diags:?}");

    let fail = "module TOP {\n    LIM(lim = 2A)\n    LIM(lm2, lim = 3A)\n}\n\
                module LIM (lim) {\n    let p = lim * 5V\n    require p <= 5W\n}\n";
    let diags = probe_diags(fail, "/mcc/u383-per-inst-fail.mc", "TOP");
    assert_eq!(
        diags.iter().filter(|c| **c == mcc::errcodes::MODULE_REQUIRE_UNSATISFIED).count(),
        1,
        "both instances violate the same written row; the row reports once: {diags:?}"
    );
}

/// The let-row refusal face mirrors leg3: an unregistered product reports
/// E5417 at the let row, the name stays unbound, and the require reading it
/// defers silently.
#[test]
fn u383__let_refusal_reports_and_defers_the_require() {
    let src = "module main {\n\
               \x20   let bad = 1A * 1A\n\
               \x20   require bad <= 3W\n\
               }\n";
    let diags = probe_diags(src, "/mcc/u383-let-refusal.mc", "main");
    assert!(
        diags.contains(&mcc::errcodes::EVAL_NO_DERIVED_FAMILY),
        "the unregistered product reports at the let row: {diags:?}"
    );
    assert!(
        !diags.contains(&mcc::errcodes::MODULE_REQUIRE_UNSATISFIED),
        "the require reading an unbound let defers: {diags:?}"
    );
}

// U383 leg4c — value functions.
//
// A return carrying `*`/`/` parses as a computation (`McFuncReturn::Value`),
// not a connection: the formals bind positionally to the evaluated arguments
// and the stored expression evaluates per instance at the value-face call
// site. A call that names no value function keeps its written form, silently.

/// Quantity-in, quantity-out: `Drop(20mA, 100R)` lands typed — the same
/// storage a direct `2V` row holds (canon §7.4 note 2 flipped).
#[test]
fn u383__value_func_quantity_out() {
    let src = "component C {\n\
               \x20   pins = [\n\
               \x20       1 = A\n\
               \x20       2 = B\n\
               \x20   ]\n\
               \x20   func Drop(i, r) { return r * i }\n\
               \x20   spec = [\n\
               \x20       vdrop = Drop(20mA, 100R)\n\
               \x20       direct = 2V\n\
               \x20   ]\n\
               }\n\
               module main {\n\
               \x20   C c1()\n\
               }\n";
    let (diags, attrs) = probe(&src, "/mcc/u383-value-func-quantity.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    assert!(rows.contains("vdrop = 2.00V"), "{rows}");
    assert!(rows.contains("direct = 2V"), "{rows}");
}

/// A dimensionless ratio is the bare scalar (leg3 ruling ① carried through
/// the call), and the arguments read the sibling spec rows by name.
#[test]
fn u383__value_func_ratio_reads_sibling_rows() {
    let src = "component C {\n\
               \x20   pins = [\n\
               \x20       1 = A\n\
               \x20       2 = B\n\
               \x20   ]\n\
               \x20   func Ratio(v_hi, v_lo) { return v_hi / v_lo }\n\
               \x20   spec = [\n\
               \x20       vin = 5V\n\
               \x20       vout = 2.5V\n\
               \x20       frac = Ratio(vin, vout)\n\
               \x20   ]\n\
               }\n\
               module main {\n\
               \x20   C c1()\n\
               }\n";
    let (diags, attrs) = probe(&src, "/mcc/u383-value-func-ratio.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    assert!(rows.contains("frac = 2"), "{rows}");
}

/// A call to an unknown name is not a refusal: the row stays as written,
/// silently — the legacy face for every unresolvable call.
#[test]
fn u383__unknown_value_call_stays_raw() {
    let src = "component C {\n\
               \x20   pins = [\n\
               \x20       1 = A\n\
               \x20       2 = B\n\
               \x20   ]\n\
               \x20   spec = [\n\
               \x20       m = Mystery(3V)\n\
               \x20   ]\n\
               }\n\
               module main {\n\
               \x20   C c1()\n\
               }\n";
    let (diags, attrs) = probe(&src, "/mcc/u383-value-func-unknown.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    assert!(rows.contains(r#"m = "Mystery(3V)""#), "{rows}");
}

/// An arity mismatch against a known value function is not a refusal either —
/// the written form survives (no value-face diagnostics this leg).
#[test]
fn u383__value_func_wrong_arity_stays_raw() {
    let src = "component C {\n\
               \x20   pins = [\n\
               \x20       1 = A\n\
               \x20       2 = B\n\
               \x20   ]\n\
               \x20   func Drop(i, r) { return r * i }\n\
               \x20   spec = [\n\
               \x20       vdrop = Drop(20mA)\n\
               \x20   ]\n\
               }\n\
               module main {\n\
               \x20   C c1()\n\
               }\n";
    let (diags, attrs) = probe(&src, "/mcc/u383-value-func-arity.mc");
    assert!(diags.is_empty(), "got {diags:?}");
    let rows = attrs_of(&attrs, "c1").join("\n");
    assert!(rows.contains(r#"vdrop = "Drop(20mA)""#), "{rows}");
}

/// The arithmetic return is body content: neither E5252 (empty body) nor
/// E5103 (params but no body) fires for a value function.
#[test]
fn u383__value_func_is_not_a_stub() {
    let src = "component C {\n\
               \x20   pins = [\n\
               \x20       1 = A\n\
               \x20       2 = B\n\
               \x20   ]\n\
               \x20   func Drop(i, r) { return r * i }\n\
               \x20   spec = [\n\
               \x20       vdrop = Drop(20mA, 100R)\n\
               \x20   ]\n\
               }\n\
               module main {\n\
               \x20   C c1()\n\
               }\n";
    let (diags, _) = probe(&src, "/mcc/u383-value-func-stub.mc");
    assert!(
        !diags.contains(&mcc::errcodes::FUNC_EMPTY_BODY),
        "E5252 must not fire: {diags:?}"
    );
    assert!(
        !diags.contains(&mcc::errcodes::FUNC_PARAMS_NO_BODY),
        "E5103 must not fire: {diags:?}"
    );
}

/// The module-body face calls value functions too: a `let` evaluates the
/// call per instance and the `require` judges the result — 15W passes
/// silently, 25W violates with exactly one E5463 (per instance, leg4b).
#[test]
fn u383__module_let_calls_value_func_per_instance() {
    let src = "module AMP(v) {\n\
               \x20   func Pd(v, i) { return v * i }\n\
               \x20   let p = Pd(v, 5A)\n\
               \x20   require p <= 20W\n\
               }\n\
               module main {\n\
               \x20   AMP(v = 3V)\n\
               }\n";
    let diags = probe_diags(src, "/mcc/u383-value-func-module.mc", "main");
    assert!(diags.is_empty(), "got {diags:?}");

    let violating = src.replace("AMP(v = 3V)", "AMP(v = 5V)");
    let diags = probe_diags(&violating, "/mcc/u383-value-func-module-bad.mc", "main");
    assert_eq!(
        diags
            .iter()
            .filter(|c| **c == mcc::errcodes::MODULE_REQUIRE_UNSATISFIED)
            .count(),
        1,
        "exactly one instance violates: {diags:?}"
    );
}
