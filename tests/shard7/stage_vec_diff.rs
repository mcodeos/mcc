// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `stage.vec` law: the first segment whose objects are not all instances.
//!
//! Design §2.4 says each class here carries the key its *kind* owns and nothing
//! is invented for the kinds that own none: a layer is a `bid`, a box an
//! instance, an endpoint a pin, a net a member set, a trunk no id at all. So
//! this law is the first with three key functions in one table, and each of the
//! three has a decision worth pinning:
//!
//! * A **trunk** owns no id, so it is keyed on the `path` the emitter gives it
//!   (its name qualified by its layer) -- the design's own words, not a
//!   fallback. Its lanes are compared as a **set**: the lane index comes either
//!   from the source's bracket lane or from the member's position in the trunk,
//!   so comparing the raw array would read one inserted member as every later
//!   lane having changed.
//! * An **endpoint** is keyed on its pin's canonical path, never on the net it
//!   sits on: on `hbl`, 84 of 186 endpoints sit on a name the builder minted
//!   (`_net<k>`), and two builds do not agree on those. Where a pin is an
//!   endpoint of several nets the key is not unique, and the row is reported as
//!   `duplicate-key` rather than disambiguated by a name.
//! * A **projection record** states its identity as three components the emitter
//!   publishes (`layer`, `net`, `endpoint`) -- not as the `path` it also
//!   publishes, which is those three joined. A row whose net is a minted name is
//!   **refused** (`net_origin`: keying on it "would claim a stability the name
//!   does not have"), the same answer the drawing gives a pin with no
//!   `canon_key`. Measured: 4 records per side on `hbl`, 0 on `hbl1`, 2 on `hs`.
//!
//! The branches the given circuits cannot reach are built by hand and marked as
//! such; the rest runs the real readout, because a synthetic circuit gives the
//! emitter's own vocabulary nothing to say.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use mcc::stages::stage_diff::{StageDiff, VEC_LAW};
use mcc::stages::{net_key, net_origin};

/// A small two-pin circuit: three parts on three nets, with one trunk.
const BASE_SRC: &str = r#"component CAP(cap::INT) {
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

/// The same circuit with one more part inserted **before** the existing three.
/// Nothing about `c1`/`c2`/`c3` changed, so anything this difference reports
/// about them would be renumbering read as change.
const INSERTED_SRC: &str = r#"component CAP(cap::INT) {
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

// ── Running the real readout ──

fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

/// A fresh, **empty** directory to run a CLI invocation in: `viz/project.rs`
/// writes `baseline/render_projection.md` relative to the cwd.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-vec-diff-{name}-{}-{:?}",
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

fn items_of(stdout: &str) -> Vec<Value> {
    let envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"]["items"]
        .as_array()
        .expect("items is an array")
        .clone()
}

/// `show stage vec` over a source written into a scratch dir.
fn vec_on(name: &str, source: &str) -> (Vec<Value>, PathBuf, PathBuf) {
    let dir = scratch(name);
    let path = dir.join("circuit.mc");
    std::fs::write(&path, source).expect("write the source");
    let (stdout, stderr, ok) = run(
        &dir,
        &[
            "--local",
            "show",
            "stage",
            "vec",
            "-f",
            "json",
            "-F",
            path.to_str().expect("source path"),
        ],
    );
    assert!(ok, "show stage vec failed: {stderr}");
    (items_of(&stdout), path, dir)
}

/// `show stage vec` over the five-file hbl project.
fn hbl_vec(name: &str) -> Vec<Value> {
    let dir = scratch(name);
    let entry = hbl_entry();
    let (stdout, stderr, ok) = run(
        &dir,
        &[
            "--local",
            "show",
            "stage",
            "vec",
            "-f",
            "json",
            "-F",
            entry.to_str().expect("fixture path"),
        ],
    );
    assert!(ok, "show stage vec failed: {stderr}");
    items_of(&stdout)
}

/// `mcc diff <a> <b> --view stage.vec`, returning the `result.stage` block.
fn diff(cwd: &Path, a: &Path, b: &Path) -> Value {
    let (stdout, stderr, ok) = run(
        cwd,
        &[
            "--local",
            "diff",
            a.to_str().expect("path a"),
            b.to_str().expect("path b"),
            "--view",
            "stage.vec",
            "-f",
            "json",
        ],
    );
    assert!(ok, "`mcc diff --view stage.vec` failed: {stderr}");
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

/// The `(type, kind, id)` of every change row, in the order published.
fn triples(stage: &Value) -> Vec<(String, String, String)> {
    stage["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|c| {
            (
                c["type"].as_str().unwrap_or("-").to_string(),
                c["kind"].as_str().unwrap_or("-").to_string(),
                c["id"].as_str().unwrap_or("-").to_string(),
            )
        })
        .collect()
}

