// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// U343 B1 arm 1: the top-level DOT/CURLY/CURLY_MN operand reader in
// `McOpd::new` used to rebuild non-McIds chain elements from their Display
// text. A curly-MN value operand (`m{P | N}`) therefore collapsed into the
// flat name run `mPN` — both curly faces lost their member grouping — and the
// Display `_ =>` fallback could even paste `<node_type_N>` placeholders into
// the chain. The structural reader carries the faces as segments: base id,
// then one Curly group holding both sides' members (the value-face reading of
// the pipe boundary — the same ',' / '|' equivalence the ids-level reader
// applies), and rejects elements with no operand reading instead of
// inventing names.
//
// Fixtures cover every new reader branch: CURLY_MN base + sides, multi-member
// sides, `this{P | N}`, an `Ids` chain member (`uC{ADC.P | N}`, U249 shape), a
// Square member (`m{P | [N]}`), a dotted int tail (`m{P | N}.3`, DotInt), and
// the honest-reject face (`m{(P) | N}` — the group member has no operand
// reading, so the attr registers with no value rather than a fabricated one).

#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

fn setup(uri: &str, source: &str) {
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), source);
}

/// The registered `note` attribute, rendered (`note = <value>`).
fn first_note_value(uri: &str) -> String {
    let comp = mcc::get_component_def(&McIds::from("C"), &uri.to_string()).expect("C definition missing");
    let mcc::McCMIE::Component(comp) = comp else {
        panic!("C should resolve to a component");
    };
    let attr = comp
        .attrs
        .iter()
        .find(|a| a.id.to_string() == "note")
        .unwrap_or_else(|| panic!("attr 'note' missing in {:?}", comp.attrs.iter().map(|a| a.id.to_string()).collect::<Vec<_>>()));
    assert!(
        !attr.values.is_empty(),
        "attr 'note' lost its value entirely"
    );
    format!("{attr}")
}

#[test]
fn u343_b1__curly_mn_value_face_keeps_both_sides() {
    let _lock = common::lock();

    let uri = "/mcc/u343_b1_curly_mn.mc";
    let source = r#"
component C {
    pins = [
        P = 1
        N = 2
    ]
    note = m{P | N}
}
module main {
    C c1
}
"#;
    setup(uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri.to_string()).expect("build failed");

    // The pipe faces survive as one member group — not the flat `mPN` run
    // the Display-text fallback produced.
    assert_eq!(first_note_value(uri), "note = m{P, N}");
}

#[test]
fn u343_b1__curly_mn_multi_member_sides() {
    let _lock = common::lock();

    let uri = "/mcc/u343_b1_curly_mn_multi.mc";
    let source = r#"
component C {
    pins = [
        P = 1
        N = 2
    ]
    note = m{P, N | P, N}
}
module main {
    C c1
}
"#;
    setup(uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri.to_string()).expect("build failed");

    assert_eq!(first_note_value(uri), "note = m{P, N, P, N}");
}

#[test]
fn u343_b1__this_curly_mn_keeps_self_face_word() {
    let _lock = common::lock();

    let uri = "/mcc/u343_b1_this_curly_mn.mc";
    let source = r#"
component C {
    pins = [
        P = 1
        N = 2
    ]
    note = this{P | N}
}
module main {
    C c1
}
"#;
    setup(uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri.to_string()).expect("build failed");

    assert_eq!(first_note_value(uri), "note = this{P, N}");
}

#[test]
fn u343_b1__curly_mn_ids_chain_member_stays_one_member() {
    let _lock = common::lock();

    let uri = "/mcc/u343_b1_curly_mn_chain.mc";
    let source = r#"
component C {
    pins = [
        P = 1
        N = 2
    ]
    note = uC{ADC.P | N}
}
module main {
    C c1
}
"#;
    setup(uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri.to_string()).expect("build failed");

    // `ADC.P` stays one chain member (U249), not two.
    assert_eq!(first_note_value(uri), "note = uC{ADC.P, N}");
}

#[test]
fn u343_b1__curly_mn_square_member_carries_row() {
    let _lock = common::lock();

    let uri = "/mcc/u343_b1_curly_mn_square.mc";
    let source = r#"
component C {
    pins = [
        P = 1
        N = 2
    ]
    note = m{P | [N]}
}
module main {
    C c1
}
"#;
    setup(uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri.to_string()).expect("build failed");

    assert_eq!(first_note_value(uri), "note = m{P, [N]}");
}

#[test]
fn u343_b1__dot_int_tail_keeps_the_dot() {
    let _lock = common::lock();

    let uri = "/mcc/u343_b1_dot_int.mc";
    let source = r#"
component C {
    pins = [
        P = 1
        N = 2
    ]
    note = m{P | N}.3
}
module main {
    C c1
}
"#;
    setup(uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri.to_string()).expect("build failed");

    // The int tail is a DotInt — the dot survives (`m{P, N}.3`), it does not
    // glue onto the last name.
    assert_eq!(first_note_value(uri), "note = m{P, N}.3");
}

#[test]
fn u343_b1__unreadable_member_rejects_the_operand_instead_of_inventing_a_name() {
    let _lock = common::lock();

    let uri = "/mcc/u343_b1_unreadable_member.mc";
    let source = r#"
component C {
    pins = [
        P = 1
        N = 2
    ]
    note = m{(P) | N}
}
module main {
    C c1
}
"#;
    setup(uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri.to_string()).expect("build failed");

    // The parenthesized member has no operand reading: the whole operand is
    // rejected (attr registered with no value), not rebuilt from Display text.
    let comp = mcc::get_component_def(&McIds::from("C"), &uri.to_string()).expect("C definition missing");
    let mcc::McCMIE::Component(comp) = comp else {
        panic!("C should resolve to a component");
    };
    let attr = comp
        .attrs
        .iter()
        .find(|a| a.id.to_string() == "note")
        .expect("attr 'note' missing");
    assert!(
        attr.values.is_empty(),
        "unreadable member must not fabricate a value, got: {:?}",
        attr.values
    );
}
