// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: instance option syntax `inst{a|b}` on connection lines
// must parse and expand (P0-1).
//
// Regression: the mca.y grammar declared the CURLY_MN option productions with
// `mc_phrase` inside the braces, but `mc_phrase` has no bare-`mc_opd`
// derivation — so a plain identifier option (`modldo{vin|vout}`) could never
// be reduced by the GLR parser, and the whole connection line was dropped with
// E1003/E1002 (e.g. mcs/hbl/src/hbl.mc L24 `V5V ->
// modldo{vin|vout} -> V3V3`). The fix switches the brace elements to
// `mc_idans` (identifier lists) and walks the full id chain in the semantic
// layer so multi-member options like `mcu513{ MIC | DAC_OUT, SPK_MUTE }` keep
// all members.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

const SOURCE: &str = r#"
module POWER_LDO()
{
    io vin
    io vout
}

module main
{
    io V5V
    io V3V3
    POWER_LDO modldo
    V5V -> modldo{vin|vout} -> V3V3
}
"#;

fn build_flat(source: &str) -> mcc::InstTable {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/curly-option.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");

    table
}

#[test]
fn mat_curlopt__connection_parses_and_builds() {
    // Previously: E1003/E1002 at the connection line, whole line dropped.
    let table = build_flat(SOURCE);

    let net_names: Vec<String> = table.get_nets().iter().map(|n| n.name.clone()).collect();
    let joined = net_names.join(" ");

    // The option `{vin|vout}` must create the instance's two pin buses and
    // connect them: V5V -> modldo.vin, modldo.vout -> V3V3.
    assert!(
        net_names.iter().any(|n| n == "V5V"),
        "V5V net missing, nets: {joined}"
    );
    assert!(
        net_names.iter().any(|n| n == "V3V3"),
        "V3V3 net missing, nets: {joined}"
    );

    // modldo.vin must land on the same net as V5V (input side) and
    // modldo.vout on the same net as V3V3 (output side).
    for net in table.get_nets() {
        for &point_id in &net.points {
            let Some(entry) = table.get_entry(point_id) else {
                continue;
            };
            if entry.path.ends_with("modldo.vin") {
                assert_eq!(net.name, "V5V", "modldo.vin should join V5V");
            }
            if entry.path.ends_with("modldo.vout") {
                assert_eq!(net.name, "V3V3", "modldo.vout should join V3V3");
            }
        }
    }
}

#[test]
fn mat_curlopt__left_and_right_members_created() {
    // `mcu513{ MIC | DAC_OUT, SPK_MUTE }` — the right option has two members;
    // both must survive the id-chain extraction in the semantic layer.
    // The right side is a 2*1 column `[VOUT, VOUT2]` so the series stays
    // legal under vec-dianlu.md §5.2 (node right `N*1` vs column `N*1`); a
    // single `VOUT` would be an illegal `2*1 -> 1*1` broadcast (§5.3.1).
    let source = r#"
module MCU()
{
    io MIC
    io DAC_OUT
    io SPK_MUTE
}

module main
{
    io VIN
    io VOUT
    io VOUT2
    MCU mcu513
    VIN -> mcu513{ MIC | DAC_OUT, SPK_MUTE } -> [VOUT, VOUT2]
}
"#;
    let table = build_flat(source);

    let net_names: Vec<String> = table.get_nets().iter().map(|n| n.name.clone()).collect();
    let joined = net_names.join(" ");

    // Collect every entry path so a missing member fails loudly (a bare
    // `ends_with` assert inside the loop would silently pass if the member
    // were dropped during parsing).
    let mut entry_paths: Vec<String> = Vec::new();
    for net in table.get_nets() {
        for &point_id in &net.points {
            if let Some(entry) = table.get_entry(point_id) {
                entry_paths.push(entry.path.clone());
            }
        }
    }
    let paths_joined = entry_paths.join(" ");
    assert!(
        entry_paths.iter().any(|p| p.ends_with("mcu513.MIC")),
        "mcu513.MIC missing from netlist: {paths_joined}"
    );
    assert!(
        entry_paths.iter().any(|p| p.ends_with("mcu513.DAC_OUT")),
        "mcu513.DAC_OUT missing from netlist: {paths_joined}"
    );
    assert!(
        entry_paths.iter().any(|p| p.ends_with("mcu513.SPK_MUTE")),
        "mcu513.SPK_MUTE missing from netlist (trailing option member dropped?): {paths_joined}"
    );

    // Input side: MIC joins VIN; output side: DAC_OUT joins VOUT and
    // SPK_MUTE joins VOUT2 (row order preserved, no broadcast).
    for net in table.get_nets() {
        for &point_id in &net.points {
            let Some(entry) = table.get_entry(point_id) else {
                continue;
            };
            if entry.path.ends_with("mcu513.MIC") {
                assert_eq!(net.name, "VIN", "MIC should join VIN, nets: {joined}");
            }
            if entry.path.ends_with("mcu513.DAC_OUT") {
                assert_eq!(net.name, "VOUT", "DAC_OUT should join VOUT, nets: {joined}");
            }
            if entry.path.ends_with("mcu513.SPK_MUTE") {
                assert_eq!(
                    net.name, "VOUT2",
                    "SPK_MUTE should join VOUT2 (right option lost trailing members), nets: {joined}"
                );
            }
        }
    }
}