fn count(d: &StageDiff, ty: &str, kind: &str) -> usize {
    d.changes
        .iter()
        .filter(|c| c["type"] == ty && c["kind"] == kind)
        .count()
}

/// The change row for one `(type, kind)`, which must be there exactly once.
fn one(d: &StageDiff, ty: &str, kind: &str) -> Value {
    let found: Vec<&Value> = d
        .changes
        .iter()
        .filter(|c| c["type"] == ty && c["kind"] == kind)
        .collect();
    assert_eq!(found.len(), 1, "{ty} {kind}: {:?}", d.changes);
    found[0].clone()
}

fn reasons(d: &StageDiff) -> Vec<String> {
    d.unaligned
        .iter()
        .map(|u| u["reason"].as_str().unwrap_or("-").to_string())
        .collect()
}

// ── Synthetic items ──
//
// Minimum-shape items carrying only the fields the law reads, so a test says
// which branch it is about rather than which fields happened to be present.

fn ck(path: &str, ident: &str) -> Value {
    json!({ "path": path, "def": { "uri": "/src/m.mc", "ident": ident } })
}

fn box_(path: &str, ident: &str, class_name: &str, pins: usize) -> Value {
    json!({
        "class": "box", "key": "D1", "point": null, "path": path,
        "canon_key": ck(path, ident), "name": path.rsplit('.').next().unwrap_or(path),
        "class_name": class_name, "kind": "component", "pins": pins,
        "layer": "main", "loc": null,
    })
}

fn layer(path: &str, boxes: usize, nets: usize) -> Value {
    json!({
        "class": "layer", "key": "D0", "point": null, "path": path,
        "canon_key": ck(path, path), "name": path, "style": "block",
        "boxes": boxes, "nets": nets, "root": true, "loc": null,
    })
}

/// An endpoint row: the pin's canonical path is its identity, and `net` is the
/// bare name of the net it is on -- which is exactly what the law does **not**
/// compare.
fn endpoint(pin_path: &str, pin: &str, io: &str, net: &str) -> Value {
    json!({
        "class": "endpoint", "key": "N4:0", "point": "N4:0", "path": pin_path,
        "canon_key": ck(pin_path, "AMP"), "pin": pin, "io": io,
        "net": net, "layer": "main", "loc": null,
    })
}

/// A net row. `name` is `None` for a net the builder named, which is what the
/// emitter marks with a null `key` and a non-`source` `origin`.
///
/// The classification is the engine's own and not a second copy of the rule:
/// whether a name is the source's is the whole question here, and a fixture that
/// decided it by its own spelling test could agree with the emitter while the
/// law read something else.
fn net(name: Option<&str>, members: &[&str]) -> Value {
    let origin = name.map(|n| net_origin(n).as_str()).unwrap_or("anonymous");
    let key = name.and_then(net_key);
    json!({
        "class": "net", "key": key, "point": null, "path": null, "canon_key": null,
        "name": name, "origin": origin, "kind": "signal", "role": "signal",
        "nid": 3, "members": members, "endpoints": members.len(),
        "layer": "main", "loc": null,
    })
}

/// A trunk: `(member, lane, left, right)` per lane, in the order given.
fn trunk(path: &str, lanes: &[(&str, u16, &str, &str)]) -> Value {
    let lanes: Vec<Value> = lanes
        .iter()
        .map(|(m, l, left, right)| json!({ "member": m, "lane": l, "left": left, "right": right }))
        .collect();
    json!({
        "class": "trunk", "key": null, "point": null, "path": path, "canon_key": null,
        "name": path.rsplit('.').next().unwrap_or(path), "kind": "bus", "op": "series",
        "dir": "->", "lanes": lanes, "loc": null,
    })
}

/// The per-layer projection row: the one carrying the count pair.
fn proj_layer(layer: &str, before: u64, after: u64) -> Value {
    json!({
        "class": "projection", "key": null, "point": null,
        "path": format!("{layer}/nets"), "canon_key": null, "layer": layer,
        "net": null, "endpoint": null, "rule": "-",
        "before": before, "after": after, "note": "-", "loc": null,
    })
}

