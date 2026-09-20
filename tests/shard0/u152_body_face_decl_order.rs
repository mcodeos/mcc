// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U152 ②: the MultiPort body face must read its lane order from the pin
//! declaration order, never from the BTreeMap key order.
//!
//! `McPins::get_pins_by_io` iterated `pins` (a `BTreeMap` keyed by pin-id
//! string), so every face vector it fed the positional pairing core came out
//! in dictionary order — "1", "10", "11", "12", "2", … A parallel fold of two
//! parenthesized bodies (`(a) + (b)`) pairs the two faces k↔k, so with ≥10
//! direction-worded pins the fold silently mis-paired lanes (a.10 rode lane 2).
//! The fix reads `decl_order` (R0 source-order law); these locks pin both
//! verdicts of the discriminator, neither of which the pre-fix binary
//! satisfies: pre-fix, lock 1 pairs a.10↔b.10 and lock 2's creation order runs
//! 1, 10, 11, 12, 2, …
//!
//! Same in-process harness as `rule_audit_a`: build module `main`, read the
//! connection list in creation order.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

#[derive(Debug, Default)]
struct Probe {
    /// (sorted point paths) per connection, in creation order
    nets: Vec<Vec<String>>,
    diags: Vec<(u32, String)>,
}

fn probe(source: &str) -> Probe {
    let _lock = common::lock();
    let system_root = mcc::cli::datadir::data_root();
    mcc::mcc_clear_workspace();
    mcc::mcc_set_system_root(&system_root);
    mcc::mcc_init();

    let uri: McURI = "/mcc/u152-body-face.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let mut p = Probe::default();
    if let Ok((inst, _arena, _store, _net_store)) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri)
    {
        for c in inst.connections.iter() {
            let mut pts: Vec<String> = c.points.iter().map(|x| x.path.clone()).collect();
            pts.sort();
            p.nets.push(pts);
        }
    }
    p.diags = mcc::mcc_diagnose(&uri)
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    p
}

/// SRC presents 12 `in` pins written in ascending id order plus one `out`;
/// SINK presents 12 `in` pins whose ids are written in scrambled order
/// (1, 12, 11, …, 2 — declaration order ≠ dictionary order ≠ numeric order)
/// plus one `out`. The fold `(a) + (b)` must pair a's k-th declared input
/// with b's k-th declared input, whatever the pin ids.
#[test]
fn lock_pp_body_face__fold_pairs_in_declaration_order() {
    let a_in: Vec<String> = (1..=12).map(|i| format!("        in {i} = P{i:02}")).collect();
    let order = [1usize, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2];
    let b_in: Vec<String> = order.iter().map(|&i| format!("        in {i} = Q{i:02}")).collect();
    let src = format!(
        "component SRC12\n{{\n    name = \"SRC12\"\n    pins = [\n{}\n        out 300 = DONE\n    ]\n}}\n\ncomponent SINKB\n{{\n    name = \"SINKB\"\n    pins = [\n{}\n        out 301 = DONE2\n    ]\n}}\n\nmodule main()\n{{\n    SRC12 a\n    SINKB b\n    (a) + (b)\n}}\n",
        a_in.join("\n"),
        b_in.join("\n")
    );
    let p = probe(&src);
    // a's lane k (id k, declared k-th) pairs b's k-th DECLARED input.
    let expect: Vec<Vec<String>> = (1..=12)
        .map(|k| {
            let mut pair = vec![format!("a.{k}"), format!("b.{}", order[k - 1])];
            pair.sort();
            pair
        })
        .collect();
    let mut got = p.nets.clone();
    got.retain(|pts| {
        pts.len() == 2
            && pts.iter().any(|x| x.starts_with("a."))
            && pts.iter().any(|x| x.starts_with("b."))
            && !pts.iter().any(|x| x.ends_with("300") || x.ends_with("301"))
    });
    assert_eq!(
        got, expect,
        "the fold must pair body faces in declaration order; diags={:?} nets={:?}",
        p.diags, p.nets
    );
}

/// Ascending control: with both sides written in ascending id order the fold
/// pairs lane k↔k, and the lane connections are *created* in declaration
/// order (1, 2, …) — pre-fix the creation order ran 1, 10, 11, 12, 2, …
#[test]
fn lock_pp_body_face__fold_creates_lanes_in_declaration_order() {
    let a_in: Vec<String> = (1..=12).map(|i| format!("        in {i} = P{i:02}")).collect();
    let b_in: Vec<String> = (1..=12).map(|i| format!("        in {i} = Q{i:02}")).collect();
    let src = format!(
        "component SRC12\n{{\n    name = \"SRC12\"\n    pins = [\n{}\n        out 300 = DONE\n    ]\n}}\n\ncomponent SINKB\n{{\n    name = \"SINKB\"\n    pins = [\n{}\n        out 301 = DONE2\n    ]\n}}\n\nmodule main()\n{{\n    SRC12 a\n    SINKB b\n    (a) + (b)\n}}\n",
        a_in.join("\n"),
        b_in.join("\n")
    );
    let p = probe(&src);
    let lane_pairs: Vec<Vec<String>> = p
        .nets
        .iter()
        .filter(|pts| {
            pts.iter().any(|x| x.starts_with("a."))
                && pts.iter().any(|x| x.starts_with("b."))
                && pts.len() == 2
                && !pts.iter().any(|x| x.ends_with("300") || x.ends_with("301"))
        })
        .cloned()
        .collect();
    let expect: Vec<Vec<String>> = (1..=12)
        .map(|k| vec![format!("a.{k}"), format!("b.{k}")])
        .collect();
    assert_eq!(
        lane_pairs, expect,
        "lane k must pair lane k, created in declaration order; diags={:?} nets={:?}",
        p.diags, p.nets
    );
}
