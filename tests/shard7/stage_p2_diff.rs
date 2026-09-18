// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `stage.p2` law: what a difference of two flat electrical truths keys on,
//! and what it refuses to compare.
//!
//! The law is not the drawing's with other class names. Two of its decisions are
//! its own, and both are things a naive implementation gets wrong:
//!
//! * A **net row is an item here**, where the drawing only ever sees a net as a
//!   reference from a pin. So a net can be keyed on its **member list** -- the
//!   list of canonical paths it joins -- and a net with no authored name still
//!   aligns. The drawing cannot do that, and needs its nameless-pin count
//!   instead. That is why the drawing publishes that count and this law does not.
//! * A name is **content, not a key**, for the same reason a box's def is: the
//!   emitter's own `key` is `net:<name>`, and measured on `hbl` the two nets
//!   named `GND` -- one per scope -- both publish `net:GND`. 32 of the 59 net
//!   rows carry a name but only 25 names are distinct, so keying on it would
//!   leave 14 rows in seven `duplicate-key` pairs; comparing it as content is
//!   what makes a rename a `modify` rather than a delete plus an add.
//!
//! The branches the two given boards cannot reach are built by hand and marked
//! as such. Everything else runs the real readout, because a synthetic circuit
//! gives sorting and scope collisions nothing to work with.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use mcc::stages::stage_diff::{StageDiff, P2_LAW};

/// A small two-pin circuit: three parts on two nets.
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

