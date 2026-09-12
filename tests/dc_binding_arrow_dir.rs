// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PWR-10 (`DC_BINDING_DIR_MISMATCH` = 6028, intent-design.md §5.3.2):
//! a direction-word power terminal must sit at the end of its own connection
//! chain its direction word claims — a source (psrc) face leads the chain /
//! the right of a `{L|R}` through, a sink (psnk) trails it / sits on the left.
//!
//! Adjudicated at the instantiation layer (stmt.rs `audit_dc_binding_dir`),
//! *after* the gapped member flatten, so the judged position is arrow-glyph
//! independent (members are stored source-first; `<-` swaps operands).
//!
//! Scope per the 2026-09-10 decision: both **module power ports** and **leaf
//! component power pins** are checked. The direction word stays authoritative
//! for the semantic rules (6011/6019/6021/pwrflow) — this is a Warning that
//! tells the author the chain disagrees with the declared direction contract.
//!
//! Exemption: a module wiring its own body into its own exported power-port
//! rows is internal implementation (the direction word is the contract to the
//! *parent* frame). Leaf component pins are always judged (no body to be self).
//!
//! Component pins carry `::DC(…)` contract rows the no-library single-string
//! harness resolves fine; **module-body** `psrc/psnk …::DC(…)` rows require
//! interface DC from the mcode library (power_intent_l1.rs build_project_codes
//! note), so module-port boards load through a recursive project + the library.

#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// A leaf converter with two directional DC power-group rows (psnk in / psrc
/// out) — the canonical `{VIN | VOUT}` through shape (curly_dc_face_rows SRC_PWR).
const LDO_PWR: &str = r#"
component LDO_PWR
{
    pins = [
        psnk [1,2] = VIN{Vin, GND}::DC(5V)
        psrc [3,2] = VOUT{Vout, GND}::DC(3.3V)
    ]
}

module top()
{
    LDO_PWR ldo

    [VMAIN_5V, GND] -> ldo{VIN | VOUT} -> [VDD_3V3, GND]
}
"#;

/// The same through with its faces reversed — `{VOUT | VIN}` puts the psnk
/// input on the output (right) face and the psrc output on the input (left)
/// face → each face contradicts its position → 6028.
const LDO_FLIPPED: &str = r#"
component LDO_PWR
{
    pins = [
        psnk [1,2] = VIN{Vin, GND}::DC(5V)
        psrc [3,2] = VOUT{Vout, GND}::DC(3.3V)
    ]
}

module top()
{
    LDO_PWR ldo

    [VMAIN_5V, GND] -> ldo{VOUT | VIN} -> [VDD_3V3, GND]
}
"#;

/// Module file declaring the power modules under test (module-body power rows
/// need the mcode library, hence the project harness below).
const POWER_MODULES: &str = r#"
module POWER_USB()
{
    psrc vin{V5V, GND}::DC(5V)
}

module POWER_LDO()
{
    psnk vin{V5V, GND}::DC(5V)
    psrc vout{VDD_3V3, GND}::DC(3.3V)
}
"#;

/// ① a source module-port face at the wrong (load) end: `POWER_USB` exports a
/// `psrc vin` source face, but the parent drives it from the chain tail →
/// 6028. Same literal pair both ways — only the arrow/side differs.
const MOD_PSRC_TAIL_MAIN: &str = r#"
use ./power.mc
module main
{
    conduit GND @role(main)
    POWER_USB USB
    [V5V, GND] -> USB.vin
}
"#;

/// ② the same module source face at the chain head (its correct end) → silent.
const MOD_PSRC_HEAD_MAIN: &str = r#"
use ./power.mc
module main
{
    conduit GND @role(main)
    POWER_USB USB
    USB.vin -> [V5V, GND]
}
"#;

/// ③ module through: `{vin | vout}` order right (psnk left / psrc right) is
/// silent; flipping the module's two exported faces → 6028.
const MOD_THROUGH_OK_MAIN: &str = r#"
use ./power.mc
module main
{
    conduit GND @role(main)
    POWER_LDO LDO
    [V5V, GND] -> LDO{vin | vout} -> [VDD_3V3, GND]
}
"#;

const MOD_THROUGH_FLIPPED_MAIN: &str = r#"
use ./power.mc
module main
{
    conduit GND @role(main)
    POWER_LDO LDO
    [V5V, GND] -> LDO{vout | vin} -> [VDD_3V3, GND]
}
"#;

/// ④ exemption — a module feeds its own exported `psrc` face from inside its
/// own body (the direction word is a contract to the parent frame; internal
/// wiring into the row is legal). Without the exemption this tail bind would
/// read as "psrc at the load end" → 6028; the exemption keeps it silent.
const POWER_USB_SELF: &str = r#"
module POWER_USB()
{
    psrc vin{V5V, GND}::DC(5V)

    V5V_RAW -> vin.V5V
    GND_RAW -> vin.GND
}
"#;

const MOD_SELF_INTERNAL_MAIN: &str = r#"
use ./power.mc
module main
{
    conduit GND @role(main)
    POWER_USB USB
}
"#;

