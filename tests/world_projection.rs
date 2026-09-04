// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage A (world-core refactor): `CircuitWorld` as the projection root.
//!
//! One frozen instantiation per circuit key yields every read projection
//! (tree / flat `InstTable` / net diagnostics) off the world's live `DianLu`.
//! `CircuitWorld::flatten` derives the flat projection once (invariant B,
//! cached no-op thereafter) and returns the flat electrical net-check
//! diagnostics for the caller — the world itself never writes the workspace
//! diagnostics store. These tests lock that contract.

#![allow(non_snake_case)]

mod common;

use mcc::McIds;

/// Two-input/one-output buffer used by the driver-conflict fixture.
const BUF: &str = "component BUF {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n";

/// `b1.Y -> b2.Y` merges two `Out` pins onto one net → a 4101 driver conflict
/// (plus floating-input / module-port / partial-wiring companions).
const CONFLICT_SRC: &str = "module main {\n    BUF b1\n    BUF b2\n    b1.Y -> b2.Y\n}";

/// Build `main` into a fresh world; return the world + its circuit key.
fn build_world() -> (mcc::CircuitWorld, mcc::CircuitKey) {
    let uri = "/mcc/world-proj.mc".to_string();
    common::load_string(&uri, &format!("{BUF}{CONFLICT_SRC}"));
    let mut world = mcc::CircuitWorld::new(1000);
    let entry = mcc::McSpaceName::new(&McIds::from("main"), uri.clone());
    let key = world.instantiate(&entry).expect("instantiate main");
    (world, key)
}

/// The world instantiates once per key and holds a live circuit; `flatten`
/// derives the cached flat projection and returns its net diagnostics — and
/// never writes them to the workspace store.
#[test]
fn world_flat__projection_cached_diags_returned_store_untouched() {
    let _lock = common::lock();
    common::reset();

    let (mut world, key) = build_world();

    // Not yet projected.
    assert!(world.flat(&key).is_none(), "flat projection must be lazy");
    assert_eq!(world.net_diags(&key).map(|d| d.len()).unwrap_or(0), 0);

    // One flatten → projection present, diagnostics returned with the net
    // codes this fixture is known to produce.
    let diags1 = world.flatten(&key).expect("flatten");
    assert!(
        world.flat(&key).is_some(),
        "flatten must cache the projection"
    );
    assert!(world.circuit(&key).unwrap().table().is_some());
    assert!(
        diags1.iter().any(|d| d.code == 4101),
        "driver-conflict fixture must report a 4101; got {:?}",
        diags1.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert_eq!(world.net_diags(&key).unwrap().len(), diags1.len());

    // A second flatten is a cached no-op: identical diagnostics.
    let diags2 = world.flatten(&key).expect("flatten again");
    assert_eq!(
        diags1
            .iter()
            .map(|d| (d.code, d.msg.as_str()))
            .collect::<Vec<_>>(),
        diags2
            .iter()
            .map(|d| (d.code, d.msg.as_str()))
            .collect::<Vec<_>>(),
        "flatten must be idempotent (cached no-op)"
    );

    // The world performs no global writes: no flat net-check code may have
    // reached the workspace diagnostics store.
    let store_codes = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .collect::<Vec<_>>();
    assert!(
        !store_codes
            .iter()
            .any(|c| { (4101..=4119).contains(c) || *c == 4056 || *c == 6005 }),
        "world flatten must not write net diagnostics to the store; got {store_codes:?}"
    );
}

/// The key is stable for an entry and its registry survives in the world; the
/// registry carries the interned circuit paths (D1) so re-instantiation of the
/// same entry resumes the same id namespace.
#[test]
fn world_flat__instantiate_returns_stable_key_and_registry() {
    let _lock = common::lock();
    common::reset();

    let uri = "/mcc/world-proj.mc".to_string();
    common::load_string(&uri, &format!("{BUF}{CONFLICT_SRC}"));
    let entry = mcc::McSpaceName::new(&McIds::from("main"), uri);

    let mut world = mcc::CircuitWorld::new(1000);
    let key = world.instantiate(&entry).expect("instantiate");
    assert_eq!(key.top, "main");
    assert!(
        world.registry(&key).is_some(),
        "registry must be world-held"
    );
    assert_eq!(world.circuits().count(), 1);

    // Re-instantiating the same entry returns the same key and keeps one
    // circuit (the old DianLu is replaced, the registry carried forward).
    let key2 = world.instantiate(&entry).expect("re-instantiate");
    assert_eq!(key, key2);
    assert_eq!(world.circuits().count(), 1);
}
