// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP §1 U97 (both rulings: the predicate and the fold): a bare member key
//! written under an owner names
//! the **declared** member of that owner when exactly one such member exists.
//!
//! The shape this locks: hbl's root layer wires the flash to the MCU's SPI port
//! by pin number, and the reference reaches the builder as a `NetPoint` whose
//! owner is the instance and whose path is the bare member key (`MCU513` + `8`).
//! Before the fold, no lookup knew about that spelling, so two separate
//! consequences followed:
//!
//! - `flatten_nets` minted a key nobody declared (`main.MCU513.8`), and
//! - the vec builder's owner fallback attached the whole `MCU513` box, which
//!   erased *which* member the wire reaches — the four SPI nets collapsed onto
//!   one endpoint, the bus trunk vanished and the edge lost its `[4]` label.
//!
//! The two callers now share one rule, [`InstTable::declared_member_port_of`],
//! so neither ladder can drift: a declared member **Port** of the owner whose
//! path ends in `.{key}`, and only when it is unique. Ambiguity is not a
//! preference to be guessed at (AGENTS.md "no guessing") — it falls through to
//! the coarser fallback that already existed.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashSet;
use std::path::PathBuf;

use mcc::instant::mc_net::NetPoint;
use mcc::vector::builder::report::ResolutionOutcome;
use mcc::vector::builder::resolve::{resolve_netpoint_v2, ResolveOutcome};
use mcc::{IOType, InstKind, InstTable, McIds};

/// A table holding `main` → `main.S` (a module instance) → the given children,
/// all registered directly under `main.S`.
///
/// The ladder under test reads the table alone ([`resolve_netpoint_v2`] takes a
/// `&InstTable`), so no build is needed and every branch is reachable by
/// construction. The caller must hold [`common::lock`] — `register` writes to
/// the process-global diagnostic log.
fn table_with_children(children: &[(&str, InstKind)]) -> (InstTable, u32) {
    let uri = "/mcc/u97-declared-member-port.mc".to_string();
    common::load_string(&uri, "module main {}");
    let mut table = InstTable::new(1000);
    let main_id = table.register(
        "main".to_string(),
        InstKind::Module,
        None,
        "main".to_string(),
        IOType::None,
        None,
        String::new(),
    );
    let sub_id = table.register(
        "main.S".to_string(),
        InstKind::Module,
        Some(main_id),
        "S".to_string(),
        IOType::None,
        None,
        String::new(),
    );
    for (path, kind) in children {
        table.register(
            (*path).to_string(),
            kind.clone(),
            Some(sub_id),
            "sub".to_string(),
            IOType::None,
            None,
            String::new(),
        );
    }
    (table, sub_id)
}

/// Resolve `owner.key` — the spelling this rule exists for.
fn resolve_member(table: &InstTable, owner: &str, key: &str) -> ResolveOutcome {
    let point = NetPoint::with_owner(key, owner, IOType::None, None);
    resolve_netpoint_v2(table, &point, "main", "SPI")
}

/// The single record the attempt left, or a panic naming what it produced
/// instead — a silent extra record is a failure of this rule, not noise.
fn only_outcome(out: &ResolveOutcome) -> ResolutionOutcome {
    assert_eq!(
        out.records.len(),
        1,
        "one point must leave exactly one record: {:?}",
        out.records
    );
    out.records[0].outcome.clone()
}

// (a) the fold itself: one declared member Port under the owner, matched by key

#[test]
fn u97__bare_member_key_resolves_to_the_declared_member_port() {
    let _lock = common::lock();
    let (table, sub_id) = table_with_children(&[
        ("main.S.SPI", InstKind::Port),
        ("main.S.SPI.8", InstKind::Port),
        ("main.S.SPI.9", InstKind::Port),
        ("main.S.UART0", InstKind::Port),
        ("main.S.UART0.RX", InstKind::Port),
        ("main.S.UART0.TX", InstKind::Port),
    ]);
    let spi8 = table
        .get_id_by_path("main.S.SPI.8")
        .expect("declared member");

    let out = resolve_member(&table, "S", "8");
    assert_eq!(
        only_outcome(&out),
        ResolutionOutcome::DeclaredMemberPort {
            member: "8".into(),
            port_path: "main.S.SPI.8".into(),
        }
    );
    assert_eq!(out.ids, vec![i64::from(spi8)]);
    assert_ne!(
        out.ids,
        vec![i64::from(sub_id)],
        "the coarse owner fallback is what this rule replaces"
    );

    // A named member key works the same way (`UART0.RX`, reached bare).
    let rx = table
        .get_id_by_path("main.S.UART0.RX")
        .expect("declared member");
    let out = resolve_member(&table, "S", "RX");
    assert_eq!(out.ids, vec![i64::from(rx)]);
    assert!(matches!(
        only_outcome(&out),
        ResolutionOutcome::DeclaredMemberPort { .. }
    ));
}

// (b) ambiguity is not guessed at: two member Ports claim the key

#[test]
fn u97__ambiguous_member_key_falls_through_to_the_existing_fallback() {
    let _lock = common::lock();
    let (table, sub_id) = table_with_children(&[
        ("main.S.SPI", InstKind::Port),
        ("main.S.SPI.8", InstKind::Port),
        ("main.S.ALT", InstKind::Port),
        ("main.S.ALT.8", InstKind::Port),
    ]);

    let out = resolve_member(&table, "S", "8");
    assert_eq!(
        only_outcome(&out),
        ResolutionOutcome::OwnerFallback,
        "two members claim '8'; the ladder must not pick one"
    );
    assert_eq!(out.ids, vec![i64::from(sub_id)]);
}

