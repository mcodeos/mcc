// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc diff` over **saved readings** — the second kind of operand.
//!
//! CIMP §1 U96, ruled 2026-09-18: the operand gains a kind, a reading saved
//! earlier by `mcc show stage <seg> -f json -o <file>`. The reason is §6.2's
//! same-source difference: "same source, two compilers" cannot be asked of one
//! process, because a process holds one binary, so each side has to be read by
//! the build in question and saved, and the difference is then taken over the
//! two files.
//!
//! This file locks the two things that makes true:
//!
//! - **a saved reading is the same reading** — a difference over two archives
//!   is *the same rows* as a difference over the two worlds they were read from
//!   (not merely a similar page), so the operand kind changes what is needed to
//!   ask the question and not what the answer is;
//! - **a saved reading states what it is** — the self-description: which segment
//!   (`view`) and which alignment table (`key_table`). A pairing that disagrees
//!   on either is refused rather than answered, because the answer would be this
//!   build's key table speaking for readings it does not describe.
//!
//! The refusals are exercised with readings whose fields are **rewritten** after
//! the fact (`rewrite`): one build cannot otherwise produce a reading of another
//! segment, or under another table, and a lock that needs two builds is not a
//! lock. Each rewritten fixture says so where it is used.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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

/// A fresh, **empty** directory to run a CLI invocation in.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-saved-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Write a source into `<scratch>/<name>.mc`; returns its path.
fn source(name: &str, src: &str) -> PathBuf {
    let dir = scratch(&format!("{name}-src"));
    let path = dir.join("circuit.mc");
    std::fs::write(&path, src).expect("write the source");
    path
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

/// Save a `stage.<seg>` reading of `target` — the documented producer, spelled
/// exactly as the design spells it — and return the file written.
fn save(cwd: &Path, seg: &str, target: &Path, name: &str) -> PathBuf {
    let out = cwd.join(format!("{name}.json"));
    let (_, err, ok) = run(
        cwd,
        &[
            "--local",
            "show",
            "stage",
            seg,
            "-F",
            target.to_str().expect("source path"),
            "-f",
            "json",
            "-o",
            out.to_str().expect("archive path"),
        ],
    );
    assert!(ok, "`show stage {seg} -o` failed: {err}");
    assert!(
        out.is_file(),
        "`-o` must write the reading; a lock over an archive needs one"
    );
    out
}

/// The `result.stage` block of a `-f json` difference, asserting it ran.
fn diff_stage(cwd: &Path, a: &Path, b: &Path) -> Value {
    let (stdout, stderr, ok) = run(
        cwd,
        &[
            "--local",
            "diff",
            a.to_str().expect("path a"),
            b.to_str().expect("path b"),
            "-f",
            "json",
        ],
    );
    assert!(ok, "`mcc diff` failed: {stderr}");
    let env: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    env["result"]["stage"].clone()
}

/// A `mcc diff` that is expected to be refused; returns the stderr.
fn refused(cwd: &Path, a: &Path, b: &Path) -> String {
    let (_, stderr, ok) = run(
        cwd,
        &[
            "--local",
            "diff",
            a.to_str().expect("path a"),
            b.to_str().expect("path b"),
        ],
    );
    assert!(!ok, "this pairing must be refused, not answered: {stderr}");
    stderr
}

/// The reading at one of the two `stage.<seg>` segments, as `result.stage`.
fn reading(cwd: &Path, target: &Path, seg: &str) -> Value {
    let archive = save(cwd, seg, target, &format!("read-{seg}"));
    let text = std::fs::read_to_string(&archive).expect("read the archive");
    let env: Value = serde_json::from_str(&text).expect("archive is JSON");
    env["result"]["stage"].clone()
}

/// Copy `archive` to `<cwd>/<name>.json` with one field changed.
///
/// One build cannot produce a reading of a *different* segment, or under a
/// different table, so a lock on those refusals has to hand the command a field
/// it did not write. Only the one field under test is touched; everything else
/// is a reading this binary really did produce.
fn rewrite(cwd: &Path, archive: &Path, name: &str, field: &str, value: Option<&str>) -> PathBuf {
    let text = std::fs::read_to_string(archive).expect("read the archive");
    let mut env: Value = serde_json::from_str(&text).expect("archive is JSON");
    let stage = env["result"]["stage"]
        .as_object_mut()
        .expect("`result.stage` is an object");
    match value {
        Some(v) => {
            stage.insert(field.to_string(), Value::String(v.to_string()));
        }
        None => {
            stage.remove(field);
        }
    }
    let out = cwd.join(format!("{name}.json"));
    std::fs::write(&out, serde_json::to_string(&env).expect("re-serialize")).expect("write");
    out
}

fn count(stage: &Value, word: &str) -> u64 {
    stage["counts"][word].as_u64().unwrap_or(0)
}

// ── The two kinds of operand are one reading ──

/// A saved reading does not differ from itself — design §7 phase 4 ①, asked of
/// the archive operand.
#[test]
fn a_saved_reading_does_not_differ_from_itself() {
    let run_dir = scratch("self-run");
    let target = source("self", BASE_SRC);
    let a = save(&run_dir, "viz", &target, "self");

    let stage = diff_stage(&run_dir, &a, &a);
    for word in ["remove", "add", "modify", "unaligned", "changes"] {
        assert_eq!(
            count(&stage, word),
            0,
            "`{word}` must be zero: nothing changed"
        );
    }
    assert!(
        stage["items"].as_array().expect("items").is_empty(),
        "a self-difference has no rows"
    );
    // A difference over one reading is the reading's own identity, not this
    // build's: the tokens are the archive's.
    assert_eq!(stage["world_ver"], stage["diff"]["other"]["world_ver"]);
    assert_eq!(stage["top"], "main");
}

/// A saved reading and the world it was read from are the same reading — the
/// mixed pairing, which is what "compare against the baseline I saved" is.
///
/// This is also where a float is *read back* rather than built: a metric goes to
/// decimal in the file and returns through a parser, and a parser that is off by
/// one ULP reports a change of nothing (`serde_json`'s fast float path did
/// exactly that until `float_roundtrip` was turned on — this test is what found
/// it). The assertion is on the row count, so a rounding difference anywhere in
/// the metrics family fails here rather than one field at a time.
#[test]
fn a_saved_reading_matches_the_world_it_came_from() {
    let run_dir = scratch("mixed-run");
    let target = source("mixed", BASE_SRC);
    let a = save(&run_dir, "viz", &target, "mixed");

    let stage = diff_stage(&run_dir, &a, &target);
    assert_eq!(count(&stage, "changes"), 0, "one reading, taken twice");
    assert_eq!(stage["view"], "diff.stage.viz");
    // Side B here was read by **this** build, and side A by the file — the two
    // producers agree because they are the same process, and the point is that
    // the command reports both rather than assuming either.
    assert_eq!(stage["mcc_version"], stage["diff"]["other"]["mcc_version"]);
}

/// The central claim: two archives report **the same rows** as the two worlds
/// they were read from.
///
/// Not "a similar page" — the items, the counts and the non-row block are
/// compared whole. The insertion also has to be *visible* (a non-zero `add`),
/// or the equality above would hold for a command that reported nothing at all.
#[test]
fn two_saved_readings_report_what_the_two_worlds_report() {
    let run_dir = scratch("same-run");
    let base = source("same-base", BASE_SRC);
    let inserted = source("same-inserted", INSERTED_SRC);
    let a = save(&run_dir, "viz", &base, "base");
    let b = save(&run_dir, "viz", &inserted, "inserted");

    let from_worlds = diff_stage(&run_dir, &base, &inserted);
    let from_archives = diff_stage(&run_dir, &a, &b);

    assert_eq!(
        count(&from_archives, "add"),
        7,
        "one box, its two pins, and the four wire rows of the redrawn page \
         (b3719's dedupe redraws the coincident VDD-GND runs): a difference of \
         two empty pages would satisfy every equality below"
    );
    assert_eq!(from_archives["items"], from_worlds["items"]);
    assert_eq!(from_archives["counts"], from_worlds["counts"]);
    assert_eq!(from_archives["diff"], from_worlds["diff"]);
    assert_eq!(from_archives["world_ver"], from_worlds["world_ver"]);
    assert_eq!(from_archives["top_ver"], from_worlds["top_ver"]);
}

/// A reading of another segment is refused, and the message names both.
///
/// The fixture is a real `stage.vec` reading rewritten to say `stage.viz`: one
/// build cannot write a `stage.vec` reading that claims to be `stage.viz`, and
/// the point is the *check*, not how a lying archive could arise.
#[test]
fn a_reading_of_another_segment_is_refused() {
    let run_dir = scratch("seg-run");
    let target = source("seg", BASE_SRC);
    let vec = save(&run_dir, "vec", &target, "vec");
    let lying = rewrite(&run_dir, &vec, "claims-viz", "view", Some("stage.viz"));
    let viz = save(&run_dir, "viz", &target, "viz");

    let stderr = refused(&run_dir, &lying, &viz);
    assert!(
        stderr.contains("stage.viz") && stderr.contains("stage.vec"),
        "the message must name what the operand holds and what was asked for: {stderr}"
    );
}

/// A reading aligned under another table is refused, and the message names both
/// tables.
#[test]
fn a_reading_under_another_key_table_is_refused() {
    let run_dir = scratch("table-run");
    let target = source("table", BASE_SRC);
    let a = save(&run_dir, "viz", &target, "table");
    let older = rewrite(
        &run_dir,
        &a,
        "older-table",
        "key_table",
        Some("stage.viz.keys.0"),
    );

    let stderr = refused(&run_dir, &older, &a);
    assert!(
        stderr.contains("stage.viz.keys.0") && stderr.contains("stage.viz.keys.2"),
        "the message must name both tables: {stderr}"
    );
}

/// A reading that names no table at all is refused too — and with its own
/// message, because "an older build wrote this" and "another table wrote this"
/// are different things to tell a reader.
#[test]
fn a_reading_that_names_no_key_table_is_refused() {
    let run_dir = scratch("notable-run");
    let target = source("notable", BASE_SRC);
    let a = save(&run_dir, "viz", &target, "notable");
    let silent = rewrite(&run_dir, &a, "no-table", "key_table", None);

    let stderr = refused(&run_dir, &silent, &a);
    assert!(
        stderr.contains("states no key table"),
        "a reading that says nothing about its table is a different complaint: {stderr}"
    );
    assert!(
        !stderr.contains("keys.0"),
        "and it must not invent a table it was never told: {stderr}"
    );
}

/// An envelope that is not a stage reading is refused as such, rather than
/// handed to the parser as source.
///
/// The contrast is deliberate: a JSON file that is not an *envelope* at all
/// (`{"hello": 1}`) is not recognised, falls through to the world path and
/// fails there — the two branches of "not a stage reading" are different, and
/// this lock keeps them apart.
#[test]
fn an_envelope_without_a_stage_reading_is_refused() {
    let run_dir = scratch("env-run");
    let target = source("env", BASE_SRC);
    let a = save(&run_dir, "viz", &target, "env");
    let headless = rewrite(&run_dir, &a, "no-stage", "view", None);
    // Removing `view` is not enough: the envelope must lose `result.stage`
    // itself, so the last valid reading is rewritten into a plain envelope.
    let text = std::fs::read_to_string(&headless).expect("read");
    let mut env: Value = serde_json::from_str(&text).expect("JSON");
    env["result"]
        .as_object_mut()
        .expect("result")
        .remove("stage");
    std::fs::write(&headless, serde_json::to_string(&env).expect("serialize")).expect("write");

    let stderr = refused(&run_dir, &headless, &a);
    assert!(
        stderr.contains("not a stage reading"),
        "an envelope without `result.stage` is recognisably a reading: {stderr}"
    );

    let plain = run_dir.join("plain.json");
    std::fs::write(&plain, r#"{"hello": 1}"#).expect("write plain JSON");
    let stderr = refused(&run_dir, &plain, &a);
    assert!(
        !stderr.contains("not a stage reading"),
        "a file that is not an envelope is not this command's business to judge: {stderr}"
    );
}

/// Each side's **producer** is reported, and the envelope's own identity is the
/// left operand's — including its version.
///
/// The fixture is a real reading with its `mcc_version` rewritten to a build
/// that never existed. A different producer is not a refusal (two builds may
/// differ in every way but the key table, which is the whole point of the
/// operand) — but it must be *visible*: a difference states what its rows are a
/// statement about, and asserting this build's version over another build's
/// reading would be exactly the claim an archive operand exists to avoid.
#[test]
fn each_side_reports_the_build_that_produced_it() {
    let run_dir = scratch("who-run");
    let target = source("who", BASE_SRC);
    let a = save(&run_dir, "viz", &target, "who");
    let other = rewrite(
        &run_dir,
        &a,
        "other-build",
        "mcc_version",
        Some("0.0.0.other"),
    );
    let this_build = reading(&run_dir, &target, "viz")["mcc_version"]
        .as_str()
        .expect("a version")
        .to_string();
    assert_ne!(this_build, "0.0.0.other", "the fixture must differ");

    let stage = diff_stage(&run_dir, &other, &a);
    assert_eq!(count(&stage, "changes"), 0, "the two readings are equal");
    assert_eq!(
        stage["mcc_version"], "0.0.0.other",
        "the envelope's identity is side A's"
    );
    assert_eq!(
        stage["diff"]["other"]["mcc_version"], this_build,
        "side B's producer is reported beside it"
    );
    assert_eq!(
        stage["key_table"], "stage.viz.keys.2",
        "and the table the rows were taken under is stated by the answer"
    );
}

// ── The derived rows are diffed (the law hole §6.8 named) ──

/// The hole §6.8 of the view model named, closed: an **attribution** change with
/// an **unchanged net set** is seen.
///
/// The fixture drops `EARTH`'s `@role(earth)` declaration while the connection
/// that draws it stays, so the drawing is the same drawing — same nets, same
/// pins, same groups — and only the intent family's claim list loses a member.
/// While the derived rows stood outside [`stage.viz`]'s law this read as **zero
/// changes**: `net_refs` reads pins, not claims, and a skipped class is skipped
/// by `Law::index` on both sides. The `intent` row is compared as a **set**
/// without the run-local `nid`, so the lock fails on the one real delta rather
/// than on renumbering noise.
///
/// [`stage.viz`]: mcc::stages::stage_diff::VIZ_LAW
#[test]
fn an_attribution_change_with_an_unchanged_net_set_is_seen() {
    let run_dir = scratch("claim-run");
    let base = source(
        "claim-base",
        r#"
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
    conduit EARTH @role(earth)
    conduit ESDGND @role(protective)
    CAP c1(1)
    CAP c2(1)
    c1.1 -> EARTH
    c2.1 -> ESDGND
}
"#,
    );
    // The declaration is commented out — the line keeps its place, so no
    // statement shifts and the group rows cannot churn (and a bare `conduit`
    // would not do: the conduit itself is the claim, `l1_refs` takes every one).
    // Nothing about the drawing's objects moved.
    let unclaimed = source(
        "claim-unclaimed",
        r#"
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
    // conduit EARTH @role(earth)
    conduit ESDGND @role(protective)
    CAP c1(1)
    CAP c2(1)
    c1.1 -> EARTH
    c2.1 -> ESDGND
}
"#,
    );

    let stage = diff_stage(&run_dir, &base, &unclaimed);

    let intent_rows: Vec<&Value> = stage["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|r| r["kind"] == "intent")
        .collect();
    assert_eq!(
        intent_rows.len(),
        1,
        "exactly the attribution row changed: {}",
        stage["items"]
    );
    let row = intent_rows[0];
    assert_eq!(
        row["type"], "modify",
        "the row survives, its claim list does not: {row}"
    );
    assert_eq!(row["id"], "power-intent", "{row}");
    let delta = row["delta"]["nets_set"].clone();
    assert!(!delta.is_null(), "the delta is the claim list: {row}");
    let to_members = delta["to"].as_array().expect("side B claims");
    let kept = to_members
        .iter()
        .filter(|m| delta["from"].as_array().expect("side A claims").contains(m))
        .count();
    assert_eq!(
        kept,
        to_members.len(),
        "side B's claims are the subset side A keeps — one member lost, none gained: {delta}"
    );

    // And the net set really is unchanged: no box, pin, group or net row moved.
    // This half is what makes the lock about attribution and not about drawing.
    for kind in ["box", "pin", "group", "net"] {
        let got = stage["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|r| r["kind"] == kind)
            .count();
        assert_eq!(
            got, 0,
            "`{kind}` rows must not move — the drawing did not: {row}"
        );
    }
}