/// One projection action record. `note` is written the way the emitter writes
/// it, id and all, because that id is the reason the law does not compare it.
fn proj_record(layer: &str, net: &str, endpoint: &str, rule: &str) -> Value {
    json!({
        "class": "projection", "key": null, "point": null,
        "path": format!("{layer}/{net}/{endpoint}"), "canon_key": null, "layer": layer,
        "net": net, "endpoint": endpoint, "rule": rule,
        "before": null, "after": null,
        "note": format!("boundary port group (id=79), kept as PortTerminal marker"),
        "loc": null,
    })
}

/// One reading with every class the law names, at least two members each, so a
/// self-difference says something about all six and not just the easy ones.
fn mixed() -> Vec<Value> {
    vec![
        box_("main.u1", "AMP", "AMP", 3),
        box_("main.u2", "AMP", "AMP", 2),
        layer("main", 2, 3),
        endpoint("main.u1.1", "1", "passive", "VDD"),
        endpoint("main.u1.2", "2", "passive", "_net1"),
        net(Some("VDD"), &["main.u1.1", "main.u2.1"]),
        net(None, &["main.u1.2", "main.u2.2"]),
        trunk("main.u2", &[("1", 0, "main.u2.1", "main.u1.1")]),
        proj_layer("main", 3, 2),
        proj_record("main", "VDD", "VDD", "c"),
    ]
}

// ── A reading does not differ from itself ──

/// The strongest single statement: the same items compared twice produce
/// nothing, across every class the law names.
#[test]
fn a_reading_does_not_differ_from_itself() {
    let items = mixed();
    let d = VEC_LAW.diff(&items, &items);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
}

/// The two blocks the drawing publishes are the law's, and this law does not
/// have them. `None` and not a zeroed report: this segment draws no boxes in the
/// drawing sense -- a `box` here carries no position at all -- so a stability
/// summary would be a claim it cannot make, and its nets are items keyed on
/// their members, so there are no net references to count.
#[test]
fn the_two_blocks_of_the_drawing_are_not_this_laws() {
    let items = mixed();
    let d = VEC_LAW.diff(&items, &items);

    assert!(d.stability.is_none(), "stability: {:?}", d.stability);
    assert!(d.nameless_net_pins.is_none(), "{:?}", d.nameless_net_pins);
}

/// A class the law does not name takes no part in the difference -- and is not
/// reported as unalignable either, which is the difference between "not this
/// law's class" and "this law's class, with no key".
#[test]
fn a_class_the_law_does_not_name_takes_no_part() {
    let a = vec![
        json!({ "class": "metrics", "family": "determinism", "field": "box_order_hash", "value": 1 }),
        json!({ "class": "segment", "kind": "edge", "from": [], "to": [] }),
    ];
    let b = vec![
        json!({ "class": "metrics", "family": "determinism", "field": "box_order_hash", "value": 2 }),
        json!({ "class": "segment", "kind": "wire", "net": "N", "from_at": [0, 0], "to_at": [1, 1] }),
    ];
    let d = VEC_LAW.diff(&a, &b);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
}

// ── The three key functions ──

/// **A trunk is keyed on its path**, which is design §2.4's own ruling ("a trunk
/// owns no id. Its name and its lanes are what it is") and not a fallback to a
/// name: the path is that name qualified by its layer.
#[test]
fn a_trunk_is_keyed_on_its_path() {
    let a = vec![
        trunk("main.t1", &[("A", 0, "main.u1.1", "main.u2.1")]),
        trunk("main.t2", &[("A", 0, "main.u1.2", "main.u2.2")]),
    ];
    let b = vec![
        trunk("main.t2", &[("A", 0, "main.u1.2", "main.u2.2")]),
        trunk("main.t3", &[("A", 0, "main.u1.3", "main.u2.3")]),
    ];
    let d = VEC_LAW.diff(&a, &b);

    assert_eq!(count(&d, "remove", "trunk"), 1, "{:?}", d.changes);
    assert_eq!(count(&d, "add", "trunk"), 1, "{:?}", d.changes);
    assert_eq!(one(&d, "remove", "trunk")["id"], "main.t1");
    assert_eq!(one(&d, "add", "trunk")["id"], "main.t3");
}

