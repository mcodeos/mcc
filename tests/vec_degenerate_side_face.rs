// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance surface for the **degenerate side written left** `Parallel`
//! family (`VCC + R101`) -- the one `[V]` the L0 operand-fold design left open
//! (`mcd/doc/vector-conn-l0-operand-fold-design.md` §1.5 items 1-2 / §5 open item 1,
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
//!
//! `VDD + R1 -> GND` is the first row: the `-> GND` leg must land on `R1.2`,
//! the *free* port of `opds[1]`, not on the label written first.
//!
//! **Both** operands degenerate (`[VCC, GND] + [R1.1, R1.2]`) is the tier
//! *above* the law: no face has to be chosen, §5.1's `N*1 + N*1` row applies,
//! and the columns pair **element-wise** -- one net per lane.
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

/// Codes that are build-info, not a verdict (same set the vector-oracle
/// family tolerates).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
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

// ── the flagged fixture: degenerate side written left ───────────────────────

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

// ── both sides degenerate: the lane stack ───────────────────────────────────

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

// ── the mirror: degenerate side written right ───────────────────────────────

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
