// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §7 phase four: `mcc diff <A> <B> --view stage.viz`.
//!
//! The acceptance criteria for the phase are read off here:
//!
//! 1. One source read twice differs from itself in nothing.
//! 2. A one-instance edit is reported as the instance's own items and nothing
//!    else — no pre-existing box may read as changed.
//! 3. A rebuilt world reports real changes and not "every id moved".
//!
//! What is **not** tested here is the alignment law itself — which key each class
//! aligns on, what a `modify` delta may say, how an unalignable item is reported.
//! That contract has its own file (`stage_viz_diff.rs`) and is exercised against
//! the producer directly with hand-built worlds. This file tests the **command**:
//! that it reads each operand as `show stage viz` would, that it hands the two
//! readings to that one comparison rather than re-implementing it, that the
//! answer is carried in the existing envelope, and that it never vetoes an exit
//! code (law C).
//!
//! ⚠ A CLI invocation must run in a **fresh empty directory**: `viz/project.rs`
//! writes `baseline/render_projection.md` relative to the cwd.
//!
//! Run with `--test-threads=1` — this shard takes the CLI and is not parallel-safe.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// The names the two branches of `stage.diff.nameless_net_pins` stand for.
const COUNT_WORDS: [&str; 5] = ["remove", "add", "modify", "unaligned", "changes"];

// ── Fixture plumbing ──

fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

