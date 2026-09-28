// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U316 site 8: the two chain-operand faces are mirrors.
//!
//! Verdict (2026-09-27): the fallback-arm divergence between
//! `get_left_points` and `get_right_points` (member expansion + the P2-10
//! bare-name expansion existed only on the left fallback) was
//! **unreachable** — both dead arms retired in place (U314 item 9).
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

const PRE: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pull([n1, n2]) {\n        n1 - this - n2\n    }\n}\nmodule main {\n    io SPI{SCLK, MOSI}\n    io VDD\n    func M() {\n";

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

fn codes(body: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{PRE}{body}\n    }}\n}}\n");
    let uri = McURI::from("/mcc/u316-lr-mirror.mc");
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// The mirrored spellings must produce byte-identical nets — the fallback
/// asymmetry never reaches the observable face.
///
/// U339 (ruled 2026-09-28): the previous vehicle pinned the phantom-net face —
/// the `_` placeholder made `[_, VDD]` a one-lane actual against the
/// two-member formal `[n1, n2]`, and the half-bound body still expanded,
/// wiring `n1` as the literal phantom `_R1.n1` on an anonymous `_net0`. The
/// shape-law landing turns that into a deficit width error (E4180) and the
/// body no longer runs on rejected bindings (E4176 skip precedent), so both
/// faces produce no nets at all. The lock now pins the still-symmetric
/// outcome: the mirror property survives the error-ization — both chain faces
/// reject the deficit spelling identically, and no phantom net is produced
/// from either side.
#[test]
fn bare_membered_port_collapses_symmetrically_on_both_chain_faces() {
    let l = nets("SPI - RES(1k).Pull([_, VDD])");
    let r = nets("RES(1k).Pull([_, VDD]) - SPI");
    assert_eq!(l, Vec::<String>::new(), "left-side face: deficit body is skipped, no phantom nets");
    assert_eq!(r, Vec::<String>::new(), "right-side face: deficit body is skipped, no phantom nets");
    assert!(
        codes("SPI - RES(1k).Pull([_, VDD])").contains(&4180),
        "the deficit spelling still reports the width error (E4180)"
    );
}
