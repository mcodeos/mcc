// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M5 acceptance: the difference between two `stage.viz` readings.
//!
//! The contract is mostly a list of things that must **not** be done, because
//! every one of them is a way to make the difference look right while keying on
//! something that does not survive a rebuild:
//!
//! * `key` (`D39`) is the `InstTable` row number, so inserting one instance
//!   shifts all of them. Comparing whole items would read that shift as "every
//!   box changed"; a `modify` on a box must therefore name only `at` when the
//!   box merely moved.
//! * `nid` is a per-build net ordinal. Keying nameless nets on it would report a
//!   page of comings and goings that are artifacts of renumbering.
//! * `PointId` is the ordinal of first interning, so a pin's `key` is not an
//!   identity either.
//! * A metric publishes `canon_key: null` and is still perfectly alignable, so
//!   "no canonical key" cannot be the test for "cannot be aligned".
//!
//! Three branches have **no members** in the hbl readout, measured: no pin has a
//! null `canon_key` (175 of 175), there are no `wire` segments (the device layer
//! does not go through the router), and nothing is unalignable. Those are built
//! by hand here and marked as such; the rest of the file uses the real readout,
//! because a synthetic circuit gives sorting nothing to reorder.
//!
//! ⚠ A CLI invocation must run in a **fresh empty directory** — `viz/project.rs`
//! unconditionally writes `baseline/render_projection.md` relative to the cwd.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use mcc::stages::stage_diff::{StageDiff, LOCALITY_MIN_SAMPLE, VIZ_LAW};

/// A small named-instance circuit. Named on purpose: an auto-named device
/// renumbers when a sibling is inserted, which would churn the `path` column and
/// hide the property under test.
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
    CAP c3(1)
    c1.Cap([VDD, GND])
    c2.Cap([VDD, c3.1])
    c3.Cap([c2.2, GND])
}
"#;

/// The same circuit with one more capacitor inserted *ahead* of the others.
/// Nothing about `c1` / `c2` / `c3` changed.
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
    CAP c3(1)
    c0.Cap([VDD, GND])
    c1.Cap([VDD, GND])
    c2.Cap([VDD, c3.1])
    c3.Cap([c2.2, GND])
}
"#;

// ── Fixture plumbing ──

fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

/// A fresh, **empty** directory to run a CLI invocation in.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-viz-diff-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn run(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run mcc");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn stage_of(stdout: &str) -> Value {
    let envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

/// `show stage viz` against the hbl project (the five-file world) and against a
/// standalone source written into a scratch dir.
fn hbl_viz(name: &str) -> Vec<Value> {
    let dir = scratch(name);
    let entry = hbl_entry();
    let (stdout, stderr, ok) = run(
        &dir,
        &[
            "--local",
            "show",
            "stage",
            "viz",
            "-f",
            "json",
            "-F",
            entry.to_str().expect("fixture path"),
        ],
    );
    assert!(ok, "show stage viz failed: {stderr}");
    items_of(&stage_of(&stdout))
}

fn viz_on(name: &str, source: &str) -> Vec<Value> {
    let dir = scratch(name);
    let path = dir.join("circuit.mc");
    std::fs::write(&path, source).expect("write the source");
    let (stdout, stderr, ok) = run(
        &dir,
        &[
            "--local",
            "show",
            "stage",
            "viz",
            "-f",
            "json",
            "-F",
            path.to_str().expect("source path"),
        ],
    );
    assert!(ok, "show stage viz failed: {stderr}");
    items_of(&stage_of(&stdout))
}

fn items_of(stage: &Value) -> Vec<Value> {
    stage["items"]
        .as_array()
        .expect("items is an array")
        .clone()
}

fn changes_of(d: &StageDiff) -> &[Value] {
    &d.changes
}

fn count(d: &StageDiff, ty: &str, kind: &str) -> usize {
    changes_of(d)
        .iter()
        .filter(|c| c["type"] == ty && c["kind"] == kind)
        .count()
}

