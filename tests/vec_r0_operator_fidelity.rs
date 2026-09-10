// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! R0 (`vec-dianlu.md` §2.4) — written operators survive to the netlist.
//!
//! Two "word-level" losses were registered as violations of §2.4 ban 2 (the
//! operator is **encoded**, never rewritten):
//!
//! * **A6** — `merge_adjacent_curly_split` merged two adjacent same-name
//!   single-member Bus phrases into one Bus **without looking at `gaps`**, so
//!   a written `->` between them was deleted along with the boundary. The
//!   pre-pass has been retired; its repair target (a parser defect that split
//!   `Name{a,b}` at statement start) no longer reproduces on any board.
//! * **A7** — `phrase_to_members` flattened a `Series` lane of a `Multiple`
//!   into separate lanes and dropped its gaps. A chain is **one** lane
//!   (`get_left_points` / `get_right_points` read it as its first / last
//!   member), so the flatten changed the lane faces as well as the direction.
//!   `normalize_multiple_lanes` now keeps it whole.
//!
//! The locks below are end-to-end, on the net partition: the claim is what the
//! two points *are*, not how the phrase tree looks.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

const DECLS: &str = "component VOUT(res::INT) {\n    pins = [\n        VCC = 1\n        GND = 2\n    ]\n}\n";

/// Build `main` and return (diagnostic codes sorted, net partition).
///
/// The partition is normalized to a sorted list of sorted member lists: net
/// *names* are not part of the claim (they are synthesized), the **grouping of
/// points** is.
fn build(statements: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!("{DECLS}module main {{\n    VOUT d1(1), d2(1)\n{statements}\n}}\n");
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();

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
    (codes, partition)
}

/// A6: `d1.VCC -> d1.GND` is a **chain of two members of one instance**, so
/// both land on one net. It used to be rewritten into a single
/// `Bus(d1, [VCC, GND])` — which `expand_multi_member_buses` then turned into a
/// two-lane `Multiple` — deleting the operator and splitting the two members
/// onto separate nets.
///
/// Locked by equivalence with the same chain written with `-`: the arrows
/// differ only in the direction they record, never in the wiring (§1.4 — the
/// junction is positional).
#[test]
fn curly_merge__same_owner_member_operator_is_not_erased() {
    let (arrow_codes, arrow_nets) = build("    d1.VCC -> d1.GND", "/mcc/r0-a6-arrow.mc");
    let (dash_codes, dash_nets) = build("    d1.VCC - d1.GND", "/mcc/r0-a6-dash.mc");

    assert_eq!(arrow_codes, dash_codes, "same diagnostics");
    assert_eq!(
        arrow_nets, dash_nets,
        "`->` and `-` must wire the same (vec-dianlu.md §1.4)"
    );
    assert!(
        arrow_nets
            .iter()
            .any(|n| n.contains(&"d1.VCC".to_string()) && n.contains(&"d1.GND".to_string())),
        "both members of the same instance must share one net; got {arrow_nets:?}"
    );
}

/// A6: the same shape across two *different* instances already worked (names
/// differ, so the old pre-pass never fired); kept as the control for the cell
/// above so a future regression cannot pass by making every statement vanish.
#[test]
fn curly_merge__different_owner_still_chains() {
    let (codes, nets) = build("    d1.VCC -> d2.GND", "/mcc/r0-a6-cross-owner.mc");
    // The only diagnostics are W5641 — the two instances' other members are
    // left unused. No shape or member error.
    assert_eq!(
        codes,
        vec![mcc::errcodes::UNUSED_PARAM_OR_PORT],
        "only the unused-member warnings"
    );
    assert!(
        nets
            .iter()
            .any(|n| n.contains(&"d1.VCC".to_string()) && n.contains(&"d2.GND".to_string())),
        "d1.VCC and d2.GND must share one net; got {nets:?}"
    );
}

/// A6 (the gap this closes): a same-owner member reference that is not
/// declared is reported. `error_codes.rs` recorded this as a "pre-existing gap,
/// tracked separately" — the merge used to fold `vout.VCC -> vout.VCC1V2` into
/// `vout{VCC, VCC1V2}` before the member check ran, hiding the undeclared
/// member.
#[test]
fn curly_merge__undeclared_member_is_reported() {
    let src = "interface DC(volt)\n{\n    pins = [\n        1 = VCC\n        2 = GND\n    ]\n}\nmodule main\n{\n    out vout::DC(3.3V)\n    vout.VCC -> vout.VCC1V2\n}";
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/r0-a6-undeclared-member.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&mcc::errcodes::BUS_MEMBER_UNDECLARED),
        "E3181 must fire for vout.VCC1V2; got {codes:?}"
    );
}