/// The same circuit with `c1` **replaced** by a part of another class. The part
/// keeps its path and its two pins, so the replacement is visible as content and
/// not as a rebuild of everything around it.
const REPLACED_SRC: &str = r#"component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([n1, n2]) {
        n1 - this - n2
    }
}
component RES(r::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
    func Res([n1, n2]) {
        n1 - this - n2
    }
}
module main {
    io VDD
    io GND
    RES c1(1)
    CAP c2(1)
    CAP c3(1)
    c1.Res([VDD, GND])
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
        "mcc-stage-p2-diff-{name}-{}-{:?}",
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

/// `show stage p2` over a source written into a scratch dir.
fn p2_on(name: &str, source: &str) -> Vec<Value> {
    let dir = scratch(name);
    let path = dir.join("circuit.mc");
    std::fs::write(&path, source).expect("write the source");
    let (stdout, stderr, ok) = run(
        &dir,
        &[
            "--local",
            "show",
            "stage",
            "p2",
            "-f",
            "json",
            "-F",
            path.to_str().expect("source path"),
        ],
    );
    assert!(ok, "show stage p2 failed: {stderr}");
    items_of(&stdout)
}

/// `show stage p2` over the five-file hbl project.
fn hbl_p2(name: &str) -> Vec<Value> {
    let dir = scratch(name);
    let entry = hbl_entry();
    let (stdout, stderr, ok) = run(
        &dir,
        &[
            "--local",
            "show",
            "stage",
            "p2",
            "-f",
            "json",
            "-F",
            entry.to_str().expect("fixture path"),
        ],
    );
    assert!(ok, "show stage p2 failed: {stderr}");
    items_of(&stdout)
}

fn count(d: &StageDiff, ty: &str, kind: &str) -> usize {
    d.changes
        .iter()
        .filter(|c| c["type"] == ty && c["kind"] == kind)
        .count()
}

// ── Synthetic items ──
//
// Minimum-shape items carrying only the fields the law reads, so a test says
// which branch it is about rather than which fields happened to be present.

fn ck(path: &str, ident: &str) -> Value {
    json!({ "path": path, "def": { "uri": "/src/m.mc", "ident": ident } })
}

/// An instance, a bus member, a label or a point: a row with a canonical path.
fn node(class: &str, path: &str, ident: &str, class_name: &str) -> Value {
    json!({
        "class": class, "key": null, "point": null, "path": path,
        "canon_key": ck(path, ident), "class_name": class_name,
        "net": null, "members": null, "loc": null,
    })
}

/// A net row. `name` is `None` for a net with no authored name, which is what
/// the emitter marks by a null `key`.
fn net(name: Option<&str>, members: &[&str]) -> Value {
    json!({
        "class": "net", "key": name.map(|n| format!("net:{n}")), "point": null,
        "path": null, "canon_key": null, "class_name": null,
        "net": name, "members": members, "loc": null,
    })
}

/// One reading with every class the law names, so a self-difference says
/// something about all five and not just the easy ones.
fn mixed() -> Vec<Value> {
    vec![
        node("instance", "main.u1", "AMP", "AMP"),
        node("bus", "main.u1.vin", "AMP", ""),
        node("label", "main.NET_A", "AMP", ""),
        node("point", "main.u1.1", "AMP", "1"),
        net(Some("NET_A"), &["main.u1.1", "main.r1.2"]),
        net(None, &["main.u1.2", "main.r1.1"]),
    ]
}

// ── A reading does not differ from itself ──

/// The strongest single statement: the same items compared twice produce
/// nothing, across every class the law names.
#[test]
fn a_reading_does_not_differ_from_itself() {
    let items = mixed();
    let d = P2_LAW.diff(&items, &items);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
}

/// The two blocks the drawing publishes are the law's, and this law does not
/// have them. `None` and not a zeroed report: "0 pins on nameless nets" and an
/// all-zero stability summary are statements this segment cannot make, and an
/// empty stability summary reads as "every box stayed put".
#[test]
fn the_segment_makes_no_claim_it_cannot_make() {
    let items = mixed();
    let d = P2_LAW.diff(&items, &items);

    assert!(
        d.nameless_net_pins.is_none(),
        "nets are items here, so there are no pins to count"
    );
    assert!(
        d.stability.is_none(),
        "this segment draws no boxes, so it has no box summary"
    );
}

// ── What is not an identity ──

/// A point's `net` is a bare name, and for an anonymous net that name is
/// numbered within its scope. Comparing it would read a renumbering -- which an
/// insertion causes -- as every point in that scope having moved.
#[test]
fn a_points_net_name_is_not_compared() {
    let a = vec![json!({
        "class": "point", "key": "N5:0", "point": "N5:0", "path": "main.u1.1",
        "canon_key": ck("main.u1.1", "AMP"), "class_name": "1", "net": "_net0",
        "members": null, "loc": null,
    })];
    // The same point, renumbered onto a later anonymous net -- and, in the
    // second half, onto a net that is named. Neither is a change to the point:
    // which net it is on is what the net rows' member lists carry.
    let anon = vec![json!({
        "class": "point", "key": "N9:7", "point": "N9:7", "path": "main.u1.1",
        "canon_key": ck("main.u1.1", "AMP"), "class_name": "1", "net": "_net3",
        "members": null, "loc": null,
    })];
    let named = vec![json!({
        "class": "point", "key": "N9:7", "point": "N9:7", "path": "main.u1.1",
        "canon_key": ck("main.u1.1", "AMP"), "class_name": "1", "net": "GND",
        "members": null, "loc": null,
    })];

    assert!(
        P2_LAW.diff(&a, &anon).changes.is_empty(),
        "a renumbering is not a change"
    );
    assert!(
        P2_LAW.diff(&a, &named).changes.is_empty(),
        "the field is excluded outright, not by the shape of the name"
    );
}

/// Build-local ordinals -- `key` is the `InstTable` row, `point` is the ordinal
/// of first interning, `loc` is a source position -- are never keys and are
/// never compared.
#[test]
fn build_local_ordinals_are_not_compared() {
    let a = vec![json!({
        "class": "instance", "key": "D39", "point": null, "path": "main.u1",
        "canon_key": ck("main.u1", "AMP"), "class_name": "AMP", "net": null,
        "members": null, "loc": { "uri": "/src/m.mc", "line": 3 },
    })];
    let b = vec![json!({
        "class": "instance", "key": "D41", "point": "N7:2", "path": "main.u1",
        "canon_key": ck("main.u1", "AMP"), "class_name": "AMP", "net": null,
        "members": null, "loc": { "uri": "/other/m.mc", "line": 91 },
    })];

    let d = P2_LAW.diff(&a, &b);
    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
}

/// The def is compared by **ident**, not by uri: two builds of one source from
/// different directories are the same design, not a replacement.
#[test]
fn a_build_from_another_directory_is_not_a_replacement() {
    let a = vec![node("instance", "main.u1", "AMP", "AMP")];
    let b = vec![json!({
        "class": "instance", "key": "D0", "point": null, "path": "main.u1",
        "canon_key": { "path": "main.u1",
                       "def": { "uri": "/elsewhere/deep/m.mc", "ident": "AMP" } },
        "class_name": "AMP", "net": null, "members": null, "loc": null,
    })];
    assert!(
        P2_LAW.diff(&a, &b).changes.is_empty(),
        "a different uri is the same design"
    );

    // And the mirror, so the field is not merely skipped: another ident is a
    // replacement, reported as a modify with the def in its delta.
    let c = vec![node("instance", "main.u1", "LDO", "LDO")];
    let d = P2_LAW.diff(&a, &c);
    assert_eq!(
        count(&d, "modify", "instance"),
        1,
        "changes: {:?}",
        d.changes
    );
    assert!(
        d.changes[0]["delta"]["def"]["from"] == "AMP"
            && d.changes[0]["delta"]["def"]["to"] == "LDO",
        "the delta names what changed: {:?}",
        d.changes[0]
    );
}

// ── A net's key is its members ──

/// The members are the identity: a net that gained or lost a point is not the
/// net it was, and it reads as a delete plus an add rather than as a modify.
#[test]
fn a_nets_members_are_its_key() {
    let a = vec![net(Some("NET_A"), &["main.u1.1", "main.u1.2"])];
    let b = vec![net(Some("NET_A"), &["main.u1.1", "main.u1.2", "main.r1.1"])];

    let d = P2_LAW.diff(&a, &b);
    assert_eq!(count(&d, "remove", "net"), 1, "{:?}", d.changes);
    assert_eq!(count(&d, "add", "net"), 1, "{:?}", d.changes);
    assert_eq!(count(&d, "modify", "net"), 0, "{:?}", d.changes);
}

/// A net's handle carries its members, so the delete and the add above do not
/// print alike. A handle that is not the identity cannot tell two rows apart,
/// and the two rows here are the ones the reader most needs to tell apart.
#[test]
fn a_nets_handle_carries_its_members() {
    let a = vec![net(Some("GND"), &["main.u1.1"])];
    let b = vec![net(Some("GND"), &["main.u1.1", "main.r1.1"])];

    let d = P2_LAW.diff(&a, &b);
    let ids: Vec<&str> = d
        .changes
        .iter()
        .map(|c| c["id"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(ids.len(), 2, "{:?}", d.changes);
    assert_ne!(ids[0], ids[1], "two rows about different nets: {ids:?}");
    assert!(
        ids.iter().all(|i| i.contains("main.")),
        "the handle names the members: {ids:?}"
    );
    assert!(
        !ids.iter().any(|i| i.contains('\u{2}')),
        "the key's separator is a control character and must not be printed: {ids:?}"
    );
}

/// Authoring a name, or not, is the net's own reading and is compared as
/// content. This is the branch that makes a rename a `modify`.
///
/// **Synthetic, and it has to be**: on both given boards a net's name reaches
/// its member list through the labelled point itself, so renaming one moves a
/// member and reads as a delete plus an add. The branch is what a name that no
/// member path carries would produce.
#[test]
fn a_named_nets_name_is_compared() {
    let a = vec![net(Some("VDD"), &["main.u1.1"])];
    let b = vec![net(Some("VCC"), &["main.u1.1"])];

    let d = P2_LAW.diff(&a, &b);
    assert_eq!(count(&d, "modify", "net"), 1, "{:?}", d.changes);
    assert_eq!(count(&d, "remove", "net"), 0, "{:?}", d.changes);
    assert_eq!(
        d.changes[0]["delta"]["net"]["from"], "VDD",
        "{:?}",
        d.changes[0]
    );
    assert_eq!(
        d.changes[0]["delta"]["net"]["to"], "VCC",
        "{:?}",
        d.changes[0]
    );
}

/// An anonymous net's name is numbered **within its scope**, so it is not
/// compared either -- the members already say which net this is.
#[test]
fn an_anonymous_nets_name_is_not_compared() {
    let members = ["main.u1.1", "main.r1.1"];
    let anon = |name: &str| {
        vec![json!({
            "class": "net", "key": null, "point": null, "path": null,
            "canon_key": null, "class_name": null, "net": name,
            "members": members, "loc": null,
        })]
    };

    // The same net, renumbered within its scope. A renumbering is not a change,
    // and the members already say which net this is.
    let d = P2_LAW.diff(&anon("_net0"), &anon("_net9"));
    assert!(
        d.changes.is_empty(),
        "a renumbering is not a change: {:?}",
        d.changes
    );
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
}

/// The emitter's `key` is `net:<name>`, which is a **name** and not a key: two
/// nets called `GND` in two scopes publish the same one. Keying on it would put
/// them both in `duplicate-key`; keying on the members keeps them apart.
#[test]
fn two_nets_of_one_name_do_not_collide() {
    let a = vec![
        net(Some("GND"), &["main.dc.GND", "main.dc.c1.2"]),
        net(Some("GND"), &["main.mcu.GND", "main.mcu.uc.21"]),
    ];
    let b = a.clone();

    let d = P2_LAW.diff(&a, &b);
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert_eq!(
        a.iter().filter(|i| i["key"] == "net:GND").count(),
        2,
        "the premise: one emitted key, two rows"
    );
}

// ── Unalignable things are reported, never guessed at ──

/// Two rows claiming one key is a real shape, and it is marked rather than
/// settled by arrival order.
#[test]
fn a_duplicated_key_is_reported_not_first_arrival() {
    let a = vec![
        net(None, &["main.u1.1"]),
        net(None, &["main.u1.1"]),
        net(None, &["main.u1.1"]),
    ];
    let d = P2_LAW.diff(&a, &[]);

    assert_eq!(d.unaligned.len(), 2, "unaligned: {:?}", d.unaligned);
    assert!(
        d.unaligned
            .iter()
            .all(|u| u["reason"] == "duplicate-key" && u["class"] == "net"),
        "{:?}",
        d.unaligned
    );
    assert_eq!(
        count(&d, "remove", "net"),
        1,
        "the first arrival is still a row: {:?}",
        d.changes
    );
}

/// A net with no points has nothing to be keyed on. The design names this blind
/// spot ("a net that is declared but carries no pins"); it is reported rather
/// than given a synthesised key.
///
/// **Synthetic, and it has to be**: on `hbl` every net row has at least two
/// members (measured: the smallest member list is 2).
#[test]
fn a_net_with_no_points_cannot_be_keyed() {
    let a = vec![net(None, &[]), net(Some("EMPTY"), &[])];
    let d = P2_LAW.diff(&a, &[]);

    assert_eq!(d.unaligned.len(), 2, "unaligned: {:?}", d.unaligned);
    assert!(
        d.unaligned
            .iter()
            .all(|u| u["reason"] == "no-key" && u["class"] == "net"),
        "{:?}",
        d.unaligned
    );
    assert!(
        d.changes.is_empty(),
        "nothing may be claimed about them: {:?}",
        d.changes
    );
}

// ── The real board ──

/// The real readout, compared with itself. Nothing is unalignable on `hbl` --
/// every node row has a canonical path and every net row has members -- so the
/// count is a reading and not a default.
#[test]
fn the_real_board_does_not_differ_from_itself() {
    let items = hbl_p2("hbl-self");
    let d = P2_LAW.diff(&items, &items);

    assert!(d.changes.is_empty(), "changes: {:?}", d.changes);
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
    assert!(
        items.len() > 400,
        "the premise: a whole board, {}",
        items.len()
    );
}

/// An insertion adds, and nothing else moves: not one `modify`, so no existing
/// part reads as changed by the renumbering an insertion causes.
#[test]
fn an_insertion_adds_without_renumbering_anything() {
    let a = p2_on("ins-base", BASE_SRC);
    let b = p2_on("ins-new", INSERTED_SRC);
    let d = P2_LAW.diff(&a, &b);

    assert_eq!(count(&d, "add", "instance"), 1, "{:?}", d.changes);
    assert_eq!(count(&d, "add", "point"), 2, "{:?}", d.changes);
    assert_eq!(
        count(&d, "modify", "instance") + count(&d, "modify", "point"),
        0,
        "an insertion must not read as a change to the parts already there: {:?}",
        d.changes
    );
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
    // The new part joins two nets, so those two nets are no longer the nets
    // they were -- and that is the only other thing this difference says.
    assert_eq!(count(&d, "remove", "net"), 2, "{:?}", d.changes);
    assert_eq!(count(&d, "add", "net"), 2, "{:?}", d.changes);

    // The mirror, so the direction cannot be read off the shapes.
    let back = P2_LAW.diff(&b, &a);
    assert_eq!(count(&back, "remove", "instance"), 1, "{:?}", back.changes);
    assert_eq!(count(&back, "add", "instance"), 0, "{:?}", back.changes);
}

/// Replacing a part keeps its path, so the replacement reads as content: one
/// `modify` on the instance, and the two points that carry the class def change
/// with it. Nothing is rebuilt.
#[test]
fn a_replaced_part_is_one_modify_and_not_a_rebuild() {
    let a = p2_on("swap-base", BASE_SRC);
    let b = p2_on("swap-res", REPLACED_SRC);
    let d = P2_LAW.diff(&a, &b);

    assert_eq!(count(&d, "modify", "instance"), 1, "{:?}", d.changes);
    let replaced = d
        .changes
        .iter()
        .find(|c| c["kind"] == "instance" && c["type"] == "modify")
        .expect("the replaced instance");
    assert_eq!(replaced["id"], "main.c1");
    assert_eq!(replaced["delta"]["class_name"]["from"], "CAP");
    assert_eq!(replaced["delta"]["class_name"]["to"], "RES");

    assert_eq!(count(&d, "add", "instance"), 0, "{:?}", d.changes);
    assert_eq!(count(&d, "remove", "instance"), 0, "{:?}", d.changes);
    assert_eq!(
        count(&d, "modify", "point"),
        2,
        "its two points carry the new class def: {:?}",
        d.changes
    );
    assert!(d.unaligned.is_empty(), "unaligned: {:?}", d.unaligned);
}

/// A difference is a function of its two inputs, so a second run over the same
/// items gives the same rows in the same order.
#[test]
fn the_rows_have_one_order() {
    let a = p2_on("order-base", BASE_SRC);
    let b = p2_on("order-new", INSERTED_SRC);
    let first = P2_LAW.diff(&a, &b);
    let second = P2_LAW.diff(&a, &b);
    assert_eq!(first.changes, second.changes);

    let kinds: Vec<&str> = first
        .changes
        .iter()
        .map(|c| c["kind"].as_str().unwrap_or(""))
        .collect();
    let mut sorted = kinds.clone();
    sorted.sort_unstable();
    assert_eq!(
        kinds, sorted,
        "rows are sorted by kind: {:?}",
        first.changes
    );
}
