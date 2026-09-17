// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §2.1 / §7, phase one acceptance: the pin-level stage key.
//!
//! Every segment downstream of Pass2 aligns on one key — the `PointId`
//! (arena node + stable def-member ordinal) the net layer derives. Until this
//! landed the flat table carried it nowhere: `InstEntry` bridged *instances*
//! (`node_id`) but a pin row had no key at all, so vec/viz could only name a
//! pin by its `InstTable` row number, which shifts when an unrelated instance
//! is inserted ahead of it.
//!
//! §7 phase one fixes the acceptance: two builds of the same source agree, and
//! inserting an instance leaves every pre-existing pin's key untouched. Both
//! halves are asserted here, and the fixture is checked to actually exercise
//! the branches (pins *and* ports carry keys; labels carry none) — a fixture
//! that fills no branch would pass while testing nothing.

use crate::common;

use mcc::{McIds, McURI};
use std::fs;

/// Two capacitors plus a sub-module boundary. The `CAP` bodies declare a pin
/// table, so their pins are real points; the `io` rows are ports. Every
/// instance is **explicitly named** — an auto-named device renumbers when a
/// sibling is inserted (D1), which would churn the path column and hide the
/// property under test.
const BASE_SRC: &str = r#"
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
    CAP c1(1)
    CAP c2(1)
    c1.Cap([VDD, GND])
    c2.Cap([VDD, GND])
}
"#;

/// The same circuit with one more capacitor inserted *ahead* of the existing
/// ones. Nothing about `c1` / `c2` changed, so their pins must keep the keys
/// they had.
const INSERTED_SRC: &str = r#"
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
    CAP c0(1)
    CAP c1(1)
    CAP c2(1)
    c0.Cap([VDD, GND])
    c1.Cap([VDD, GND])
    c2.Cap([VDD, GND])
}
"#;

/// A single component, so the virtual (non-project) view renders it on its own
/// with its pins drawn. A pin table is what makes each pin a real point.
const RENDER_SRC: &str = r#"
component CAPPINS
{
    pins = [
        1 = A
        2 = B
    ]
}
"#;

/// Build `main` and flatten to the flat instance table.
fn build_flat(source: &str) -> mcc::InstTable {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/point-identity.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    table
}

/// The stage key of every entry that carries one, as `(path, kind, key)`,
/// sorted so two builds compare as sets rather than sequences.
fn stage_keys(table: &mcc::InstTable) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = table
        .iter()
        .filter_map(|(_, e)| {
            e.point
                .map(|p| (e.path.clone(), e.kind.to_string(), p.to_string()))
        })
        .collect();
    out.sort();
    out
}

/// The full stage key of one entry, split into its two halves so a test can
/// say *which* half moved. See [`the_numbered_form_is_a_first_interning_ordinal`].
fn key_of(table: &mcc::InstTable, path: &str) -> Option<(String, String)> {
    table.iter().find(|(_, e)| e.path == path).and_then(|(_, e)| {
        e.point
            .map(|p| (p.node.to_string(), p.pin.0.to_string()))
    })
}

#[test]
fn every_pin_and_port_carries_a_key_and_no_other_kind_does() {
    let table = build_flat(BASE_SRC);
    let keys = stage_keys(&table);

    // Branch coverage: the fixture must exercise both point-bearing kinds, or
    // the assertions below would hold vacuously.
    let kind_of = |k: &str| keys.iter().filter(|(_, kind, _)| kind == k).count();
    assert_eq!(kind_of("Pin"), 4, "two capacitors × two pins");
    assert_eq!(kind_of("Port"), 2, "main's two io rows");
    assert_eq!(
        keys.len(),
        6,
        "only pins and ports are points — a Module or Component entry is an \
         instance (keyed by `node_id` / canonical path), a Label is neither"
    );

    // The key is the net layer's own value, not a second one: `pin_id` is the
    // row number and is deliberately *not* what a key reads.
    assert!(
        !keys.iter().any(|(_, _, key)| key.is_empty()),
        "a point key is never empty"
    );
}

#[test]
fn two_builds_of_the_same_source_agree_key_for_key() {
    let first = stage_keys(&build_flat(BASE_SRC));
    let second = stage_keys(&build_flat(BASE_SRC));
    assert_eq!(
        first, second,
        "same source, same binary, two builds — the key set must be identical"
    );
}

