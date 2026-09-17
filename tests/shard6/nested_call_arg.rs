// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! func-return-design §1: **a call sitting in an argument position runs
//! first**, and its return face is the argument.
//!
//! ```text
//! LNK().Go(LNK().Go(SIG, A1), A2)
//!   ≡ LNK().Go(SIG, A1)        -- inner, materialized first
//!     LNK().Go(<its return>, A2)
//! ```
//!
//! Substitution only reads an argument phrase's left endpoint, which a call
//! does not have, so the inner call used to vanish without a diagnostic while
//! its formal name leaked into the netlist as a label: `SIG -> A1 -> A2` came
//! out as a single hop. The load-bearing assertions are therefore the **nets**
//! (every hop lands) and the **component count** (each hop is its own device).

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// Two-pin link: `Go` shorts `net - this - vcc` and returns `vcc`, so chaining
/// `Go` calls walks the net one device at a time.
const LNK: &str = "component LNK() {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Go(net, vcc) {\n        net - this - vcc\n        return vcc\n    }\n}\n";

/// Module skeleton: the chain's endpoints plus a third hop target.
const HEAD: &str = "module main {\n    io SIG\n    io A1\n    io A2\n    io A3\n    func M() {\n";

fn src_of(body: &str) -> String {
    format!("{LNK}{HEAD}{body}\n    }}\n}}\n")
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

/// Wiring codes (`4xxx`) only — a clean nest must not raise one.
fn wiring_codes_of(src: &str, uri: &str) -> Vec<u32> {
    codes_of(src, uri)
        .into_iter()
        .filter(|c| (4000..5000).contains(c))
        .collect()
}

/// The auto-component heads in the partition (a point whose last segment is a
/// numeric pin), i.e. the devices the expansion created.
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

/// The pins of `net` on `side` (`1` = entry, `2` = exit).
fn pins_on<'a>(net: &'a [String], side: &str) -> Vec<&'a String> {
    net.iter().filter(|p| p.ends_with(side)).collect()
}

/// §1: the inner call is materialized and its return face becomes the outer
/// argument — two devices, and the middle net carries both devices' pins.
#[test]
fn nested_call_arg__inner_call_runs_first() {
    let src = src_of("        LNK().Go(LNK().Go(SIG, A1), A2)");
    let parts = partition_of(&src, "/mcc/nested-call-arg.mc");

    assert_eq!(
        instance_heads(&parts).len(),
        2,
        "each hop is its own link; nets={parts:?}"
    );
    assert_eq!(parts.len(), 3, "SIG / A1 / A2 nets only; nets={parts:?}");

    let sig = net_with(&parts, "SIG");
    assert_eq!(
        (
            sig.len(),
            pins_on(sig, ".1").len(),
            pins_on(sig, ".2").len()
        ),
        (2, 1, 0),
        "SIG feeds the inner link's pin 1; nets={parts:?}"
    );
    let a1 = net_with(&parts, "A1");
    assert_eq!(
        (a1.len(), pins_on(a1, ".1").len(), pins_on(a1, ".2").len()),
        (3, 1, 1),
        "the inner link's return (its pin 2) must feed the outer link; nets={parts:?}"
    );
    let a2 = net_with(&parts, "A2");
    assert_eq!(
        (a2.len(), pins_on(a2, ".1").len(), pins_on(a2, ".2").len()),
        (2, 0, 1),
        "the outer link's pin 2 is the written rail; nets={parts:?}"
    );
    assert_eq!(
        wiring_codes_of(&src, "/mcc/nested-call-arg.mc"),
        Vec::<u32>::new(),
        "the nest must not raise a wiring code"
    );
}

/// The same tree the `=>` chain folds into, written out: every hop must land,
/// not just the outermost one.
#[test]
fn nested_call_arg__uscore_chain_delivers_every_hop() {
    let src = src_of("        SIG => LNK().Go(_, A1) => LNK().Go(_, A2) => LNK().Go(_, A3)");
    let parts = partition_of(&src, "/mcc/nested-call-chain.mc");

    assert_eq!(
        instance_heads(&parts).len(),
        3,
        "three folds must build three links; nets={parts:?}"
    );
    assert_eq!(parts.len(), 4, "one net per hop; nets={parts:?}");
    for (net, entries, exits) in [("A1", 1, 1), ("A2", 1, 1), ("A3", 0, 1)] {
        let ps = net_with(&parts, net);
        assert_eq!(
            (pins_on(ps, ".1").len(), pins_on(ps, ".2").len()),
            (entries, exits),
            "{net} must be an interior hop ({entries} in, {exits} out); nets={parts:?}"
        );
    }
    assert_eq!(
        wiring_codes_of(&src, "/mcc/nested-call-chain.mc"),
        Vec::<u32>::new(),
        "the chain must not raise a wiring code"
    );
}

/// §1: an inner call that cannot be materialized must be **reported**: the
/// formal name must not be left in the netlist as a label with no diagnostic.
#[test]
fn nested_call_arg__unmaterialized_inner_call_is_reported() {
    for body in [
        // A bare class name with no def to resolve against.
        "        LNK().Go(ZZZ(SIG), A2)",
        // A method on a receiver whose class has no def.
        "        LNK().Go(MISSING().Go(SIG, A1), A2)",
    ] {
        let codes = codes_of(&src_of(body), "/mcc/nested-call-unmaterialized.mc");
        assert!(
            codes.contains(&4152),
            "an unresolvable inner call must report E4152; body={body:?} codes={codes:?}"
        );
    }
}
