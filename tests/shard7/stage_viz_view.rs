// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §7 phase two (batch 2c) acceptance: `stage.viz`.
//!
//! Phase two asks three things of every stage view, and this file asserts all
//! three for the laid-out schematic — the segment whose objects have positions:
//!
//! 1. **Two runs are byte-for-byte identical**, on both faces (§5.3 ruling ③,
//!    O15). This is the batch's real risk (§7.2): `stage.p2` and `stage.vec` read
//!    data the build already froze, while this one reads the graph the *layouter*
//!    produced, so a single unstable decision anywhere in layout, routing, label
//!    placement or wire hops would surface here and nowhere else.
//! 2. **The readout does not contradict the existing ones.** Four independent
//!    checks, each of which needs an *explanation* rather than a matching number:
//!    * the boxes it shows are the vector segment's boxes minus exactly the
//!      root layer's auto-named passives (the block-diagram rule that drops
//!      them), and nothing else;
//!    * every pin resolves to a Pass2 row, and the run-local key is present only
//!      where Pass2 says the row is a physical point;
//!    * every block edge's end resolves to a Pass2 row — including the endpoints
//!      that the block layer's own boxes do not carry, and with the run-local key
//!      only where the row is a point (§2.4: an endpoint is not necessarily a
//!      point);
//!    * the aggregate report rows are bounded by the scope rows beside them, so a
//!      partial sum cannot be read as a whole-drawing one.
//! 3. **The published geometry is the box's own.** Every anchor lies on the edge
//!    its `side` names, which is checked against the box's rectangle rather than
//!    against the rule that produced it.
//!
//! The fixture is the real `tests/fixtures/hbl` project (seven layers, 63 boxes,
//! 175 pins, 10 block edges), for the same reason the other two acceptances use
//! it: a toy fixture gives sorting nothing to reorder and layout nothing to get
//! wrong.
//!
//! ⚠ The harness runs every CLI invocation in a **fresh empty directory**.
//! `viz/project.rs` unconditionally writes `baseline/render_projection.md`
//! relative to the current directory; that file is gitignored and is not a
//! product of this batch.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A small named-instance circuit, for the rebuild half.
///
/// Named on purpose: an auto-named device renumbers when a sibling is inserted,
/// which would churn the `path` column and hide the property under test.
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

/// A fresh, **empty** directory to run a CLI invocation in — see the module note
/// on why it must not be the fixture tree.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-viz-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc show stage viz …` from `cwd` against the hbl fixture.
fn run_stage(cwd: &Path, extra: &[&str]) -> (String, String, bool) {
    run_seg(cwd, "viz", extra)
}

/// Run `mcc show stage <seg> …` from `cwd` against the hbl **project**.
///
/// The project root is handed over as the `-F` entry, not as a copied source
/// file: `hbl.mc` alone is one file of a five-file project, so copying it into a
/// scratch directory would read a different (much thinner) world.
fn run_seg(cwd: &Path, seg: &str, extra: &[&str]) -> (String, String, bool) {
    let mut args = vec!["--local", "show", "stage", seg];
    args.extend_from_slice(extra);
    args.push("-F");
    let entry = hbl_entry();
    args.push(entry.to_str().expect("fixture path"));
    run(cwd, &args)
}

fn seg_of_hbl(cwd: &Path, seg: &str) -> Value {
    let (stdout, stderr, ok) = run_seg(cwd, seg, &["-f", "json"]);
    assert!(ok, "show stage {seg} failed: {stderr}");
    stage_of(&stdout)
}

/// Run `mcc show stage viz …` against a standalone source written into `cwd`.
fn run_on(cwd: &Path, source: &str) -> Value {
    let path = cwd.join("circuit.mc");
    std::fs::write(&path, source).expect("write the source");
    let (stdout, stderr, ok) = run(
        cwd,
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
    stage_of(&stdout)
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

/// Rows of the text face, split on the documented column separator (two or more
/// spaces) and trimmed of the padding each column adds.
fn text_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            l.split("  ")
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect()
        })
        .collect()
}

fn items_of(stage: &Value) -> Vec<Value> {
    stage["items"]
        .as_array()
        .expect("items is an array")
        .clone()
}

fn of_class<'a>(items: &'a [Value], class: &str) -> Vec<&'a Value> {
    items.iter().filter(|i| i["class"] == class).collect()
}

fn canon_path(item: &Value) -> Option<&str> {
    item["canon_key"]["path"].as_str()
}

/// The whole envelope with the one field that cannot be stable blanked out —
/// `summary.elapsed_ms` is a wall clock (measured: 0 or 1 for the same input).
fn envelope_without_clock(stdout: &str) -> String {
    let mut envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}"));
    envelope["result"]["summary"]["elapsed_ms"] = Value::from(0);
    serde_json::to_string(&envelope).expect("the envelope re-serializes")
}

fn projection_bytes(stdout: &str) -> String {
    serde_json::to_string(&stage_of(stdout)).expect("the projection serializes")
}

fn yaml_without_clock(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with("elapsed_ms:"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether a `stage.vec` `net` item lists `path` among the pins it joins.
///
/// The member list is the handle of a net that owns no key (§2.4), and it is
/// also what joins the two views: both sides spell a pin through the same
/// `InstTable` row, so the paths are comparable even though the two views number
/// their objects independently.
fn lists_member(net: &Value, path: &str) -> bool {
    net["members"]
        .as_array()
        .is_some_and(|m| m.iter().any(|p| p.as_str() == Some(path)))
}

/// One `metrics` item's value, by its `<family>.<field>` path.
fn metric<'a>(items: &'a [Value], path: &str) -> &'a Value {
    let row = items
        .iter()
        .find(|i| i["class"] == "metrics" && i["path"] == path)
        .unwrap_or_else(|| panic!("no metrics row `{path}`"));
    &row["value"]
}

// ── Determinism ──

