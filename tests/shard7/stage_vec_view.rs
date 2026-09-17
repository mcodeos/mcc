// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §7 phase two (batch 2b) acceptance: `stage.vec`.
//!
//! Phase two asks three things of every stage view, and this file asserts all
//! three for the vector graph:
//!
//! 1. **Two runs are byte-for-byte identical**, on both faces (§5.3 ruling ③,
//!    O15 — text is the default face and enters the acceptance).
//! 2. **The readout does not contradict the existing ones.** Two independent
//!    checks carry this, and both need an *explanation* rather than a matching
//!    number:
//!    * the net count the view publishes must equal the projection's own
//!      account of what it produced (the log's per-layer `after` sum), and at
//!      least one layer must actually move nets — otherwise the log carries no
//!      information a single number could not;
//!    * every box and every endpoint must resolve to a Pass2 row of the same
//!      build, with the one structural exception named explicitly (the root
//!      instance is a *layer*, not a box).
//! 3. **The canonical sequence survives a rebuild.** Sorting by the canonical
//!    key (O15) is what makes the artifact's *order* — not merely its keys —
//!    comparable across builds, so inserting an instance must leave the existing
//!    sequence a subsequence of the new one.
//!
//! The fixture is the real `tests/fixtures/hbl` project (seven layers, 67 boxes,
//! 60 nets, 186 endpoints, 16 trunks), for the same reason the `stage.p2`
//! acceptance uses it: a toy fixture gives sorting nothing to reorder.
//!
//! ⚠ The harness runs every CLI invocation in a **fresh empty directory**.
//! `viz/project.rs` unconditionally writes `baseline/render_projection.md`
//! relative to the current directory; that file is gitignored, is not a product
//! of this batch, and using it as an oracle would be circular — it is written by
//! the same code the view reads. The view reads the in-memory log only.

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
        "mcc-stage-vec-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc show stage vec …` from `cwd` against the hbl fixture.
fn run_stage(cwd: &Path, extra: &[&str]) -> (String, String, bool) {
    run_seg(cwd, "vec", extra)
}

/// Run `mcc show stage <seg> …` from `cwd` against the hbl **project**.
///
/// The project root is handed over as the `-F` entry, not as a copied source
/// file: `hbl.mc` alone is one file of a five-file project, so copying it into a
/// scratch directory would read a different (much thinner) world than the one
/// the vec half of the test reads.
fn run_seg(cwd: &Path, seg: &str, extra: &[&str]) -> (String, String, bool) {
    let mut args = vec!["--local", "show", "stage", seg];
    args.extend_from_slice(extra);
    args.push("-F");
    let entry = hbl_entry();
    args.push(entry.to_str().expect("fixture path"));
    run(cwd, &args)
}

/// The same, as a parsed stage view.
fn seg_of_hbl(cwd: &Path, seg: &str) -> Value {
    let (stdout, stderr, ok) = run_seg(cwd, seg, &["-f", "json"]);
    assert!(ok, "show stage {seg} failed: {stderr}");
    stage_of(&stdout)
}

