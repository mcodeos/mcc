// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U343-C2: the declaration-position sub selection is the instance's default
//! exposed face.
//!
//! `LDO2 ldo{VIN | VOUT}` / `LDO2 ldo.VIN` wrap the declare as the head child
//! of an OPD_CURLY_MN / OPD_DOT. The generic connection parse used to strand
//! the selection as a single-member statement (no adjacency to wire), so the
//! face was inert and a later bare reference to `ldo` fell through to the
//! whole-instance anchor. The landing routes the wrapped declare through the
//! declaration path, records the selection as the instance's default exposed
//! face (members validated against the class pins, E3179 + pruned), and makes
//! the bare reference evaluate through the face — byte-identical to the
//! use-position curly form. The `::` declare form rides the same unwrap.

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
"#;

fn netlines(src: &str) -> (Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/u343-decl-face.mc");
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

/// The declaration pipe face registers, and the bare reference consumes it:
/// the endpoints are the selected pins, not the whole-instance anchor.
#[test]
fn u343__decl_pipe_face_bare_reference_consumes_the_face() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN | VOUT}}\n    [A] -> ldo -> [B]\n}}"
));
    assert_eq!(
        lines,
        vec![
            "A <= [top.A, top.ldo.1]".to_string(),
            "B <= [top.B, top.ldo.2]".to_string(),
        ]
    );
    assert_eq!(codes, vec![5641, 5102, 4116, 4119]);
}

/// The degenerate dot selection repeats the single member on both sides, so
/// the bare reference joins both chain neighbours to that one pin.
#[test]
fn u343__decl_dot_face_single_member_joins_both_neighbours() {
    let (lines, codes) = netlines(&format!("{PRELUDE}    LDO2 ldo.VIN\n    [A] -> ldo -> [B]\n}}"));
    assert_eq!(lines, vec!["A <= [top.A, top.B, top.ldo.1]".to_string()]);
    assert_eq!(codes, vec![5641, 5102, 4116, 4119, 4119]);
}

/// A face member that is not a pin reports E3179 and is pruned — the face
/// never carries a phantom endpoint.
#[test]
fn u343__decl_face_bad_member_reports_e3179_and_is_pruned() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN | BOGUS}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(lines, vec!["A <= [top.A, top.ldo.1]".to_string()]);
    assert!(codes.contains(&3179), "codes: {codes:?}");
    assert!(!codes.contains(&4007), "codes: {codes:?}");
}

/// Every face member pruned leaves no face: the bare reference falls back to
/// the whole-instance anchor, exactly like a face-less declaration.
#[test]
fn u343__decl_face_all_members_bad_drops_to_the_anchor_fallback() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{XXX | YYY}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(lines, vec!["A <= [top.A, top.B, top.ldo]".to_string()]);
    assert!(codes.contains(&3179), "codes: {codes:?}");
}

/// The `::` declare form wraps the same MCAST_DECLARE, so the same unwrap
/// attaches the face to the materialized instance.
#[test]
fn u343__iface_colon_declare_form_rides_the_same_face() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    ldo::LDO2(1u){{VIN | VOUT}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(
        lines,
        vec![
            "A <= [top.A, top.ldo.1]".to_string(),
            "B <= [top.B, top.ldo.2]".to_string(),
        ]
    );
    assert_eq!(codes, vec![5641, 5102, 4116, 4119]);
}

/// Control: a face-less declaration keeps the whole-instance anchor.
#[test]
fn u343__faceless_declaration_keeps_the_anchor_fallback() {
    let (lines, codes) = netlines(&format!("{PRELUDE}    LDO2 ldo\n    [A] -> ldo -> [B]\n}}"));
    assert_eq!(lines, vec!["A <= [top.A, top.B, top.ldo]".to_string()]);
    assert_eq!(codes, vec![5641, 5102, 4112, 4116, 4119, 4119, 4119]);
}
