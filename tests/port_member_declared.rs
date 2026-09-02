// Copyright (c) 2026 MCode
//
// Integration tests for the authoritative-declared-shape rule: a module
// io/out/in port's declaration is authoritative. Member/lane access against a
// port declared without members (scalar) is E3183 (BUS_MEMBER_ON_SCALAR_PORT);
// member/lane access against a membered/typed port is validated against its
// declared member set (undeclared member → E3181). Internal undeclared nets
// remain usage-defined and never trigger these gates.
//
// Reference: mcd doc/plan/io-port-declared-shape-rule.md.
//
// NOTE: These tests share global mcc state, so a mutex serializes them.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const E3183: u32 = mcc::errcodes::BUS_MEMBER_ON_SCALAR_PORT;
const E3181: u32 = mcc::errcodes::BUS_MEMBER_UNDECLARED;

/// Acquire lock, load + build `src`, return emitted codes (3181/3183 detail
/// printed on assertion failure via the returned code list).
fn codes_of(source: &str) -> Vec<u32> {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/port-member-declared.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    mcc::mcc_diagnose_all()
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Number of E3183 in the code list.
fn count3183(codes: &[u32]) -> usize {
    codes.iter().filter(|&&c| c == E3183).count()
}

// ── (a) scalar-declared io port + curly member use → E3183 ─────────────────
// us513 regression shape: bare `io MIC` + body `MIC{P,N}`.

#[test]
fn curly_member_on_scalar_io_port_errors_once() {
    let codes = codes_of(
        r#"
module main
{
    io MIC

    MIC{P,N} -> [GND, GND]
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        1,
        "exactly one E3183 for a curly member use of a scalar io port; codes: {codes:?}"
    );
}

#[test]
fn curly_member_on_scalar_out_port_errors() {
    let codes = codes_of(
        r#"
module main
{
    out spi1

    spi1{CS, SCLK} -> [GND, GND]
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        1,
        "curly member use of a scalar out port must be E3183; codes: {codes:?}"
    );
}

// ── (b) scalar-declared port + dotted member access → E3183 ────────────────

#[test]
fn dotted_member_on_scalar_io_port_errors_once() {
    let codes = codes_of(
        r#"
module main
{
    io SPI

    SPI.SCLK -> GND
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        1,
        "exactly one E3183 for a dotted member use of a scalar io port; codes: {codes:?}"
    );
}

// ── Membered/typed ports validate against their declared member set ────────

#[test]
fn declared_member_on_membered_port_is_clean() {
    let codes = codes_of(
        r#"
module main
{
    io SPI{SCLK, MOSI, CSN, MISO}

    SPI.SCLK -> GND
    SPI{SCLK, MOSI} -> [GND, GND]
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        0,
        "declared members on a membered port are legal; codes: {codes:?}"
    );
}

#[test]
fn whole_port_use_on_membered_declaration_is_clean() {
    // hbl1 us513 MIC regression shape, inverted: MIC is declared `{P,N}`, so
    // the whole-port `MIC{P,N}` use is legal and must not report E3183.
    let codes = codes_of(
        r#"
module main
{
    io MIC{P,N}

    MIC{P,N} -> [GND, GND]
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        0,
        "whole-port use matching a membered declaration is legal; codes: {codes:?}"
    );
}

#[test]
fn undeclared_member_on_membered_port_reports_e3181_not_e3183() {
    let codes = codes_of(
        r#"
module main
{
    io SPI{SCLK, MOSI}

    SPI.CSN -> GND
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        0,
        "undeclared member on a membered port is E3181, not E3183; codes: {codes:?}"
    );
    assert!(
        codes.contains(&E3181),
        "undeclared member on a membered port must report E3181; codes: {codes:?}"
    );
}

// ── Negative: whole-port scalar ↔ scalar stays legal (no E3183) ────────────

#[test]
fn whole_scalar_port_to_scalar_net_is_clean() {
    let codes = codes_of(
        r#"
module main
{
    out vout

    vout -> GND
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        0,
        "whole-port scalar connection is legal; codes: {codes:?}"
    );
}

// ── Positive: internal undeclared nets are never gated ─────────────────────

#[test]
fn undeclared_net_member_reference_is_not_gated() {
    // `phantom` is an internal net (declared nowhere, usage-defined): member
    // access on it is not a declared-port violation, so no E3183 may fire.
    let codes = codes_of(
        r#"
module main
{
    phantom.SIG -> GND
}
"#,
    );
    assert_eq!(
        count3183(&codes),
        0,
        "member reference on an undeclared net is not a declared-port violation; codes: {codes:?}"
    );
}
