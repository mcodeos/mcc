// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! DianLu — the core circuit object (design §12.2).
//!
//! One instantiation = one `DianLu`: the instance tree (with the vector
//! grouping nodes) plus a lazily derived flat projection. `flatten()` is the
//! single one-way projection exit — it derives the flat `InstTable` from the
//! already-built tree and never re-enters instantiation, so an
//! instantiation-side diagnostic fires exactly once no matter how many times
//! the projection is taken. These tests lock that contract.

use mcc::McIds;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

/// Reset the mcc_* workspace for one test. The caller must hold `TEST_LOCK`.
fn reset_workspace() {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(Path::new(""));
    mcc::mcc_clear_workspace();
}

/// Build a `DianLu` for `src` and return it. The caller must hold `TEST_LOCK`.
fn build_dianlu(src: &str) -> mcc::DianLu {
    let uri = "/mcc/dianlu.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let ident = McIds::from("main");
    mcc::mcc_build_dianlu(&ident, &uri, 1000).expect("mcc_build_dianlu")
}

/// A 0-pin net statement fires GAP2 (E4057) during instantiation
/// (`add_connection`), exactly once for a single `DianLu` — and twice calling
/// `flatten()` must NOT re-instantiate (the old `mcb_pass2_flat` re-ran the
/// whole instantiation, doubling E4057 until `has_code_at` dedup papered over
/// it; the structural fix is that flatten never re-enters instantiation).
#[test]
fn single_instantiation_gap2_fires_once_after_two_flattens() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let mut dl =
        build_dianlu("module main {\n    func main() {\n        res[1:2] -> led[3:4]\n    }\n}");

    let _ = dl.flatten();
    let t1 = dl.table().unwrap() as *const _;
    let _ = dl.flatten();
    let t2 = dl.table().unwrap() as *const _;
    assert!(
        std::ptr::eq(t1, t2),
        "flatten is cached — one projection only, never a re-derivation"
    );

    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert_eq!(
        count(&codes, 4057),
        1,
        "one instantiation → one E4057 even after two flatten() calls; got {codes:?}"
    );
}

/// `into_parts` hands back the tree and the flat projection together; the flat
/// table actually carries the built instance entries.
#[test]
fn into_parts_returns_tree_and_flat_projection() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let mut dl = build_dianlu("module main {\n    io A\n    io GND\n    A -> GND\n}");
    dl.flatten();
    let (tree, table) = dl.into_parts();

    assert!(
        table.len() >= 2,
        "flat projection has instance entries; len={}",
        table.len()
    );
    assert_eq!(
        tree.name, "main",
        "the tree is the instantiated entry module"
    );
}

/// A tree-only consumer never builds the flat projection: `into_tree` discards
/// the table (which is never constructed), so the object is cheap for the
/// `mcc build` (non-flat) path.
#[test]
fn tree_only_consumer_never_projects() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let dl = build_dianlu("module main {\n    io A\n    io GND\n    A -> GND\n}");
    assert!(
        dl.table().is_none(),
        "tree-only consumer: the flat projection is not built"
    );
    let _tree = dl.into_tree();
}

