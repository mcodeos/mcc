// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §7 phase two (batch 2a) acceptance: `stage.p2`.
//!
//! Phase two's row in the landing table asks for three things of every stage
//! view, and this file asserts all three for `stage.p2`:
//!
//! 1. **Two runs are byte-for-byte identical.** The views exist because
//!    `MC_VEC_DUMP` / `MC_VIZ_DUMP` were *unsorted* — every comparison was a bet
//!    on iteration order (§1.2 ③). Sorting once, at assembly, is the whole fix,
//!    so it is asserted at the byte level, on **both** faces: the text face is
//!    the default face and therefore enters the acceptance too (§5.3 ruling ③,
//!    O15).
//! 2. **The readout does not contradict the existing ones.** `stage.p2`'s
//!    instance and net counts are compared against `mcc build`'s own summary —
//!    two different aggregations (`InstTable` row classes vs. a walk over the
//!    `InstanceNode` tree) that must agree on the same build.
//! 3. **The key is insertion-independent** where it claims to be. The
//!    canonical key survives having an unrelated instance inserted ahead of it;
//!    the run-local key does not, and that asymmetry is asserted rather than
//!    noted — it is the entire reason §2 keeps two forms apart.
//!
//! The fixture is the real `tests/fixtures/hbl` project (seven layers) for the
//! CLI half and a small hand-written circuit for the key half. A toy fixture
//! cannot carry the byte-identity half: with one box and one net, sorting has
//! nothing to reorder and the assertion passes over a single element.
//!
//! ⚠ The text face's row set must actually fill every branch, or the branches
//! it does not reach are asserted vacuously ("green" because empty) — so
//! [`every_class_the_readout_reaches_is_exercised`] counts members per class
//! and fails if a class the readout can emit is thin. Both halves of the
//! fixture pair are needed: `hbl` reaches `label` and `bus` (sub-module port
//! members), the small circuit reaches an anonymous net.

use crate::common;

use mcc::{McIds, McURI};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Two capacitors plus a third wired between them. Named instances on purpose:
/// an auto-named device renumbers when a sibling is inserted, which would churn
/// the `path` column and hide the property under test.
///
/// Branches reached: 4 instances, 8 points (2 ports + 6 pins), 2 labelled nets
/// with 3 members each, and one anonymous net with 2 — so every class the
/// readout can emit that this fixture claims to cover has ≥ 2 members.
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

/// The same circuit with one more capacitor inserted *ahead* of the existing
/// ones. Nothing about `c1` / `c2` / `c3` changed.
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
///
/// Empty on purpose: the readout must not depend on where it is run, and a
/// stray file written into the fixture tree would be mistaken for a product.
/// Each call gets its own directory so two tests cannot see each other's
/// leftovers.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-p2-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc show stage p2 …` from `cwd` and return `(stdout, stderr, ok)`.
///
/// `--local` because `show stage` is local-only: with an `mcc start` service
/// running it is delegated and prints nothing (design §5.3 ① ⚠ — the same trap
/// `show lapper` / `show ast` have).
fn run_stage(cwd: &Path, extra: &[&str]) -> (String, String, bool) {
    let mut args = vec!["--local", "show", "stage", "p2"];
    args.extend_from_slice(extra);
    let entry = hbl_entry();
    args.push("-F");
    let entry = entry.to_str().expect("fixture path");
    args.push(entry);
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&args)
        .output()
        .expect("run mcc show stage p2");
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

/// Rows of the text face: everything after the `#` header lines, split on the
/// column separator and trimmed of the column padding.
///
/// Splitting on **two spaces** (not on any whitespace) is the documented
/// grammar — a cell may legitimately hold a single space, as
/// `main.MCU513.[VCC_1V2, GND]` does — and the padding a column adds is exactly
/// what the trim removes. A cell holding *two* spaces in a row would break the
/// row into five tokens, which the arity assertion in
/// [`the_text_face_presents_the_same_items_as_the_json_face`] then fails on.
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

// ── Determinism: two runs, byte for byte, on both faces ──

/// The projection of one run, re-serialized so two runs can be compared as
/// bytes regardless of which face produced them.
fn projection_bytes(stdout: &str) -> String {
    serde_json::to_string(&stage_of(stdout)).expect("the projection serializes")
}

