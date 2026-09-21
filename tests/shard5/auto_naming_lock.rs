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
//! and Stub `@?<class>_<n>` sequences — backstops with no reachable consumer
//! today, see CIMP section 1 U80 O8) are locked by the in-crate unit test
//! `instant::mc_mod::tests::mat_aname__sequence_lock`
//! (`src/instant/mc_mod/mod.rs`).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

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
    // its two pins as child entries. The bare-`GND` statements union into the
    // single `main.GND` net (the flat layer no longer splits ground by
    // statement, so no `GND@<line>` fragment labels appear in the netlist).
    assert_eq!(
        paths,
        vec![
            "main",
            "main.GND",
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

    // Net table: both caps bridge VDD -> the single unified `GND` net (both
    // returns + the `main.GND` port share one copper); the resistor pulls VDD
    // into its pin 2 (pin 1 left floating).
    assert_eq!(
        nets,
        vec![
            "GND <= [main.GND, main._C1.2, main._C2.2]",
            "VDD <= [main.VDD, main._C1.1, main._C2.1, main._R1.2]",
        ],
        "net table around auto-named instances changed (P0.5 lock)"
    );

    // 5641 (unused ctor param cap/res) x2, 4116 (one pin of _R1 unconnected),
    // 4119 (that same pad sits on no net — `_R1.1` is direction-free, so the
    // directional float checks cannot see it). `main.GND` is a real unified net
    // (port + both cap returns); floating (4117/4114) never fire on top-module
    // ports, so only _R1's lone pin 1 reports 4116. Set-equal check —
    // diagnostic insertion order is not part of the naming contract.
    assert_eq!(
        codes,
        vec![4116, 4119, 5641, 5641],
        "diagnostic codes around auto-named instances changed (P0.5 lock)"
    );
}

// ── U170 lock: bare anonymous zero-arg constructions of non-two-pin classes ──
// The retired parse-time eager Endpoint arm (semantic/basic/mc_fcall.rs, U170
// retirement comment) used to swallow bare `TP()` constructions whole: the
// `@`-anonymous component was never stored (semantic/module/mod.rs add_component,
// P2-10) and Pass2 instantiates constructions only from FuncCall phrases
// (instant/mc_mod/funccall.rs), so the class silently produced zero instances
// with zero diagnostics — and inside a group it poisoned the whole connection
// statement. Now every bare construction keeps the FuncCall form (the P2-12
// contract two-pin classes already had) and Pass2 auto-names + wires it.
const TP_COMP: &str = "component TP() {\n    pins = [\n        1 = 1\n    ]\n}\n";

#[test]
fn mat_u170__bare_anon_one_pin_construction_instantiates_and_wires() {
    // Replicated (`TP()*2`) and single bare forms both instantiate; every
    // single-pin device lands its only pin on the driven net. Auto-names use
    // the Normal `_TP<n>` family (Pass2 counter, one sequence per prefix).
    let src = format!(
        "{TP_COMP}module main {{\n    io GND\n    TP()*2 -> GND*2\n    TP() -> GND\n}}"
    );
    let (paths, nets, codes) = build_all(&src);

    assert_eq!(
        paths,
        vec![
            "main",
            "main.GND",
            "main._TP1",
            "main._TP1.1",
            "main._TP2",
            "main._TP2.1",
            "main._TP3",
            "main._TP3.1",
        ],
        "bare anonymous one-pin constructions must materialize as auto-named instances (U170)"
    );
    assert_eq!(
        nets,
        vec!["GND <= [main.GND, main._TP1.1, main._TP2.1, main._TP3.1]"],
        "every bare-construction pin must sit on the target net (U170)"
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "bare constructions must stay diagnostic-free when wired (U170)"
    );
}

#[test]
fn mat_u170__bare_construction_inside_group_no_longer_poisons_the_statement() {
    // The hbl SPEAKER_M shape: a bare construction inside a parallel group,
    // the group chained onward. Before the U170 fix the group kept a bare
    // `@TP1` junction point and dropped the statement's wiring.
    let src = format!(
        "{TP_COMP}{RES_COMP}module main {{\n    io VDD\n    io GND\n    (VDD + TP()) -> RES(1).1 -> GND\n}}"
    );
    let (paths, nets, codes) = build_all(&src);

    assert!(
        paths.contains(&"main._TP1".to_string()) && paths.contains(&"main._TP1.1".to_string()),
        "group member bare construction must instantiate (U170): {paths:?}"
    );
    assert!(
        paths.contains(&"main._R1".to_string()) && paths.contains(&"main._R1.1".to_string()),
        "the chained statement must still instantiate its downstream device (U170): {paths:?}"
    );
    // Series semantics: the whole group sits left of `_R1`, so the group
    // member's pin and the chain head (RES pin 1) share one net.
    let tp_net = nets
        .iter()
        .find(|l| l.contains("main._TP1.1"))
        .expect("the group member pin sits on some net (U170)");
    assert!(
        tp_net.contains("main._R1.1") && tp_net.contains("main.VDD"),
        "group member pin must land on the chain-head net (U170): {tp_net}"
    );
    assert!(
        !codes.contains(&4119),
        "no silently floating pads may remain after the U170 fix: {codes:?}"
    );
}