// ── Synthetic items ──
//
// These exist for the branches the real readout cannot reach (see the module
// note). Each is a minimum-shape item carrying only the fields the difference
// reads, so a test says which branch it is about rather than which fields
// happened to be present.

fn ck(path: &str, ident: &str) -> Value {
    json!({ "path": path, "def": { "uri": "/src/m.mc", "ident": ident } })
}

fn box_item(path: &str, ident: &str, at: [f64; 2]) -> Value {
    json!({
        "class": "box", "key": null, "point": null, "path": path,
        "canon_key": ck(path, ident), "name": path.rsplit('.').next().unwrap(),
        "class_name": ident, "kind": "two_pin", "at": at, "size": [10.0, 5.0],
        "pins": 2, "anchors": 2, "layer": "main", "loc": null,
    })
}

fn pin_item(path: &str, ident: &str, net: Option<&str>, nid: Option<i64>) -> Value {
    json!({
        "class": "pin", "key": null, "point": null, "path": path,
        "canon_key": ck(path, ident), "net": net, "nid": nid, "num": "1", "name": "",
        "io": "input", "side": "top", "offset": 0.5, "at": [0.0, 0.0], "anchors": 1,
        "box": "main.B1", "layer": "main", "loc": null,
    })
}

/// A pin with no canonical key at all -- the one real path to "cannot be
/// aligned". Measured to have **no members** on hbl, so it is built by hand.
fn keyless_pin(key: Option<&str>, name: &str) -> Value {
    json!({
        "class": "pin", "key": key, "point": null, "path": null, "canon_key": null,
        "net": null, "nid": null, "num": "?", "name": name, "io": "input",
        "side": "top", "offset": 0.5, "at": [0.0, 0.0], "anchors": 1, "box": "main.B1",
        "layer": "main", "loc": null,
    })
}

fn metric(family: &str, field: &str, value: Value) -> Value {
    json!({
        "class": "metrics", "key": null, "point": null, "canon_key": null,
        "path": format!("{family}.{field}"), "family": family, "field": field,
        "value": value, "layer": null, "loc": null,
    })
}

fn edge_seg(from: &[&str], to: &[&str]) -> Value {
    let end = |ps: &[&str]| {
        Value::Array(
            ps.iter()
                .map(|p| json!({ "path": p, "point": null }))
                .collect(),
        )
    };
    json!({
        "class": "segment", "key": null, "point": null, "path": null, "canon_key": null,
        "kind": "edge", "net": "N", "nid": null, "index": 0, "edge_kind": "signal",
        "lanes": 1, "trunk": null, "ret": null, "from": end(from), "to": end(to),
        "from_at": null, "to_at": null, "length": null, "layer": "main", "loc": null,
    })
}

/// A wire segment: coordinates and no ends. Measured to have **no members** on
/// hbl, so it is built by hand.
fn wire_seg(net: &str, from: [f64; 2], to: [f64; 2], length: f64) -> Value {
    json!({
        "class": "segment", "key": null, "point": null, "path": null, "canon_key": null,
        "kind": "wire", "net": net, "nid": 1, "index": 0, "edge_kind": null, "lanes": null,
        "trunk": null, "ret": null, "from": null, "to": null, "from_at": from,
        "to_at": to, "length": length, "layer": "main", "loc": null,
    })
}

// ── Same input ──

/// The strongest single statement: the difference of a reading with itself is
/// empty. Anything that leaks a build-local ordinal into a key or a comparison
/// shows up here first.
#[test]
fn a_reading_does_not_differ_from_itself() {
    for (label, items) in [("synthetic", synthetic_world()), ("hbl", hbl_viz("self"))] {
        let d = VIZ_LAW.diff(&items, &items);
        assert!(
            changes_of(&d).is_empty(),
            "{label}: a reading differs from itself: {:?}",
            changes_of(&d).get(0)
        );
        if label == "hbl" {
            assert!(d.unaligned.is_empty(), "hbl: unaligned on itself");
        }
        assert_eq!(
            d.stability
                .as_ref()
                .expect("this law draws boxes")
                .unchanged_boxes_moved,
            0,
            "{label}"
        );
        assert_eq!(
            d.stability
                .as_ref()
                .expect("this law draws boxes")
                .route_hashes_changed,
            0,
            "{label}"
        );
        assert!(
            !d.stability
                .as_ref()
                .expect("this law draws boxes")
                .locality_warning,
            "{label}"
        );
    }
}

