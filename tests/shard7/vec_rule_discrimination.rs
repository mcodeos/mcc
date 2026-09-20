// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Rule-discriminating fixtures (unified-core §5 item ⑥, methodology §1.5 E).
//!
//! A rule-discriminating fixture is an input on which two *candidate* rules
//! disagree, so the observed result reveals which rule the engine actually
//! follows. Keyword scans cannot find semantic-level drift -- both candidate
//! rules are order preserving and neither sorts -- so this class of fixture is
//! the only guard for such a branch.
//!
//! # The branch locked here: series pairing
//!
//! The **series pairing rule** (`vec-dianlu` §5.2, eval.md §11.3, interface
//! ruling of 2026-09-19): two equal-width operand faces are paired by a hard
//! **positional zip in written order** (`expand_match`) -- ordinal k on the
//! two sides is the same wire, and member names are never a matching
//! criterion. The rejected candidate pairs equal-named members across
//! different written orders. The two agree whenever the right face lists the
//! same member names in the same order, so the fixture writes the right face's
//! names **reversed** -- the one shape on which the rules diverge:
//!
//! ```text
//! u1.SPI{SCLK, MOSI} -> u2.SPI{MOSI, SCLK}
//! by name : SCLK<->SCLK  MOSI<->MOSI   => u1.1<->u2.2  u1.2<->u2.1
//! by pos  : first<->first second<->second => u1.1<->u2.1  u1.2<->u2.2
//! ```
//!
//! Under the positional law the reversed written order is a legal crossing:
//! names are labels, never a pairing criterion. E4052 is retired (design doc
//! section 6.1 D3, 2026-09-20 ruling), so the divergent cells assert quiet +
//! the positional partition itself - the partition is the lock.
//!
//! The three series operators (`-`, `->`, `<-`) share the pairing and differ
//! only in the internal connection direction (§5.2), so each arm gets its own
//! cell: an arm-specific mis-wiring is a separate code path even when the rule
//! is shared. Whole-interface operands (`u1.SPI -> u2.SPI`) are deliberately
//! **not** used -- those connect as a single interface-level point and never
//! reach per-member pairing -- so the cells select the members explicitly.
//!
//! # Deliberately not duplicated here
//!
//! Parallel folding and group expansion already have discriminating cells of
//! their own:
//!
//! * Parallel folding -- `vec_degenerate_side_face.rs` locks the face-side law
//!   (a degenerate operand sticks to its *written* side) against the rejected
//!   `opds[0]` candidate, by writing the same operands in both orders.
//! * Group expansion -- `vec_group_expansion_equivalence.rs` locks a group
//!   against the handwritten statements it must equal, which is exactly what a
//!   "treat the group as one broadcast operand" implementation would fail.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// A two-member bus device whose `SPI` interface lists its members
/// `SCLK, MOSI` in that order (`SCLK` is pin 1, `MOSI` pin 2).
const BUS_FWD: &str =
    "component BUS_FWD {\n    pins = [\n        io [1:2] = SPI{SCLK, MOSI}\n    ]\n}\n";

/// The same device with the members declared in the **opposite** order
/// (`MOSI` is pin 1, `SCLK` pin 2) -- the divergence input: a by-name pairing
/// would still match `SCLK` to `SCLK`, the positional zip pairs `SCLK` to the
/// right face's *first* member, `MOSI`.
const BUS_REV: &str =
    "component BUS_REV {\n    pins = [\n        io [1:2] = SPI{MOSI, SCLK}\n    ]\n}\n";

/// The positional partition, written once so every cell states the same law:
/// ordinal k on the two sides is the same wire, so `u1.1` lands on `u2.1` and
/// `u1.2` on `u2.2` regardless of the member names. A by-name pairing would
/// produce the cross `u1.1<->u2.2` / `u1.2<->u2.1`.
fn positional_partition() -> Vec<Vec<String>> {
    vec![
        vec!["u1.1".to_string(), "u2.1".to_string()],
        vec!["u1.2".to_string(), "u2.2".to_string()],
    ]
}

/// Codes that are build-info, not a verdict (same set the vector-oracle family
/// tolerates).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
}

/// Build `main` with the body statement `body` and return (non-benign codes
/// sorted, net partition).
///
/// The partition is normalized to a sorted list of sorted member lists: net
/// *names* are synthesized, so the claim is about the **grouping of points**.
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src =
        format!("{BUS_FWD}{BUS_REV}module main {{\n    BUS_FWD u1\n    BUS_REV u2\n{body}\n}}\n");
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

// one cell per series arm: written position decides, names never realign

/// `->` (series, left to right): the pairing is a positional zip in written
/// order; the reversed member names must not realign it, and the crossing is
/// legal (quiet - E4052 retired).
#[test]
fn pair__arrow_series_zip_by_written_position() {
    let (codes, nets) = build(
        "    u1.SPI{SCLK, MOSI} -> u2.SPI{MOSI, SCLK}",
        "/mcc/rd-pair-arrow.mc",
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "a crossed writing is legal under the positional law; got {codes:?}"
    );
    assert_eq!(
        nets,
        positional_partition(),
        "`->` must zip SPI members by written position, never by name; got {nets:?}"
    );
}

/// `-` (series, undirected): same operands, same pairing -- the undirected arm
/// shares the positional zip.
#[test]
fn pair__dash_series_zip_by_written_position() {
    let (codes, nets) = build(
        "    u1.SPI{SCLK, MOSI} - u2.SPI{MOSI, SCLK}",
        "/mcc/rd-pair-dash.mc",
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "a crossed writing is legal under the positional law; got {codes:?}"
    );
    assert_eq!(
        nets,
        positional_partition(),
        "`-` must zip SPI members by written position, never by name; got {nets:?}"
    );
}

/// `<-` (series, right to left): the direction flips but the pairing rule does
/// not -- an implementation that swapped the operands before zipping would
/// produce the cross partition and fail here.
#[test]
fn pair__back_arrow_series_zip_by_written_position() {
    let (codes, nets) = build(
        "    u1.SPI{SCLK, MOSI} <- u2.SPI{MOSI, SCLK}",
        "/mcc/rd-pair-back.mc",
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "a crossed writing is legal under the positional law; got {codes:?}"
    );
    assert_eq!(
        nets,
        positional_partition(),
        "`<-` must zip SPI members by written position, never by name; got {nets:?}"
    );
}

/// The aligned written order (`SCLK, MOSI` on both faces) is the correct way
/// to declare the connection: quiet. `u2` is BUS_REV (pin 1 = MOSI), so the
/// right face's `{SCLK, MOSI}` *selects* u2.2 then u2.1 -- selection reads
/// names, pairing reads position -- and the zip lands SCLK<->SCLK,
/// MOSI<->MOSI, which shows up as the cross partition in pin numbers.
#[test]
fn pair__aligned_written_order_is_quiet() {
    let (codes, nets) = build(
        "    u1.SPI{SCLK, MOSI} -> u2.SPI{SCLK, MOSI}",
        "/mcc/rd-pair-aligned.mc",
    );
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "aligned order is quiet; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["u1.1".to_string(), "u2.2".to_string()],
            vec!["u1.2".to_string(), "u2.1".to_string()],
        ],
        "selection reads names, pairing is positional over the selected lists; got {nets:?}"
    );
}
