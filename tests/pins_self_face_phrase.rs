// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `pins` self face in a method body: `pins.N` is the same face as
//! `this.N`, in a connection line as well as in a call argument.
//!
//! `MCAST_OPD_PINS` (54) is its own node type, and `McPhrase::new` answers it
//! through the same arm as `MCAST_OPD_THIS`. The attribute survey recorded a
//! split behaviour instead -- live in a call argument, a dead arm (E4009
//! `PHRASE_AST_TYPE_UNEXPECTED`) in a connection line. These cases pin the
//! single behaviour: one partition, one diagnostic code, whichever spelling.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate.
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Three pins and a method whose body spells the self face one way or another.
const TEMPLATE: &str = r#"component QUAD {
    pins = [
        1 = P1
        2 = P2
        3 = P3
    ]

    func Link([a, b]) {
{{BODY}}
    }
}

module main {
    Q1::QUAD().Link([N1, N2])
}
"#;

fn src_of(body: &str) -> String {
    TEMPLATE.replace("{{BODY}}", body)
}

/// A method body from its connection lines, at the template's indentation.
fn body_of(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|l| format!("        {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same body with the self face spelled `pins` and spelled `this`.
fn body_pair_lines(pins_lines: [&str; 2], this_lines: [&str; 2]) -> (String, String) {
    (body_of(&pins_lines), body_of(&this_lines))
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net names dropped, so two spellings of the same wiring compare equal.
fn nets_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    common::load_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
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
    parts
}

/// Every diagnostic code emitted while building `src`.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    common::load_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn net_holding<'a>(parts: &'a [Vec<String>], needle: &str) -> Option<&'a Vec<String>> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains(needle)))
}

/// The two spellings wire the same nets: the self face is one face.
#[test]
fn pins_selfface__same_wiring_as_this() {
    let (pins_body, this_body) =
        body_pair_lines(["pins.1 -> a", "b -> pins.3"], ["this.1 -> a", "b -> this.3"]);
    let pins = nets_of(&src_of(&pins_body), "/mcc/pins-selfface.mc");
    let this = nets_of(&src_of(&this_body), "/mcc/this-selfface.mc");

    assert_eq!(pins, this, "`pins.N` must wire exactly as `this.N`");
    assert_eq!(pins.len(), 2, "one net per connected pin; nets={pins:?}");

    let first = net_holding(&pins, "Q1.1").expect("the first line landed a pin");
    assert!(
        first.iter().any(|p| p.contains("N1")),
        "the first line must reach its argument; net={first:?}"
    );
    let third = net_holding(&pins, "Q1.3").expect("the second line landed a pin");
    assert!(
        third.iter().any(|p| p.contains("N2")),
        "the second line must reach its argument; net={third:?}"
    );
}

/// Every spelling of the self face resolves in a connection line, and each
/// resolves exactly as its `this` twin: the spelling is not what decides
/// whether the phrase reaches `McPhrase::new`'s self-face arm.
#[test]
fn pins_selfface__every_spelling_resolves_like_this() {
    let cases = [
        body_pair_lines(["pins.1 -> a", "b -> pins.3"], ["this.1 -> a", "b -> this.3"]),
        body_pair_lines(
            ["pins.P1 -> a", "b -> pins.P3"],
            ["this.P1 -> a", "b -> this.P3"],
        ),
        body_pair_lines(["pins{1} -> a", "b -> pins{3}"], ["this{1} -> a", "b -> this{3}"]),
        body_pair_lines(
            ["pins{1:2} -> [a, b]", ""],
            ["this{1:2} -> [a, b]", ""],
        ),
    ];

    for (pins_body, this_body) in cases {
        let pins = codes_of(&src_of(&pins_body), "/mcc/pins-selfface-spelling.mc");
        let this = codes_of(&src_of(&this_body), "/mcc/this-selfface-spelling.mc");

        assert!(
            !pins.contains(&mcc::errcodes::PHRASE_AST_TYPE_UNEXPECTED),
            "`{}` must resolve, not report E4009; codes: {pins:?}",
            pins_body.trim()
        );
        assert_eq!(
            pins, this,
            "`pins` must report the same codes as `this` for the same body"
        );
    }
}

/// A member the component lacks reports the same code under either spelling,
/// which is what makes the two faces one face: the `pins` spelling is not the
/// one that fails earlier or differently.
#[test]
fn pins_selfface__bad_member_reports_the_same_code_as_this() {
    let (bad_pins, bad_this) = body_pair_lines(
        ["pins.9 -> a", "b -> pins.3"],
        ["this.9 -> a", "b -> this.3"],
    );
    let pins = codes_of(&src_of(&bad_pins), "/mcc/pins-selfface-missing.mc");
    let this = codes_of(&src_of(&bad_this), "/mcc/this-selfface-missing.mc");

    assert!(
        pins.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "an absent pin number must report the pin-not-found code; codes: {pins:?}"
    );
    assert_eq!(
        pins, this,
        "the two spellings must report the same code set for the same miss"
    );
}
