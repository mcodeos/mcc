// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Access-bit spelling matrix for component pin member access.
//!
//! Three spellings address the same pins and must land the same partition:
//!
//!   - bare curly        `d1{2:3}`
//!   - `pins` transparency `d1.pins{2:3}`
//!   - mixed member list `d1.pins{1, 2:3}`
//!
//! `McIds::as_component_member()` reads the `(Ida, DotIda, Curly)` triple and
//! originally dropped `IdsSegment::Slice` members, so a range spelling produced
//! zero members and fell through to the ghost-bus path — the range became one
//! opaque name, got isolated by `NetPoint::new` and surfaced as a `@_phantom_`
//! pin (E3179). The `Slice` branch keeps the range expandable.
//!
//! The assertions are on the net **partition**, plus one count control
//! (`pins{2:3}` against a 1-member and a 3-member side) so a range that
//! degrades to a single member cannot pass by matching itself.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate.
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// Four named-numbered pins: a range has something to expand into.
const QUAD: &str = "component QUAD {\n    pins = [\n        1 = 1\n        2 = 2\n        3 = 3\n        4 = 4\n    ]\n}\n";

const HEAD: &str = "module main {\n    QUAD d1\n    QUAD d2\n    ";

fn src_of(body: &str) -> String {
    format!("{QUAD}{HEAD}{body}\n}}\n")
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped. Point paths keep their instance prefix, so the forms'
/// component wiring is comparable.
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

/// Every diagnostic code emitted while building `src`.
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

/// The net carrying `needle`, by substring match over its point paths.
fn net_holding<'a>(parts: &'a [Vec<String>], needle: &str) -> Option<&'a Vec<String>> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains(needle)))
}

/// No point path may carry a ghost-bus artifact.
fn assert_no_ghost_paths(parts: &[Vec<String>], what: &str) {
    for ps in parts {
        for p in ps {
            assert!(
                !p.contains("phantom") && !p.contains('{') && !p.contains('}'),
                "{what}: ghost point '{p}' in partition {parts:?}"
            );
        }
    }
}

/// The fix itself: a range member expands to its pins instead of collapsing
/// into one opaque name (which E3179 then reports as a phantom).
#[test]
fn pins_access__range_expands_to_members() {
    let parts = nets_of(
        &src_of("d1.pins{2:3} -> d2.pins{2:3}"),
        "/mcc/pins-range.mc",
    );

    assert_no_ghost_paths(&parts, "range member");
    assert_eq!(parts.len(), 2, "one net per lane; nets={parts:?}");

    let n2 = net_holding(&parts, "d1.2").expect("d1 pin 2 landed");
    assert!(
        n2.iter().any(|p| p.starts_with("d2.2")),
        "pin 2 pairs per lane; net={n2:?}"
    );
    let n3 = net_holding(&parts, "d1.3").expect("d1 pin 3 landed");
    assert!(
        n3.iter().any(|p| p.starts_with("d2.3")),
        "pin 3 pairs per lane; net={n3:?}"
    );

    let codes = codes_of(
        &src_of("d1.pins{2:3} -> d2.pins{2:3}"),
        "/mcc/pins-range-codes.mc",
    );
    assert!(
        !codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "a range member must not produce E3179; codes: {codes:?}"
    );
}

/// `pins{...}` is transparent: the same members produce the same partition as
/// the bare curly spelling.
#[test]
fn pins_access__pins_spelling_equals_bare_curly() {
    for (pins_form, bare_form) in [
        ("d1.pins{2:3} -> d2.pins{2:3}", "d1{2:3} -> d2{2:3}"),
        (
            "d1.pins{1, 2:3} -> d2.pins{1, 2, 3}",
            "d1{1, 2:3} -> d2{1, 2, 3}",
        ),
        ("d1.pins{2:3} -> d2.pins{2, 3}", "d1{2:3} -> d2{2, 3}"),
    ] {
        let with_pins = nets_of(&src_of(pins_form), "/mcc/pins-spelling.mc");
        let bare = nets_of(&src_of(bare_form), "/mcc/pins-bare.mc");
        assert_eq!(
            with_pins, bare,
            "'{pins_form}' must land the same partition as '{bare_form}'"
        );
        assert_no_ghost_paths(&with_pins, pins_form);
    }
}

/// Mixed member list — single pins and a range side by side.
#[test]
fn pins_access__mixed_list_lands_every_member() {
    let parts = nets_of(
        &src_of("d1.pins{1, 2:3} -> d2.pins{1, 2, 3}"),
        "/mcc/pins-mixed.mc",
    );

    assert_no_ghost_paths(&parts, "mixed member list");
    assert_eq!(parts.len(), 3, "one net per member; nets={parts:?}");
    for (a, b) in [("d1.1", "d2.1"), ("d1.2", "d2.2"), ("d1.3", "d2.3")] {
        let net = net_holding(&parts, a).unwrap_or_else(|| panic!("{a} landed; nets={parts:?}"));
        assert!(
            net.iter().any(|p| p.starts_with(b)),
            "{a} pairs with {b}; net={net:?}"
        );
    }
}

/// Count control: the range really contributes `to - from + 1` members, so a
/// 2-member range against a 1-member or 3-member side is a shape mismatch.
/// Without this, an expansion that silently collapses to one member would make
/// the equality tests above vacuous.
#[test]
fn pins_access__range_member_count_is_exact() {
    let two_vs_one = codes_of(
        &src_of("d1.pins{2:3} -> d2.pins{2}"),
        "/mcc/pins-count-1.mc",
    );
    assert!(
        two_vs_one.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "2-member range against 1 member must be a shape mismatch; codes: {two_vs_one:?}"
    );

    let two_vs_three = codes_of(
        &src_of("d1.pins{2:3} -> d2.pins{1, 2, 3}"),
        "/mcc/pins-count-3.mc",
    );
    assert!(
        two_vs_three.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "2-member range against 3 members must be a shape mismatch; codes: {two_vs_three:?}"
    );

    let three_vs_three = codes_of(
        &src_of("d1.pins{1, 2:3} -> d2.pins{1, 2, 3}"),
        "/mcc/pins-count-ok.mc",
    );
    assert!(
        !three_vs_three.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "3-member mixed list against 3 members is legal; codes: {three_vs_three:?}"
    );
}

/// A descending range expands by declaration direction, so it is the reversed
/// member list and pairs lane by lane against it.
#[test]
fn pins_access__descending_range_reverses_lanes() {
    let descending = nets_of(
        &src_of("d1.pins{3:1} -> d2.pins{3, 2, 1}"),
        "/mcc/pins-desc.mc",
    );

    assert_no_ghost_paths(&descending, "descending range");
    assert_eq!(
        descending.len(),
        3,
        "one net per member; nets={descending:?}"
    );
    for (a, b) in [("d1.3", "d2.3"), ("d1.2", "d2.2"), ("d1.1", "d2.1")] {
        let net = net_holding(&descending, a)
            .unwrap_or_else(|| panic!("{a} landed; nets={descending:?}"));
        assert!(
            net.iter().any(|p| p.starts_with(b)),
            "descending lane pairs {a} with {b}; net={net:?}"
        );
    }
}
