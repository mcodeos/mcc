// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! unified-core §4.4 [V]: **P2-5 bus-lane expansion**.
//!
//! A multi-member bus written as the prefix of a call whose Set actual names it
//! must expand to one call per lane:
//!
//! ```text
//! SPI{SCLK, MOSI}                 ; 2-member bus
//! SPI => RES(10).Pullup([_, VDD]) ; documented form — folds to .Pullup([SPI, VDD])
//! ```
//!
//! The `=>` prefix folds at parse time (`mc_fcall.rs` §1), so the bus lands
//! *inside* the call's actuals and never appears as a chain member. The trigger
//! therefore reads the bus off the FuncCall's parameter face
//! (`fc_bus_in_set` / `bus_lane_phrases`, stmt.rs), not off chain adjacency.
//!
//! What is locked here is the **equivalence the design states**: the bus form
//! lands exactly the partition of the handwritten per-lane form:
//!
//! ```text
//! SPI.SCLK - RES(10).Pullup([SPI.SCLK, VDD])
//! SPI.MOSI - RES(10).Pullup([SPI.MOSI, VDD])
//! ```
//!
//! The assertion is on the net **partition** (point-sets that share a net,
//! canonicalized, net names dropped) and on the auto-instance names inside it —
//! never on a diagnostic code list, which would be satisfied by expanding into
//! nothing. Two anti-false-green checks below pin that both forms are
//! non-trivial (two components, every pin wired) before the equality is read.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// A two-pin resistor whose `Pullup` body wires `n1 - this - n2`, so a call
/// expands into a real component with both pins landed.
const RES: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Module skeleton: the bus is declared as a membered port (`io SPI{...}`),
/// which is what registers it in the bus table.
const HEAD: &str = "module main {\n    io SPI{SCLK, MOSI}\n    io VDD\n    func M() {\n";

fn src_of(body: &str) -> String {
    format!("{RES}{HEAD}{body}\n    }}\n}}\n")
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped. Point paths keep their instance prefix so the two forms'
/// component wiring is comparable.
fn partition_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut parts: Vec<Vec<String>> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(_, pts)| {
                    let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
                    ps.sort();
                    ps
                })
                .filter(|ps| !ps.is_empty())
                .collect()
        })
        .unwrap_or_default();
    parts.sort();
    parts
}

/// Non-benign diagnostic codes for `src`.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Codes unrelated to this lock: the short probe names warn (5641/5642/5643,
/// 5054) and the inline component cannot resolve its *catalog* class without a
/// system library (`INST_CLASS_UNRESOLVED` 3157 / `INST_CLASS_NOT_LOADED`
/// 5256). The components are still built — which is what this lock reads — and
/// any real wiring failure shows up as a `4xxx` code, which is not benign.
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054 | 3157 | 5256)
}

/// The auto-instance heads (`_Rn`) appearing in the partition's point paths.
fn instance_names(parts: &[Vec<String>]) -> BTreeSet<String> {
    parts
        .iter()
        .flatten()
        .filter_map(|p| p.rsplit_once('.').map(|(head, _)| head.to_string()))
        .filter(|head| head.starts_with('_'))
        .collect()
}

/// The net carrying `needle`, by substring match over its point paths.
fn net_holding<'a>(parts: &'a [Vec<String>], needle: &str) -> Option<&'a Vec<String>> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains(needle)))
}

const DOCUMENTED: &str = "        SPI => RES(10).Pullup([_, VDD])";
const HANDWRITTEN: &str = "        SPI.SCLK - RES(10).Pullup([SPI.SCLK, VDD])\n        SPI.MOSI - RES(10).Pullup([SPI.MOSI, VDD])";

/// A resistor whose `Pullup` declares **two scalar network formals**, so the
/// bus fills a scalar formal rather than a Set slot — the spelling
/// `param-prefix-design.md` §5 writes (`Pullup(_, VDD)` → `.Pullup(I2C0, VDD)`).
const RES_SCALAR: &str = "component RESS(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

fn scalar_src_of(body: &str) -> String {
    format!("{RES_SCALAR}{HEAD}{body}\n    }}\n}}\n")
}