/// Phase C1: every frozen tree node carries a companion `node_id`, and the
/// identity registry rebuilt from the tree (`DianLu::new`) resolves the same
/// canonical paths to the same ids as the tree itself (per-build
/// determinism: same path → same id).
#[test]
fn identity_registry_rebuilt_from_frozen_tree_matches() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let dl =
        build_dianlu("module main {\n    func main() {\n        res[1:2] -> led[3:4]\n    }\n}");
    // The construction-time registry already knows the circuit root.
    let root_id = dl
        .identity()
        .node_id_of("main")
        .expect("registry knows the circuit root path");
    let tree = dl.into_tree();
    // The frozen tree carries the same id on the root node.
    assert_eq!(
        tree.node_id,
        Some(root_id),
        "tree root node_id equals the registry's path id"
    );
    // Every child node also carries a non-empty id.
    for comp in &tree.components {
        assert!(
            comp.node_id.is_some(),
            "component '{}' carries a node id",
            comp.name
        );
    }
    for sub in &tree.sub_modules {
        assert!(
            sub.node_id.is_some(),
            "sub-module '{}' carries a node id",
            sub.name
        );
    }
    // Rebuilding a DianLu from the frozen tree must reproduce the same ids.
    // (identity-only view; the Phase D net-table store starts empty here.)
    let dl2 = mcc::DianLu::new(
        tree,
        1000,
        Rc::new(RefCell::new(mcc::NetTableStore::new())),
    );
    assert_eq!(
        dl2.identity().node_id_of("main"),
        Some(root_id),
        "rebuild from frozen tree keeps the same path -> id mapping"
    );
}

/// Phase C1: `run_submodule_method` re-entry (sub-module instance method
/// call) must carry the circuit-global registry into the lifted sub-builder
/// and write the re-entered products' ids back into the frozen tree — the
/// products carry a non-empty `node_id` and the registry resolves their
/// canonical path.
#[test]
fn submodule_method_reentry_interms_products() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
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
module REG(in VIN) {
    func Add(net) {
        CAP(1).Cap([net, VIN])
    }
}
module main {
    io VDD
    io GND
    REG ldo(VDD)
    ldo.Add(GND)
}";
    let dl = build_dianlu(src);
    let tree = dl.into_tree();
    // The declared sub-module instance carries a circuit-global id.
    let ldo_id = tree
        .sub_modules
        .iter()
        .find(|s| s.name == "ldo")
        .expect("declared sub-module 'ldo'")
        .node_id
        .expect("sub-module 'ldo' carries a node id");
    // The re-entered body's product (auto-named inside `ldo`) carries a
    // non-empty id, and the registry resolves its full canonical path to the
    // same id (the lift's registry was carried in and written back).
    let ldo = tree.sub_modules.iter().find(|s| s.name == "ldo").unwrap();
    let ldo_products: Vec<_> = ldo
        .components
        .iter()
        .map(|c| {
            (
                format!("main.ldo.{}", c.name),
                c.node_id.expect("re-entered product carries a node id"),
            )
        })
        .collect();
    let ldo_conns: Vec<_> = ldo
        .connections
        .iter()
        .map(|c| format!("conn#{}", c.id))
        .collect();
    assert!(
        !ldo_products.is_empty(),
        "re-entered sub-module body produced components; conns={ldo_conns:?}"
    );
    let tree2 = mcc::DianLu::new(
        tree,
        1000,
        Rc::new(RefCell::new(mcc::NetTableStore::new())),
    );
    let reg = tree2.identity();
    for (path, id) in &ldo_products {
        assert_eq!(
            reg.node_id_of(path),
            Some(*id),
            "registry resolves re-entered product '{path}' to its tree id"
        );
        assert_ne!(*id, ldo_id, "product id differs from the sub-module id");
        assert!(
            id.0 > ldo_id.0,
            "product id allocates after the sub-module id"
        );
    }
}

/// Ordered content signature of a flat table: every entry `(path, class,
/// kind)` in id order, then every net `(name, points)` in net-id order.
/// Independent of HashMap iteration order (entries/nets are BTreeMaps), so two
/// independently built tables can be compared for equality.
fn flat_signature(t: &mcc::InstTable) -> Vec<String> {
    let mut out: Vec<String> = t
        .iter()
        .map(|(id, e)| format!("{id}:{}:{}:{:?}", e.path, e.class_name, e.kind))
        .collect();
    out.extend(t.get_nets().iter().map(|n| {
        let points = n
            .points
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("{}:{}:{}", n.id, n.name, points)
    }));
    out
}