// ── Every branch has members ──

/// Build a reading that puts at least two members in every branch the contract
/// names, so no branch is skipped wholesale and reported as green.
fn synthetic_world() -> Vec<Value> {
    let mut v = Vec::new();
    // Boxes: two that stay, two that move, two that appear, two that vanish.
    for i in 0..2 {
        v.push(box_item(&format!("main.keep{i}"), "CAP", [1.0, 1.0]));
        v.push(box_item(&format!("main.moves{i}"), "CAP", [2.0, 2.0]));
        v.push(box_item(&format!("main.gone{i}"), "CAP", [3.0, 3.0]));
    }
    // Pins. The boxes that vanish carry nets of their own, so those nets leave
    // the reading with them -- that is the only way a net "is removed".
    for i in 0..2 {
        v.push(pin_item(
            &format!("main.keep0.p{i}"),
            "CAP",
            Some("net:N1"),
            Some(1),
        ));
        v.push(pin_item(
            &format!("main.keep1.p{i}"),
            "CAP",
            Some("net:N2"),
            Some(2),
        ));
        v.push(pin_item(
            &format!("main.moves0.p{i}"),
            "CAP",
            Some("net:N1"),
            Some(1),
        ));
        v.push(pin_item(
            &format!("main.moves1.p{i}"),
            "CAP",
            Some("net:N2"),
            Some(2),
        ));
        v.push(pin_item(
            &format!("main.gone0.p{i}"),
            "CAP",
            Some("net:N7"),
            Some(7),
        ));
        v.push(pin_item(
            &format!("main.gone1.p{i}"),
            "CAP",
            Some("net:N8"),
            Some(8),
        ));
    }
    // Metrics: two that change.
    v.push(metric("determinism", "route_geometry_hash", json!("aaa")));
    v.push(metric("determinism", "graph_input_hash", json!("bbb")));
    v.push(metric("scope", "layers", json!(1)));
    // Segments: two edges that stay, two that reroute, two wires that stay.
    v.push(edge_seg(&["main.keep0.p0"], &["main.keep1.p0"]));
    v.push(edge_seg(&["main.keep0.p1"], &["main.keep1.p1"]));
    v.push(edge_seg(&["main.moves0.p0"], &["main.moves1.p0"]));
    v.push(edge_seg(&["main.gone0.p0"], &["main.gone1.p0"]));
    v.push(wire_seg("N", [0.0, 0.0], [5.0, 0.0], 5.0));
    v.push(wire_seg("N", [0.0, 1.0], [5.0, 1.0], 5.0));
    // Two pins that cannot be aligned (no canonical key).
    v.push(keyless_pin(None, "alpha"));
    v.push(keyless_pin(None, "beta"));
    v
}

