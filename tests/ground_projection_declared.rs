// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Declared-ground projection (viz face) — viz-level locks for the
//! declared-identity ground retirement (classification-retirement-design §5 +
//! split-ground-copper-design v0.2 §7.1) on the hbl fixture.
//!
//! The flat-layer per-statement ground partition is RETIRED as of the terminal
//! batch (v0.2 §6 — it no longer exists in builder.rs), and the
//! per-(copper×statement) drawing-axis glyph re-anchor is deferred to a later
//! viz batch. What is locked here is the projection's ground classification +
//! rule-c boundary semantics:
//!
//! ① Declared ground nets merge / classify as Ground through `detect_net_attr`
//!    (Ret/Reference), never by the net name — the `main` ground conductor `GND`
//!    is one projected Ground net (rule-a union of the bare `GND` + `V1V2.GND` +
//!    `V3V3.GND` + `V5V.GND` raw nets). `V5V.GND` is IN this conductor: its
//!    tie closed when the LDO boundary became a declared `::DC` pair. See ②.
//! ② The §7.1 accepted consequence has MIGRATED BACK, as the design predicted
//!    ("migrates back once the LDO boundary is declared as a ::DC pair, cf. mcs
//!    hbl"). A `::DC` face pair is a property of the declaration, not of the
//!    spelling: the scalar header `in vin::DC(5V)` / `out vout::DC(3.3V)` brings
//!    the same supply/return faces over from the DC interface's own pin table
//!    that `psnk vin{V5V, GND}::DC(5V)` writes out, so the LDO body's nets are
//!    declared and the sub-block ground tie holds structurally — no name guess.
//! ③ Rule (c): a pseudo endpoint is dropped (rail boundary declaration) iff the
//!    GROUP resolves to a declared supply identity; Signal groups keep their
//!    pseudo endpoints as Boundary / PortTerminal markers (§5⑥ — a scalar
//!    power-name port like `in VDD_3V3` projects to Signal + Boundary, not rail;
//!    a *port* of a declared rail net keeps its boundary frame, which is why the
//!    sub-layers' rail nets carry one while main's `GND` pseudo label does not).

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
/// main's ground plane is exactly ONE declared Ground net: the merged `GND`
/// conductor (bare `GND` + `V1V2.GND` + `V3V3.GND` + `V5V.GND`). The 5V return
/// is in it because the sub-block tie that used to be unassertable is now a
/// declaration: the LDO's scalar header `in vin::DC(5V)` declares its return
/// face, which is the same copper its `vout` face returns on. This is the §7.1
/// migration the design predicted for this fixture (cf. mcs hbl, which had
/// already migrated to the written-pair spelling).
#[test]
fn main_ground_plane_is_declared_return_nets_only() {
    let (main, table) = build_projected();
    let gnd = net(&main, "GND");
    assert_eq!(
        gnd.attr.as_ref().expect("declared rail attr").role,
        AttrRole::Ret
    );

    // Every Ground/Ret net on main is a *declared* return — and exactly the one
    // expected conductor (never a name-guessed second).
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
        vec!["GND"],
        "main ground-plane roster must be exactly [GND] — one return copper"
    );

    // The per-rail return spellings are all the SAME copper now, so none of
    // them survives as a net of its own.
    for absorbed in ["V3V3.GND", "V5V.GND", "V1V2.GND"] {
        assert!(
            main.nets.iter().all(|n| n.name != absorbed),
            "per-rail return '{absorbed}' must have merged into the single GND conductor"
        );
    }
    let _ = table;
}

/// ②: the scalar-header LDO body now DECLARES its faces. Its nets are classified
/// by the declared `::DC` face each one is — and the roles come from the face's
/// position in the interface's pin table, so the layer classifies identically
/// whichever spelling declared it.
#[test]
fn scalar_ldo_header_declares_its_faces() {
    let (main, _table) = build_projected();
    let ldo = main
        .blocks
        .iter()
        .find(|b| b.name == "LDO")
        .expect("hbl has an LDO sub-layer");

    let expected = [
        ("vin.VCC", AttrRole::Hot),
        ("vout.VCC", AttrRole::Hot),
        ("vin.GND", AttrRole::Ret),
    ];
    for (want, role) in expected {
        let n = net(ldo, want);
        assert_eq!(
            n.attr.as_ref().map(|a| a.role.clone()),
            Some(role),
            "net '{want}': a scalar ::DC header declares its faces"
        );
    }
    // Nothing here is judged by its name: the layer declares exactly its faces
    // and nothing else. (`kind` ↔ `attr` agreement across every layer is the
    // global invariant in tests/retirement_net_classification.rs.)
    let classified: Vec<&str> = ldo
        .nets
        .iter()
        .filter(|n| n.attr.is_some())
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(
        classified,
        vec!["vin.VCC", "vout.VCC", "vin.GND"],
        "LDO's classified nets must be exactly its declared ::DC faces"
    );
}

/// ③ (rule c / §5⑥): a pseudo endpoint is dropped as a rail boundary only when
/// the GROUP is a declared supply identity. Declared rail nets drop the pseudo
/// endpoints that are the layer's own *names*; a declared rail net reached
/// through a real **port** keeps its boundary frame — the port is a port, and a
/// boundary drawn around this module must show it.
#[test]
fn rail_nets_drop_pseudo_endpoints_but_keep_port_frames() {
    let (main, _table) = build_projected();

    // Declared rail nets drop every pseudo endpoint (net name, not connection).
    // main's return is one conductor now, named by the bare `GND` label.
    for rail_net in ["V5V.VCC", "V3V3.VCC", "V1V2.VCC", "GND"] {
        assert!(
            net(&main, rail_net).boundary.is_none(),
            "declared rail net '{rail_net}' must not carry a PortTerminal boundary"
        );
    }

    // The LDO's named ports are real ports of a declared rail net — the frame
    // stays (their endpoints are Port members, not the layer's own bare names).
    let ldo = main.blocks.iter().find(|b| b.name == "LDO").expect("LDO");
    for want in ["vin.VCC", "vout.VCC", "vin.GND"] {
        let n = net(ldo, want);
        assert!(
            n.boundary.is_some(),
            "LDO declared rail net '{want}' must keep the boundary frame for its port, got none"
        );
    }
}
