// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! split-ground batch2 (viz projection face) — viz-level locks for the
//! declared-identity ground retirement (classification-retirement-design §5 +
//! split-ground-copper-design v0.2 §7.1) on the hbl fixture.
//!
//! The flat-layer `split_ground_nets` per-statement partition is RETIRED as of
//! the terminal batch (v0.2 §6 — it no longer exists in builder.rs), and the
//! per-(copper×statement) drawing-axis glyph re-anchor is deferred to a later
//! viz batch. What is locked here is the projection's ground classification +
//! rule-c boundary semantics:
//!
//! ① Declared ground nets merge / classify as Ground through `detect_net_attr`
//!    (Ret/Reference), never by the net name — the `main` ground conductor
//!    `V3V3.GND` is one projected Ground net (rule-a union of the bare `GND` +
//!    `V1V2.GND` + `V3V3.GND` raw nets).
//! ② Undeclared power-NAME nets never classify Ground and never pull a rail into
//!    the conductor — the scalar-header LDO body (`in vin::DC(5V)` ports carry no
//!    member role) keeps `vin.GND`/`vout.VCC` as plain Signal, and its sub-block
//!    ground tie is no longer guessed by name: `V5V.GND` stays a separate declared
//!    Ground net rather than merging into `V3V3.GND` (§7.1 accepted consequence —
//!    migrates back once the LDO boundary is declared as a ::DC pair, cf. mcs hbl).
//! ③ Rule (c): a pseudo endpoint is dropped (rail boundary declaration) iff the
//!    GROUP resolves to a declared supply identity; Signal groups keep their
//!    pseudo endpoints as Boundary / PortTerminal markers (§5⑥ — a scalar
//!    power-name port like `in VDD_3V3` projects to Signal + Boundary, not rail).

use std::path::PathBuf;

use mcc::vector::model::{AttrRole, McVecBlock};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// The mcc_* workspace is global state; tests must be serialized (same as tests/rail_rules.rs).
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_projected() -> (McVecBlock, mcc::InstTable) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let raw = mcc::vector::builder::visit::build_mc_vec(&tree, &table, &arena, &store);
    let (projected, _log) = mcc::viz::project::project_block_tree(&raw, &table);
    (projected, table)
}

fn net<'a>(layer: &'a McVecBlock, name: &str) -> &'a mcc::vector::model::McVecNet {
    layer
        .nets
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("layer '{}' should contain net '{}'", layer.name, name))
}

/// ① + ②: the projection's ground classification is declared-identity, never name.
///
/// main's ground plane is exactly two declared Ground nets after the batch:
/// the merged `V3V3.GND` conductor (bare `GND` + `V1V2.GND` + `V3V3.GND` — the
/// 3.3V/1.2V return copper) and a separate `V5V.GND` (5V return). The sub-block
/// tie that used to glue V5V.GND into V3V3.GND ran through the LDO body, whose
/// scalar header exposes no declared ground member — with name guessing retired,
/// that tie is no longer asserted (§7.1). Both nets carry the Ret mirror.
#[test]
fn main_ground_plane_is_declared_return_nets_only() {
    let (main, table) = build_projected();

    let v33 = net(&main, "V3V3.GND");
    assert_eq!(
        v33.attr.as_ref().expect("declared rail attr").role,
        AttrRole::Ret
    );
    let v5 = net(&main, "V5V.GND");
    assert_eq!(
        v5.attr.as_ref().expect("declared rail attr").role,
        AttrRole::Ret
    );

    // The raw bare `GND` label net was absorbed into V3V3.GND (rule-a union),
    // so no un-merged bare `GND` net survives on main, and no net is named from
    // a bare ground keyword that has no declaration behind it.
    assert!(
        main.nets.iter().all(|n| n.name != "GND"),
        "bare 'GND' net must have been merged into the V3V3.GND conductor"
    );

    // Every Ground/Ret net on main is a *declared* return — and exactly the two
    // expected ones (never a name-guessed third).
    let grounds: Vec<&str> = main
        .nets
        .iter()
        .filter(|n| {
            matches!(
                n.attr.as_ref().map(|a| &a.role),
                Some(AttrRole::Ret) | Some(AttrRole::Reference)
            )
        })
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(
        grounds,
        vec!["V3V3.GND", "V5V.GND"],
        "main ground-plane roster must be exactly [V3V3.GND, V5V.GND]"
    );
    let _ = table;
}

/// ②: the scalar-header LDO body (power-NAME ports, no member role, no conduit/
/// rail declaration of its own) has zero Ground/Power nets — its `vin.GND`
/// stays a plain Signal even though the leaf says "GND".
#[test]
fn undeclared_ldo_power_names_are_signal_not_ground() {
    let (main, _table) = build_projected();
    let ldo = main
        .blocks
        .iter()
        .find(|b| b.name == "LDO")
        .expect("hbl has an LDO sub-layer");

    for want in ["vin.VCC", "vout.VCC", "vin.GND"] {
        let n = net(ldo, want);
        assert!(
            n.attr.is_none(),
            "net '{want}': scalar power-name port with no declaration must stay Signal (no attr)"
        );
    }
    assert!(
        ldo.nets.iter().all(|n| n.attr.is_none()),
        "nothing in the LDO layer may be classified rail without a declaration"
    );
}

/// ③ (rule c / §5⑥): a pseudo endpoint is dropped as a rail boundary only when
/// the GROUP is a declared supply identity. Declared rail nets (main's V5V.VCC,
/// MIC's dc.GND) drop their pseudo boundary → boundary None. Signal groups keep
/// their pseudo endpoints as Boundary / PortTerminal markers — including the
/// LDO's scalar power-NAME ports (`vin.GND`, `vout.VCC`), which are the §5⑥
/// specimen: they project to Signal + Boundary, never to a rail net.
#[test]
fn scalar_power_name_boundary_is_portterminal_and_rail_drops_pseudo() {
    let (main, _table) = build_projected();

    // Declared rail nets drop every pseudo endpoint (net name, not connection).
    for rail_net in ["V5V.VCC", "V3V3.VCC", "V1V2.VCC", "V3V3.GND", "V5V.GND"] {
        assert!(
            net(&main, rail_net).boundary.is_none(),
            "declared rail net '{rail_net}' must not carry a PortTerminal boundary"
        );
    }

    // §5⑥: scalar power-NAME boundary ports stay Signal + Boundary (PortTerminal).
    let ldo = main.blocks.iter().find(|b| b.name == "LDO").expect("LDO");
    for want in ["vin.VCC", "vout.VCC", "vin.GND"] {
        let n = net(ldo, want);
        assert!(
            n.boundary.is_some(),
            "LDO scalar power-name net '{want}' must keep its pseudo endpoint as a Boundary (PortTerminal), got none"
        );
    }
}
