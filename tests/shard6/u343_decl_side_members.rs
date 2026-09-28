// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U343 B1 arm 5: the declaration-position face reader (`split_decl_face`)
//! used to read every side member through `to_id_or_ida_or_num`, whose
//! fallback descends into the first sub-chain only. A dotted member (`VIN.1`,
//! the `mc_ids MCPT_DOT mc_int` shape) therefore read as the bare head `VIN`
//! — red-proven on the pipe spelling: the face silently exposed
//! `["VIN", "VOUT"]` and the chain wired `A` to `top.ldo.1` behind the
//! author's back, with no diagnostic. The reader now goes through
//! `McPhrase::curly_mn_side_members` (the arm-2 structural reader), so the
//! member keeps its full name and the face resolver prunes it with E3179.
//!
//! The three declaration spellings diverge, and each case below records the
//! empirically verified shape:
//! - pipe: the dotted member is pruned (E3179); only the surviving member
//!   wires. This is the red-proofed case.
//! - comma: the grammar itself rejects a dotted member at the GLR stage
//!   (E2081/E2082, both faces agree) — the reader never sees the shape, so
//!   the reader change on that branch is defense-in-depth with no fixture
//!   that can reach it.
//! - dot: pre and post read identically here — the unresolvable member set
//!   drops the face to the whole-instance anchor fallback, and the dot-face
//!   path prunes silently (no E3179; pre-existing behavior, unchanged by
//!   this arm). Locked as a behavior guard: the truncated head must never
//!   wire as `top.ldo.1`.
//!
//! The healthy plain spellings are covered by `u343_decl_face.rs`
//! (pipe / dot) and `u354_comma_decl.rs` (comma) — they run against the same
//! reader now. The CURLY_MN empty-side guard (a side that reads zero members
//! drops the face) has no fixture: a side that would read zero members
//! either dies at the GLR stage or is the plain grammar-degenerate form that
//! already reads its member.

#![allow(non_snake_case)]

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
    let uri = McURI::from("/mcc/u343-decl-side-members.mc");
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

/// A dotted member on a pipe side keeps its full name: the face prunes it
/// with E3179 and wires only the surviving member. The pre-fix read exposed
/// the bare head `VIN` instead, and the chain silently wired `A` to
/// `top.ldo.1` — the author's `.1` never entered the picture.
#[test]
fn u343_b5__dotted_pipe_decl_member_is_pruned_not_truncated() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN.1 | VOUT}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(
        lines,
        vec!["B <= [top.B, top.ldo.2]".to_string()],
        "only the surviving VOUT member wires; the dotted member must not land as bare VIN (ldo.1): lines: {lines:?}"
    );
    assert!(
        codes.contains(&3179),
        "the full dotted member must be reported, not silently truncated: codes: {codes:?}"
    );
}

/// The comma spelling never reaches the reader with a dotted member: the
/// grammar rejects the shape at the GLR stage and both faces agree (the same
/// honest double report the statement position gives). Locked so a grammar
/// relaxation cannot silently start truncating on this branch.
#[test]
fn u343_b5__dotted_comma_decl_member_dies_at_the_grammar() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo{{VIN.1, VOUT}}\n    [A] -> ldo -> [B]\n}}"
    ));
    assert!(lines.is_empty(), "lines: {lines:?}");
    assert!(
        codes.contains(&2081) && codes.contains(&2082),
        "the dotted comma member must die at the grammar, both faces: codes: {codes:?}"
    );
}

/// The dot spelling drops the unresolvable member set to the whole-instance
/// anchor fallback — silently (no E3179 on this path; pre-existing behavior,
/// identical pre and post). The property under lock: the truncated head must
/// never wire as `top.ldo.1`.
#[test]
fn u343_b5__dotted_dot_decl_member_drops_to_the_anchor_fallback() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    LDO2 ldo.VIN.1\n    [A] -> ldo -> [B]\n}}"
    ));
    assert_eq!(
        lines,
        vec!["A <= [top.A, top.B, top.ldo]".to_string()],
        "the face falls back to the whole-instance anchor, never the truncated head pin: lines: {lines:?}"
    );
    assert!(
        !codes.contains(&3179),
        "the dot-face path prunes silently (pre-existing); codes: {codes:?}"
    );
}
