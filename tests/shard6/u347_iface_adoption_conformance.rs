// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U347 C1/C2 — the interface adoption conformance reports (CIMP-OPEN U347
//! ruling 2026-09-28). Both faces were silent drops at the adoption choke
//! point (`Mc2Interface::with_params` / `with_ids_and_params`):
//!
//! - **C1 → E3190** (`IFACE_ARG_EXCESS`): constructor arguments past the
//!   declared formal table are dropped by the cond-environment zip. The
//!   surplus is reported; the adoption itself still builds (an error does not
//!   block instantiation) — the retained pins stay bound.
//! - **C2 → E3191** (`IFACE_ROW_UNBOUND_PARAM`): a cond branch whose
//!   pin-row name is computed from a parameter (`"VCC" + canon(volt)`) with
//!   no argument binding it — the row lands in the parsed table's dynamic
//!   pins this face never resolves, so it registered no pin and said nothing.
//!   A declared default covers its parameter (the author stated a value), so
//!   `PWR()` against `PWR(volt = 3.3V)` stays quiet on E3191.
//!
//! Axes ① (role binding) and ② (pin shape) of U347 are covered by the
//! existing family (E4104/E4185/E4184 role gates; E4186/E3111/E5151 shape
//! gates) and are not re-locked here.
//!
//! Fixtures are self-contained (inline interfaces, no system library) so the
//! locks cannot drift with the live `~/.mcode` root.

#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;
use mcc::McURI;

/// A cond-parameterized interface: both branches compute row 1's name from
/// `volt`; row 2 is static GND. Adopted by FLASH over two pin IDs.
fn pwr_src(args: &str, decl_tail: &str) -> String {
    format!(
        "interface PWR(volt::UV.VOLT{decl_tail})\n{{\n    if (volt < 0V)\n        pins = [\n            1 = \"VCC\" + canon(volt), \"neg\"\n            2 = GND, \"gnd\"\n        ]\n    else\n        pins = [\n            1 = \"VCC\" + canon(volt), \"pos\"\n            2 = GND, \"gnd\"\n        ]\n}}\n\ncomponent FLASH\n{{\n    name = \"FLASH\"\n    pins = [\n        [10, 11] = U1::PWR({args})\n    ]\n}}\n\nmodule main\n{{\n    FLASH f1\n}}\n"
    )
}

/// Diagnostic codes for `src`, deduplicated in emit order.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &u, 1000);
    let mut seen = Vec::new();
    for d in mcc::mcc_diagnose_all() {
        if !seen.contains(&d.code) {
            seen.push(d.code);
        }
    }
    seen
}

fn messages_of(src: &str, uri: &str, code: u32) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &u, 1000);
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.msg.clone())
        .collect()
}

/// C1: a surplus constructor argument is reported, not dropped in silence.
#[test]
fn e3190__surplus_args_reported() {
    let codes = codes_of(
        &pwr_src("3.3V, 9", ""),
        "mem://u347/c1/main.mc",
    );
    assert!(
        codes.contains(&3190),
        "expected E3190 among {codes:?}"
    );
}

/// C1 message cites the interface name, the declared arity, and the passed
/// count — the three numbers the fix needs.
#[test]
fn e3190__message_cites_name_arity_passed() {
    let msgs = messages_of(
        &pwr_src("3.3V, 9", ""),
        "mem://u347/c1msg/main.mc",
        3190,
    );
    let m = msgs.join("\n");
    assert!(m.contains("'U1'"), "message names the interface: {m}");
    assert!(m.contains('1'), "message cites the declared arity: {m}");
    assert!(m.contains('2'), "message cites the passed count: {m}");
}

/// C1 positive control: the literal-complete adoption stays quiet.
#[test]
fn positive__full_args_quiet() {
    let codes = codes_of(
        &pwr_src("3.3V", ""),
        "mem://u347/pos/main.mc",
    );
    assert!(!codes.contains(&3190), "E3190 must stay quiet: {codes:?}");
    assert!(!codes.contains(&3191), "E3191 must stay quiet: {codes:?}");
}

/// C2: the missing argument leaves the computed row unbound — reported.
#[test]
fn e3191__missing_args_reported() {
    let codes = codes_of(
        &pwr_src("", ""),
        "mem://u347/c2/main.mc",
    );
    assert!(
        codes.contains(&3191),
        "expected E3191 among {codes:?}"
    );
}

/// C2 message names the interface and the unbound parameter.
#[test]
fn e3191__message_names_param() {
    let msgs = messages_of(
        &pwr_src("", ""),
        "mem://u347/c2msg/main.mc",
        3191,
    );
    let m = msgs.join("\n");
    assert!(m.contains("'U1'"), "message names the interface: {m}");
    assert!(m.contains("volt"), "message names the parameter: {m}");
}

/// A declared default covers its parameter — the author stated a value, so
/// the adoption stays quiet on E3191. (The computed row still does not
/// materialize from the default; the existing E3111 width gate flags that
/// downstream — this lock pins only the E3191 silence.)
#[test]
fn default_covers_report() {
    let codes = codes_of(
        &pwr_src("", " = 3.3V"),
        "mem://u347/c3/main.mc",
    );
    assert!(!codes.contains(&3191), "E3191 must stay quiet: {codes:?}");
}
