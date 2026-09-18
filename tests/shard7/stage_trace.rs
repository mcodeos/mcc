// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §7 phase three acceptance: `mcc trace <KEY>` (batch 3c).
//!
//! `join` walks one hop per invocation and answers "what happened to this
//! statement's rows". `trace` answers the other question a reader actually has —
//! "here is one thing I can point at, where did it come from and where did it
//! go" — by following a single key along the whole chain at once. The chain is
//! read top to bottom, so the output is one row per stage, in chain order.
//!
//! Three properties are the command, and this file asserts each of them:
//!
//! 1. **The key's form is read off the key.** Four forms, no `--kind`, and no
//!    fifth (§0.2 corollary 2). The forms overlap in their characters — a def key
//!    `lib/power.mc::LDO` is dotted like a path, a position `mcu.mc:23` carries a
//!    colon like a point handle — so the discrimination is worth pinning as a
//!    unit, not only end to end.
//! 2. **Coming in by any form lands on the same object.** A source position, an
//!    in-domain handle and a canonical path are three ways to say one thing; the
//!    test drives all three at one object and requires the walks to agree row for
//!    row. A fourth, the def key, is deliberately *not* one of those ways: a def
//!    is a template, and the readout says so rather than inventing a handle for
//!    it.
//! 3. **Where a walk stops, it says which stage stopped it.** A statement whose
//!    rows the circuit never took reports the class `drop` at `p2` and `-` beyond
//!    it — and exits 0. A readout is not a verdict (law C); only a key that names
//!    nothing at all, which is an unhonourable argument rather than a reading,
//!    fails.
//!
//! Plus the invariants every readout of this family owes: two runs
//! byte-identical, one `items` behind both faces, the class it prints being the
//! class `join` prints for the same object (§5.3 ruling 3), and off-hop objects
//! named as such instead of being made to look lost.
//!
//! ⚠ **A class may not be reported by a name match.** [`two_statements_alike…`]
//! reuses `stage_join.rs`'s byte-identical fixture: the two `c2.Cap([VDD, GND])`
//! statements must each trace to their own line, both `drop`, and neither may
//! borrow the rows of the instance the name does resolve to.
//!
//! ⚠ **What is not asserted, and why.** `stage.*` between two builds may differ
//! in the wall-clock field of the envelope, so "byte-identical" is a claim about
//! the projection, never about the envelope (§5.3 O15). The two-run test splits
//! the claim accordingly instead of quietly weakening it.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The stage names of the chain, in order. Every trace prints exactly these
/// rows, one per stage, whether or not that stage had anything to say — a stage
/// that is missing from the output cannot be told from one that was not walked.
const STAGES: &[&str] = &["src", "ast", "p2", "vec", "viz"];

/// Two statements spelled byte for byte alike, at two positions, plus a control
/// statement that is fully taken.
///
/// The second `CAP c2(…)` declaration is what makes the two `c2.Cap(…)` lines
/// produce no rows; a walk that fell back to matching by name would hand them
/// the rows of something else and report a `carry`. Positions are derived from
/// this very text by [`line_of`], so inserting a line above cannot silently point
/// an assertion at the wrong statement.
const TWICE_SRC: &str = r#"
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
    c1.Cap([VDD, GND])
    CAP c2(1)
    CAP c2(2)
    c2.Cap([VDD, GND])
    c2.Cap([VDD, GND])
}
"#;

// ── Fixture plumbing ──

/// The entry of the real seven-layer project, which most of this file reads.
fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

/// A sibling of the entry, for the cases that need a file other than the top
/// one (a wide fan lives in a component's body, not in the top module).
fn hbl_sibling(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hbl/src")
        .join(name)
}

/// A fresh, **empty** directory to run a CLI invocation in.
///
/// Empty on purpose: the readout must not depend on where it is run, and a stray
/// file written into the fixture tree would be mistaken for a product.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-trace-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Write `src` into a fresh scratch directory and return the file's path.
fn write_fixture(name: &str, src: &str) -> PathBuf {
    let dir = scratch(name);
    let path = dir.join("main.mc");
    std::fs::write(&path, src).expect("write the fixture source");
    path
}

