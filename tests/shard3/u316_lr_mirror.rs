// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U316 site 8: the two chain-operand faces are mirrors.
//!
//! Verdict (2026-09-27): the fallback-arm divergence between
//! `get_left_points` and `get_right_points` (member expansion + the P2-10
//! bare-name expansion exist only on the left fallback) is **unreachable**.
//! The semantic layer rewrites every member-carrying spelling into Bus-base
//! endpoints (instref.rs, `dot_or_curly`), which the mirrored Bus arm
//! (points.rs:974) intercepts on both faces. Instrumented runs — a targeted
//! spelling matrix (bare membered ports, bus literals, interface instances,
//! dotted/curly/unresolved refs) plus the full shard3 and shard6 suites —
//! recorded zero hits on both fallback arms and zero P2-10 expansions. The
//! dead arms are handed to U314.
//!
//! The locks pin the observable contract the instrumentation justified: the
//! mirrored chain spellings (bare membered port `SPI` on either side) produce
//! byte-identical nets — the faces treat the operand symmetrically. (The
//! representative point currently floats out of the nets on both sides: a
//! real but symmetric limitation of the bare membered-port collapse.)

use crate::common;

use mcc::{McIds, McURI};

const PRE: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\nmodule main {\n    io SPI{SCLK, MOSI}\n    io VDD\n    func M() {\n";

fn nets(body: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{PRE}{body}\n    }}\n}}\n");
    let uri = McURI::from("/mcc/u316-lr-mirror.mc");
    mcc::mcc_load_from_string(&uri, &src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut lines: Vec<String> = Vec::new();
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

/// The mirrored spellings must produce byte-identical nets — the fallback
/// asymmetry never reaches the observable face.
#[test]
fn bare_membered_port_collapses_symmetrically_on_both_chain_faces() {
    let expected = vec![
        "VDD <= [main.VDD, main._R1.2]",
        "_net0 <= [main._R1.1, main._R1.n1]",
    ];
    let l = nets("SPI - RES(1k).Pullup([_, VDD])");
    let r = nets("RES(1k).Pullup([_, VDD]) - SPI");
    assert_eq!(l, expected, "left-side face changed");
    assert_eq!(r, expected, "right-side face changed");
}