/// The second version: a move, an add, a remove, a content change and a reroute,
/// each with two members, on the world above.
fn synthetic_world_next() -> Vec<Value> {
    let mut v = Vec::new();
    for i in 0..2 {
        v.push(box_item(&format!("main.keep{i}"), "CAP", [1.0, 1.0]));
        // Moved: same content, new position.
        v.push(box_item(&format!("main.moves{i}"), "CAP", [9.0, 9.0]));
        v.push(box_item(&format!("main.added{i}"), "CAP", [4.0, 4.0]));
    }
    for i in 0..2 {
        v.push(pin_item(
            &format!("main.keep0.p{i}"),
            "CAP",
            Some("net:N1"),
            Some(1),
        ));
        v.push(pin_item(
            &format!("main.keep1.p{i}"),
            "CAP",
            Some("net:N2"),
            Some(2),
        ));
        // These two nets are now referenced by nothing that moved onto them and
        // by nothing else, so they leave; two others arrive.
        v.push(pin_item(
            &format!("main.moves0.p{i}"),
            "CAP",
            Some("net:N9"),
            Some(9),
        ));
        v.push(pin_item(
            &format!("main.moves1.p{i}"),
            "CAP",
            Some("net:N10"),
            Some(10),
        ));
        v.push(pin_item(
            &format!("main.added0.p{i}"),
            "CAP",
            Some("net:N9"),
            Some(9),
        ));
        v.push(pin_item(
            &format!("main.added1.p{i}"),
            "CAP",
            Some("net:N10"),
            Some(10),
        ));
    }
    v.push(metric("determinism", "route_geometry_hash", json!("zzz")));
    v.push(metric("determinism", "graph_input_hash", json!("ccc")));
    v.push(metric("scope", "layers", json!(1)));
    v.push(edge_seg(&["main.keep0.p0"], &["main.keep1.p0"]));
    v.push(edge_seg(&["main.keep0.p1"], &["main.keep1.p1"]));
    // Rerouted: different endpoints, so a different key -- a remove and an add.
    v.push(edge_seg(&["main.moves0.p0"], &["main.keep1.p1"]));
    v.push(edge_seg(&["main.moves1.p0"], &["main.keep1.p0"]));
    v.push(wire_seg("N", [0.0, 0.0], [5.0, 0.0], 5.0));
    v.push(wire_seg("N", [0.0, 1.0], [5.0, 1.0], 5.0));
    v.push(keyless_pin(None, "alpha"));
    v.push(keyless_pin(None, "beta"));
    v
}

#[test]
fn every_change_class_has_members() {
    let a = synthetic_world();
    let b = synthetic_world_next();
    let d = VIZ_LAW.diff(&a, &b);

    for (ty, kind) in [
        ("add", "box"),
        ("remove", "box"),
        ("modify", "box"),
        ("modify", "pin"),
        ("add", "net"),
        ("remove", "net"),
        ("add", "segment"),
        ("remove", "segment"),
        ("modify", "metrics"),
    ] {
        assert!(
            count(&d, ty, kind) >= 2,
            "{ty}/{kind} has {} members, needs 2 -- a branch with no members is \
             skipped wholesale and reports as green",
            count(&d, ty, kind)
        );
    }
    assert!(
        d.unaligned.len() >= 2,
        "the unalignable branch needs members"
    );
}

/// A box that only moved must say so with `at` and nothing else. If `key` (the
/// `InstTable` row number) leaks into the comparison, every box in the world
/// reads as changed the moment one instance is inserted.
#[test]
fn a_box_that_only_moved_names_only_its_position() {
    let a = vec![box_item("main.b", "CAP", [1.0, 1.0])];
    let b = vec![box_item("main.b", "CAP", [4.0, 5.0])];
    let d = VIZ_LAW.diff(&a, &b);
    let c = &changes_of(&d)[0];
    assert_eq!(c["type"], "modify");
    assert_eq!(c["kind"], "box");
    assert_eq!(
        c["delta"]["at"],
        json!({ "from": [1.0, 1.0], "to": [4.0, 5.0] })
    );
    assert_eq!(
        c["delta"]
            .as_object()
            .expect("delta")
            .keys()
            .collect::<Vec<_>>(),
        vec!["at"],
        "a move must not report anything else as changed"
    );
    assert_eq!(
        d.stability
            .as_ref()
            .expect("this law draws boxes")
            .unchanged_boxes_total,
        1
    );
    assert_eq!(
        d.stability
            .as_ref()
            .expect("this law draws boxes")
            .unchanged_boxes_moved,
        1
    );
    assert!(
        (d.stability
            .as_ref()
            .expect("this law draws boxes")
            .max_unchanged_box_delta
            - 5.0)
            .abs()
            < 1e-9
    );
}