// (c) the rule counts declared Ports only — a bus member spelling is not one

#[test]
fn u97__only_a_declared_port_member_folds() {
    let _lock = common::lock();
    let (table, sub_id) = table_with_children(&[
        ("main.S.BUS", InstKind::Bus),
        ("main.S.BUS.8", InstKind::Bus),
        ("main.S.NET", InstKind::Label),
        ("main.S.NET.8", InstKind::Label),
    ]);

    let out = resolve_member(&table, "S", "8");
    assert_eq!(
        only_outcome(&out),
        ResolutionOutcome::OwnerFallback,
        "a Bus / Label child ending in '.8' is not a member Port"
    );
    assert_eq!(out.ids, vec![i64::from(sub_id)]);
}

// (d) no member Port claims the key at all: the fallback, not a near-miss

#[test]
fn u97__unmatched_member_key_falls_through_to_the_existing_fallback() {
    let _lock = common::lock();
    let (table, sub_id) = table_with_children(&[
        ("main.S.SPI", InstKind::Port),
        ("main.S.SPI.8", InstKind::Port),
        ("main.S.SPI.9", InstKind::Port),
    ]);

    // Near-misses a laxer test would fold: `88` *starts with* the declared key
    // `8`, `SCLK` is a declared member's **name** where that member's key is
    // `8`, and `7` names nothing at all ⇒ the fold must be an exact match on
    // the tail after the dot, never a prefix nor a name.
    for key in ["7", "88", "SCLK"] {
        let out = resolve_member(&table, "S", key);
        assert_eq!(
            only_outcome(&out),
            ResolutionOutcome::OwnerFallback,
            "key '{key}' names no declared member"
        );
        assert_eq!(out.ids, vec![i64::from(sub_id)], "key '{key}'");
    }
}

// (e) the real board: the declared member is what the wire lands on, and the
//     key the boundary fallback used to mint is gone

/// Freeze the real hbl board and hand back its flat table.
///
/// The caller must hold [`common::lock`].
fn frozen_hbl() -> InstTable {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    let entry_uri = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);
    let (_inst, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    table
}

#[test]
fn u97__hbl_lands_on_the_declared_member_and_invents_no_key() {
    let _lock = common::lock();
    let table = frozen_hbl();

    // `io [8:11] = SPI{...}` in the MCU declares the four member keys as Ports
    // under the instance path.
    let declared: Vec<u32> = ["8", "9", "10", "11"]
        .iter()
        .map(|k| {
            let path = format!("main.MCU513.SPI.{k}");
            let id = table.get_id_by_path(&path).unwrap_or_else(|| {
                let near: Vec<&str> = table
                    .iter()
                    .filter(|(_, e)| e.path.starts_with("main.MCU513.SPI"))
                    .map(|(_, e)| e.path.as_str())
                    .collect();
                panic!("the board declares {path}; it has {near:?}")
            });
            assert_eq!(
                table.get_entry(id).map(|e| e.kind.clone()),
                Some(InstKind::Port),
                "{path} must be a declared Port"
            );
            id
        })
        .collect();

    // The minted spelling is not in the table — that is the whole point of the
    // fold, and the reason `resolve_netpoint_v2` can now reach the declared Port
    // at all (a minted `main.MCU513.8` would win the Direct step first).
    for k in ["8", "9", "10", "11"] {
        let invented = format!("main.MCU513.{k}");
        assert_eq!(
            table.get_id_by_path(&invented),
            None,
            "{invented} is a key nobody declared"
        );
    }

    // And the information the coarse fallback destroyed is back, one layer
    // over: since the boundary pin point carries its component segment
    // (CIMP §1 U127, batch b3616), the four *pins* lie in four distinct nets —
    // one pin, one identity, and the junction runs both of its segments on it.
    // The declared member ports above stay what the fold made them: labels.
    let pin_of = |k: &str| -> u32 {
        let path = format!("main.MCU513.UC.{k}");
        table
            .get_id_by_path(&path)
            .unwrap_or_else(|| panic!("the board spells the boundary pin {path}"))
    };
    let nets = table.get_nets();
    let nets_of = |id: &u32| -> Vec<usize> {
        nets.iter()
            .enumerate()
            .filter(|(_, n)| n.points.contains(id))
            .map(|(i, _)| i)
            .collect()
    };
    let pins: Vec<u32> = ["8", "9", "10", "11"].iter().map(|k| pin_of(k)).collect();
    // A boundary pin is a junction: the inner segment's net and the outer
    // segment's net both claim it, so each pin sits in exactly two nets.
    let mut landed: HashSet<usize> = HashSet::new();
    for id in &pins {
        let hits = nets_of(id);
        assert_eq!(
            hits.len(),
            2,
            "a boundary pin joins its two segments: {hits:?}"
        );
        landed.extend(hits);
    }
    // And no two pins share either segment — that distinctness is exactly
    // what the old coarse fallback destroyed (four lanes, one shorted net).
    assert_eq!(
        landed.len(),
        2 * pins.len(),
        "the four SPI pins must not share a net on either segment"
    );
    for i in landed {
        let net = &nets[i];
        assert!(
            net.points.len() >= 2,
            "net '{}' carries {} point(s): {:?}",
            net.name,
            net.points.len(),
            net.points
        );
    }

    // The member rows carry the label, not the connection: the boundary
    // junction anchors the component pin, so a declared member port sits in no
    // net at all — the pin row above is the one the net reads.
    for id in &declared {
        let hits = nets.iter().filter(|n| n.points.contains(id)).count();
        assert_eq!(
            hits, 0,
            "a declared boundary member carries no net claim: {hits} rows"
        );
    }
}
