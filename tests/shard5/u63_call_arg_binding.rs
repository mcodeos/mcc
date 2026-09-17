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

/// Diagnostic codes produced by building `main` in `source`.
fn codes(tag: &str, source: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u63-{tag}.mc");
    mcc::mcc_load_from_string(&uri, source);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
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

// ── Two-actual re-calls: the whole netlist used to come out empty, silently ──
//
// CIMP U68. The defect sat upstream of this binder: `A -> V3V3::PWRLINE(3.3V)`
// registers `V3V3` in the BUS table (a declared interface instance is a bus of
// its members), so the P2-5 lane expansion claimed the call once per bus member
// and shredded the actuals into single-member lanes. One lane against a
// two-member port is either a vector width mismatch or an argument with no free
// port -- so both actuals were lost and the parent's nets were empty with
// nothing on screen (`mcc check` reports the file clean: it does not
// instantiate). Only a CONSTRUCTION expands once per lane; a re-call binds
// ports, and a whole interface actual is the binder's own business.

/// Two declared supply ports, two actuals on one re-call line: each supply
/// takes its own port, in declaration order.
#[test]
fn u68__a_two_arg_re_call_binds_both_supplies() {
    let ports = "psnk ZRAIL{ZVDD,GND}, psnk ARAIL{AVDD,GND}";
    let n = nets("two-arg-recall", &board(ports, "s(V3V3, V1V2)"));

    assert!(
        pairs_with(&n, "V3V3", "ZVDD"),
        "the first actual takes the first declared supply port: {}",
        dump(&n)
    );
    assert!(
        pairs_with(&n, "V1V2", "AVDD"),
        "the second actual takes the second declared supply port; before the fix both were shredded and the table was empty: {}",
        dump(&n)
    );
    assert!(
        !pairs_with(&n, "V1V2", "ZVDD") && !pairs_with(&n, "V3V3", "AVDD"),
        "the two supplies must not swap ports: {}",
        dump(&n)
    );
}

/// One declared supply port and one signal bus, two actuals: the first actual
/// takes the supply port and the second lands on nothing -- never on the signal
/// bus. Shredding the actuals into lanes would also have handed the bus a lane.
#[test]
fn u68__an_extra_actual_never_lands_on_the_signal_bus() {
    let ports = "psnk ZRAIL{VDDIO,GND}, io AAA{P,N}";
    let n = nets("two-arg-extra", &board(ports, "s(V3V3, V1V2)"));

    assert!(
        pairs_with(&n, "V3V3", "s.ZRAIL.VDDIO"),
        "the first actual takes the declared supply port: {}",
        dump(&n)
    );
    assert!(
        !took_supply(&n, "V1V2", &["s.AAA.P", "s.AAA.N"]),
        "the second actual has no port to reach and must not take the signal bus: {}",
        dump(&n)
    );
}

// ── The excess actual must also reach the USER, not only the internal surface ──
//
// `INST_ARG_UNBOUND_DETAILED` is the code for an actual with no formal port
// left. It was produced, but only through `record_warning`, whose own doc says
// the channel is *not* surfaced in the build report -- the sole on-screen trace
// was the internal 940 info in the diagnostic log. So in the canonical form
// above, `V1V2` vanished with the file reported clean. The lock is therefore on
// the user-visible surface (`mcc_diagnose_all`), not on the internal one.

const E4175: u32 = 4175;

/// A member named on a declaration that does not declare it.
const E3181: u32 = 3181;

/// An actual with no formal port left is reported at user level.
#[test]
fn u68__an_extra_actual_reaches_the_build_report() {
    let ports = "psnk ZRAIL{VDDIO,GND}, io AAA{P,N}";
    let c = codes("two-arg-extra-diag", &board(ports, "s(V3V3, V1V2)"));

    assert!(
        c.contains(&E4175),
        "the second actual has no port to reach and must be reported, not dropped: codes={c:?}"
    );
}

/// The control: a complete binding reports nothing. Reaching the user-visible
/// channel must not degrade into reporting every re-call.
#[test]
fn u68__a_fully_bound_re_call_reports_no_unbound_arg() {
    let ports = "psnk ZRAIL{ZVDD,GND}, psnk ARAIL{AVDD,GND}";
    let c = codes("two-arg-bound-diag", &board(ports, "s(V3V3, V1V2)"));

    assert!(
        !c.contains(&E4175),
        "both actuals have a formal port; the code must not fire on a complete binding: codes={c:?}"
    );
}

// ── The two neighbouring shapes: an empty netlist that IS reported ──
//
// b3410 registered a lane literal (`s(V3V3.VPOS, V1V2.VPOS)`) and a
// single-member scalar (`s(V1V2.P)`) as an unfixed residual, both `count: 0`
// before and after that batch. The count is right and the reading of it was
// wrong: an actual holding one member against a port holding two is a vector
// width mismatch (E4180), and that code's own message says to pass the whole
// interface -- `s(V3V3)`, the very form that binds. So the empty netlist is the
// DESIGNED verdict on a reported error, not a hole with nothing on screen.
// These cases pin the report, since there is nothing to fix on the binding side.

const E4180: u32 = 4180;

/// A lane literal actual against a membered port is a width mismatch, reported.
#[test]
fn u68__a_lane_literal_actual_is_a_reported_width_mismatch() {
    let ports = "psnk ZRAIL{VDDIO,GND}, io AAA{P,N}";
    let c = codes(
        "lane-literal-diag",
        &board(ports, "s(V3V3.VPOS, V1V2.VPOS)"),
    );

    assert!(
        c.contains(&E4180),
        "a single-member actual against a two-member port must be reported, not dropped: codes={c:?}"
    );
}

/// The same for a single-member scalar actual, which additionally names a member
/// the interface does not declare.
#[test]
fn u68__a_single_member_scalar_actual_is_a_reported_width_mismatch() {
    let ports = "psnk ZRAIL{VDDIO,GND}, io AAA{P,N}";
    let c = codes("single-member-diag", &board(ports, "s(V1V2.P)"));

    assert!(
        c.contains(&E4180),
        "a single-member actual against a two-member port must be reported, not dropped: codes={c:?}"
    );
    assert!(
        c.contains(&E3181),
        "`.P` is not a member PWRLINE declares; that must be reported too: codes={c:?}"
    );
}