/// A module replacement is a `modify`, not a delete plus an add -- which is what
/// folding the def into the key would produce.
#[test]
fn a_replaced_def_is_a_modify_not_a_delete_and_an_add() {
    let a = vec![box_item("main.b", "CAP_A", [1.0, 1.0])];
    let b = vec![box_item("main.b", "CAP_B", [1.0, 1.0])];
    let d = VIZ_LAW.diff(&a, &b);
    assert_eq!(count(&d, "add", "box"), 0);
    assert_eq!(count(&d, "remove", "box"), 0);
    assert_eq!(count(&d, "modify", "box"), 1);
    assert_eq!(
        changes_of(&d)[0]["delta"]["def"],
        json!({"from": "CAP_A", "to": "CAP_B"})
    );
}

/// Two builds of the same source from different directories differ in
/// `def.uri` and in `loc.uri` and in nothing that matters. If either counted as
/// content, every item in the world would read as changed.
#[test]
fn a_different_source_path_is_not_a_change() {
    let mut a = vec![box_item("main.b", "CAP", [1.0, 1.0])];
    let mut b = a.clone();
    a[0]["canon_key"]["def"]["uri"] = json!("/one/m.mc");
    b[0]["canon_key"]["def"]["uri"] = json!("/two/m.mc");
    a[0]["loc"] = json!({ "uri": "/one/m.mc", "line": 3, "span": null });
    b[0]["loc"] = json!({ "uri": "/two/m.mc", "line": 3, "span": null });
    let d = VIZ_LAW.diff(&a, &b);
    assert!(
        changes_of(&d).is_empty(),
        "the same item built from a different path read as a change: {:?}",
        changes_of(&d).get(0)
    );
}

// ── Nothing is guessed at ──

/// Two pins that cannot be aligned, on both sides, with the same `key` and
/// `name`. They must be reported as unalignable -- not matched by name, and not
/// silently dropped.
#[test]
fn an_item_with_no_key_is_reported_not_matched_by_name() {
    let a = vec![
        keyless_pin(Some("D1"), "same"),
        keyless_pin(Some("D2"), "same"),
    ];
    let b = vec![
        keyless_pin(Some("D1"), "same"),
        keyless_pin(Some("D2"), "same"),
    ];
    let d = VIZ_LAW.diff(&a, &b);
    assert!(
        changes_of(&d).is_empty(),
        "an unkeyable item was matched anyway: {:?}",
        changes_of(&d).get(0)
    );
    assert_eq!(d.unaligned.len(), 4, "two items, two sides");
    assert!(d.unaligned.iter().all(|u| u["reason"] == "no-key"));
}

/// A metric publishes `canon_key: null` and is still alignable on
/// `(family, field)`. This is the reading that rules out "no canonical key means
/// unalignable" as a general law.
#[test]
fn a_metric_aligns_even_though_it_has_no_canonical_key() {
    let a = vec![
        metric("scope", "layers", json!(7)),
        metric("scope", "boxes", json!(63)),
        metric("determinism", "route_geometry_hash", json!("h")),
    ];
    let b = vec![
        metric("scope", "layers", json!(7)),
        metric("scope", "boxes", json!(63)),
        metric("determinism", "route_geometry_hash", json!("h")),
    ];
    let d = VIZ_LAW.diff(&a, &b);
    assert!(changes_of(&d).is_empty());
    assert!(d.unaligned.is_empty(), "a metric is not unalignable");
}

/// The whole hbl readout has 97 metrics and 10 segments carrying
/// `canon_key: null`, and none of them is a change or an unalignable item.
#[test]
fn the_real_readout_has_no_spurious_unaligned_items() {
    let items = hbl_viz("unaligned");
    let d = VIZ_LAW.diff(&items, &items);
    assert!(
        d.unaligned.is_empty(),
        "{} items in a real readout could not be aligned: {:?}",
        d.unaligned.len(),
        d.unaligned.first()
    );
}

