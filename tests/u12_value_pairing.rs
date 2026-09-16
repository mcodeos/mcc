// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: which formal port an actual argument reaches (CIMP U12).
//
// A sub-module's interface ports are handed supplies at the call site --
// `SINK s(V3V3, V1V2)`. The binders used to pair an argument with a port by
// scanning both names for a digit-V-digit fragment, so `V3V3` "matched" a port
// whose member was called `VDD_3V3`: two unrelated names that happen to spell
// 3.3 volts the same way. The pairing now reads the voltage each side DECLARED
// -- `V3V3::DC(3.3V)` against `[VDD_3V3,GND]::DC(3.3V)` -- and falls back to
// position when either side declares no value. A spelling never decides
// (AGENTS.md, "no guessing from names").

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// A supply interface that declares nothing about the names of its rails: the
/// written members are the author's own labels, which is exactly what makes a
/// name-shaped pairing wrong here.
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

/// A board with two named supplies in the top module, handed to a sub-module
/// whose ports are `ports`. `decls` declares the supplies, `args` is the
/// call-site argument list.
fn board(ports: &str, decls: &str, args: &str) -> String {
    format!(
        r#"{IFACE}
module SINK({ports})
{{
}}

module main
{{
    io A
    io B
{decls}
    SINK s({args})
}}
"#
    )
}

/// The two supplies, `V3V3` at 3.3 V and `V1V2` at 1.2 V, each declared by a
/// bare interface on a connection line of the top module.
const DECLS: &str = r#"    A -> V3V3::PWRLINE(3.3V)
    B -> V1V2::PWRLINE(1.2V)"#;

/// Build `source` and return the top module's nets, each as the set of point
/// paths it holds.
fn nets(tag: &str, source: &str) -> Vec<BTreeSet<String>> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u12-{tag}.mc");
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
/// last segment it is? Comparing whole segments keeps `s.V3V3` from matching on
/// the `V3V3` inside `s.V3V3_A`.
fn is_point(path: &str, name: &str) -> bool {
    path == name || path.ends_with(&format!(".{name}"))
}

/// Is `path` one of the points the declared supply `supply` contributes? A
/// declaration `V3V3::PWRLINE(3.3V)` carries its interface's members over, so it
/// is a whole family of points (`V3V3.VPOS`, `V3V3.GND`), not one. The trailing
/// dot keeps the prefix from reaching a differently named supply (`V3V3_A`).
fn of_supply(path: &str, supply: &str) -> bool {
    path == supply || path.starts_with(&format!("{supply}."))
}

/// Do the supply `supply` and the point `port` share a net?
fn pairs_with(nets: &[BTreeSet<String>], supply: &str, port: &str) -> bool {
    nets.iter()
        .any(|n| n.iter().any(|p| of_supply(p, supply)) && n.iter().any(|p| is_point(p, port)))
}

/// The net holding `name`, for the failure messages: a wrong pairing is only
/// readable when the nets are on screen.
fn dump(nets: &[BTreeSet<String>]) -> String {
    nets.iter()
        .map(|n| n.iter().cloned().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The written argument order does not decide the binding: `V1V2` reaches the
/// port declared at 1.2 V whether it is written first or second.
#[test]
fn u12__value_pairing_ignores_argument_order() {
    let ports = "[VDD_3V3, GND]::PWRLINE(3.3V), [VCC_1V2, GND]::PWRLINE(1.2V)";

    let forward = nets("order-forward", &board(ports, DECLS, "V3V3, V1V2"));
    assert!(
        pairs_with(&forward, "V3V3", "s.VDD_3V3"),
        "V3V3 belongs on the 3.3 V port: {}",
        dump(&forward)
    );
    assert!(
        pairs_with(&forward, "V1V2", "s.VCC_1V2"),
        "V1V2 belongs on the 1.2 V port: {}",
        dump(&forward)
    );

    let swapped = nets("order-swapped", &board(ports, DECLS, "V1V2, V3V3"));
    assert!(
        pairs_with(&swapped, "V3V3", "s.VDD_3V3"),
        "V3V3 belongs on the 3.3 V port whichever slot it is written in: {}",
        dump(&swapped)
    );
    assert!(
        pairs_with(&swapped, "V1V2", "s.VCC_1V2"),
        "V1V2 belongs on the 1.2 V port whichever slot it is written in: {}",
        dump(&swapped)
    );
}

/// The discriminating case: a port whose member is SPELLED `V3V3_A` but
/// DECLARED at 1.2 V must not take the 3.3 V argument. Under a name-shaped
/// pairing the spelling won and this failed; the value is what decides now.
#[test]
fn u12__a_spelled_name_no_longer_decides() {
    // Port 0 spells 3V3 and declares 1.2 V; port 1 spells 1V2 and declares
    // 3.3 V. The two spellings point at the wrong ports on purpose.
    let ports = "[V3V3_A, GND]::PWRLINE(1.2V), [VCC_1V2, GND]::PWRLINE(3.3V)";
    let n = nets("spelled-name", &board(ports, DECLS, "V3V3, V1V2"));

    assert!(
        pairs_with(&n, "V3V3", "s.VCC_1V2"),
        "the 3.3 V argument belongs on the port DECLARED at 3.3 V, not on the one spelled 3V3: {}",
        dump(&n)
    );
    assert!(
        !pairs_with(&n, "V3V3", "s.V3V3_A"),
        "a member named V3V3_A declared at 1.2 V must not take the 3.3 V argument: {}",
        dump(&n)
    );
    assert!(
        pairs_with(&n, "V1V2", "s.V3V3_A"),
        "the 1.2 V argument belongs on the port DECLARED at 1.2 V: {}",
        dump(&n)
    );
}

/// A range is not a value, so a port declared `2.5V~5.5V` declares nothing to
/// pair on and never wins the value pairing -- even when its member names carry
/// the argument's spelling.
#[test]
fn u12__a_range_declares_no_value_to_pair_on() {
    let ports = "[VCC3V3_RANGE, GND]::PWRLINE(2.5V~5.5V), [VCC_1V2, GND]::PWRLINE(3.3V)";
    let n = nets("range", &board(ports, DECLS, "V3V3, V1V2"));

    assert!(
        pairs_with(&n, "V3V3", "s.VCC_1V2"),
        "the 3.3 V argument must reach the port declared at exactly 3.3 V: {}",
        dump(&n)
    );
    assert!(
        !pairs_with(&n, "V3V3", "s.VCC3V3_RANGE"),
        "a port declared over a range declares no value and must not pair by spelling: {}",
        dump(&n)
    );
}

/// The declaration is read from the caller's own symbol table, so it does not
/// have to be written above the call: a module's declarations are all in hand
/// by the time its instances are built.
#[test]
fn u12__a_declaration_below_the_call_is_still_read() {
    let ports = "[VDD_3V3, GND]::PWRLINE(3.3V), [VCC_1V2, GND]::PWRLINE(1.2V)";
    let src = format!(
        r#"{IFACE}
module SINK({ports})
{{
}}

module main
{{
    io A
    io B
    SINK s(V1V2, V3V3)
{DECLS}
}}
"#
    );
    let n = nets("decl-order", &src);
    assert!(
        pairs_with(&n, "V3V3", "s.VDD_3V3") && pairs_with(&n, "V1V2", "s.VCC_1V2"),
        "the declaration is read wherever it is written in the module: {}",
        dump(&n)
    );
}