/// 1-based line of the `nth` occurrence of `needle`, derived from the source
/// rather than written down.
fn line_of(src: &str, needle: &str, nth: usize) -> usize {
    let mut seen = 0;
    for (i, line) in src.lines().enumerate() {
        if line.contains(needle) {
            seen += 1;
            if seen == nth {
                return i + 1;
            }
        }
    }
    panic!("`{needle}` occurrence #{nth} is not in the fixture");
}

/// Run `mcc trace <key> …` from `cwd` and return `(stdout, stderr, ok)`.
///
/// `--local` because the readout is local-only: with an `mcc start` service
/// running, a delegated invocation prints nothing at all (§5.3 last ⚠ — the same
/// trap `show stage p2` has).
fn run_trace_in(cwd: &Path, entry: &Path, key: &str, extra: &[&str]) -> (String, String, bool) {
    let mut args: Vec<&str> = vec!["--local", "trace", key];
    args.extend_from_slice(extra);
    args.push("-F");
    args.push(entry.to_str().expect("fixture path"));
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&args)
        .output()
        .expect("run mcc trace");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// The JSON projection of a trace that is expected to succeed.
fn trace_of(cwd: &Path, entry: &Path, key: &str, extra: &[&str]) -> Value {
    let mut args: Vec<&str> = vec!["-f", "json"];
    args.extend_from_slice(extra);
    let (stdout, stderr, ok) = run_trace_in(cwd, entry, key, &args);
    assert!(ok, "`trace {key}` failed: {stderr}");
    stage_of(&stdout)
}

/// The text face of a trace that is expected to succeed.
fn trace_text_of(cwd: &Path, entry: &Path, key: &str) -> String {
    let (stdout, _, ok) = run_trace_in(cwd, entry, key, &[]);
    assert!(ok, "`trace {key}` (text) failed");
    stdout
}

/// Run `mcc show stage <seg> -f json` from `cwd` — one segment's own readout,
/// which is what a key has to be a key *of*.
fn run_show_stage_in(cwd: &Path, entry: &Path, seg: &str) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(["--local", "show", "stage", seg, "-f", "json", "-F"])
        .arg(entry)
        .output()
        .expect("run mcc show stage");
    assert!(
        out.status.success(),
        "`show stage {seg}` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    stage_of(&String::from_utf8_lossy(&out.stdout))
}

/// Run `mcc join <a> <b> -f json` from `cwd` — the hop readout whose classes a
/// trace has to agree with (§5.3 ruling 3).
fn run_join_in(cwd: &Path, entry: &Path, a: &str, b: &str) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(["--local", "join", a, b, "-f", "json", "-F"])
        .arg(entry)
        .output()
        .expect("run mcc join");
    assert!(
        out.status.success(),
        "`join {a} {b}` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    stage_of(&String::from_utf8_lossy(&out.stdout))
}

fn stage_of(stdout: &str) -> Value {
    let envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

/// The one row of the walk that belongs to `stage`.
///
/// Panics if there is no such row: every stage of the chain is printed on every
/// run, so a missing row is a defect in the readout, not an absence of news.
fn row_at<'a>(view: &'a Value, stage: &str) -> &'a Value {
    view["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .find(|i| i["stage"] == stage)
        .unwrap_or_else(|| panic!("the walk printed no `{stage}` row"))
}

