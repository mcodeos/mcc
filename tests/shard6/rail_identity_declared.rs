// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Rail identity is a **declaration**, never a spelling (ledger U55-U59;
//! world-axioms §1 A1 -- "no logical policy may rest on a guess about a name";
//! identity-design §3.1 -- "no `is_ground_name` / `MemberRole::Ground` /
//! last-segment normalization as a merge criterion").
//!
//! Every rail predicate in the tree used to answer its question by looking at
//! the name: `is_ground_name`, `is_supply_name`, `rail_identity`,
//! `looks_like_power_rail`, `is_power_rail_name`, `POWER_PIN_NAMES`,
//! `GROUND_PIN_NAMES`. A rail-looking name is not a declaration -- it records
//! what a human called the net, not what the author declared it to be. These
//! cells lock the replacement: every answer now comes from a declaration the
//! author actually wrote (a `psrc/psnk/psbi ... ::DC(...)` face row, an
//! `IOType::Power` direction word, or the owner's own port / rail declaration),
//! and name equality is **exact string equality** -- case-sensitive, zero
//! normalization (U49).
//!
//! The negative cells matter as much as the positive ones: they are what keeps
//! a name table from growing back.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::instant::netcheck::{Finding, Level, Report};

/// Build `src` in a fresh workspace and return the codes diagnosed **for that
/// file only** (`mcc_diagnose_all` spans the workspace and would leak findings
/// from earlier cells into later ones).
fn codes_of(uri: &str, src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    mcc::instant::reset_r05_counter();
    let u: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &u);
    mcc::mcc_diagnose(&u).iter().map(|d| d.code).collect()
}

/// Build flat (pass1 + pass2 + flatten) and return the netcheck report.
fn report_of(uri: &str, src: &str) -> Report {
    let _lock = common::lock();
    common::reset();
    mcc::instant::reset_r05_counter();
    let u: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&u, src);
    let (_, table) = mcc::mcc_build_flat(&mcc::McIds::from("main"), &u, 1000).expect("flat build");
    mcc::instant::netcheck::run(&table)
}

/// Findings of one rule at one level.
fn rule_findings<'a>(report: &'a Report, rule: &str, level: Level) -> Vec<&'a Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.rule == rule && f.level == level)
        .collect()
}

/// A plain two-pin resistor, pins `1` / `2` (no face declared anywhere).
const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

// ── E3136 floating net label: the owner's own declaration is the identity ──

/// A bare rail-*looking* name that nothing declares is a floating label. The
/// old exemption skipped any name that `is_supply_name` / `is_ground_name`
/// recognized, so `GND` here was silent on its spelling alone.
#[test]
fn rid__undeclared_rail_looking_name_is_a_floating_label() {
    let src = format!(
        "{RES2}module main {{\n    RES2 R1\n    func M() {{\n        R1.2 -> GND\n    }}\n}}\n"
    );
    let codes = codes_of("/mcc/rid-bare-gnd.mc", &src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "an undeclared `GND` is a dangling name, not an implicit rail; got {codes:?}"
    );
}

/// The same statement with `GND` declared by the owning module is quiet: the
/// declaration, not the spelling, is what confers identity.
#[test]
fn rid__declared_rail_name_is_quiet() {
    let src = format!(
        "{RES2}module main {{\n    io GND\n    RES2 R1\n    func M() {{\n        R1.2 -> GND\n    }}\n}}\n"
    );
    let codes = codes_of("/mcc/rid-declared-gnd.mc", &src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "`GND` declared as `main`'s own port is an identity; got {codes:?}"
    );
}

/// Name equality is **exact**: a declaration of `gnd` does not declare `GND`.
/// The former word table upper-cased before comparing, so the two spellings
/// were the same rail.
#[test]
fn rid__name_equality_is_exact_string() {
    let src = format!(
        "{RES2}module main {{\n    io gnd\n    RES2 R1\n    func M() {{\n        R1.2 -> GND\n    }}\n}}\n"
    );
    let codes = codes_of("/mcc/rid-case-gnd.mc", &src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "`io gnd` does not declare `GND` (case-sensitive, zero normalization); got {codes:?}"
    );
}

/// The `V` + digit heuristic (`V3V3`, `V5V`, ...) is gone with the table: an
/// undeclared `VDD_3V3` is a dangling name like any other.
#[test]
fn rid__v_digit_spelling_earns_no_exemption() {
    let src = format!(
        "{RES2}module main {{\n    RES2 R1\n    func M() {{\n        R1.2 -> VDD_3V3\n    }}\n}}\n"
    );
    let codes = codes_of("/mcc/rid-vdigit.mc", &src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "`VDD_3V3` is not a rail without a declaration; got {codes:?}"
    );
}

// ── R09 floating power pin: the pin's own declared face decides ──

