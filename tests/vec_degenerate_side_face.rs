// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance surface for the **degenerate side written left** `Parallel`
//! family (`VCC + R101`) -- the one `[V]` the L0 operand-fold design left open
//! (`l0-operand-fold-design.md` §1.5 items 1-2 / §5 open item 1,
//! `vec-dianlu.md` §8.4 item 3, the "incidental finding").
//!
//! # The law being locked
//!
//! `+` consumes no port, so the operand that carries no left/right distinction
//! of its own (`Point` / `Column` -- a bare net label, a rail list, a single
//! pin) has to pick a face: it sticks to the face on its **written** side
//! (§5.1, the face-side law). The result therefore is **not** `opds[0]`:
//!
//! | written form | result left | result right |
//! |---|---|---|
//! | `1*1 + 1*2` (degenerate left) | `[lopd]` (the merged net) | `ropd.2` (free port) |
//! | `1*2 + 1*1` (degenerate right) | `lopd.1` | `lopd.2` |
//! | `N*1 + N*1,M*1` (degenerate left) | `lopd` (N) | `ropd`'s right face (M) |
//!
//! `VDD + R1 -> GND` is the first row: the `-> GND` leg must land on `R1.2`,
//! the *free* port of `opds[1]`, not on the label written first.
//!
//! **Both** operands degenerate (`[VCC, GND] + [R1.1, R1.2]`) is the tier
//! *above* the law: no face has to be chosen, §5.1's `N*1 + N*1` row applies,
//! and the columns pair **element-wise** -- one net per lane.
//!
//! # The width face
//!
//! The same law decides the **shape** a `+` presents to its neighbours
//! (`OpdShape::of`, via `eval_port_elems`), and that half is where the third
//! row is observable: `[A, B] + (nd) -> [X, Y, Z]` with `nd` a body whose in-
//! and out-faces are 2 and 3 wide. Reading the width off `opds[0]` makes the
//! `+` a two-wide column, the `-> [X, Y, Z]` leg a 2-vs-3 mismatch, and drops
//! the whole statement (E4007); reading §5.1's result row makes it a
//! `node{[A,B] | [nd.3, nd.4, nd.5]}` and the leg legal. The cells at the end
//! lock both sides of that verdict.
//!
//! # What this fixture caught (2026-09-11)
//!
//! Three cells below were red when this file was written, and the three
//! failures were three different defects, none of which any existing target
//! observed:
//!
//! * `VDD + R1 -> GND` emitted **only** the `Parallel` connection and dropped
//!   the `R1.2 <-> GND` leg **silently** (no diagnostic). A bare `Label`
//!   endpoint has empty faces through the raw accessors
//!   (`get_left_points` / `get_right_points` return `vec![]` for it), so its
//!   reduction is `Unknown` with two empty faces; `fold_parallel` then took the
//!   left-anchored branch and returned empty faces, which `vexpr_step` read as
//!   the ordinary "nothing on this face" skip.
//! * `(VDD + R1) -> GND` -- the same statement parenthesized -- **shorted
//!   `VDD` to `GND`**, because the one-element `Group` fell to the raw
//!   accessors, whose `Group` arm concatenates the branches' faces, and the
//!   inner `Parallel`'s flavour there is `opds[0]`.
//! * `[VCC, GND] + [R1.1, R1.2]` emitted one over-wide net
//!   `[VCC, R1.1, R1.2]` -- `GND` dropped silently -- because a lane stack
//!   arrives as a multi-lane `Multiple` of bare names, again with no face
//!   through the raw accessors, and the member-view fallback kept only
//!   `normalized.first()`; every lane but the first was lost. A following
//!   `-> [RA, RB]` leg vanished whole for the same reason (the `+`'s external
//!   face came out empty, so the series had nothing to attach to).
//!
//! The fixes live in `vexpr/eval.rs`: the external-face fold normalizes a bare
//! `Label` / `List` / `Interface` endpoint to its member view
//! (`vexpr_fold_parallel_face`), a multi-lane `Multiple` operand unions **all**
//! of its lanes' faces on both halves of the `+`, and `vexpr_fold_member`
//! gained the see-through unary-`Group` arm the module doc had assumed away.
//!
//! # What is asserted
//!
//! The **net partition** -- the grouping of points -- not the operator tags or
//! the synthesized net names. All three defects above are visible as a wrong
//! partition (a missing member, or two nets collapsed into one), so the law
//! is what the partition states, and it stays readable if the internal
//! representation moves.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Plain two-pin resistor, pins `1 = 1` / `2 = 2`.
const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Codes that are not this family's verdict (same set the vector-oracle
/// family tolerates). `3136` (floating net label) joins them because the cells
/// below write **bare** `VDD` / `GND` as the degenerate-side operand under
/// test: a name no scope declares is a floating label since world-axioms §1 A1
/// replaced the rail-*spelling* exemption, and this file already carries E3136
/// alongside its verdict that way (see the `E3136` note further down).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054 | 3136)
}

