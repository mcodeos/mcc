// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! unified-core §4.4 [V]: **P2-9 idempotence**.
//!
//! Within one statement, a `FuncCall` member can be reached twice — once by the
//! member loop (`process_member_internal`, stmt.rs:498) and once by the
//! lane-by-lane wiring that a `_` lead or a transposed member switches the
//! chain onto (`vexpr_lane_chain`, stmt.rs:548). `auto_inst_map` is cleared at
//! every statement boundary (phases.rs:748), so it exists precisely to answer
//! "this call was already instantiated in *this* statement, do not build a
//! second component": the guard at stmt.rs:2731 keys on `member_key` (the
//! parser-assigned `FuncCall.id`) and returns early.
//!
//! The `[V]` item is that this dedup is *complete* for the shape it was written
//! for: one written call ⇒ one component. The lock below pins that with the
//! minimal chain that reaches both paths — a lane chain whose member constructs
//! a component inline and calls a method on it (`CAP(100n).Cap([VOUT, GND])`),
//! sitting behind a `_` lead so the lane wiring runs.
//!
//! The assertion is on the **instance names** in the net partition, not on a
//! component count from an internal API: a duplicate shows up as a second
//! `_Cn` whose pins land on the very same nets. Equality of the partition with
//! the single-instance form would also pass if both forms were empty, so the
//! capacitor's own pins are asserted onto their nets as well.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// Plain two-pin capacitor with a `Cap` func, so `CAP(100n).Cap([a, b])` both
/// constructs the part and runs its wiring body.
const CAP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Build `main`, returning the net partition: the set of point-sets that share
/// a net, canonicalized (inner sorted, outer sorted) and with net NAMES
/// dropped. Net names are not part of what is being locked.
fn partition_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut parts: Vec<Vec<String>> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(_, pts)| {
                    let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
                    ps.sort();
                    ps
                })
                .filter(|ps| !ps.is_empty())
                .collect()
        })
        .unwrap_or_default();
    parts.sort();
    parts
}

/// Non-benign diagnostic codes for `src` (see [`benign`]).
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Codes unrelated to this lock: the short probe names warn (5641/5642/5643,
/// 5054), and the inline `CAP(100n)` construct cannot resolve its *catalog*
/// class without a system library (`INST_CLASS_UNRESOLVED` 3157 /
/// `INST_CLASS_NOT_LOADED` 5256). The component itself is still built — which
/// is all this lock needs — and any change to that is caught by the partition
/// assertions below, not by the code list.
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054 | 3157 | 5256)
}

/// The instance-name prefixes carried by the partition's point paths (the
/// segment before the final `.`, e.g. `_C1` in `_C1.1`).
fn instance_names(parts: &[Vec<String>]) -> BTreeSet<String> {
    parts
        .iter()
        .flatten()
        .filter_map(|p| p.rsplit_once('.').map(|(head, _)| head.to_string()))
        .filter(|head| head.starts_with('_'))
        .collect()
}

/// The net carrying `needle` (substring match over its point paths), if any.
fn net_holding<'a>(parts: &'a [Vec<String>], needle: &str) -> Option<&'a Vec<String>> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains(needle)))
}

/// The lane chain that reaches both instantiation paths: `VIN` on the left,
/// and a `_` lead on the right so `needs_lane_by_lane` switches the chain onto
/// the lane wiring (stmt.rs:518-521).
const LANE_CHAIN_SRC: &str = "module main {\n    io VIN\n    io VOUT\n    io GND\n    func M() {\n        VIN - [CAP(100n).Cap([VOUT, GND]), _]\n    }\n}\n";

fn lane_chain(suffix: &str) -> String {
    format!("{CAP}{LANE_CHAIN_SRC}{suffix}")
}

/// One written `CAP(100n).Cap(..)` in a lane chain yields exactly **one**
/// component. Two would put `_C1`'s and `_C2`'s pins on the same nets.
#[test]
fn p29__lane_chain_funccall_member_instantiates_once() {
    let src = lane_chain("");
    let parts = partition_of(&src, "/mcc/vec-p29-once.mc");

    let names = instance_names(&parts);
    assert_eq!(
        names,
        BTreeSet::from(["_C1".to_string()]),
        "one written call must instantiate exactly one component; got {names:?} \
         (partition={parts:?})"
    );

    // Anti-false-green: the surviving instance must actually be wired. An
    // empty or degenerate build would satisfy the count above trivially.
    assert!(
        net_holding(&parts, "_C1.1").is_some(),
        "the capacitor's pin 1 must land on a net; partition={parts:?}"
    );
    assert!(
        net_holding(&parts, "_C1.2").is_some(),
        "the capacitor's pin 2 must land on a net; partition={parts:?}"
    );

    assert_eq!(
        codes_of(&src, "/mcc/vec-p29-once.mc"),
        Vec::<u32>::new(),
        "the lane chain must stay quiet apart from the benign codes"
    );
}

/// Rebuilding the same source yields the same partition — the dedup state does
/// not leak across statements or across builds.
#[test]
fn p29__rebuild_lands_the_same_partition() {
    let src = lane_chain("");
    let first = partition_of(&src, "/mcc/vec-p29-rebuild.mc");
    let second = partition_of(&src, "/mcc/vec-p29-rebuild.mc");
    assert_eq!(
        first, second,
        "two builds of the same source must land the same partition"
    );
}
