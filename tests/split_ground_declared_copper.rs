// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! split_ground exemption by declared return copper (split-ground-copper-design
//! v0.2 §3 criterion 2): a bare ground endpoint whose name equals this module's
//! OWN declared return copper (a `conduit` name or a declared DC rail's `ret`
//! member) is that copper — the whole net is exempt from the per-statement
//! electric partition. A legacy module with no power-intent declaration keeps
//! the per-statement local-ground split (`GND@<line>` groups, model A ‑ the
//! schematic convention the split exists to serve).
//!
//! Same two-CAP bridge fixture as `auto_naming_lock` (which locks the legacy
//! `GND@7` fragmentation): toggling in a `conduit GND @role(main)` declaration
//! must turn the two per-line `GND@7` groups into one unified `GND` copper net.

#![allow(non_snake_case)]

mod common;

use mcc::McIds;

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";

/// Net table of `main` after pass-2 flatten: `name <= [path1, path2, ...]`,
/// one sorted line per net.
fn net_table(src: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/split-ground-declared-copper.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");

    let mut lines = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        lines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    lines.sort();
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
fn legacy_no_declaration_keeps_split() {
    let nets = net_table(&format!("{CAP_COMP}{LEGACY}"));
    // The legacy split is locked by auto_naming_lock (GND@7): bare `GND` with
    // no declared copper still fragments per statement.
    assert!(
        nets.iter().any(|l| l.contains("GND@")),
        "legacy bare-GND must still fragment per statement, got:\n{}",
        nets.join("\n")
    );
}

#[test]
fn declared_copper_unifies_bare_ground() {
    let nets = net_table(&format!("{CAP_COMP}{DECLARED}"));
    // No per-line fragments may survive: `conduit GND` makes the bare-GND net
    // that declared copper, so it is one net carrying both cap returns.
    assert!(
        !nets.iter().any(|l| l.contains("GND@")),
        "declared-copper net must not fragment, got:\n{}",
        nets.join("\n")
    );
    let gnd = nets
        .iter()
        .find(|l| l.starts_with("GND <="))
        .unwrap_or_else(|| panic!("expected unified GND net, got:\n{}", nets.join("\n")));
    assert!(
        gnd.contains("_C1.2") && gnd.contains("_C2.2") && gnd.contains("main.GND"),
        "GND net must carry both cap returns + the port, got: {gnd}"
    );
}
