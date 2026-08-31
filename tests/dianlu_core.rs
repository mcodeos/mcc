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
    let mut dl = build_dianlu("module main {\n    func main() {\n        res[1:2] -> led[3:4]\n    }\n}");

    let t1 = dl.flatten() as *const _;
    let t2 = dl.flatten() as *const _;
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

    assert!(table.len() >= 2, "flat projection has instance entries; len={}", table.len());
    assert_eq!(
        tree.name,
        "main",
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