/// The lane **index** is not compared, because the producer assigns it either
/// from the source's bracket lane or from the member's position in the trunk --
/// a position in this build's append order. Comparing the raw array would read
/// one inserted member as every later lane having changed.
#[test]
fn a_trunks_lanes_are_compared_as_a_set() {
    let a = vec![trunk(
        "main.t1",
        &[
            ("A", 0, "main.u1.1", "main.u2.1"),
            ("B", 1, "main.u1.2", "main.u2.2"),
        ],
    )];
    // The same two lanes, renumbered, and listed in the other order.
    let b = vec![trunk(
        "main.t1",
        &[
            ("B", 0, "main.u1.2", "main.u2.2"),
            ("A", 7, "main.u1.1", "main.u2.1"),
        ],
    )];
    let d = VEC_LAW.diff(&a, &b);
    assert!(
        d.changes.is_empty(),
        "a renumbering is not a change: {:?}",
        d.changes
    );

    // A lane that actually moved is one, though.
    let c = vec![trunk(
        "main.t1",
        &[
            ("A", 0, "main.u1.1", "main.u2.9"),
            ("B", 1, "main.u1.2", "main.u2.2"),
        ],
    )];
    let d = VEC_LAW.diff(&a, &c);
    let m = one(&d, "modify", "trunk");
    assert!(
        m["delta"]["lane_pairs"].is_object(),
        "the lane set is what changed: {m:?}"
    );
}

/// A trunk's own `kind`, `op` and `dir` are content, so a combination order
/// change reads on the trunk row rather than nowhere.
#[test]
fn a_trunks_combination_is_content() {
    let a = vec![trunk("main.t1", &[("A", 0, "main.u1.1", "main.u2.1")])];
    let mut b = a.clone();
    b[0]["dir"] = json!("--");
    b[0]["op"] = json!("parallel");

    let d = VEC_LAW.diff(&a, &b);
    let m = one(&d, "modify", "trunk");
    assert_eq!(m["delta"]["dir"]["from"], "->");
    assert_eq!(m["delta"]["dir"]["to"], "--");
    assert_eq!(m["delta"]["op"]["to"], "parallel");
}

/// **An endpoint is keyed on its pin**, and two rows of one pin are reported as
/// `duplicate-key` rather than told apart by the net's name. A pin that is an
/// endpoint of several nets is a real shape -- measured on `hs`, 14 groups over
/// 19 rows, every one of them involving a minted net name -- and a name the
/// builder minted is not an identity to disambiguate with.
#[test]
fn an_endpoint_is_keyed_on_its_pin_and_not_on_its_net() {
    let a = vec![
        endpoint("main.u1.1", "1", "passive", "VDD"),
        endpoint("main.u1.1", "1", "passive", "_net5"),
    ];
    let d = VEC_LAW.diff(&a, &a);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert_eq!(d.unaligned.len(), 2, "{:?}", d.unaligned);
    assert!(
        reasons(&d).iter().all(|r| r == "duplicate-key"),
        "{:?}",
        d.unaligned
    );
    assert!(d.unaligned.iter().all(|u| u["class"] == "endpoint"));
}

/// An endpoint's `net` is a **bare name**, and the law does not compare it --
/// measured, 84 of the 186 endpoints on `hbl` sit on a name the builder minted,
/// so comparing it would read a renumbering as every one of those endpoints
/// having moved. The membership is not lost: it is the net row's member list,
/// which is that row's key.
#[test]
fn an_endpoints_net_name_is_not_compared() {
    let a = vec![
        endpoint("main.u1.1", "1", "passive", "_net5"),
        endpoint("main.u2.1", "1", "passive", "VDD"),
    ];
    let b = vec![
        endpoint("main.u1.1", "1", "passive", "_net9"),
        endpoint("main.u2.1", "1", "passive", "GND"),
    ];
    let d = VEC_LAW.diff(&a, &b);
    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
}

/// A net is keyed on its **member set** -- the same key `stage.p2` gives the
/// same nets -- and its name is **content**, but only when the emitter says the
/// name is the source's. An anonymous row renamed by the builder is not a
/// change, because the two builds do not agree on that name.
#[test]
fn a_net_is_keyed_on_its_members_and_its_name_is_content() {
    let a = vec![
        net(Some("VDD"), &["main.u1.1", "main.u2.1"]),
        net(None, &["main.u1.2", "main.u2.2"]),
    ];
    let mut b = a.clone();
    b[0]["name"] = json!("VBAT");
    b[0]["key"] = json!("net:VBAT");
    b[1]["name"] = json!("_net9");

    let d = VEC_LAW.diff(&a, &b);
    let m = one(&d, "modify", "net");
    assert_eq!(m["delta"]["name"]["from"], "VDD");
    assert_eq!(m["delta"]["name"]["to"], "VBAT");
    assert_eq!(
        count(&d, "modify", "net"),
        1,
        "a rename is one change: {:?}",
        d.changes
    );
}