// ── Segments ──

/// The endpoint lists are sorted by the emitter, and the key depends on that:
/// the same route with its lanes listed in another order is the same route.
#[test]
fn a_route_with_its_lanes_in_another_order_is_the_same_route() {
    let a = vec![edge_seg(&["main.a", "main.b"], &["main.c", "main.d"])];
    let b = vec![edge_seg(&["main.b", "main.a"], &["main.d", "main.c"])];
    let d = VIZ_LAW.diff(&a, &b);
    assert!(
        changes_of(&d).is_empty(),
        "a lane reorder read as a change: {:?}",
        changes_of(&d).get(0)
    );
}

/// A segment has no id (ruling O9), so a rerouted segment cannot be told from one
/// deleted plus one added. That is not a gap to be papered over -- it is why
/// "rerouted" is read at the layer level. What must not happen is a `modify`
/// claiming a segment was moved.
#[test]
fn a_rerouted_segment_is_a_remove_and_an_add_never_a_modify() {
    let a = vec![edge_seg(&["main.a"], &["main.b"])];
    let b = vec![edge_seg(&["main.a"], &["main.c"])];
    let d = VIZ_LAW.diff(&a, &b);
    assert_eq!(count(&d, "remove", "segment"), 1);
    assert_eq!(count(&d, "add", "segment"), 1);
    assert_eq!(
        count(&d, "modify", "segment"),
        0,
        "a segment cannot be reported as moved"
    );
    // Rerouting is a layer-level reading.
    assert_eq!(
        d.stability
            .as_ref()
            .expect("this law draws boxes")
            .route_hashes_changed,
        1
    );
}

/// A wire segment has no ends, so its key is where it runs. Measured to have no
/// members on hbl (the device layer does not go through the router), so it is
/// built by hand.
#[test]
fn a_wire_segment_is_keyed_on_where_it_runs() {
    let a = vec![
        wire_seg("N", [0.0, 0.0], [5.0, 0.0], 5.0),
        wire_seg("N", [0.0, 1.0], [5.0, 1.0], 5.0),
    ];
    let mut b = a.clone();
    // Same route, different length: a content change, not a different route.
    b[0]["length"] = json!(7.5);
    let d = VIZ_LAW.diff(&a, &b);
    assert_eq!(count(&d, "modify", "segment"), 1);
    assert_eq!(count(&d, "add", "segment"), 0);
    assert_eq!(count(&d, "remove", "segment"), 0);

    // Moved in space: a different key, so a remove and an add.
    let c = vec![
        wire_seg("N", [9.0, 9.0], [5.0, 0.0], 5.0),
        wire_seg("N", [0.0, 1.0], [5.0, 1.0], 5.0),
    ];
    let d2 = VIZ_LAW.diff(&a, &c);
    assert_eq!(count(&d2, "remove", "segment"), 1);
    assert_eq!(count(&d2, "add", "segment"), 1);
}

// ── Nets are references ──

/// There is no net item, so a net appears and disappears through the pins that
/// reference it.
#[test]
fn a_net_comes_and_goes_with_the_pins_that_reference_it() {
    let a = vec![
        pin_item("main.b.p1", "CAP", Some("net:N1"), Some(1)),
        pin_item("main.b.p2", "CAP", Some("net:N1"), Some(1)),
        pin_item("main.b.p3", "CAP", Some("net:N2"), Some(2)),
        pin_item("main.b.p4", "CAP", Some("net:N2"), Some(2)),
    ];
    let mut b = a.clone();
    for p in b.iter_mut() {
        if p["net"] == "net:N2" {
            p["net"] = json!("net:N3");
            p["nid"] = json!(3);
        }
    }
    let d = VIZ_LAW.diff(&a, &b);
    let nets: Vec<&Value> = changes_of(&d)
        .iter()
        .filter(|c| c["kind"] == "net")
        .collect();
    assert_eq!(nets.len(), 2, "one net out, one net in: {nets:?}");
    assert!(nets
        .iter()
        .any(|c| c["type"] == "remove" && c["id"] == "net:N2"));
    assert!(nets
        .iter()
        .any(|c| c["type"] == "add" && c["id"] == "net:N3"));
    // No net *item* is invented: only these two rows name a net.
    assert_eq!(count(&d, "add", "segment"), 0);
}

