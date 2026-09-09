// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Split-ground terminal state (split-ground-copper-design v0.2 §6): the flat
//! layer no longer fabricates per-statement `GND@<line>` electric fragments —
//! a bare-`GND` net stays ONE net per distinct base, whether or not the module
//! declares its return copper. The two cases now differ only on the identity
//! axis (NetIslandIndex attribution):
//!
//!   - LEGACY (no power-intent declaration): the unified `GND` net is
//!     `Signal` / `resolvable = false` — never judged, never guessed
//!     (net-island-attribution-design.md §8; ruling ①).
//!   - DECLARED (`conduit GND`): the same unified `GND` net anchors to the
//!     declared copper as `Reference`, `resolvable = true`.
//!
//! Same two-CAP bridge fixture as `auto_naming_lock` (which used to lock the
//! legacy `GND@7` fragmentation).

#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";

/// Build `main` and return the flat table + island index.
fn build_flat(src: &str) -> (mcc::InstTable, mcc::NetIslandIndex) {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/split-ground-declared-copper.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let idx = mcc::NetIslandIndex::build(&table);
    (table, idx)
}

/// The `GND` net line(s) after flatten: `name <= [path1, path2, ...]`.
fn gnd_lines(table: &mcc::InstTable) -> Vec<String> {
    let mut lines = Vec::new();
    for net in table.get_nets() {
        if !net.name.eq_ignore_ascii_case("gnd") {
            continue;
        }
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        lines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    lines
}

const LEGACY: &str = r#"
module main {
    io VDD
    io GND
    CAP(1).Cap([VDD, GND])
    CAP(1).Cap([VDD, GND])
}
"#;

const DECLARED: &str = r#"
module main {
    conduit GND @role(main)
    io VDD
    io GND
    CAP(1).Cap([VDD, GND])
    CAP(1).Cap([VDD, GND])
}
"#;

#[test]
fn legacy_ground_unifies_and_stays_unresolvable() {
    let (table, idx) = build_flat(&format!("{CAP_COMP}{LEGACY}"));
    // No per-statement `GND@` fragments survive — the bare-`GND` statements
    // union into one net carrying the port and both cap returns.
    let all: Vec<String> = table.get_nets().iter().map(|n| n.name.clone()).collect();
    assert!(
        !all.iter().any(|n| n.contains('@')),
        "no @-suffixed ground fragments in the terminal state, got: {all:?}"
    );
    let gnd = gnd_lines(&table);
    assert_eq!(
        gnd,
        vec!["GND <= [main.GND, main._C1.2, main._C2.2]".to_string()],
        "bare-GND net must unify port + both returns, got: {gnd:?}"
    );
    // No declaration → the unified net is Signal / unresolvable: never judged,
    // never guessed (net-island §8).
    let mut hits: Vec<&mcc::NetAttribution> = idx
        .nets()
        .filter(|a| a.name == "GND" && a.module.is_some())
        .collect();
    assert_eq!(hits.len(), 1, "one GND attribution; got {hits:?}");
    assert_eq!(hits.remove(0).role, mcc::NetRole::Signal);
    assert!(!idx.nets().find(|a| a.name == "GND").unwrap().resolvable);
}

#[test]
fn declared_copper_anchors_unified_ground() {
    let (table, idx) = build_flat(&format!("{CAP_COMP}{DECLARED}"));
    // Same unified net shape as legacy — the difference is identity only.
    let gnd = gnd_lines(&table);
    assert_eq!(
        gnd,
        vec!["GND <= [main.GND, main._C1.2, main._C2.2]".to_string()],
        "GND net must carry both cap returns + the port, got: {gnd:?}"
    );
    // `conduit GND` makes the unified net that declared copper: Reference.
    let mut hits: Vec<&mcc::NetAttribution> = idx
        .nets()
        .filter(|a| a.name == "GND" && a.module.is_some())
        .collect();
    assert_eq!(hits.len(), 1, "one GND attribution; got {hits:?}");
    let a = hits.remove(0);
    assert_eq!(a.role, mcc::NetRole::Reference);
    assert_eq!(a.copper.as_deref(), Some("GND"));
    assert!(a.resolvable);
}
