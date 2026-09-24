// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection-time pin option conflict detection (§4.2 check 2 / §4.3
//! `used_options`, U289 C1).
//!
//! A `|` option group gives one physical pin several names. When two names
//! of DIFFERENT options reach the same pin through connections, the pin is
//! being asked to carry two functions at once — E5156, reported at the
//! resolving connection. Names of ONE option never conflict: the alias form
//! (`1 = A | B`, a single-pinid row, §2.7) shares one option ordinal, and
//! two pins of the same option are two pins.
//!
//! Known boundary (not locked here as expected-pass): a DOTTED option member
//! (`u.SPI0.SCLK`, interface or bus option) is silently dropped before it
//! reaches the resolver, so a cross-option conflict written in dotted form
//! cannot fire today. That drop is a separate pre-existing gap (zero
//! diagnostics on a vanished statement); this lock holds the bare-name face,
//! which is what the resolver actually sees.

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