/// Run `mcc show stage <seg> …` against a standalone source written into `cwd`.
fn run_on(cwd: &Path, seg: &str, source: &str) -> Value {
    let path = cwd.join("circuit.mc");
    std::fs::write(&path, source).expect("write the source");
    let (stdout, stderr, ok) = run(
        cwd,
        &[
            "--local",
            "show",
            "stage",
            seg,
            "-f",
            "json",
            "-F",
            path.to_str().expect("source path"),
        ],
    );
    assert!(ok, "show stage {seg} failed: {stderr}");
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

// ── Determinism ──

/// Two runs of one source are byte-for-byte identical, on every face.
#[test]
fn two_runs_are_byte_identical_on_every_face() {
    let first = scratch("det-a");
    let second = scratch("det-b");

    for format in ["json", "json-pretty"] {
        let (a, ea, oka) = run_stage(&first, &["-f", format]);
        let (b, eb, okb) = run_stage(&second, &["-f", format]);
        assert!(oka && okb, "`show stage vec -f {format}` failed: {ea}{eb}");
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
        "`-f yaml`: `elapsed_ms` must be the only line that differs"
    );
    assert!(ya.contains("view: stage.vec"), "{ya:.200}");

    let _ = std::fs::remove_dir_all(&first);
    let _ = std::fs::remove_dir_all(&second);
}

// ── Not contradicting the existing readouts ──

/// The projection log is the *same computation* that produced the graph, not a
/// second derivation of it.
///
/// Three readings of one number must agree — the view's own header count, the
/// count of net items, and the projection's sum of "nets after" — and the log
/// must additionally show a layer where the projection actually *moved* nets.
/// Without that last part the assertion would pass on a log of zeros.
#[test]
fn the_projection_is_the_same_computation_as_the_net_count() {
    let dir = scratch("proj");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage vec failed: {err}");
    let stage = stage_of(&stdout);
    let items = items_of(&stage);

    let nets = of_class(&items, "net").len() as u64;
    assert_eq!(
        stage["counts"]["nets"].as_u64(),
        Some(nets),
        "the header count and the net rows must be one number"
    );

    let per_layer: Vec<&Value> = of_class(&items, "projection")
        .into_iter()
        .filter(|i| i["before"].as_u64().is_some())
        .collect();
    assert!(
        per_layer.len() >= 2,
        "the log must carry one row per layer, got {}",
        per_layer.len()
    );
    let after_sum: u64 = per_layer
        .iter()
        .map(|i| i["after"].as_u64().unwrap_or(0))
        .sum();
    let before_sum: u64 = per_layer
        .iter()
        .map(|i| i["before"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(
        after_sum, nets,
        "the view's net count must be the projection's own after-sum: the graph \
         is built from the projected block, so re-deriving the count separately \
         would let the two agree by luck"
    );

    // A layer where the projection removed nets. This is the whole reason the
    // log is published: one number could never say "19 became 14".
    let moved: Vec<&&Value> = per_layer
        .iter()
        .filter(|i| i["before"].as_u64() > i["after"].as_u64())
        .collect();
    assert!(
        !moved.is_empty(),
        "no layer in hbl reports a net-count change — the log would then carry \
         nothing a single number could not (before={before_sum}, after={after_sum})"
    );

    // Each action records a rule the projection actually implements. The rules
    // are the projection's own vocabulary: a=merge, b=endpoint dedup, c=pseudo
    // endpoint removal.
    let actions = of_class(&items, "projection")
        .into_iter()
        .filter(|i| i["before"].is_null())
        .count();
    assert!(actions > 0, "the log must also carry per-action rows");
    for i in of_class(&items, "projection") {
        let rule = i["rule"].as_str().expect("a rule");
        assert!(
            matches!(rule, "-" | "a" | "b" | "c"),
            "the projection's vocabulary is a|b|c: {i}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every box is a Pass2 instance, and the one instance with no box is the root —
/// which this view renders as a **layer** instead.
///
/// Two different pipelines (`InstTable` row classes vs. the block builder's
/// boxes) must agree on the same build, and the disagreement has to have a
/// structural explanation rather than a tolerance.
#[test]
fn a_box_is_an_instance_and_the_root_is_the_layer() {
    let dir = scratch("boxes");
    let p2 = seg_of_hbl(&dir, "p2");
    let vec = seg_of_hbl(&dir, "vec");

    let instances: BTreeSet<String> = of_class(&items_of(&p2), "instance")
        .iter()
        .filter_map(|i| canon_path(i).map(str::to_string))
        .collect();
    let boxes: BTreeSet<String> = of_class(&items_of(&vec), "box")
        .iter()
        .filter_map(|i| canon_path(i).map(str::to_string))
        .collect();
    let layers: BTreeSet<String> = of_class(&items_of(&vec), "layer")
        .iter()
        .filter_map(|i| canon_path(i).map(str::to_string))
        .collect();

    assert!(
        !boxes.is_empty() && !instances.is_empty(),
        "the fixture is thin"
    );

    let unknown: Vec<&String> = boxes.difference(&instances).collect();
    assert!(
        unknown.is_empty(),
        "a box names an instance Pass2 does not know: {unknown:?}"
    );

    let without_box: Vec<&String> = instances.difference(&boxes).collect();
    assert_eq!(
        without_box.len(),
        1,
        "exactly one Pass2 instance is not a box: the root, which is a layer \
         instead — anything else is a dropped or duplicated instance: \
         {without_box:?}"
    );
    assert!(
        layers.contains(without_box[0]),
        "the instance without a box must be the root layer: {:?} vs {layers:?}",
        without_box[0]
    );

    // Every layer's own path is an instance Pass2 knows, too.
    let unknown_layers: Vec<&String> = layers.difference(&instances).collect();
    assert!(
        unknown_layers.is_empty(),
        "a layer names an instance Pass2 does not know: {unknown_layers:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A `net:` key names a net **the source wrote**, and every such net is one
/// Pass2 knows too.
///
/// The two segments count different things — Pass2's table has 59 net rows over
/// 35 names, this view 60 over 48 — and that is legitimate, because they are
/// different segments. What must not happen is a key that claims to name
/// something no other readout can find. So: a keyed net's name must appear among
/// Pass2's net names, and a net the compiler minted (an anonymous `_net<k>`, or
/// the builder's `<base>~<k>` segment name) must carry no key and fall back on
/// its member set.
#[test]
fn a_keyed_net_names_a_source_net_that_pass2_knows() {
    let dir = scratch("netkeys");
    let p2 = seg_of_hbl(&dir, "p2");
    let vec = seg_of_hbl(&dir, "vec");

    let p2_names: BTreeSet<String> = of_class(&items_of(&p2), "net")
        .iter()
        .filter_map(|n| n["net"].as_str().map(str::to_string))
        .collect();
    assert!(p2_names.len() > 20, "the Pass2 net table is thin");

    let vec_items = items_of(&vec);
    let nets = of_class(&vec_items, "net");
    let keyed: Vec<&Value> = nets
        .iter()
        .copied()
        .filter(|n| !n["key"].is_null())
        .collect();
    let keyless: Vec<&Value> = nets
        .iter()
        .copied()
        .filter(|n| n["key"].is_null())
        .collect();
    assert!(
        !keyed.is_empty() && !keyless.is_empty(),
        "both branches must fill"
    );

    for n in &keyed {
        let name = n["name"].as_str().expect("a name");
        assert_eq!(n["key"].as_str(), Some(format!("net:{name}").as_str()));
        assert_eq!(n["origin"].as_str(), Some("source"), "{n}");
        assert!(
            p2_names.contains(name),
            "`{name}` is keyed as a source net but Pass2 has no net of that name"
        );
    }

    for n in &keyless {
        let origin = n["origin"].as_str().expect("an origin");
        assert!(
            matches!(origin, "segment" | "anonymous"),
            "a net with no key must be one the compiler minted, not a source name \
             the view failed to key: {n}"
        );
        // The keyless net's handle is its member set, and it must be present —
        // `null` there would leave the item unidentifiable by any means.
        assert!(
            n["members"].as_array().is_some_and(|m| !m.is_empty()),
            "a keyless net must still publish its members: {n}"
        );
    }

    // Both minted families are exercised on the fixture.
    for want in ["segment", "anonymous"] {
        assert!(
            keyless.iter().any(|n| n["origin"].as_str() == Some(want)),
            "no `{want}` net in hbl — that branch would pass unwatched"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// A Pass2-labelled net that the vec graph no longer carries is **named by the
/// projection's own log**.
///
/// This is the reconciliation the phase exists for. The projection merges and
/// drops nets, so the two segments legitimately disagree on the count; what makes
/// them non-contradictory is that the log says which net went where. On hbl the
/// only lost name is `V3V3.GND`, and the log's rule `a` records it as
/// `union 4 nets: GND + V1V2.GND + V3V3.GND + V5V.GND` — the answer to "why does
/// `verify` count more nets than this view".
#[test]
fn a_net_the_projection_removed_is_named_in_the_log() {
    let dir = scratch("lost");
    let p2 = seg_of_hbl(&dir, "p2");
    let vec = seg_of_hbl(&dir, "vec");

    let p2_keyed: BTreeSet<String> = of_class(&items_of(&p2), "net")
        .iter()
        .filter_map(|n| n["key"].as_str().map(|k| k.to_string()))
        .collect();
    let vec_named: BTreeSet<String> = of_class(&items_of(&vec), "net")
        .iter()
        .filter_map(|n| n["name"].as_str().map(str::to_string))
        .collect();

    let lost: Vec<String> = p2_keyed
        .iter()
        .map(|k| k.trim_start_matches("net:").to_string())
        .filter(|n| !vec_named.contains(n))
        .collect();
    assert!(
        !lost.is_empty(),
        "hbl is expected to lose a net in the projection; with none lost this \
         assertion proves nothing"
    );

    // Every record, as one searchable string: a lost net may be named as the
    // record's subject or inside its note (a union names its members there).
    let records: Vec<String> = of_class(&items_of(&vec), "projection")
        .into_iter()
        .filter(|i| i["before"].is_null())
        .map(|i| i.to_string())
        .collect();

    for name in &lost {
        assert!(
            records.iter().any(|r| r.contains(name)),
            "Pass2 net `{name}` is gone from the vec graph and no projection \
             record mentions it — the two views then contradict each other with \
             nothing to explain the difference"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every endpoint resolves to a Pass2 row, and the run-local `point` key is
/// present **only** where Pass2 says the row is a physical point.
///
/// The implication runs one way on purpose, and the fixture shows why: 26
/// endpoints in hbl name a `point`-class row and still carry no `PointId` — the
/// module ports whose crossing is a boundary, not a connection (§2.4: not every
/// object is a point). Asserting equality instead would fail on the design.
#[test]
fn every_endpoint_resolves_to_a_pass2_row() {
    let dir = scratch("endpoints");
    let p2 = seg_of_hbl(&dir, "p2");
    let vec = seg_of_hbl(&dir, "vec");

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
    assert!(known.len() > 100, "the Pass2 table must be substantial");

    let vec_items = items_of(&vec);
    let endpoints = of_class(&vec_items, "endpoint");
    assert!(
        endpoints.len() >= 20,
        "hbl must give a substantial endpoint set, got {}",
        endpoints.len()
    );

    let mut with_point = 0usize;
    let mut keyless_points = 0usize;
    for e in &endpoints {
        let path = canon_path(e).unwrap_or_else(|| {
            panic!("an endpoint with an InstTable row must carry a canonical key: {e}")
        });
        let classes = known.get(path).unwrap_or_else(|| {
            panic!("endpoint {path} names no Pass2 row — the two segments disagree")
        });

        let has_point = e["key"].as_str().is_some();
        if has_point {
            with_point += 1;
            assert!(
                classes.contains("point"),
                "an endpoint carries a `PointId` while Pass2 classes {path} as \
                 {classes:?} — the key would then name something that is not a point"
            );
        } else if classes.contains("point") {
            keyless_points += 1;
        }
    }

    assert!(
        with_point > 0,
        "no endpoint carries a `point` key — the run-local half is unexercised"
    );
    assert!(
        keyless_points > 0,
        "no endpoint resolves to a point-class row without a `point` key — the \
         `None` family (§2.4) is unexercised, so the implication above is \
         asserted over a branch with no members"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── One `items`, two faces ──

#[test]
fn the_text_face_presents_the_same_items_as_the_json_face() {
    let dir = scratch("faces");
    let (text, et, okt) = run_stage(&dir, &[]);
    let (json, ej, okj) = run_stage(&dir, &["-f", "json"]);
    assert!(okt && okj, "show stage vec failed: {et}{ej}");

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
    assert!(ok, "show stage vec failed: {err}");

    assert!(!text.contains('\t'), "no tab-delimited columns");
    assert!(!text.contains('\u{1b}'), "no ANSI escapes");
    for line in text.lines() {
        assert!(
            !line.contains('\u{2500}') && !line.contains('\u{2502}'),
            "no box drawing: {line:?}"
        );
    }
    let header = text.lines().next().expect("a header line");
    assert!(header.starts_with("# stage.vec"), "{header}");
    assert!(header.contains("world_ver="), "{header}");

    // A missing value prints `-`, never an empty column: trunks and projection
    // rows own no key at all, so their first column is exactly the glyph.
    let missing = text_rows(&text).into_iter().filter(|r| r[0] == "-").count();
    assert!(
        missing >= 2,
        "the keyless classes (trunk, projection) must print the missing-value \
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
    assert!(ok, "show stage vec failed: {err}");
    let stage = stage_of(&stdout);
    let items = items_of(&stage);

    for class in ["layer", "box", "net", "endpoint", "trunk", "projection"] {
        let got = of_class(&items, class);
        assert!(
            got.len() >= 2,
            "class `{class}` has {} member(s) in hbl — too thin to be exercised",
            got.len()
        );
    }

    // §2.4's key table, class by class: a layer and a box key on their instance;
    // a net keys on its label only when it has one; a trunk and a projection row
    // own no id at all and must therefore carry `null` rather than a synthesised
    // key.
    for i in of_class(&items, "layer") {
        assert!(i["key"].as_str().is_some_and(|k| k.starts_with('D')), "{i}");
        assert!(canon_path(i).is_some(), "{i}");
    }
    for i in of_class(&items, "box") {
        assert!(canon_path(i).is_some(), "a box names an instance: {i}");
    }
    for i in of_class(&items, "trunk") {
        assert!(
            i["key"].is_null() && i["canon_key"].is_null(),
            "a trunk owns no id (§2.4) — a key here would be fabricated: {i}"
        );
        assert!(i["lanes"].as_array().is_some_and(|l| !l.is_empty()), "{i}");
    }

    // A labelled net keys on its label; an anonymous one carries no key at all
    // and is identified by its member set (§2.4). Only the first is asserted
    // here — hbl may legitimately have no anonymous net — and the `key: null`
    // branch is exercised regardless by the trunk and projection rows.
    let nets = of_class(&items, "net");
    let labelled = nets.iter().filter(|n| n["key"].as_str().is_some()).count();
    assert!(labelled > 0, "no labelled net in hbl");
    for n in &nets {
        assert!(
            n["members"].as_array().is_some_and(|m| !m.is_empty()),
            "every net must publish its member set: {n}"
        );
        if let Some(k) = n["key"].as_str() {
            assert!(
                k.starts_with("net:"),
                "a labelled net keys on its label: {n}"
            );
        }
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

    let a = run_on(&base, "vec", BASE_SRC);
    let b = run_on(&inserted, "vec", INSERTED_SRC);

    let seq = |stage: &Value| -> Vec<String> {
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

    let (sa, sb) = (seq(&a), seq(&b));
    assert!(!sa.is_empty(), "the small circuit must give boxes");
    assert!(
        sa.windows(2).all(|w| w[0] < w[1]),
        "the canonical sequence must be sorted: {sa:?}"
    );

    // `a`'s sequence is a subsequence of `b`'s: inserting `c0` adds a row, it
    // does not reorder anything.
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

    // And the run-local handle really does move — otherwise the two forms would
    // be interchangeable and §2's whole distinction would be untested.
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
