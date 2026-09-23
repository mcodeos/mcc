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

use crate::common;

use mcc::cli::ExportKind;
use mcc::{McIds, McURI};

/// A board just big enough for the exporters to have something to walk: one
/// defined component, one instance, one wired port pair.
const DISPATCH_SOURCE: &str = r#"
module PWR_REG()
{
    io vin
    io vout
}

module main
{
    io V5V
    io V3V3
    PWR_REG reg
    V5V -> reg{vin|vout} -> V3V3
}
"#;

/// Build the fixture and hand the four handles `build_payload` walks. The
/// caller holds [`common::lock`].
fn dispatch_fixture() -> (
    mcc::MccProjectTree,
    mcc::InstTable,
    mcc::NodeArena,
    mcc::InstanceStore,
) {
    common::reset();
    let uri = McURI::from("/mcc/export-dispatch.mc");
    mcc::mcc_load_from_string(&uri, DISPATCH_SOURCE);
    mcc::mcc_build_flat_with_arena(&McIds::from("main"), &uri, 1000).expect("flat build")
}

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
    assert_eq!(ExportKind::ALL.len(), 6);
}

/// The `inst-list` tag must reach the inst-list arm through the real dispatch
/// path: the payload is the `{ items, defs }` object (inst-list design §5 --
/// every face carries the same two things). Density alone does not prove arm
/// ownership (U270): b3890 landed `id()` = 4/5 swapped with both arms unmoved,
/// so an inst-list request hit the kicad-sch guidance arm and every existing
/// lock stayed green.
#[test]
fn export_kind__inst_list_tag_dispatches_to_the_inst_list_shape() {
    let _lock = common::lock();
    let (tree, table, arena, store) = dispatch_fixture();
    let (_, payload, count) = mcc::export::build_payload(
        &tree,
        &table,
        &arena,
        &store,
        "main",
        ExportKind::InstList.id(),
        0,
    );
    assert!(count > 0, "the arm ran and produced rows for the fixture");
    let items = payload
        .get("items")
        .and_then(|v| v.as_array())
        .expect("payload[items] is the row array");
    assert!(
        items.iter().any(|r| r.get("path").is_some()),
        "rows carry the canonical instance path"
    );
    assert!(
        payload.get("defs").is_some(),
        "payload[defs] is the definition ledger"
    );
}

/// The `kicad-sch` tag must reach the guidance arm: the graphical export
/// writes one file per sheet and cannot come back as a single payload, so the
/// arm points an RPC caller at the CLI instead of returning a silently
/// partial artifact.
#[test]
fn export_kind__kicad_sch_tag_dispatches_to_the_guidance_arm() {
    let _lock = common::lock();
    let (tree, table, arena, store) = dispatch_fixture();
    let (listing, payload, count) = mcc::export::build_payload(
        &tree,
        &table,
        &arena,
        &store,
        "main",
        ExportKind::KiCadSch.id(),
        0,
    );
    assert_eq!(count, 0, "no payload was produced");
    assert!(
        payload.is_null(),
        "no payload object masquerades as a sheet"
    );
    assert!(
        listing.contains("mcc export kicad-sch"),
        "the guidance names the CLI product, got: {listing}"
    );
}
