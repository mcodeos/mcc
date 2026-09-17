// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: which formal port an actual argument may reach at all (CIMP U31).
//
// `SINK s(V3V3)` binds by position when neither side declares a voltage. A
// candidate is a port that DECLARES a power contract -- a power direction word,
// a `::DC` face pair, or a voltage (AGENTS.md, "no guessing from names": shape
// is not evidence either) -- and the candidates run in declaration order, so a
// signal bus like `io MIC{P,N}` cannot take the argument by position. A
// candidate set drawn by SHAPE, or one drawn from the sorted port table, lands
// the supply on the microphone's differential pair with zero diagnostics: that
// is the failure these cases pin.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

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

/// Two board nets, each carrying one of the supplies. The supplies declare a
/// voltage; the sub-module's ports deliberately do not, so there is nothing to
/// pair on by value and the POSITION is what decides the binding.
const DECLS: &str = r#"    A -> V3V3::PWRLINE(3.3V)
    B -> V1V2::PWRLINE(1.2V)"#;

/// A board whose top module carries the supplies, handed to a sub-module that
/// declares `ports` in exactly the written order.
fn board(ports: &str, args: &str) -> String {
    format!(
        r#"{IFACE}
module SINK({ports})
{{
}}

module main
{{
    io A
    io B
{DECLS}
    SINK s({args})
}}
"#
    )
}

/// Build `source` and return the top module's nets, each as the set of point
/// paths it holds.
fn nets(tag: &str, source: &str) -> Vec<BTreeSet<String>> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u31-{tag}.mc");
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

/// `INST_ARG_NO_FORMAL_PORT`: an actual with no formal port left to bind.
const E4151: u32 = 4151;

/// Diagnostic codes produced by building `main` in `source`.
fn codes(tag: &str, source: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u31-{tag}.mc");
    mcc::mcc_load_from_string(&uri, source);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// Is `path` the point named `name` -- the name itself, or a dotted path whose
/// last segment it is? Comparing whole segments keeps `s.V3V3` from matching on
/// the `V3V3` inside `s.V3V3_A`.
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

/// Has the supply `supply` landed anywhere among `port`'s members?
fn took_supply(nets: &[BTreeSet<String>], supply: &str, members: &[&str]) -> bool {
    members.iter().any(|m| pairs_with(nets, supply, m))
}

/// A membered port that declares no power contract is not a candidate at all,
/// so it cannot take the argument however the ports are ordered. `AAA{P,N}`
/// sorts before `ZRAIL{VDDIO,GND}`, which is the order the ports come out in.
#[test]
fn u31__a_signal_bus_never_takes_a_supply() {
    // Written declaration order: the supply port FIRST, the signal bus second.
    let ports = "psnk ZRAIL{VDDIO,GND}, io AAA{P,N}";
    let n = nets("signal-bus", &board(ports, "V3V3"));

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

/// The same when the signal bus is written first. A candidate set drawn by
/// shape, or taken in the sorted port table's order, hands it the supply.
#[test]
fn u31__a_signal_bus_written_first_takes_nothing() {
    let ports = "io AAA{P,N}, psnk ZRAIL{VDDIO,GND}";
    let n = nets("signal-first", &board(ports, "V3V3"));

    assert!(
        !took_supply(&n, "V3V3", &["s.AAA.P", "s.AAA.N"]),
        "a signal bus written first must not take the supply: {}",
        dump(&n)
    );
    assert!(
        pairs_with(&n, "V3V3", "s.ZRAIL.VDDIO"),
        "the supply belongs on the only port that declares a power contract: {}",
        dump(&n)
    );
}

/// Between two supply ports the slots run in DECLARATION order (source order,
/// §11), not in the alphabetical order a sorted port map hands out. Declared
/// `ZRAIL` then `ARAIL`; the first argument takes `ZRAIL`.
#[test]
fn u31__slots_run_in_declaration_order() {
    let ports = "psnk ZRAIL{ZVDD,GND}, psnk ARAIL{AVDD,GND}";
    let n = nets("slot-order", &board(ports, "V3V3, V1V2"));

    assert!(
        pairs_with(&n, "V3V3", "s.ZRAIL.ZVDD"),
        "the first argument takes the first declared supply port, not the first alphabetically: {}",
        dump(&n)
    );
    assert!(
        pairs_with(&n, "V1V2", "s.ARAIL.AVDD"),
        "the second argument takes the second declared supply port: {}",
        dump(&n)
    );
}

/// An unnamed bracket supply port is a real port: it declares a power contract,
/// so it is a candidate, and its members are reachable under their flat names
/// (`s.VDDIO`). A port with no members is one no argument can bind to, and one
/// that publishes no path for the parent to reach.
#[test]
fn u31__an_unnamed_bracket_supply_is_bindable() {
    let ports = "psnk [VDDIO,GND]";
    let n = nets("anonymous", &board(ports, "V3V3"));

    assert!(
        pairs_with(&n, "V3V3", "s.VDDIO"),
        "an unnamed supply port is a candidate and publishes its members: {}",
        dump(&n)
    );
}

/// An argument with no formal port left on the DECLARATION form (`SINK s(...)`)
/// is reported at user level, the same way the call-site path reports it. The
/// two paths share the ordering criterion but draw their candidate sets
/// separately, so a lock on one path says nothing about the other.
#[test]
fn u31__an_extra_argument_reaches_the_build_report() {
    let ports = "psnk ZRAIL{VDDIO,GND}, io AAA{P,N}";
    let c = codes("extra-arg-diag", &board(ports, "V3V3, V1V2"));

    assert!(
        c.contains(&E4151),
        "the second argument has no port to reach and must be reported, not dropped: codes={c:?}"
    );
}

/// The control: a complete binding reports nothing.
#[test]
fn u31__a_fully_bound_declaration_reports_no_extra_argument() {
    let ports = "psnk ZRAIL{ZVDD,GND}, psnk ARAIL{AVDD,GND}";
    let c = codes("bound-diag", &board(ports, "V3V3, V1V2"));

    assert!(
        !c.contains(&E4151),
        "both arguments have a formal port; the code must not fire on a complete binding: codes={c:?}"
    );
}