/// The member set **is** the identity, so a net that gained a member reads as a
/// removal and an addition rather than as a change. That is the honest cost of
/// keying on membership, and it is the same reading the drawing's own count
/// cannot give: this segment reports it row by row.
#[test]
fn a_net_that_gained_a_member_is_a_removal_and_an_addition() {
    let a = vec![net(Some("VDD"), &["main.u1.1"])];
    let b = vec![net(Some("VDD"), &["main.u1.1", "main.u2.1"])];
    let d = VEC_LAW.diff(&a, &b);

    assert_eq!(count(&d, "remove", "net"), 1, "{:?}", d.changes);
    assert_eq!(count(&d, "add", "net"), 1, "{:?}", d.changes);
    // The handle carries the members, because the name is the same on both
    // sides: two rows a reader cannot tell apart would be the defect the header
    // rule forbids.
    assert_eq!(one(&d, "remove", "net")["id"], "net:VDD main.u1.1");
    assert_eq!(one(&d, "add", "net")["id"], "net:VDD main.u1.1, main.u2.1");
}

/// A net with no members has nothing to be keyed on: the design names the blind
/// spot ("a net that is declared but carries no pins") and it is reported rather
/// than given a synthesised key.
#[test]
fn a_net_with_no_members_cannot_be_aligned() {
    let a = vec![net(Some("A"), &[]), net(Some("B"), &[])];
    let d = VEC_LAW.diff(&a, &a);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert_eq!(
        d.unaligned.len(),
        4,
        "both sides, both rows: {:?}",
        d.unaligned
    );
    assert!(
        reasons(&d).iter().all(|r| r == "no-key"),
        "{:?}",
        d.unaligned
    );
}

/// A box with no canonical path (a synthesised box the builder invented) has no
/// key either.
#[test]
fn a_box_with_no_canonical_path_cannot_be_aligned() {
    let a = vec![
        json!({ "class": "box", "path": "", "canon_key": { "path": "", "def": null } }),
        json!({ "class": "box", "path": "main.u2", "canon_key": null }),
    ];
    let d = VEC_LAW.diff(&a, &a);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert_eq!(d.unaligned.len(), 4, "{:?}", d.unaligned);
    assert!(
        reasons(&d).iter().all(|r| r == "no-key"),
        "{:?}",
        d.unaligned
    );
}

/// Build-local ordinals are not compared: `key` (`D{n}`), `point` (`N{n}:{k}`),
/// `nid` and `loc` all shift when an instance is inserted ahead of another.
///
/// A **net's** `key` is left alone, and that is the point of singling it out:
/// here it is not an ordinal but the net's *authored name* (`net:VDD`), which is
/// content. The two kinds of key live in one field whose meaning is the class's,
/// which is why this law reads them per class rather than by field name.
#[test]
fn build_local_ordinals_are_not_compared() {
    let a = mixed();
    let mut b = a.clone();
    for item in b.iter_mut() {
        let obj = item.as_object_mut().expect("an item is an object");
        if obj.get("class").and_then(Value::as_str) != Some("net") && obj.contains_key("key") {
            obj.insert("key".into(), json!("D900"));
        }
        if obj.contains_key("point") {
            obj.insert("point".into(), json!("N900:7"));
        }
        if obj.contains_key("nid") {
            obj.insert("nid".into(), json!(900));
        }
        if obj.contains_key("loc") {
            obj.insert(
                "loc".into(),
                json!({ "line": 900, "span": null, "uri": "/elsewhere.mc" }),
            );
        }
    }
    let d = VEC_LAW.diff(&a, &b);
    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
}

/// A layer is keyed on its path and compared by its counts, so a layer that
/// gained a box reads as one change on the layer row.
#[test]
fn a_layer_is_compared_by_its_counts() {
    let a = vec![layer("main", 2, 3)];
    let b = vec![layer("main", 3, 3)];
    let d = VEC_LAW.diff(&a, &b);

    let m = one(&d, "modify", "layer");
    assert_eq!(m["delta"]["boxes"]["from"], 2);
    assert_eq!(m["delta"]["boxes"]["to"], 3);
    assert_eq!(m["id"], "main");
}

