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