/// Build `main` and return (non-benign codes sorted, net partition).
///
/// The partition is normalized to a sorted list of sorted member lists: net
/// *names* are synthesized, so the claim is about the **grouping of points**.
/// Single-point nets do not appear in the store, so a dangling pin is the
/// *absence* of a net rather than a one-element one.
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{RES2}module main {{\n    RES2 R1\n    RES2 R2\n    func M() {{\n{body}\n    }}\n}}\n"
    );
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

/// The two nets the flagged statement must produce: the label shorted onto the
/// resistor's **left** pin, and its **right** pin carried on to ground. Written
/// out once so every cell below states the same law.
fn two_nets(uri: &str, body: &str) -> Vec<Vec<String>> {
    let (codes, nets) = build(body, uri);
    assert_eq!(codes, Vec::<u32>::new(), "quiet statement; got {codes:?}");
    nets
}

// the flagged fixture: degenerate side written left

/// `VDD + R1 -> GND` -- the exact statement L0 §1.5 named as the open `[V]`.
///
/// The label is written first, so it merges onto `R1`'s **left** face and the
/// `->` leg must reach the free `R1.2`. Pre-fix this statement emitted one
/// connection and dropped the ground leg with no diagnostic at all.
#[test]
fn degenerate_side__written_left_keeps_the_series_leg() {
    let nets = two_nets("/mcc/degen-left.mc", "        VDD + R1 -> GND");
    assert_eq!(
        nets,
        vec![
            vec!["GND".to_string(), "R1.2".to_string()],
            vec!["R1.1".to_string(), "VDD".to_string()],
        ],
        "the label lands on R1.1 and the ground leg on R1.2; got {nets:?}"
    );
}

/// `(VDD + R1) -> GND` -- the same statement parenthesized. The parentheses are
/// a structural marker, transparent to evaluation (L0 §3.6), so the partition
/// must be **identical** to the unparenthesized cell above.
///
/// Pre-fix this was not merely imprecise but wrong: the group fell to the raw
/// accessors, whose inner-`Parallel` flavour is `opds[0]`, and the `-> GND` leg
/// shorted the label straight to ground (`[VDD, GND]`).
#[test]
fn degenerate_side__parenthesized_left_is_see_through() {
    let nets = two_nets("/mcc/degen-paren.mc", "        (VDD + R1) -> GND");
    assert_eq!(
        nets,
        vec![
            vec!["GND".to_string(), "R1.2".to_string()],
            vec!["R1.1".to_string(), "VDD".to_string()],
        ],
        "a one-element group is see-through; got {nets:?}"
    );
    assert!(
        !nets.iter().any(|n| n.contains(&"GND".to_string())
            && n.contains(&"VDD".to_string())
            && !n.contains(&"R1.2".to_string())),
        "the label must never reach ground except through the resistor; got {nets:?}"
    );
}

/// `VDD + (R1 -> GND)` -- the group on the **other** side of the `+`. Here the
/// group is a `+` operand, not the `->` accumulator, so the degenerate label
/// still sticks left and the same partition must come out.
#[test]
fn degenerate_side__group_as_parallel_operand_is_see_through() {
    let nets = two_nets("/mcc/degen-paren-right.mc", "        VDD + (R1 -> GND)");
    assert_eq!(
        nets,
        vec![
            vec!["GND".to_string(), "R1.2".to_string()],
            vec!["R1.1".to_string(), "VDD".to_string()],
        ],
        "the group operand folds as the series it wraps; got {nets:?}"
    );
}