// ── The projection class: two families under one class name ──

/// The emitter puts two structurally different rows under `projection`. They are
/// told apart by the **count pair**, which only the per-layer row carries -- a
/// row whose pair is `0 -> 0` is still a per-layer row, because the test is the
/// pair's presence and not its value.
#[test]
fn a_per_layer_row_is_the_one_with_the_count_pair() {
    let a = vec![proj_layer("main", 3, 2), proj_layer("usb", 0, 0)];
    let b = vec![proj_layer("main", 3, 1), proj_layer("usb", 0, 1)];
    let d = VEC_LAW.diff(&a, &b);

    assert_eq!(count(&d, "modify", "projection"), 2, "{:?}", d.changes);
    let ids: Vec<&str> = d
        .changes
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["main/nets", "usb/nets"], "sorted by id: {ids:?}");
    assert_eq!(count(&d, "add", "projection"), 0, "{:?}", d.changes);
}

/// An action record's identity is the three components the emitter publishes,
/// not the `path` it also publishes -- which is those three joined. A net whose
/// name is the source's may key a row; a name the builder minted may not.
#[test]
fn a_minted_net_name_in_a_record_is_refused() {
    let a = vec![
        proj_record("MIC", "_net5", "MIC", "c"),
        proj_record("MCU513", "SPI.SCLK~0", "SPI", "c"),
    ];
    let d = VEC_LAW.diff(&a, &a);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert_eq!(
        d.unaligned.len(),
        4,
        "both sides, both rows: {:?}",
        d.unaligned
    );
    assert!(
        reasons(&d).iter().all(|r| r == "no-key"),
        "{:?}",
        d.unaligned
    );
    assert!(d.unaligned.iter().all(|u| u["class"] == "projection"));
}

/// …and a record whose net name **is** the source's is keyed on it, so two
/// records of one layer and endpoint but different nets are two rows and not a
/// collision.
#[test]
fn an_authored_net_name_keys_a_record() {
    let a = vec![
        proj_record("main", "VDD", "VDD", "c"),
        proj_record("main", "GND", "VDD", "c"),
    ];
    let d = VEC_LAW.diff(&a, &a);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
}

/// A record's `rule` is content, so the same action reclassified reads as a
/// change; its `note` is **not**, because that prose embeds a builder id
/// (`port_group_id`) -- an ordinal assigned during construction.
#[test]
fn a_records_note_is_not_compared() {
    let a = vec![proj_record("main", "VDD", "VDD", "c")];

    let mut noted = a.clone();
    noted[0]["note"] = json!("supply boundary port group (id=412), kept as PortTerminal marker");
    let d = VEC_LAW.diff(&a, &noted);
    assert!(
        d.changes.is_empty(),
        "a note is not a change: {:?}",
        d.changes
    );

    let mut ruled = a.clone();
    ruled[0]["rule"] = json!("b");
    let d = VEC_LAW.diff(&a, &ruled);
    let m = one(&d, "modify", "projection");
    assert_eq!(m["delta"]["rule"]["from"], "c");
    assert_eq!(m["delta"]["rule"]["to"], "b");
}

/// The two families do not share a key space: a record that moved to another
/// layer is a removal and an addition, not a change of the layer row it would
/// collide with.
#[test]
fn the_two_projection_families_do_not_share_a_key() {
    let a = vec![
        proj_layer("main", 3, 2),
        proj_record("main", "VDD", "VDD", "c"),
    ];
    let b = vec![
        proj_layer("main", 3, 2),
        proj_record("usb", "VDD", "VDD", "c"),
    ];
    let d = VEC_LAW.diff(&a, &b);

    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
    assert_eq!(count(&d, "remove", "projection"), 1, "{:?}", d.changes);
    assert_eq!(count(&d, "add", "projection"), 1, "{:?}", d.changes);
    assert_eq!(one(&d, "remove", "projection")["id"], "main/VDD/VDD");
}

// ── The real readout ──

/// Two readings of one source are two worlds and still not two *states*: the
/// difference is empty, and the command says so in its own vocabulary.
#[test]
fn two_readings_of_one_source_do_not_differ() {
    let (_, path, dir) = vec_on("self", BASE_SRC);
    let stage = diff(&dir, &path, &path);

    assert_eq!(triples(&stage).len(), 0, "{stage}");
    assert_eq!(stage["counts"]["unaligned"], 0, "{stage}");
    assert_eq!(stage["view"], "diff.stage.vec");
}

