// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U316 site 4: closure face machines.
//!
//! Verdict (2026-09-27): the parse-time tail snapshot (`McClosure.right`,
//! baked as the last body statement's `get_right()`) is the **single read
//! machine on the right face** — `get_right` and `eval_port_elems(right)`
//! both clone it — and on every corpus-reachable shape the left face
//! machines (syntactic `get_left` vs `eval_port_elems` recursion) agree.
//! The divergence families (endpoint lists, multi-member buses, interface
//! edges, mismatched groups) are all gated on closure spellings no live
//! corpus uses yet; re-derive the faces only when closures activate.
//! Known dormant risk: the snapshot is never refreshed after Pass1b's
//! `fill_return_shapes` resolves shapes inside closure tails.

use crate::common;

use mcc::{McIds, McURI};

const SRC: &str = r#"
component RES(res::INT) {
    pins = [ 1 = 1  2 = 2 ]
    func Pullup([n1, n2]) {
        n1 - this - n2
        return [n1, n2]
    }
}
module top {
    io GND
    io VCC
    func M() {
        DEV D
        D => |ports| {
            ports -> GND
            R1::RES(0).Pullup([ports.1, VCC])
        }
    }
}
"#;

fn closure_of() -> mcc::McPhrase {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/u316-closure.mc");
    mcc::mcc_load_from_string(&uri, SRC);
    let inst = mcc::mcc_build(&McIds::from("top"), &uri).expect("build");
    let f = inst.def.funcs.find("M").expect("func M");
    for stmt in f.stmts.iter() {
        if matches!(stmt, mcc::McPhrase::Closure(_)) {
            return stmt.clone();
        }
    }
    panic!("no closure phrase in func M");
}

/// The right face is the parse-time tail snapshot: the tail funcall's
/// declared right buses, verbatim — one machine, cloned everywhere.
#[test]
fn closure_right_face_is_the_tail_snapshot() {
    let mcc::McPhrase::Closure(c) = closure_of() else {
        panic!("expected a closure phrase");
    };
    assert_eq!(c.params.len(), 1, "one formal `ports`");
    assert_eq!(c.body.len(), 3, "two body stmts + tail funcall");
    let rights: Vec<String> = c.right.iter().map(|b| b.name().to_string()).collect();
    assert_eq!(rights, vec!["RES.out"], "tail funcall's declared right face");
}

/// The netlist face: body lines wire, the formal becomes a net label, the
/// deferred subinstance materializes, and the right snapshot publishes —
/// with no 4xxx wiring diagnostics.
#[test]
fn closure_netlist_face_wires_body_and_publishes_snapshot() {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/u316-closure-nets.mc");
    mcc::mcc_load_from_string(&uri, SRC);
    let (_, table) =
        mcc::mcc_build_flat(&McIds::from("top"), &uri, 1000).expect("flat build");
    let mut netlines: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        netlines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    netlines.sort();
    assert_eq!(
        netlines,
        vec![
            "GND <= [top.GND, top.ports]",
            "VCC <= [top.R1, top.VCC, top.ports.1]",
        ],
        "closure netlist face changed"
    );
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        !codes.iter().any(|c| (4000..5000).contains(c)),
        "no wiring diagnostics expected; got {codes:?}"
    );
}