/// `R2.1 + R1 -> GND` -- a **`Point`** degenerate left operand (a pin, not a
/// label). The degenerate-left branch is the same one, but reached with an
/// operand that resolves by pointer rather than by name; it must behave
/// identically.
#[test]
fn degenerate_side__point_left_operand_keeps_the_series_leg() {
    let nets = two_nets("/mcc/degen-point.mc", "        R2.1 + R1 -> GND");
    assert_eq!(
        nets,
        vec![
            vec!["GND".to_string(), "R1.2".to_string()],
            vec!["R1.1".to_string(), "R2.1".to_string()],
        ],
        "the pin lands on R1.1 and the ground leg on R1.2; got {nets:?}"
    );
}

// both sides degenerate: the lane stack

/// `[VCC, GND] + [R1.1, R1.2]` -- **both** operands degenerate, so no face has
/// to be chosen: §5.1's `N*1 + N*1` row applies and the two columns pair
/// **element-wise**, giving two nets `{VCC, R1.1}` / `{GND, R1.2}`.
///
/// Pre-fix this emitted a single over-wide net `[VCC, R1.1, R1.2]` and dropped
/// `GND` with no diagnostic, because both operands reach the fold as a
/// multi-lane `Multiple` of bare names, which has **no** face through the raw
/// accessors; the member-view fallback then kept only `normalized.first()`.
/// A following `-> [RA, RB]` leg was dropped whole for the same reason (the
/// `+` had empty external faces, so the series found nothing to attach to).
#[test]
fn degenerate_side__both_columns_pair_element_wise() {
    let nets = two_nets("/mcc/degen-column.mc", "        [VCC, GND] + [R1.1, R1.2]");
    assert_eq!(
        nets,
        vec![
            vec!["GND".to_string(), "R1.2".to_string()],
            vec!["R1.1".to_string(), "VCC".to_string()],
        ],
        "two columns pair lane by lane, one net per lane; got {nets:?}"
    );
}

// the mirror: degenerate side written right

/// `R1 + VDD -> GND` -- the mirror of the flagged family (§5.1 table row
/// `1*2 + 1*1`). The label is now written **right**, so it merges onto `R1`'s
/// right face and the result keeps `opds[0]`'s own two faces, left to right:
/// `R1.1`, `R1.2`. The `-> GND` leg therefore lands on `R1.2` -- the face the
/// label was just merged onto -- which ties the label to ground through the
/// resistor's right end, and leaves `R1.1` free.
///
/// This is the law's outcome, not a preferred circuit. §1.4 states the reason
/// directly: a chain is continued by writing the new operand on the **right**,
/// so it attaches to the accumulated value's right face -- "the chain's
/// extension end (the opening)". In this written order the opening *is* the
/// merged `VDD` net, and the `->` leg (the outer-right end of the series, by
/// §5.2, the outer-ends law) is exactly that face. The idiomatic writing of a resistor
/// from a rail to ground is `VDD + R1 -> GND` (the cell above), where the free
/// pin is the outer right end.
///
/// It is locked here because it is the `else` arm of the face-side law --
/// without it the mirror branch of the fold would be covered by no fixture at
/// all -- and because the day the law changes it must be this test that says
/// so.
#[test]
fn degenerate_side__mirror_written_right_follows_the_table() {
    let nets = two_nets("/mcc/degen-mirror.mc", "        R1 + VDD -> GND");
    assert_eq!(
        nets,
        vec![vec![
            "GND".to_string(),
            "R1.2".to_string(),
            "VDD".to_string(),
        ]],
        "the label merges onto R1.2 and the ground leg reuses that face; got {nets:?}"
    );
}

// the width face: a column against a body with unequal faces

