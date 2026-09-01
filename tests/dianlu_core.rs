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
use std::path::Path;
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
    let dl2 = mcc::DianLu::new(tree, 1000);
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
    let tree2 = mcc::DianLu::new(tree, 1000);
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
    let tree_table = mcc::InstTable::from_module_inst(tree, 1000);

    assert_eq!(
        flat_signature(arena_table),
        flat_signature(&tree_table),
        "arena-driven flatten and tree-recursive flatten are line-for-line identical"
    );
}
