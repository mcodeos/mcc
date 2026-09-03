// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase B (dianlu-tree refactor) P0.5 golden lock: the observable
//! auto-naming sequence (`_C1`/`_C2`/`_R1`) produced by
//! `McModuleInst::auto_name` from the position counters, as seen through the
//! flattened instance table, the net table, and the diagnostics. Locked
//! before Phase B moves the counters out of the model into
//! `InstantiationBuilder`, so the move is a pure relocation with zero
//! observable naming change.
//!
//! The per-kind counter semantics themselves (Phantom `@_phantom_<class>_<n>`
//! and Stub `@?<class>_<n>` sequences, which are backstops rarely reachable
//! through the public build API) are locked by the in-crate unit test
//! `mc_mod::tests::auto_name_sequence_lock`.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Build `main` and flatten; return (sorted instance paths, sorted net lines, codes).
fn build_all(src: &str) -> (Vec<String>, Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/auto-name.mc");
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut paths: Vec<String> = table.iter().map(|(_, e)| e.path.clone()).collect();
    paths.sort();
    let mut netlines: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        netlines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    netlines.sort();
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort();
    (paths, netlines, codes)
}

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";
const RES_COMP: &str =
    "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

#[test]
fn mat_aname__normal_sequence_lock() {
    // Two anonymous CAP constructions then one anonymous RES construction ->
    // sequential per-prefix counters: _C1, _C2, then _R1 (RES shares neither
    // the CAP prefix counter nor the ground-label namespace).
    let src = format!(
        "{CAP_COMP}{RES_COMP}module main {{\n    io VDD\n    io GND\n    CAP(1).Cap([VDD, GND])\n    CAP(1).Cap([VDD, GND])\n    RES(2).1 -> VDD\n}}"
    );
    let (paths, nets, codes) = build_all(&src);

    // Auto-name sequence: `_C1`/`_C2` before `_R1`; each device materializes
    // its two pins as child entries; the ground label `GND@7` (line 7 of the
    // fixture) carries the split-ground suffix path.
    assert_eq!(
        paths,
        vec![
            "main",
            "main.GND",
            "main.GND@7",
            "main.VDD",
            "main._C1",
            "main._C1.1",
            "main._C1.2",
            "main._C2",
            "main._C2.1",
            "main._C2.2",
            "main._R1",
            "main._R1.1",
            "main._R1.2",
        ],
        "auto-name Normal sequence changed (P0.5 lock)"
    );

    // Net table: both caps bridge VDD -> the GND@7 group; the resistor pulls
    // VDD into its pin 2 (pin 1 left floating).
    assert_eq!(
        nets,
        vec![
            "GND@7 <= [main.GND@7, main._C1.2]",
            "GND@7 <= [main.GND@7, main._C2.2]",
            "VDD <= [main.VDD, main._C1.1, main._C2.1, main._R1.2]",
        ],
        "net table around auto-named instances changed (P0.5 lock)"
    );

    // 5641 (unused ctor param cap/res) x2, 4116 (one pin of _R1 unconnected),
    // 4117 (bidirectional port main.GND left with no net). Set-equal check —
    // diagnostic insertion order is not part of the naming contract.
    assert_eq!(
        codes,
        vec![4116, 4117, 5641, 5641],
        "diagnostic codes around auto-named instances changed (P0.5 lock)"
    );
}
