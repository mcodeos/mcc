// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U131 — named actual arguments at func call sites (spec/10-funcs.md §7
//! items 10-12). A named actual `k: v` (the `k = v` twin spells the same
//! node) matches a formal by exact name; the remaining *positional* actuals
//! then fill the formals the named args did not claim, in declaration order.
//! The discriminative cells:
//!
//! * unknown name → E4176 (the name matches no formal on the func-call
//!   face, where no attribute keys are in scope);
//! * the same name twice → E4176 with its own word ("duplicate"), not the
//!   unknown-name reading the claimed-slot path used to give;
//! * positional overflow → E4176 arity (unchanged);
//! * a bracket vector formal's member names are not argument names
//!   (`Pullup(n1: …)` against `func Pullup([n1, n2])`) — one honest E4176
//!   instead of the width-gate E4180 + phantom-pin 3179 cascade a scalar
//!   bound to the whole vector used to produce;
//! * a single formal receiving a bracket set takes the *one* bracket
//!   argument whether named or positional — the named and positional twins
//!   are judged equal, not by the absence of codes but by an equal
//!   diagnostic multiset and an equal net partition;
//! * the receiver `this` is not in the argument table, so it is not
//!   nameable.
//!
//! Binding correctness is asserted through the real net partition
//! (`export::netlist::collect_nets`), not through the absence of codes —
//! a silent mis-assignment also reads as zero codes. The live vehicles are
//! an instance method (`c1.link(a, b)`) and a bare module-level func (both
//! reach `McParamBindings`); the gap1 `RES` shape gives a live bracket
//! formal.
#![allow(non_snake_case)]

use crate::common;

use mcc::export::netlist::{collect_nets, PointNaming};
use mcc::{McIds, McURI};
use std::collections::BTreeMap;

/// Component with a two-formal method; the body joins them with a plain
/// wire, so the net partition shows which actual landed on which formal.
/// The component's own pins stay unconnected — the 4112/4116/4119
/// unconnected-pin noise is orthogonal to every cell and filtered out.
const METHOD_CLASS: &str = "component C {\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    func link(a, b) {\n        a - b\n    }\n}\n";

/// gap1's live bracket-formal vehicle: `Pullup([n1, n2])` declares one
/// vector formal whose members are nameable inside the body.
const RES_CLASS: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Component with a single formal, for the one-bracket-argument cell.
const SINGLE_CLASS: &str = "component S {\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    func pull(net) {\n        net - this\n    }\n}\n";

fn load(uri: &str, src: &str) {
    common::reset();
    let owned: McURI = uri.to_string();
    mcc::mcc_load_from_string(&owned, src);
}

/// The full build's diagnostics as `(code, message)` pairs (sorted). The
/// message is part of the reading: the duplicate and vector-member cells
/// are judged by their own words, not by the shared code alone.
fn diags(src: &str) -> Vec<(u32, String)> {
    let _lock = common::lock();
    load("/mcc/u131.mc", src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &McURI::from("/mcc/u131.mc"), 1000)
        .expect("flat build");
    let mut ds: Vec<(u32, String)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    ds.sort();
    ds
}

/// Diagnostics minus the unconnected-pin noise the toy classes produce
/// (4112 no-pins / 4116 coverage / 4119 per-pin) — orthogonal to binding.
fn bind_diags(src: &str) -> Vec<(u32, String)> {
    diags(src)
        .into_iter()
        .filter(|(c, _)| !matches!(c, 4112 | 4116 | 4119))
        .collect()
}

/// The flat net partition as `net -> sorted pad list` (local naming).
fn nets(src: &str) -> BTreeMap<String, Vec<String>> {
    let _lock = common::lock();
    load("/mcc/u131.mc", src);
    let mut dl = mcc::mcc_build_dianlu(&McIds::from("main"), &McURI::from("/mcc/u131.mc"), 0)
        .expect("dianlu build");
    let _ = dl.flatten_with_prefix(None);
    let arena = dl.arena().clone();
    let store = dl.store().clone();
    let (tree, table) = dl.into_parts();
    let net_store = table.net_table();
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_nets(
        &tree,
        &arena,
        &store,
        &net_store.borrow(),
        PointNaming::Local,
        &mut out,
    );
    out.into_iter()
        .map(|(n, mut pts)| {
            pts.sort();
            (n, pts)
        })
        .collect()
}

fn board(call: &str) -> String {
    format!("{METHOD_CLASS}\nmodule main {{\n    C c1\n    {call}\n}}\n")
}

fn bare_module(body: &str) -> String {
    format!("module main {{\n    func f(a, b) {{\n        a - b\n    }}\n{body}\n}}\n")
}

/// E4176 messages of one build, filtered to the bind gate.
fn bind_errors(src: &str) -> Vec<String> {
    bind_diags(src)
        .into_iter()
        .filter(|(c, _)| *c == 4176)
        .map(|(_, m)| m)
        .collect()
}

/// `c1.link(N1, N2)` — both formals positional, in declaration order.
fn control() -> BTreeMap<String, Vec<String>> {
    nets(&board("c1.link(N1, N2)"))
}

#[test]
fn u131_method_named_then_positional_fills_declaration_order() {
    // `b` is named, `N1` is positional: N1 must land on `a` (the first
    // unclaimed formal in declaration order), so the body wire joins
    // N1-N2 exactly as the all-positional control does.
    let src = board("c1.link(b: N2, N1)");
    assert!(
        bind_errors(&src).is_empty(),
        "named+positional mix must bind cleanly: {:?}",
        bind_errors(&src)
    );
    assert_eq!(nets(&src), control());
}

