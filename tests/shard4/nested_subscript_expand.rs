// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Nested-subscript expansion locks (vec-arch.md §4.1.1 R2, CIMP U237 case B /
//! U246): the inner member group of `S[1:4][1,2]` reads as a per-instance
//! member face — node `4*2` for a pair, a point `4*1` for a single member —
//! and `S[1:4]{1,2}` expands per instance (R3 element-wise sub), never
//! collapsing to the first instance's bus. Pairing is the §5.2 flat
//! instance-major zip, verbatim-identical to the dot-subscript control.

use crate::common;

use mcc::{McIds, McURI};

const HDR_COMP: &str = "component HDR4 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

fn module(body: &str) -> String {
    format!("{HDR_COMP}module main {{\n{body}}}\n")
}

/// Build `main`, return the diagnostic codes (sorted).
fn codes(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v
}

/// Build `main` with nets, returning the frozen net-table store.
fn build_net_store(src: &str, uri: &str) -> Vec<(String, Vec<String>)> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(n, pts)| (n.clone(), pts.iter().map(|p| p.path.clone()).collect()))
                .collect()
        })
        .unwrap_or_default()
}

/// The point set of the whole net table, order-independent.
fn point_set(nets: &[(String, Vec<String>)]) -> Vec<Vec<String>> {
    let mut v: Vec<Vec<String>> = nets.iter().map(|(_, ps)| ps.clone()).collect();
    v.sort();
    v
}

/// The R2 pair form: each instance's `[1,2]` is a `1*2` row, so the operand
/// is the node `4*2` and the `->` pairs per instance, instance-major —
/// `S_i.k ↔ T_i.k`, eight nets, zero diagnostics.
#[test]
fn bracket_nested_pair_expands_per_instance() {
    let src = module("    HDR4 S[1:4]\n    HDR4 T[1:4]\n    S[1:4][1,2] -> T[1:4][1,2]\n");
    let nets = build_net_store(&src, "/mcc/nested-probe.mc");
    let expected: Vec<Vec<String>> = (1..=4)
        .flat_map(|i| {
            [
                vec![format!("S{i}.1"), format!("T{i}.1")],
                vec![format!("S{i}.2"), format!("T{i}.2")],
            ]
        })
        .collect();
    assert_eq!(point_set(&nets), {
        let mut e = expected;
        e.sort();
        e
    });
    assert_eq!(codes(&src, "/mcc/nested-probe.mc"), Vec::<u32>::new());
}

/// A single inner member is a point `4*1` (the ruling's `4*1` shape): it
/// converges to the dot-subscript path, and the net table is verbatim-identical
/// to the `.1` control.
#[test]
fn single_inner_member_matches_dot_control() {
    let bracket = module("    HDR4 S[1:4]\n    HDR4 T[1:4]\n    S[1:4][1] -> T[1:4][1]\n");
    let dot = module("    HDR4 S[1:4]\n    HDR4 T[1:4]\n    S[1:4].1 -> T[1:4].1\n");
    let expected: Vec<Vec<String>> = (1..=4)
        .map(|i| vec![format!("S{i}.1"), format!("T{i}.1")])
        .collect();
    assert_eq!(point_set(&build_net_store(&bracket, "/mcc/nested-probe.mc")), {
        let mut e = expected.clone();
        e.sort();
        e
    });
    assert_eq!(point_set(&build_net_store(&dot, "/mcc/nested-probe.mc")), {
        let mut e = expected;
        e.sort();
        e
    });
    assert_eq!(codes(&bracket, "/mcc/nested-probe.mc"), Vec::<u32>::new());
    assert_eq!(codes(&dot, "/mcc/nested-probe.mc"), Vec::<u32>::new());
}

/// R5 row-vector law: an inner member group wider than two has no defined
/// pairing — rejected at phrase construction with SHAPE_MEMBER_GROUP_WIDTH.
#[test]
fn inner_group_wider_than_two_is_rejected() {
    let src = module("    HDR4 S[1:4]\n    HDR4 T[1:4]\n    S[1:4][1,2,3] -> T[1:4][1,2,3]\n");
    assert!(codes(&src, "/mcc/nested-probe.mc").contains(&2909));
}

/// R3 element-wise sub: the curly group on an array expands per instance —
/// the same eight instance-major nets as the bracket-nested pair, never the
/// former first-instance collapse (0 errors, only S1/T1 wired).
#[test]
fn curly_group_on_array_expands_per_instance() {
    let src = module("    HDR4 S[1:4]\n    HDR4 T[1:4]\n    S[1:4]{1,2} -> T[1:4]{1,2}\n");
    let nets = build_net_store(&src, "/mcc/nested-probe.mc");
    let expected: Vec<Vec<String>> = (1..=4)
        .flat_map(|i| {
            [
                vec![format!("S{i}.1"), format!("T{i}.1")],
                vec![format!("S{i}.2"), format!("T{i}.2")],
            ]
        })
        .collect();
    assert_eq!(point_set(&nets), {
        let mut e = expected;
        e.sort();
        e
    });
    assert_eq!(codes(&src, "/mcc/nested-probe.mc"), Vec::<u32>::new());
}

/// Literal double-bracket nesting (`[[1,2]]`) stays on the mcast grammar's
/// error recovery (E2082 clause skipped): the U237 ruling admits the flat
/// inner group only, and the bracketed variant is explicitly illegal.
#[test]
fn double_bracket_stays_grammar_illegal() {
    let src = module("    HDR4 S[1:4]\n    HDR4 T[1:4]\n    S[1:4][[1,2]] -> T[1:4][[1,2]]\n");
    assert!(codes(&src, "/mcc/nested-probe.mc").contains(&2082));
}