/// Two runs of one source are byte-for-byte identical, on every face.
///
/// This is the assertion the batch exists to make possible: the view's items come
/// out of the layouter, so a nondeterministic layout decision — the defect class
/// the viz domain has hit before (`radial.rs` R0, the driver vote in
/// `render/mod.rs`) — shows up here as a diff.
#[test]
fn two_runs_are_byte_identical_on_every_face() {
    let first = scratch("det-a");
    let second = scratch("det-b");

    for format in ["json", "json-pretty"] {
        let (a, ea, oka) = run_stage(&first, &["-f", format]);
        let (b, eb, okb) = run_stage(&second, &["-f", format]);
        assert!(oka && okb, "`show stage viz -f {format}` failed: {ea}{eb}");
        assert!(!a.trim().is_empty(), "`-f {format}` printed nothing");
        assert_eq!(
            projection_bytes(&a),
            projection_bytes(&b),
            "`-f {format}`: the stage view itself must be byte-identical"
        );
        assert_eq!(
            envelope_without_clock(&a),
            envelope_without_clock(&b),
            "`-f {format}`: `elapsed_ms` must be the *only* field that differs"
        );
    }

    // The text face is compared in full: it carries no measurement of its own.
    let (at, eta, oka) = run_stage(&first, &[]);
    let (bt, etb, okb) = run_stage(&second, &[]);
    assert!(oka && okb, "the text face failed: {eta}{etb}");
    assert_eq!(at, bt, "the text face must be byte-identical across runs");

    let (ya, eya, okya) = run_stage(&first, &["-f", "yaml"]);
    let (yb, eyb, okyb) = run_stage(&second, &["-f", "yaml"]);
    assert!(okya && okyb, "the yaml face failed: {eya}{eyb}");
    assert_eq!(
        yaml_without_clock(&ya),
        yaml_without_clock(&yb),
        "`-f yaml`: `elapsed_ms` must be the *only* line that differs"
    );
    assert!(ya.contains("view: stage.viz"), "{ya:.200}");

    let _ = std::fs::remove_dir_all(&first);
    let _ = std::fs::remove_dir_all(&second);
}

