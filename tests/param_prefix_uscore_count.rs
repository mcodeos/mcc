// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! param-prefix §4 / §9.7: **the `=>` prefix redeems exactly one `_`**.
//!
//! The prefix is ONE operand, so the right-hand actual list must hold exactly
//! one `_` placeholder. Zero placeholders (including the empty list `f()`) and
//! two or more are both E4176 — the prefix is never silently prepended, and a
//! vector prefix does not spread across slots (`[A,B] => f(_, _)` is not a
//! spelling). The `_` may sit anywhere in the list: `.Two(VDD, _)` folds to
//! `.Two(VDD, A)`.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

/// Two-pin resistor with two call faces over the same body:
/// `Pullup` takes one **index** formal (`.Pullup(_)` consumes a whole vector,
/// arity = 1, terminal-formal-design §15.6); `Two` takes two **scalar** formals.
const RES: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n    func Two(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

const HEAD: &str = "module main {\n    io A\n    io B\n    io VDD\n    func M() {\n";

fn src_of(body: &str) -> String {
    format!("{RES}{HEAD}{body}\n    }}\n}}\n")
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped. Point paths keep their instance prefix.
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

/// Wiring codes (`4xxx`) only — a fold that binds cleanly must not raise one.
fn wiring_codes_of(src: &str, uri: &str) -> Vec<u32> {
    codes_of(src, uri)
        .into_iter()
        .filter(|c| (4000..5000).contains(c))
        .collect()
}

/// The auto-instance heads (`_RES*`) appearing in the partition's point paths.
fn instance_count(parts: &[Vec<String>]) -> usize {
    let mut heads: Vec<&str> = parts
        .iter()
        .flatten()
        .filter_map(|p| p.rsplit_once('.').map(|(head, _)| head))
        .filter(|head| head.starts_with('_'))
        .collect();
    heads.sort_unstable();
    heads.dedup();
    heads.len()
}

/// Exactly one `_` — the canonical fold. `[A, B]` is ONE actual for the index
/// formal, so the call wires once and reports no wiring code.
#[test]
fn prefix_uscore__exactly_one_folds() {
    let body = "        [A, B] => RES(10).Pullup(_)";
    let parts = partition_of(&src_of(body), "/mcc/prefix-uscore-one.mc");
    assert_eq!(
        instance_count(&parts),
        1,
        "the unique `_` must fold into one wired call; nets={parts:?}"
    );
    assert_eq!(
        wiring_codes_of(&src_of(body), "/mcc/prefix-uscore-one.mc"),
        Vec::<u32>::new(),
        "the fold must not raise a wiring code"
    );
}

/// The `_` need not be the first actual: `.Two(VDD, _)` + `A` folds to
/// `.Two(VDD, A)`.
#[test]
fn prefix_uscore__placeholder_need_not_be_first() {
    let body = "        A => RES(10).Two(VDD, _)";
    let parts = partition_of(&src_of(body), "/mcc/prefix-uscore-second.mc");
    assert_eq!(
        instance_count(&parts),
        1,
        "the trailing `_` must be the one redeemed; nets={parts:?}"
    );
    assert_eq!(
        wiring_codes_of(&src_of(body), "/mcc/prefix-uscore-second.mc"),
        Vec::<u32>::new(),
        "the fold must not raise a wiring code"
    );
}

/// Zero placeholders — the old silent prepend. `f()` and a fully-written list
/// are both E4176, and nothing is wired.
#[test]
fn prefix_uscore__zero_placeholders_reports_e4176() {
    for body in [
        "        [A, B] => RES(10).Pullup()",
        "        A => RES(10).Two(VDD, B)",
    ] {
        let codes = codes_of(&src_of(body), "/mcc/prefix-uscore-zero.mc");
        assert!(
            codes.contains(&4176),
            "a prefix against a list with no `_` must report E4176; body={body:?} codes={codes:?}"
        );
        assert_eq!(
            partition_of(&src_of(body), "/mcc/prefix-uscore-zero.mc"),
            Vec::<Vec<String>>::new(),
            "and must not wire anything; body={body:?}"
        );
    }
}

/// Two or more placeholders — `[A,B] => f(_, _)` is **not** a spelling. The
/// vector prefix is one actual, so it cannot redeem two slots; E4176, nothing
/// wired. (The old all-placeholder branch folded `[A,B]` into the first slot
/// and ignored the rest, silently.)
#[test]
fn prefix_uscore__two_or_more_placeholders_report_e4176() {
    for body in [
        "        [A, B] => RES(10).Two(_, _)",
        "        [A, B] => RES(10).Two(_, _, _)",
        "        A => RES(10).Two(_, _)",
    ] {
        let codes = codes_of(&src_of(body), "/mcc/prefix-uscore-many.mc");
        assert!(
            codes.contains(&4176),
            "a list with two-plus `_` must report E4176; body={body:?} codes={codes:?}"
        );
        assert_eq!(
            partition_of(&src_of(body), "/mcc/prefix-uscore-many.mc"),
            Vec::<Vec<String>>::new(),
            "and must not wire anything; body={body:?}"
        );
    }
}
