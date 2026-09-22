// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! A differential pair is **declared** by tagging interface member rows with
//! `@pair(group)` (diff-pair-design.md, ruled 2026-09-23; was U61's
//! `diff_pair = [P, N]` body key, now retired), never read off a net name.
//!
//! Locks the whole chain from the declaration to the idiom the drawing side
//! consumes: the interface's two tagged member rows → `MemberInfo.diff` on the
//! port members → `NetAttrMirror.diff` on the nets born at those legs →
//! `idiom::analyze` pairing them. Both legs of one declaration carry the same
//! `group` (the port's flat path) and opposite `positive`, and that shared
//! value is the whole pairing rule. The leg order is member order — the
//! language declares no polarity, the first member is the derived leg A.
//!
//! The two specimens divide the claim: the declared one is named nothing like a
//! pair (so a pass cannot come from the spelling), and the undeclared one is
//! named exactly like one (so a pass would have to come from the spelling).
//! Before U61 the second specimen was the detection's only criterion.
//!
//! The definition-side gates live here too: a group with other than two legs
//! is E5512, and the retired `diff_pair` body key is E5513.

use crate::common;

use mcc::vector::graph::{McVecGraph, VizNet};
use mcc::viz::idiom::{analyze, IdiomKind};
use mcc::{McIds, McURI};

