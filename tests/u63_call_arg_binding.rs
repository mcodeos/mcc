// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: which formal port an actual argument may reach on the CALL
// SITE path (CIMP U63) -- `MIC_SIP MIC` declares an instance with no args, and
// `MIC(V3V3)` re-calls it to bind the argument.
//
// The candidate set runs in two stages: the callee's own declaration first
// (a power direction word, a `::DC` face pair, or a declared voltage, in
// declaration order), and the shape set only when the callee declares no power
// contract at all. A shape set drawn unconditionally puts `io AAA{P,N}` in the
// running and, because the port table comes out alphabetically rather than in
// written order, hands it the supply on the positional fallback with zero
// diagnostics: that is the failure these cases pin. A candidate set narrowed
// without the second stage silently stops binding every passive leaf.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// A two-member supply interface. Its rails are the author's own labels, which
/// is what keeps the fixture honest about names deciding nothing.
const IFACE: &str = r#"
interface PWRLINE(volt)
{
    voltage = volt

    pins = [
        1 = VPOS, "positive", voltage:0.0V
        2 = GND, "ground", voltage:0.0V
    ]
}
"#;

/// A board whose top module carries the supplies and declares the sub-module
/// instance WITHOUT args -- the re-call line is what binds them.
fn board(ports: &str, call: &str) -> String {
    format!(
        r#"{IFACE}
module SINK({ports})
{{
}}

module main
{{
    io A
    io B
    A -> V3V3::PWRLINE(3.3V)
    B -> V1V2::PWRLINE(1.2V)
    SINK s
    {call}
}}
"#
    )
}

/// Build `source` and return the top module's nets, each as the set of point
/// paths it holds.
fn nets(tag: &str, source: &str) -> Vec<BTreeSet<String>> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u63-{tag}.mc");
    mcc::mcc_load_from_string(&uri, source);
    let (_, _, _, store) = mcc::mcc_build_with_nets(&McIds::from("main"), &uri)
        .unwrap_or_else(|_| panic!("build failed for {tag}"));

    let mut out: Vec<BTreeSet<String>> = Vec::new();
    if let Some(table) = store.get("main") {
        for (_, pts) in table.iter() {
            let mut net: BTreeSet<String> = BTreeSet::new();
            for p in pts.iter() {
                net.insert(p.path.clone());
            }
            if !net.is_empty() {
                out.push(net);
            }
        }
    }
    out
}

/// Is `path` the point named `name` -- the name itself, or a dotted path whose
/// last segment it is? Comparing whole segments keeps `s.ZRAIL` from matching
/// on the `ZRAIL` inside `s.ZRAIL_A`.
fn is_point(path: &str, name: &str) -> bool {
    path == name || path.ends_with(&format!(".{name}"))
}

/// Is `path` one of the points the declared supply `supply` contributes? A
/// declaration `V3V3::PWRLINE(3.3V)` carries its interface's members over, so it
/// is a whole family of points (`V3V3.VPOS`, `V3V3.GND`), not one.
fn of_supply(path: &str, supply: &str) -> bool {
    path == supply || path.starts_with(&format!("{supply}."))
}

/// Do the supply `supply` and the point `port` share a net?
fn pairs_with(nets: &[BTreeSet<String>], supply: &str, port: &str) -> bool {
    nets.iter()
        .any(|n| n.iter().any(|p| of_supply(p, supply)) && n.iter().any(|p| is_point(p, port)))
}

/// The net holding `name`, for the failure messages: a wrong landing is only
/// readable when the nets are on screen.
fn dump(nets: &[BTreeSet<String>]) -> String {
    nets.iter()
        .map(|n| n.iter().cloned().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Has the supply `supply` landed anywhere among `members`?
fn took_supply(nets: &[BTreeSet<String>], supply: &str, members: &[&str]) -> bool {
    members.iter().any(|m| pairs_with(nets, supply, m))
}

/// A membered port that declares no power contract is not a candidate at all in
/// stage 1, so it cannot take the argument however the ports are ordered. The
/// written order declares the supply port FIRST while the sorted port table
/// hands out `AAA` first: the positional fallback must not follow the table.
#[test]
fn u63__a_re_call_never_lands_a_supply_on_a_signal_bus() {
    let ports = "psnk ZRAIL{VDDIO,GND}, io AAA{P,N}";
    let n = nets("signal-bus", &board(ports, "s(V3V3)"));

    assert!(
        pairs_with(&n, "V3V3", "s.ZRAIL.VDDIO"),
        "the supply belongs on the port declaring a power contract: {}",
        dump(&n)
    );
    assert!(
        !took_supply(&n, "V3V3", &["s.AAA.P", "s.AAA.N"]),
        "a signal bus must never take a supply, silently or otherwise: {}",
        dump(&n)
    );
}

/// Between two supply ports the slots run in DECLARATION order (source order,
/// §11), not in the alphabetical order a sorted port map hands out. Declared
/// `ZRAIL` then `ARAIL`: the argument takes `ZRAIL`, and `ARAIL` -- first in the
/// table -- stays empty.
#[test]
fn u63__slots_run_in_declaration_order() {
    let ports = "psnk ZRAIL{ZVDD,GND}, psnk ARAIL{AVDD,GND}";
    let n = nets("slot-order", &board(ports, "s(V3V3)"));

    assert!(
        pairs_with(&n, "V3V3", "s.ZRAIL.ZVDD"),
        "the argument takes the first declared supply port, not the first alphabetically: {}",
        dump(&n)
    );
    assert!(
        !took_supply(&n, "V3V3", &["s.AVDD", "s.AGND"]),
        "the port declared second must not take the argument by table order: {}",
        dump(&n)
    );
}

/// Stage 2: a callee declaring no power contract at all has no declaration to
/// bind by, and its argument list is an ordered list of connection endpoints.
/// Dropping the fallback would leave `V3V3` bound to nothing here, silently.
#[test]
fn u63__a_contract_less_callee_still_binds_its_endpoints() {
    let ports = "io P{A,B}";
    let n = nets("fallback", &board(ports, "s(V3V3)"));

    assert!(
        pairs_with(&n, "V3V3", "s.P.A") && pairs_with(&n, "V3V3", "s.P.B"),
        "a callee with no declared power contract still takes the ordered endpoint list: {}",
        dump(&n)
    );
}