/// The documented bus form and the handwritten per-lane form land the same
/// partition — the law P2-5 states.
#[test]
fn p25__documented_fold_form_equals_handwritten_per_lane() {
    let folded = partition_of(&src_of(DOCUMENTED), "/mcc/vec-p25-fold.mc");
    let handwritten = partition_of(&src_of(HANDWRITTEN), "/mcc/vec-p25-hand.mc");

    // Anti-false-green: both sides must actually build two wired components.
    // Without this, "expand into nothing" would satisfy the equality below.
    for (label, parts) in [("folded", &folded), ("handwritten", &handwritten)] {
        let names = instance_names(parts);
        assert_eq!(
            names.len(),
            2,
            "{label} form must build one component per bus lane; got {names:?} \
             (partition={parts:?})"
        );
        assert!(
            net_holding(parts, "SPI.SCLK").is_some(),
            "{label}: the SCLK lane must land on a net; partition={parts:?}"
        );
        assert!(
            net_holding(parts, "SPI.MOSI").is_some(),
            "{label}: the MOSI lane must land on a net; partition={parts:?}"
        );
        assert!(
            net_holding(parts, "VDD").map(|ps| ps.len()).unwrap_or(0) >= 3,
            "{label}: VDD must carry both components' second pin; partition={parts:?}"
        );
    }

    assert_eq!(
        folded, handwritten,
        "the bus form must land the handwritten per-lane partition"
    );
}

/// The folded form is clean: expanding the bus must not emit the width or shape
/// errors the un-expanded call would.
#[test]
fn p25__bus_form_expands_without_diagnostics() {
    assert_eq!(
        codes_of(&src_of(DOCUMENTED), "/mcc/vec-p25-clean.mc"),
        Vec::<u32>::new(),
        "the documented bus form must expand quietly"
    );
}

/// A whole-value bus actual (`Cap(BUS)`, not inside a Set) is the §11.6
/// vector fill, not lane expansion — one component, not N.
#[test]
fn p25__whole_value_bus_actual_is_not_lane_expanded() {
    let parts = partition_of(
        &src_of("        SPI => RES(10).Pullup(_)"),
        "/mcc/vec-p25-whole.mc",
    );
    assert_eq!(
        instance_names(&parts).len(),
        1,
        "a whole-value bus actual fills the call once, it does not lane-expand; \
         partition={parts:?}"
    );
}

/// The scalar formals' handwritten counterpart of [`HANDWRITTEN`].
const HANDWRITTEN_SCALAR: &str =
    "        SPI.SCLK - RESS(10).Pullup(SPI.SCLK, VDD)\n        SPI.MOSI - RESS(10).Pullup(SPI.MOSI, VDD)";

/// The §5 spelling — a multi-member bus filling a **scalar** network formal
/// (`RESS(10).Pullup(_, VDD)` folds to `.Pullup(SPI, VDD)`) — lane-expands too,
/// and lands the same partition as its handwritten per-lane form.
#[test]
fn p25__scalar_formal_bus_spelling_agrees_too() {
    let scalar = partition_of(
        &scalar_src_of("        SPI => RESS(10).Pullup(_, VDD)"),
        "/mcc/vec-p25-scalar.mc",
    );
    let handwritten = partition_of(
        &scalar_src_of(HANDWRITTEN_SCALAR),
        "/mcc/vec-p25-scalar-hand.mc",
    );

    assert_eq!(
        instance_names(&scalar).len(),
        2,
        "one component per bus lane; partition={scalar:?}"
    );
    assert_eq!(
        scalar, handwritten,
        "the scalar-formal spelling must land the handwritten per-lane partition"
    );
    assert_eq!(
        codes_of(
            &scalar_src_of("        SPI => RESS(10).Pullup(_, VDD)"),
            "/mcc/vec-p25-scalar-clean.mc"
        ),
        Vec::<u32>::new(),
        "the scalar-formal spelling must expand quietly"
    );
}

/// The chain spelling (`BUS - C(..).M([BUS, V])`) is the other face of the same
/// law and must land on the same partition as the folded one.
#[test]
fn p25__chain_spelling_agrees_with_the_folded_spelling() {
    let folded = partition_of(&src_of(DOCUMENTED), "/mcc/vec-p25-agree-fold.mc");
    let chained = partition_of(
        &src_of("        SPI - RES(10).Pullup([SPI, VDD])"),
        "/mcc/vec-p25-agree-chain.mc",
    );
    assert_eq!(
        chained, folded,
        "both spellings of the bus-lane expansion must land the same partition"
    );
}
