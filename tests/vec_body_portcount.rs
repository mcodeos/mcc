// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance surface for the **body pair** constraint on `+` / `-`: two
//! component bodies may only be paired when their port counts match.
//!
//! # The law being locked
//!
//! `+` between two component bodies stacks them: the terminals line up
//! column by column, so a 1-port body against a 2-port body has nothing to
//! pair with. `TP1 + R1` is the flagged case -- treating it as **silently
//! legal**, emitting one connection `{TP1.1, R1.1}` with no diagnostic at all,
//! is what happens when `TP1` reduces to a bare `Point` and the engine applies
//! the face-side law as if it were a net label, implicitly taking its only pin.
//!
//! The constraint is on the **pair**, not on either operand alone. A body
//! against a non-body stays legal and keeps its ordinary reading:
//!
//! | statement | bodies | ports | verdict |
//! |---|---|---|---|
//! | `TP1 + R1` / `R1 + TP1` | two | 1 ≠ 2 | E2908, no connection |
//! | `TP1 + TP2` | two | 1 = 1 | legal, one net |
//! | `R1 + R2` | two | 2 = 2 | legal, two nets |
//! | `VCC + R1` | one | -- | legal (net attach, the face-side law) |
//! | `(R2.1 -> VLBL) + TP1` | one | -- | legal (hang a test point on a node) |
//!
//! The last row is live real-board usage (`hbl/src/power.mc` writes
//! `((usbsock.VBUS -> USB_VBUS) + TP1) -> RES(0R) -> ...` to put a label and a
//! test point on the same node), which is why the predicate reads the pair and
//! not the degenerate side: forbidding bodies on the degenerate side outright
//! would outlaw that idiom.
//!
//! # What is asserted
//!
//! The diagnostic code for the rejected pair, and the **net partition** for the
//! legal ones -- the grouping of points, not the operator tags or the
//! synthesized net names.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Plain two-pin resistor, pins `1 = 1` / `2 = 2`.
const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Plain one-pin test point -- a **real component body** with a single port.
const TESTPOINT: &str = "component TESTPOINT {\n    pins = [\n        1 = 1\n    ]\n}\n";

/// Codes that are build-info, not a verdict (same set the vector-oracle
/// family tolerates).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
}

/// Build `main` and return (non-benign codes sorted, net partition).
///
/// Single-point nets do not appear in the store, so a dangling pin is the
/// *absence* of a net rather than a one-element one.
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{RES2}{TESTPOINT}module main {{\n    RES2 R1\n    RES2 R2\n    \
         TESTPOINT TP1\n    TESTPOINT TP2\n    func M() {{\n{body}\n    }}\n}}\n"
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

/// The two-pin body pair: `R1.1 <-> R2.1` and `R1.2 <-> R2.2`, one net per
/// column. Written out so the clean cells state the same law.
fn resistor_pair() -> Vec<Vec<String>> {
    vec![
        vec!["R1.1".to_string(), "R2.1".to_string()],
        vec!["R1.2".to_string(), "R2.2".to_string()],
    ]
}

/// Assert the pair is rejected with E2908 and emits no connection.
fn rejected(uri: &str, body: &str) {
    let (codes, nets) = build(body, uri);
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_INST_PORTCOUNT_PLUSMINUS),
        "a 1-port body against a 2-port body must be rejected; got {codes:?}"
    );
    assert!(
        nets.is_empty(),
        "the rejected pair generates no connection; got {nets:?}"
    );
}

/// Assert the pair is accepted quietly and produces exactly `expected`.
fn accepted(uri: &str, body: &str, expected: Vec<Vec<String>>) {
    let (codes, nets) = build(body, uri);
    assert_eq!(codes, Vec::<u32>::new(), "quiet statement; got {codes:?}");
    assert_eq!(nets, expected, "wrong net partition");
}

// the flagged pair: two bodies, unequal port counts

/// `TP1 + R1` -- the flagged statement. A 1-port body written against a 2-port
/// body. Pre-fix this was silently legal: one connection `{TP1.1, R1.1}` and no
/// diagnostic, the test point's only pin implicitly taken by the face-side law.
#[test]
fn body_pair__one_port_against_two_ports_is_rejected() {
    rejected("/mcc/body-pair-tp-r.mc", "        TP1 + R1");
}

/// `R1 + TP1` -- the mirror. The 1-port body is now written **right**, which is
/// a different branch of the face-side law, so it needs its own cell.
#[test]
fn body_pair__mirror_one_port_right_is_rejected() {
    rejected("/mcc/body-pair-r-tp.mc", "        R1 + TP1");
}

/// `TP1 - R1` -- the series operator carries the same constraint (E2905 already
/// scopes its 3+ port rule to `+` / `-`), so the pair rule does too.
#[test]
fn body_pair__series_one_port_against_two_ports_is_rejected() {
    rejected("/mcc/body-pair-series.mc", "        TP1 - R1");
}

// the legal pairs

/// `TP1 + TP2` -- two bodies, **equal** port counts. Both are single-point, so
/// the pair is element-wise: one net carrying both test points' only pin.
#[test]
fn body_pair__equal_one_port_bodies_are_legal() {
    accepted(
        "/mcc/body-pair-tp-tp.mc",
        "        TP1 + TP2",
        vec![vec!["TP1.1".to_string(), "TP2.1".to_string()]],
    );
}

/// `R1 + R2` -- two 2-port bodies, one net per column. This is the shape the
/// constraint exists to protect: the terminals line up, so the stack is real.
#[test]
fn body_pair__equal_two_port_bodies_are_legal() {
    accepted("/mcc/body-pair-r-r.mc", "        R1 + R2", resistor_pair());
}

// a body against a non-body: unconstrained

/// `VCC + R1` -- only **one** body. The label takes the face-side law's net
/// attach and must not be caught by the pair rule.
#[test]
fn body_pair__label_against_body_is_legal() {
    accepted(
        "/mcc/body-pair-label.mc",
        "        VCC + R1",
        // `R1.2` stays free -- a single point is no net, so it is absent.
        vec![vec!["R1.1".to_string(), "VCC".to_string()]],
    );
}

/// `(R2.1 -> R1.1) + TP1` -- the real-board idiom: a node built by a series,
/// with a test point hung on it (`hbl/src/power.mc` writes the same shape as
/// `((usbsock.VBUS -> USB_VBUS) + TP1) -> RES(0R) -> ...`, there with a net
/// label as the series' right end). The left operand is a series result, not a
/// body, so the pair rule does not apply and the test point's pin merges with
/// the node.
#[test]
fn body_pair__test_point_on_a_node_is_legal() {
    accepted(
        "/mcc/body-pair-tp-node.mc",
        "        (R2.1 -> R1.1) + TP1",
        vec![vec![
            "R1.1".to_string(),
            "R2.1".to_string(),
            "TP1.1".to_string(),
        ]],
    );
}