/// Phase C two-track consistency: the arena-driven flatten
/// (`DianLu::flatten`, sub-module order sourced from arena `children` edges)
/// and the tree-recursive flatten (`InstTable::from_module_inst`) produce the
/// identical flat projection for the same frozen tree — entries and nets
/// line-for-line. This locks "arena edges drive the traversal with zero
/// projection change" (design §4: the tree is a view over arena edges) and is
/// the template every later consumer switch (export / viz walks) is verified
/// against.
#[test]
fn arena_flatten_matches_tree_recursive_flatten() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    // A sub-module circuit: nesting + components + ports + nets all present,
    // so both traversal paths cover the full flatten projection.
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
module REG(in VIN) {
    func Add(net) {
        CAP(1).Cap([net, VIN])
    }
}
module main {
    io VDD
    io GND
    REG ldo(VDD)
    ldo.Add(GND)
}";
    let mut dl = build_dianlu(src);
    dl.flatten();
    let arena_table = dl.table().expect("flatten ran");
    let tree = dl.tree();
    let tree_table = mcc::InstTable::from_module_inst(tree, 1000, dl.net_store());

    assert_eq!(
        flat_signature(arena_table),
        flat_signature(&tree_table),
        "arena-driven flatten and tree-recursive flatten are line-for-line identical"
    );
}

/// Phase C two-track consistency (viz walk): `build_mc_vec` (tree-recursive)
/// and `build_mc_vec_with_arena` (sub-module order sourced from the arena
/// `children` edges) produce the identical `McVecBlock` tree for the same
/// frozen tree. Debug formatting is compared — same-process builds share the
/// HashMap seed, so any structural drift between the two walks surfaces as a
/// mismatch; the 1:1 alignment guard inside `arena_sub_modules` additionally
/// panics on any arena/tree divergence.
#[test]
fn mcviz_arena_walk_matches_tree_recursive_walk() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
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
module REG(in VIN) {
    func Add(net) {
        CAP(1).Cap([net, VIN])
    }
}
module main {
    io VDD
    io GND
    REG ldo(VDD)
    ldo.Add(GND)
}";
    let uri = "/mcc/dianlu.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let ident = McIds::from("main");
    let (tree, table, arena) =
        mcc::mcc_build_flat_with_arena(&ident, &uri, 1000).expect("mcc_build_flat_with_arena");

    let vec_tree = mcc::build_mc_vec(&tree, &table);
    let vec_arena = mcc::build_mc_vec_with_arena(&tree, &table, &arena);
    assert_eq!(
        format!("{vec_tree:?}"),
        format!("{vec_arena:?}"),
        "arena-driven and tree-recursive McVecBlock walks are identical"
    );
    assert!(
        vec_tree.total_blocks() >= 2,
        "the sub-module circuit produces a nested block tree (got {})",
        vec_tree.total_blocks()
    );
}

