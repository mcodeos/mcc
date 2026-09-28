// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U316 site 7: the substitution fold face versus multi-lane actuals.
//!
//! Monitor data (2026-09-27): the `[U308-TRACE] subst Ports DIVERGE` watch
//! (subst.rs) stayed silent across every mcs board build and shards 2/3/4 —
//! the two Ports read machines agree on everything reachable. The probes
//! below then pinned the real, reachable disease family: a multi-lane value
//! inside a Set actual slips past the bus-in-set guard (which answers bus
//! *names* only) onto the fold face, where lanes conflate (interface
//! instance, silently) or vanish (bus literal, with a width diagnostic).
//! Filed as U328.
//!
//! U329 (ruling 2026-09-28, matching-rules-design.md §6.2): the direct face
//! converged onto the same verdict. The b4096 prelude declared the instance
//! with the list-target form `io [1,2] = I2C0::I2C(MASTER)` — which is
//! E2082 illegal in a module body, so the pinned face A behavior was the
//! artifact of an instance that never existed. Under the legal scalar
//! declaration the port table registers the member set, the P2-5 parameter
//! face is fold-provenance-gated (`pre_closure`), and a directly-written
//! bare interface instance reaches the width gate as its honest 2 members:
//! 3 lanes against a 2-slot formal is E4180 — the same verdict as face B.
//! The prefix face (`=>`) keeps its per-lane fan-out (locked in
//! `u329_prefix_fanout_stays_per_lane`).

use crate::common;

use mcc::{McIds, McURI};

const PRELUDE: &str = r#"
component RES(r) {
    pins = [ 1 = 1  2 = 2 ]
    func Pull([n1, n2]) {
        n1 - this - n2
        return [n1, n2]
    }
}
interface I2C(role)
{
    pins = [
        1 = SCL
        2 = SDA
    ]
    role MASTER { name = "Master" }
}
module top {
    io VDD
    io I2C0::I2C(MASTER)
    func M() {
"#;

fn netlines(src: &str) -> (Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/u316-fold-faces.mc");
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("top"), &uri, 1000).expect("flat build");
    let mut lines: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = Vec::new();
        for pid in &net.points {
            if let Some(e) = table.get_entry(*pid) {
                let p = e.path.clone();
                if !pts.contains(&p) {
                    pts.push(p);
                }
            }
        }
        pts.sort();
        lines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    lines.sort();
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (lines, codes)
}

/// Face A (U329 ruling): a directly-written bare interface instance in a Set
/// actual answers its declared member width at the gate — 2 members stuffed
/// into the single `n1` slot make 3 lanes against a 2-slot formal, E4180,
/// the same verdict as the bus-literal spelling. The former silent
/// conflation (lanes merged onto one net, zero diagnostics) is gone.
#[test]
fn iface_instance_in_set_actual_answers_width_e4180() {
    let (_, codes) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pull([I2C0, VDD])\n    }}\n}}"
    ));
    assert!(
        codes.contains(&4180),
        "U329 face A: the bare interface instance must reach the width gate \
         honestly (E4180, same verdict as face B); got {codes:?}"
    );
    let (_, dotted_codes) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pull([I2C0.SCL, VDD])\n    }}\n}}"
    ));
    assert!(
        !dotted_codes.contains(&4180),
        "the dotted control stays clean; got {dotted_codes:?}"
    );
}

/// The corpus workaround contract: dotted member spellings bind lane-correct.
#[test]
fn dotted_member_spelling_binds_lane_correct() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pull([I2C0.SCL, VDD])\n    }}\n}}"
    ));
    assert!(
        !codes.contains(&4180) && !codes.contains(&4007),
        "dotted spelling is the lane-correct contract (no width/shape diag); got {codes:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("top.I2C0.SCL") && l.contains("top.R1")),
        "the dotted lane binds to the resistor; got {lines:?}"
    );
}

/// Face B: a bus literal inside a Set actual loses its lanes (the resistor
/// keeps only the VDD side) under E4180.
#[test]
fn bus_literal_set_member_loses_lanes_under_width_diag() {
    let (_, codes) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pull([I2C0{{SCL, SDA}}, VDD])\n    }}\n}}"
    ));
    assert!(
        codes.contains(&4180),
        "face B: the Set member's width fires E4180; got {codes:?}"
    );
}

/// U329: the `=>` prefix face keeps its per-lane fan-out — the one face
/// where it is legal (2026-09-11 ruling, fold-provenance-gated P2-5). Two
/// anonymous resistors, one per lane, VDD shared on the return side.
/// (The flat table projects an anonymous construction as its whole-instance
/// point, so the lane nets merge with VDD through the shared `_R*` points in
/// this view; pin-level lane separation is locked by the u329-probe2 CLI
/// netlist, recorded in mcd log/9.28.u329-refit.)
#[test]
fn u329_prefix_fanout_stays_per_lane() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}        I2C0 => RES(10k).Pull([_, VDD])\n    }}\n}}"
    ));
    assert!(
        !codes.contains(&4180),
        "the prefix face fans out legally (no width diag); got {codes:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("top._R1"))
            && lines.iter().any(|l| l.contains("top._R2")),
        "fan-out builds one anonymous resistor per lane; got {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("top.I2C0.SCL"))
            && lines.iter().any(|l| l.contains("top.I2C0.SDA")),
        "both interface lanes reach the table; got {lines:?}"
    );
}
