// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §5③-⑤ synthetic locks (classification-retirement-design) on the hbl fixture.
//!
//! Rulings locked here:
//! ① no declaration → the net stays `Signal` — even when its **name** is `VCC`/`GND`.
//!    (The hbl fixture no longer carries that specimen: its once-undeclared scalar
//!    `::DC` header now declares its faces. The name-trap lock lives at unit level
//!    in `member_role_declared_identity::legacy_members_have_no_role`, where the
//!    member really is named `GND`/`VDD` — a stronger specimen than a leaf spelling.)
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

/// Ruling ③ at the net level, on the LDO body whose header is a **scalar**
/// `::DC` port pair (`in vin::DC(5V)` / `out vout::DC(3.3V)`).
///
/// A `::DC` contract declares a supply face and a return face, and the faces
/// sit at the declared positions in the interface's own pin table — so these
/// nets are *declared*, and their classification follows the declared face,
/// never the net's leaf spelling. Identity is spelling-independent: the same
/// contract written `[VCC, GND]::DC(3.3V)` declares the same two faces.
#[test]
fn scalar_dc_header_faces_are_declared_not_name_guessed() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let graph = build_graph();
    let ldo = find_layer(&graph, "LDO").expect("hbl has an LDO sub-layer");

    let expected = [
        ("vin.VCC", AttrRole::Hot, NetKind::Power),
        ("vout.VCC", AttrRole::Hot, NetKind::Power),
        ("vin.GND", AttrRole::Ret, NetKind::Ground),
    ];
    for (want, role, kind) in expected {
        let n = net(ldo, want);
        assert_eq!(
            n.kind, kind,
            "net '{want}': kind must follow its declared ::DC face"
        );
        let a = n
            .attr
            .as_ref()
            .unwrap_or_else(|| panic!("net '{want}': a declared ::DC face anchors an attr"));
        assert_eq!(
            a.role, role,
            "net '{want}': the declared face position decides the role, not the leaf name"
        );
        assert!(
            a.resolvable,
            "net '{want}': attr mirror is produced resolvable"
        );
    }

    // Every classified net in the LDO layer is one of the declared faces — no
    // net in this layer is judged Ground/Power without a declaration behind it.
    for n in &ldo.nets {
        match n.attr.as_ref().map(|a| &a.role) {
            None => assert!(
                !matches!(n.kind, NetKind::Ground | NetKind::Power),
                "LDO layer net '{}': no declaration -> must not be Ground/Power (ruling ①/③)",
                n.name
            ),
            Some(AttrRole::Ret) | Some(AttrRole::Reference) => {
                assert_eq!(n.kind, NetKind::Ground, "LDO net '{}'", n.name)
            }
            Some(AttrRole::Hot) => {
                assert_eq!(n.kind, NetKind::Power, "LDO net '{}'", n.name)
            }
            Some(AttrRole::Signal) => {
                assert_eq!(n.kind, NetKind::Signal, "LDO net '{}'", n.name)
            }
        }
    }
}

/// Declared rails cross the root layer (module-scope domain/rail sets) as Ground/Power nets,
/// carrying an attr mirror whose `role` says which side — the name of the rail only lands in
/// `copper`. NetKind must follow the declared role.
#[test]
fn declared_rails_kind_follows_attr_role() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let graph = build_graph();

    // Declared ground side -> NetKind::Ground, attr role Ret. Every return in
    // this design is the same copper inside the modules that face each other
    // (the LDO's `vin` and `vout` both declare their 2nd face the return), so
    // main has ONE ground conductor, carrying the bare `GND` label it is named
    // by — not one per rail.
    let gnd = net(&graph, "GND");
    assert_eq!(gnd.kind, NetKind::Ground);
    let a: &NetAttrMirror = gnd
        .attr
        .as_ref()
        .expect("declared rail net must carry an attr mirror");
    assert_eq!(a.role, AttrRole::Ret);
    assert!(a.resolvable);

    // Declared hot sides -> NetKind::Power, attr role Hot. The net label
    // (`V3V3.VCC`) and the declared copper are different names, proving kind
    // comes from the declaration, not the net label.
    //
    // Fidelity gap (found here, not fixed here): the copper is the DC
    // interface's GENERIC face spelling `VCC`, where the call site's argument
    // selects a more precise one. A scalar `x::DC(v)` port takes its member
    // names from `iface.base.pins` — the interface class *before* its
    // parameters are bound, i.e. whichever `pins = […]` branch is last in the
    // body (ifs/dc.mc's final `else`, the generic fallback) — rather than the
    // branch the argument selects (`VCC3V3`). The names were decorative while
    // the members had no role; now that a scalar `::DC` port declares its
    // faces, they land in identity. Binding the interface before reading its
    // pin table is a separate change outside the drawing batch.
    let vcc = net(&graph, "V3V3.VCC");
    assert_eq!(vcc.kind, NetKind::Power);
    assert_eq!(vcc.attr.as_ref().expect("attr").role, AttrRole::Hot);
    assert_eq!(vcc.attr.as_ref().unwrap().copper.as_deref(), Some("VCC"));

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