/// The stages the walk printed, in the order it printed them.
fn stages_of(view: &Value) -> Vec<String> {
    view["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|i| i["stage"].as_str().unwrap_or("-").to_string())
        .collect()
}

/// The `.class` of one stage's row, or `"-"` when the row carries none.
fn class_at(view: &Value, stage: &str) -> String {
    row_at(view, stage)["class"]
        .as_str()
        .unwrap_or("-")
        .to_string()
}

/// The `.key` of one stage's row as `-`-or-string, for the same reason.
fn key_at(view: &Value, stage: &str) -> String {
    match row_at(view, stage)["key"].as_str() {
        Some(k) => k.to_string(),
        None => "-".to_string(),
    }
}

/// The `.why` note of one stage's row.
fn why_at(view: &Value, stage: &str) -> String {
    row_at(view, stage)["why"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn count(view: &Value, key: &str) -> u64 {
    view["counts"][key]
        .as_u64()
        .unwrap_or_else(|| panic!("`{key}` is not a number in {}", view["counts"]))
}

/// `uri:line` as the readouts spell a source position, for matching one
/// command's row against another's.
fn loc_key(item: &Value) -> String {
    let uri = item["loc"]["uri"].as_str().unwrap_or("<none>");
    let line = item["loc"]["line"].as_u64().unwrap_or(0);
    format!("{uri}:{line}")
}

/// The projection of one run, re-serialized so two runs compare as bytes.
fn projection_bytes(stdout: &str) -> String {
    serde_json::to_string(&stage_of(stdout)).expect("the projection serializes")
}

/// The envelope with the one field that **cannot** be stable blanked out:
/// `summary.elapsed_ms` is a wall clock, so the envelope as a whole is not a
/// byte-stable artifact and no acceptance may ask it to be. Splitting the claim
/// into "the projection is identical" and "the clock is the only thing that
/// moves" is what keeps the first honest (§5.3 O15).
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

/// The cells of the text face's row for `stage`: the non-`#` line whose first
/// cell is that stage name, split on the column separator.
fn text_row(text: &str, stage: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            l.split("  ")
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .find(|cells| cells.first().map(String::as_str) == Some(stage))
        .unwrap_or_else(|| panic!("the text face has no `{stage}` row:\n{text}"))
}

// ── Determinism: two runs, byte for byte, on every face ──

#[test]
fn two_runs_are_byte_identical_on_every_face() {
    let first = scratch("det-a");
    let second = scratch("det-b");
    let entry = hbl_entry();
    // A key whose walk reaches all five stages: a walk that stopped early would
    // leave most of the projection untested for stability.
    let key = format!("{}:19", entry.display());

    let (a_json, _, ok) = run_trace_in(&first, &entry, &key, &["-f", "json"]);
    assert!(ok, "trace -f json failed");
    let (b_json, _, _) = run_trace_in(&second, &entry, &key, &["-f", "json"]);
    assert_eq!(
        projection_bytes(&a_json),
        projection_bytes(&b_json),
        "the projection moved between two runs"
    );
    // And the only part of the envelope allowed to move is the clock.
    assert_eq!(
        envelope_without_clock(&a_json),
        envelope_without_clock(&b_json),
        "something other than the clock moved between two runs"
    );

    let (a_pretty, _, _) = run_trace_in(&first, &entry, &key, &["-f", "json-pretty"]);
    let (b_pretty, _, _) = run_trace_in(&second, &entry, &key, &["-f", "json-pretty"]);
    assert_eq!(projection_bytes(&a_pretty), projection_bytes(&b_pretty));

    let (a_yaml, _, _) = run_trace_in(&first, &entry, &key, &["-f", "yaml"]);
    let (b_yaml, _, _) = run_trace_in(&second, &entry, &key, &["-f", "yaml"]);
    assert_eq!(yaml_without_clock(&a_yaml), yaml_without_clock(&b_yaml));

    let (a_text, _, _) = run_trace_in(&first, &entry, &key, &[]);
    let (b_text, _, _) = run_trace_in(&second, &entry, &key, &[]);
    assert_eq!(a_text, b_text, "the text face moved between two runs");
    assert!(a_text.contains("# trace"), "the text face lost its header");
}

#[test]
fn the_walk_always_prints_every_stage_in_chain_order() {
    let cwd = scratch("order");
    let entry = hbl_entry();
    let view = trace_of(&cwd, &entry, &format!("{}:19", entry.display()), &[]);
    assert_eq!(
        stages_of(&view),
        STAGES.to_vec(),
        "the walk is read top to bottom, so the rows are the chain in order"
    );
    // Every stage is printed; how many *reached* the key is a separate number, so
    // a walk that stopped early cannot be read as a chain that was shorter.
    assert_eq!(count(&view, "stages"), STAGES.len() as u64);
    assert_eq!(
        count(&view, "reached"),
        3,
        "this key is named at src, ast and p2 and nowhere past that"
    );
    assert!(count(&view, "reached") <= count(&view, "stages"));
    assert_eq!(
        view["view"], "trace",
        "the view names itself; a caller keys off this string"
    );
}

// ── The key's form is read off the key (§5.3 ③, §0.2 corollary 2) ──

#[test]
fn the_four_key_forms_are_told_apart_by_content_alone() {
    use mcc::stages::trace::{parse_key, KeyForm};

    // Each form, and the word it prints.
    assert_eq!(parse_key("N12:3"), Some(KeyForm::Domain));
    assert_eq!(parse_key("N12"), Some(KeyForm::Domain));
    assert_eq!(parse_key("top.u1.vin"), Some(KeyForm::Canon));
    assert_eq!(parse_key("lib/power.mc::LDO"), Some(KeyForm::Def));
    assert_eq!(parse_key("mcu.mc:23"), Some(KeyForm::Loc));
    assert_eq!(parse_key("mcu.mc:23:7"), Some(KeyForm::Loc));

    // The overlaps the ordering exists for: a def key is dotted like a path, and
    // a position carries a colon like a point handle. Each look-alike has to land
    // on the form its punctuation can only be.
    assert_eq!(parse_key("lib/power.mc::LDO"), Some(KeyForm::Def));
    assert_ne!(parse_key("lib/power.mc::LDO"), Some(KeyForm::Canon));
    assert_eq!(parse_key("mcu.mc:23"), Some(KeyForm::Loc));
    assert_ne!(parse_key("mcu.mc:23"), Some(KeyForm::Domain));

    // And the near misses that must not be read as a form they only resemble:
    // `N` followed by digits is a node, `N` followed by anything else is a path.
    assert_eq!(parse_key("Nabc"), Some(KeyForm::Canon));
    assert_eq!(parse_key("N12:x"), Some(KeyForm::Canon));
    assert_eq!(parse_key("mcu.mc:x"), Some(KeyForm::Canon));

    // Every form has a distinct word: a caller keys off the word, so two forms
    // sharing one would make the header ambiguous.
    let words = [
        KeyForm::Domain.as_str(),
        KeyForm::Canon.as_str(),
        KeyForm::Def.as_str(),
        KeyForm::Loc.as_str(),
    ];
    let unique: std::collections::BTreeSet<&str> = words.iter().copied().collect();
    assert_eq!(unique.len(), words.len(), "two forms share a word");

    // An empty key is no key at all — not the canonical form, which is what
    // falls through when nothing else matches.
    assert_eq!(parse_key(""), None);
}

#[test]
fn the_form_the_readout_reports_is_the_form_it_parsed() {
    let cwd = scratch("form");
    let entry = hbl_entry();
    let cases: [(&str, &str); 4] = [
        ("N1:0", "domain"),
        ("main.V1V2", "canon"),
        (&format!("{}::main", entry.display()), "def"),
        (&format!("{}:19", entry.display()), "loc"),
    ];
    for (key, form) in cases {
        let view = trace_of(&cwd, &entry, key, &[]);
        assert_eq!(
            view["counts"]["form"], form,
            "`{key}` was not reported as a `{form}` key"
        );
    }
}

// ── Any form lands on the same object ──

#[test]
fn three_forms_reach_one_object_and_walk_it_identically() {
    let cwd = scratch("agree");
    let entry = hbl_entry();
    // `main.V1V2` is the point the statement at hbl.mc:19 writes. Its in-domain
    // handle is `N1:0`, and its source position is the line it is written on —
    // three ways to name one thing, so three walks that must agree.
    let by_domain = trace_of(&cwd, &entry, "N1:0", &[]);
    let by_canon = trace_of(&cwd, &entry, "main.V1V2", &[]);
    let by_loc = trace_of(&cwd, &entry, &format!("{}:19", entry.display()), &[]);

    for other in [&by_canon, &by_loc] {
        assert_eq!(
            serde_json::to_string(&by_domain["items"]).unwrap(),
            serde_json::to_string(&other["items"]).unwrap(),
            "two forms of one object walked it differently"
        );
    }
    // And the object they agree on is the one the canonical path names: the walk
    // is keyed on identity, never on the spelling it was entered by.
    assert_eq!(key_at(&by_domain, "p2"), "N1:0");
    assert_eq!(row_at(&by_domain, "src")["loc"]["line"], 19);
}

// ── A position resolves to the statement it names, not to its neighbour ──

#[test]
fn a_source_position_resolves_to_the_statement_it_names() {
    let cwd = scratch("pos");
    let entry = hbl_entry();
    let src = std::fs::read_to_string(&entry).expect("read the entry source");
    // A clause's `end` offset is the byte after its last one, which the newline
    // before the next statement puts on the next line — so a position test
    // written on lines rather than on bytes admits the statement on the *next*
    // line as well, and a lookup that takes the first match hands back the
    // wrong one. These two statements sit on adjacent lines, which is what makes
    // the mistake observable: the second one's line must resolve to the second
    // one's text, not to its neighbour's.
    let first = line_of(&src, "FLASH.GD25Q32E", 1);
    let second = line_of(&src, "MCU513.i2c().loadFlash", 1);
    assert_eq!(second, first + 1, "this lock needs the two lines adjacent");

    for (line, needle) in [(first, "FLASH.GD25Q32E"), (second, "MCU513.i2c()")] {
        let view = trace_of(&cwd, &entry, &format!("{}:{line}", entry.display()), &[]);
        assert_eq!(
            row_at(&view, "src")["loc"]["line"],
            line as u64,
            "the position resolved to another statement's line"
        );
        let detail = row_at(&view, "src")["detail"].as_str().unwrap_or("");
        assert!(
            detail.contains(needle),
            "line {line} resolved to `{detail}`, which is not the statement there"
        );
    }
}

// ── Where a walk stops, and that stopping is not a verdict ──

#[test]
fn a_statement_the_circuit_never_took_reports_which_stage_stopped_it() {
    let cwd = scratch("stop");
    let src = TWICE_SRC;
    let entry = write_fixture("stopped", src);
    let line = line_of(src, "c2.Cap([VDD, GND])", 1);

    let (stdout, stderr, ok) = run_trace_in(
        &cwd,
        &entry,
        &format!("{}:{line}", entry.display()),
        &["-f", "json"],
    );
    assert!(
        ok,
        "a readout never vetoes an exit code (law C), even for a dropped statement: {stderr}"
    );
    let view = stage_of(&stdout);

    // The statement is there at the head of the chain …
    assert_eq!(row_at(&view, "src")["loc"]["line"], line as u64);
    assert!(row_at(&view, "ast")["key"]
        .as_str()
        .unwrap_or("")
        .starts_with("phrase#"));
    // … it is reported as dropped at the hop that dropped it …
    assert_eq!(class_at(&view, "p2"), "drop");
    assert_eq!(
        count(&view, "reached"),
        2,
        "the walk reached the source and the hop that dropped it, and no further"
    );
    // … and beyond that hop there is nothing to print but the zero.
    for stage in ["vec", "viz"] {
        assert_eq!(class_at(&view, stage), "-");
        assert_eq!(key_at(&view, stage), "-");
        assert!(
            why_at(&view, stage).contains("0 hop row(s)"),
            "a blank row must say it is a zero, not look unrendered"
        );
    }

    // The control: the same construct that *is* taken walks the whole chain, so
    // "everything stops at p2" cannot pass this test.
    let control = line_of(src, "CAP c1(1)", 1);
    let view = trace_of(&cwd, &entry, &format!("{}:{control}", entry.display()), &[]);
    assert_eq!(class_at(&view, "p2"), "carry");
    assert_eq!(class_at(&view, "vec"), "carry");
    assert_eq!(class_at(&view, "viz"), "carry");
    assert_eq!(count(&view, "reached"), 5);
    assert_eq!(
        key_at(&view, "p2"),
        key_at(&view, "viz"),
        "a carried object keeps its key along the chain"
    );
}

#[test]
fn two_statements_alike_at_two_positions_keep_their_own_rows() {
    let cwd = scratch("twice");
    let src = TWICE_SRC;
    let entry = write_fixture("twice", src);
    let first = line_of(src, "c2.Cap([VDD, GND])", 1);
    let second = line_of(src, "c2.Cap([VDD, GND])", 2);
    assert_ne!(first, second);

    let a = trace_of(&cwd, &entry, &format!("{}:{first}", entry.display()), &[]);
    let b = trace_of(&cwd, &entry, &format!("{}:{second}", entry.display()), &[]);

    // Each position traced its own statement: the source row differs, so a walk
    // that answered by spelling would have produced two identical rows here.
    assert_eq!(row_at(&a, "src")["loc"]["line"], first as u64);
    assert_eq!(row_at(&b, "src")["loc"]["line"], second as u64);
    assert_ne!(
        row_at(&a, "ast")["key"],
        row_at(&b, "ast")["key"],
        "two positions, one ordinal: the derived index is not per-statement"
    );

    // And both are mismatches. The instance the name *does* resolve to, `c2`, is
    // declared twice in this fixture; a lookup that fell back to the name would
    // have handed these statements `c2`'s rows and reported a `carry`.
    for view in [&a, &b] {
        assert_eq!(class_at(view, "p2"), "drop");
        assert_eq!(key_at(view, "p2"), "-");
        assert_eq!(count(view, "reached"), 2);
    }
}

// ── A def is a template, and the walk says so ──

#[test]
fn a_def_key_lists_its_instances_without_inventing_a_handle() {
    let cwd = scratch("def");
    let src = TWICE_SRC;
    let entry = write_fixture("def", src);
    // A def key, spelled with a path relative to the run directory: the key
    // names a file, and the readout has to recognise that file as the one it
    // loaded rather than refusing on the spelling.
    let view = trace_of(&cwd, &entry, "main.mc::CAP", &[]);

    assert_eq!(view["counts"]["form"], "def", "the def form reports itself");
    // A def is not a statement, and no stage of the chain walks a template; the
    // two source-side rows say which kind of thing they were asked about.
    for stage in ["src", "ast"] {
        assert_eq!(class_at(&view, stage), "-");
        assert_eq!(key_at(&view, stage), "-");
        assert!(
            why_at(&view, stage).contains("definition"),
            "the row must say a def is not a statement: {}",
            why_at(&view, stage)
        );
    }
    // The objects it names are the ones the def body produced — every key here
    // is a real object of `stage.p2`, so the fan is a list of instances and not
    // a handle the readout made up for the def itself.
    let keys = key_at(&view, "p2");
    assert!(
        !keys.is_empty() && keys != "-",
        "the def resolved to nothing"
    );
    let keys: Vec<&str> = keys.split_whitespace().collect();
    assert!(keys.len() >= 2, "the fixture declares two instances of CAP");
    let p2 = run_show_stage_in(&cwd, &entry, "p2");
    // The keys of `stage.p2` as that view itself lists them. Taking them from
    // the hop instead would be the weaker check: a hop only names the objects it
    // matched, so a key it never mentioned would pass for "real".
    let known: std::collections::BTreeSet<String> = p2["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["key"].as_str().map(str::to_string))
        .collect();
    for k in &keys {
        assert!(
            known.contains(*k),
            "`{k}` is not a key of stage.p2, so the walk invented it"
        );
    }
    // The def's own fan stops at p2: no object of the vec / viz stage carries
    // the def as a key, and printing one would be exactly the invented handle.
    for stage in ["vec", "viz"] {
        assert_eq!(key_at(&view, stage), "-");
        assert!(why_at(&view, stage).contains("template"));
    }
}

// ── One items, one class: trace prints what join prints ──

#[test]
fn the_class_a_walk_prints_is_the_class_the_hop_prints() {
    let cwd = scratch("agree-class");
    let entry = hbl_entry();
    let src = std::fs::read_to_string(&entry).expect("read the entry source");

    // The first hop's own readout, indexed by the position it attributed to each
    // of its rows.
    let p2 = run_join_in(&cwd, &entry, "src", "p2");
    let mut by_loc: BTreeMap<String, String> = BTreeMap::new();
    for item in p2["items"].as_array().expect("items") {
        if let Some(class) = item["class"].as_str() {
            by_loc.insert(loc_key(item), class.to_string());
        }
    }

    // A `carry`, an `expand` and a `drop`: one position per class, so a walk
    // that hard-coded a class, or read a different build, fails here.
    let wanted: [(&str, &str); 3] = [
        ("FLASH.GD25Q32E FLASH(V3V3)", "carry"),
        ("V5V -> LDO", "expand"),
        ("MIC(V3V3).MIC ->", "drop"),
    ];
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (needle, class) in wanted {
        let line = line_of(&src, needle, 1);
        let key = format!("{}:{line}", entry.display());
        let hop = by_loc
            .get(&key)
            .unwrap_or_else(|| panic!("the hop attributed nothing to {key}"));
        assert_eq!(
            hop, class,
            "the fixture does not spell `{needle}` as a {class}"
        );
        let view = trace_of(&cwd, &entry, &key, &[]);
        assert_eq!(
            class_at(&view, "p2"),
            *class,
            "`{needle}`: trace and join disagree on the class of {key}"
        );
        *seen.entry(class.to_string()).or_insert(0) += 1;
    }
    assert_eq!(
        seen.values().sum::<usize>(),
        3,
        "each of the three classes must be exercised, not skipped"
    );
}

// ── Off-hop objects are named as such, never left looking lost ──

#[test]
fn an_off_hop_object_says_so_instead_of_looking_lost() {
    let cwd = scratch("offhop");
    let entry = hbl_entry();
    // A bus and a label: objects of `stage.p2` that hold no join key, because no
    // hop matches on them. Their walk must say that in words.
    for key in ["main.MCU513.UC.XTAL", "main.GND"] {
        let view = trace_of(&cwd, &entry, key, &[]);
        assert_eq!(
            key_at(&view, "p2"),
            key,
            "an off-hop object still prints the path it was found by"
        );
        let why = why_at(&view, "p2");
        assert!(
            why.contains("off-hop"),
            "`{key}`: the row must say why no hop named it, not print a bare zero: {why}"
        );
        // The class is the view's own class for the object, not a hop's word.
        let class = class_at(&view, "p2");
        assert!(
            class == "bus" || class == "label",
            "`{key}` is a {class}, which is not a class of this hop"
        );
        // And the source-side rows say which question could not be asked.
        assert!(why_at(&view, "src").contains("no join key"));
    }
}

// ── The text face marks what it did not print ──

#[test]
fn the_key_column_marks_what_it_did_not_print() {
    let cwd = scratch("wide");
    let entry = hbl_entry();
    // One statement whose rows are many: a clause in a component body fanning
    // out to more objects than a fixed-width column can carry. The line is read
    // from the sibling's own text, so the fixture moving cannot leave this
    // assertion pointing at a different clause.
    let wide = hbl_sibling("us513.mc");
    let src = std::fs::read_to_string(&wide).expect("read the sibling source");
    let line = line_of(&src, "UC.6 -> CAP", 1);
    let key = format!("{}:{line}", wide.display());

    let text = trace_text_of(&cwd, &entry, &key);
    let cells = text_row(&text, "p2");
    let cell = cells
        .get(2)
        .unwrap_or_else(|| panic!("no key cell: {cells:?}"));
    assert!(
        cell.contains("more"),
        "a key cell that was cut short must say so: {cell}"
    );
    let shown: usize = cell
        .rsplit_once('+')
        .and_then(|(_, n)| n.trim_end_matches(" more").parse().ok())
        .unwrap_or_else(|| panic!("the marker does not carry a count: {cell}"));
    assert!(shown > 0, "the marker counted nothing: {cell}");

    // The JSON face carries the whole list, so the elision is a rendering
    // choice and never a loss of the reading. The text face prints a prefix of
    // it, so everything the text face did print has to be that prefix: a marker
    // that counted a different set from the one it printed would be a lie about
    // the same data.
    let view = trace_of(&cwd, &entry, &key, &[]);
    let all = key_at(&view, "p2");
    let keys: Vec<&str> = all.split_whitespace().collect();
    let printed = keys
        .len()
        .checked_sub(shown)
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            panic!("the text face elided nothing, so it does not exercise the marker: {cell}")
        });
    assert!(
        !all.contains("more"),
        "the JSON face must not carry the text face's marker: {all}"
    );
    assert_eq!(
        cell.split(" +").next().unwrap_or(""),
        keys[..printed].join(" "),
        "the text face printed something other than the prefix of the key list"
    );
}

// ── An argument that cannot be honoured fails; a reading never does ──

#[test]
fn a_key_that_names_nothing_fails_loudly_instead_of_reading_as_empty() {
    let cwd = scratch("fail");
    let entry = hbl_entry();
    let cases: [(&str, &str); 4] = [
        ("", "form"),
        ("V1V2", "object"),
        (&format!("{}:1", entry.display()), "spans"),
        ("N99:9", "object"),
    ];
    for (key, needle) in cases {
        let (stdout, stderr, ok) = run_trace_in(&cwd, &entry, key, &[]);
        assert!(!ok, "`trace {key}` should not have succeeded");
        assert!(
            stdout.is_empty(),
            "`trace {key}` wrote a reading to stdout while failing: {stdout}"
        );
        assert!(
            stderr.contains(needle),
            "`trace {key}`: the message does not say why it cannot be honoured: {stderr}"
        );
    }

    // The boundary this file's other tests lean on: a position outside every
    // statement is an argument error, and a position inside one is not.
    let src = std::fs::read_to_string(&entry).expect("read the entry source");
    let good = line_of(&src, "V5V -> LDO", 1);
    let (_, _, ok) = run_trace_in(&cwd, &entry, &format!("{}:{good}", entry.display()), &[]);
    assert!(ok, "a position inside a statement must be honoured");
}
