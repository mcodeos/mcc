// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U339 — the member-func shape law (2026-09-28 ruling, canon
//! `mcd/doc/vector/vec-dianlu.md` §7.7): a member-func call's connection
//! effect is decided solely by the shape of its evaluated result. The two
//! silent phantom faces the ruling abolishes:
//!
//! - a **scalar chain head** (`HEAD1 => CAP.Cap(_) -> [X, Y]`) used to
//!   half-bind the `[net1, net2]` formal and leave `net2` a bare literal that
//!   the body wired as the phantom net `_C1.net2`;
//! - a **scalar single argument** (`CAP(..).Cap(P)`) used to leave `net2`
//!   unbound so `cap.2` silently landed on an anonymous `_net0`.
//!
//! Both faces are deficit width mismatches (fewer actual lanes than formal
//! members): the boundary reports E4180 and — since the U339 landing — the
//! func body no longer runs on rejected bindings (the E4176 skip precedent).
//! The receiver itself stays built: an error does not block instantiation
//! (`mcrule.md` §11.6), it only withholds the body's wiring.
//!
//! The compliant column-vector head is locked as the positive control: one
//! instance bridging both lanes, head lane + tail member + cap pin on one
//! net per lane. The surplus actual (`[a, b, c]` against two slots) is
//! deliberately **not** re-locked here — it keeps building
//! (`noblock__surplus_elements_still_build`): the fork + zip clamp leaves
//! every member bound, so there is no phantom to abolish.
//!
//! Fixtures are self-contained (inline component, no system library) so the
//! locks cannot drift with the live `~/.mcode` root.

// Family naming `u339__{essence}` deliberately doubles the underscore so the
// grep-able family token stays separate.
#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::McIds;
use mcc::McURI;

/// Two-pin device with one indexed formal over two slots — the shape the
/// deficit family fills. Body mirrors the library `CAP.Cap`.
const CAP: &str = "component CAP(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";

/// `main` whose `func M()` body is the case under test. The calls sit inside
/// a func so the `=>` pre-closure face is reachable (the scalar-head family's
/// template, mirroring the pwrint `components.mc` form).
fn src(body: &str) -> String {
    format!(
        "{CAP}module main {{\n    io P\n    io Q\n    io X\n    io Y\n    io HEAD1\n    func M() {{\n{body}\n    }}\n}}\n"
    )
}

/// The names of the components the arena actually built for `main` — the
/// structural "was it built" read (see
/// `error_does_not_block_instantiation.rs` for why this is structural).
fn devices_of(src: &str, uri: &str) -> BTreeSet<String> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (tree, arena, store, _) =
        mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    mcc::TreeView::new(&arena, &store)
        .components(&tree)
        .map(|c| c.name.clone())
        .collect()
}

/// Diagnostic codes for `src`, in emit order with multiplicity. Built through
/// `mcc_build_flat` so the connection/net checks run (E4007's phase).
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &u, 1000);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// The connection-derived wiring of `src` — one entry per net, holding the
/// point paths on it.
fn nets_of(src: &str, uri: &str) -> Vec<Vec<String>> {
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

fn has_cap_pin_net(nets: &[Vec<String>]) -> bool {
    nets.iter()
        .any(|net| net.iter().any(|p| p.contains("_C1.") || p.contains("_C2.")))
}

/// Scalar single argument: the deficit is reported and no phantom net is
/// produced — `cap.2` no longer silently lands on an anonymous `_net0`
/// (the abolished pwrint `main.mc` face).
#[test]
fn u339__scalar_single_arg_reports_width_error_without_phantom_net() {
    let src = src("        CAP(10).Cap(P)");
    let nets = nets_of(&src, "/mcc/u339-single-arg.mc");
    assert!(
        !has_cap_pin_net(&nets),
        "no cap pin may sit on a net after the deficit rejection; nets={nets:?}"
    );
    assert!(
        codes_of(&src, "/mcc/u339-single-arg.mc").contains(&4180),
        "the scalar single argument still reports the width error (E4180)"
    );
    assert_eq!(
        devices_of(&src, "/mcc/u339-single-arg.mc"),
        BTreeSet::from(["_C1".to_string()]),
        "the receiver stays built — an error does not block instantiation"
    );
}

/// Scalar chain head: the deficit is reported (E4180 at the call boundary;
/// the statement face independently reports the chain shape E4007) and the
/// body's phantom `_C1.net2` is no longer produced (the abolished pwrint
/// `components.mc` face).
#[test]
fn u339__scalar_chain_head_reports_width_error_without_phantom_net() {
    let src = src("        HEAD1 => CAP(10).Cap(_) -> [X, Y]");
    let nets = nets_of(&src, "/mcc/u339-scalar-head.mc");
    assert!(
        !has_cap_pin_net(&nets),
        "no cap pin may sit on a net after the deficit rejection; nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-scalar-head.mc");
    assert!(
        codes.contains(&4180),
        "the scalar head still reports the width error (E4180); codes={codes:?}"
    );
    assert!(
        codes.contains(&4007),
        "the chain statement face independently reports the shape mismatch \
         (E4007) — the general connection judgment, not a form-specific gate; \
         codes={codes:?}"
    );
    assert_eq!(
        devices_of(&src, "/mcc/u339-scalar-head.mc"),
        BTreeSet::from(["_C1".to_string()]),
        "the receiver stays built — an error does not block instantiation"
    );
}

/// Positive control: a compliant two-lane column-vector head still bridges —
/// one instance, its pins one per lane, each net carrying the head lane, the
/// tail member and the cap pin. The law's column arm must survive the
/// deficit error-ization untouched.
#[test]
fn u339__column_head_still_bridges_both_lanes() {
    let src = src("        [P, Q] => CAP(10).Cap(_) -> [X, Y]");
    let mut nets = nets_of(&src, "/mcc/u339-column-head.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("_C1.")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["P".to_string(), "X".to_string(), "_C1.1".to_string()],
            vec!["Q".to_string(), "Y".to_string(), "_C1.2".to_string()],
        ],
        "the column head bridges one instance across both lanes; nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-column-head.mc");
    assert!(
        !codes.contains(&4180) && !codes.contains(&4007),
        "the compliant column head is diagnosed by nothing; codes={codes:?}"
    );
}
