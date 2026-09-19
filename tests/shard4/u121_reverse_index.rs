// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the reverse index (CIMP §1 U121,
// `doc/arch/space/organization-units-design.md` §9).
//
// Build mirrors definition → circuit in one direction only (§9.1), so "where is
// the def I typed on the board?" had no read at all — and scanning for it is
// O(n) per keystroke, which an editor's hot query cannot pay (§9.4). What is
// locked here:
//
//   1. One name reaches **both** levels of the forward leg: typing a class name
//      finds the part it names (instance level) and the pins that make it up
//      (point level), including the pins of an instance nested inside another
//      module — the def half of which is recovered by walking `parent_id` up.
//   2. No key is a number. The keys are names and canonical pairs, and this is
//      the discipline §9.6 states as the difference between a derived index and
//      the forbidden bridge — a value row may carry a `NodeId` or a `PointId`
//      (that is what a join handle is), but nothing may be *keyed* by one.
//   3. The index is a function of the build: two builds of one source agree key
//      for key and row for row, and the outward key list is in key order rather
//      than arrival order (discipline 0).
//   4. The derivation is wired to the projection: a `DianLu` that has not been
//      flattened has no index, and the one it has afterwards is exactly what
//      `ReverseIndex::derive` makes of its own table — a refactor cannot drop
//      the derivation and leave the read answering an older world.
//
// Two properties are deliberately **not** locked here, because they are shape
// claims rather than runtime readings: the index holds no `Serialize` impl and
// is never persisted (both are visible in the type, not in a run), and it is
// never promoted to an identity (nothing outside this process can hold a key).
//
// The fixture is two-of-everything where a branch could be bypassed: two
// component classes, two instances of one of them at two different depths, and
// a def instantiated both directly under the top and inside a submodule.

#![allow(non_snake_case)]

use crate::common;

use mcc::{Hit, InstKind, McIds, McURI, ReverseIndex};

const SOURCE: &str = r#"
component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}

component CAP
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module inner(psnk GND)
{
    RES R1
    R1.1 -> GND
    R1.2 -> GND
}

module main(psnk GND)
{
    inner U_IN
    RES R2
    CAP C1
    R2.1 -> GND
    R2.2 -> GND
    C1.1 -> GND
    C1.2 -> GND
    U_IN.GND -> GND
}
"#;

const URI: &str = "/mcc/reverse-index.mc";

/// The instance level and the point level: the two row sets the forward leg
/// publishes (build-design §3.7, point level by O16), and the value set of this
/// index. Anything outside these four kinds names no def of its own, so it is
/// not a key — which the first lock below checks from the rows themselves.
fn is_instance_level(k: &InstKind) -> bool {
    matches!(k, InstKind::Module | InstKind::Component)
}

fn is_point_level(k: &InstKind) -> bool {
    matches!(k, InstKind::Pin | InstKind::Port)
}

/// Freeze the fixture and derive its index.
///
/// The caller must hold [`common::lock`] for its whole body: the definition
/// space is process-global, and a sibling test's `reset()` landing mid-build
/// would strip the class defs this index is built from.
fn frozen(source: &str) -> ReverseIndex {
    common::reset();
    let uri: McURI = URI.to_string();
    common::load_string(URI, source);
    let ident = McIds::from("main");
    let (_inst, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&ident, &uri, 1).expect("pass2_flat failed");
    ReverseIndex::derive(&table)
}

/// Every key with its rows, in key order — the whole index, in one comparable
/// value. Used for the determinism lock, which is about the entire reading
/// rather than about one key of it.
fn snapshot(index: &ReverseIndex) -> Vec<(String, Vec<Hit>)> {
    index
        .names()
        .map(|(k, hits)| (k.clone(), hits.clone()))
        .collect()
}

/// The paths of one key's rows, sorted, so an assertion names the rows it means
/// without depending on the walk order of the table underneath.
fn paths(index: &ReverseIndex, key: &str) -> Vec<String> {
    let mut p: Vec<String> = index.lookup(key).iter().map(|h| h.path.clone()).collect();
    p.sort();
    p
}