/// Phase D: the lane layer collects one structured `Trunk` per connection
/// statement from the frozen tree (design §11.3 ③). Component-pin statements
/// resolve their physical points to `(NodeId, DefMemberId)`; non-component
/// endpoints (module ports / labels) skip their lane without dropping the
/// statement trunk.
#[test]
fn lane_layer_one_trunk_per_connection_statement() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
}
module main {
    io VDD
    io GND
    CAP c1
    CAP c2
    c1.1 -> c2.1
    c1.2 -> GND
}";
    let dl = build_dianlu(src);

    // Two connection statements → two statement trunks, each carrying its
    // source span.
    let trunks = dl.lanes().to_vec();
    assert_eq!(trunks.len(), 2, "one trunk per connection statement");
    assert!(
        trunks.iter().all(|t| t.stmt_span.is_some()),
        "every statement trunk carries its source span"
    );

    // `c1.1 -> c2.1`: both endpoints are component pins in the module scope,
    // so the trunk has one directed lane between the two physical points.
    assert_eq!(trunks[0].lanes.len(), 1, "component-to-component lane");
    let lane = &trunks[0].lanes[0];
    match (&lane.source, &lane.target) {
        (mcc::PointGroup::One(a), mcc::PointGroup::One(b)) => {
            let c1 = dl
                .tree()
                .components
                .iter()
                .find(|c| c.name == "c1")
                .unwrap();
            let c2 = dl
                .tree()
                .components
                .iter()
                .find(|c| c.name == "c2")
                .unwrap();
            assert_eq!(a.node, c1.node_id.unwrap(), "source node is c1");
            assert_eq!(b.node, c2.node_id.unwrap(), "target node is c2");
            assert_eq!(
                c1.def.pins.ledger.id_of("1"),
                Some(a.pin),
                "source pin is c1's pin 1"
            );
        }
        other => panic!("expected One/One lane, got {other:?}"),
    }

    // `c1.2 -> GND`: GND is a declared module port — the port-ordinal
    // convention resolves it to `(main node, port ordinal)`, so the lane is
    // now complete (component pin on one side, module port on the other).
    assert_eq!(
        trunks[1].lanes.len(),
        1,
        "port-ordinal resolution gives the port-boundary lane"
    );
    match (&trunks[1].lanes[0].source, &trunks[1].lanes[0].target) {
        (mcc::PointGroup::One(a), mcc::PointGroup::One(b)) => {
            let c1 = dl
                .tree()
                .components
                .iter()
                .find(|c| c.name == "c1")
                .unwrap();
            assert_eq!(a.node, c1.node_id.unwrap(), "source node is c1");
            assert_eq!(b.node, dl.tree().node_id.unwrap(), "target node is main");
            let gnd_ord = dl
                .tree()
                .ports
                .iter()
                .position(|p| p.name == "GND")
                .unwrap();
            assert_eq!(b.pin.0, gnd_ord as u32, "target is main's GND port ordinal");
        }
        other => panic!("expected One/One lane, got {other:?}"),
    }
}

/// Phase D: the lane-layer walk follows the arena children edges, so
/// statements of sub-modules are collected too — nesting does not lose
/// statements, and the statement count equals the tree's total connection
/// count.
#[test]
fn lane_layer_collects_submodule_statements() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
}
module REG(in VIN) {
    CAP c
    c.1 -> VIN
}
module main {
    io VDD
    io GND
    REG r(VDD)
    r.c -> GND
}";
    let dl = build_dianlu(src);
    let tree = dl.tree();

    // Expected statements: `c.1 -> VIN` inside `r` (2 points, one statement)
    // plus `r.c -> GND` inside `main` (one statement).
    let total: usize = tree.connections.len()
        + tree
            .sub_modules
            .iter()
            .map(|s| s.connections.len())
            .sum::<usize>();
    assert_eq!(
        dl.lanes().len(),
        total,
        "every statement of the tree has a trunk"
    );
    assert_eq!(total, 2, "fixture has exactly two connection statements");
}

/// Phase D: the net layer (design §11.3 ③) derives union-find equivalence
/// classes from the lane layer — lanes sharing an endpoint collapse into one
/// net, in first-seen point order.
#[test]
fn net_layer_unions_shared_endpoints() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
}
module main {
    CAP c1
    CAP c2
    CAP c3
    c1.1 -> c2.1
    c2.1 -> c3.1
}";
    let dl = build_dianlu(src);

    let nets = dl.nets();
    assert_eq!(
        nets.len(),
        1,
        "two lanes sharing c2.1 collapse into one net"
    );
    let net = &nets[0];
    assert_eq!(net.label, None, "owner-only statement carries no label");
    assert_eq!(
        net.points.len(),
        3,
        "net members are c1.1, c2.1, c3.1 in written order; got {:?}",
        net.points
    );
    let c1 = dl
        .tree()
        .components
        .iter()
        .find(|c| c.name == "c1")
        .unwrap();
    let c2 = dl
        .tree()
        .components
        .iter()
        .find(|c| c.name == "c2")
        .unwrap();
    let c3 = dl
        .tree()
        .components
        .iter()
        .find(|c| c.name == "c3")
        .unwrap();
    let p1 = mcc::PointId {
        node: c1.node_id.unwrap(),
        pin: c1.def.pins.ledger.id_of("1").unwrap(),
    };
    let p2 = mcc::PointId {
        node: c2.node_id.unwrap(),
        pin: c2.def.pins.ledger.id_of("1").unwrap(),
    };
    let p3 = mcc::PointId {
        node: c3.node_id.unwrap(),
        pin: c3.def.pins.ledger.id_of("1").unwrap(),
    };
    assert_eq!(net.points, vec![p1, p2, p3], "first-seen written order");
}