/// A nameless net (segment-minted `~`, or anonymous `_net<k>`) has no key and
/// only a build-local `nid`. Keying on `nid` would report a page of comings and
/// goings that are pure renumbering; the contract counts them instead.
#[test]
fn a_nameless_net_is_counted_and_never_listed() {
    let a = vec![
        pin_item("main.b.p1", "CAP", None, Some(1)),
        pin_item("main.b.p2", "CAP", None, Some(1)),
        pin_item("main.b.p3", "CAP", None, Some(2)),
        pin_item("main.b.p4", "CAP", None, Some(2)),
    ];
    // The same four pins on the same two unnamed nets, renumbered -- exactly what
    // a rebuild with one instance inserted upstream produces.
    let b = vec![
        pin_item("main.b.p1", "CAP", None, Some(7)),
        pin_item("main.b.p2", "CAP", None, Some(7)),
        pin_item("main.b.p3", "CAP", None, Some(8)),
        pin_item("main.b.p4", "CAP", None, Some(8)),
    ];
    let d = VIZ_LAW.diff(&a, &b);
    assert_eq!(
        count(&d, "add", "net") + count(&d, "remove", "net"),
        0,
        "renumbering an unnamed net must not read as a net coming or going"
    );
    assert_eq!(
        d.nameless_net_pins
            .expect("this law reads nets off its pins"),
        (4, 4)
    );

    // One fewer pin on an unnamed net is a count, not a list.
    let c = vec![
        pin_item("main.b.p1", "CAP", None, Some(7)),
        pin_item("main.b.p2", "CAP", None, Some(7)),
        pin_item("main.b.p3", "CAP", None, Some(8)),
    ];
    let d2 = VIZ_LAW.diff(&a, &c);
    assert_eq!(
        d2.nameless_net_pins
            .expect("this law reads nets off its pins"),
        (4, 3)
    );
    assert_eq!(count(&d2, "add", "net") + count(&d2, "remove", "net"), 0);
}

// ── Locality ──

#[test]
fn locality_has_both_branches() {
    // Four boxes that did not change and did not move: local.
    let a: Vec<Value> = (0..LOCALITY_MIN_SAMPLE)
        .map(|i| box_item(&format!("main.b{i}"), "CAP", [1.0, 1.0]))
        .collect();
    let d = VIZ_LAW.diff(&a, &a);
    assert_eq!(
        d.stability
            .as_ref()
            .expect("this law draws boxes")
            .unchanged_boxes_total,
        LOCALITY_MIN_SAMPLE
    );
    assert!(
        !d.stability
            .as_ref()
            .expect("this law draws boxes")
            .locality_warning
    );

    // The same four, all of them moved: the change was not local.
    let mut b = a.clone();
    for (i, item) in b.iter_mut().enumerate() {
        item["at"] = json!([100.0 + i as f64, 100.0]);
    }
    let d2 = VIZ_LAW.diff(&a, &b);
    assert_eq!(
        d2.stability
            .as_ref()
            .expect("this law draws boxes")
            .unchanged_boxes_moved,
        LOCALITY_MIN_SAMPLE
    );
    assert!(
        d2.stability
            .as_ref()
            .expect("this law draws boxes")
            .locality_warning
    );

    // Below the sample size, "most of them moved" is one box and says nothing.
    let small: Vec<Value> = (0..LOCALITY_MIN_SAMPLE - 1)
        .map(|i| box_item(&format!("main.b{i}"), "CAP", [1.0, 1.0]))
        .collect();
    let mut moved = small.clone();
    for item in moved.iter_mut() {
        item["at"] = json!([100.0, 100.0]);
    }
    let d3 = VIZ_LAW.diff(&small, &moved);
    assert!(
        !d3.stability
            .as_ref()
            .expect("this law draws boxes")
            .locality_warning
    );
}