/// Inserting an instance ahead of the existing ones leaves the **canonical
/// key** of every pre-existing pin untouched: `main.c1.1` is still `main.c1.1`,
/// and it is still the first declared member of its def.
///
/// ⚠ `NodeId`'s integer half does *not* survive the insertion, and that is not
/// a defect of this bridge — see
/// [`the_numbered_form_is_a_first_interning_ordinal`].
#[test]
fn inserting_an_instance_leaves_existing_canonical_keys_untouched() {
    let base = build_flat(BASE_SRC);
    let inserted = build_flat(INSERTED_SRC);

    for path in ["main.c1.1", "main.c1.2", "main.c2.1", "main.c2.2"] {
        let before = key_of(&base, path).expect("base fixture has this pin");
        let after = key_of(&inserted, path).expect("insertion must not rename it");
        // The member half is the def's own ledger ordinal — it survives,
        // because the ledger merges by name and `c0` is a different device.
        assert_eq!(
            before.1, after.1,
            "{path}: the def-member half of the key is insertion-independent"
        );
    }

    // The inserted device is new and distinct — not silently reusing an
    // existing key.
    let c0 = key_of(&inserted, "main.c0.1").expect("inserted pin has a key");
    let c1 = key_of(&inserted, "main.c1.1").expect("existing pin has a key");
    assert_ne!(c0, c1, "two devices never share a point");
}

/// The phase-one deliverable's other half: the key must reach the **rendered
/// element**, or a stage readout has nothing to read it out of (§2.1 Pass4).
///
/// This runs the real pipeline — source → flat table → vec block → graph →
/// SVG — and then checks the drawn pin. The value asserted is read back out of
/// the flat table rather than hardcoded, so the two halves of the pipeline must
/// agree with **each other**, not with a literal in this file.
///
/// A component's own pins are the fixture (not a wired module) because that is
/// the box kind whose renderer draws pin elements: a wiring-level box (a
/// capacitor with a symbol) draws its symbol and no pins, so a fixture built
/// from one would assert the attribute over an empty set and pass vacuously.
#[test]
fn the_rendered_pin_element_publishes_the_key() {
    let _lock = common::lock();

    let dir = std::env::temp_dir().join(format!("mcc-stage-key-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp fixture dir");
    let path = dir.join("part.mc");
    fs::write(&path, RENDER_SRC).expect("write the fixture");
    let uri: McURI = path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    mcc::mcc_load_project(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    let (instance, table, arena, store) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("build the fixture");
    let block = mcc::build_mc_vec(&instance, &table, &arena, &store);
    let graph =
        mcc::mcc_virtual_prepare_graph(mcc::build_mc_vec_graph(&block, &table), &targets[0]);
    let document = mcc::viz::api::render(graph);
    let svg = &document.root_layer().expect("root layer").svg;

    // Branch coverage first: if this box kind drew no pin elements at all, every
    // assertion below would hold over an empty set and report green.
    let pin_groups = svg.matches(r#"class="pin""#).count();
    let keyed: Vec<(String, String)> = table
        .iter()
        .filter_map(|(_, e)| e.point.map(|p| (e.id.to_string(), p.to_string())))
        .collect();
    assert_eq!(
        pin_groups,
        keyed.len(),
        "every pin the table gives a key must be drawn as a pin element \
         (drawn {pin_groups}, keyed {keyed:?})"
    );

    // Each drawn pin publishes *the key the table holds for that row*, next to
    // the row number — so the element and the table name one point by one
    // computation, and `data-pin-id` is still readable for existing lookups.
    for (row, key) in &keyed {
        assert!(
            svg.contains(&format!(r#"data-pin-id="{row}""#)),
            "row {row} must still be published as `data-pin-id`"
        );
        assert!(
            svg.contains(&format!(r#"data-point="{key}""#)),
            "the pin element for row {row} must publish its stage key {key}"
        );
    }

    fs::remove_dir_all(&dir).ok();
}

/// WITNESS — a measured property, not a contract.
///
/// `NodeId`'s integer half is the **ordinal of first interning**
/// (`IdentityRegistry::intern` hands out a monotonic `next`, and
/// `build_identity_registry` only *resumes* what the build already assigned),
/// so it is determined by the source but is **not** insertion-independent:
/// putting one device ahead of another pushes every later device's number up
/// by one.
///
/// Consequence, and the reason this is asserted rather than merely noted:
/// **the numbered form is a within-build join handle only.** Anything that
/// crosses builds — a saved baseline, a diff against a previous run, an
/// external reference — must compare the *canonical* form (the entry path and
/// the def key), never this integer. If this test ever fails because the
/// numbers stopped moving, that is good news that invalidates a documented
/// limitation: revisit `stage-readout-design.md` §1.2 ② / §2 before "fixing"
/// the test.
#[test]
fn the_numbered_form_is_a_first_interning_ordinal() {
    let base = build_flat(BASE_SRC);
    let inserted = build_flat(INSERTED_SRC);

    let (base_node, _) = key_of(&base, "main.c1.1").unwrap();
    let (ins_node, _) = key_of(&inserted, "main.c1.1").unwrap();

    assert_ne!(
        base_node, ins_node,
        "`main.c1` keeps its canonical path but takes a new number: one device \
         (`c0`) was interned ahead of it"
    );

    // The member half did not move, so the shift is entirely the node half.
    let (_, base_member) = key_of(&base, "main.c1.1").unwrap();
    let (_, ins_member) = key_of(&inserted, "main.c1.1").unwrap();
    assert_eq!(base_member, ins_member);
}