/// Phase D: the derived net's label is the first statement label among its
/// member lanes (`ConnectionInst::net_name`) — a chain statement that
/// touches a module port at one end keeps its component-side lane and names
/// the net with the port label.
#[test]
fn net_layer_labels_chain_statement() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
}
module main {
    io GND
    CAP c1
    CAP c2
    GND -> c1.2 -> c2.2
}";
    let dl = build_dianlu(src);

    let nets = dl.nets();
    assert_eq!(nets.len(), 1, "the chain produces one net");
    let net = &nets[0];
    assert_eq!(
        net.label.as_deref(),
        Some("GND"),
        "statement label names the net"
    );
    assert_eq!(
        net.points.len(),
        3,
        "the port-ordinal point joins the net: GND + c1.2 + c2.2"
    );
}

/// Phase D: the lane layer groups one source statement into one trunk even
/// when the engine explodes it into per-pair connections — a chain
/// `GND -> c1.2 -> c2.2` splits into two `ConnectionInst`s but stays one
/// statement trunk that keeps the resolvable middle lane and the per-point
/// labels of the connections that introduced them.
#[test]
fn lane_layer_one_trunk_per_chain_statement() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
}
module main {
    io GND
    CAP c1
    CAP c2
    GND -> c1.2 -> c2.2
}";
    let dl = build_dianlu(src);
    let trunks = dl.lanes();
    assert_eq!(trunks.len(), 1, "one chain statement -> one trunk");
    let t = &trunks[0];
    assert_eq!(
        t.lanes.len(),
        2,
        "the chain's per-pair lanes: GND->c1.2 and c1.2->c2.2"
    );
    assert_eq!(t.points.len(), 3, "port + both component pins resolve");

    let c1 = dl
        .tree()
        .components
        .iter()
        .find(|c| c.name == "c1")
        .unwrap();
    let c2 = dl
        .tree()
        .components
        .iter()
        .find(|c| c.name == "c2")
        .unwrap();
    let p_c1_2 = mcc::PointId {
        node: c1.node_id.unwrap(),
        pin: c1.def.pins.ledger.id_of("2").unwrap(),
    };
    let p_c2_2 = mcc::PointId {
        node: c2.node_id.unwrap(),
        pin: c2.def.pins.ledger.id_of("2").unwrap(),
    };
    let gnd_ord = dl
        .tree()
        .ports
        .iter()
        .position(|p| p.name == "GND")
        .unwrap();
    let p_gnd = mcc::PointId {
        node: dl.tree().node_id.unwrap(),
        pin: mcc::DefMemberId(gnd_ord as u32),
    };
    assert_eq!(
        t.points,
        vec![
            (p_gnd, Some("GND".to_string())),
            (p_c1_2, Some("GND".to_string())),
            (p_c2_2, None),
        ],
        "points in written order with the introducing connection's label"
    );
}