/// Body with **unequal** faces: in-pins `1`, `2` (width 2) and out-pins
/// `3`, `4`, `5` (width 3). A two-pin body has no such asymmetry, so this is
/// the only shape that makes §5.1's third row observable as a width.
const BODY5: &str = "component BODY5 {\n    pins = [\n        in 1 = I1\n        in 2 = I2\n        out 3 = O1\n        out 4 = O2\n        out 5 = O3\n    ]\n}\n";

/// Build `main` with a single `BODY5 U1` and return (non-benign codes sorted,
/// net partition). Same normalization as [`build`], with the body in place of
/// the resistor pair.
///
/// The five labels are declared module ports: an undeclared bare name raises
/// `E3136` (floating label) even when the wiring is right, and the width face,
/// not the label resolution, is what these cells are about. The handful of
/// rail names that are exempt (`VCC` / `VDD` / `GND`) cannot supply five
/// distinct labels.
fn build_body5(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{BODY5}module main {{\n    io A\n    io B\n    io X\n    io Y\n    io Z\n    BODY5 U1\n    func M() {{\n{body}\n    }}\n}}\n"
    );
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

/// The [`two_nets`] contract over the [`build_body5`] header.
fn two_nets_body5(uri: &str, body: &str) -> Vec<Vec<String>> {
    let (codes, nets) = build_body5(body, uri);
    assert_eq!(codes, Vec::<u32>::new(), "quiet statement; got {codes:?}");
    nets
}

/// `[A, B] + (U1) -> [X, Y, Z]` -- the divergence cell of L0 §1.5 item 2, the
/// width face of the same law. The `+`'s result row is `column 2*1 + node
/// 2*1,3*1`, so its right face is `U1`'s three out-pins and the three-wide
/// series leg is legal. Read off `opds[0]` the `+` is a two-wide column and the
/// leg is a 2-vs-3 mismatch that drops the whole statement.
#[test]
fn degenerate_side__column_against_unequal_body_reads_the_result_row() {
    let nets = two_nets_body5(
        "/mcc/degen-width-body.mc",
        "        [A, B] + (U1) -> [X, Y, Z]",
    );
    assert_eq!(
        nets,
        vec![
            vec!["A".to_string(), "U1.1".to_string()],
            vec!["B".to_string(), "U1.2".to_string()],
            vec!["U1.3".to_string(), "X".to_string()],
            vec!["U1.4".to_string(), "Y".to_string()],
            vec!["U1.5".to_string(), "Z".to_string()],
        ],
        "the column pairs with the in-face and the out-face carries the leg; got {nets:?}"
    );
}

/// The rejected mirror: `[A, B] + (U1) -> [X, Y]`. The **same** result row read
/// with a two-wide leg -- `node 2*1,3*1` against a two-wide column -- is a real
/// §5.2 mismatch, so the cell proves the width face is *read*, not merely that
/// the statement stopped erroring: a fix that made every `+` legal would pass
/// the cell above and fail this one.
#[test]
fn degenerate_side__column_against_unequal_body_narrow_leg_is_rejected() {
    let (codes, _) = build_body5(
        "        [A, B] + (U1) -> [X, Y]",
        "/mcc/degen-width-narrow.mc",
    );
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "a 3-wide face against a 2-wide leg is a series mismatch; got {codes:?}"
    );
}

/// The node spelling of the same cell: `[A, B] + U1{1, 2 | 3, 4, 5} -> [X, Y, Z]`.
/// Writing the faces out instead of letting the body supply them must reach the
/// identical partition -- the width is a property of the result row, not of how
/// the operand was spelled.
#[test]
fn degenerate_side__column_against_written_node_reads_the_result_row() {
    let nets = two_nets_body5(
        "/mcc/degen-width-node.mc",
        "        [A, B] + U1{1, 2 | 3, 4, 5} -> [X, Y, Z]",
    );
    assert_eq!(
        nets,
        vec![
            vec!["A".to_string(), "U1.1".to_string()],
            vec!["B".to_string(), "U1.2".to_string()],
            vec!["U1.3".to_string(), "X".to_string()],
            vec!["U1.4".to_string(), "Y".to_string()],
            vec!["U1.5".to_string(), "Z".to_string()],
        ],
        "the node spelling must fold to the same result row; got {nets:?}"
    );
}