/// A pin merely *named* `VDD` carries no face. R09 used to accept a
/// power-looking spelling, so this pin fired the warn with nothing declared.
/// The fixture wires the device into a net, so the cell isolates the face
/// question from the orphan/undriven rules.
#[test]
fn rid__supply_looking_pin_name_carries_no_face() {
    let src = "component R {\n    pins = [\n        in 1 = VDD\n        2 = SIG\n    ]\n}\nmodule main {\n    io SIG\n    R r1\n    r1.2 -> SIG\n}\n";
    let report = report_of("/mcc/rid-r09-name.mc", src);
    let hits = rule_findings(&report, "R09", Level::Warn);
    assert!(
        hits.is_empty(),
        "a pin named `VDD` that nothing declared a supply is not a floating power pin; findings: {:?}",
        report.findings
    );
}

/// The same pin with its face declared (`psnk`) does fire — the positive half
/// of the pair, so the negative cell above cannot pass by the rule being dead.
#[test]
fn rid__declared_face_is_a_floating_power_pin() {
    let src = "component R {\n    pins = [\n        psnk 1 = VDD\n        2 = SIG\n    ]\n}\nmodule main {\n    io SIG\n    R r1\n    r1.2 -> SIG\n}\n";
    let report = report_of("/mcc/rid-r09-face.mc", src);
    let hits = rule_findings(&report, "R09", Level::Warn);
    assert_eq!(
        hits.len(),
        1,
        "a declared supply face left unconnected is a floating power pin; findings: {:?}",
        report.findings
    );
}

// ── HW1 (E5454) power pin without a voltage declaration ──

/// HW1 used to consult `POWER_PIN_NAMES` (`VCC`, `VDD`, ...), so a pin named
/// `VCC` raised the hint with nothing declared. It is now gated on the pin's
/// own declaration: `in 1 = VCC` declares no supply face and stays quiet.
#[test]
fn rid__power_looking_pin_name_earns_no_hw1_hint() {
    let src = "component BAD {\n    pins = [\n        in 1 = VCC\n    ]\n}\nmodule main {\n    BAD b1\n}\n";
    let codes = codes_of("/mcc/rid-hw1-name.mc", src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_PIN_NO_VOLTAGE),
        "a pin named `VCC` with no declared supply face is not HW1; got {codes:?}"
    );
}

/// `psnk` declares the pin a supply terminal, so the same name does fire.
#[test]
fn rid__declared_supply_pin_is_hw1() {
    let src = "component BAD {\n    pins = [\n        psnk 1 = VCC\n    ]\n}\nmodule main {\n    BAD b1\n}\n";
    let codes = codes_of("/mcc/rid-hw1-face.mc", src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_PIN_NO_VOLTAGE),
        "a declared supply pin with no voltage declaration is HW1; got {codes:?}"
    );
}

// ── A component's own `pins.pwr` row is a declaration (the A1 exemption) ──

/// A name a component declares in its own pin row is an identity, whatever it
/// is called: `psnk [1,2] = [VDD, VSS]::DC(3.3V)` declares both faces, so the
/// two names are not floating labels even though nothing else names them.
#[test]
fn rid__declared_pin_faces_are_identities() {
    let src = "component B {\n    pins = [\n        psnk [1, 2] = [VDD, VSS]::DC(3.3V)\n    ]\n    func G() {\n        VDD -> VSS\n    }\n}\nmodule main {\n    io VDD\n    B b1\n    b1.VDD -> VDD\n}\n";
    let codes = codes_of("/mcc/rid-declared-faces.mc", src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "both faces are declared by the pin row, so neither name is dangling; got {codes:?}"
    );
}

// ── R03 short circuit: a *declared* supply and a *declared* return ──

/// The positive twin lives in `gate_phase1::dlu_gate__module_level_true_miss_shorts_via_r03`;
/// this is the negative half of the same law, and the one that a name table
/// cannot pass. Two components differ only in whether their pin row **declares**
/// its faces (`psnk [1,2] = [VDD, VSS]::DC(3.3V)`) or merely names them
/// (`1 = VDD` / `2 = VSS`). Both are wired into the same two bare module nets,
/// yet only the declared one pairs a supply with a return and shorts.
#[test]
fn rid__named_pins_alone_pair_no_supply_with_no_return() {
    let declared = report_of(
        "/mcc/rid-r03-declared.mc",
        "component B {\n    pins = [\n        psnk [1, 2] = [VDD, VSS]::DC(3.3V)\n    ]\n}\nmodule main {\n    io vdd\n    io vss\n    B b\n    b.VDD -> vdd\n    b.VSS -> vss\n    uC.ADC.P -> vdd\n    uC.ADC.P -> vss\n}\n",
    );
    assert!(
        !rule_findings(&declared, "R03", Level::Error).is_empty(),
        "a declared supply joined to a declared return is a short; findings: {:?}",
        declared.findings
    );

    let named = report_of(
        "/mcc/rid-r03-named.mc",
        "component B {\n    pins = [\n        1 = VDD\n        2 = VSS\n    ]\n}\nmodule main {\n    io vdd\n    io vss\n    B b\n    b.VDD -> vdd\n    b.VSS -> vss\n    uC.ADC.P -> vdd\n    uC.ADC.P -> vss\n}\n",
    );
    assert!(
        rule_findings(&named, "R03", Level::Error).is_empty(),
        "pins merely named `VDD`/`VSS` declare no face, so nothing shorts; findings: {:?}",
        named.findings
    );
}
