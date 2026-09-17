// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! param-prefix §9.2 + the def-space receiver judgment: the `=>` prefix
//! receiver may be an **already-declared instance**, and whether the receiver
//! is an instance or a construction is decided in the **def space**, never by
//! the spelling.
//!
//! ```text
//! RES r1(10)
//! A => r1.Two(_, VDD)        ≡ r1.Two(A, VDD)      -- one component: r1
//! A => RES(10).Two(_, VDD)   ≡ RES(10).Two(A, VDD) -- one component: a new one
//! ```
//!
//! The withdrawn rule asked whether the receiver's **class name started with an
//! uppercase letter**, which is a statement about spelling: it called the
//! uppercase instance `R1` a construction (synthesizing `R1(..)`, so the
//! declared `R1` stayed unwired) and the lowercase class `res` an instance (so
//! `res(10)` pinned onto a non-existent instance and nothing was constructed).
//! The load-bearing assertions below are therefore the **component set** and
//! the **pin wiring**, not the shape of any one path — a spelling-driven engine
//! gets one of the two directions wrong whichever way it guesses.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// Two-pin resistor whose `Two` body wires `n1 - this - n2`. `Two` declares two
/// scalar network formals so the fold's two actuals (`A`, `VDD`) bind one-to-
/// one; the `=>` prefix redeems the single `_` in place (param-prefix §9.7).
const RES: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Two(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

/// The lowercase twin, for the other direction of the def-space judgment.
const RES_LOWER: &str = "component res(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Two(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

/// Module skeleton: the two nets the fold names, the declared receiver, and a
/// func whose body carries the folded statement.
fn src_of(component: &str, decl: &str, body: &str) -> String {
    format!(
        "{component}module main {{\n    io A\n    io VDD\n    {decl}\n    func M() {{\n{body}\n    }}\n}}\n"
    )
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped.
fn partition_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
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

/// Every diagnostic code emitted while building `src`, sorted and deduped.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Wiring codes (`4xxx`) only — a clean fold must not raise one. An unwired
/// receiver shows up here as E4112/E4116 ("0 of 2 pins connected").
fn wiring_codes_of(src: &str, uri: &str) -> Vec<u32> {
    codes_of(src, uri)
        .into_iter()
        .filter(|c| (4000..5000).contains(c))
        .collect()
}

/// The **component** heads in the partition: a point whose last segment is a
/// numeric pin (`r1.1`, `_res1.2`), which excludes the module port paths
/// (`A`, `VDD`) — those name the nets, not the components.
fn instance_heads(parts: &[Vec<String>]) -> BTreeSet<String> {
    parts
        .iter()
        .flatten()
        .filter_map(|p| {
            let (head, pin) = p.rsplit_once('.')?;
            pin.parse::<u32>().ok()?;
            Some(head.to_string())
        })
        .collect()
}

/// The net holding `name`.
fn net_with<'a>(parts: &'a [Vec<String>], name: &str) -> &'a Vec<String> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p == name))
        .unwrap_or_else(|| panic!("no net holds '{name}'; nets={parts:?}"))
}

/// Func `M`'s parsed stmts.
fn func_m_stmts(src: &str, uri: &str) -> Vec<mcc::McPhrase> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let inst = mcc::mcc_build(&McIds::from("main"), &u).expect("build");
    inst.def
        .funcs
        .find("M")
        .map(|f| f.stmts.clone())
        .unwrap_or_default()
}

/// Walk phrases to find the first FuncCall with `func_name`.
fn find_funccall<'a>(stmts: &'a [mcc::McPhrase], name: &str) -> Option<&'a mcc::McFuncCall> {
    fn walk<'a>(phrase: &'a mcc::McPhrase, name: &str) -> Option<&'a mcc::McFuncCall> {
        match phrase {
            mcc::McPhrase::FuncCall(f) => {
                if f.func_name.to_string() == name {
                    return Some(f);
                }
                f.caller.as_ref().and_then(|c| walk(c, name))
            }
            mcc::McPhrase::Series(elems, _) => elems.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Parallel(v) | mcc::McPhrase::Multiple(v) => {
                v.iter().find_map(|e| walk(e, name))
            }
            mcc::McPhrase::Group(g) => g.opds.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Transposed(inner) => walk(inner, name),
            mcc::McPhrase::Reversed(inner) => walk(inner, name),
            mcc::McPhrase::Member(p, _) => walk(p, name),
            mcc::McPhrase::Closure(c) => c.body.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Lead | mcc::McPhrase::Endpoint(_) => None,
        }
    }
    stmts.iter().find_map(|s| walk(s, name))
}

