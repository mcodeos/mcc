// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Polarity reverse ERC (`POLARITY_REVERSED` = 6062, rules-catalog family B9,
//! landed U319).
//!
//! Scope is the class's own declaration: pin rows naming both polarity sides
//! (`+`/`-` or `ANODE`/`CATHODE`). The judgment reads the finished net map —
//! a landed net's potential is provable only as a declared rail hot's signed
//! nominal or the paired return's 0; unknown potentials stay silent, never
//! guessed (U319 ruling: Hot/Ret axis with signed comparison, so a negative
//! rail judges naturally).
//!
//! Acceptance discipline (§1 taxonomy): every verdict branch carries at least
//! two members — the violation branch holds an electrolytic spelling (flag
//! written alongside), a diode spelling (names alone) and a negative-rail
//! member; the silence branch holds the correct placements on both rail
//! polarities, the one-net no-polarity shape, both-unknown and
//! one-side-unknown potentials, and the one-sided / unnamed parts.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The electrolytic spelling: `+`/`-` aliases and the polarized flag.
const ELEC: &str = "component ELEC {\n    pins = [\n        1 = \\+ | ANODE\n        2 = \\- | CATHODE\n    ]\n    spec = [\n        polarized = true\n    ]\n}\n";

/// The diode spelling: ANODE/CATHODE names, no flag.
const DIOD: &str = "component DIOD {\n    pins = [\n        1 = ANODE\n        2 = CATHODE\n    ]\n}\n";

/// A part with only one side named: no judgment here.
const HALF: &str = "component HALF {\n    pins = [\n        1 = \\+\n        2 = B\n    ]\n}\n";

/// A two-pin part with no polarity words at all.
const PLAIN: &str = "component PLAIN {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n";

/// A positive-rail module body: VCC hot, GND return.
fn pos_rail(body: &str) -> String {
    format!(
        "{ELEC}\n{DIOD}\n{HALF}\n{PLAIN}\nmodule main {{\n    \
         conduit GND @role(main) @star\n    \
         domain PWR {{ rail [VCC, GND]::DC(5V) }}\n{body}}}\n"
    )
}

/// A negative-rail module body: V5N hot at −5 V, NGND return.
fn neg_rail(body: &str) -> String {
    format!(
        "{ELEC}\n{PLAIN}\nmodule main {{\n    \
         conduit NGND @role(main) @star\n    \
         domain N {{ rail [V5N, NGND]::DC(-5V) }}\n{body}}}\n"
    )
}

/// 6062 rows, in emission order, as `message` strings.
fn rows(src: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/polarity-reverse.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    mcc::check::nets::run_net_checks(&table)
        .iter()
        .filter(|r| r.code == mcc::errcodes::POLARITY_REVERSED)
        .map(|r| r.message.clone())
        .collect()
}

// ── 1. The violations

#[test]
fn reversed_electrolytic_and_reversed_diode_fire_per_instance() {
    // Two different spellings, two instances, two fires — a per-instance rule
    // must not read as per-net or per-part.
    let src = pos_rail(
        "    ELEC e1\n    DIOD d1\n    GND -> e1 -> VCC\n    GND -> d1 -> VCC\n",
    );
    let rows = rows(&src);
    assert_eq!(rows.len(), 2, "two reversed instances → 6062 ×2: {rows:?}");
    assert!(
        rows.iter().all(|m| m.contains("e1") || m.contains("d1")),
        "each row names its instance: {rows:?}"
    );
}

#[test]
fn negative_rail_reversal_fires_by_signed_comparison() {
    // + on the −5 V hot, − on the 0 V return: the signed comparison fires
    // where a naive Hot/Ret symmetry would call the wiring normal.
    let src = neg_rail("    ELEC e1\n    V5N -> e1 -> NGND\n");
    let rows = rows(&src);
    assert_eq!(rows.len(), 1, "negative-rail reversal fires once: {rows:?}");
}

#[test]
fn message_names_terminals_nets_and_potentials() {
    let src = pos_rail("    ELEC e1\n    GND -> e1 -> VCC\n");
    let rows = rows(&src);
    let m = &rows[0];
    assert!(
        m.contains("e1")
            && m.contains("GND")
            && m.contains("VCC")
            && m.contains("5V")
            && m.contains("pin 1")
            && m.contains("pin 2"),
        "message names instance, terminals, nets and potentials: {m}"
    );
}

// ── 2. The silences the ruling names

#[test]
fn correct_placements_stay_silent_on_both_rail_polarities() {
    let pos = pos_rail(
        "    ELEC e1\n    DIOD d1\n    VCC -> e1 -> GND\n    VCC -> d1 -> GND\n",
    );
    assert_eq!(rows(&pos), Vec::<String>::new());
    // + on the 0 V return, − on the −5 V hot: right for a negative rail.
    let neg = neg_rail("    ELEC e1\n    NGND -> e1 -> V5N\n");
    assert_eq!(rows(&neg), Vec::<String>::new());
}

#[test]
fn both_terminals_on_one_net_is_no_polarity() {
    let src = pos_rail("    ELEC e1\n    e1{1, 2} - [VCC, VCC]\n");
    assert_eq!(rows(&src), Vec::<String>::new());
}

#[test]
fn unknown_potentials_stay_silent() {
    // The rule never guesses what the board did not declare. Two members:
    // both terminals on plain signal nets, and one terminal known (VCC) with
    // the other unknown (a signal net) — either side unknown silences.
    let both = pos_rail(
        "    ELEC e1\n    conduit S1\n    conduit S2\n    S1 -> e1 -> S2\n",
    );
    assert_eq!(rows(&both), Vec::<String>::new());
    let one = pos_rail(
        "    ELEC e1\n    conduit S1\n    e1{1, 2} - [VCC, S1]\n",
    );
    assert_eq!(rows(&one), Vec::<String>::new());
}

#[test]
fn one_sided_and_unnamed_parts_are_out_of_scope() {
    // HALF names only its positive side; PLAIN names no polarity side. Both
    // sit reversed on the rails and stay silent: no witness, no judgment.
    let src = pos_rail(
        "    HALF h1\n    PLAIN p1\n    GND -> h1 -> VCC\n    GND -> p1 -> VCC\n",
    );
    assert_eq!(rows(&src), Vec::<String>::new());
}