/// Phase D: a vector broadcast statement (`c[1:2].Cap([VDD, GND])`) explodes
/// into four connections but stays ONE statement trunk — the per-member,
/// per-pin points survive the merge with their own connection labels
/// (members' pin 1 on VDD, pin 2 on GND).
#[test]
fn lane_layer_one_trunk_per_broadcast_statement() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
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
    assert_eq!(trunks.len(), 1, "one broadcast statement -> one trunk");
    let t = &trunks[0];
    assert_eq!(
        t.points.len(),
        6,
        "both members x both pins + the VDD/GND port-ordinal points"
    );

    let c1 = dl
        .tree()
        .components
        .iter()
        .find(|c| c.name == "c1")
        .unwrap();
    let c2 = dl
        .tree()
        .components
        .iter()
        .find(|c| c.name == "c2")
        .unwrap();
    let pin1 = c1.def.pins.ledger.id_of("1").unwrap();
    let pin2 = c1.def.pins.ledger.id_of("2").unwrap();
    let vdd_ord = dl
        .tree()
        .ports
        .iter()
        .position(|p| p.name == "VDD")
        .unwrap();
    let gnd_ord = dl
        .tree()
        .ports
        .iter()
        .position(|p| p.name == "GND")
        .unwrap();
    let p_vdd = mcc::PointId {
        node: dl.tree().node_id.unwrap(),
        pin: mcc::DefMemberId(vdd_ord as u32),
    };
    let p_gnd = mcc::PointId {
        node: dl.tree().node_id.unwrap(),
        pin: mcc::DefMemberId(gnd_ord as u32),
    };
    let expect = vec![
        (p_vdd, Some("VDD".to_string())),
        (
            mcc::PointId {
                node: c1.node_id.unwrap(),
                pin: pin1,
            },
            Some("VDD".to_string()),
        ),
        (
            mcc::PointId {
                node: c1.node_id.unwrap(),
                pin: pin2,
            },
            Some("GND".to_string()),
        ),
        (p_gnd, Some("GND".to_string())),
        (
            mcc::PointId {
                node: c2.node_id.unwrap(),
                pin: pin1,
            },
            Some("VDD".to_string()),
        ),
        (
            mcc::PointId {
                node: c2.node_id.unwrap(),
                pin: pin2,
            },
            Some("GND".to_string()),
        ),
    ];
    assert_eq!(
        t.points, expect,
        "per-member written order with correct labels"
    );
    // Slice keep-bundle: the two member pins collapse into two bundle lanes —
    // `VDD -> c[1:2].1` and `c[1:2].2 -> GND` — each keeping its members in
    // the declared member-set order (design §4 / §11.3 ③).
    assert_eq!(
        t.lanes.len(),
        2,
        "one Slice lane per member pin, not one per exploded connection"
    );
    let vec_node = dl.tree().vectors[0].node_id.unwrap();
    match (&t.lanes[0].source, &t.lanes[0].target) {
        (mcc::PointGroup::One(src), mcc::PointGroup::Slice { base, members }) => {
            assert_eq!(src.node, p_vdd.node, "source is the VDD port point");
            assert_eq!(base.node, vec_node, "base is the vector grouping node");
            assert_eq!(base.pin, pin1, "base pin is the members' shared pin 1");
            let m1 = mcc::PointId {
                node: c1.node_id.unwrap(),
                pin: pin1,
            };
            let m2 = mcc::PointId {
                node: c2.node_id.unwrap(),
                pin: pin1,
            };
            assert_eq!(members, &vec![m1, m2], "members in declared order c1, c2");
        }
        other => panic!("expected One -> Slice lane, got {other:?}"),
    }
    match (&t.lanes[1].source, &t.lanes[1].target) {
        (mcc::PointGroup::Slice { base, members }, mcc::PointGroup::One(tgt)) => {
            assert_eq!(base.node, vec_node, "base is the vector grouping node");
            assert_eq!(base.pin, pin2, "base pin is the members' shared pin 2");
            assert_eq!(tgt.node, p_gnd.node, "target is the GND port point");
            let m1 = mcc::PointId {
                node: c1.node_id.unwrap(),
                pin: pin2,
            };
            let m2 = mcc::PointId {
                node: c2.node_id.unwrap(),
                pin: pin2,
            };
            assert_eq!(members, &vec![m1, m2], "members in declared order c1, c2");
        }
        other => panic!("expected Slice -> One lane, got {other:?}"),
    }
}

