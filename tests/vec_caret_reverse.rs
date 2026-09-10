// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: reverse `^` on a curly-mn node (`X{A, B | C, D}`) swaps the
// node's left/right port groups (vec-dianlu.md §6.3). `McPhrase::reverse()` and
// the MCAST_OPD_CARET parse handler route `Endpoint(Node)` through
// `std::mem::swap(input, output)`, so the netlist must re-wire the members:
//
//   [VA, VB] -> mcu{A, B | C, D}  -> [VC, VD]     // A->VA B->VB C->VC D->VD
//   [VA, VB] -> mcu{A, B | C, D}^ -> [VC, VD]     // A->VC B->VD C->VA D->VB
//
// Regression: this was documented as unimplemented (vec-dianlu §8.10.4 #1,
// "`^` reversal does not transpose a Node"), fixed in 1519947 and re-verified
// here end-to-end.

mod common;

use mcc::{McIds, McURI};

const NODE_SRC: &str = r#"
module MCU()
{
    io A
    io B
    io C
    io D
}

module main
{
    io VA
    io VB
    io VC
    io VD
    MCU mcu513
    [VA, VB] -> mcu513{A, B | C, D} -> [VC, VD]
}
"#;

const REVERSED_SRC: &str = r#"
module MCU()
{
    io A
    io B
    io C
    io D
}

module main
{
    io VA
    io VB
    io VC
    io VD
    MCU mcu513
    [VA, VB] -> mcu513{A, B | C, D}^ -> [VC, VD]
}
"#;

/// Build the module itself (connections, not the flat table).
fn build_inst(source: &str) -> mcc::McModuleInst {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/caret-parallel.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri).expect("build failed")
}

/// Collect all 2-point connection pairs as (left.path, right.path).
fn pairs(inst: &mcc::McModuleInst) -> Vec<(String, String)> {
    inst.connections
        .iter()
        .filter(|c| c.points.len() == 2)
        .map(|c| (c.points[0].path.clone(), c.points[1].path.clone()))
        .collect()
}

fn build_flat(source: &str) -> mcc::InstTable {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/caret-node.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");

    table
}

/// Net name that the given member path ends up on.
fn net_of(table: &mcc::InstTable, needle: &str) -> Option<String> {
    for net in table.get_nets() {
        for &pid in &net.points {
            if let Some(e) = table.get_entry(pid) {
                if e.path.ends_with(needle) {
                    return Some(net.name.clone());
                }
            }
        }
    }
    None
}

#[test]
fn caret_swaps_node_input_output() {
    // Without `^`: A->VA, B->VB, C->VC, D->VD (input [A,B], output [C,D]).
    let table = build_flat(NODE_SRC);
    let va = net_of(&table, ".VA").unwrap();
    let vb = net_of(&table, ".VB").unwrap();
    let vc = net_of(&table, ".VC").unwrap();
    let vd = net_of(&table, ".VD").unwrap();
    let mca = net_of(&table, "mcu513.A").unwrap();
    let mcb = net_of(&table, "mcu513.B").unwrap();
    let mccnet = net_of(&table, "mcu513.C").unwrap();
    let mcd = net_of(&table, "mcu513.D").unwrap();
    assert!(
        mca == va && mcb == vb && mccnet == vc && mcd == vd,
        "baseline: node members must map A->VA B->VB C->VC D->VD; \
         got A->{mca} B->{mcb} C->{mccnet} D->{mcd}"
    );

    // With `^`: input becomes [C,D], output becomes [A,B], so A->VC B->VD C->VA D->VB.
    let table = build_flat(REVERSED_SRC);
    let va = net_of(&table, ".VA").unwrap();
    let vb = net_of(&table, ".VB").unwrap();
    let vc = net_of(&table, ".VC").unwrap();
    let vd = net_of(&table, ".VD").unwrap();
    let mca = net_of(&table, "mcu513.A").unwrap();
    let mcb = net_of(&table, "mcu513.B").unwrap();
    let mccnet = net_of(&table, "mcu513.C").unwrap();
    let mcd = net_of(&table, "mcu513.D").unwrap();
    assert!(
        mca == vc && mcb == vd && mccnet == va && mcd == vb,
        "`^` must swap node input/output: expected A->VC B->VD C->VA D->VB; \
         got A->{mca} B->{mcb} C->{mccnet} D->{mcd}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// `^` on a two-pin parallel: a REAL reversal (vec-dianlu.md §6.3 / eval.md §5.6)
// ═══════════════════════════════════════════════════════════════════════════
//
// `R1 + R2` (two two-pin parts) stacks into a `1*2` node (`1*2 +- 1*2 = 1*2`):
// its left face is the two pin-1s, its right face the two pin-2s — genuinely
// different lists, so `^` swaps which face the neighbouring nets land on.
// This is the case the old syntactic judgement ("any `Parallel` is orderless")
// got wrong: it dropped the swap *and* warned E2903.

const RES2: &str = r#"
component RES2()
{
    pins = [
        1 = P1
        2 = P2
    ]
}
"#;

const PAR_SRC: &str = r#"
module main
{
    VEXT -> (R1::RES2() + R2::RES2()) -> GND
}
"#;

const PAR_REVERSED_SRC: &str = r#"
module main
{
    VEXT -> (R1::RES2() + R2::RES2())^ -> GND
}
"#;

/// Pair membership, order-insensitive.
fn has(got: &[(String, String)], a: &str, b: &str) -> bool {
    got.iter()
        .any(|(l, r)| (l == a && r == b) || (l == b && r == a))
}

#[test]
fn caret_on_twopin_parallel_swaps_the_faces() {
    let plain = pairs(&build_inst(&format!("{RES2}{PAR_SRC}")));
    let rev = pairs(&build_inst(&format!("{RES2}{PAR_REVERSED_SRC}")));

    // Baseline: the parallel's left face is the pin-1s, right face the pin-2s,
    // so `VEXT` lands on `R1.1` and `GND` on `R1.2`.
    assert!(has(&plain, "VEXT", "R1.1"), "baseline VEXT face: {plain:?}");
    assert!(has(&plain, "GND", "R1.2"), "baseline GND face: {plain:?}");
    assert!(has(&plain, "R1.1", "R2.1"), "baseline lanes: {plain:?}");
    assert!(has(&plain, "R1.2", "R2.2"), "baseline lanes: {plain:?}");

    // Reversed: the faces swap, so the neighbouring nets move to the other
    // pin — `VEXT` now meets pin 2, `GND` meets pin 1.
    assert!(
        has(&rev, "VEXT", "R1.2"),
        "`^` must move VEXT to pin 2: {rev:?}"
    );
    assert!(
        has(&rev, "GND", "R1.1"),
        "`^` must move GND to pin 1: {rev:?}"
    );
    assert!(has(&rev, "R1.1", "R2.1"), "lanes unchanged by `^`: {rev:?}");
    assert!(has(&rev, "R1.2", "R2.2"), "lanes unchanged by `^`: {rev:?}");
}