#[test]
fn u131_written_order_is_immaterial() {
    // The named argument written last claims its formal first all the same:
    // `f(N1, b: N2)` and `f(b: N2, N1)` are one fact in two written orders.
    let src = board("c1.link(N1, b: N2)");
    assert!(bind_errors(&src).is_empty());
    assert_eq!(nets(&src), control());
}

#[test]
fn u131_colon_and_equal_spellings_are_one_fact() {
    let colon = board("c1.link(b: N2, N1)");
    let equal = board("c1.link(b = N2, N1)");
    assert_eq!(bind_diags(&colon), bind_diags(&equal));
    assert_eq!(nets(&colon), nets(&equal));
}

#[test]
fn u131_unknown_name_is_a_hard_error() {
    let src = board("c1.link(x: N1, N2)");
    let errs = bind_errors(&src);
    assert_eq!(errs.len(), 1, "one orphan name, one error: {errs:?}");
    assert!(errs[0].contains("Unknown parameter"), "{errs:?}");
    assert!(errs[0].contains('x'), "{errs:?}");
}

#[test]
fn u131_duplicate_name_has_its_own_word() {
    let src = board("c1.link(b: N1, b: N2)");
    let errs = bind_errors(&src);
    assert_eq!(errs.len(), 1, "{errs:?}");
    // The claimed-slot path used to report this as an *unknown* name —
    // the name is known, it is assigned twice.
    assert!(errs[0].contains("Duplicate parameter"), "{errs:?}");
    assert!(errs[0].contains('b'), "{errs:?}");
}

#[test]
fn u131_positional_overflow_stays_an_arity_error() {
    let src = board("c1.link(N1, N2, N3)");
    let errs = bind_errors(&src);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("Too many arguments"), "{errs:?}");
}

#[test]
fn u131_bare_func_call_binds_named_then_positional() {
    let control = bare_module("    f(N1, N2)\n");
    let named = bare_module("    f(b: N2, N1)\n");
    assert!(bind_errors(&named).is_empty(), "{:?}", bind_errors(&named));
    assert_eq!(nets(&named), nets(&control));
}

#[test]
fn u131_bracket_formal_member_is_not_an_argument_name() {
    // `n1`/`n2` are nameable inside the body, not at the call site: a named
    // argument supplies the one whole-formal value, and a Multiple formal
    // has no whole-formal name. One honest E4176 — not the E4180 +
    // phantom-pin 3179 cascade a scalar bound to the whole vector produced.
    let named = format!("{RES_CLASS}\nmodule main {{\n    RES(1k) r1\n    r1.Pullup(n2: N2, n1: N1)\n}}\n");
    let errs = bind_errors(&named);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("Vector formal member"), "{errs:?}");
    assert!(errs[0].contains("n2"), "{errs:?}");
    // The cascade codes are gone with it.
    let all: Vec<u32> = bind_diags(&named).iter().map(|(c, _)| *c).collect();
    assert!(!all.contains(&4180), "{all:?}");
    assert!(!all.contains(&3179), "{all:?}");

    // The positional twin stays live: N1-r1.1, N2-r1.2.
    let pos = format!("{RES_CLASS}\nmodule main {{\n    RES(1k) r1\n    r1.Pullup([N1, N2])\n}}\n");
    assert!(bind_errors(&pos).is_empty());
    let part = nets(&pos);
    let n1 = vec!["N1".to_string(), "r1.1".to_string()];
    let n2 = vec!["N2".to_string(), "r1.2".to_string()];
    assert_eq!(part.get("N1").map(Vec::as_slice), Some(n1.as_slice()));
    assert_eq!(part.get("N2").map(Vec::as_slice), Some(n2.as_slice()));
}

#[test]
fn u131_single_formal_named_set_is_one_bracket_argument() {
    // `pull(net: [N1, N2])` ≡ `pull([N1, N2])`: the named form supplies the
    // one bracket argument, judged equal to its positional twin by both the
    // diagnostic multiset and the net partition (the toy body's 4007 shape
    // mismatch is shared by both twins, so it is evidence of equality, not
    // noise to hide).
    let named = format!("{SINGLE_CLASS}\nmodule main {{\n    S s1\n    s1.pull(net: [N1, N2])\n}}\n");
    let pos = format!("{SINGLE_CLASS}\nmodule main {{\n    S s1\n    s1.pull([N1, N2])\n}}\n");
    assert_eq!(diags(&named), diags(&pos));
    assert_eq!(nets(&named), nets(&pos));
    assert!(
        diags(&named).iter().any(|(c, _)| *c == 4007),
        "the shared shape mismatch must be present in both twins"
    );
}

#[test]
fn u131_receiver_this_is_not_nameable() {
    // The receiver travels outside the argument table, so `this:` is not a
    // name the binder ever sees: the spelled argument is dropped on the
    // receiver-substitution path, `N2` fills `a`, and the call fails on the
    // now-missing `b`. The judgment that matters: the call cannot succeed
    // with `this` bound, and the failure is an E4176.
    let src = board("c1.link(this: N1, N2)");
    let errs = bind_errors(&src);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].contains("Missing required parameter: b"), "{errs:?}");
}
