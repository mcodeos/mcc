// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U338: the bare inline instance as a `=>` pair-chain anchor.
//!
//! b4064 measured the gap: `[A, B] => CAP(x) -> [C, D]` dropped the prefix at
//! parse, merged both cap pins onto one phantom net, and answered one leaf
//! point at the chain tail (E4007, head nets lost). The b4149 probe pinned
//! the mechanism; the fix keeps the prefix as terminal-binding lanes on the
//! construction (`pre_closure` + caller-less + real-`left` marker). These
//! locks pin the new face against its `.Pull(_)` anchor control: identical
//! netlist multiset, identical diagnostics, and the width-mismatch face
//! reporting E4180 once without the tail re-judging it as E4007.

use crate::common;

use mcc::{McIds, McURI};

const PRELUDE: &str = r#"
component RES(r) {
    pins = [
        1 = _
        2 = _
    ]
    func Pull([n1, n2]) {
        n1 - this - n2
        return [n1, n2]
    }
}
module top {
    io A1
    io A2
    io B1
    io B2
    func M() {
"#;

fn netlines(src: &str) -> (Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/u338-inline-anchor.mc");
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("top"), &uri, 1000).expect("flat build");
    let mut lines: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        lines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    lines.sort();
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (lines, codes)
}

/// The ruled equivalence: the bare inline anchor and the `.Pull(_)` method
/// anchor produce the same netlist multiset with zero diagnostics —
/// `[A1, A2] => CAP(x)` binds the prefix lanes pairwise to the cap's
/// terminals and the tail zips row-wise against the same lane set.
#[test]
fn bare_anchor_matches_method_anchor_netlist() {
    let (bare_lines, bare_codes) = netlines(&format!(
        "{PRELUDE}        [A1, A2] => RES(10k) -> [B1, B2]\n    }}\n}}"
    ));
    let (method_lines, method_codes) = netlines(&format!(
        "{PRELUDE}        [A1, A2] => RES(10k).Pull(_) -> [B1, B2]\n    }}\n}}"
    ));
    assert!(
        !bare_lines.is_empty(),
        "the bare anchor must produce a netlist, not an evaporated statement"
    );
    assert_eq!(
        bare_lines, method_lines,
        "bare anchor and method anchor must land the same netlist multiset"
    );
    assert_eq!(bare_codes, method_codes, "diagnostics must match too");
    assert!(
        !bare_codes.contains(&4007),
        "E4007 must be gone from the anchored form; got {bare_codes:?}"
    );
}

/// Width mismatch: one prefix lane against a two-terminal cap reports E4180
/// once — the chain tail falls to the instance's own ordered terminal face
/// instead of re-judging the same fact as E4007 — and the instance is still
/// created (errors do not block instantiation).
#[test]
fn scalar_prefix_lane_reports_e4180_once_and_keeps_instance() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}        [A1] => RES(10k) -> [B1, B2]\n    }}\n}}"
    ));
    assert!(
        codes.contains(&4180),
        "E4180 width mismatch must fire on the scalar prefix; got {codes:?}"
    );
    assert!(
        !codes.contains(&4007),
        "the tail must not re-report the same width fact as E4007; got {codes:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains(".2")),
        "the instance stays created and its exit terminal still reaches the tail; got {lines:?}"
    );
}