/// The insert probe: one part added **ahead** of the others. Nothing about the
/// parts already there changed, so nothing about them may be reported --
/// including their boxes, whose `key` is an `InstTable` row number that the
/// insertion shifts.
#[test]
fn inserting_an_instance_reports_only_that_instance() {
    let (_, a, dir) = vec_on("insert-a", BASE_SRC);
    let src_dir = dir.join("ins");
    std::fs::create_dir_all(&src_dir).expect("create dir");
    let b = src_dir.join("circuit.mc");
    std::fs::write(&b, INSERTED_SRC).expect("write the source");

    let stage = diff(&dir, &a, &b);
    let rows = triples(&stage);

    assert!(
        rows.contains(&("add".into(), "box".into(), "main.c0".into())),
        "{rows:?}"
    );
    assert_eq!(
        rows.iter()
            .filter(|(t, k, _)| t == "add" && k == "box")
            .count(),
        1,
        "exactly one box is new: {rows:?}"
    );
    // Its two pins are new too, and each is one endpoint row on the pin's own
    // path -- not on the net it sits on, which `c0` merely joined.
    for pin in ["main.c0.1", "main.c0.2"] {
        assert!(
            rows.contains(&("add".into(), "endpoint".into(), pin.into())),
            "{pin} is new: {rows:?}"
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|(t, k, _)| t == "add" && k == "endpoint")
            .count(),
        2,
        "{rows:?}"
    );
    assert_eq!(
        rows.iter().filter(|(t, _, _)| t == "modify").count(),
        1,
        "the only modification is the layer's own count: {rows:?}"
    );
    assert!(
        rows.iter()
            .any(|(t, k, id)| t == "modify" && k == "layer" && id == "main"),
        "{rows:?}"
    );
}

/// The cost of keying a net on its members, on a real readout: the nets the new
/// part joined read as a removal and an addition, while the part that was
/// already there still reports nothing.
#[test]
fn the_nets_the_new_part_joined_read_as_removal_and_addition() {
    let (_, a, dir) = vec_on("members-a", BASE_SRC);
    let src_dir = dir.join("ins");
    std::fs::create_dir_all(&src_dir).expect("create dir");
    let b = src_dir.join("circuit.mc");
    std::fs::write(&b, INSERTED_SRC).expect("write the source");

    let stage = diff(&dir, &a, &b);
    let rows = triples(&stage);

    assert_eq!(
        rows.iter()
            .filter(|(t, k, _)| t == "remove" && k == "net")
            .count(),
        2,
        "the two nets it joined: {rows:?}"
    );
    assert_eq!(
        rows.iter()
            .filter(|(t, k, _)| t == "add" && k == "net")
            .count(),
        2,
        "{rows:?}"
    );
    // The handle prints the members, so the two rows of one net are readable as
    // what they are: the same name over a different member list.
    let ids: Vec<&str> = rows
        .iter()
        .filter(|(t, k, _)| t == "remove" && k == "net")
        .map(|(_, _, id)| id.as_str())
        .collect();
    assert!(
        ids.iter().any(|id| id.starts_with("net:VDD "))
            && ids.iter().any(|id| id.starts_with("net:GND ")),
        "{ids:?}"
    );
}

/// The real board, where the law's one refusal is exercised: `hbl` has records
/// whose net the builder minted, and they are reported as unalignable on both
/// sides rather than keyed on the name. Measured: 4 per side.
#[test]
fn a_real_board_refuses_only_its_minted_net_records() {
    let items = hbl_vec("hbl");
    let d = VEC_LAW.diff(&items, &items);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert_eq!(d.unaligned.len(), 8, "4 per side: {:?}", d.unaligned);
    assert!(
        reasons(&d).iter().all(|r| r == "no-key"),
        "{:?}",
        d.unaligned
    );
    assert!(
        d.unaligned.iter().all(|u| u["class"] == "projection"),
        "{:?}",
        d.unaligned
    );
    let sides: Vec<&str> = d
        .unaligned
        .iter()
        .map(|u| u["side"].as_str().unwrap())
        .collect();
    assert_eq!(
        sides.iter().filter(|s| **s == "a").count(),
        4,
        "each side reports its own: {sides:?}"
    );
}
