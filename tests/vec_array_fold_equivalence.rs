// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! unified-core §4.4 [V]: an **array member** in a chain, reduced by the fold
//! to one N-point × N-point operand, must land the same net partition as the
//! explicit per-member expansion.
//!
//! This is the `[V]` item "the array `@@ARRAY` iterated layer (still routed
//! through recursion / iterated expansion, not entering this fold chain)" —
//! the layer stays outside the fold by design (§7.2 H4), so what has to be
//! *proved* is that the fold's own reading of the array member is equivalent
//! to the per-member one (vec-dianlu §7.6: `A[1:2].M ≡ A1.M; A2.M`).
//!
//! The faces come from `decode_array_face` (funccall.rs) via
//! `vexpr_reduce` → `get_left_points` → `resolve_funccall_face`; the fold then
//! row-zips them like any other N-point operand. Equivalence is asserted on the
//! **partition** (which points share a net), never on net names or counts —
//! the same membership-based discipline `vec_series_rowzip.rs` uses.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Plain two-pin CAP (`1 = 1`, `2 = 2`), constructed via the `::CAP()` array
/// form so `cap[1:2]::CAP()` materializes cap1/cap2.
const CAP_PLAIN: &str = "component CAP {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Build `main`, returning the net partition: the set of point-sets that share
/// a net, canonicalized (inner sorted, outer sorted) and with net NAMES dropped.
/// An equivalence oracle must not depend on which name a net happens to take.
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

/// The net carrying `needle` (substring match over its point paths), if any.
fn net_holding<'a>(parts: &'a [Vec<String>], needle: &str) -> Option<&'a Vec<String>> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains(needle)))
}

/// Assert each named member pin exists on its own net, and no two of them share
/// one. Equality with the per-member form alone would also pass if the array
/// form degenerated to "everything shorted" *and* the hand form did too; this
/// pins the actual positional structure, so a collapse cannot hide behind it.
fn assert_members_on_distinct_nets(parts: &[Vec<String>], member_pins: &[&str]) {
    let owners: Vec<&Vec<String>> = member_pins
        .iter()
        .map(|pin| {
            net_holding(parts, pin)
                .unwrap_or_else(|| panic!("no net carries {pin}; partition={parts:?}"))
        })
        .collect();
    for i in 0..owners.len() {
        for j in (i + 1)..owners.len() {
            assert_ne!(
                owners[i], owners[j],
                "{} and {} share a net — the array member collapsed instead of \
                 row-zipping; partition={parts:?}",
                member_pins[i], member_pins[j]
            );
        }
    }
    assert_eq!(
        parts.len(),
        member_pins.len(),
        "expected one net per member; partition={parts:?}"
    );
}

/// `[VDD, GND] -> cap[1:2]` (array member) vs `VDD -> cap1; GND -> cap2`
/// (explicit per-member). Same source face (a 2-point column), same target
/// face; the fold's row-zip must reproduce the per-member pairing.
#[test]
fn array_fold__array_member_equals_per_member_on_the_left() {
    let array_form = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    [VDD, GND] -> cap[1:2]\n}}"
    );
    let per_member = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    VDD -> cap1\n    GND -> cap2\n}}"
    );
    let a = partition_of(&array_form, "/mcc/afe-left-array.mc");
    let b = partition_of(&per_member, "/mcc/afe-left-member.mc");
    assert!(
        !a.is_empty(),
        "array form produced no nets — the array member did not fold"
    );
    assert_members_on_distinct_nets(&a, &["cap1.1", "cap2.1"]);
    assert_eq!(
        a, b,
        "array-member fold must equal the per-member expansion;\n array={a:?}\n member={b:?}"
    );
}

/// `cap[1:2] -> [VDD, GND]` (array member, mirrored direction) vs
/// `cap1 -> VDD; cap2 -> GND`.
#[test]
fn array_fold__array_member_equals_per_member_on_the_right() {
    let array_form = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    cap[1:2] -> [VDD, GND]\n}}"
    );
    let per_member = format!(
        "{CAP_PLAIN}module main {{\n    io VDD\n    io GND\n    cap[1:2]::CAP()\n    cap1 -> VDD\n    cap2 -> GND\n}}"
    );
    let a = partition_of(&array_form, "/mcc/afe-right-array.mc");
    let b = partition_of(&per_member, "/mcc/afe-right-member.mc");
    assert!(
        !a.is_empty(),
        "array form produced no nets — the array member did not fold"
    );
    assert_members_on_distinct_nets(&a, &["cap1.2", "cap2.2"]);
    assert_eq!(
        a, b,
        "array-member fold must equal the per-member expansion;\n array={a:?}\n member={b:?}"
    );
}

/// Three-wide: the array member must not collapse or broadcast — the N×N
/// operand row-zips positionally, exactly like three scalar legs.
#[test]
fn array_fold__three_wide_array_member_stays_positional() {
    let array_form = format!(
        "{CAP_PLAIN}module main {{\n    io A\n    io B\n    io C\n    cap[1:3]::CAP()\n    [A, B, C] -> cap[1:3]\n}}"
    );
    let per_member = format!(
        "{CAP_PLAIN}module main {{\n    io A\n    io B\n    io C\n    cap[1:3]::CAP()\n    A -> cap1\n    B -> cap2\n    C -> cap3\n}}"
    );
    let a = partition_of(&array_form, "/mcc/afe-three-array.mc");
    let b = partition_of(&per_member, "/mcc/afe-three-member.mc");
    assert_members_on_distinct_nets(&a, &["cap1.1", "cap2.1", "cap3.1"]);
    assert_eq!(
        a, b,
        "3-wide array-member fold must stay positional;\n array={a:?}\n member={b:?}"
    );
}