// ============================================================================
// Phase E — overlay layer (§3/§4, design §5 D5)
// ============================================================================

/// Phase E: the circuit-level overlay derives `labels` and the lookup indexes
/// from the frozen tree + net layer. A chain `GND -> c1.2 -> c2.2` unions
/// into one net named `GND`: exactly one label entry, and `point_index["GND"]`
/// carries every physical point of that net in one lookup — the D5 one-hit
/// contract for a scalar net.
#[test]
fn overlay_labels_and_indexes_lock_named_nets() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
    let src = "\
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
}
module main {
    io GND
    CAP c1
    CAP c2
    GND -> c1.2 -> c2.2
}";
    let dl = build_dianlu(src);
    let ov = dl.overlays();

    // The chain unions into one net named "GND": exactly one label entry,
    // whose net carries the port and both component pins.
    assert_eq!(ov.labels.len(), 1, "one derived net named GND");
    let (net_id, name) = &ov.labels[0];
    assert_eq!(name, "GND");
    let gnd_net = dl
        .nets()
        .iter()
        .find(|n| n.id == *net_id)
        .expect("the labelled net lives in the net layer");
    assert_eq!(gnd_net.points.len(), 3, "port + both component pins");

    let tree = dl.tree();
    let c1 = tree.components.iter().find(|c| c.name == "c1").unwrap();
    let c2 = tree.components.iter().find(|c| c.name == "c2").unwrap();
    let p_c1_2 = mcc::PointId {
        node: c1.node_id.unwrap(),
        pin: c1.def.pins.ledger.id_of("2").unwrap(),
    };
    let p_c2_2 = mcc::PointId {
        node: c2.node_id.unwrap(),
        pin: c2.def.pins.ledger.id_of("2").unwrap(),
    };
    let gnd_ord = tree.ports.iter().position(|p| p.name == "GND").unwrap();
    let p_gnd = mcc::PointId {
        node: tree.node_id.unwrap(),
        pin: mcc::DefMemberId(gnd_ord as u32),
    };
    assert_eq!(
        ov.point_index.get("GND"),
        Some(&vec![p_gnd, p_c1_2, p_c2_2]),
        "point_index hits every member point of the GND net in one lookup"
    );
    assert_eq!(
        ov.name_index.get("c1"),
        Some(&vec![c1.node_id.unwrap()]),
        "component hit by bare member-set symbol"
    );
    assert_eq!(
        ov.name_index.get("main.c1"),
        Some(&vec![c1.node_id.unwrap()]),
        "component also hit by canonical path"
    );
    assert_eq!(
        ov.name_index.get("main"),
        Some(&vec![tree.node_id.unwrap()]),
        "the entry module node is indexed under its canonical name"
    );
}

/// Phase E (D5): the vector base names the ordered member set — `c[1:2]` hits
/// every member node in ONE `name_index` lookup, no per-member scan and no
/// flat-table reverse lookup.
#[test]
fn overlay_name_index_vector_base_hits_all_members() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();
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
    let c1 = tree.components.iter().find(|c| c.name == "c1").unwrap();
    let c2 = tree.components.iter().find(|c| c.name == "c2").unwrap();

    let ov = dl.overlays();
    assert_eq!(
        ov.name_index.get("c"),
        Some(&vec![c1.node_id.unwrap(), c2.node_id.unwrap()]),
        "the vector base resolves to the full member node set in one lookup"
    );
    // The grouping node itself stays reachable as a first-class arena node
    // (Phase C) under its canonical path.
    let vec_node = tree.vectors[0].node_id.unwrap();
    assert_eq!(
        ov.name_index.get("main.c"),
        Some(&vec![vec_node]),
        "the vector grouping node under its canonical path"
    );
}
