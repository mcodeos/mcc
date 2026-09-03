// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §11.5.2 read-side structural query API (dianlu-tree-architecture-plan §11.5.2).
//!
//! A small query layer sitting on arena + lanes + nets so consumers (LSP /
//! drawing / ERC) read through it uniformly instead of re-walking the
//! recursive tree: `point.net()` / `net.points()` / `net.fanout(point)` /
//! `lane.owner_trunk()` / module subtree walk. These tests lock the surface
//! on real `DianLu` builds.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::McIds;

/// Build a `DianLu` for `src` and return it. The caller must hold the lock
/// from [`common::lock`].
fn build_dianlu(src: &str) -> mcc::DianLu {
    let uri = "/mcc/read-api.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let ident = McIds::from("main");
    mcc::mcc_build_dianlu(&ident, &uri, 1000).expect("mcc_build_dianlu")
}

/// T4 read-side: a component pin's stable member id. Prefers the registry
/// ledger (the lane layer's authority); falls back to declaration order for
/// defs that never reached the world registry (same fallback as `lane.rs`).
fn pin_member_id(comp: &mcc::McComponentInst, name: &str) -> Option<mcc::DefMemberId> {
    if !comp.def.uri.is_empty() {
        let sn = mcc::McSpaceName::new(&comp.def.name, comp.def.uri.clone());
        if let Some(id) = mcc::def_member_id_of(&sn, mcc::DefKind::Component, name) {
            return Some(id);
        }
    }
    comp.def
        .pins
        .decl_order
        .iter()
        .position(|pid| pid == name)
        .map(|ord| mcc::DefMemberId(ord as u32))
}

/// `point.net()` and `net.fanout(point)`: a vector-receiver dispatch
/// `c[1:2].Cap([VDD, GND])` (§7.6) unions each member pin with its scalar
/// endpoint. The VDD net holds VDD + c1.1 + c2.1; every one of those points
/// resolves back to that net, and fanout is exactly the net's member set.
#[test]
fn dlu_read__point_net_and_fanout() {
    let _lock = common::lock();
    common::reset();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([n1, n2]) {
        n1 - this - n2
    }
}
module main {
    io VDD
    io GND
    CAP c[1:2](1)
    c[1:2].Cap([VDD, GND])
}";
    let dl = build_dianlu(src);
    let tree = dl.tree();
    let view = mcc::TreeView::new(dl.arena(), dl.store());
    let c1 = view.components(tree).find(|c| c.name == "c1").unwrap();
    let c2 = view.components(tree).find(|c| c.name == "c2").unwrap();
    let pin1 = pin_member_id(c1, "1").unwrap();
    let vdd_ord = tree.ports.iter().position(|p| p.name == "VDD").unwrap();
    let p_vdd = mcc::PointId {
        node: tree.node_id.unwrap(),
        pin: mcc::DefMemberId(vdd_ord as u32),
    };
    let p_c1_1 = mcc::PointId {
        node: c1.node_id.unwrap(),
        pin: pin1,
    };
    let p_c2_1 = mcc::PointId {
        node: c2.node_id.unwrap(),
        pin: pin1,
    };

    let vdd_net = dl.point_net(p_vdd).expect("VDD point resolves to a net");
    assert_eq!(vdd_net.points().len(), 3, "VDD + c1.1 + c2.1");
    for p in [p_vdd, p_c1_1, p_c2_1] {
        assert!(vdd_net.points().contains(&p), "net contains {p}");
        // Every member of the net resolves back to the same net.
        assert!(
            dl.point_net(p).is_some_and(|n| n.id == vdd_net.id),
            "{p} resolves to the VDD net"
        );
        // Fanout is exactly the net's member set.
        let fanout = dl.point_fanout(p).expect("fanout of a net member");
        assert_eq!(fanout, vdd_net.points(), "fanout of {p} equals net points");
    }
}

/// `lane.owner_trunk()`: a vector-receiver dispatch statement's lanes all
/// belong to the one statement trunk, resolve back to it by containment, and
/// the `(trunk, ordinal)` LaneRef spelling (`lane`) returns the same lanes in
/// order.
#[test]
fn dlu_read__lane_owner_trunk() {
    let _lock = common::lock();
    common::reset();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([n1, n2]) {
        n1 - this - n2
    }
}
module main {
    io VDD
    io GND
    CAP c[1:2](1)
    c[1:2].Cap([VDD, GND])
}";
    let dl = build_dianlu(src);
    let trunks = dl.lanes();
    assert_eq!(trunks.len(), 1, "one trunk for the one dispatch statement");
    let trunk = &trunks[0];
    assert!(!trunk.lanes.is_empty(), "the dispatch emits lanes");

    for (ord, lane) in trunk.lanes.iter().enumerate() {
        let owner = dl.lane_owner_trunk(lane).expect("lane has an owning trunk");
        assert_eq!(
            owner.id, trunk.id,
            "lane {ord} belongs to the statement trunk"
        );
        let ref_lane = dl
            .lane(trunk.id, ord)
            .expect("(trunk, ordinal) LaneRef resolves");
        assert_eq!(
            ref_lane, lane,
            "LaneRef {}:{ord} returns the lane",
            trunk.id
        );
    }
}

/// Module subtree walk + arena edges: a two-level tree (`main` → `r1` → `s1`
/// plus `main` → `c1`) — the walk from the root visits every node id, and
/// `children`/`parent` expose the arena edges.
#[test]
fn dlu_read__module_subtree_and_edges() {
    let _lock = common::lock();
    common::reset();
    let src = "\
component RES(res::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
}
module SUB {
    RES s1
}
module main {
    SUB r1
    RES c1
}";
    let dl = build_dianlu(src);
    let tree = dl.tree();
    let view = mcc::TreeView::new(dl.arena(), dl.store());
    let root = tree.node_id.unwrap();
    let r1 = view.sub_modules(tree).find(|s| s.name == "r1").unwrap();
    let c1 = view.components(tree).find(|c| c.name == "c1").unwrap();
    let s1 = view.components(r1).find(|c| c.name == "s1").unwrap();

    let subtree = dl.module_subtree(root);
    assert_eq!(subtree[0], root, "walk starts at the root");
    for (label, id) in [
        ("r1", r1.node_id.unwrap()),
        ("c1", c1.node_id.unwrap()),
        ("s1", s1.node_id.unwrap()),
    ] {
        assert!(subtree.contains(&id), "subtree contains {label}");
    }

    // Arena edges expose the parent/child structure.
    assert!(dl.children(root).unwrap().contains(&r1.node_id.unwrap()));
    assert!(dl.children(root).unwrap().contains(&c1.node_id.unwrap()));
    assert_eq!(dl.parent(r1.node_id.unwrap()), Some(root));
    assert_eq!(dl.parent(s1.node_id.unwrap()), r1.node_id);
    assert_eq!(dl.parent(c1.node_id.unwrap()), Some(root));
}