/// A fresh, **empty** directory to run a CLI invocation in.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-diff-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Write a source into `<scratch>/<name>.mc` and return both the source path and
/// a second, empty directory to run the invocation in.
///
/// The two are kept apart because the run directory is written to (the projection
/// baseline) and the source directory is not.
fn source(name: &str, src: &str) -> (PathBuf, PathBuf) {
    let src_dir = scratch(&format!("{name}-src"));
    let run_dir = scratch(&format!("{name}-run"));
    let path = src_dir.join("circuit.mc");
    std::fs::write(&path, src).expect("write the source");
    (path, run_dir)
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

/// `mcc diff <a> <b>` from `cwd`, with `extra` appended.
fn diff(cwd: &Path, a: &Path, b: &Path, extra: &[&str]) -> (String, String, bool) {
    let mut args = vec!["--local", "diff"];
    let (as_, bs) = (a.to_str().expect("path a"), b.to_str().expect("path b"));
    args.push(as_);
    args.push(bs);
    args.extend_from_slice(extra);
    run(cwd, &args)
}

/// The `result.stage` block of a `-f json` difference, asserting it ran.
fn stage_of_diff(cwd: &Path, a: &Path, b: &Path) -> Value {
    let (stdout, stderr, ok) = diff(cwd, a, b, &["-f", "json"]);
    assert!(ok, "`mcc diff` failed: {stderr}");
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

/// The `result.stage` block of a `show stage viz` reading, asserting it ran.
fn stage_of_show(cwd: &Path, target: &Path) -> Value {
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
            target.to_str().expect("target path"),
        ],
    );
    assert!(ok, "`show stage viz` failed: {stderr}");
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

fn items(stage: &Value) -> Vec<Value> {
    stage["items"]
        .as_array()
        .expect("items is an array")
        .clone()
}

fn count(stage: &Value, word: &str) -> u64 {
    stage["counts"][word]
        .as_u64()
        .unwrap_or_else(|| panic!("no `{word}` in the counts block: {}", stage["counts"]))
}

/// The `(type, kind, id)` of every change row, in the order they were published.
fn triples(stage: &Value) -> Vec<(String, String, String)> {
    items(stage)
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

/// A difference of two readings of `src` — the same source at two paths, so the
/// two operands are two worlds and not one world read twice.
fn pair(name: &str, a_src: &str, b_src: &str) -> (PathBuf, PathBuf, PathBuf) {
    let (a, run_a) = source(&format!("{name}-a"), a_src);
    let (b, _run_b) = source(&format!("{name}-b"), b_src);
    (a, b, run_a)
}

/// The whole envelope with the one field that cannot be stable blanked out —
/// `summary.elapsed_ms` is a wall clock.
fn envelope_without_clock(stdout: &str) -> String {
    let mut envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}"));
    envelope["result"]["summary"]["elapsed_ms"] = Value::from(0);
    serde_json::to_string(&envelope).expect("the envelope re-serializes")
}

/// Rows of the text face — everything below the header and the counts line,
/// split on the documented column separator (two or more spaces) and trimmed of
/// the padding each column adds.
fn text_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .skip(1)
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

/// The header's `word number` pairs, including the `nameless a/b` one.
fn counts_line(text: &str) -> Vec<(String, String)> {
    let line = text
        .lines()
        .find(|l| l.starts_with('#'))
        .unwrap_or_else(|| panic!("no counts line in the text face:\n{text}"));
    let words: Vec<&str> = line.trim_start_matches('#').split_whitespace().collect();
    assert_eq!(
        words.len() % 2,
        0,
        "the counts line is word/number pairs: {line}"
    );
    words
        .chunks(2)
        .map(|p| (p[0].to_string(), p[1].to_string()))
        .collect()
}

// ── Acceptance ①: a reading does not differ from itself ──

/// The strongest single statement, and the one that says the two operands are
/// two worlds rather than one accumulated one: the same plate read twice has no
/// difference at all. If the second load piled onto the first, the second side
/// would be the first plus itself and the direction of every row would be an
/// artefact.
#[test]
fn a_reading_does_not_differ_from_itself() {
    let entry = hbl_entry();
    let cwd = scratch("self");
    let stage = stage_of_diff(&cwd, &entry, &entry);

    assert_eq!(stage["view"], "diff.stage.viz");
    assert_eq!(stage["top"], "main");
    assert!(items(&stage).is_empty(), "a self-difference has no rows");
    for w in COUNT_WORDS {
        assert_eq!(
            count(&stage, w),
            0,
            "`{w}` must be zero in a self-difference"
        );
    }
    assert_eq!(stage["diff"]["unaligned"], Value::Array(vec![]));

    // The stability summary is the part that would catch a comparison keyed on a
    // build-local ordinal: every box is in both readings and moved in neither.
    let st = &stage["diff"]["stability"];
    assert!(
        st["unchanged_boxes_total"].as_u64().unwrap_or(0) > 0,
        "{st}"
    );
    assert_eq!(st["unchanged_boxes_moved"], 0);
    assert_eq!(st["max_unchanged_box_delta"], 0.0);
    assert_eq!(st["route_hashes_changed"], 0);
    assert_eq!(st["locality_warning"], false);

    // And the one number that is *not* zero: pins on nets with no cross-build
    // key. It is a coverage count, one per side, not a change count — a
    // self-difference prints it non-zero on purpose.
    let nameless = stage["diff"]["nameless_net_pins"]
        .as_array()
        .expect("a pair");
    assert_eq!(nameless.len(), 2);
    assert_eq!(nameless[0], nameless[1], "both sides are the same world");
    assert!(
        nameless[0].as_u64().unwrap_or(0) > 0,
        "this plate has nameless nets; a zero here would make the pair vacuous"
    );

    let _ = std::fs::remove_dir_all(&cwd);
}

/// The same source at two paths is two token pairs and still no difference.
///
/// This is the reading that separates the two kinds of key (design §3, law A):
/// the world token folds where the source was read from, so the two readings are
/// different *worlds* — while the projection they are compared in is keyed by
/// canonical instance paths, so item for item nothing moved. A comparison that
/// had been made over the tokens would report these two as incomparable; one
/// made over build-local ordinals would report every item as changed.
#[test]
fn two_paths_holding_one_source_do_not_differ() {
    let (a, b, cwd) = pair("copies", BASE_SRC, BASE_SRC);
    let stage = stage_of_diff(&cwd, &a, &b);

    for w in COUNT_WORDS {
        assert_eq!(count(&stage, w), 0, "`{w}` must be zero: nothing changed");
    }
    assert!(
        stage["world_ver"] != stage["diff"]["other"]["world_ver"],
        "the two operands must be two worlds, or this test says nothing:\n{stage}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
}

// ── Acceptance ②: a one-instance edit is reported as the instance ──

/// One instance inserted ahead of the others is exactly its own items and
/// nothing else.
///
/// The inserted instance is *first* on purpose: its box lands at the head of the
/// instance table, so an ordinal-keyed comparison would shift every following box
/// and report the whole page as changed. The assertion that catches that is the
/// pair of negatives — no `box` or `pin` is `modify`, and the boxes that were in
/// both readings are still counted as unchanged.
#[test]
fn an_inserted_instance_is_the_only_change() {
    let (a, b, cwd) = pair("insert", BASE_SRC, INSERTED_SRC);
    let stage = stage_of_diff(&cwd, &a, &b);

    assert_eq!(count(&stage, "remove"), 0, "nothing was taken out");
    assert_eq!(count(&stage, "add"), 3, "one box and its two pins");

    let added: Vec<(String, String, String)> = triples(&stage)
        .into_iter()
        .filter(|(t, _, _)| t == "add")
        .collect();
    assert_eq!(
        added,
        vec![
            ("add".into(), "box".into(), "main.c0".into()),
            ("add".into(), "pin".into(), "main.c0.1".into()),
            ("add".into(), "pin".into(), "main.c0.2".into()),
        ],
        "the added items are the new instance's own"
    );

    // The churn assertion. A box that was in both readings may not be reported as
    // changed in any way — not as a move, not as a content change.
    for (ty, kind, id) in triples(&stage) {
        if ty == "modify" {
            assert!(
                kind != "box" && kind != "pin" && kind != "segment",
                "`{id}` was in both readings; a `{kind}` may not read as changed"
            );
        }
    }

    let st = &stage["diff"]["stability"];
    assert_eq!(
        st["unchanged_boxes_moved"], 0,
        "the box ordinal shifted but no box moved"
    );
    assert_eq!(st["max_unchanged_box_delta"], 0.0);
    assert!(
        st["unchanged_boxes_total"].as_u64().unwrap_or(0) >= 3,
        "the three pre-existing boxes must still be counted as unchanged: {st}"
    );

    // The layer's own box count is the one non-metric `modify` this edit makes,
    // and it is stated as a count rather than as a list of boxes: the layer did
    // not move, one box joined it.
    let rows = items(&stage);
    let layer: Vec<&Value> = rows
        .iter()
        .filter(|c| c["kind"] == "layer" && c["type"] == "modify")
        .collect();
    assert_eq!(layer.len(), 1, "one layer, one count: {layer:?}");
    assert_eq!(layer[0]["id"], "main");
    assert_eq!(layer[0]["delta"]["boxes"]["from"], 3);
    assert_eq!(layer[0]["delta"]["boxes"]["to"], 4);

    let _ = std::fs::remove_dir_all(&cwd);
}

/// The difference is stated **against the first operand**: the same two readings
/// swapped report the mirror image, not a different reading.
#[test]
fn the_second_operand_is_reported_against_the_first() {
    let (a, b, cwd) = pair("reverse", BASE_SRC, INSERTED_SRC);
    let forward = stage_of_diff(&cwd, &a, &b);
    let backward = stage_of_diff(&cwd, &b, &a);

    assert_eq!(count(&forward, "add"), 3);
    assert_eq!(count(&forward, "remove"), 0);
    assert_eq!(count(&backward, "add"), 0);
    assert_eq!(count(&backward, "remove"), 3);

    // The envelope's tokens name the **left** operand, which is what its items
    // are a statement about.
    assert_eq!(forward["world_ver"], backward["diff"]["other"]["world_ver"]);
    assert_eq!(forward["diff"]["other"]["world_ver"], backward["world_ver"]);

    let _ = std::fs::remove_dir_all(&cwd);
}

// ── Law B: the answer rides the existing envelope ──

/// Each side is read the way `show stage viz` reads it, and the envelope says so:
/// the difference's own token pair is the left operand's, and the right one's is
/// published beside it.
#[test]
fn the_envelope_names_both_sides_as_show_would_name_them() {
    let (a, b, cwd) = pair("tokens", BASE_SRC, INSERTED_SRC);
    let shown_a = stage_of_show(&cwd, &a);
    let shown_b = stage_of_show(&cwd, &b);
    let stage = stage_of_diff(&cwd, &a, &b);

    assert_eq!(
        stage["world_ver"], shown_a["world_ver"],
        "the reading this difference is stated against is `show`'s reading of A"
    );
    assert_eq!(stage["top_ver"], shown_a["top_ver"]);
    assert_eq!(stage["top"], shown_a["top"]);
    assert_eq!(stage["diff"]["other"]["world_ver"], shown_b["world_ver"]);
    assert_eq!(stage["diff"]["other"]["top_ver"], shown_b["top_ver"]);

    // A view name of its own, so a consumer can tell a difference from a segment
    // view without looking at which fields are present.
    assert_eq!(stage["view"], "diff.stage.viz");

    let _ = std::fs::remove_dir_all(&cwd);
}

/// The counts block counts **changes** and nothing else.
///
/// The unalignable-pin coverage is a property of the two readings, not of the
/// difference: folding it in would print a non-zero number in a block whose every
/// other member is zero for a self-difference. It rides `stage.diff` as a pair.
#[test]
fn the_counts_block_holds_only_change_counts() {
    let (a, b, cwd) = pair("counts", BASE_SRC, INSERTED_SRC);
    let stage = stage_of_diff(&cwd, &a, &b);

    let keys: Vec<&str> = stage["counts"]
        .as_object()
        .expect("the counts block is an object")
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected: Vec<&str> = COUNT_WORDS.to_vec();
    expected.sort_unstable();
    assert_eq!(
        keys, expected,
        "the vocabulary of this view is its change types"
    );

    assert_eq!(
        count(&stage, "changes") as usize,
        items(&stage).len(),
        "`changes` is the row count"
    );
    assert_eq!(
        count(&stage, "add") + count(&stage, "remove") + count(&stage, "modify"),
        count(&stage, "changes"),
        "every row is one of the three change types"
    );

    let self_diff = stage_of_diff(&cwd, &a, &a);
    for w in COUNT_WORDS {
        assert_eq!(count(&self_diff, w), 0, "`{w}` in a self-difference");
    }

    let _ = std::fs::remove_dir_all(&cwd);
}

// ── Law C: a readout does not veto an exit code ──

/// A non-empty difference is the answer, not a failure — on every face.
#[test]
fn a_non_empty_difference_still_exits_zero() {
    let (a, b, cwd) = pair("law-c", BASE_SRC, INSERTED_SRC);
    for fmt in [
        &[][..],
        &["-f", "json"][..],
        &["-f", "yaml"][..],
        &["-f", "csv"][..],
    ] {
        let (stdout, stderr, ok) = diff(&cwd, &a, &b, fmt);
        assert!(ok, "a non-empty difference failed with {fmt:?}: {stderr}");
        assert!(
            !stdout.trim().is_empty(),
            "{} printed nothing",
            fmt.join(" ")
        );
    }

    // The mirror image, so the exit code cannot be reading the direction of the
    // change as a verdict either.
    let (_, stderr, ok) = diff(&cwd, &b, &a, &[]);
    assert!(ok, "the reverse direction failed: {stderr}");

    let _ = std::fs::remove_dir_all(&cwd);
}

/// An unknown `--view` is refused **before** either operand is read.
///
/// The flag's value domain is a closed set, not a free string: a name no key
/// table covers would silently compare nothing and report no differences, which
/// is indistinguishable from two identical worlds. Until the other segments have
/// per-class key tables of their own, the closed set has one member.
#[test]
fn an_unknown_view_is_refused() {
    let (a, b, cwd) = pair("view", BASE_SRC, INSERTED_SRC);
    let (stdout, stderr, ok) = diff(&cwd, &a, &b, &["--view", "stage.p2"]);

    assert!(!ok, "an unknown view must not run");
    assert!(stdout.is_empty(), "nothing may be published: {stdout}");
    assert!(
        stderr.contains("stage.viz"),
        "the refusal must name what is accepted: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
}

// ── The two faces of one answer ──

/// The text face and the JSON face are one answer written twice: the counts line
/// is the counts block, and the rows are the items.
#[test]
fn the_text_face_reads_the_same_answer_as_the_json_face() {
    let (a, b, cwd) = pair("faces", BASE_SRC, INSERTED_SRC);
    let stage = stage_of_diff(&cwd, &a, &b);
    let (text, stderr, ok) = diff(&cwd, &a, &b, &[]);
    assert!(ok, "the text face failed: {stderr}");

    let head = text.lines().next().expect("a header line");
    assert!(
        head.starts_with("diff.stage.viz "),
        "the header names the view: {head}"
    );
    assert!(
        head.contains(&stage["world_ver"].as_str().unwrap_or("-")),
        "the header names the left world: {head}"
    );
    assert!(
        head.contains(&stage["diff"]["other"]["world_ver"].as_str().unwrap_or("-")),
        "the header names the right world: {head}"
    );

    let pairs = counts_line(&text);
    for w in COUNT_WORDS {
        let printed = pairs
            .iter()
            .find(|(word, _)| word == w)
            .unwrap_or_else(|| panic!("no `{w}` in the counts line: {text}"));
        assert_eq!(
            printed.1,
            count(&stage, w).to_string(),
            "the text face and the envelope disagree about `{w}`"
        );
    }

    // The rows: one per change, one per unalignable item, four columns each.
    let rows = text_rows(&text);
    assert_eq!(
        rows.len() as u64,
        count(&stage, "changes") + stage["diff"]["unaligned"].as_array().unwrap().len() as u64,
        "the text face prints a row per change:\n{text}"
    );
    for row in &rows {
        assert_eq!(row.len(), 4, "a row is type, kind, id, delta: {row:?}");
        assert!(
            COUNT_WORDS.contains(&row[0].as_str()),
            "a row opens with a change type: {row:?}"
        );
    }

    // The prohibitions every readout in this family obeys.
    assert!(!text.contains('\u{1b}'), "no ANSI escapes");
    assert!(!text.contains('\t'), "no tab-delimited columns");

    let _ = std::fs::remove_dir_all(&cwd);
}

/// `-o` redirects the text face into a file and prints nothing.
#[test]
fn the_output_flag_redirects_the_text_face() {
    let (a, b, cwd) = pair("output", BASE_SRC, INSERTED_SRC);
    let (stdout, stderr, ok) = diff(&cwd, &a, &b, &[]);
    assert!(ok, "the stdout run failed: {stderr}");

    let path = cwd.join("difference.txt");
    let (redirected, stderr, ok) = diff(&cwd, &a, &b, &["-o", path.to_str().unwrap()]);
    assert!(ok, "the redirected run failed: {stderr}");
    assert!(
        redirected.is_empty(),
        "nothing may reach stdout as well: {redirected}"
    );

    // Byte for byte, including the trailing newline: `-o` moves the same bytes
    // from the stream to the file rather than rendering a second time.
    let written = std::fs::read_to_string(&path).expect("the file was written");
    assert_eq!(
        written, stdout,
        "the file holds the text face and nothing else"
    );

    let _ = std::fs::remove_dir_all(&cwd);
}

/// Two runs of one difference agree, byte for byte apart from the wall clock.
#[test]
fn two_runs_of_one_difference_agree() {
    let (a, b, cwd) = pair("stable", BASE_SRC, INSERTED_SRC);
    let (first, ea, oka) = diff(&cwd, &a, &b, &["-f", "json"]);
    let (second, eb, okb) = diff(&cwd, &a, &b, &["-f", "json"]);
    assert!(oka && okb, "`mcc diff` failed: {ea}{eb}");
    assert_eq!(
        envelope_without_clock(&first),
        envelope_without_clock(&second),
        "`elapsed_ms` must be the only field that differs"
    );

    let (t1, _, _) = diff(&cwd, &a, &b, &[]);
    let (t2, _, _) = diff(&cwd, &a, &b, &[]);
    assert_eq!(t1, t2, "the text face carries no measurement of its own");

    let _ = std::fs::remove_dir_all(&cwd);
}
