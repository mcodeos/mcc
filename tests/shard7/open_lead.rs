// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U283 open-lead doctrine (mcd/doc/erc/open-lead-design.md): one anchor
//! census judges the whole circuit-completeness family. The judging unit is
//! the statement; every endpoint it constructs is anchored (declared pin /
//! port), explicitly relinquished (NC), or an anonymous `_` point. An
//! anonymous point gathering two or more anchored operands is an interior
//! splice — the star form `(A,B,C) -> _` — and stays silent; one gathering
//! fewer is a free end: no anchored operands at all is the floating wire,
//! some anchored plus a free end is the open lead. Anonymous wires never
//! merge across statements, so two statements take two warnings.
//!
//! The green cells lock faces that are already correct at today's HEAD (the
//! E5411 floating-wire warning, the star silences, the anchored negatives,
//! the name-disease split). The spec cells lock the coded rulings (①–③,
//! 2026-09-24, doctrine §6): the freshly minted open-lead warning E4065,
//! its per-statement counting, and the E4057 narrowing off the pure
//! placeholder face.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashSet;

/// Build `src` and return every emitted diagnostic code in order — a Vec, not
/// a set: the two-statement cell counts occurrences.
fn build_code_seq(src: &str) -> Vec<u32> {
    common::reset();
    let uri = "/mcc/open-lead-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

fn build_codes(src: &str) -> HashSet<u32> {
    build_code_seq(src).into_iter().collect()
}

/// The codes this doctrine owns or borders; a silent cell must emit none of
/// them (other families' codes are not this file's business).
fn family_noise(codes: &HashSet<u32>) -> Vec<u32> {
    use mcc::errcodes::{
        EXPR_PLACEHOLDER_ONLY, FLOATING_PLACEHOLDER, FUNC_FLOATING_LABEL, NET_DROPPED_STATEMENT,
        OPEN_LEAD,
    };
    [
        EXPR_PLACEHOLDER_ONLY,
        OPEN_LEAD,
        FLOATING_PLACEHOLDER,
        NET_DROPPED_STATEMENT,
        FUNC_FLOATING_LABEL,
    ]
    .into_iter()
    .filter(|c| codes.contains(c))
    .collect()
}

// Green cells: faces already correct at today's HEAD.

#[test]
fn open_lead__floating_wire_placeholder_only_warns_e5411() {
    let _lock = common::lock();

    // `_ -> _`: no anchored operand anywhere — the floating wire. E5411
    // already owns this face at warning tier (doctrine §5.1).
    let src = "module main {\n    _ -> _\n}";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::EXPR_PLACEHOLDER_ONLY),
        "E5411 not emitted for the floating wire; got codes: {codes:?}"
    );
}

#[test]
fn open_lead__star_splice_gathers_three_anchors_stays_silent() {
    let _lock = common::lock();

    // `(p, q, r) -> _`: the anonymous point gathers three anchored operands —
    // an interior splice, not a free end. Silent (doctrine §2).
    let src = "module main(p, q, r) {\n    (p, q, r) -> _\n}";
    let codes = build_codes(src);
    let noise = family_noise(&codes);
    assert!(
        noise.is_empty(),
        "star splice with three anchors must stay silent; got family codes: {noise:?}"
    );
}

#[test]
fn open_lead__star_splice_gathers_two_anchors_stays_silent() {
    let _lock = common::lock();

    // `(p, q) -> _`: two anchored operands meet at the anonymous point — p
    // and q joined through a splice. Silent (doctrine §2).
    let src = "module main(p, q) {\n    (p, q) -> _\n}";
    let codes = build_codes(src);
    let noise = family_noise(&codes);
    assert!(
        noise.is_empty(),
        "star splice with two anchors must stay silent; got family codes: {noise:?}"
    );
}

#[test]
fn open_lead__anchored_pair_is_complete_and_silent() {
    let _lock = common::lock();

    // Both ends anchored: the complete cell — the census finds nothing to
    // report (doctrine §2, zero free ends).
    let src = "module main(p, q) {\n    p -> q\n}";
    let codes = build_codes(src);
    let noise = family_noise(&codes);
    assert!(
        noise.is_empty(),
        "an anchored pair is complete and must stay silent; got family codes: {noise:?}"
    );
}

#[test]
fn open_lead__dc_nominal_anchor_is_complete_and_silent() {
    let _lock = common::lock();

    // A `::DC(...)` nominal is an anchored face (U227): both ends land on
    // declared terminals, so the census is clean (doctrine §5 negative).
    let src = "module main {\n    io p::DC(3.3V)\n    io q::DC(-5V)\n    p -> q\n}";
    let codes = build_codes(src);
    let noise = family_noise(&codes);
    assert!(
        noise.is_empty(),
        "::DC-anchored pair is complete and must stay silent; got family codes: {noise:?}"
    );
}

#[test]
fn open_lead__undeclared_endpoint_is_the_name_disease_e3136() {
    let _lock = common::lock();

    // `A -> _` with A declared nowhere: the name disease (E3136) — not the
    // open-lead disease. Both may coexist once the coding batch lands, but
    // the name half must never be swallowed (doctrine §5, dual-disease cell).
    let src = "module main {\n    A -> _\n}";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 not emitted for the undeclared endpoint; got codes: {codes:?}"
    );
}

#[test]
fn open_lead__nc_tail_note_stays_silent() {
    let _lock = common::lock();

    // The explicit-NC tail note (`@nc_pin`, nc-design §5) is a declaration,
    // not a disease: the clause stays silent here (doctrine §2, relinquished
    // bucket — the NC face's own tiers are nc-design's business).
    let src = "module main(p, q) {\n    p -> q @nc_pin\n}";
    let codes = build_codes(src);
    let noise = family_noise(&codes);
    assert!(
        noise.is_empty(),
        "an explicit-NC clause must stay silent; got family codes: {noise:?}"
    );
}

// Spec cells: the coded rulings (doctrine §6).

#[test]
fn open_lead__open_lead_one_anchor_one_free_end_warns() {
    let _lock = common::lock();

    // `p -> _` with p declared: one anchored end plus one free end — the open
    // lead. Silent at today's HEAD (the real gap, doctrine §4); the coding
    // batch gives it the fresh warning (ruling ②).
    let src = "module main(p) {\n    p -> _\n}";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::OPEN_LEAD),
        "open lead must warn with the freshly minted code; got codes: {codes:?}"
    );
}

#[test]
fn open_lead__two_statements_are_two_distinct_open_leads() {
    let _lock = common::lock();

    // Anonymous wires never merge across statements (doctrine §2 identity
    // law): `p -> _` and `q -> _` are two distinct anonymous wires and take
    // exactly two warnings — no cross-statement dedup.
    let src = "module main(p, q) {\n    p -> _\n    q -> _\n}";
    let n = build_code_seq(src)
        .into_iter()
        .filter(|c| *c == mcc::errcodes::OPEN_LEAD)
        .count();
    assert_eq!(
        n, 2,
        "two statements are two distinct open leads; got {n} open-lead warnings"
    );
}

#[test]
fn open_lead__pure_placeholder_face_has_no_dropped_statement_error() {
    let _lock = common::lock();

    // Ruling ①: a placeholder end is not a resolution failure — it has
    // nowhere to resolve to. E4057 narrows to named-endpoint resolution
    // failures; the pure placeholder face answers to E5411 alone.
    let src = "module main {\n    _ -> _\n}";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_DROPPED_STATEMENT),
        "E4057 must not fire on the pure placeholder face; got codes: {codes:?}"
    );
}
