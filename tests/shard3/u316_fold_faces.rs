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
//! Filed as U328; the live corpus works around it with dotted member
//! spellings (mcs us513.mc, the `修复·7b` note). These locks pin the current
//! faces so the U328 fix flips them deliberately.

use crate::common;

use mcc::{McIds, McURI};

const PRELUDE: &str = r#"
component RES(r) {
    pins = [ 1 = 1  2 = 2 ]
    func Pullup([n1, n2]) {
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
    io [1,2] = I2C0::I2C(MASTER)
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
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        lines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    lines.sort();
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (lines, codes)
}

/// Disease face A (U328): an interface instance inside a Set actual folds to
/// ONE instance point — both lanes land on a single net — and the diagnostic
/// set is byte-identical to the lane-correct dotted spelling (nothing fires).
#[test]
fn iface_instance_in_set_actual_collapses_to_one_net_silently() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pullup([I2C0, VDD])\n    }}\n}}"
    ));
    assert_eq!(
        lines,
        vec!["I2C0 <= [top.I2C0, top.R1, top.VDD]"],
        "U328 face A: lanes conflate onto one instance-point net; got {lines:?}"
    );
    let (_, dotted_codes) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pullup([I2C0.SCL, VDD])\n    }}\n}}"
    ));
    assert_eq!(
        codes, dotted_codes,
        "face A must stay silent relative to the dotted control (nothing distinguishes them)"
    );
}

/// The corpus workaround contract: dotted member spellings bind lane-correct.
#[test]
fn dotted_member_spelling_binds_lane_correct() {
    let (lines, _) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pullup([I2C0.SCL, VDD])\n    }}\n}}"
    ));
    assert_eq!(
        lines,
        vec!["I2C0.SCL <= [top.I2C0.SCL, top.R1, top.VDD]"],
        "dotted spelling is the lane-correct contract; got {lines:?}"
    );
}

/// Disease face B (U328): a bus literal inside a Set actual loses its lanes
/// (the resistor keeps only the VDD side) under E4180 + E4007.
#[test]
fn bus_literal_set_member_loses_lanes_under_width_diag() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}        R1::RES(10k).Pullup([I2C0{{SCL, SDA}}, VDD])\n    }}\n}}"
    ));
    assert_eq!(
        lines,
        vec!["R1 <= [top.R1, top.VDD]"],
        "U328 face B: the Set member's lanes drop entirely (no I2C0 net at all); got {lines:?}"
    );
    assert!(
        codes.contains(&4180) && codes.contains(&4007),
        "E4180 width + E4007 shape fire on face B; got {codes:?}"
    );
}
