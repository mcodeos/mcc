// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! A subscript glued onto a reserved word in a phrase is an error
//! (status-design.md §2.7 trap 1).
//!
//! `pins[3:5]`, `this[3:5]` and `u1.pins[3:5]` are ONE identifier: the lexer
//! keeps the subscript inside the token, so the keyword `pins` never exists and
//! the naming addresses nothing. Before PHRASE_RESERVED_WORD_SUBSCRIBED (4024)
//! such a phrase fell through to the ghost path — a phantom net / pin (E3179)
//! or plain silence.
//!
//! The criterion is the segment's lexical form read against the key registry
//! (`attr_keys::is_reserved`), never the resolved name table: `A[1]` — a name
//! an `A[1:2]` declaration flattens to — is a legal reference and stays silent.
//!
//! Position matters. The phrase positions (module body, method body,
//! module-level connection line) report. An argument table is not one of them:
//! `f([q1.pins[3], q1.pins.4])` expands the fused name as members and addresses
//! real pins, so it stays silent — this file locks that side too, so widening
//! the check to the argument table would show up as a failure rather than as a
//! quiet break of live code.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate.
#![allow(non_snake_case)]

use crate::common;

use mcc::{DiagnosticLevel, McIds};

/// The code under lock, named for the assertions below.
const SUB: u32 = mcc::errcodes::PHRASE_RESERVED_WORD_SUBSCRIBED;

/// Five named-numbered pins: a range has something to expand into.
const QUAD: &str = "component QUAD {\n    pins = [\n        1 = A\n        2 = B\n        3 = C\n        4 = D\n        5 = E\n    ]\n}\n";

/// The body spelling hangs off one method of a five-pin component.
fn body_src(line: &str) -> String {
    format!(
        "component QUAD {{\n    pins = [\n        1 = A\n        2 = B\n        3 = C\n        4 = D\n        5 = E\n    ]\n\n    func Link([x]) {{\n        {line}\n    }}\n}}\n\nmodule main {{\n    Q1::QUAD().Link(N9)\n}}\n"
    )
}

/// A module-body line needs a live instance to hang off.
fn module_body_src(line: &str) -> String {
    format!("{QUAD}\nmodule main {{\n    Q1::QUAD()\n    {line}\n}}\n")
}

/// A module-level connection line between two instances.
fn pair_src(line: &str) -> String {
    format!("{QUAD}\nmodule main {{\n    U1::QUAD()\n    U2::QUAD()\n    {line}\n}}\n")
}

/// One build of `src`: every `(code, level)` emitted, plus the net partition of
/// `main` (point paths sorted, net names dropped).
fn build_of(src: &str, uri: &str) -> (Vec<(u32, DiagnosticLevel)>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let (_, _, _, net_store) =
        mcc::mcc_build_with_nets(&McIds::from("main"), &uri.to_string()).expect("build");
    let diags: Vec<(u32, DiagnosticLevel)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.level))
        .collect();
    let mut parts: Vec<Vec<String>> = net_store
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
    parts.sort();
    (diags, parts)
}

