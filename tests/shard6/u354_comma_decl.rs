// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U354: the declaration-position comma spelling joins the sub-selection
//! family.
//!
//! `LDO2 ldo{VIN, VOUT}` used to die E2082 at the `{` — the pipe form had the
//! `mc_phrase{n | m}` production, the comma list had none once the declare
//! reduced. The grammar now derives the one-side OPD_CURLY (the `this{a,b}`
//! precedent) from `mc_declare_a1` / `mc_declare_b`, and the semantic layer
//! reads the degenerate face off it (left = right = the members, the column
//! solution of vec-dianlu.md §3.6). Every spelling below must land
//! byte-identical to its use-position comma form.

use crate::common;

use mcc::{McIds, McURI};

const PRELUDE: &str = r#"
component LDO2(r) {
    pins = [
        1 = VIN, "input"
        2 = VOUT, "output"
        3 = GND, "ground"
    ]
}
module top {
    io A
    io B
    io G
"#;

fn netlines(src: &str) -> (Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/u354-comma-decl.mc");
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("top"), &uri, 1000).expect("flat build");
    let mut lines: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = Vec::new();
        for pid in net.points.iter() {
            if let Some(e) = table.get_entry(*pid) {
                pts.push(e.path.clone());
            }
        }
        pts.sort();
        lines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    lines.sort();
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (lines, codes)
}

/// The statement-position comma declare registers the degenerate face, and
/// the bare reference zips it memberwise — byte-identical to the
/// use-position comma form.
#[test]
fn u354__decl_comma_face_zips_memberwise_like_use_position() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN, VOUT}}\n    [A, G] -> ldo -> [B, G]\n}}"
    ));
    assert_eq!(
        lines,
        vec![
            "A <= [top.A, top.B, top.ldo.1]".to_string(),
            "G <= [top.G, top.ldo.2]".to_string(),
        ]
    );
    assert!(!codes.contains(&2082), "codes: {codes:?}");
    assert!(!codes.contains(&4007), "codes: {codes:?}");
}

/// The chain-position comma declare rides the same face: the declare inside
/// the chain exposes the degenerate Ports and the neighbours zip memberwise.
#[test]
fn u354__chain_comma_declare_zips_memberwise() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    [A, G] -> LDO2 ldo{{VIN, VOUT}} -> [B, G]\n}}"
    ));
    assert_eq!(
        lines,
        vec![
            "A <= [top.A, top.B, top.ldo.1]".to_string(),
            "G <= [top.G, top.ldo.2]".to_string(),
        ]
    );
    assert!(!codes.contains(&2082), "codes: {codes:?}");
    assert!(!codes.contains(&4007), "codes: {codes:?}");
}

/// The `::` declare form takes the comma spelling through the same arm and
/// lands byte-identical.
#[test]
fn u354__colon_comma_declare_rides_the_same_face() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    ldo::LDO2(1u){{VIN, VOUT}}\n    [A, G] -> ldo -> [B, G]\n}}"
    ));
    assert_eq!(
        lines,
        vec![
            "A <= [top.A, top.B, top.ldo.1]".to_string(),
            "G <= [top.G, top.ldo.2]".to_string(),
        ]
    );
    assert!(!codes.contains(&2082), "codes: {codes:?}");
}

/// A single-member comma face is the dot form's degenerate twin: one member
/// repeated on both sides joins both chain neighbours to that one pin.
#[test]
fn u354__single_member_comma_face_matches_the_dot_form() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(lines, vec!["A <= [top.A, top.B, top.ldo.1]".to_string()]);
    assert!(!codes.contains(&2082), "codes: {codes:?}");
    assert!(!codes.contains(&4007), "codes: {codes:?}");
}

/// A face member that is not a pin reports E3179 and is pruned — the comma
/// face never carries a phantom endpoint. The surviving single-member face
/// still wires the 1-wide neighbours.
#[test]
fn u354__comma_face_bad_member_reports_e3179_and_is_pruned() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN, BOGUS}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(
        lines,
        vec!["A <= [top.A, top.B, top.ldo.1]".to_string()],
        "codes: {codes:?}"
    );
    assert!(codes.contains(&3179), "codes: {codes:?}");
    assert!(!codes.contains(&2082), "codes: {codes:?}");
}
/// Every comma member pruned leaves no face: the bare reference falls back
/// to the whole-instance anchor, exactly like a face-less declaration.
#[test]
fn u354__comma_face_all_members_bad_drops_to_the_anchor_fallback() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{XXX, YYY}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(lines, vec!["A <= [top.A, top.B, top.ldo]".to_string()]);
    assert!(codes.contains(&3179), "codes: {codes:?}");
}

/// The comma face is a column vector: a 1-wide chain against the 2-wide face
/// reports the series shape mismatch instead of silently truncating.
#[test]
fn u354__narrow_chain_against_wide_comma_face_reports_e4007() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN, VOUT}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert!(lines.is_empty(), "lines: {lines:?}");
    assert!(
        codes.contains(&4007),
        "the narrow chain must report the series shape mismatch: codes: {codes:?}"
    );
}
