// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection-time pin option conflict detection (§4.2 check 2 / §4.3
//! `used_options`, U289 C1; dotted curly-group face U301).
//!
//! A `|` option group gives one physical pin several names. When two names
//! of DIFFERENT options reach the same pin through connections, the pin is
//! being asked to carry two functions at once — E5156, reported at the
//! resolving connection. Names of ONE option never conflict: the alias form
//! (`1 = A | B`, a single-pinid row, §2.7) shares one option ordinal, and
//! two pins of the same option are two pins.
//!
//! The dotted curly-group members (`u.SPI0.SCLK`, `u.SPI0{SCLK}`) resolve
//! through the bus-port lane expansion (P3-1) — that arm records the option
//! use under the registration spelling (`SPI0.SCLK`), so the curly face
//! participates in E5156 exactly like the flat face. A bare leaf segment
//! (`u.SCLK`) is NOT a registered name for a dotted pin and stays E3179.

use crate::common;

use mcc::{DiagnosticLevel, McIds, McURI};

const CODE: u32 = mcc::errcodes::PIN_CONFLICTING_OPTIONS;

/// Two options on the same pin group: pin 1 carries `PA1` (option 1) and
/// `PB5` (option 2).
const OPT_SRC: &str = r#"
component OPT
{
    pins = [
        io [1,2] = PA[1, 2]
                    | PB[5, 6]::GPIO(Provider)
    ]
}

module main
{
    io VDD
    OPT u
{body}
}
"#;

/// One build of `src`: how many times `CODE` fired.
fn conflicts_of(src: &str) -> usize {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/pin-option-conflict.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &uri);
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == CODE && d.level == DiagnosticLevel::Warning)
        .count()
}

/// Two different options reach pin 1: `PA1` and `PB5` — the conflict.
#[test]
fn pin_opt__two_options_on_one_pin_report() {
    let src = OPT_SRC.replace("{body}", "    u.PA1 -> VDD\n    u.PB5 -> VDD\n");
    assert_eq!(
        conflicts_of(&src),
        1,
        "pin 1 reached as PA1 and PB5 is the option conflict"
    );
}

/// The same option reaching two of its own pins never reports.
#[test]
fn pin_opt__same_option_twice_is_quiet() {
    let src = OPT_SRC.replace("{body}", "    u.PA1 -> VDD\n    u.PA2 -> VDD\n");
    assert_eq!(
        conflicts_of(&src),
        0,
        "PA1 and PA2 are two pins of one option, not a conflict"
    );
}

/// A pin id and one option name reaching the same pin never reports — the
/// raw id carries no option identity, so there is nothing to conflict with.
#[test]
fn pin_opt__id_plus_one_option_is_quiet() {
    let src = OPT_SRC.replace("{body}", "    u.1 -> VDD\n    u.PA1 -> VDD\n");
    assert_eq!(
        conflicts_of(&src),
        0,
        "the pin id spelling does not conflict with an option name"
    );
}

/// The alias form (`1 = A | B`, §2.7) is one function under several names:
/// both names reaching the pin are one option, so no conflict.
#[test]
fn pin_opt__alias_run_never_conflicts() {
    let src = r#"
component AL
{
    pins = [
        io 1 = VA | VB
    ]
}

module main
{
    io VDD
    AL u
    u.VA -> VDD
    u.VB -> VDD
}
"#;
    assert_eq!(
        conflicts_of(src),
        0,
        "VA and VB are aliases of one pin, not two options"
    );
}

/// A single-option group (no `|` at all) cannot conflict with itself.
#[test]
fn pin_opt__single_option_group_is_quiet() {
    let src = r#"
component PL
{
    pins = [
        io [1,2] = SPI0{SCLK, MOSI}
    ]
}

module main
{
    io VDD
    PL u
    u.SPI0.SCLK -> VDD
    u.SPI0.MOSI -> VDD
}
"#;
    assert_eq!(
        conflicts_of(src),
        0,
        "a single-option group cannot conflict with itself"
    );
}

/// The dotted curly-group face (U301): the dotted member `u.SPI0.SCLK`
/// resolves through the bus-port lane expansion and records option 0; the
/// flat `u.PB5` is option 1 — one physical pin asked for both is the
/// conflict, reported once.
#[test]
fn pin_opt__dotted_member_cross_option_reports() {
    let src = r#"
component CU
{
    pins = [
        io [1,2] = SPI0{SCLK, MOSI}
                    | PB[5, 6]
    ]
}

module main
{
    io VDD
    CU u
    u.SPI0.SCLK -> VDD
    u.PB5 -> VDD
}
"#;
    assert_eq!(
        conflicts_of(src),
        1,
        "pin 1 reached as SPI0.SCLK (dotted) and PB5 (flat) is the option conflict"
    );
}

/// The curly-group statement face records too: a one-lane `u.SPI0{SCLK}`
/// group against the flat `u.PB5` option is the same cross-option conflict.
#[test]
fn pin_opt__curly_group_face_cross_option_reports() {
    let src = r#"
component CG
{
    pins = [
        io [1,2] = SPI0{SCLK, MOSI}
                    | PB[5, 6]
    ]
}

module main
{
    io VDD
    CG u
    u.SPI0{SCLK} -> VDD
    u.PB5 -> VDD
}
"#;
    assert_eq!(
        conflicts_of(src),
        1,
        "the curly group lane records its option like the dotted spelling"
    );
}

/// The dotted face repeating the SAME option never reports — the second use
/// lands on the recorded ordinal, and identical ordinals are not a conflict.
#[test]
fn pin_opt__dotted_same_option_repeat_is_quiet() {
    let src = r#"
component DR
{
    pins = [
        io [1,2] = SPI0{SCLK, MOSI}
                    | PB[5, 6]
    ]
}

module main
{
    io VDD
    DR u
    u.SPI0.SCLK -> VDD
    u.SPI0.SCLK -> VDD
}
"#;
    assert_eq!(
        conflicts_of(src),
        0,
        "the same dotted member twice is one option, not a conflict"
    );
}

/// The raw pin id stays option-neutral on the dotted face too: `u.1` and the
/// dotted `u.SPI0.SCLK` reach the same pin without a conflict report.
#[test]
fn pin_opt__dotted_plus_id_is_quiet() {
    let src = r#"
component DI
{
    pins = [
        io [1,2] = SPI0{SCLK, MOSI}
                    | PB[5, 6]
    ]
}

module main
{
    io VDD
    DI u
    u.SPI0.SCLK -> VDD
    u.1 -> VDD
}
"#;
    assert_eq!(
        conflicts_of(src),
        0,
        "the pin id spelling carries no option and never conflicts"
    );
}