fn count_of(diags: &[(u32, DiagnosticLevel)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// Does any point path hold `needle`?
fn wires(parts: &[Vec<String>], needle: &str) -> bool {
    parts.iter().any(|ps| ps.iter().any(|p| p.contains(needle)))
}

#[test]
fn phrase_subscribed__method_body_range_is_one_error() {
    let (diags, _) = build_of(&body_src("pins[3:5] -> x"), "/mcc/sub-body.mc");
    assert!(
        diags.contains(&(SUB, DiagnosticLevel::Error)),
        "`pins[3:5]` in a method body must be an error-level diagnostic; got {diags:?}"
    );
    assert_eq!(
        count_of(&diags, SUB),
        1,
        "the fused spelling is one fact and must be reported once; got {diags:?}"
    );
}

#[test]
fn phrase_subscribed__method_body_this_range_is_one_error() {
    let (diags, _) = build_of(&body_src("this[3:5] -> x"), "/mcc/sub-body-this.mc");
    assert!(
        diags.contains(&(SUB, DiagnosticLevel::Error)),
        "`this[3:5]` in a method body must be an error-level diagnostic; got {diags:?}"
    );
    assert_eq!(
        count_of(&diags, SUB),
        1,
        "`this[3:5]` fuses the same way and must be reported once; got {diags:?}"
    );
}

#[test]
fn phrase_subscribed__module_body_range_is_one_error() {
    let (diags, _) = build_of(&module_body_src("pins[3:5] -> N1"), "/mcc/sub-mod.mc");
    assert!(
        diags.contains(&(SUB, DiagnosticLevel::Error)),
        "`pins[3:5]` in a module body must be an error-level diagnostic; got {diags:?}"
    );
}

#[test]
fn phrase_subscribed__qualified_member_range_is_one_error() {
    let (diags, _) = build_of(
        &pair_src("U1.pins[3:5] -> U2{3:5}"),
        "/mcc/sub-qualified.mc",
    );
    assert!(
        diags.contains(&(SUB, DiagnosticLevel::Error)),
        "`U1.pins[3:5]` must be an error-level diagnostic; got {diags:?}"
    );
    assert_eq!(
        count_of(&diags, SUB),
        1,
        "the instance-qualified spelling is one fact; got {diags:?}"
    );
}

/// The complement: every spelling that does address real pins must stay quiet,
/// so the new code cannot rot into a net over legal names.
#[test]
fn phrase_subscribed__legal_spellings_stay_silent() {
    let cases: [(&str, String); 7] = [
        ("dot member", pair_src("U1.pins.3 -> N1")),
        ("curly on pins", pair_src("U1.pins{3:5} -> U2{3:5}")),
        ("bare curly", pair_src("U1{3:5} -> U2{3:5}")),
        ("bare pins word", pair_src("U1.pins -> N6")),
        ("this.pins member", body_src("this.pins.3 -> x")),
        ("curly on pins in body", body_src("pins{3:5} -> x")),
        ("curly on this in body", body_src("this{3:5} -> x")),
    ];
    for (label, src) in cases {
        let (diags, _) = build_of(&src, "/mcc/sub-legal.mc");
        assert_eq!(
            count_of(&diags, SUB),
            0,
            "{label}: a legal spelling must not be reported; got {diags:?}"
        );
    }
}

/// `A[1]` is a name an `A[1:2]` pin declaration flattens to, so the segment
/// carries a subscript while the word is not reserved. Silence here is the
/// point of judging the lexical form against the registry instead of walking a
/// word list: a widened check would report every flattened member name.
#[test]
fn phrase_subscribed__flattened_member_name_stays_silent() {
    let src = "component RANGE {\n    pins = [\n        [1:2] = A[1:2]\n    ]\n}\n\nmodule main {\n    R1::RANGE()\n    R1.A[1] -> N1\n}\n";
    let (diags, _) = build_of(src, "/mcc/sub-flat.mc");
    assert_eq!(
        count_of(&diags, SUB),
        0,
        "a flattened member name is a legal reference; got {diags:?}"
    );
}

/// An argument table expands the fused spelling as members, so `pins[3]` there
/// reaches a real pin. Silence is therefore correct, and the wiring is asserted
/// beside it so this row cannot pass by the phrase having been dropped.
#[test]
fn phrase_subscribed__argument_table_spelling_stays_silent() {
    let src = "component TWOPIN {\n    pins = [\n        1 = P1\n        2 = P2\n    ]\n\n    func Link([a, b]) {\n        a -> b\n    }\n}\n\ncomponent QUAD {\n    pins = [\n        1 = A\n        2 = B\n        3 = C\n        4 = D\n        5 = E\n    ]\n}\n\nmodule main {\n    Q1::QUAD()\n    T1::TWOPIN()\n    T1.Link([Q1.pins[3], Q1.pins.4])\n}\n";
    let (diags, parts) = build_of(src, "/mcc/sub-arg.mc");
    assert_eq!(
        count_of(&diags, SUB),
        0,
        "an argument table addresses members, not a ghost; got {diags:?}"
    );
    assert!(
        wires(&parts, "Q1.3") && wires(&parts, "Q1.4"),
        "the fused spellings must land on pin 3 and pin 4; got {parts:?}"
    );
}