/// An interface that declares its two legs, and a module whose port is
/// declared with that interface, its two legs wired to two separate loads.
/// The leg names carry no power/pair vocabulary: the shared `@pair` tag names
/// them, nothing else could.
const DECLARED: &str = r#"
interface DIFF(role) {
    pins = [ 1 = P @pair(p); 2 = N @pair(p) ]
    role Receiver {
        name = "Differential receiver"
    }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// The corpus spelling: the port writes no member names of its own, so its
/// members come from the interface body (`io [16, 17] = ADC::ADC.DIFF(Receiver)`,
/// `tests/fixtures/hbl/src/us513.mc`). The declaration tags its rows by those
/// same names, so the pair still lands.
const MEMBERS_FROM_INTERFACE: &str = r#"
interface DIFF(role) {
    pins = [ 1 = P @pair(p); 2 = N @pair(p) ]
    role Receiver {
        name = "Differential receiver"
    }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io adc::DIFF(Receiver)
    RCV u1
    RCV u2
    adc.P -> u1.1
    adc.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// Two nets spelled like a pair and nothing declaring them.
const SPELLED_ONLY: &str = r#"
component DRV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    DRV u1
    u1.1 -> DIO_MIC_P
    u1.2 -> DIO_MIC_N
}
"#;

/// A group with one leg is not a pair (E5512).
const ONE_LEG: &str = r#"
interface DIFF(role) {
    pins = [ 1 = P @pair(p); 2 = N ]
    role Receiver {
        name = "Differential receiver"
    }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// A group with three legs is not a pair (E5512).
const THREE_LEGS: &str = r#"
interface DIFF(role) {
    pins = [ 1 = P @pair(p); 2 = N @pair(p); 3 = M @pair(p) ]
    role Receiver {
        name = "Differential receiver"
    }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// The retired body key still written (E5513) — the tagged rows declare the
/// pair all the same, so the warning is the only verdict.
const RETIRED_KEY: &str = r#"
interface DIFF(role) {
    diff_pair = [P, N]
    pins = [ 1 = P @pair(p); 2 = N @pair(p) ]
    role Receiver {
        name = "Differential receiver"
    }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// Escaped connector spellings (`D\+` / `D\-`, `mcode/ifs/usb.mc`): the tag
/// pairs the rows whatever the names spell, and the stored name equals the
/// member name the nets were born from.
const ESCAPED_NAMES: &str = r#"
interface DIFF(role) {
    pins = [ 1 = D\+ @pair(d); 2 = D\- @pair(d) ]
    role Receiver {
        name = "Differential receiver"
    }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{D\+, D\-}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.D\+ -> u1.1
    d.D\- -> u2.1
    u1.2 -> u2.2
}
"#;

fn graph_of(src: &str) -> McVecGraph {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/declared-diff-pair.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (inst, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &uri, 1).expect("flat build");
    let block = mcc::vector::builder::visit::build_mc_vec(&inst, &table, &arena, &store);
    mcc::vector::graph::fromblock::build_mc_vec_graph(&block, &table)
}

/// Non-benign diagnostic codes of one build (the same shape the connect-rule
/// locks use): unused-param family codes are noise here.
fn codes_of(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/declared-diff-pair.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat_with_arena(&McIds::from("main"), &uri, 1);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !matches!(c, 5641 | 5642 | 5643 | 5054))
        .collect();
    codes.sort_unstable();
    codes
}

fn all_layers<'a>(g: &'a McVecGraph, out: &mut Vec<&'a McVecGraph>) {
    out.push(g);
    for s in &g.sub_graphs {
        all_layers(s, out);
    }
}

/// Every net in the graph that was born at a declared differential leg.
fn nets_with_faces(g: &McVecGraph) -> Vec<(&VizNet, &mcc::vector::model::DiffFace)> {
    let mut layers = Vec::new();
    all_layers(g, &mut layers);
    layers
        .iter()
        .flat_map(|l| l.nets.iter())
        .filter_map(|n| {
            n.attr
                .as_ref()
                .and_then(|a| a.diff.as_ref())
                .map(|f| (n, f))
        })
        .collect()
}

#[test]
fn declared_faces_reach_the_nets_and_pair_up() {
    let graph = graph_of(DECLARED);

    let faces = nets_with_faces(&graph);
    assert_eq!(
        faces.len(),
        2,
        "one declaration names exactly two faces, got: {:?}",
        faces
            .iter()
            .map(|(n, f)| (n.name.as_str(), f))
            .collect::<Vec<_>>()
    );

    let (net_a, face_a) = faces[0];
    let (net_b, face_b) = faces[1];
    assert_eq!(
        face_a.group, face_b.group,
        "both faces of one declaration share the port's flat path as their group"
    );
    assert_ne!(
        face_a.positive, face_b.positive,
        "the leg order is member order: one face is leg A, the other is not"
    );
    assert_eq!(
        (net_a.name.as_str(), net_b.name.as_str()),
        if face_a.positive {
            ("d.P", "d.N")
        } else {
            ("d.N", "d.P")
        },
        "the member the declaration tags first is the leg A face"
    );

    // The idiom layer pairs the two nets by the declaration alone.
    let mut layers = Vec::new();
    all_layers(&graph, &mut layers);
    let pairs: Vec<_> = layers
        .iter()
        .flat_map(|l| analyze(l))
        .filter(|m| m.kind == IdiomKind::DiffPair)
        .collect();
    assert_eq!(
        pairs.len(),
        1,
        "the two declared faces form exactly one pair, got: {pairs:?}"
    );
    assert_eq!(pairs[0].member_box_ids.len(), 2);
}

#[test]
fn faces_land_when_the_port_writes_no_member_names() {
    let graph = graph_of(MEMBERS_FROM_INTERFACE);

    let faces = nets_with_faces(&graph);
    assert_eq!(
        faces.len(),
        2,
        "the interface body's own pin names carry the declaration, got: {:?}",
        faces
            .iter()
            .map(|(n, f)| (n.name.as_str(), f))
            .collect::<Vec<_>>()
    );
    assert_eq!(faces[0].1.group, faces[1].1.group);
    assert_ne!(faces[0].1.positive, faces[1].1.positive);
}

#[test]
fn pair_spelled_p_n_without_a_declaration_is_not_a_pair() {
    let graph = graph_of(SPELLED_ONLY);

    assert!(
        nets_with_faces(&graph).is_empty(),
        "a net name is not a declaration"
    );

    let mut layers = Vec::new();
    all_layers(&graph, &mut layers);
    assert!(
        layers
            .iter()
            .flat_map(|l| analyze(l))
            .all(|m| m.kind != IdiomKind::DiffPair),
        "DIO_MIC_P / DIO_MIC_N declare nothing, so they pair with nothing"
    );
}

#[test]
fn one_member_group_is_e5512() {
    assert!(
        codes_of(ONE_LEG).contains(&5512),
        "a one-leg @pair group is a pair-width disease"
    );
}

#[test]
fn three_member_group_is_e5512() {
    assert!(
        codes_of(THREE_LEGS).contains(&5512),
        "a three-leg @pair group is a pair-width disease"
    );
}

#[test]
fn retired_diff_pair_key_warns_e5513() {
    assert!(
        codes_of(RETIRED_KEY).contains(&5513),
        "the retired diff_pair body key must warn, not degrade silently"
    );
    // And the tagged rows still declare the pair — the warning is the only
    // verdict, the declaration is not lost.
    let graph = graph_of(RETIRED_KEY);
    assert_eq!(nets_with_faces(&graph).len(), 2);
}

#[test]
fn escaped_connector_names_pair_through_the_tag() {
    let graph = graph_of(ESCAPED_NAMES);

    let faces = nets_with_faces(&graph);
    assert_eq!(
        faces.len(),
        2,
        "the escaped connector names are ordinary member names to the tag, \
         got: {:?}",
        faces
            .iter()
            .map(|(n, f)| (n.name.as_str(), f))
            .collect::<Vec<_>>()
    );
    assert_eq!(faces[0].1.group, faces[1].1.group);
}