/// ⑤ psbi is a conditional source — it may sit on either side, so it never
/// conflicts; direction-word-less DC rows carry no contract and never fire.
/// (Component pins only — no module power rows — so the single-string harness
/// resolves them without the library.)
const PSBI_AND_WORDLESS: &str = r#"
component ORING2
{
    pins = [
        [1,2] = [IN1, GND]
        [5,6] = [OUT, GND]
    ]
}

component BAT_PWR
{
    pins = [
        psbi [1,2] = BAT{VCC, GND}::DC(5V)
    ]
}

module top()
{
    ORING2 o
    BAT_PWR b

    b.BAT -> o{ [IN1, GND] | [OUT, GND] } -> [VMAIN_5V, GND]
}
"#;

// single-string harness: component-pin boards only (no interface DC)
fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/dc-binding-arrow-dir.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn msgs6028(src: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/dc-binding-arrow-dir.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == 6028)
        .map(|d| d.msg.clone())
        .collect()
}

// ── project harness: module-body `psrc/psnk …::DC(…)` rows need the library ───
/// Recursive project load over real temp files + the mcode library (module-body
/// power rows require interface DC, which the no-lib single-string harness
/// never loads). `tag` keeps each test's temp directory unique.
fn build_proj(tag: &str, power_mc: &str, main_mc: &str) -> Vec<u32> {
    let _lock = common::lock();
    let dir = std::env::temp_dir().join(format!("mcc-pwr10-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("power.mc"), power_mc).unwrap();
    std::fs::write(dir.join("main.mc"), main_mc).unwrap();
    let entry = dir.join("main.mc").canonicalize().unwrap();
    let uri: McURI = entry.to_string_lossy().to_string();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&dir);
    mcc::mcc_load_project(&uri);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    let _ = std::fs::remove_dir_all(&dir);
    codes
}

fn n6028(codes: &[u32]) -> usize {
    codes.iter().filter(|&&c| c == 6028).count()
}

// component through, group order right → silent
#[test]
fn comp_through_correct_order_is_silent() {
    let codes = build_codes(LDO_PWR);
    assert_eq!(
        n6028(&codes),
        0,
        "psnk-left / psrc-right {{VIN | VOUT}} through must not warn: {:?}",
        codes
    );
}

// component through, group order flipped → 6028
#[test]
fn comp_through_flipped_order_warns() {
    let codes = build_codes(LDO_FLIPPED);
    assert!(
        n6028(&codes) >= 1,
        "flipped {{VOUT | VIN}} puts each directional face on the wrong side: {:?}",
        codes
    );
    let m = msgs6028(LDO_FLIPPED);
    assert!(
        m.iter()
            .any(|msg| msg.contains("ldo.VIN") || msg.contains("ldo.VOUT")),
        "6028 message should name the offending terminal: {m:?}"
    );
}

// ① module source port at the tail → 6028
#[test]
fn module_psrc_at_tail_warns() {
    let codes = build_proj("mod-tail", POWER_MODULES, MOD_PSRC_TAIL_MAIN);
    assert!(
        n6028(&codes) >= 1,
        "a psrc module port driven from the chain tail must warn: {:?}",
        codes
    );
}

// ② module source port at the head → silent
#[test]
fn module_psrc_at_head_is_silent() {
    let codes = build_proj("mod-head", POWER_MODULES, MOD_PSRC_HEAD_MAIN);
    assert_eq!(
        n6028(&codes),
        0,
        "psrc module port leading its chain is the contract end: {:?}",
        codes
    );
}

// ③ module through, faces right → silent; flipped → 6028
#[test]
fn module_through_correct_order_is_silent() {
    let codes = build_proj("mod-thru-ok", POWER_MODULES, MOD_THROUGH_OK_MAIN);
    assert_eq!(
        n6028(&codes),
        0,
        "{{vin | vout}} with psnk-left / psrc-right faces is the contract end: {:?}",
        codes
    );
}

#[test]
fn module_through_flipped_order_warns() {
    let codes = build_proj("mod-thru-flip", POWER_MODULES, MOD_THROUGH_FLIPPED_MAIN);
    assert!(
        n6028(&codes) >= 1,
        "flipped {{vout | vin}} module through must warn: {:?}",
        codes
    );
}

// ④ self-owned internal wiring is exempt
#[test]
fn module_own_body_wiring_is_exempt() {
    let codes = build_proj("mod-self", POWER_USB_SELF, MOD_SELF_INTERNAL_MAIN);
    assert_eq!(
        n6028(&codes),
        0,
        "a module feeding its own exported psrc face internally is not a mismatch: {:?}",
        codes
    );
}

// ⑤ psbi never warns; direction-word-less rows never warn
#[test]
fn psbi_and_wordless_rows_never_warn() {
    let codes = build_codes(PSBI_AND_WORDLESS);
    assert_eq!(
        n6028(&codes),
        0,
        "psbi is conditional and plain DC rows carry no contract: {:?}",
        codes
    );
}
