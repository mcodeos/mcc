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