// ── The real readout ──

/// The fixture the whole batch is about: one instance inserted *ahead* of three
/// others. Insertion is what renumbers every `D{}` in the world, so this is where
/// a whole-item comparison would fall over.
#[test]
fn inserting_an_instance_does_not_erase_the_boxes_that_did_not_change() {
    let a = viz_on("insert-base", BASE_SRC);
    let b = viz_on("insert-inserted", INSERTED_SRC);
    let d = VIZ_LAW.diff(&a, &b);

    assert_eq!(count(&d, "remove", "box"), 0, "nothing was removed");
    assert_eq!(
        count(&d, "add", "box"),
        1,
        "exactly the inserted instance appears"
    );
    let added = changes_of(&d)
        .iter()
        .find(|c| c["type"] == "add" && c["kind"] == "box")
        .expect("the inserted box");
    assert!(
        added["id"].as_str().unwrap_or("").ends_with("c0"),
        "the added box is the inserted one, got {}",
        added["id"]
    );

    // Every other change is either a hash metric (inserting a box does change
    // the box order, and that is what these rows are for) or a box that moved --
    // never a box that changed in name, size, or pin count, which is what a
    // `D{}` row-number shift would have smeared into a whole-item comparison.
    // A pin is the sharpest probe here: its `key` and `point` are the ordinal of
    // first interning, so an insertion renumbers them. If either were compared,
    // pins would light up. None may change.
    assert_eq!(
        count(&d, "modify", "pin") + count(&d, "remove", "pin"),
        0,
        "an insertion must not change the pins that were already there"
    );
    for c in changes_of(&d) {
        if c["type"] == "remove" {
            // The drawn-wire face, not the circuit: b3719's wire dedupe redraws
            // a coincident run when its payload changes, and the whole-item
            // differ spells a redraw remove + add of one id. Only a segment has
            // that face — a box or a pin taken out would be the law broken.
            assert_eq!(
                c["kind"], "segment",
                "an insertion must not remove anything but a redrawn wire run: {c}"
            );
            assert!(
                changes_of(&d)
                    .iter()
                    .any(|o| o["type"] == "add" && o["id"] == c["id"]),
                "a removed segment must be a redraw — the same id added back: {c}"
            );
            continue;
        }
        if c["type"] == "add" {
            continue;
        }
        if c["kind"] == "box" && c["type"] == "modify" {
            assert_eq!(
                c["delta"]
                    .as_object()
                    .expect("delta")
                    .keys()
                    .collect::<Vec<_>>(),
                vec!["at"],
                "a box that only moved reported something else as changed: {c}"
            );
            continue;
        }
        // A layer's counts and a report hash are expected to move when a box is
        // added; nothing else is.
        assert!(
            matches!(c["kind"].as_str(), Some("layer") | Some("metrics")),
            "unexpected change from an insertion: {c}"
        );
    }

    assert!(
        d.stability
            .as_ref()
            .expect("this law draws boxes")
            .unchanged_boxes_total
            >= 3,
        "c1/c2/c3 did not change, so they must still count as unchanged: {:?}",
        d.stability
    );
}

/// The same board read twice: nothing changed, and the difference says so.
#[test]
fn the_real_readout_does_not_differ_from_itself() {
    let a = hbl_viz("hbl-a");
    let b = hbl_viz("hbl-b");
    let d = VIZ_LAW.diff(&a, &b);
    assert!(
        changes_of(&d).is_empty(),
        "two readings of one board differ: {:?}",
        changes_of(&d).get(0)
    );
    assert_eq!(
        d.stability
            .as_ref()
            .expect("this law draws boxes")
            .route_hashes_changed,
        0
    );
    assert!(
        !d.stability
            .as_ref()
            .expect("this law draws boxes")
            .locality_warning
    );
}