/// §9.2: the receiver is a **declared instance** — the fold lands on it and
/// constructs nothing. `r1` must carry both pins (the withdrawn shape built a
/// second, anonymous `RES` and left `r1` unwired).
#[test]
fn prefix_instance_receiver__folds_onto_the_declared_instance() {
    let src = src_of(RES, "RES r1(10)", "        A => r1.Two(_, VDD)");
    let parts = partition_of(&src, "/mcc/prefix-instance-receiver.mc");

    assert_eq!(
        instance_heads(&parts),
        BTreeSet::from(["r1".to_string()]),
        "the fold must land on the declared instance and construct nothing; nets={parts:?}"
    );
    assert!(
        net_with(&parts, "A").contains(&"r1.1".to_string()),
        "the prefix must reach r1's pin 1; nets={parts:?}"
    );
    assert!(
        net_with(&parts, "VDD").contains(&"r1.2".to_string()),
        "the written rail must reach r1's pin 2; nets={parts:?}"
    );
    assert_eq!(
        wiring_codes_of(&src, "/mcc/prefix-instance-receiver.mc"),
        Vec::<u32>::new(),
        "the fold must not raise a wiring code"
    );
}

/// §9.2 + the def-space judgment: an instance whose **name** starts uppercase is
/// still an instance. The withdrawn rule read the receiver's spelling as a class
/// construction here, synthesized `R1(..)` and left the declared `R1` unwired.
#[test]
fn prefix_instance_receiver__uppercase_name_is_still_an_instance() {
    let src = src_of(RES, "RES R1(10)", "        A => R1.Two(_, VDD)");
    let parts = partition_of(&src, "/mcc/prefix-instance-uppercase.mc");

    assert_eq!(
        instance_heads(&parts),
        BTreeSet::from(["R1".to_string()]),
        "an uppercase instance name must not be read as a construction; nets={parts:?}"
    );
    assert!(
        net_with(&parts, "A").contains(&"R1.1".to_string()),
        "the prefix must reach R1's pin 1; nets={parts:?}"
    );
    assert_eq!(
        wiring_codes_of(&src, "/mcc/prefix-instance-uppercase.mc"),
        Vec::<u32>::new(),
        "the fold must not raise a wiring code"
    );
}

/// The other direction: a class **spelled in lowercase** is still a
/// construction. The withdrawn rule read `res(10)` as an instance receiver, so
/// the receiver never resolved and no component was built at all.
#[test]
fn prefix_instance_receiver__lowercase_class_is_still_a_construction() {
    let src = src_of(RES_LOWER, "", "        A => res(10).Two(_, VDD)");
    let parts = partition_of(&src, "/mcc/prefix-instance-lowercase.mc");

    let heads = instance_heads(&parts);
    assert_eq!(
        heads.len(),
        1,
        "exactly one constructed component; heads={heads:?} (nets={parts:?})"
    );
    let head = heads.iter().next().unwrap();
    assert!(
        head.starts_with("_res"),
        "the construction is anonymous and class-named; heads={heads:?}"
    );
    assert!(
        net_with(&parts, "A").contains(&format!("{head}.1")),
        "the prefix must reach the construction's pin 1; nets={parts:?}"
    );
    assert_eq!(
        wiring_codes_of(&src, "/mcc/prefix-instance-lowercase.mc"),
        Vec::<u32>::new(),
        "the fold must not raise a wiring code"
    );
}

/// The judgment itself, as the parsed call records it: the construction
/// receiver is flagged, the declared-instance receiver is not — and the
/// rendered form follows, so a chain receiver never loses its own call.
#[test]
fn prefix_instance_receiver__records_the_def_space_judgment() {
    let ctor = func_m_stmts(
        &src_of(RES, "", "        A => RES(10).Two(_, VDD)"),
        "/mcc/prefix-instance-ctor.mc",
    );
    let fc = find_funccall(&ctor, "Two").expect("folded call");
    assert!(
        fc.receiver_is_ctor,
        "`RES(10)` is a construction (a def-space hit); stmts={ctor:?}"
    );

    let inst = func_m_stmts(
        &src_of(RES, "RES r1(10)", "        A => r1.Two(_, VDD)"),
        "/mcc/prefix-instance-hit.mc",
    );
    let fc = find_funccall(&inst, "Two").expect("folded call");
    assert!(
        !fc.receiver_is_ctor,
        "`r1` is a declared instance, not a construction; stmts={inst:?}"
    );
    let rendered: Vec<String> = inst.iter().map(|s| format!("{s}")).collect();
    assert!(
        rendered.iter().any(|s| s.contains("r1.Two(A, VDD)")),
        "the instance receiver must render as written, not flattened; rendered={rendered:?}"
    );
}
