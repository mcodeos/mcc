// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! P0.3 — single-member range `res[4]` resolves to a scalar member (`res4`),
//! not a vector group (architecture doc §11.3, contract E).
//!
//! The vector pipeline only recognizes a member set when it has ≥2 expanded
//! names (`expanded.len() >= 2`); a single-member range is a scalar member:
//!   - materializes as `main.res4` (scalar), never a literal `res[4]` path
//!   - no `McVectorInst` grouping (contract E guard at mc_inst.rs `>= 2`)
//!   - no E3179 (COMPONENT_PIN_NOT_FOUND) phantom
//!   - exactly one member — no sibling probing to res5
//!
//! KNOWN-FAILURE AT v0.7.11 (Phase 0 baseline): the current code creates a
//! literal `res[4]` instance (`main.res[4]`, pins `main.res[4].1/.2`) — the
//! `>= 2` guard at mc_inst.rs makes `should_expand` false for a single member,
//! so `names_to_create = ["res[4]"]` (literal bracket name leaks through
//! flatten, violating invariant B). This test asserts the Phase 1 pipeline
//! target (step 3 len==1 → `Endpoint(Single)` + scalar materialization); it
//! flips green when the vector pipeline lands (Phase 1.2 / step 3, contract E).

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests sharing mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const RES_COMP: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";

/// Build `main` from `src`, returning (paths, nets, diagnostic codes).
fn build(src: &str, uri: &str) -> (Vec<String>, Vec<String>, Vec<u32>) {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &u, 1000).expect("flat build");

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

    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (paths, netlines, codes)
}

fn assert_no_path_containing(paths: &[String], fragment: &str, what: &str) {
    for p in paths {
        assert!(
            !p.contains(fragment),
            "{what}: path '{p}' must not contain '{fragment}'; got {paths:?}"
        );
    }
}

#[test]
fn single_member_range_res4_is_scalar() {
    let src = format!(
        "{RES_COMP}module main {{\n    io VDD\n    io NET\n    io VCC\n    func M() {{\n        res[4]::RES(0)\n        res[4].Pullup([NET, VCC])\n    }}\n}}\n"
    );
    let (paths, nets, codes) = build(&src, "/mcc/single-member-range.mc");

    // Scalar member `res4` materializes — never a literal `res[4]` path.
    assert!(
        paths.iter().any(|p| p == "main.res4"),
        "res4 materialized as scalar member; got {paths:?}"
    );
    assert_no_path_containing(&paths, "res[4]", "single-member range");

    // Exactly one member — no sibling probing to res5.
    assert!(
        !paths.iter().any(|p| p == "main.res5"),
        "no res5 sibling probed; got {paths:?}"
    );

    // No E3179 phantom.
    assert!(
        !codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "no E3179; got {codes:?}"
    );

    // res4 pins land on the real NET/VCC nets (scalar broadcast, one member).
    let n1 = nets.iter().find(|n| n.contains("main.NET")).expect("NET net");
    assert!(
        n1.contains("main.res4.1"),
        "res4 pin 1 on NET; got {n1}"
    );
    let n2 = nets.iter().find(|n| n.contains("main.VCC")).expect("VCC net");
    assert!(
        n2.contains("main.res4.2"),
        "res4 pin 2 on VCC; got {n2}"
    );
}
