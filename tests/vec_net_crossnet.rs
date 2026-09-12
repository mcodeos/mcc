// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance surface for the **net pair** constraint on `+`: two bodiless
//! operands may not be fused, because a label names an equipotential region
//! that already exists.
//!
//! # The law being locked
//!
//! A label or rail is not a connection — it is the **name of an existing
//! equipotential region** (vec-dianlu §1.4 / §5.4). Its potential is carried
//! by its name, so two different names are two different potentials.
//!
//! `+` against a body extends that region onto the body's written-side pin:
//! the face-side law's net attach, which is legitimate. But when **neither**
//! side is a body there is nothing to stack and nothing to extend onto, so
//! `+` can only fuse the two regions into one — a dead short. `VCC + GND` used
//! to be **silently legal**: one net, zero diagnostics.
//!
//! | statement | bodies | verdict |
//! |---|---|---|
//! | `VCC + GND` / `GND + VCC` | none | E2908's sibling: `CONN_NET_CROSSNET`, no connection |
//! | `VCC + VCC` | none | legal (one region named twice) |
//! | `VCC + R1` | one | legal (net attach, the face-side law) |
//! | `TP1 + R1` | two | E2908 (the *body* pair rule, which runs first) |
//!
//! The rule is deliberately **`+`-only**: it is the parallel merge that fuses
//! the two regions. This is the bodiless sibling of `vec_body_portcount.rs`
//! — that one fires when *both* sides are bodies, this one when *neither* is;
//! `+` with exactly one body is unconstrained by both.
//!
//! # What is asserted
//!
//! The diagnostic code for the rejected merge, and the **net partition** for
//! the legal ones — the grouping of points, not the operator tags or the
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
/// Single-point nets do not appear in the store, so a lone label is the
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

/// Assert the merge is rejected with `CONN_NET_CROSSNET` and emits no
/// connection. The cascade codes the dropped statement drags in are allowed;
/// what matters is that the verdict code is present and nothing got wired.
fn rejected(uri: &str, body: &str) {
    let (codes, nets) = build(body, uri);
    assert!(
        codes.contains(&mcc::errcodes::CONN_NET_CROSSNET),
        "two distinct bodiless nets must not be fused; got {codes:?}"
    );
    assert!(
        nets.is_empty(),
        "the rejected merge generates no connection; got {nets:?}"
    );
}

/// Assert the pair is accepted quietly and produces exactly `expected`.
fn accepted(uri: &str, body: &str, expected: Vec<Vec<String>>) {
    let (codes, nets) = build(body, uri);
    assert_eq!(codes, Vec::<u32>::new(), "quiet statement; got {codes:?}");
    assert_eq!(nets, expected, "wrong net partition");
}

// the flagged merge: two bodiless operands, two different names

/// `VCC + GND` -- the flagged statement. Two labels, no body on either side.
/// Pre-fix this was silently legal: one net carrying both names, no diagnostic.
#[test]
fn net_pair__two_distinct_labels_are_rejected() {
    rejected("/mcc/net-pair-vcc-gnd.mc", "        VCC + GND");
}

/// `GND + VCC` -- the mirror. The predicate reads the written pair, so the
/// order must not change the verdict.
#[test]
fn net_pair__order_does_not_matter() {
    rejected("/mcc/net-pair-gnd-vcc.mc", "        GND + VCC");
}

// the legal boundaries

/// `VCC + VCC` -- the same name twice names **one** region, so there is no
/// cross-net to report. This is the discriminator that says the rule is
/// *name inequality*, not merely "both sides are bodiless". The result is a
/// single-point net, which the store omits.
#[test]
fn net_pair__same_label_twice_is_legal() {
    accepted("/mcc/net-pair-vcc-vcc.mc", "        VCC + VCC", vec![]);
}

/// `VCC + R1` -- exactly **one** body. The label attaches to the body's
/// written-side pin (the face-side law) and must not be caught by this rule.
#[test]
fn net_pair__label_against_body_is_legal() {
    accepted(
        "/mcc/net-pair-label-body.mc",
        "        VCC + R1",
        // `R1.2` stays free -- a single point is no net, so it is absent.
        vec![vec!["R1.1".to_string(), "VCC".to_string()]],
    );
}

/// `TP1 + R1` -- two bodies, so this is the **body pair** rule's case. It runs
/// first and must keep winning: the new predicate must not shadow E2908, and
/// the body rule must not be reported as a net crossnet.
#[test]
fn net_pair__body_pair_rule_still_wins_for_two_bodies() {
    let (codes, nets) = build("        TP1 + R1", "/mcc/net-pair-body-counters.mc");
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_INST_PORTCOUNT_PLUSMINUS),
        "two bodies stay E2908's case; got {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_NET_CROSSNET),
        "a body pair is not a net crossnet; got {codes:?}"
    );
    assert!(
        nets.is_empty(),
        "the rejected pair wires nothing; got {nets:?}"
    );
}

/// A four-pin part, so a range operand is available.
const QUAD: &str =
    "component QUAD {\n    pins = [\n        1 = 1\n        2 = 2\n        3 = 3\n        4 = 4\n    ]\n}\n";

/// `t[1:4] + S[1:4]` -- an **instance pin range**, not a net name.
///
/// The phrase layer keeps a range as a flat name (`Bus { name: "t[1:4]" }`),
/// which is shape-identical to a free net of that spelling, so the name alone
/// cannot decide whether the operand is a body. The head identifier can: `t`
/// and `S` are declared instances, so both operands are bodies of four ports
/// each, and §5.4 calls two equal-port bodies legal (the E2908 body-pair
/// rule's own `R1 + R2` row).
///
/// This cell exists because the head check was **missing** and the rule fired
/// here: the live board `tc275/tc275knl.mc` writes
/// `K[1:44] <- (t275[1:44] + S1[1:44])` four times, and all four went red.
#[test]
fn net_pair__instance_pin_range_is_a_body_not_a_net() {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{QUAD}module main {{\n    QUAD t\n    QUAD S\n    func M() {{\n        \
         t[1:4] + S[1:4]\n    }}\n}}\n"
    );
    let u = McURI::from("/mcc/net-pair-range.mc");
    mcc::mcc_load_from_string(&u, &src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    assert!(
        !codes.contains(&mcc::errcodes::CONN_NET_CROSSNET),
        "an instance pin range is a body, not a net name; got {codes:?}"
    );
}
