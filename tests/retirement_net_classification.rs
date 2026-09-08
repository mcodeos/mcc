// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §5③-⑤ synthetic locks (classification-retirement-design) on the hbl fixture.
//!
//! Rulings locked here:
//! ① no declaration → the net stays `Signal` — even when its **name** is `VCC`/`GND`.
//! ③ the drawing side's power/ground classification is driven by the net's declared
//!    `attr` role, never by re-guessing the name; `attr: None` ⇒ never `Ground`/`Power`.
//!
//! hbl draws rail glyphs per declared ground/power **net** in the Device pipeline
//! (one glyph per net — see tests/rail_rules.rs), not via `Symbol::PowerRail` boxes, so
//! the observable lock is `VizNet.kind` ↔ `VizNet.attr` agreement across every layer.
//! The flatten-level member-role side (ret member → Ground etc.) is already locked by
//! tests/member_role_declared_identity.rs.

use std::path::PathBuf;

use mcc::vector::graph::{McVecGraph, NetKind};
use mcc::vector::model::{AttrRole, NetAttrMirror};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// The mcc_* workspace is global state; tests must be serialized (same as tests/rail_rules.rs).
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_graph() -> McVecGraph {
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table, &arena, &store);
    mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table)
}

fn find_layer<'a>(g: &'a McVecGraph, name: &str) -> Option<&'a McVecGraph> {
    if g.name == name {
        return Some(g);
    }
    g.sub_graphs.iter().find_map(|s| find_layer(s, name))
}

fn all_layers<'a>(g: &'a McVecGraph, out: &mut Vec<&'a McVecGraph>) {
    out.push(g);
    for s in &g.sub_graphs {
        all_layers(s, out);
    }
}

fn net<'a>(g: &'a McVecGraph, name: &str) -> &'a mcc::vector::graph::VizNet {
    g.nets
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("layer '{}' should contain net '{}'", g.name, name))
}

/// Ruling ① at the net level: the LDO body (an IC whose `vin{VCC,GND}` header group is a
/// non-DC IO group and which declares no conduit/rail) has three power-**named** nets. Without
/// a declaration they must all stay `Signal` / `attr: None` — and nothing in that layer may be
/// judged Ground/Power by name.
#[test]
fn undeclared_power_named_nets_are_signal() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let graph = build_graph();
    let ldo = find_layer(&graph, "LDO").expect("hbl has an LDO sub-layer");

    for want in ["vin.VCC", "vout.VCC", "vin.GND"] {
        let n = net(ldo, want);
        assert_eq!(
            n.kind,
            NetKind::Signal,
            "net '{want}': a power-NAME net with no declaration must stay Signal (ruling ①)"
        );
        assert!(
            n.attr.is_none(),
            "net '{want}': attr must be None when no declaration anchors the net"
        );
    }

    for n in &ldo.nets {
        assert!(
            !matches!(n.kind, NetKind::Ground | NetKind::Power),
            "LDO layer net '{}': nothing may be judged Ground/Power without a declaration",
            n.name
        );
    }
}

/// Declared rails cross the root layer (module-scope domain/rail sets) as Ground/Power nets,
/// carrying an attr mirror whose `role` says which side — the name of the rail only lands in
/// `copper`. NetKind must follow the declared role.
#[test]
fn declared_rails_kind_follows_attr_role() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let graph = build_graph();

    // Declared ground side -> NetKind::Ground, attr role Ret.
    let gnd = net(&graph, "V3V3.GND");
    assert_eq!(gnd.kind, NetKind::Ground);
    let a: &NetAttrMirror = gnd
        .attr
        .as_ref()
        .expect("declared rail net must carry an attr mirror");
    assert_eq!(a.role, AttrRole::Ret);
    assert!(a.resolvable);

    // Declared hot sides -> NetKind::Power, attr role Hot. (V3V3.VCC's declared copper is
    // VDD_3V3 — a different name — proving kind comes from the declaration, not the net label.)
    let vcc = net(&graph, "V3V3.VCC");
    assert_eq!(vcc.kind, NetKind::Power);
    assert_eq!(vcc.attr.as_ref().expect("attr").role, AttrRole::Hot);
    assert_eq!(
        vcc.attr.as_ref().unwrap().copper.as_deref(),
        Some("VDD_3V3")
    );

    let v1v2 = net(&graph, "V1V2.VCC");
    assert_eq!(v1v2.kind, NetKind::Power);
    assert_eq!(v1v2.attr.as_ref().expect("attr").role, AttrRole::Hot);
}

/// Global invariant across every layer: `kind` is a pure function of the declared `attr`
/// role. No net may be `Ground`/`Power` without a resolvable attr declaring that role, and no
/// attr may claim a role the net's kind contradicts.
#[test]
fn kind_is_pure_function_of_declared_attr() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let graph = build_graph();

    let mut layers = Vec::new();
    all_layers(&graph, &mut layers);
    assert!(
        layers.len() >= 6,
        "expected main + 6 device sub-layers, got {}",
        layers.len()
    );

    for g in &layers {
        for n in &g.nets {
            let ctx = format!("layer '{}' net '{}'", g.name, n.name);
            match &n.attr {
                None => {
                    assert!(
                        !matches!(n.kind, NetKind::Ground | NetKind::Power),
                        "{ctx}: no declaration -> must not be classified Ground/Power (ruling ①/③)"
                    );
                }
                Some(a) => {
                    assert!(
                        a.resolvable,
                        "{ctx}: attr mirror is only ever produced resolvable"
                    );
                    let expected = match a.role {
                        AttrRole::Ret | AttrRole::Reference => NetKind::Ground,
                        AttrRole::Hot => NetKind::Power,
                        AttrRole::Signal => NetKind::Signal,
                    };
                    assert_eq!(
                        n.kind, expected,
                        "{ctx}: kind must follow the declared attr role"
                    );
                }
            }
        }
    }
}
