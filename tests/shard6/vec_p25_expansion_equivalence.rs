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

use crate::common;

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

/// A whole-value bus actual (`Cap(BUS)`, no sibling actual) is the
/// whole-value fill, not lane expansion — one component, not N.
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

/// The **Set** face of the same whole-value rule: a bus alone in the Set
/// (`Pullup([SPI])`, no sibling actual) fills the indexed formal once — the
/// bus's lanes take the member slots positionally — instead of lane-expanding.
/// This is the `Cap([BUS])` case, and it must land the partition of the
/// handwritten two-slot form `Pullup([SPI.SCLK, SPI.MOSI])`.
#[test]
fn p25__set_form_whole_value_bus_actual_is_not_lane_expanded() {
    let whole = partition_of(
        &src_of("        RES(10).Pullup([SPI])"),
        "/mcc/vec-p25-whole-set.mc",
    );
    let handwritten = partition_of(
        &src_of("        RES(10).Pullup([SPI.SCLK, SPI.MOSI])"),
        "/mcc/vec-p25-whole-set-hand.mc",
    );

    // Anti-false-green: one component, both lanes landed, and on *different*
    // nets — the positional fill, not a share-into-one-net degenerate.
    assert_eq!(
        instance_names(&whole).len(),
        1,
        "a whole-value bus actual inside a Set fills the call once, it does not \
         lane-expand; partition={whole:?}"
    );
    let sclk = net_holding(&whole, "SPI.SCLK")
        .unwrap_or_else(|| panic!("SCLK lane must land on a net; partition={whole:?}"));
    let mosi = net_holding(&whole, "SPI.MOSI")
        .unwrap_or_else(|| panic!("MOSI lane must land on a net; partition={whole:?}"));
    assert_ne!(
        sclk, mosi,
        "the two lanes must fill the two member slots separately; partition={whole:?}"
    );

    assert_eq!(
        whole, handwritten,
        "the whole-value Set form must land the handwritten two-slot partition"
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

// ── R1 whole-reference at a call site (intent-reference-layer-design.md §10.2
//    D1, ruling "uniform rewrite" §10.10.1) ──

/// A module declaring all three branches of the whole-reference predicate:
/// `DVDD` has exactly one `::DC` rail (whole-referenceable), `DUALA` has two
/// (not), `BARE` has none (not). Every rejection cell below is a domain that
/// is *present and rejected*, so none of them can pass by the parser having
/// dropped the domain.
const HEAD_PAIR: &str = "module main {\n    io VDD_3V3\n    io GND\n    io A1\n    io A2\n    io AG\n    domain DVDD  { rail [VDD_3V3, GND]::DC(3.3V) }\n    domain DUALA { rail [A1, AG]::DC(3.3V)\n                   rail [A2, AG]::DC(1.8V) }\n    domain BARE  {}\n    func M() {\n";

fn pair_src_of(body: &str) -> String {
    format!("{RES}{HEAD_PAIR}{body}\n    }}\n}}\n")
}

/// A bare domain name that is *whole-referenceable* denotes its declared
/// `[hot, ret]` pair: the call `RES(10).Pullup(DVDD)` lands exactly the
/// partition the written `RES(10).Pullup([VDD_3V3, GND])` lands.
///
/// The assertion is on the partition, never on a diagnostic list — a code list
/// would be satisfied by the name expanding into nothing. The two anti-false-
/// green checks pin that the written form is non-trivial *before* the equality
/// is read: the two lanes must land on two different nets.
#[test]
fn u79_r1__whole_referenceable_domain_name_equals_its_written_pair() {
    let named = pair_src_of("        RES(10).Pullup(DVDD)");
    let written = pair_src_of("        RES(10).Pullup([VDD_3V3, GND])");
    let by_name = partition_of(&named, "/mcc/u79-r1-named.mc");
    let by_pair = partition_of(&written, "/mcc/u79-r1-written.mc");

    // Pin the written form first: both lanes on real, distinct nets.
    let hot = net_holding(&by_pair, "VDD_3V3")
        .unwrap_or_else(|| panic!("the written pair must land VDD_3V3: {by_pair:?}"));
    let ret = net_holding(&by_pair, "GND")
        .unwrap_or_else(|| panic!("the written pair must land GND: {by_pair:?}"));
    assert_ne!(
        hot, ret,
        "the printed pair must land two different nets, otherwise the equality \
         below is satisfied by expanding into nothing"
    );
    assert!(
        !codes_of(&named, "/mcc/u79-r1-named.mc").contains(&mcc::errcodes::VECTOR_WIDTH_MISMATCH),
        "a whole-referenceable name fits a 2-member vector formal and must not \
         report a width mismatch"
    );
    assert_eq!(
        by_name, by_pair,
        "DVDD must land exactly the partition [VDD_3V3, GND] lands"
    );
}

/// The predicate's two rejection branches, each measured **against a plain
/// undeclared name in the same position** rather than against a hardcoded
/// expectation: a domain that is not whole-referenceable must behave exactly
/// like a name the scope never declared. That pins "not rewritten" without
/// restating the width rule here.
#[test]
fn u79_r1__non_whole_referenceable_domains_behave_like_an_undeclared_name() {
    let plain = codes_of(
        &pair_src_of("        RES(10).Pullup(ZZZ)"),
        "/mcc/u79-r1-plain.mc",
    );
    assert!(
        plain.contains(&mcc::errcodes::VECTOR_WIDTH_MISMATCH),
        "the baseline must be a real mismatch, not silence: {plain:?}"
    );
    for (dom, why) in [
        (
            "DUALA",
            "declares two ::DC rails, so it stands for no single pair",
        ),
        ("BARE", "declares no rail at all, so it stands for nothing"),
    ] {
        let codes = codes_of(
            &pair_src_of(&format!("        RES(10).Pullup({dom})")),
            &format!("/mcc/u79-r1-{dom}.mc"),
        );
        assert_eq!(
            codes, plain,
            "{dom} {why} — it must not widen into a pair, so it must read \
             exactly like an undeclared name"
        );
    }
}

// ── R1 whole-reference at a **chain word position** (intent-reference-layer-design.md
//    §10.11.4; write point = `McOpd::Id`'s bare-name arm) ──

/// A child module exposing a **two-member** port. Declared without an interface
/// on purpose: this file builds without the system library (`common::reset`),
/// and there a `::DC` port has *unknown* width — every chain touching one is
/// rejected on shape whatever stands on the other side (measured). The rule
/// under test never reads the receiver (§10.10.1 "uniform rewrite": no
/// receiver-shape table),
/// so a plain membered port is the faithful receiver.
const WCHILD: &str = "module CHILD2 {\n    io vin{VDD_3V3, GND}\n}\n";

/// `main`'s ports and its child, written first so the only thing that moves in
/// the ordering test below is where the `domain` clause sits.
const WHEAD: &str =
    "module main {\n    io VDD_3V3\n    io GND\n    io A1\n    io AG\n    CHILD2 b1\n";

/// The whole-referenceable domain (D1): **one** `::DC` rail states the pair.
const WDOM: &str = "    domain DVDD { rail [VDD_3V3, GND]::DC(3.3V) }\n";

/// The predicate's two rejection branches, each declared *rather than absent*
/// so no rejection cell below can pass because the clause was dropped: two
/// `::DC` rails state two pairs, and an empty domain states none.
const WDOM_NO: &str = "    domain DUALA { rail [A1, AG]::DC(3.3V)\n                   rail [A1, AG]::DC(1.8V) }\n    domain BARE  {}\n";

fn wsrc(doms: &str, body: &str) -> String {
    format!("{WCHILD}{WHEAD}{doms}{body}\n}}\n")
}

/// A bare whole-referenceable domain name in a chain word position denotes its
/// declared pair: `b1.vin -> DVDD` lands exactly the partition
/// `b1.vin -> [VDD_3V3, GND]` lands, and does it quietly.
///
/// Asserted on the partition, never on a code list — a list of codes would be
/// satisfied by the name expanding into nothing. The anti-false-green check
/// pins that the written form lands the pair on two *different* ports before the
/// equality is read.
#[test]
fn u101_r1__chain_word_position_equals_the_written_pair() {
    let named_src = wsrc(WDOM, "    b1.vin -> DVDD");
    let written_src = wsrc(WDOM, "    b1.vin -> [VDD_3V3, GND]");
    let named = partition_of(&named_src, "/mcc/u101-word-named.mc");
    let written = partition_of(&written_src, "/mcc/u101-word-written.mc");

    let hot = net_holding(&written, "VDD_3V3").expect("the written pair lands VDD_3V3");
    let ret = net_holding(&written, "GND").expect("the written pair lands GND");
    assert_ne!(
        hot, ret,
        "the written pair must land two different nets, otherwise the equality \
         below holds by both sides wiring nothing"
    );
    assert_eq!(
        named, written,
        "DVDD written as a chain word must land exactly the partition its \
         written-out pair lands"
    );
    // The whole point of the rule: the name was a *floating net name* before it,
    // so a quiet reading is the observable half of the widening (E3136 gone).
    assert!(
        codes_of(&named_src, "/mcc/u101-word-named-codes.mc").is_empty(),
        "a whole-referenceable domain name must not be reported as floating"
    );
}

/// Guard ③: a word position's meaning does not depend on where the `domain`
/// clause is written. Pass1 peeks the declaration table before the body walk, so
/// "used above its declaration" must not silently fall back to the floating-net
/// reading of the same name.
#[test]
fn u101_r1__the_word_position_does_not_depend_on_where_the_domain_is_written() {
    let above = partition_of(
        &wsrc(WDOM, "    b1.vin -> DVDD"),
        "/mcc/u101-order-above.mc",
    );
    let below = partition_of(
        &format!("{WCHILD}{WHEAD}    b1.vin -> DVDD\n{WDOM}}}\n"),
        "/mcc/u101-order-below.mc",
    );
    assert!(!above.is_empty(), "the pair must land something to compare");
    assert_eq!(
        above, below,
        "a domain name must mean the same thing above and below its own clause"
    );
}

/// §10.10.1 "uniform rewrite": the widening is **unconditional on the receiver's
/// shape** — no receiver-shape table — so a domain name landing in a *single-word*
/// position widens there too, and the existing shape gate reports the mismatch. The
/// alternative reading of the same name (a floating net name) is what the second
/// half rules out: the plain name beside it keeps E3136 exactly as before.
#[test]
fn u101_r1__a_single_word_position_fails_on_shape_not_as_a_floating_name() {
    let named = codes_of(
        &wsrc(WDOM, "    b1.vin.VDD_3V3 -> DVDD"),
        "/mcc/u101-1w-named.mc",
    );
    assert!(
        named.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "the widened pair against a one-wide position is a shape mismatch: {named:?}"
    );
    assert!(
        !named.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "the name resolved to a declared pair, so it is no longer a floating \
         net name: {named:?}"
    );

    let plain = codes_of(
        &wsrc(WDOM, "    b1.vin.VDD_3V3 -> ZZZ"),
        "/mcc/u101-1w-plain.mc",
    );
    assert!(
        plain.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "the control must keep the floating reading: {plain:?}"
    );
}

/// The predicate's rejection branches **at a chain word position**, each
/// measured against a plain undeclared name in the same position rather than
/// against a hardcoded expectation: a domain that is not whole-referenceable
/// must read exactly like a name the scope never declared.
///
/// The last cell is the non-vacuity control — the whole-referenceable domain in
/// that same position must *not* read like the undeclared name, otherwise every
/// equality above would hold because the rule never fires.
#[test]
fn u101_r1__non_whole_referenceable_domains_read_like_an_undeclared_name() {
    let both = format!("{WDOM}{WDOM_NO}");
    let plain = codes_of(&wsrc(&both, "    b1.vin -> ZZZ"), "/mcc/u101-rej-plain.mc");
    assert!(
        !plain.is_empty(),
        "the baseline must be a real mismatch (a one-wide name against a \
         two-wide port), not silence"
    );
    for (dom, why) in [
        (
            "DUALA",
            "declares two ::DC rails, so it stands for no single pair",
        ),
        ("BARE", "declares no rail at all, so it stands for nothing"),
    ] {
        let codes = codes_of(
            &wsrc(&both, &format!("    b1.vin -> {dom}")),
            &format!("/mcc/u101-rej-{dom}.mc"),
        );
        assert_eq!(
            codes, plain,
            "{dom} {why} — it must not widen at a word position either, so it \
             must read exactly like an undeclared name"
        );
    }
    assert_ne!(
        codes_of(&wsrc(&both, "    b1.vin -> DVDD"), "/mcc/u101-rej-dvdd.mc"),
        plain,
        "the whole-referenceable domain must differ from the undeclared name, \
         otherwise the two equalities above judge nothing"
    );
}

// ── R1 whole-reference against a **curly receiver** (the "module-port word
//    position" of intent-reference-layer-design.md §10.9 step 2) ──

/// A child module exposing **two scalar ports**, so a curly group `b1{p, m}`
/// resolves through `dot_or_curly` into a two-lane bus. Declared without an
/// interface for the same harness reason as `CHILD2` above.
const MCHILD: &str = "module CHILD3 {\n    io p\n    io m\n}\n";

/// `main`'s rail-face names and the child instance, written first.
const MHEAD: &str =
    "module main {\n    io VDD_3V3\n    io GND\n    io A1\n    io AG\n    CHILD3 b1\n";

fn msrc(doms: &str, body: &str) -> String {
    format!("{MCHILD}{MHEAD}{doms}{body}\n}}\n")
}

/// The **receiver** of a whole reference may be a curly group on an instance:
/// `DVDD -> b1{p, m}` (source side) and `b1{p, m} -> DVDD` (target side) must
/// each land exactly the partition their written-out pairs land.
///
/// The domain word itself is a bare single identifier in both spellings — the
/// receiver's curly shape comes from `dot_or_curly`, not from the widening —
/// so no write point besides the bare-name arm exists; this test pins that
/// reading against a regression that narrows the widening to dotted receivers.
#[test]
fn u79_r1__a_curly_module_port_receiver_equals_the_written_pair() {
    for (named_body, written_body, side) in [
        ("    DVDD -> b1{p, m}", "    [VDD_3V3, GND] -> b1{p, m}", "source"),
        ("    b1{p, m} -> DVDD", "    b1{p, m} -> [VDD_3V3, GND]", "target"),
    ] {
        let named = partition_of(&msrc(WDOM, named_body), "/mcc/u79-curly-named.mc");
        let written = partition_of(&msrc(WDOM, written_body), "/mcc/u79-curly-written.mc");

        // Anti-false-green: the written form lands the two rail faces on two
        // different nets, otherwise the equality below judges nothing.
        let hot = net_holding(&written, "VDD_3V3").unwrap_or_else(|| {
            panic!("the written pair must land VDD_3V3 ({side}): {written:?}")
        });
        let ret = net_holding(&written, "GND")
            .unwrap_or_else(|| panic!("the written pair must land GND ({side}): {written:?}"));
        assert_ne!(hot, ret, "the written pair must land two nets ({side})");

        assert_eq!(
            named, written,
            "the domain word against a curly receiver must land exactly the \
             written-out pair's partition ({side})"
        );
        assert!(
            codes_of(&msrc(WDOM, named_body), "/mcc/u79-curly-named-codes.mc").is_empty(),
            "the whole reference at a curly receiver must be quiet ({side})"
        );
    }
}

/// The rejection branches survive the curly receiver too: a domain that is not
/// whole-referenceable reads exactly like an undeclared name there, and the
/// whole-referenceable one must not (non-vacuity, same shape as the chain
/// lock above).
#[test]
fn u79_r1__non_whole_referenceable_domains_keep_their_curly_reading() {
    let both = format!("{WDOM}{WDOM_NO}");
    let plain = codes_of(
        &msrc(&both, "    b1{p, m} -> ZZZ"),
        "/mcc/u79-curly-rej-plain.mc",
    );
    assert!(
        !plain.is_empty(),
        "the baseline must be a real mismatch (a one-wide name against a \
         two-wide curly group), not silence: {plain:?}"
    );
    for (dom, why) in [
        (
            "DUALA",
            "declares two ::DC rails, so it stands for no single pair",
        ),
        ("BARE", "declares no rail at all, so it stands for nothing"),
    ] {
        let codes = codes_of(
            &msrc(&both, &format!("    b1{{p, m}} -> {dom}")),
            &format!("/mcc/u79-curly-rej-{dom}.mc"),
        );
        assert_eq!(
            codes, plain,
            "{dom} {why} — it must not widen against a curly receiver either, \
             so it must read exactly like an undeclared name"
        );
    }
    assert_ne!(
        codes_of(
            &msrc(&both, "    b1{p, m} -> DVDD"),
            "/mcc/u79-curly-rej-dvdd.mc"
        ),
        plain,
        "the whole-referenceable domain must differ from the undeclared name, \
         otherwise the two equalities above judge nothing"
    );
}

/// Guard ① and guard ② together: only a **bare single identifier**, and only in
/// the scope that owns the declaration.
///
/// ① is asserted by its sharpest consequence: in a source where the very same
/// fixture reports the collision (test below), a *dotted* word position writes
/// the same base name and must report nothing — the lookup key is the whole
/// written word, so `DVDD.pins` is not a bare name at all.
#[test]
fn u101_r1__a_dotted_word_position_is_not_a_bare_name() {
    let head = format!("    io DVDD::DC(5V)\n{WDOM}");
    let with_domain = codes_of(
        &wsrc(&head, "    DVDD.pins -> b1.vin"),
        "/mcc/u101-dot-with.mc",
    );
    let without = codes_of(
        &wsrc("    io DVDD::DC(5V)\n", "    DVDD.pins -> b1.vin"),
        "/mcc/u101-dot-without.mc",
    );
    assert!(
        !with_domain.contains(&mcc::errcodes::DOMAIN_ENDPOINT_NAME_COLLISION),
        "a dotted word position is not the bare name the table holds: {with_domain:?}"
    );
    assert_eq!(
        with_domain, without,
        "the domain clause must change nothing for a dotted word position"
    );
}

/// Guard ②: the table answers for the **owning** scope alone. A module's own
/// `func` body resolves against that module; a component's `func` body — nobody's
/// module, no domains — leaves its bare names exactly as they were.
///
/// Both halves are asserted, so neither can be green because the rule never
/// fires anywhere: the module half must *differ* from its undeclared twin, the
/// component half must not.
#[test]
fn u101_r1__the_rule_reads_only_the_owning_module() {
    let module_named = codes_of(
        &wsrc(WDOM, "    func M() { b1.vin -> DVDD }"),
        "/mcc/u101-scope-mod-named.mc",
    );
    let module_plain = codes_of(
        &wsrc(WDOM, "    func M() { b1.vin -> ZZZ }"),
        "/mcc/u101-scope-mod-plain.mc",
    );
    assert_ne!(
        module_named, module_plain,
        "in the module's own func the domain name resolves against that module, \
         so it cannot read like an undeclared name"
    );

    // The component half is the ruling's own worked example: a component pin
    // named like a domain of a module written elsewhere in the same file.
    let comp = "component COMP1 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func W() {\n        {NAME} - this - 1\n    }\n}\n";
    let module = "module main {\n    io VDD_3V3\n    io GND\n    domain DVDD { rail [VDD_3V3, GND]::DC(3.3V) }\n}\n";
    let named = codes_of(
        &format!("{}{}", comp.replace("{NAME}", "DVDD"), module),
        "/mcc/u101-scope-comp-named.mc",
    );
    let plain = codes_of(
        &format!("{}{}", comp.replace("{NAME}", "ZZZ"), module),
        "/mcc/u101-scope-comp-plain.mc",
    );
    assert_eq!(
        named, plain,
        "a component func body owns no domains, so a bare name there must read \
         exactly as it did before the rule existed"
    );
}

/// R4 step 3 (`intent-reference-layer-design.md` §10.5): a bare word position
/// where **both** readings hold — the name is a whole-referenceable domain *and*
/// an endpoint already declared in the scope — has no single meaning, so it is
/// reported (6050) instead of being resolved silently by the reading order.
///
/// Each endpoint kind the rule names is filled in, and each is read against its
/// own twin: the same source with the `domain` clause removed. Nothing but the
/// diagnostic may change, which is what makes "reported, not resolved" a
/// measurement rather than a promise.
#[test]
fn u101_r4__a_name_that_is_both_a_domain_and_an_endpoint_is_reported() {
    let body = "    b1.vin.VDD_3V3 -> DVDD\n";
    for (decl, slug, why) in [
        ("    io DVDD::DC(5V)\n", "port", "a declared port"),
        ("    CHILD2 DVDD\n", "instance", "a declared instance"),
        (
            "    conduit DVDD @role(main)\n",
            "conduit",
            "a conductor identity",
        ),
    ] {
        let with_domain = wsrc(&format!("{decl}{WDOM}"), body);
        let twin = wsrc(decl, body);
        let codes = codes_of(&with_domain, &format!("/mcc/u101-coll-{slug}.mc"));
        assert!(
            codes.contains(&mcc::errcodes::DOMAIN_ENDPOINT_NAME_COLLISION),
            "'DVDD' is both a whole-referenceable domain and {why} — the two \
             readings name different nets, so this word position must be \
             reported, not resolved by order: {codes:?}"
        );
        let rest: Vec<u32> = codes
            .iter()
            .copied()
            .filter(|c| *c != mcc::errcodes::DOMAIN_ENDPOINT_NAME_COLLISION)
            .collect();
        assert_eq!(
            rest,
            codes_of(&twin, &format!("/mcc/u101-coll-{slug}-twin.mc")),
            "the collision must not resolve the word either way — the reading \
             stays the endpoint one, so only the diagnostic may change"
        );
        assert_eq!(
            partition_of(&with_domain, &format!("/mcc/u101-collp-{slug}.mc")),
            partition_of(&twin, &format!("/mcc/u101-collp-{slug}-twin.mc")),
            "the wiring must be identical with and without the domain clause"
        );
    }

    // Control: the same word position with no endpoint of that name is not a
    // collision — the rule widens it instead.
    assert!(
        !codes_of(&wsrc(WDOM, body), "/mcc/u101-coll-only.mc")
            .contains(&mcc::errcodes::DOMAIN_ENDPOINT_NAME_COLLISION),
        "a domain whose name no endpoint claims is not a collision"
    );
}

/// §10.11.3 ①: a statement the chain-shape gate rejected *was* read to the end,
/// so `CONN_STMT_PARSE_FAILED` ("failed to parse") must not be stacked on top of
/// the shape code that already names the defect.
///
/// The control is a statement whose failure is **not** a shape: it met an
/// operator the grammar has no reading for, carries no shape fact, and keeps the
/// wrapper. (① does not touch the other half of the finding — the rejected
/// statement is still dropped whole; that is the ruling's own note, not
/// something this lock claims.)
#[test]
fn u101_diag__a_shape_failure_is_not_restated_as_a_parse_failure() {
    let shaped = codes_of(&wsrc(WDOM, "    b1.vin -> ZZZ"), "/mcc/u101-diag-shaped.mc");
    assert!(
        shaped.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "the premise must be a real shape failure: {shaped:?}"
    );
    assert!(
        !shaped.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "a statement carrying its own shape failure must not also be called a \
         parse failure: {shaped:?}"
    );

    let unreadable = codes_of(
        &wsrc(WDOM, "    [VDD_3V3, GND] ~ b1.vin"),
        "/mcc/u101-diag-unreadable.mc",
    );
    assert!(
        unreadable.contains(&mcc::errcodes::CONN_OPERATOR_UNSUPPORTED),
        "the control must be a real non-shape failure: {unreadable:?}"
    );
    assert!(
        unreadable.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "a statement that genuinely failed to parse must keep the wrapper: \
         {unreadable:?}"
    );
}

/// §10.11.4's written-down open hole: a domain name as a **literal element**
/// (`[DVDD, GND]`) is a word of its own and rides the same arm, so it widens too
/// and the list's column widths stop agreeing. The ruling's condition was that
/// this stays **loud** — if it ever goes quiet, this is the test that must fail,
/// so the word-position rule gets re-ruled instead of patched.
#[test]
fn u101_r1__a_domain_name_as_a_literal_element_is_loud() {
    let clean = codes_of(
        &wsrc(WDOM, "    [VDD_3V3, GND] -> b1.vin"),
        "/mcc/u101-el-clean.mc",
    );
    assert!(
        clean.is_empty(),
        "the premise: the same list of declared names is a legal connection, so \
         anything the domain element adds comes from the widening: {clean:?}"
    );
    let with_domain = codes_of(
        &wsrc(WDOM, "    [DVDD, GND] -> b1.vin"),
        "/mcc/u101-el-domain.mc",
    );
    assert!(
        !with_domain.is_empty(),
        "the widened element leaves a three-wide list against a two-wide port; \
         that must be reported, not silently accepted"
    );
    let plain = codes_of(
        &wsrc(WDOM, "    [ZZZ, GND] -> b1.vin"),
        "/mcc/u101-el-plain.mc",
    );
    assert_ne!(
        with_domain, plain,
        "the domain element must read differently from an undeclared element, \
         otherwise this cell is measuring the list form rather than the widening"
    );
}
