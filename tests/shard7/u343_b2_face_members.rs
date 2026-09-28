// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// U343 B1 arm 2: the curly-MN face member reader (`curly_mn_side_members`)
// used to read each member through `to_id_or_ida_or_num`, whose fallback
// descends into the first sub-chain only. An `mc_opd` member carrying a dot
// sibling (`IN1.1`, the `mc_ids MCPT_DOT mc_int` shape) therefore read as the
// bare head name `IN1` — the `.1` tail vanished from the face and the
// selection landed on the wrong pin point behind the author's back. The
// structural reader goes through `McOpd::new`, so the member keeps its full
// name; the face resolver then reports the unresolvable member honestly
// (E1162) instead of silently connecting the truncated head.
//
// The curly-body call site (`base{...}` use position) reads through the same
// helper now, so a bracket-row body unfolds to all its members there too.
// (Member shapes with no operand reading — expression rows, fcall members —
// die at the GLR stage today (E2081/E2082, both faces agree), so the reader's
// report-and-skip branch is a guard for shapes the grammar does not currently
// deliver; there is no fixture that can reach it.)

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Three-pin part: the face positions under test name these pins.
const ORING: &str = "component ORING {\n    pins = [\n        IN1 = 1\n        GND = 2\n        OUT = 3\n    ]\n}\n";

/// Codes that are not this family's verdict (the benign set the vector-oracle
/// family tolerates — unwired-pin remarks from the points a face selection
/// leaves dangling).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054 | 5070 | 5071 | 5072 | 3136)
}

/// Build `main` and return (non-benign codes sorted, net partition).
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!("{ORING}module main {{\n    ORING or1\n{body}\n}}\n");
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();

    let mut partition: Vec<Vec<String>> = net_store
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
    partition.sort();
    (codes, partition)
}

#[test]
fn u343_b2__dotted_side_member_keeps_its_tail() {
    // `IN1.1` is a full operand (`mc_ids MCPT_DOT mc_int`). The truncated read
    // collapsed the member to the bare head `IN1`, so the left face came out
    // one wide against the two-wide leg and the statement died on a
    // misleading E4007 shape mismatch — the author's `.1` was never in the
    // picture. The carried read keeps `IN1.1` and the face resolver reports
    // the member honestly (E1162, member not found).
    let (codes, nets) = build(
        "    or1{IN1.1 | OUT.2} - [GND, GND]\n",
        "/mcc/u343_b2_dot_tail.mc",
    );
    assert!(
        codes.contains(&mcc::errcodes::PHRASE_COMPONENT_MEMBER_NOT_FOUND),
        "the full dotted member must be reported, not silently truncated; got {codes:?}"
    );
    let flat: Vec<String> = nets.into_iter().flatten().collect();
    assert!(
        !flat.iter().any(|p| p.ends_with("or1.IN1") || p.ends_with("or1.OUT")),
        "the truncated head pins must not be connected behind the author's back; got {flat:?}"
    );
}

#[test]
fn u343_b2__plain_row_face_is_unchanged() {
    // The canonical row spelling keeps reading all four members — the
    // structural reader must not disturb the healthy path the pwrint corpus
    // depends on. The two-wide `->` chain makes every member observable: a
    // member lost in the read would vanish from the net partition.
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{ORING}module main {{\n    ORING or1\n    func M() {{\n        \
         [GND, GND] -> or1{{[IN1, GND] | [OUT, GND]}} -> [GND, GND]\n    }}\n}}\n"
    );
    let u = McURI::from("/mcc/u343_b2_plain_row.mc");
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();
    assert!(codes.is_empty(), "quiet chain face; got {codes:?}");

    let flat: Vec<String> = net_store
        .get("main")
        .map(|t| t.iter().flat_map(|(_, pts)| pts.iter().map(|p| p.path.clone())).collect())
        .unwrap_or_default();
    for member in ["or1.IN1", "or1.GND", "or1.OUT"] {
        assert!(
            flat.iter().any(|p| p.ends_with(member)),
            "row member {member} must reach the net store; got {flat:?}"
        );
    }
}