/// The whole envelope, with the one field that **cannot** be stable blanked out.
///
/// `summary.elapsed_ms` is a wall-clock measurement: measured under load, it is
/// `0` when the envelope assembles in under a millisecond and `1` when it does
/// not. So the envelope *as a whole* is not a byte-stable artifact and no
/// acceptance can ask it to be — which is why the assertions below are split
/// into "the projection is identical" and "the clock is the *only* thing that
/// differs". The second is not a loosening: it is what keeps the first honest,
/// by failing if some other field quietly starts moving too.
fn envelope_without_clock(stdout: &str) -> String {
    let mut envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}"));
    envelope["result"]["summary"]["elapsed_ms"] = Value::from(0);
    serde_json::to_string(&envelope).expect("the envelope re-serializes")
}

/// The same scrub for the YAML face, which `serde_json` cannot read and which
/// writes the field on its own line.
fn yaml_without_clock(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with("elapsed_ms:"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The acceptance the whole view family exists for: two runs of the same source
/// are byte-for-byte identical.
///
/// The text face is compared **in full** — it is the default face, O15 puts it
/// inside this acceptance rather than treating it as a convenience view, and it
/// carries no measurement of its own. A machine face rides the shared envelope,
/// whose `summary.elapsed_ms` is a wall clock (see [`envelope_without_clock`]),
/// so there the projection and the clock are asserted separately.
#[test]
fn two_runs_are_byte_identical_on_every_face() {
    let first = scratch("det-a");
    let second = scratch("det-b");

    for format in ["text", "csv"] {
        let (a, ea, oka) = run_stage(&first, &["-f", format]);
        let (b, eb, okb) = run_stage(&second, &["-f", format]);
        // Branch coverage: a readout that printed nothing would compare equal.
        assert!(oka && okb, "`show stage p2 -f {format}` failed: {ea}{eb}");
        assert!(
            a.lines().count() > 2,
            "`-f {format}` printed only a header — the comparison below would be vacuous"
        );
        assert_eq!(
            a,
            b,
            "`-f {format}`: two runs of the same source differ ({} vs {} bytes)",
            a.len(),
            b.len()
        );
    }

    for format in ["json", "json-pretty"] {
        let (a, ea, oka) = run_stage(&first, &["-f", format]);
        let (b, eb, okb) = run_stage(&second, &["-f", format]);
        assert!(oka && okb, "`show stage p2 -f {format}` failed: {ea}{eb}");
        assert!(a.len() > 1000, "`-f {format}` printed nothing to compare");
        assert_eq!(
            projection_bytes(&a),
            projection_bytes(&b),
            "`-f {format}`: the stage view itself must be byte-identical"
        );
        assert_eq!(
            envelope_without_clock(&a),
            envelope_without_clock(&b),
            "`-f {format}`: `summary.elapsed_ms` must be the *only* field that \
             differs between two runs"
        );
    }

    let (a, ea, oka) = run_stage(&first, &["-f", "yaml"]);
    let (b, eb, okb) = run_stage(&second, &["-f", "yaml"]);
    assert!(oka && okb, "`show stage p2 -f yaml` failed: {ea}{eb}");
    assert_eq!(
        yaml_without_clock(&a),
        yaml_without_clock(&b),
        "`-f yaml`: `elapsed_ms` must be the only line that differs"
    );
    // The YAML face carries the same `view`, so it cannot be a different shape.
    assert!(a.contains("view: stage.p2"), "{a:.200}");

    let _ = std::fs::remove_dir_all(&first);
    let _ = std::fs::remove_dir_all(&second);
}

/// The world token is a fingerprint of the loaded source set: present here, and
/// equal for two runs of one source. Two *different* sources must not collide,
/// or the token is a constant wearing a hash's clothes.
#[test]
fn the_world_token_fingerprints_the_loaded_sources() {
    let dir = scratch("wv");
    let (stdout, stderr, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage p2 failed: {stderr}");
    let stage = stage_of(&stdout);

    let wv = stage["world_ver"]
        .as_str()
        .unwrap_or_else(|| panic!("world_ver must be a token for a loadable project: {stage}"));
    assert!(wv.starts_with("w_"), "world_ver spells its form: {wv}");

    // A second, unrelated project: a different source set must give a different
    // token. (If `world_ver` were a constant, every assertion above would hold.)
    let other = scratch("wv-other");
    std::fs::write(
        other.join("other.mc"),
        "module other {\n    io A\n    io B\n}\n",
    )
    .expect("write the second source");
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(&other)
        .args(["--local", "show", "stage", "p2", "-f", "json", "-F"])
        .arg(other.join("other.mc"))
        .output()
        .expect("run mcc on the second source");
    let other_stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let other_stage = stage_of(&other_stdout);
    let other_wv = other_stage["world_ver"].as_str().unwrap_or("");
    assert_ne!(
        wv, other_wv,
        "two different source sets must not fingerprint to one token"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&other);
}

// ── One `items`, two faces ──

/// §5.3 ruling ③: text and JSON render from *one* traversal. Two traversals drift,
/// so the two faces are required to present the same sequence — same rows, in
/// the same order, with the same key and the same `loc`.
#[test]
fn the_text_face_presents_the_same_items_as_the_json_face() {
    let dir = scratch("faces");
    let (text, et, okt) = run_stage(&dir, &[]);
    let (json, ej, okj) = run_stage(&dir, &["-f", "json"]);
    assert!(okt && okj, "show stage p2 failed: {et}{ej}");

    let items = stage_of(&json)["items"]
        .as_array()
        .expect("items is an array")
        .clone();
    let rows = text_rows(&text);

    assert!(items.len() > 20, "hbl must give a substantial view");
    assert_eq!(
        rows.len(),
        items.len(),
        "the text face must print exactly one row per item"
    );

    for (item, row) in items.iter().zip(rows.iter()) {
        // Arity 4 also asserts that no cell contains a space: the split above
        // is on whitespace, so a value like `3 members` would show up as five
        // tokens and break the row's columns.
        assert_eq!(
            row.len(),
            4,
            "each text row is key/target/class/loc, with no cell containing a \
             space: {row:?}"
        );
        // First column is always the key; an object that owns none prints the
        // missing-value glyph (§5.3: never an empty column).
        let want_key = item["key"].as_str().unwrap_or("-");
        assert_eq!(row[0], want_key, "first column is the key, row {row:?}");
        // Last column is always `loc`, back to the source.
        let loc = &item["loc"];
        let want_loc = match loc["uri"].as_str() {
            Some(uri) => format!("{uri}:{}", loc["line"].as_u64().unwrap_or(0)),
            None => "-".to_string(),
        };
        assert_eq!(row[3], want_loc, "last column is `loc`, item {item}");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// §5.3's four prohibitions on the text face, asserted rather than trusted.
#[test]
fn the_text_face_obeys_the_four_prohibitions() {
    let dir = scratch("prose");
    let (text, err, ok) = run_stage(&dir, &[]);
    assert!(ok, "show stage p2 failed: {err}");

    assert!(
        !text.contains('\t'),
        "no tab-delimited columns: widths are computed from the data"
    );
    assert!(
        !text.contains('\u{1b}'),
        "no ANSI escapes — this face is read by machines as well as people"
    );
    for line in text.lines() {
        assert!(
            !line.contains('\u{2500}') && !line.contains('\u{2502}'),
            "no box drawing: {line:?}"
        );
    }
    // The header names the view, the scope and the world token (§5.3 ①).
    let header = text.lines().next().expect("a header line");
    assert!(header.starts_with("# stage.p2"), "{header}");
    assert!(header.contains("world_ver="), "{header}");

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Branch coverage of the item classes ──

/// Each object kind the readout emits, and which key it is entitled to (§2.4 —
/// **not every object is a point**). A class with fewer than two members in the
/// fixture is asserted rather than tolerated: a one-member branch is a branch
/// the acceptance silently skips.
#[test]
fn every_class_the_readout_reaches_is_exercised() {
    let dir = scratch("classes");
    let (stdout, err, ok) = run_stage(&dir, &["-f", "json"]);
    assert!(ok, "show stage p2 failed: {err}");
    let stage = stage_of(&stdout);
    let items = stage["items"].as_array().expect("items is an array");

    let of =
        |class: &str| -> Vec<&Value> { items.iter().filter(|i| i["class"] == class).collect() };

    for class in ["instance", "point", "net", "label", "bus"] {
        let got = of(class);
        assert!(
            got.len() >= 2,
            "class `{class}` has {} member(s) in hbl — a branch this thin is not \
             exercised by the assertions below",
            got.len()
        );
    }

    // An instance is keyed and carries the canonical key's def half.
    for i in of("instance") {
        assert!(i["key"].as_str().is_some_and(|k| k.starts_with('D')), "{i}");
        assert!(i["canon_key"]["path"].as_str().is_some(), "{i}");
    }

    // A point carries **both** keys: the run-local handle and the canonical
    // path. §3: a view carrying only the run-local one is void after the next
    // compiler change.
    //
    // ⚠ Not every `point`-class row has one, and that is the design, not a gap:
    // §2.4 says `InstEntry.point` is `None` for the three kinds that own no
    // physical point — a label, a bus member, and an endpoint the router
    // synthesised (here `main.DCDC.GND`, a sub-module port absorbed into the
    // `V3V3.GND` bus). What must never happen is a *half*-filled row: a point
    // with no canonical path is exactly the artifact §3 warns about.
    let mut keyed = 0usize;
    let mut keyless = 0usize;
    for i in of("point") {
        assert!(
            i["canon_key"]["path"].as_str().is_some(),
            "the canonical path is the one key every point-class row has: {i}"
        );
        match i["point"].as_str() {
            Some(p) => {
                keyed += 1;
                assert_eq!(
                    i["point"], i["key"],
                    "the run-local key *is* the point id: {i}"
                );
                assert!(p.contains(':'), "a PointId is `N<node>:<member>`: {p}");
            }
            None => {
                keyless += 1;
                assert!(
                    i["key"].is_null(),
                    "an object with no point has no run-local key either — \
                     fabricating one is what §0.2 forbids: {i}"
                );
            }
        }
    }
    assert!(
        keyed >= 2,
        "hbl must actually exercise the keyed branch: {keyed}"
    );
    assert!(
        keyless >= 2,
        "and the point-less branch (§2.4's three kinds): {keyless}"
    );

    // A label / bus member owns no physical point, so it has **no** key at all
    // — not a synthesised one (§2.4: fabricating an id is exactly what the
    // design's discipline 2 forbids).
    for class in ["label", "bus"] {
        for i in of(class) {
            assert!(i["key"].is_null(), "{class} owns no key: {i}");
            assert!(i["point"].is_null(), "{class} owns no point: {i}");
        }
    }

    // A labelled net keys on its label; an anonymous one has no key and is
    // matched by member-set overlap, so its member list must be present and
    // sorted.
    let mut labelled = 0;
    let mut anon = 0;
    for i in of("net") {
        let members = i["members"]
            .as_array()
            .unwrap_or_else(|| panic!("every net lists its members: {i}"));
        let mut sorted = members.clone();
        sorted.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        assert_eq!(members, &sorted, "a net's member set is sorted: {i}");
        match i["key"].as_str() {
            Some(k) => {
                assert!(
                    k.starts_with("net:"),
                    "a labelled net keys on its label: {i}"
                );
                labelled += 1;
            }
            None => anon += 1,
        }
    }
    assert!(labelled >= 2, "hbl has labelled nets: {labelled}");
    assert!(anon >= 2, "hbl has anonymous nets: {anon}");

    // The counts line is derived from these same items, so it cannot disagree
    // with them.
    let counts = &stage["counts"];
    assert_eq!(
        counts["instances"].as_u64(),
        Some(of("instance").len() as u64)
    );
    assert_eq!(counts["points"].as_u64(), Some(of("point").len() as u64));
    assert_eq!(counts["nets"].as_u64(), Some(of("net").len() as u64));

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Non-contradiction with the existing readouts ──

/// `stage.p2` reads `InstTable` rows by class; `mcc build`'s summary walks the
/// `InstanceNode` tree and counts `pass2.nets`. Two different aggregations over
/// the same build must agree — this is the "no contradiction" half of phase two's
/// acceptance, and it is checked against a *second command*, not against this
/// file's own arithmetic.
#[test]
fn the_counts_do_not_contradict_the_build_summary() {
    let dir = scratch("agree");
    let (stage_stdout, es, oks) = run_stage(&dir, &["-f", "json"]);
    assert!(oks, "show stage p2 failed: {es}");

    let entry = hbl_entry();
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(&dir)
        .args(["--local", "build", "-f", "json"])
        .arg(&entry)
        .output()
        .expect("run mcc build");
    // ⚠ `build`'s exit code is **not** asserted: hbl reports an electrical
    // error (`driver-conflict` on `V3V3.GND`), so `build` exits non-zero while
    // still emitting its envelope. That asymmetry is the point of this test —
    // the readout answers 0 regardless (law C: a view is a readout, not a
    // verdict), and a caller who wants the verdict asks `build` for it.
    let build: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout))
        .unwrap_or_else(|e| panic!("build emitted no envelope (exit {:?}): {e}", out.status));

    let stage = stage_of(&stage_stdout);
    let summary = &build["result"]["summary"];
    assert_eq!(
        stage["counts"]["instances"], summary["instance_count"],
        "stage.p2's instance rows and the build's instance tree must count the \
         same instances"
    );
    assert_eq!(
        stage["counts"]["nets"], summary["net_count"],
        "stage.p2's net rows and the build's net list must count the same nets"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── The key: two forms, and only one of them crosses builds ──

/// Build `main` in a fresh workspace and return the `stage.p2` items.
///
/// Library-level rather than CLI-level: the property under test is about the
/// key, not about the command, and going in through `build_p2` puts the two
/// builds in one process where their items can be compared field by field.
fn items_of(source: &str) -> Vec<Value> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/stage-p2-view.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    mcc::stages::p2::build_p2(&table, "main", 0).items
}

fn item_of<'a>(items: &'a [Value], class: &str, path: &str) -> &'a Value {
    items
        .iter()
        .find(|i| i["class"] == class && i["path"] == path)
        .unwrap_or_else(|| panic!("no {class} item at {path}"))
}

/// The canonical key of every point, as one string, keyed by instance path.
fn canonical_points(items: &[Value]) -> BTreeMap<String, String> {
    items
        .iter()
        .filter(|i| i["class"] == "point")
        .map(|i| {
            (
                i["path"].as_str().expect("a point has a path").to_string(),
                i["canon_key"].to_string(),
            )
        })
        .collect()
}

/// Inserting an unrelated instance ahead of the existing ones leaves every
/// pre-existing point's **canonical** key untouched.
#[test]
fn inserting_an_instance_leaves_canonical_keys_untouched() {
    let base = canonical_points(&items_of(BASE_SRC));
    let inserted = canonical_points(&items_of(INSERTED_SRC));

    for path in [
        "main.c1.1",
        "main.c1.2",
        "main.c2.1",
        "main.c2.2",
        "main.c3.1",
        "main.c3.2",
    ] {
        let before = base
            .get(path)
            .unwrap_or_else(|| panic!("base fixture has {path}"));
        let after = inserted
            .get(path)
            .unwrap_or_else(|| panic!("insertion must not rename {path}"));
        assert_eq!(
            before, after,
            "{path}: the canonical key is the form that crosses builds, so it \
             cannot move when an unrelated instance is inserted"
        );
    }

    // And the canonical key is not degenerate: the two devices do not share one.
    assert_ne!(base["main.c1.1"], base["main.c2.1"]);
    assert_ne!(base["main.c1.1"], base["main.c3.1"]);
}

/// WITNESS — the other half of the two-form rule, and the reason the text face
/// prints the run-local key as a *fast channel* rather than as the identity:
/// a `PointId`'s node half is the ordinal of first interning, so one device
/// inserted ahead pushes every later device's number up.
///
/// If this ever fails because the numbers stopped moving, that is good news
/// that invalidates a documented limitation — revisit
/// `stage-readout-design.md` §1.2 ② / §2 before "fixing" the assertion.
#[test]
fn the_run_local_key_does_move_when_an_instance_is_inserted() {
    let base = items_of(BASE_SRC);
    let inserted = items_of(INSERTED_SRC);

    let before = item_of(&base, "point", "main.c1.1")["key"].clone();
    let after = item_of(&inserted, "point", "main.c1.1")["key"].clone();
    assert_ne!(
        before, after,
        "`main.c1` keeps its canonical path but takes a new number: one device \
         was interned ahead of it"
    );
    // The two forms are published side by side, so a consumer never has to pick
    // one blind.
    assert_eq!(before, item_of(&base, "point", "main.c1.1")["point"]);
    assert!(after.is_string() && before.is_string());
}

/// A net with no label has **no key** — it is matched by member-set overlap.
/// And a labelled net keys on its label, which is spelled `net:<label>` in the
/// text face's key column (§5.3 ①).
#[test]
fn a_labelled_net_keys_on_its_label_and_an_anonymous_one_carries_no_key() {
    let items = items_of(BASE_SRC);

    let labelled: Vec<&Value> = items
        .iter()
        .filter(|i| i["class"] == "net" && i["key"].is_string())
        .collect();
    let anon: Vec<&Value> = items
        .iter()
        .filter(|i| i["class"] == "net" && i["key"].is_null())
        .collect();

    assert_eq!(labelled.len(), 2, "VDD and GND are the labelled nets");
    assert_eq!(anon.len(), 1, "the c2/c3 junction is the anonymous net");

    for net in &labelled {
        let label = net["net"].as_str().expect("a net has a name");
        assert_eq!(net["key"], format!("net:{label}"));
        // ≥ 2 members, so the member-set comparison below is not degenerate.
        assert!(
            net["members"].as_array().is_some_and(|m| m.len() >= 2),
            "{net}"
        );
    }
    for net in &anon {
        assert!(
            net["members"].as_array().is_some_and(|m| m.len() >= 2),
            "a keyless net is compared by member-set overlap, so its members \
             must be present: {net}"
        );
        assert!(net["canon_key"].is_null());
    }
}
