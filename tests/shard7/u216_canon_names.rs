// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the U216 pair: (a) the `canon` value primitive — one number → the
//! canonical rail text (one fixed decimal, the point read `V`, a negative
//! marked `N`), so a computed pin row spells an addressable, dot-free name;
//! (b) the numeric-tail instantiation clause — `K.7400 u1` shares the
//! declaration-side shape and instantiates like any other class reference.

#![allow(non_snake_case)]

use crate::common;

use mcc::{eval, McIds, McURI};

/// A DC-shaped family under the U216 naming law: computed rail names on the
/// sign branches, a generic name on the ELSE branch — which is also what the
/// declared family table (the boundary face, U141) spells.
const DC_LAW_FAMILY: &str = r#"
interface DC(volt)
{
    pins = [
        1 = VCC, "family table"
        2 = GND, "family table"
    ]
    if (volt < 0V)
        pins = [
            1 = "VCC" + canon(volt), "computed rail"
            2 = GND, "ground"
        ]
    else if (volt > 0V)
        pins = [
            1 = "VCC" + canon(volt), "computed rail"
            2 = GND, "ground"
        ]
}

component BRD
{
    pins = [
        1 = "V" + canon(3.3V), "computed name"
        2 = "V" + canon(-5V), "computed name"
    ]
}
"#;

/// Codes that are build-info, not a verdict (same set the shard7 family
/// tolerates).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
}

fn u216_build(body: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{DC_LAW_FAMILY}module main {{\n{body}\n}}\n");
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, &src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();
    codes
}

/// The primitive itself: fixed decimal, the point read `V`, the negative
/// marked `N`; anything that binds to no single number reads as empty text —
/// the concatenation then keeps the generic name.
#[test]
fn u216__canon_spells_the_canonical_rail_text() {
    let s = |v: eval::Value| match eval::canon(&v) {
        eval::Value::Str(t) => t,
        other => panic!("canon must return text, got {other:?}"),
    };
    assert_eq!(s(eval::Value::Float(3.3)), "3V3");
    assert_eq!(s(eval::Value::Float(5.0)), "5V0");
    assert_eq!(s(eval::Value::Float(0.0)), "0V0");
    assert_eq!(s(eval::Value::Float(-3.3)), "3V3N");
    assert_eq!(s(eval::Value::Float(12.0)), "12V0");
    assert_eq!(s(eval::Value::Undef), "");
    assert_eq!(s(eval::Value::Str("x".into())), "");
}

/// A computed pin row registers the evaluated name as a connectable pin —
/// the name is static text once the environment binds it, in declaration
/// order.
#[test]
fn u216__computed_pin_names_register_and_connect() {
    let codes = u216_build(
        "    BRD b1\n    b1.V3V3 -> b1.V5V0N",
        "/mcc/u216-computed-name-connect.mc",
    );
    assert!(
        codes.is_empty(),
        "both computed names must register; got {codes:?}"
    );
}

/// The numeric-tail instantiation clause: `K.7400 u1` — the third
/// declaration-side position to take the optional numeric tail, after
/// `mc_class_name` and `mc_iface_name`. Before the clause this was E2082.
#[test]
fn u216__numeric_tail_instantiation_is_quiet() {
    let _lock = common::lock();
    common::reset();
    let src = r#"
component K.7400
{
    pins = [
        1 = A, "gate in"
        2 = B, "gate out"
    ]
}

module main
{
    K.7400 u1
    u1.A -> u1.B
}
"#;
    let u = McURI::from("/mcc/u216-numeric-tail.mc");
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();
    assert!(
        !codes.contains(&2082),
        "the numeric-tail clause must instantiate; got {codes:?}"
    );
    assert!(codes.is_empty(), "quiet build expected; got {codes:?}");
}

/// The boundary face still spells the declared family table (U141 holds under
/// the naming law): an `io` port over the computed family pairs ordinals
/// under the ELSE-branch names, never the per-parameter rail names.
#[test]
fn u216__boundary_face_keeps_the_declared_family_table() {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{DC_LAW_FAMILY}module main {{\n    \
         io p::DC(3.3V)\n    io q::DC(-5V)\n    p.VCC -> q.VCC\n}}\n"
    );
    let u = McURI::from("/mcc/u216-boundary-family-table.mc");
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) =
        mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
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
    assert_eq!(
        partition,
        vec![vec!["p.VCC".to_string(), "q.VCC".to_string()]],
        "the boundary spells the declared table even across parameters, \
         never the per-parameter rail names; got {partition:?}"
    );
}
