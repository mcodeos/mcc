// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §4.6 C-3 — per-edge truth is never overwritten by an aggregate.
//!
//! R0's obligation downstream of `eval_chain` (unified-core-design.md
//! §4.6 C-3): every `ConnectionInst` carries a per-edge `dir`/`op`, and that
//! truth must survive to the vector layer. The aggregates the vector layer
//! derives (`Trunk.dir`, `NetShape.dir`) are **projections** of those per-edge
//! values — the majority vote — never a replacement for them, and
//! de-duplication may not drop a connection merely because another shares its
//! point set with a different arrow.
//!
//! Two locks, both on vector-layer products (not the netlist):
//!
//! * trunk `dir` is the majority of its member-lane directions, not the first
//!   member's (`visit.rs` post-pass): the fixture writes the first lane
//!   Undirected and the second `->`, so a "first wins" implementation would
//!   report Undirected;
//! * `dedup_connections` keys on the unordered point set **plus** `dir`/`op`
//!   (`phases.rs`): the fixture writes the same pair of points twice with `-`
//!   then `->`, so a point-set-only key would drop the `->` edge and the
//!   surviving net could only be Undirected.

mod common;

use mcc::{ConnDir, McIds, McURI};

/// A two-member bus port; the members are wired individually so their lanes
/// carry different arrows.
const MIXED_DIR_SOURCE: &str = r#"
component SPI_DEV
{
    pins = [
        io [1:2] = SPI{SCLK, MOSI}
        ps 3 = GND
    ]
}
module main(ps GND)
{
    SPI_DEV U1
    SPI_DEV U2
    U1.SPI.SCLK - U2.SPI.SCLK
    U1.SPI.MOSI -> U2.SPI.MOSI
}
"#;

/// The same pair of points written twice: first Undirected, then `->`.
const DUP_DIR_SOURCE: &str = r#"
component RES2
{
    pins = [
        1 = 1
        2 = 2
    ]
}
module main
{
    RES2 R1
    RES2 R2
    R1.1 - R2.1
    R1.1 -> R2.1
}
"#;

fn build_block(source: &str) -> mcc::vector::model::McVecBlock {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/vec-per-edge-truth.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (inst, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&entry.ident, &uri, 1).expect("pass2_flat failed");
    mcc::vector::builder::visit::build_mc_vec(&inst, &table, &arena, &store)
}

#[test]
fn trunk_dir_is_the_majority_of_its_member_lanes_not_the_first() {
    let block = build_block(MIXED_DIR_SOURCE);
    let trunks = &block.port_trunks;
    assert_eq!(trunks.len(), 1, "expected one SPI trunk, got {trunks:#?}");
    let trunk = &trunks[0];
    assert_eq!(trunk.name, "U1");
    assert_eq!(trunk.members.len(), 2, "expected SCLK + MOSI lanes");

    // The per-lane truths are preserved: each member keeps the arrow its own
    // connection was written with (mixed here), never the trunk aggregate.
    let dirs: Vec<(String, ConnDir)> = trunk
        .members
        .iter()
        .map(|m| (m.member.clone(), m.dir))
        .collect();
    assert_eq!(
        dirs,
        vec![
            ("SCLK".to_string(), ConnDir::Undirected),
            ("MOSI".to_string(), ConnDir::LtoR),
        ],
        "each member lane must carry its own written direction"
    );

    // The trunk aggregate is the majority projection. Undirected is not a
    // vote, so LtoR wins 1-0 — an implementation that took the first member
    // (Undirected) would fail here.
    assert_eq!(
        trunk.dir,
        ConnDir::LtoR,
        "trunk dir must be the member-lane majority, not the first lane"
    );
}

#[test]
fn dedup_keeps_dir_variant_edges_over_the_same_point_set() {
    let block = build_block(DUP_DIR_SOURCE);
    let net = block
        .nets
        .iter()
        .find(|n| n.nets.len() == 2)
        .unwrap_or_else(|| panic!("expected the merged two-point net, got {:#?}", block.nets));
    let shape = net.shape.as_ref().expect("net shape");
    // Both edges survive de-duplication, so the survivor's direction is the
    // majority of `-` (Undirected) and `->` (LtoR): LtoR. A point-set-only
    // de-dup key would have dropped the `->` edge and left Undirected.
    assert_eq!(
        shape.dir,
        ConnDir::LtoR,
        "the `->` edge must survive alongside the `-` edge"
    );
}
