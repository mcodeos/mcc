// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The export kind table is one table (CIMP U18 item 4; build-design §3.6
//! contract 3): `ExportKind` carries the kind, the `u8` tag `build_payload`
//! dispatches on, the `KIND` token an outward face prints, and the parser for
//! that token; the RPC `features.export` array is derived from it.
//!
//! These cells are what keeps the outward faces from becoming hand-kept
//! copies of it again -- a copy silently misses a kind or spells it another
//! way, and nothing reports the difference.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use mcc::cli::ExportKind;

/// The tag sequence `build_payload` matches on: exactly `0..len`, so no arm
/// can be missing and two kinds cannot share one arm.
#[test]
fn export_kind__tags_are_dense() {
    let ids: Vec<u8> = ExportKind::ALL.iter().map(|k| k.id()).collect();
    let expected: Vec<u8> = (0..ExportKind::ALL.len() as u8).collect();
    assert_eq!(ids, expected, "ids are build_payload's dispatch tags");
}

/// A token printed by `name()` must parse back to its own kind.
#[test]
fn export_kind__every_token_round_trips() {
    for k in ExportKind::ALL {
        assert_eq!(ExportKind::from_name(k.name()), k, "token {}", k.name());
    }
}

/// One token per kind: two kinds sharing a token would make `from_name` answer
/// with one of them for both.
#[test]
fn export_kind__tokens_are_unique() {
    let mut names: Vec<&str> = ExportKind::ALL.iter().map(|k| k.name()).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total, "two kinds share a KIND token");
}

/// The RPC capabilities advertise the same list, not a copy of it.
#[test]
fn export_kind__capabilities_advertise_every_kind() {
    let caps = mcc::rpc::handlers::caps_json();
    let mut advertised: Vec<String> = caps["features"]["export"]
        .as_array()
        .expect("features.export is an array")
        .iter()
        .map(|v| v.as_str().expect("a kind token").to_string())
        .collect();
    let mut expected: Vec<String> = ExportKind::ALL
        .iter()
        .map(|k| k.name().to_string())
        .collect();
    advertised.sort();
    expected.sort();
    assert_eq!(
        advertised, expected,
        "features.export must be the kind table"
    );
}

/// The CLI accepts the short `kicad` spelling while the token is
/// `kicad-netlist`; both spellings name one kind.
#[test]
fn export_kind__the_short_kicad_spelling_is_accepted() {
    assert_eq!(ExportKind::from_name("kicad"), ExportKind::KiCad);
    assert_eq!(ExportKind::KiCad.name(), "kicad-netlist");
}

/// The count is a tripwire, not a contract: adding a kind means visiting this
/// file -- and the format table in `spec/16-export-viz.md` -- on purpose.
#[test]
fn export_kind__the_table_is_a_deliberate_list() {
    assert_eq!(ExportKind::ALL.len(), 5);
}