/// The kinds of one key's rows, deduplicated in first-appearance order.
fn kinds(index: &ReverseIndex, key: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for h in index.lookup(key) {
        let w = h.kind.word();
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// The two families of "this is a number, not a name".
///
/// Structural, not a list of forbidden names: an all-digit spelling is how an
/// arena id or a flat entry id would read, and `PointId`'s `Display` is
/// `node:member` (`instant/lane.rs:64`), whose members are numbers. A def name
/// is an identifier — it cannot be either. A key that were one would mean the
/// index was keyed by a circuit-side number, which is the violation §9.6 names.
fn is_a_number(key: &str) -> bool {
    let mut digits = 0usize;
    for c in key.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else if c != ':' {
            return false;
        }
    }
    digits > 0
}

#[test]
fn u121__one_name_reaches_both_levels() {
    let _guard = common::lock();
    let index = frozen(SOURCE);

    for key in ["RES", "CAP", "inner"] {
        let instance: Vec<&str> = index
            .lookup(key)
            .iter()
            .filter(|h| is_instance_level(&h.kind))
            .map(|h| h.path.as_str())
            .collect();
        let point: Vec<&str> = index
            .lookup(key)
            .iter()
            .filter(|h| is_point_level(&h.kind))
            .map(|h| h.path.as_str())
            .collect();
        assert!(
            !instance.is_empty() && !point.is_empty(),
            "'{key}' must reach both levels; got instance {instance:?}, point {point:?} (kinds {:?})",
            kinds(&index, key)
        );
        // The rows a key reaches are only ever those two levels: a label or a
        // bus row names no def, so it can be under no key at all.
        assert_eq!(
            instance.len() + point.len(),
            index.lookup(key).len(),
            "'{key}' holds a row of neither level: {:?}",
            kinds(&index, key)
        );
    }

    // The nested case, which is where the def half has to be walked for: `R1`
    // sits inside `inner`, so its pins carry no def of their own.
    assert!(
        paths(&index, "RES").contains(&"main.U_IN.R1.1".to_string()),
        "the pins of a nested instance must be keyed by their class: {:?}",
        paths(&index, "RES")
    );
    assert!(
        paths(&index, "RES").contains(&"main.R2.1".to_string()),
        "…and so must the pins of an instance under the top: {:?}",
        paths(&index, "RES")
    );
    // A module's own name answers for its instance rows and its header ports
    // alike: both were flattened into that def.
    assert!(
        paths(&index, "inner").iter().any(|p| p == "main.U_IN"),
        "{:?}",
        paths(&index, "inner")
    );
}

#[test]
fn u121__every_row_is_keyed_under_its_own_class_name() {
    let _guard = common::lock();
    let index = frozen(SOURCE);

    let mut rows = 0usize;
    let mut pairs: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    for (key, hits) in index.names() {
        for h in hits {
            rows += 1;
            let ident = h.class.ident.to_string();
            assert_eq!(
                &ident, key,
                "a row under '{key}' names class '{ident}' (path {})",
                h.path
            );
            let (uri, ident) = h.class_pair();
            assert!(
                index.lookup_canon(&uri, &ident).contains(h),
                "the canonical lookup must agree with the typed one for {uri}#{ident}"
            );
            pairs.insert(h.class_pair());
        }
    }
    assert!(
        rows >= 8,
        "the fixture must contribute rows of both levels; got {rows}"
    );

    // The other direction: every pair the rows name is a key with rows, and its
    // rows are all keyed under that pair's name — so neither family can hold a
    // row the other does not, and no pair is a key of its own with nothing in
    // it. (The pairs the rows name *are* the canonical keys: every canon entry
    // was written by a row.)
    assert!(pairs.len() >= 4, "the fixture must key several defs");
    for (uri, ident) in pairs {
        let rows = index.lookup_canon(&uri, &ident);
        assert!(!rows.is_empty(), "{uri}#{ident} is a key with no rows");
        for h in rows {
            assert!(
                index.lookup(&ident).contains(h),
                "a canonical row must also be keyed by its name"
            );
        }
    }
}

#[test]
fn u121__no_key_is_a_number() {
    let _guard = common::lock();
    let index = frozen(SOURCE);

    assert!(
        !index.is_empty(),
        "an empty index would make this lock vacuous"
    );
    let mut checked = 0usize;
    for (key, hits) in index.names() {
        assert!(
            !is_a_number(key),
            "'{key}' is a number, and an index keyed by one is the forbidden bridge (§9.6)"
        );
        // Both halves of every canonical pair the index holds, taken from the
        // rows that wrote them — a row's `class` is exactly what `canon` was
        // registered from, so this covers the second key family without an
        // iterator over it.
        for h in hits {
            let (uri, ident) = h.class_pair();
            assert!(
                !is_a_number(&uri) && !is_a_number(&ident),
                "{uri}#{ident} is keyed by a number"
            );
            checked += 1;
        }
    }
    assert!(checked >= 8, "the fixture must be checked, not skipped");
}

#[test]
fn u121__two_builds_of_one_source_agree() {
    let _guard = common::lock();
    let first = frozen(SOURCE);
    // A second full build: reset, load, flatten again. If any part of the index
    // depended on the order rows arrived in, this is where it would show.
    let second = frozen(SOURCE);

    assert_eq!(snapshot(&first), snapshot(&second));
    assert!(!first.is_empty(), "the fixture must key something");
    assert_eq!(
        first.len(),
        first.names().count(),
        "the key count must be the number of keys"
    );
}

#[test]
fn u121__the_projection_derives_the_index_beside_the_table() {
    let _guard = common::lock();
    common::reset();
    let uri: McURI = URI.to_string();
    common::load_string(URI, SOURCE);
    let ident = McIds::from("main");
    let mut dl = mcc::mcc_build_dianlu(&ident, &uri, 1000).expect("build failed");

    // Before the projection there is no index: it is derived by `flatten`, not
    // carried by the instantiation.
    assert!(
        dl.reverse().is_none() && dl.table().is_none(),
        "the index and the table are both derived, and neither exists before flatten"
    );

    dl.flatten();
    let derived = dl.reverse().expect("flatten must derive the index");
    let table = dl.table().expect("flatten must produce the table");
    assert_eq!(
        snapshot(derived),
        snapshot(&ReverseIndex::derive(table)),
        "the wired index must be what deriving from its own table makes"
    );
}

#[test]
fn u121__the_key_index_is_ordered_and_the_pattern_matches_over_keys() {
    let _guard = common::lock();
    let index = frozen(SOURCE);
    let mut sources = mcc::stages::SourceText::new();

    // The outward key list is in key order, not in the order the table walked
    // (discipline 0: what is published may not depend on arrival order).
    let listing = mcc::key_rows(&index);
    let names: Vec<String> = listing
        .iter()
        .map(|r| r["name"].as_str().unwrap_or_default().to_string())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "the key index is published in key order");
    assert_eq!(listing.len(), index.len());

    // A pattern matches over the keys, and its rows are exactly the rows of the
    // keys it matched — the read that answers an editor's filter without ever
    // walking the circuit rows. The case is folded, so the pattern is spelled
    // the way it would be typed.
    let (matched, rows) = mcc::matched_rows(&index, "re", &mut sources);
    assert!(
        matched.contains(&"RES".to_string()),
        "a substring of a key must find it: {matched:?}"
    );
    for key in matched.iter() {
        assert!(
            key.to_lowercase().contains("re"),
            "'{key}' does not match the pattern"
        );
    }
    let want: usize = matched.iter().map(|k| index.lookup(k).len()).sum();
    assert_eq!(rows.len(), want);
    for row in rows.iter() {
        let key = row["key"].as_str().unwrap_or_default();
        assert!(
            matched.iter().any(|m| m == key),
            "a row names the key it was reached by: {key}"
        );
    }

    // Nothing matched is a reading of its own, told apart from a smaller index:
    // the key list is empty and so are the rows.
    let (none, no_rows) = mcc::matched_rows(&index, "NO_SUCH_DEF", &mut sources);
    assert!(none.is_empty() && no_rows.is_empty());
}