/// A tiny circuit is laid out twice — the fixture is not the only thing whose
/// layout must be stable, and a small graph exercises different layout branches
/// than the seven-layer one. It is also the only case here where the source is a
/// file this test wrote, so it is the only one that would catch layout depending
/// on something outside the parse.
///
/// ⚠ Both runs happen in the **same** directory. A source's path is part of the
/// world's fingerprint and of every `loc`, so two directories would differ in
/// `world_ver` and in every URI — measured, and nothing to do with layout. The
/// property under test is that the same world lays out the same way.
#[test]
fn a_small_circuit_is_also_laid_out_deterministically() {
    let dir = scratch("small");

    let a = run_on(&dir, BASE_SRC);
    let b = run_on(&dir, BASE_SRC);
    assert_eq!(
        serde_json::to_string(&a).expect("serializes"),
        serde_json::to_string(&b).expect("serializes"),
        "a small circuit's layout must be reproducible too"
    );

    let items = items_of(&a);
    let boxes = of_class(&items, "box");
    assert!(boxes.len() >= 3, "the small circuit must produce boxes");
    // A device-layer root: no block edges, but real anchors. The fixture's root
    // is a block diagram, so this is the other branch of the layout decision.
    assert_eq!(
        of_class(&items, "layer")[0]["style"].as_str(),
        Some("device"),
        "a root with no sub-modules is drawn as a device schematic"
    );
    assert!(
        of_class(&items, "pin")
            .iter()
            .any(|p| p["side"].as_str().is_some()),
        "the small circuit must actually anchor some pins"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Not contradicting the existing readouts ──

/// The aggregate reports are **partial**, and the scope rows say so.
///
/// The pipeline audits a layer only when it takes the route/audit path, which
/// device layers do not (F2: they wire themselves through the equipotential
/// trees). So `truth.boxes_total` is not the drawing's box count, and publishing
/// it next to 63 box rows without a scope would be exactly the contradiction the
/// phase forbids. Three ties make the bound checkable rather than asserted:
/// `truth.layers_total` (the accumulator's own count of audited layers) equals
/// `scope.audited_layers`, and both box aggregates equal the sum over the layers
/// the view itself flags `audited`.
#[test]
fn the_scope_rows_bound_the_aggregate_reports() {
    let dir = scratch("scope");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage viz failed: {err}");
    let items = items_of(&stage_of(&stdout));

    let layers = of_class(&items, "layer");
    assert!(layers.len() >= 2, "the fixture must have several layers");
    for l in &layers {
        assert!(
            l["audited"].is_boolean(),
            "every layer must say whether the reports cover it: {l}"
        );
    }
    let audited: Vec<&&Value> = layers.iter().filter(|l| l["audited"] == true).collect();
    assert!(
        !audited.is_empty(),
        "no audited layer — the aggregates would have nothing to be about"
    );
    assert!(
        audited.len() < layers.len(),
        "every layer is audited; this fixture is supposed to have device layers \
         that are not, and with none the scope rows would be asserting a bound \
         that never binds"
    );

    assert_eq!(
        metric(&items, "scope.layers").as_u64(),
        Some(layers.len() as u64),
        "`scope.layers` must be the number of layer rows"
    );
    assert_eq!(
        metric(&items, "scope.audited_layers").as_u64(),
        Some(audited.len() as u64),
        "`scope.audited_layers` must be the number of rows flagged `audited`"
    );
    assert_eq!(
        metric(&items, "truth.layers_total").as_u64(),
        Some(audited.len() as u64),
        "the truth report's own layer count must equal the audited count — the \
         two are computed independently (the accumulator's gate and the view's \
         flag), so agreement is the check that the scope row means what it says"
    );

    // The aggregates are about the audited layers only. Summing the view's own
    // per-layer numbers gives the same figure.
    let sum: u64 = audited
        .iter()
        .map(|l| l["boxes"].as_u64().unwrap_or(0))
        .sum();
    for path in ["truth.boxes_total", "visual.boxes_total"] {
        assert_eq!(
            metric(&items, path).as_u64(),
            Some(sum),
            "`{path}` must be the sum over the layers the reports cover"
        );
    }

    // And the bound really does bind here: the partial sum is smaller than the
    // drawing. Without this the assertions above would hold on a full audit too,
    // and the scope rows would be decoration.
    assert!(
        sum < of_class(&items, "box").len() as u64,
        "the audited box total equals the whole drawing's box count on this \
         fixture, so the scope rows prove nothing about partiality"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The boxes this view shows are the vector segment's boxes minus the root
/// layer's auto-named passives — and every other box survives.
///
/// The two segments read different things on purpose (`stage.vec` reads the
/// built graph, this view the laid-out one), so they legitimately differ; what
/// must not happen is a box vanishing with nothing to explain it. The explanation
/// is the block-diagram rule that drops root-level passives before drawing, and
/// it is stated structurally here: a missing box is one the vec view places in
/// the **root layer** whose own name is compiler-generated (`_C1`, `_R2`, …).
#[test]
fn the_boxes_dropped_before_drawing_are_the_root_autonamed_passives() {
    let dir = scratch("boxes");
    let vec = seg_of_hbl(&dir, "vec");
    let viz = seg_of_hbl(&dir, "viz");

    let vec_items = items_of(&vec);
    let vec_boxes = of_class(&vec_items, "box");
    let viz_items = items_of(&viz);
    let viz_paths: BTreeSet<String> = of_class(&viz_items, "box")
        .iter()
        .filter_map(|b| canon_path(b).map(str::to_string))
        .collect();
    assert!(!viz_paths.is_empty(), "the view must show boxes");

    // The root layer's path, from the layer that has no parent.
    let root = of_class(&viz_items, "layer")
        .into_iter()
        .find(|l| l["parent"].is_null())
        .map(|l| l["path"].as_str().expect("a layer path").to_string())
        .expect("the root layer");

    let mut missing: Vec<&Value> = Vec::new();
    for b in &vec_boxes {
        let path = canon_path(b).unwrap_or_else(|| panic!("a vec box names an instance: {b}"));
        if !viz_paths.contains(path) {
            missing.push(b);
        }
    }
    assert!(
        !missing.is_empty(),
        "hbl is expected to drop root-level passives before drawing; with none \
         dropped this assertion proves nothing"
    );

    for b in &missing {
        let path = canon_path(b).expect("a path");
        assert_eq!(
            b["layer"].as_str(),
            Some(root.as_str()),
            "`{path}` is missing from the drawing but is not in the root layer, so \
             the passive-drop rule does not explain it"
        );
        let leaf = path.rsplit('.').next().unwrap_or(path);
        assert!(
            leaf.starts_with('_'),
            "`{path}` is missing from the drawing and has an author-written name, \
             so the passive-drop rule does not explain it"
        );
    }

    // The converse: nothing else is missing. Every root-layer box with a
    // source-written name is drawn.
    let survivors: Vec<&&Value> = vec_boxes
        .iter()
        .filter(|b| {
            b["layer"].as_str() == Some(root.as_str())
                && !canon_path(b)
                    .and_then(|p| p.rsplit('.').next())
                    .unwrap_or("")
                    .starts_with('_')
        })
        .collect();
    assert!(survivors.len() >= 2, "the root layer must have real boxes");
    for b in survivors {
        let path = canon_path(b).expect("a path");
        assert!(
            viz_paths.contains(path),
            "`{path}` is an author-written box of the root layer and the drawing \
             does not show it"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every pin resolves to a Pass2 row, and the run-local key is present **only**
/// where Pass2 says the row is a physical point.
///
/// The implication runs one way on purpose, and the fixture shows why: 15 of
/// hbl's 175 drawn pins name a `point`-class row and still carry no `PointId` —
/// the module ports, whose crossing is a boundary rather than a connection
/// (§2.4: not every object is a point). Asserting equality instead would fail on
/// the design.
#[test]
fn every_pin_resolves_to_a_pass2_row() {
    let dir = scratch("pins");
    let p2 = seg_of_hbl(&dir, "p2");
    let viz = seg_of_hbl(&dir, "viz");

    let known: BTreeMap<String, BTreeSet<String>> = {
        let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for i in items_of(&p2) {
            if let Some(p) = canon_path(&i) {
                m.entry(p.to_string())
                    .or_default()
                    .insert(i["class"].as_str().unwrap_or("").to_string());
            }
        }
        m
    };

    let viz_items = items_of(&viz);
    let pins = of_class(&viz_items, "pin");
    assert!(pins.len() >= 20, "hbl must give a substantial pin set");

    let mut keyed = 0usize;
    let mut keyless = 0usize;
    for p in &pins {
        let path = canon_path(p).unwrap_or_else(|| {
            panic!("a drawn pin with an InstTable row must carry a canonical key: {p}")
        });
        let classes = known
            .get(path)
            .unwrap_or_else(|| panic!("pin {path} names no Pass2 row — the two segments disagree"));

        if let Some(k) = p["key"].as_str() {
            keyed += 1;
            assert_eq!(p["point"].as_str(), Some(k), "`point` repeats `key`: {p}");
            assert!(
                classes.contains("point"),
                "a pin carries a `PointId` while Pass2 classes {path} as \
                 {classes:?} — the key would then name something that is not a point"
            );
        } else {
            keyless += 1;
            assert!(p["point"].is_null(), "{p}");
        }
    }

    assert!(
        keyed > 0,
        "no pin carries a `PointId` — the keyed half is unexercised"
    );
    assert!(
        keyless > 0,
        "every pin carries a `PointId` — the `None` family (§2.4) is unexercised, \
         so the implication above is asserted over a branch with no members"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every drawn pin names the net it is on, and names it the way `stage.vec` does.
///
/// The SVM skeleton spells a net on every pin (§4:
/// `"pins": [ { "id": "pin:…", "net": "net:V3V3" } ]`), which is what turns
/// "highlight this net" into a lookup rather than a search. Two distinct ways to
/// get it wrong, and this test tells them apart:
///
/// * **The two views could key one net two ways.** `stage.vec` keys a `net` item;
///   this view records a key on a pin. Both go through the one `net_key` rule,
///   and the assertion here is that the key *and* the run-local id agree with the
///   vec item that lists this pin among its members.
/// * **A net with no key is not a pin with no net.** A net the builder minted
///   (`~`-split, `_net<k>`) can never be a key — its name belongs to this build's
///   segmentation — so such a pin carries the net's run-local id instead, which
///   is an index published *beside* the key and never as one (§3.5). A pin on no
///   net at all carries neither.
///
/// The third family is the one that needs an explanation rather than a count: the
/// render **promotes** the graph before laying it out (`apply_promote_recursive`,
/// on by default), and promotion replaces `graph.nets` with the nets that reach at
/// least one box of the layer, discarding the rest. So a pin can name nothing here
/// while `stage.vec` — which reads the graph *before* promotion — lists it on a
/// net. The assertion is therefore that this can only happen in a layer where this
/// view holds *fewer* nets than the vector view does, and that such a layer exists
/// (otherwise the clause is vacuous).
#[test]
fn every_pin_names_the_net_it_is_on() {
    let dir = scratch("pin-net");
    let vec = seg_of_hbl(&dir, "vec");
    let viz = seg_of_hbl(&dir, "viz");

    let vec_items = items_of(&vec);
    let viz_items = items_of(&viz);

    // The vec view's nets, by the layer they belong to. A net is built per layer
    // and two layers number theirs independently, so every lookup below is scoped
    // to the pin's own layer.
    let mut vec_nets: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for n in of_class(&vec_items, "net") {
        vec_nets
            .entry(n["layer"].as_str().unwrap_or(""))
            .or_default()
            .push(n);
    }
    let vec_net_count = |layer: &str| vec_nets.get(layer).map(Vec::len).unwrap_or(0);
    let viz_net_count = |layer: &str| -> usize {
        of_class(&viz_items, "layer")
            .into_iter()
            .find(|l| l["path"].as_str() == Some(layer))
            .map(|l| l["nets"].as_u64().unwrap_or(0) as usize)
            .unwrap_or(0)
    };

    let mut keyed = 0usize;
    let mut keyless = 0usize;
    let mut no_net_vec_agrees = 0usize;
    let mut no_net_promoted = 0usize;

    for p in of_class(&viz_items, "pin") {
        let path = p["path"]
            .as_str()
            .unwrap_or_else(|| panic!("a drawn pin must carry a canonical path: {p}"));
        let layer = p["layer"].as_str().unwrap_or("");
        let candidates: Vec<&Value> = vec_nets
            .get(layer)
            .map(|ns| {
                ns.iter()
                    .copied()
                    .filter(|n| lists_member(n, path))
                    .collect()
            })
            .unwrap_or_default();

        match (p["net"].as_str(), p["nid"].as_i64()) {
            (Some(key), Some(nid)) => {
                keyed += 1;
                let mut hit: Option<&Value> = None;
                for n in candidates.iter().copied() {
                    if n["key"].as_str() == Some(key) {
                        hit = Some(n);
                        break;
                    }
                }
                let hit = hit.unwrap_or_else(|| {
                    panic!(
                        "pin {path} names `{key}`, and no net in {layer} is keyed that \
                         way — the two views spell one net differently: {p}"
                    )
                });
                assert_eq!(
                    hit["nid"].as_i64(),
                    Some(nid),
                    "pin {path} names `{key}` with net id {nid}, while {layer} gives \
                     that key id {} — one of the two is not the net it points at",
                    hit["nid"]
                );
            }
            (None, Some(nid)) => {
                keyless += 1;
                let mut hit: Option<&Value> = None;
                for n in candidates.iter().copied() {
                    if n["nid"].as_i64() == Some(nid) {
                        hit = Some(n);
                        break;
                    }
                }
                let hit = hit.unwrap_or_else(|| {
                    panic!(
                        "pin {path} bears net id {nid}, and no net in {layer} owns it — \
                         an id naming nothing is worse than no id"
                    )
                });
                assert!(
                    hit["key"].is_null(),
                    "pin {path} omits a key while {layer} holds one for the same net \
                     (id {nid}) — a key dropped on the way out is not a net without one"
                );
            }
            (None, None) => {
                if candidates.is_empty() {
                    // Unconnected in both views: a real reading on this fixture
                    // (Pass2 rows of this kind are the ones the ERC reports as
                    // unconnected).
                    no_net_vec_agrees += 1;
                } else {
                    no_net_promoted += 1;
                    assert!(
                        viz_net_count(layer) < vec_net_count(layer),
                        "pin {path} names no net while {layer} lists it on one, and \
                         this view holds as many nets as the vector view does ({} vs \
                         {}) — promote dropped nothing there, so the missing reference \
                         has no explanation",
                        viz_net_count(layer),
                        vec_net_count(layer)
                    );
                }
            }
            (Some(key), None) => panic!("pin {path} names `{key}` with no net id: {p}"),
        }
    }

    assert!(keyed >= 2, "the keyed family is unexercised");
    assert!(
        keyless >= 2,
        "no pin sits on a keyless net — the `net_key` `None` branch is asserted over \
         an empty population"
    );
    assert!(
        no_net_vec_agrees >= 2,
        "no pin is unconnected in both views, so the family that must NOT be \
         explained by promote is unexercised"
    );
    assert!(
        no_net_promoted >= 1,
        "promote is expected to strip nets from the root layer on this fixture; if it \
         stops, the clause above passes vacuously"
    );

    // The text face carries the same reference — a key printed on one face only
    // would leave a reader to work around it.
    let (text, err, ok) = run_stage(&dir, &[]);
    assert!(ok, "show stage viz failed: {err}");
    let printed = text_rows(&text)
        .into_iter()
        .filter(|r| r[2].contains("net="))
        .count();
    assert_eq!(
        printed,
        of_class(&viz_items, "pin").len(),
        "`net=` is the pin detail's own cell, so it must appear on exactly the pin rows"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every layer names the report families whose rows cover it — the entry point
/// M2's second gap is about.
///
/// The reports are aggregate, so a reference from the drawing to them can only be
/// per layer, and it is not one answer: `connectivity` merges over every layer,
/// the four the pipeline audits a layer for sit on the layers it audited, and
/// `determinism` **assigns** rather than merges, so it describes exactly one layer
/// — one the published report does not name. That case is why this is a
/// reference and not a flag: the view reads the layer back off the report's own
/// box-geometry hash instead of guessing at the order the pipeline ran in.
///
/// Both `audited` branches are filled on this fixture (one layer audited, six
/// device layers that are not), and `determinism` has exactly one member because
/// the accumulator's assign semantics make it a singular — the count is asserted
/// from the law, not from the fixture.
#[test]
fn every_layer_names_the_reports_that_cover_it() {
    let dir = scratch("report-ref");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage viz failed: {err}");
    let items = items_of(&stage_of(&stdout));

    // The families that have rows to land on. `scope` is not a report — it is the
    // answer to how much of the drawing the others cover.
    let published: BTreeSet<String> = of_class(&items, "metrics")
        .iter()
        .filter_map(|m| m["family"].as_str())
        .filter(|f| *f != "scope")
        .map(str::to_string)
        .collect();
    assert!(
        published.len() >= 2,
        "too few report families to tell a reference from a coincidence: {published:?}"
    );

    let layers = of_class(&items, "layer");
    assert!(layers.len() >= 2, "the fixture must have several layers");

    let four = ["fidelity", "truth", "visual", "readability"];
    let (mut audited, mut unaudited) = (0usize, 0usize);
    let (mut with_det, mut with_conn) = (0usize, 0usize);
    for l in &layers {
        let reports: Vec<&str> = l["reports"]
            .as_array()
            .unwrap_or_else(|| panic!("every layer carries its report list: {l}"))
            .iter()
            .map(|r| r.as_str().expect("a family name is a string"))
            .collect();
        assert!(
            !reports.is_empty(),
            "no layer on this fixture is covered by nothing, and the text face \
             would print `-` where a reference belongs: {l}"
        );
        for f in &reports {
            assert!(
                published.contains(*f),
                "layer `{}` names report family `{f}` with no row to land on — \
                 that is a promise, not a reference",
                l["path"]
            );
        }
        let named = four.iter().filter(|f| reports.contains(*f)).count();
        if l["audited"] == true {
            audited += 1;
            assert_eq!(named, four.len(), "an audited layer names all four: {l}");
        } else {
            unaudited += 1;
            assert_eq!(named, 0, "an unaudited layer names none of the four: {l}");
        }
        if reports.contains(&"connectivity") {
            with_conn += 1;
        }
        if reports.contains(&"determinism") {
            with_det += 1;
        }
    }
    assert!(
        audited >= 1 && unaudited >= 1,
        "one branch is empty ({audited} audited, {unaudited} not), so the four \
         families would be asserted over a single case"
    );
    assert_eq!(
        with_conn,
        layers.len(),
        "connectivity merges over every layer, so every layer is covered by it"
    );
    assert!(
        metric(&items, "determinism.graph_input_hash")
            .as_str()
            .is_some_and(|h| !h.is_empty()),
        "the layer is read back off `determinism.graph_input_hash`; with that \
         field empty the mechanism cannot name anyone"
    );
    assert_eq!(
        with_det, 1,
        "the determinism report is assigned, never merged, so exactly one layer \
         is covered by it — {with_det} claim to be"
    );

    // The two faces are one source, so the printed list must be the same list.
    let (text, etext, oktext) = run_stage(&dir, &[]);
    assert!(oktext, "the text face failed: {etext}");
    let rows = text_rows(&text);
    for l in &layers {
        let path = l["path"].as_str().expect("a layer has a path");
        let want: Vec<&str> = l["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        let row = rows
            .iter()
            .find(|r| r.iter().any(|c| c.contains("reports=")) && r.iter().any(|c| c == path))
            .unwrap_or_else(|| panic!("no text row for layer `{path}`: {rows:?}"));
        let cell = row
            .iter()
            .find(|c| c.contains("reports="))
            .expect("the row that carries one has a cell with it");
        let got = cell
            .split("reports=")
            .nth(1)
            .expect("split finds it")
            .trim();
        assert_eq!(
            got,
            want.join(","),
            "the text face and the json face must name the same families for `{path}`"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every block edge's end resolves to a Pass2 row, and on this fixture every end
/// also names a pin its own box carries.
///
/// The second half is a requirement, not a coincidence (U106): the boxes are one
/// face of the circuit and a segment's ends are another, and the two are required
/// to agree. A block edge is decided at the net layer, against the whole world's
/// endpoints, while the block layer's box for a sub-module carries only the pins
/// the *diagram* shows — so the agreement is worth asserting rather than assuming.
/// This fixture now holds it; it is not a property the tree keeps everywhere, and
/// `hs` is where it does not (13 of its 142 ends name no pin of their box).
/// Measured here: 2 of 38 ends disagreed before the port-group segment was restored
/// to the path (the MIC edge's `from` ends reached the drawing spelled
/// `main.MIC.N`/`.P`, while the module box carried neither spelling and the ports
/// it does carry are `main.MIC.MIC.N`/`.P`), and 0 of 38 do now.
///
/// The resolution itself is asserted as "a row of some class", because an end that
/// **is** a pin need not be a point: the two MIC ends, like 23 of the 38 ends
/// here, are `label` rows, and the run-local key appears only where the row is a
/// point. An endpoint-scoped-to-the-layer lookup would resolve neither a `label`
/// row nor a row the box does not carry, and would report nothing wrong.
#[test]
fn every_block_edge_end_resolves_to_a_pass2_row() {
    let dir = scratch("edges");
    let p2 = seg_of_hbl(&dir, "p2");
    let viz = seg_of_hbl(&dir, "viz");

    let known: BTreeMap<String, BTreeSet<String>> = {
        let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for i in items_of(&p2) {
            if let Some(p) = canon_path(&i) {
                m.entry(p.to_string())
                    .or_default()
                    .insert(i["class"].as_str().unwrap_or("").to_string());
            }
        }
        m
    };
    assert!(known.len() > 50, "the Pass2 table must be substantial");

    let viz_items = items_of(&viz);
    let segs = of_class(&viz_items, "segment");
    let edges: Vec<&&Value> = segs.iter().filter(|s| s["kind"] == "edge").collect();
    assert!(
        edges.len() >= 2,
        "hbl's root layer must carry block edges, got {}",
        edges.len()
    );

    // The block layer's own pins, against which the ends are required to agree.
    let layer_pins: BTreeSet<String> = of_class(&viz_items, "pin")
        .iter()
        .filter_map(|p| canon_path(p).map(str::to_string))
        .collect();

    let mut ends = 0usize;
    let mut outside_the_layer = 0usize;
    let mut pointing = 0usize;
    // The `label` flavour of §2.4: ends that are not `point` rows.
    let mut nonpoints = 0usize;
    for e in &edges {
        for side in ["from", "to"] {
            let list = e[side].as_array().expect("an endpoint list");
            assert!(
                !list.is_empty(),
                "an edge with no endpoint at one end would be drawn from nowhere: {e}"
            );
            let mut prev: Option<&str> = None;
            for r in list {
                ends += 1;
                let path = r["path"]
                    .as_str()
                    .unwrap_or_else(|| panic!("an edge end with no canonical path: {r}"));
                let classes = known
                    .get(path)
                    .unwrap_or_else(|| panic!("edge end `{path}` names no Pass2 row"));
                if let Some(pt) = r["point"].as_str() {
                    pointing += 1;
                    assert!(
                        classes.contains("point"),
                        "edge end `{path}` carries the `PointId` {pt} while Pass2 \
                         classes it as {classes:?}"
                    );
                }
                if !layer_pins.contains(path) {
                    outside_the_layer += 1;
                }
                if r["point"].is_null() {
                    nonpoints += 1;
                }
                // The handle is the *canonical* endpoint pair, so the list is in
                // canonical order and not in this build's id order.
                if let Some(p) = prev {
                    assert!(p <= path, "edge ends must be in canonical order: {e}");
                }
                prev = Some(path);
            }
        }
    }

    assert!(ends >= 4, "too few edge ends to exercise the resolution");
    assert_eq!(
        outside_the_layer, 0,
        "every edge end must name a pin of the layer's own boxes (U106); the two \
         that did not were the MIC edge's `main.MIC.N`/`.P`, and this fixture is \
         the one that reads them"
    );
    assert!(
        pointing > 0,
        "no edge end carries a `PointId` — the run-local half is unexercised"
    );
    // Both flavours are exercised: §2.4's point ends and its non-point ones. The
    // non-point family is counted on its own — an end that is a pin of its box is
    // a different population from one the box cannot carry.
    assert!(
        nonpoints > 0,
        "every edge end turned out to be a `point` row; hbl is expected to have \
         ends that are `label` rows, so the `None` family would be unexercised"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A segment owns no key (§2.4): it is not an object but a path between two
/// endpoints, so its handle is the endpoint pair and its key cells are `null`.
#[test]
fn a_segment_carries_no_key() {
    let dir = scratch("segkey");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage viz failed: {err}");
    let items = items_of(&stage_of(&stdout));

    let segs = of_class(&items, "segment");
    assert!(segs.len() >= 2, "the fixture must draw segments");
    for s in &segs {
        assert!(s["key"].is_null(), "a segment owns no key: {s}");
        assert!(s["canon_key"].is_null(), "{s}");
        assert!(s["point"].is_null(), "{s}");
        assert!(
            matches!(s["kind"].as_str(), Some("edge") | Some("wire")),
            "a segment's kind says where it came from: {s}"
        );
        // One row shape for both kinds, so a reader never has to branch on
        // whether a field is absent or null.
        for field in ["from", "to", "from_at", "to_at", "length", "net", "index"] {
            assert!(
                s.get(field).is_some(),
                "every segment row carries `{field}`, whatever its kind: {s}"
            );
        }
    }

    // Both kinds are branch-checked: an edge has ends and no coordinates, a wire
    // the other way round. The fixture draws edges; a wire would be the routed
    // kind (§2.4's measured gap: the router is unreachable in the shipped
    // pipeline, so `wire` is expected to have no population here — the loop above
    // still checks its row shape).
    for s in segs.iter().filter(|s| s["kind"] == "edge") {
        assert!(s["from"].is_array() && s["to"].is_array(), "{s}");
        assert!(
            s["from_at"].is_null() && s["to_at"].is_null(),
            "an edge's coordinates are computed inside the renderer and are not \
             on the graph, so publishing some here would be inventing them: {s}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ── The geometry is the box's own ──

/// Every published anchor lies on the edge its `side` names.
///
/// This is checked against the box's **rectangle**, not against the rule that
/// produced the coordinates, so it is a real check of the view's arithmetic: a
/// `Right` anchor placed at the box's left edge (or a `Left` one at its right)
/// is on the rectangle too, and is caught here rather than by inspection.
#[test]
fn every_anchor_lies_on_the_edge_its_side_names() {
    let dir = scratch("anchors");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage viz failed: {err}");
    let items = items_of(&stage_of(&stdout));

    // A box is looked up by (layer, path): the same instance cannot be boxed
    // twice in one layer, but two layers may box different things with the same
    // short name.
    let mut boxes: BTreeMap<(String, String), &Value> = BTreeMap::new();
    for b in of_class(&items, "box") {
        let key = (
            b["layer"].as_str().unwrap_or("").to_string(),
            b["path"].as_str().unwrap_or("").to_string(),
        );
        boxes.insert(key, b);
    }

    let eps = 1e-9;
    let mut checked = 0usize;
    for p in of_class(&items, "pin") {
        let (Some(side), Some(at)) = (p["side"].as_str(), p["at"].as_array()) else {
            assert!(
                p["at"].is_null(),
                "a pin with no `side` cannot have a position: {p}"
            );
            continue;
        };
        let key = (
            p["layer"].as_str().unwrap_or("").to_string(),
            p["box"].as_str().unwrap_or("").to_string(),
        );
        let b = boxes
            .get(&key)
            .unwrap_or_else(|| panic!("pin names a box the view does not show: {p}"));
        let (bx, by) = (
            b["at"][0].as_f64().expect("box x"),
            b["at"][1].as_f64().expect("box y"),
        );
        let (bw, bh) = (
            b["size"][0].as_f64().expect("box w"),
            b["size"][1].as_f64().expect("box h"),
        );
        let (x, y) = (at[0].as_f64().expect("x"), at[1].as_f64().expect("y"));

        let on_edge = match side {
            "top" => (y - by).abs() < eps && x >= bx - eps && x <= bx + bw + eps,
            "bottom" => (y - (by + bh)).abs() < eps && x >= bx - eps && x <= bx + bw + eps,
            "left" => (x - bx).abs() < eps && y >= by - eps && y <= by + bh + eps,
            "right" => (x - (bx + bw)).abs() < eps && y >= by - eps && y <= by + bh + eps,
            other => panic!("unknown entry side `{other}`: {p}"),
        };
        assert!(
            on_edge,
            "a `{side}` anchor at ({x},{y}) is not on that edge of its box \
             (x={bx} y={by} w={bw} h={bh}): {p}"
        );
        checked += 1;
    }

    assert!(
        checked >= 10,
        "too few anchored pins to exercise all four sides, got {checked}"
    );

    // All four sides must actually occur, or the check above covers fewer
    // branches than it claims.
    let sides: BTreeSet<&str> = of_class(&items, "pin")
        .iter()
        .filter_map(|p| p["side"].as_str())
        .collect();
    for want in ["top", "bottom", "left", "right"] {
        assert!(
            sides.contains(want),
            "no `{want}` anchor in the fixture — that arm of the rule would go \
             unchecked (sides present: {sides:?})"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ── One `items`, two faces ──

#[test]
fn the_text_face_presents_the_same_items_as_the_json_face() {
    let dir = scratch("faces");
    let (text, et, okt) = run_stage(&dir, &[]);
    let (json, ej, okj) = run_stage(&dir, &["-f", "json"]);
    assert!(okt && okj, "show stage viz failed: {et}{ej}");

    let items = items_of(&stage_of(&json));
    let rows = text_rows(&text);

    assert!(items.len() > 100, "hbl must give a substantial view");
    assert_eq!(
        rows.len(),
        items.len(),
        "the text face must print exactly one row per item"
    );

    for (item, row) in items.iter().zip(rows.iter()) {
        // Arity 4 also asserts no cell holds a run of two spaces, which is the
        // column separator.
        assert_eq!(row.len(), 4, "each row is key/target/detail/loc: {row:?}");
        assert_eq!(row[0], item["key"].as_str().unwrap_or("-"), "{row:?}");
        let want_loc = match item["loc"]["uri"].as_str() {
            Some(uri) => format!("{uri}:{}", item["loc"]["line"].as_u64().unwrap_or(0)),
            None => "-".to_string(),
        };
        assert_eq!(row[3], want_loc, "last column is `loc`, item {item}");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// §5.3's four prohibitions, asserted rather than trusted.
#[test]
fn the_text_face_obeys_the_four_prohibitions() {
    let dir = scratch("prose");
    let (text, err, ok) = run_stage(&dir, &[]);
    assert!(ok, "show stage viz failed: {err}");

    assert!(!text.contains('\t'), "no tab-delimited columns");
    assert!(!text.contains('\u{1b}'), "no ANSI escapes");
    for line in text.lines() {
        assert!(
            !line.contains('\u{2500}') && !line.contains('\u{2502}'),
            "no box drawing: {line:?}"
        );
    }
    let header = text.lines().next().expect("a header line");
    assert!(header.starts_with("# stage.viz"), "{header}");
    assert!(header.contains("world_ver="), "{header}");

    // A missing value prints `-`, never an empty column: segments and metrics
    // rows own no key at all, so their first column is exactly the glyph.
    let missing = text_rows(&text).into_iter().filter(|r| r[0] == "-").count();
    assert!(
        missing >= 2,
        "the keyless classes (segment, metrics) must print the missing-value \
         glyph in the key column, got {missing}"
    );

    // Law C: a readout is not a verdict. hbl's flatten reports diagnostics, and
    // the readout still exits 0.
    let (stdout, _, _) = run_stage(&dir, &["-f", "json"]);
    assert!(
        stage_of(&stdout)["counts"]["diagnostics"]
            .as_u64()
            .unwrap_or(0)
            > 0,
        "hbl is expected to report diagnostics; if it stops, this assertion no \
         longer proves the readout tolerates them"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every item class this view can emit must have a real population in the
/// fixture — a one-member class is a branch the assertions above skip while
/// still reporting green.
#[test]
fn every_class_the_readout_reaches_is_exercised() {
    let dir = scratch("classes");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage viz failed: {err}");
    let stage = stage_of(&stdout);
    let items = items_of(&stage);

    for class in ["layer", "box", "pin", "segment", "metrics"] {
        let got = of_class(&items, class);
        assert!(
            got.len() >= 2,
            "class `{class}` has {} member(s) in hbl — too thin to be exercised",
            got.len()
        );
    }

    // The header counts and the rows are one number.
    for (word, class) in [
        ("layers", "layer"),
        ("boxes", "box"),
        ("pins", "pin"),
        ("segments", "segment"),
    ] {
        assert_eq!(
            stage["counts"][word].as_u64(),
            Some(of_class(&items, class).len() as u64),
            "counts.{word} must equal the number of `{class}` rows"
        );
    }

    // §2.4's key table, class by class: a layer and a box key on their instance;
    // a pin keys on its `PointId` when it has one; a segment and a metrics row
    // own no id at all and must therefore carry `null` rather than a synthesised
    // key.
    for i in of_class(&items, "layer") {
        assert!(i["key"].as_str().is_some_and(|k| k.starts_with('D')), "{i}");
        assert!(canon_path(i).is_some(), "{i}");
    }
    for i in of_class(&items, "box") {
        assert!(canon_path(i).is_some(), "a box names an instance: {i}");
    }
    for i in of_class(&items, "metrics") {
        assert!(
            i["key"].is_null() && i["canon_key"].is_null(),
            "a report row names a field, not an object (§2.4): {i}"
        );
        assert!(
            i["path"].as_str().is_some_and(|p| p.contains('.')),
            "a report row's path is `<family>.<field>`: {i}"
        );
    }

    // Every family M4 names is published, with a value or an explicit absence.
    let families: BTreeSet<&str> = of_class(&items, "metrics")
        .iter()
        .filter_map(|i| i["family"].as_str())
        .collect();
    for want in [
        "scope",
        "fidelity",
        "truth",
        "visual",
        "readability",
        "determinism",
        "connectivity",
        "engineer_style",
    ] {
        assert!(
            families.contains(want),
            "family `{want}` is missing from the report rows: {families:?}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ── The canonical sequence survives a rebuild ──

/// Sorting by the canonical key (O15) means the artifact's *order* survives a
/// rebuild, not merely its keys: inserting an instance adds rows without
/// reshuffling the ones already there.
///
/// The run-local key does move, and that asymmetry is the reason §2 keeps the
/// two forms apart — asserted, not noted.
#[test]
fn inserting_an_instance_does_not_reorder_the_canonical_sequence() {
    let base = scratch("rebuild-base");
    let inserted = scratch("rebuild-inserted");

    let a = run_on(&base, BASE_SRC);
    let b = run_on(&inserted, INSERTED_SRC);

    let boxes = |stage: &Value| -> Vec<String> {
        of_class(&items_of(stage), "box")
            .iter()
            .filter_map(|i| canon_path(i).map(str::to_string))
            .collect()
    };
    let keys = |stage: &Value| -> BTreeMap<String, String> {
        of_class(&items_of(stage), "box")
            .iter()
            .filter_map(|i| Some((canon_path(i)?.to_string(), i["key"].as_str()?.to_string())))
            .collect()
    };

    let (sa, sb) = (boxes(&a), boxes(&b));
    assert!(!sa.is_empty(), "the small circuit must give boxes");
    assert!(
        sa.windows(2).all(|w| w[0] < w[1]),
        "the canonical sequence must be sorted: {sa:?}"
    );

    let mut it = sb.iter();
    let mut matched = 0usize;
    for want in &sa {
        if it.any(|got| got == want) {
            matched += 1;
        }
    }
    assert_eq!(
        matched,
        sa.len(),
        "inserting an instance reshuffled the canonical sequence:\n before {sa:?}\n after  {sb:?}"
    );

    let (ka, kb) = (keys(&a), keys(&b));
    let moved: Vec<(&String, &String, &String)> = ka
        .iter()
        .filter_map(|(path, k)| {
            kb.get(path)
                .filter(|other| *other != k)
                .map(|o| (path, k, o))
        })
        .collect();
    assert!(
        !moved.is_empty(),
        "no run-local key moved when an instance was inserted — the key would \
         then be stable across builds, contradicting the measured §1.2 ② ⚠"
    );

    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&inserted);
}

// ── Engineer style: the soft family, and the count that makes it readable ──

/// Every engineer-style score comes with the count it was measured over — the
/// one family whose axes score `1.0` when they find nothing to measure.
///
/// Three things have to stay told apart, and all three are on this page: an
/// axis that measured nothing (`1.0`, count `0`), one that measured things and
/// found them in order (`1.0`, count > `0`), and the all-zero default an
/// **unwired** family would have published. The fixture fills both branches —
/// `bus_order` has nothing to measure, `signal_flow` has 49 chains — so neither
/// half of the family is asserted from the code alone.
#[test]
fn engineer_style_scores_come_with_the_count_they_were_measured_over() {
    let dir = scratch("engineer-style");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage viz failed: {err}");
    let items = items_of(&stage_of(&stdout));

    // Each score beside the count it was measured over. The right-hand names are
    // not derived from the left — the pairing is the claim, so it is spelled out.
    let axes = [
        ("signal_flow_monotonicity", "signal_flow_samples"),
        ("rail_alignment_score", "rail_alignment_samples"),
        ("ground_alignment_score", "ground_alignment_samples"),
        ("bus_order_score", "bus_order_samples"),
        ("idiom_proximity_score", "idiom_proximity_samples"),
        ("pin_side_intent_honor_rate", "pin_side_intent_samples"),
        ("functional_block_compactness", "functional_block_samples"),
        ("route_channel_clarity", "route_channel_samples"),
        ("label_readability_score", "label_readability_samples"),
    ];

    let (mut measured, mut vacuous, mut in_order) = (0usize, 0usize, 0usize);
    for (score_field, samples_field) in axes {
        let raw = metric(&items, &format!("engineer_style.{score_field}"));
        let score = raw
            .as_f64()
            .unwrap_or_else(|| panic!("`{score_field}` is a number, got {raw}"));
        let raw = metric(&items, &format!("engineer_style.{samples_field}"));
        let samples = raw
            .as_u64()
            .unwrap_or_else(|| panic!("`{samples_field}` is a count, got {raw}"));
        assert!(
            (0.0..=1.0).contains(&score),
            "`{score_field}` is a fraction between 0 and 1: {score}"
        );
        if samples == 0 {
            vacuous += 1;
            assert_eq!(
                score, 1.0,
                "`{score_field}` measured nothing, so it publishes the \
                 convention — and `{samples_field}` is what says so"
            );
        } else {
            measured += 1;
            if score == 1.0 {
                in_order += 1;
            }
        }
        assert!(
            score == 1.0 || samples > 0,
            "`{score_field}` cannot be below 1.0 without having measured \
             something, and `{samples_field}` says it measured nothing"
        );
    }

    assert!(
        measured > 0,
        "every axis is empty on a real board — the family is wired to nothing, \
         which is what its absence would have said more honestly"
    );
    assert!(
        vacuous > 0,
        "no axis is empty here, so the branch that distinguishes the convention \
         from a perfect score is unexercised"
    );
    assert!(
        in_order > 0,
        "no axis came out perfectly in order, so the other half of that branch \
         is unexercised"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
