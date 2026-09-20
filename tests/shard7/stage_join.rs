// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §7 phase three acceptance: `mcc join`, all three hops.
//!
//! **Batches 3a** (`src -> p2`) and **3b** (`p2 -> vec`, `vec -> viz`). The first
//! hop matches *statements* against the rows they wrote; the two inner hops match
//! *objects* against each other on the key §2.4 gives each kind, and where a kind
//! has no key they match on the criterion that kind has instead — a net's member
//! set (O10), a path's ordered pair of ends (O9). A key's objects form one
//! component, and the design's card is read off that component's two sides:
//! (1,1) `carry`, (1,N) `expand`, (N,1) `merge`, (1,0) `drop`, (0,1) `synth`. A
//! component with several objects on *both* sides is none of those, and it is
//! reported as the diagnostic state rather than paired by class or by name.
//!
//! Phase three's row in the landing table asks for three constructions, and this
//! file asserts all three:
//!
//! 1. **A known loss must name the segment that lost it.** A statement the
//!    parser took (`ast`) and Pass 1 recorded, but which no Pass-2 row falls
//!    inside, is a `drop` — and it must land on the **second** sub-hop counter
//!    while the first stays at zero. The asymmetry *is* the assertion: one
//!    combined "mismatch" number could not tell "the parser dropped it" from
//!    "Pass 2 never wrote it", and those two faults have different owners.
//! 2. **A mismatch is not filled in by name.** Two statements spelled byte for
//!    byte alike at two positions must be reported as two mismatches, never as
//!    one name-matched `carry` — the key is the source position, not the
//!    spelling (§5.2 hard constraint 2 / §2.2 (1)).
//! 3. **`skip` is never reported as `drop`.** `use`, `io` / `out` and the pin
//!    table of a component definition take no part in the model by AST node
//!    kind, so they are `skip`; the `drop` count must not include them.
//!
//! Plus the invariants every readout of this family owes: two runs
//! byte-identical, one `items` behind both faces, the p2 side reconciled against
//! `mcc show stage p2` on the same build, and law C — a readout never vetoes an
//! exit code.
//!
//! ⚠ **One class is empty on every fixture reachable here, and that is
//! reported rather than papered over.** [`every_row_is_accounted_for_exactly_once`]
//! proves `synth` has no members: every anchored row of this hop falls inside a
//! clause, a `func` body, or a module header, and the buckets holding the rest
//! are enumerated. The guard asserted there is the *accounting identity*, which
//! fails the moment a row stops being attributable — a stronger check than a
//! `synth >= 2` no fixture here could satisfy.
//!
//! CJK in the strings below is written as `\u{…}` escapes on purpose: the
//! readout's own annotations are Chinese, and the repository's commit gate
//! rejects CJK in Rust sources. The escapes keep this file ASCII while still
//! pinning the exact text.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The note a `drop` item carries. It states the check that was performed, and
/// stops there — "downstream truly has none" would over-claim, because this
/// build holds rows with no anchor at all and §5.2 hard constraint 3 keeps those
/// apart.
const DROP_NOTE: &str =
    "\u{65e0} p2 \u{884c}\u{951a}\u{5728}\u{672c}\u{8bed}\u{53e5}\u{8de8}\u{5ea6}\u{5185}";

/// The note a `skip` item carries: classified by AST node kind, not by name.
const SKIP_NOTE: &str = "\u{6309} AST kind \u{4e0d}\u{53c2}\u{4e0e}\u{5efa}\u{6a21}";

/// Words that must never appear in a `drop` annotation. This face is read by an
/// agent before a human sees it, and "`drop` is non-empty" is the reading most
/// easily mistaken for "the compiler is wrong" — which is exactly where law C
/// gets violated. The two Chinese entries are the design's own prohibitions,
/// written as escapes to keep this file ASCII.
const FORBIDDEN_IN_DROP_NOTE: &[&str] = &["\u{9519}", "bug", "\u{5e94}\u{5f53}"];

/// Two statements spelled byte for byte alike, at two positions, plus a control
/// statement that is fully taken.
///
/// The duplicate `c2` declaration makes the second pair of `c2.Cap(...)` lines
/// produce no rows; a join that fell back to matching by name would hand them
/// the rows of something else and report a `carry`. Positions 18 and 19 are
/// where they are written — [`line_of`] derives them from this very text so that
/// inserting a line above cannot silently point the assertion at the wrong one.
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

/// The six words of the summary line, in the order the design fixes. All six are
/// printed on every run, zeros included: a dropped zero word would make "there
/// were no merges" indistinguishable from "merges were not measured".
const SIX_WORDS: &[&str] = &["carry", "expand", "merge", "drop", "synth", "skip"];

// ── Fixture plumbing ──

fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

/// A fresh, **empty** directory to run a CLI invocation in.
///
/// Empty on purpose: the readout must not depend on where it is run, and a stray
/// file written into the fixture tree would be mistaken for a product.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-join-{name}-{}-{:?}",
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

/// Run `mcc join <a> <b> …` from `cwd` and return `(stdout, stderr, ok)`.
///
/// `--local` because the readout is local-only: with an `mcc start` service
/// running, a delegated invocation prints nothing at all (§5.3 last ⚠ — the same
/// trap `show stage p2` has).
fn run_hop_in(
    cwd: &Path,
    entry: &Path,
    a: &str,
    b: &str,
    extra: &[&str],
) -> (String, String, bool) {
    let mut args: Vec<&str> = vec!["--local", "join", a, b];
    args.extend_from_slice(extra);
    let entry = entry.to_str().expect("fixture path");
    args.push("-F");
    args.push(entry);
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&args)
        .output()
        .expect("run mcc join");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// The chain's first hop, which most of this file reads.
fn run_join_in(cwd: &Path, entry: &Path, extra: &[&str]) -> (String, String, bool) {
    run_hop_in(cwd, entry, "src", "p2", extra)
}

/// Run `mcc show stage <seg> -f json` from `cwd` — the readout of one segment,
/// which a hop's own side has to reconcile with.
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

/// Run `mcc show stage p2 …` from `cwd` — the other readout of the same build.
fn run_show_p2_in(cwd: &Path, entry: &Path) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(["--local", "show", "stage", "p2", "-f", "json", "-F"])
        .arg(entry)
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

fn counts_of(stage: &Value) -> &Value {
    &stage["counts"]
}

fn class_items<'a>(stage: &'a Value, class: &str) -> Vec<&'a Value> {
    stage["items"]
        .as_array()
        .map(|a| a.iter().filter(|i| i["class"] == class).collect())
        .unwrap_or_default()
}

fn count(c: &Value, key: &str) -> u64 {
    c[key]
        .as_u64()
        .unwrap_or_else(|| panic!("`{key}` is not a number in {c}"))
}

/// Rows of the text face: everything after the `#` header lines, split on the
/// column separator and trimmed of the padding a column adds.
///
/// Two spaces, not any whitespace: a cell may hold a single space (the `! drop`
/// prefix does), and the padding is exactly what the trim removes.
fn text_rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        // The blank line between two groups separates them; it is not a row.
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            l.split("  ")
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect()
        })
        .collect()
}

/// The class as the text face spells it: the first column, minus the two
/// prefixes it may carry (`!` for `drop`, `+` for `synth`).
fn class_cell(cells: &[String]) -> String {
    cells[0]
        .trim_start_matches(|c| c == '!' || c == '+')
        .trim()
        .to_string()
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

// ── Determinism: two runs, byte for byte, on every face ──

#[test]
fn two_runs_are_byte_identical_on_every_face() {
    let first = scratch("det-a");
    let second = scratch("det-b");
    let entry = hbl_entry();

    // The text face is compared in full: it is the default face, O15 puts it
    // inside this acceptance rather than treating it as a convenience view, and
    // it carries no measurement of its own.
    for format in ["text", "csv"] {
        let (a, ea, oka) = run_join_in(&first, &entry, &["-f", format]);
        let (b, eb, okb) = run_join_in(&second, &entry, &["-f", format]);
        assert!(oka && okb, "`join src p2 -f {format}` failed: {ea}{eb}");
        assert!(
            a.lines().count() > 3,
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

    // The hbl project is the fixture here rather than a toy one: with a single
    // box and a single net, sorting has nothing to reorder and byte-identity
    // would pass over one element.
    for format in ["json", "json-pretty"] {
        let (a, ea, oka) = run_join_in(&first, &entry, &["-f", format]);
        let (b, eb, okb) = run_join_in(&second, &entry, &["-f", format]);
        assert!(oka && okb, "`-f {format}` failed: {ea}{eb}");
        assert!(a.len() > 1000, "`-f {format}` printed nothing to compare");
        assert_eq!(
            projection_bytes(&a),
            projection_bytes(&b),
            "`-f {format}`: the projection itself must be byte-identical"
        );
        assert_eq!(
            envelope_without_clock(&a),
            envelope_without_clock(&b),
            "`-f {format}`: the wall clock must be the only thing that moves"
        );
    }

    let (a, ea, oka) = run_join_in(&first, &entry, &["-f", "yaml"]);
    let (b, eb, okb) = run_join_in(&second, &entry, &["-f", "yaml"]);
    assert!(oka && okb, "`-f yaml` failed: {ea}{eb}");
    assert!(
        a.contains("join.src->p2"),
        "the yaml face did not render: {a}"
    );
    assert_eq!(
        yaml_without_clock(&a),
        yaml_without_clock(&b),
        "`-f yaml`: the wall clock must be the only thing that moves"
    );
}

// ── The summary line: one traversal, two faces ──

/// §5.3 ②: six words, **all six always**, zeros included, in a fixed order, each
/// carrying its cardinality. Dropping a zero word would make "nothing merged"
/// indistinguishable from "merging was not measured", and without cardinality a
/// `merge` of N reads as N−1 losses.
#[test]
fn the_summary_line_prints_all_six_words_with_their_cardinality() {
    let dir = scratch("summary");
    let (stdout, stderr, ok) = run_join_in(&dir, &hbl_entry(), &["-f", "text"]);
    assert!(ok, "`join src p2` failed: {stderr}");

    // Law B: a new `view` value on the existing envelope, not a second envelope.
    let line = stdout
        .lines()
        .next()
        .unwrap_or_else(|| panic!("no header at all:\n{stdout}"));
    assert!(
        line.starts_with("# join.src->p2") && line.contains("top=") && line.contains("world_ver="),
        "the header must name the view, the top and the world: {line}"
    );

    let summary = stdout
        .lines()
        .find(|l| l.starts_with("# carry"))
        .unwrap_or_else(|| panic!("no six-word summary line:\n{stdout}"));
    let tokens: Vec<&str> = summary.trim_start_matches('#').split_whitespace().collect();
    assert_eq!(
        tokens.len(),
        SIX_WORDS.len() * 2,
        "expected six word/count pairs in a fixed order: {summary}"
    );

    // The same numbers must come out of the JSON face: one `items`, one
    // traversal, two renderings (settled ruling 3). Two traversals would drift.
    let (json, _, _) = run_join_in(&dir, &hbl_entry(), &["-f", "json"]);
    let stage = stage_of(&json);
    let c = counts_of(&stage);
    for (i, word) in SIX_WORDS.iter().enumerate() {
        assert_eq!(tokens[i * 2], *word, "word {i} is out of order: {summary}");
        let printed: u64 = tokens[i * 2 + 1]
            .parse()
            .unwrap_or_else(|_| panic!("the count for `{word}` is not a number: {summary}"));
        assert_eq!(
            printed,
            count(c, word),
            "`{word}` differs between the text and JSON faces"
        );
    }

    // The sub-hop line names both hops, and their counters are separate.
    let sub = stdout
        .lines()
        .find(|l| l.starts_with("# ") && l.contains("src->ast"))
        .unwrap_or_else(|| panic!("no sub-hop line:\n{stdout}"));
    assert!(
        sub.contains("ast->p2"),
        "the sub-hop line must name both hops, or the two faults are one number again: {sub}"
    );

    // `skip` is a verdict on a construct's kind, not on its cardinality, so its
    // count on the summary line is a total over two different things. The second
    // line splits it, and the split must be read off the items rather than
    // restated: a hardcoded count here would pass whatever the readout did.
    let downstream = class_items(&stage, "skip")
        .iter()
        .filter(|i| i["to"].as_array().is_some_and(|a| !a.is_empty()))
        .count() as u64;
    let without = count(c, "skip") - downstream;
    // Shape: `skip <label> <count>`, the same "label then count" the two sub-hop
    // counters use. Read by position so a reshuffle of the line fails here
    // instead of silently reporting another counter's number.
    let toks: Vec<&str> = sub.trim_start_matches('#').split_whitespace().collect();
    let at = toks
        .iter()
        .position(|t| *t == "skip")
        .unwrap_or_else(|| panic!("the second line must name `skip`: {sub}"));
    let printed: u64 = toks
        .get(at + 2)
        .and_then(|t| t.parse().ok())
        .unwrap_or_else(|| panic!("`skip` must be followed by a label and a count: {sub}"));
    assert_eq!(
        printed,
        count(c, "skip_with_downstream"),
        "the text and JSON faces disagree on how many `skip` clauses have rows: {sub}"
    );
    assert_eq!(
        printed, downstream,
        "the second line must count the `skip` clauses that do have rows: {sub}"
    );
    // Both tiers need members, or the split is a branch no fixture reaches —
    // and `skip` would look like a single kind of thing again.
    assert!(
        downstream >= 2 && without >= 2,
        "both `skip` tiers must be exercised (with rows {downstream}, without {without})"
    );

    // The text face's class column agrees with the JSON classes, on the two
    // classes whose prefix is load-bearing: `grep '^!'` must reach every drop
    // and nothing else.
    let rows = text_rows(&stdout);
    let spelled = |name: &str| rows.iter().filter(|r| class_cell(r) == name).count() as u64;
    for word in SIX_WORDS {
        assert_eq!(
            spelled(word),
            count(c, word),
            "the text face and the JSON face disagree on how many `{word}` items there are"
        );
    }
    let banged = rows.iter().filter(|r| r[0].starts_with('!')).count() as u64;
    assert_eq!(
        banged,
        count(c, "drop"),
        "`!` is the prefix that makes `grep '^!'` a direct route to the losses, and only `drop` may carry it"
    );
}

// ── Acceptance ①: a known loss names the segment that lost it ──

/// The two sub-hops are counted apart because they are two different faults with
/// two different owners: sub-hop 1 failing means **the parser would not take the
/// statement**, sub-hop 2 failing means **Pass 2 never wrote it**. A single
/// mismatch number could report neither, which is the whole deliverable of this
/// batch — so the asymmetry is the assertion, not a detail of it.
#[test]
fn a_statement_pass_two_never_wrote_is_a_drop_on_the_second_sub_hop() {
    for (name, entry) in [
        ("hbl", hbl_entry()),
        ("twice", write_fixture("lost", TWICE_SRC)),
    ] {
        let dir = scratch(&format!("hop-{name}"));
        let (stdout, stderr, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
        assert!(ok, "{name}: `join src p2` failed: {stderr}");
        let stage = stage_of(&stdout);
        let c = counts_of(&stage);

        let drops = class_items(&stage, "drop");
        assert!(
            drops.len() >= 2,
            "{name}: the fixture must contain at least two known losses, found {}",
            drops.len()
        );

        // Nothing is missing from the AST, and every loss is accounted for on the
        // second hop. A `declare` clause carries no Pass-1 statement record, so
        // the only clauses that reach sub-hop 2 are the participating ones this
        // classification may call `drop` — if that ever stops being true, the two
        // counters would have stopped naming the segment, and this must fail.
        assert_eq!(
            count(c, "sub_hop_src_ast"),
            0,
            "{name}: no supported clause is missing from the AST in a well-formed fixture"
        );
        assert_eq!(
            count(c, "sub_hop_ast_p2"),
            count(c, "drop"),
            "{name}: every loss must be counted on the second sub-hop, and nothing else may be"
        );

        for d in &drops {
            assert!(
                d["loc"]["line"].as_u64().unwrap_or(0) > 0,
                "{name}: a loss without a line cannot be looked up: {d}"
            );
            assert_eq!(
                d["to"],
                Value::Array(vec![]),
                "{name}: a loss owns no downstream row: {d}"
            );
            assert_eq!(
                d["why"].as_str(),
                Some(DROP_NOTE),
                "{name}: the note must state the check that ran, not a conclusion about the world: {d}"
            );
            let text = d["text"].as_str().unwrap_or("");
            assert!(
                !text.is_empty(),
                "{name}: a loss must quote the statement it lost: {d}"
            );
            for word in FORBIDDEN_IN_DROP_NOTE {
                assert!(
                    !d["why"].as_str().unwrap_or("").contains(word),
                    "{name}: this face is read as a verdict far too easily — law C is violated \
                     exactly here: {d}"
                );
            }
        }
    }
}

// ── Acceptance ②: a mismatch is never filled in by name ──

/// §5.2 hard constraint 2 / A1: when the position matches nothing, the answer is a
/// mismatch. Filling it in from a name-matched neighbour would turn a defect into
/// a plausible-looking `carry` — the one failure this hop exists to prevent.
#[test]
fn a_position_that_matches_nothing_is_never_filled_in_by_name() {
    let dir = scratch("by-name");
    let entry = write_fixture("twicesame", TWICE_SRC);
    let (stdout, stderr, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "`join src p2` failed: {stderr}");
    let stage = stage_of(&stdout);

    // The fixture writes this statement twice, byte for byte, at two positions.
    let needle = "c2.Cap([VDD, GND])";
    let alike: Vec<&Value> = stage["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["text"].as_str() == Some(needle))
        .collect();
    assert_eq!(
        alike.len(),
        2,
        "the fixture writes `{needle}` twice; the readout must keep two items, not fold them \
         into one by name: {alike:#?}"
    );

    let lines: BTreeSet<u64> = alike
        .iter()
        .map(|i| i["loc"]["line"].as_u64().unwrap_or(0))
        .collect();
    let expected: BTreeSet<u64> = [line_of(TWICE_SRC, needle, 1), line_of(TWICE_SRC, needle, 2)]
        .into_iter()
        .map(|l| l as u64)
        .collect();
    assert_eq!(
        lines, expected,
        "each identical statement must be reported at its own position: {alike:#?}"
    );

    for i in &alike {
        assert_eq!(
            i["class"].as_str(),
            Some("drop"),
            "a lookup that found nothing is a mismatch, never a name-matched carry: {i}"
        );
        assert_eq!(
            i["to"],
            Value::Array(vec![]),
            "and it must not be given rows it does not own: {i}"
        );
    }

    // The control: the statement next to them that *is* fully taken reports as a
    // carry at its own line. Without it, "everything is a mismatch" would pass
    // the assertions above.
    let taken = "CAP c1(1)";
    let carries: Vec<&Value> = stage["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["class"] == "carry" && i["text"].as_str() == Some(taken))
        .collect();
    assert_eq!(
        carries.len(),
        1,
        "the fully-taken statement must be reported as exactly one carry: {carries:#?}"
    );
    assert_eq!(
        carries[0]["loc"]["line"].as_u64(),
        Some(line_of(TWICE_SRC, taken, 1) as u64),
        "and at its own position: {carries:#?}"
    );
}

// ── Acceptance ③: `skip` is never reported as `drop` ──

/// The third item of §7's phase-three row. The judgement is by AST node kind, not by
/// name, so the
/// members come from every kind the criterion names and the count of losses must
/// not include any of them.
#[test]
fn a_clause_that_takes_no_part_in_the_model_is_skip_and_never_drop() {
    let dir = scratch("skip");
    let (stdout, stderr, ok) = run_join_in(&dir, &hbl_entry(), &["-f", "json"]);
    assert!(ok, "`join src p2` failed: {stderr}");
    let stage = stage_of(&stdout);
    let c = counts_of(&stage);
    let items = stage["items"].as_array().unwrap();

    // The three `use` clauses of hbl.mc are the cleanest members the corpus
    // offers: file level, first thing the parser sees, named by the ruling.
    let expected_uses = ["use ./power.mc", "use ./us513.mc", "use ./periph.mc"];
    for (k, line) in [5u64, 6, 7].into_iter().enumerate() {
        let at: Vec<&Value> = items
            .iter()
            .filter(|i| {
                i["loc"]["uri"]
                    .as_str()
                    .map(|u| u.ends_with("hbl.mc"))
                    .unwrap_or(false)
                    && i["loc"]["line"].as_u64() == Some(line)
            })
            .collect();
        assert_eq!(
            at.len(),
            1,
            "hbl.mc:{line} must be reported exactly once: {at:#?}"
        );
        assert_eq!(
            at[0]["class"].as_str(),
            Some("skip"),
            "hbl.mc:{line} is a `use` clause — it takes no part in the model, so it is `skip` \
             and never a loss: {}",
            at[0]
        );
        assert_eq!(
            at[0]["text"].as_str(),
            Some(expected_uses[k]),
            "and it must quote the clause it skipped: {}",
            at[0]
        );
        assert_eq!(at[0]["why"].as_str(), Some(SKIP_NOTE), "{}", at[0]);
        assert_eq!(at[0]["to"], Value::Array(vec![]), "{}", at[0]);
    }

    // The class is not a `use`-only fluke: the port clauses of a module header
    // (`io` / `out`, MCAST_NET_PORTS) and the pin table of a component
    // definition (MCAST_ATTRIBUTE_PIN) are members too. Shapes, not symbol
    // names: a leading keyword that the grammar fixes, or a table row.
    let skips = class_items(&stage, "skip");
    assert_eq!(
        skips.len() as u64,
        count(c, "skip"),
        "the class and the count must agree"
    );
    let shape = |t: &str| -> &'static str {
        let t = t.trim_start();
        if t.starts_with("use ") {
            "use"
        } else if t.starts_with("io ") || t.starts_with("out ") || t.starts_with("in ") {
            "port"
        } else if t.starts_with(|c: char| c.is_ascii_digit()) || t.starts_with('[') {
            "pin-entry"
        } else {
            "other"
        }
    };
    for family in ["use", "port", "pin-entry"] {
        let n = skips
            .iter()
            .filter(|i| shape(i["text"].as_str().unwrap_or("")) == family)
            .count();
        assert!(
            n >= 2,
            "`{family}` is one of the kinds the criterion names and has only {n} member(s) — a \
             class reached by one member is a branch the acceptance never really reaches"
        );
    }

    // And the decisive half: none of them is counted as a loss.
    let losses: Vec<&str> = class_items(&stage, "drop")
        .iter()
        .filter_map(|i| i["text"].as_str())
        .collect();
    for text in &losses {
        assert_ne!(
            shape(text),
            "use",
            "a `use` clause reported as a loss: `{text}` in {losses:?}"
        );
        assert!(
            shape(text) != "port" && shape(text) != "pin-entry",
            "a clause that takes no part in the model reported as a loss: `{text}` in {losses:?}"
        );
    }
    assert_eq!(
        count(c, "drop"),
        losses.len() as u64,
        "the loss count must not include any `skip` member: {losses:?}"
    );
}

// ── Coverage: no class is asserted vacuously ──

/// A class with one member is a branch the acceptance walks past — it reports
/// green because it matches nothing. So each class the fixture reaches must have
/// at least two, and the one class it cannot reach is named here with the reason
/// instead of being quietly skipped.
#[test]
fn every_class_the_readout_reaches_is_exercised() {
    let dir = scratch("cover");
    let (stdout, _, ok) = run_join_in(&dir, &hbl_entry(), &["-f", "json"]);
    assert!(ok);
    let stage = stage_of(&stdout);
    let c = counts_of(&stage);

    for word in SIX_WORDS {
        if *word == "synth" {
            continue;
        }
        let n = class_items(&stage, word).len();
        assert!(
            n >= 2,
            "`{word}` has {n} member(s) on this fixture; a class with one member is a branch the \
             acceptance never really reaches"
        );
    }

    // `synth` is the sixth word and is empty — a fact, not a skipped branch:
    // `every_row_is_accounted_for_exactly_once` proves the emptiness by closing
    // the accounting over every row. Constructing a member here would need a row
    // with no upstream at all, which this hop does not produce for a well-formed
    // project (§5.2 hard constraint 3 keeps that apart from a key defect).
    assert_eq!(
        count(c, "synth"),
        0,
        "this fixture is not expected to reach `synth`; if it now does, the empty-class note \
         above is stale and needs a member-count assertion instead"
    );
}

/// Every row of the hop belongs to exactly one of: a clause's span, or one of the
/// four buckets that say *why* it does not. Publishing both halves is what turns
/// an empty `synth` from a branch nobody reached into a checked claim — the sum
/// stops matching the total the moment a row becomes unattributable.
#[test]
fn every_row_is_accounted_for_exactly_once() {
    let dir = scratch("account");
    let (stdout, _, ok) = run_join_in(&dir, &hbl_entry(), &["-f", "json"]);
    assert!(ok);
    let stage = stage_of(&stdout);
    let c = counts_of(&stage);

    let with_clause = count(c, "rows_with_clause");
    let unanchored = count(c, "unanchored");
    let func_scoped = count(c, "func_scoped");
    let header_scoped = count(c, "header_scoped");
    let synth = count(c, "synth");
    let total = count(c, "rows_total");

    assert_eq!(
        with_clause + unanchored + func_scoped + header_scoped + synth,
        total,
        "rows: {with_clause} inside a clause, {unanchored} unanchored, {func_scoped} inside a \
         `func` body, {header_scoped} on a header, {synth} synth — {total} total. A row that is \
         in none of these buckets means the readout has stopped explaining its own rows"
    );

    // Every bucket that is meant to be non-empty must be filled, or the identity
    // above would hold over three empty sets.
    for (name, n) in [
        ("rows_with_clause", with_clause),
        ("unanchored", unanchored),
        ("func_scoped", func_scoped),
        ("header_scoped", header_scoped),
    ] {
        assert!(n >= 2, "`{name}` has {n} member(s) on this fixture");
    }

    // The unanchored rows are a *key* defect, not a pipeline event, and they are
    // tallied by class so the two can never be read as one number.
    let by_class = &c["unanchored_by_class"];
    let sum: u64 = by_class
        .as_object()
        .unwrap_or_else(|| panic!("`unanchored_by_class` must be an object: {by_class}"))
        .values()
        .map(|v| v.as_u64().unwrap_or(0))
        .sum();
    assert_eq!(
        sum, unanchored,
        "the per-class tally must add up to the unanchored total: {by_class}"
    );
}

// ── Cross-readout: the B side is `stage.p2`, not a second item shape ──

/// Rule 3 of the settled set: the B side of this hop *is* `mcc show stage p2`. If the
/// join grew its
/// own item shape the two would drift immediately, so the row set is required to
/// be exactly p2's items minus the bus rows — which own no point and so are not
/// chain objects at this hop (§2.4).
#[test]
fn the_p2_side_reconciles_with_show_stage_p2() {
    let dir = scratch("reconcile");
    let (join_out, e1, ok1) = run_join_in(&dir, &hbl_entry(), &["-f", "json"]);
    let (p2_out, e2, ok2) = run_show_p2_in(&dir, &hbl_entry());
    assert!(ok1 && ok2, "one of the two readouts failed: {e1}{e2}");
    let j = stage_of(&join_out);
    let p = stage_of(&p2_out);

    assert_eq!(
        j["world_ver"], p["world_ver"],
        "two readouts of one hop have to be two readouts of one world — if they are not, one of \
         them never loaded the library and every count below is about a different build"
    );
    assert_eq!(
        counts_of(&j)["diagnostics"],
        counts_of(&p)["diagnostics"],
        "the two readouts must agree on the build they looked at"
    );

    let p2_items = p["items"].as_array().expect("p2 items");
    let bus = p2_items.iter().filter(|i| i["class"] == "bus").count() as u64;
    let non_bus = p2_items.len() as u64 - bus;
    assert!(
        bus >= 2,
        "the fixture must actually carry bus rows, or this proves nothing"
    );

    assert_eq!(
        count(counts_of(&j), "rows_total"),
        non_bus,
        "the join's row set must be exactly the p2 items that are chain objects"
    );
    assert_eq!(
        count(counts_of(&j), "bus_rows"),
        bus,
        "and the bus rows it set aside must be exactly the ones p2 calls `bus`"
    );
}

// ── `--only`: a filter over the same items, never a smaller world ──

#[test]
fn only_filters_the_items_and_leaves_the_counts_describing_the_whole_hop() {
    let dir = scratch("only");
    let (full_out, _, ok1) = run_join_in(&dir, &hbl_entry(), &["-f", "json"]);
    let (only_out, _, ok2) = run_join_in(&dir, &hbl_entry(), &["-f", "json", "--only", "drop"]);
    assert!(ok1 && ok2);
    let full = stage_of(&full_out);
    let only = stage_of(&only_out);

    let items = only["items"].as_array().expect("items");
    assert_eq!(
        items.len(),
        class_items(&full, "drop").len(),
        "`--only drop` must select exactly the losses"
    );
    assert!(
        items.iter().all(|i| i["class"] == "drop"),
        "and nothing else: {items:#?}"
    );
    assert_eq!(
        only["counts"], full["counts"],
        "the counts keep describing the whole hop, so a filtered readout cannot be mistaken for a \
         world with nothing else in it"
    );
}

// ── Law C, and where it stops ──

/// Law C: a readout never vetoes an exit code. The diagnostic count is a number
/// in the header, not a gate.
#[test]
fn a_readout_never_vetoes_an_exit_code() {
    let dir = scratch("lawc");
    let (stdout, stderr, ok) = run_join_in(&dir, &hbl_entry(), &["-f", "json"]);
    assert!(
        ok,
        "`join src p2` exited non-zero on a project with diagnostics: {stderr}"
    );
    let stage = stage_of(&stdout);
    let c = counts_of(&stage);
    assert!(
        count(c, "diagnostics") > 0,
        "the fixture must actually have diagnostics, or this proves nothing"
    );
    assert!(
        count(c, "drop") > 0,
        "and at least one loss — the reading most easily mistaken for a failure"
    );
}

/// The other side of law C: law C excuses a *judged* readout, not an argument
/// that cannot be honoured. The chain only joins adjacent segments, so a pair
/// that is not in it has no world to report on and must fail loudly rather than
/// print an empty success.
#[test]
fn an_argument_outside_the_chain_fails_loudly() {
    let dir = scratch("pairs");
    for pair in [
        ["src", "vec"], // not adjacent — it would skip p2 and lose its criterion
        ["p2", "src"],  // the chain is directed; reverse belongs to `trace`
        ["viz", "p2"],  // and it is ordered
        ["p2", "viz"],  // adjacent in the chain but not to each other
        ["nope", "p2"], // not a segment at all
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
            .current_dir(&dir)
            .args(["--local", "join", pair[0], pair[1], "-f", "json", "-F"])
            .arg(hbl_entry())
            .output()
            .expect("run mcc join");
        assert!(
            !out.status.success(),
            "`join {} {}` is not a hop in the chain and must not report success",
            pair[0],
            pair[1]
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.is_empty() || (stdout.contains("\"error\"") && !stdout.contains("\"items\"")),
            "and it must not print a readout: {stdout}"
        );
    }
}

// ── The two inner hops (batch 3b) ──
//
// `p2 -> vec` and `vec -> viz` match **objects** rather than statements, so the
// six words mean something else here and one more question has to be answered:
// when a key names two objects, which two are they? The design answers with a
// card on the component a key induces — (1,1) `carry`, (1,N) `expand`, (N,1)
// `merge` — and where the component is bigger than the card allows, the readout
// answers with the diagnostic state rather than a pairing. These tests pin that
// answer, the two criteria that are deliberately **not** keys (a net's member
// set, a path's end pair), and the accounting that makes an empty class a proof.

/// The `drop` / `synth` annotations of the two inner hops, one pair per match
/// rule. Written as escapes like the others: the readout is Chinese and this
/// repository's gate rejects CJK in a Rust source.
const DROP_KEY: &str = "\u{4e0b}\u{6e38}\u{65e0}\u{540c}\u{952e}\u{9879}";
const SYNTH_KEY: &str = "\u{4e0a}\u{6e38}\u{65e0}\u{540c}\u{952e}\u{9879}";
const DROP_ENDS: &str = "\u{4e0b}\u{6e38}\u{65e0}\u{540c}\u{7aef}\u{5bf9}";
const SYNTH_ENDS: &str = "\u{4e0a}\u{6e38}\u{65e0}\u{540c}\u{7aef}\u{5bf9}";
const AMBIGUOUS_NOTE: &str = "\u{9879}\u{5e76}\u{5217}\u{ff0c}\u{4e0d}\u{731c}";

/// The two diagnostic states of the class column — the states beyond §5.3's six
/// words. They
/// are statements about *a join*, not about an object, so they may never be
/// written into a `stage.*` item (O9 / O10).
const DIAG_WORDS: &[&str] = &["branch", "ambiguous"];

/// The two hops of this batch, with the `view` value each must publish.
const HOPS: &[(&str, &str, &str)] = &[
    ("p2", "vec", "join.p2->vec"),
    ("vec", "viz", "join.vec->viz"),
];

/// Run one hop and return its projection, asserting the run itself succeeded.
fn hop_stage(cwd: &Path, entry: &Path, a: &str, b: &str) -> Value {
    let (stdout, stderr, ok) = run_hop_in(cwd, entry, a, b, &["-f", "json"]);
    assert!(ok, "`join {a} {b}` failed: {stderr}");
    stage_of(&stdout)
}

/// The rows of one class, which for these hops name their own kind.
fn rows_of<'a>(stage: &'a Value, class: &str) -> Vec<&'a Value> {
    class_items(stage, class)
}

/// A handle's two parts: the object's class in its own segment, and its key —
/// `box:D39`. One key can name two classes, so a handle that carried no class
/// would print `D39 D39` and say nothing.
fn handle_parts(handle: &str) -> (&str, &str) {
    handle
        .split_once(':')
        .unwrap_or_else(|| panic!("a handle must be `class:key`: {handle}"))
}

/// Two runs of every face are byte-identical, on both hops.
///
/// O15's discipline is a product-level one, and a readout that sorted by anything
/// but its input would fail here — for these hops that means the rows are ordered
/// by a key derived from the two segments' own items, never by the order a
/// `HashMap` happened to have.
#[test]
fn the_two_inner_hops_are_byte_identical_across_runs() {
    let entry = hbl_entry();
    let first = scratch("inner-det-a");
    let second = scratch("inner-det-b");

    for (a, b, view) in HOPS {
        for format in ["text", "json"] {
            let (x, ex, okx) = run_hop_in(&first, &entry, a, b, &["-f", format]);
            let (y, ey, oky) = run_hop_in(&second, &entry, a, b, &["-f", format]);
            assert!(okx && oky, "`join {a} {b} -f {format}` failed: {ex}{ey}");
            assert!(
                x.contains(view),
                "the header must name the hop, or two hops cannot be told apart: {x}"
            );
            if format == "json" {
                assert!(x.len() > 1000, "`-f {format}` printed nothing to compare");
                assert_eq!(
                    projection_bytes(&x),
                    projection_bytes(&y),
                    "`join {a} {b} -f {format}`: the projection itself must be stable"
                );
                assert_eq!(
                    envelope_without_clock(&x),
                    envelope_without_clock(&y),
                    "`join {a} {b} -f {format}`: the wall clock must be the only thing that moves"
                );
            } else {
                assert!(
                    x.lines().count() > 3,
                    "`join {a} {b} -f {format}` printed only a header — the comparison would be \
                     vacuous"
                );
                assert_eq!(x, y, "`join {a} {b} -f text`: two runs differ");
            }
        }
    }
}

/// Every object each hop reads is in exactly one bucket.
///
/// The identity is the one that turns an empty class into a proof: for each side,
/// the objects a kind matched plus the objects kept out of the join for a named
/// reason must add up to the segment's own item count. A class that quietly
/// stopped being counted fails here rather than showing up as a smaller number.
#[test]
fn every_object_of_both_segments_is_accounted_for_exactly_once() {
    let dir = scratch("inner-account");
    let entry = hbl_entry();

    for (a, b, _) in HOPS {
        let stage = hop_stage(&dir, &entry, a, b);
        let c = counts_of(&stage);
        let left = run_show_stage_in(&dir, &entry, a);
        let right = run_show_stage_in(&dir, &entry, b);
        assert_eq!(
            stage["world_ver"], left["world_ver"],
            "`join {a} {b}` and `show stage {a}` must be two readouts of one world"
        );
        assert_eq!(stage["world_ver"], right["world_ver"]);

        let by_kind = c["by_kind"].as_object().expect("by_kind");
        for (side, view) in [("left", &left), ("right", &right)] {
            let tally = c[format!("by_class_{side}")]
                .as_object()
                .unwrap_or_else(|| panic!("by_class_{side} must be an object"))
                .iter()
                .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0)))
                .collect::<std::collections::BTreeMap<_, _>>();

            // The tally is the segment's own histogram, not a second derivation
            // of it: rule 3 of the settled set, at object granularity.
            let mut own: std::collections::BTreeMap<String, u64> =
                std::collections::BTreeMap::new();
            for item in view["items"].as_array().expect("segment items") {
                let class = item["class"].as_str().unwrap_or("-").to_string();
                *own.entry(class).or_default() += 1;
            }
            assert_eq!(
                tally,
                own,
                "the {side} side's classes must be `show stage {}`'s classes: two readouts of one \
                 segment cannot disagree about what is in it",
                if side == "left" { a } else { b }
            );
            assert_eq!(
                count(c, &format!("{side}_total")),
                view["items"].as_array().expect("items").len() as u64,
                "and the total must be that segment's item count"
            );

            let offhop: u64 = c["offhop"]
                .as_object()
                .expect("offhop")
                .iter()
                .filter(|(k, _)| k.starts_with(&format!("{}.", if side == "left" { a } else { b })))
                .map(|(_, v)| v.as_u64().unwrap_or(0))
                .sum();
            let keyless: u64 = c["keyless"]
                .as_object()
                .expect("keyless")
                .iter()
                .filter(|(k, _)| k.starts_with(&format!("{}.", if side == "left" { a } else { b })))
                .map(|(_, v)| v.as_u64().unwrap_or(0))
                .sum();
            let joined: u64 = by_kind
                .values()
                .map(|k| count(k, &format!("joined_{side}")))
                .sum();
            assert_eq!(
                offhop + keyless + joined,
                count(c, &format!("{side}_total")),
                "`join {a} {b}`, {side} side: {offhop} outside every kind, {keyless} inside one but \
                 holding no key, {joined} joined — that must be the whole segment, or an object is \
                 in no bucket at all"
            );
        }
    }
}

// ── The card: what a key's component is ──

/// One key with one object on each side is a `carry`; one key naming nothing
/// downstream is a `drop` under the **key** check, not under the member check.
///
/// The annotation is the assertion: the six words are the same words at every
/// hop, so the note is the only place that says *which* criterion was run and
/// failed. A reader who saw "no member in common" here would go looking for the
/// wrong fault.
#[test]
fn a_key_that_names_nothing_downstream_is_a_drop_under_the_key_check() {
    let dir = scratch("inner-drop");
    let stage = hop_stage(&dir, &hbl_entry(), "p2", "vec");
    let c = counts_of(&stage);

    let drops = rows_of(&stage, "drop");
    assert!(
        drops.len() >= 2,
        "the fixture must reach `drop` at this hop, or nothing below is tested"
    );
    for row in &drops {
        assert_eq!(
            row["why"].as_str().unwrap_or("-"),
            DROP_KEY,
            "a keyed kind's loss says the key was not found: {row}"
        );
        assert_eq!(
            row["side"], "p2",
            "the subject of a loss is the upstream object — the one with no downstream"
        );
        assert!(
            row["to"].as_array().is_some_and(|t| t.is_empty()),
            "a `drop` matched nothing, so it names nothing: {row}"
        );
        assert!(
            row["loc"]["uri"].is_string() && row["loc"]["line"].is_u64(),
            "every item carries a source position, so a loss can be looked up: {row}"
        );
    }
    assert!(
        count(c, "drop") == drops.len() as u64,
        "the class column and the summary line must agree"
    );
}

/// A key that names two objects on **both** sides is the diagnostic state, not
/// an `expand`.
///
/// This is the ruling this hop exists to get right. The projection publishes a
/// module's `D<id>` twice — as the collapsed box and as the layer of its
/// interior — on each side of the hop, so the key corresponds to two objects per
/// side without saying which is which. Calling it `expand` would report a
/// fan-out that nothing measured; pairing by class or by name would be the
/// fallback the design forbids. So the row appears on both sides with the
/// candidates it refused to choose between, and the two candidates differ in
/// class while sharing the key.
#[test]
fn one_key_naming_two_objects_on_each_side_is_ambiguous_not_expand() {
    let dir = scratch("inner-ambig");
    let stage = hop_stage(&dir, &hbl_entry(), "vec", "viz");
    let c = counts_of(&stage);

    let ambiguous = rows_of(&stage, "ambiguous");
    assert!(
        ambiguous.len() >= 2,
        "the fixture must reach the diagnostic state at this hop, or the ruling is untested"
    );
    for row in &ambiguous {
        let why = row["why"].as_str().unwrap_or("-");
        assert!(
            why.ends_with(AMBIGUOUS_NOTE),
            "the row must say it refused to guess, not merely that something was odd: {row}"
        );
        // The count in front of the note is the size of the tie. A row can be
        // refused because the *other* side is tied while its own side holds one
        // candidate, so reading the number off its own side would print "1
        // tied" — a contradiction, and a reader's first reason to distrust the
        // whole column. It must always come from the side that has the tie.
        let n: u64 = why
            .split_once(' ')
            .and_then(|(n, _)| n.parse().ok())
            .unwrap_or_else(|| panic!("the note must carry its tie size: {row}"));
        assert!(
            n >= 2,
            "a tie of fewer than two objects is not a tie: {row}"
        );
    }

    // The tie is one key across two classes, and the handles must show that.
    let mut shown = 0;
    for row in &ambiguous {
        for cell in [&row["to"], &row["from"]] {
            let Some(handles) = cell.as_array().filter(|a| a.len() > 1) else {
                continue;
            };
            let handles: Vec<&str> = handles.iter().filter_map(|h| h.as_str()).collect();
            let (class0, key0) = handle_parts(handles[0]);
            let (class1, key1) = handle_parts(handles[1]);
            assert_eq!(
                key0, key1,
                "a tie is one key seen twice; two different keys would not be a tie: {row}"
            );
            assert_ne!(
                class0, class1,
                "and the two objects must be told apart by their class — identical handles would \
                 make the row unreadable: {row}"
            );
            shown += 1;
        }
    }
    assert!(
        shown >= 2,
        "the fixture must reach a tie that is actually readable, or the assertion above is vacuous"
    );

    // And the other half of the card is empty here: a fan-out needs the key to
    // name one object upstream.
    assert_eq!(
        count(c, "expand"),
        0,
        "`expand` at this hop would mean one upstream key with several downstream objects; if the \
         fixture now has one, this test's note is stale and the card needs a member assertion here"
    );
}

/// The other half of the card: one upstream object whose key names two
/// downstream ones is an `expand` — and the two objects are the two the
/// projection publishes for one module.
#[test]
fn an_expand_names_the_two_objects_one_key_reaches() {
    let dir = scratch("inner-expand");
    let stage = hop_stage(&dir, &hbl_entry(), "p2", "vec");
    let c = counts_of(&stage);

    let expands = rows_of(&stage, "expand");
    assert!(
        expands.len() >= 2,
        "the fixture must reach `expand` at this hop, or the card's middle case is untested"
    );
    assert_eq!(count(c, "expand"), expands.len() as u64);
    for row in &expands {
        assert_eq!(row["side"], "p2", "the subject is the one upstream object");
        let to = row["to"]
            .as_array()
            .expect("an expand names what it reached");
        assert_eq!(
            to.len(),
            2,
            "an expansion of one into two, and the count is on the row: {row}"
        );
        let (class0, key0) = handle_parts(to[0].as_str().expect("handle"));
        let (class1, key1) = handle_parts(to[1].as_str().expect("handle"));
        assert_eq!(key0, key1, "both downstream objects carry the upstream key");
        assert_ne!(
            class0, class1,
            "and they are two objects of the segment, not one written twice: {row}"
        );
        assert!(
            row["from"].as_array().is_some_and(|f| f.is_empty()),
            "an `expand` has one upstream, so `from` names nothing extra: {row}"
        );
    }
}

// ── The two criteria that are deliberately not keys ──

/// A net is matched by its **member set** and never by its label.
///
/// Two modules may each declare a `GND`, so a label is not an identity here — and
/// the fixture proves it twice over: one label is carried by two different nets in
/// the same segment, and nets whose labels differ are carried against each other
/// because their members are the same set. A name-keyed join would have reported
/// both as losses, or worse, paired them by spelling.
#[test]
fn a_net_is_matched_by_its_members_and_never_by_its_label() {
    let dir = scratch("inner-net");
    let entry = hbl_entry();
    let stage = hop_stage(&dir, &entry, "p2", "vec");
    let p2 = run_show_stage_in(&dir, &entry, "p2");

    // The label is not an identity in the segment the hop reads.
    let mut labels: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for item in p2["items"].as_array().expect("p2 items") {
        if item["class"] == "net" {
            if let Some(key) = item["key"].as_str() {
                *labels.entry(key.to_string()).or_default() += 1;
            }
        }
    }
    let collided: Vec<&String> = labels
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(k, _)| k)
        .collect();
    assert!(
        collided.len() >= 2,
        "the fixture must actually carry a label on two different nets, or nothing below is \
         tested: {collided:?}"
    );

    // No net row has a key at all: the hop never gives one to a net (O10 — the
    // match is not an identity and must not become one).
    let net_rows = stage["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|i| i["kind"] == "net");
    let mut carried_with_other_label = 0;
    let mut seen = 0;
    for row in net_rows {
        seen += 1;
        assert!(
            row["key"].is_null(),
            "a net carries no key at this hop: naming it would upgrade a match into an identity \
             {row}"
        );
        if row["class"] != "carry" {
            continue;
        }
        // The row's own detail is the upstream net's name and member count; the
        // downstream handle carries the other side's name. A carry whose two
        // names differ is a net matched by members and not by spelling.
        let upstream = row["text"].as_str().unwrap_or("-");
        assert!(
            upstream.contains("members="),
            "a net's row shows the member count, because the count is the criterion and two nets \
             of one segment may share a name: {row}"
        );
        let downstream = row["to"]
            .as_array()
            .and_then(|t| t.first())
            .and_then(|h| h.as_str())
            .map(|h| handle_parts(h).1)
            .unwrap_or("-");
        if upstream.split(' ').next() != Some(downstream) {
            carried_with_other_label += 1;
        }
    }
    assert!(seen >= 4, "the hop must have nets to match at all");
    assert!(
        carried_with_other_label >= 2,
        "the fixture must carry at least one net against a differently named one, or this test \
         would pass on a join that matched by name after all ({carried_with_other_label})"
    );
}

/// A path is matched by the **ordered pair of both ends** and never by its name.
///
/// A drawn segment carries a `trunk` field that is a name, and a trunk carries a
/// name too — and on this fixture the two sets collide, so a name-keyed join would
/// have paired them. It pairs fewer than the names collide on, which is the
/// assertion: the criterion is the pair, and the names are read for the row and
/// nothing else.
#[test]
fn a_path_is_matched_by_its_end_pair_and_never_by_its_name() {
    let dir = scratch("inner-path");
    let entry = hbl_entry();
    let stage = hop_stage(&dir, &entry, "vec", "viz");
    let vec = run_show_stage_in(&dir, &entry, "vec");
    let viz = run_show_stage_in(&dir, &entry, "viz");
    let c = counts_of(&stage);

    let names = |view: &Value, field: &str| -> std::collections::BTreeSet<String> {
        view["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|i| i["class"] == field)
            .filter_map(|i| i["name"].as_str().or_else(|| i["trunk"].as_str()))
            .map(str::to_string)
            .collect()
    };
    let trunk_names = names(&vec, "trunk");
    let segment_names = names(&viz, "segment");
    let shared: Vec<&String> = trunk_names.intersection(&segment_names).collect();
    assert!(
        shared.len() >= 2,
        "the fixture must actually have a trunk and a segment spelled alike, or this proves \
         nothing: {shared:?}"
    );
    // The count is the *path* kind's, not the hop's: the other kinds carry on
    // their own keys and would swamp the comparison.
    let carried = count(&c["by_kind"]["path"]["classes"], "carry");
    assert!(
        (carried as usize) < shared.len(),
        "a join matching by name would carry at least one object per colliding name, and there \
         are {}: the path kind carried {carried}",
        shared.len()
    );

    // Every path row's handle is the pair, both ends named and each end a sorted
    // list — never a bare name (O9: a path is a path between two endpoints).
    let rows = stage["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|i| i["kind"] == "path");
    let mut seen = 0;
    for row in rows {
        seen += 1;
        let pair = row["key"]
            .as_str()
            .unwrap_or_else(|| panic!("a path's key is its pair of ends: {row}"));
        let (left, right) = pair
            .split_once("->")
            .unwrap_or_else(|| panic!("a path's key is the ordered pair: {pair}"));
        for end in [left, right] {
            assert!(
                !end.is_empty() && !end.contains(' '),
                "an end is a list of canonical names: {pair}"
            );
        }
        match row["class"].as_str().unwrap_or("-") {
            "drop" => assert_eq!(row["why"], DROP_ENDS, "{row}"),
            "synth" => {
                assert_eq!(row["why"], SYNTH_ENDS, "{row}");
                let own = row["to"]
                    .as_array()
                    .and_then(|t| t.first())
                    .and_then(|h| h.as_str())
                    .unwrap_or("-");
                assert_eq!(handle_parts(own).1, pair, "a synth names itself: {row}");
            }
            _ => {
                assert_eq!(
                    row["why"], "",
                    "a carried path has nothing to explain: {row}"
                );
                let matched = row["to"]
                    .as_array()
                    .and_then(|t| t.first())
                    .and_then(|h| h.as_str())
                    .unwrap_or("-");
                let (class, key) = handle_parts(matched);
                assert_eq!(class, "segment", "a path is carried to what drew it: {row}");
                assert_eq!(key, pair, "{row}");
            }
        }
    }
    assert!(seen >= 4, "the hop must have paths on both sides: {seen}");
}

// ── The classes no fixture here reaches ──

/// Six words, two states, at both hops: each class the hop reaches has members,
/// and each class it does not is named here with the reason rather than skipped.
///
/// A class with no member is a branch the acceptance never really enters, and a
/// class with one member is nearly the same thing — so both are called out.
#[test]
fn every_class_the_inner_hops_reach_is_exercised() {
    let dir = scratch("inner-cover");
    let entry = hbl_entry();
    let p2_vec = hop_stage(&dir, &entry, "p2", "vec");
    let vec_viz = hop_stage(&dir, &entry, "vec", "viz");

    // Reached at both hops, with room to spare on the fixture.
    for (stage, hop) in [(&p2_vec, "p2->vec"), (&vec_viz, "vec->viz")] {
        let c = counts_of(stage);
        for word in ["carry", "drop", "ambiguous"] {
            let n = rows_of(stage, word).len();
            assert!(
                n >= 2,
                "`{word}` has {n} member(s) at {hop}; a class with one member is a branch the \
                 acceptance never really reaches"
            );
            assert_eq!(count(c, word), n as u64);
        }
    }

    // `expand` is reached where one upstream key reaches two downstream objects,
    // which the projection does for an instance and does not do for a box.
    assert!(rows_of(&p2_vec, "expand").len() >= 2);
    assert_eq!(
        count(counts_of(&vec_viz), "expand"),
        0,
        "at vec -> viz both sides publish a module's box and its layer, so a key there names two \
         objects per side rather than one; if this is no longer true the note is stale and the \
         class needs a member assertion here"
    );

    // `synth` is reached at the outer hop only, and thinly there.
    assert!(
        rows_of(&vec_viz, "synth").len() >= 2,
        "vec -> viz reaches `synth`"
    );
    let notes: BTreeSet<&str> = rows_of(&vec_viz, "synth")
        .iter()
        .map(|r| r["why"].as_str().unwrap_or("-"))
        .collect();
    for (note, what) in [
        (SYNTH_KEY, "a point the drawing has no upstream point for"),
        (SYNTH_ENDS, "a segment whose pair of ends no trunk carries"),
    ] {
        assert!(
            notes.contains(note),
            "`{note}` ({what}) is missing from the `synth` notes: {notes:?}"
        );
    }

    // At p2 -> vec `synth` is empty, and that emptiness is a checked claim
    // rather than a branch nobody reached: both sides are read off one
    // construction — p2 off the module's frozen net table, vec off the block
    // builder's nets over the same connections — so a vec net with no upstream
    // net over its members means the two no longer agree on what was built.
    // That divergence is CIMP §1 U104, where a re-entered sub-module body
    // (`func` with a boundary formal, called from the parent after the module's
    // table had already been frozen) added connections the vec builder saw and
    // the net table did not, leaving the four SPI conductors shorted into one
    // undiagnosed net in vec while p2 read them as unconnected. Every re-entry
    // now rebuilds the table, so the reading here is 0 — a nonzero one is that
    // defect returning and belongs investigated, not re-baselined.
    assert_eq!(
        rows_of(&p2_vec, "synth").len(),
        0,
        "p2 -> vec reaches `synth` — a downstream net whose members match no upstream net, i.e. \
         the two views disagree about the same construction (CIMP §1 U104): {:?}",
        rows_of(&p2_vec, "synth")
    );

    // `skip` exists at the source hop only: it is a statement about an AST node
    // kind, and neither of these hops reads an AST node. Emitting it here would
    // be a claim this hop cannot support, so zero is asserted rather than noted.
    for (stage, hop) in [(&p2_vec, "p2->vec"), (&vec_viz, "vec->viz")] {
        assert_eq!(
            count(counts_of(stage), "skip"),
            0,
            "`skip` belongs to the source hop, where the item set is AST clauses; {hop} has no \
             AST node to classify and must not claim otherwise"
        );
    }

    // `merge` is one downstream object reached by several upstream keys. No key
    // on this fixture has that shape — every keyed kind's left side is either
    // alone or doubled on both sides — so zero is read off the card, not assumed.
    for (stage, hop) in [(&p2_vec, "p2->vec"), (&vec_viz, "vec->viz")] {
        assert_eq!(
            count(counts_of(stage), "merge"),
            0,
            "if {hop} now has a `merge`, this note is stale and the class needs a member \
             assertion instead of a zero"
        );
    }

    // `branch` is O9's state for a path whose ends are not both named. It is
    // reachable only from a routed `wire` segment, which this fixture has none of.
    for (stage, hop) in [(&p2_vec, "p2->vec"), (&vec_viz, "vec->viz")] {
        assert_eq!(
            count(counts_of(stage), "branch"),
            0,
            "if {hop} now has a `branch`, the fixture grew a path with an unnamed end and the note \
             is stale"
        );
    }
}

/// The two diagnostic states live in the class column and nowhere else.
///
/// O9 says a path with no pair is not given a second life as an item, and O10 says
/// a member-set match is not an identity. Both would be broken the moment a
/// `branch` or an `ambiguous` was written back into a segment's item — where
/// `key: null` means "this object holds no identity", a different statement.
#[test]
fn the_diagnostic_states_are_never_written_back_into_a_segment() {
    let dir = scratch("inner-writeback");
    let entry = hbl_entry();
    for view in ["p2", "vec", "viz"] {
        let stage = run_show_stage_in(&dir, &entry, view);
        for item in stage["items"].as_array().expect("items") {
            let class = item["class"].as_str().unwrap_or("-");
            assert!(
                !DIAG_WORDS.contains(&class),
                "`stage.{view}` may not carry `{class}`: the diagnostic states are statements \
                 about a join, not classes of an object"
            );
        }
    }
}

// ── `--only`, at the hops that have two diagnostic states ──

/// `--only` accepts the six words and the two diagnostic states, filters the same
/// items, and leaves the counts describing the whole hop. Asking for a class the
/// hop has none of is an empty readout, not a failure — law C.
#[test]
fn only_accepts_the_two_diagnostic_states_as_well() {
    let dir = scratch("inner-only");
    let entry = hbl_entry();
    let full = hop_stage(&dir, &entry, "vec", "viz");

    let (out, err, ok) = run_hop_in(
        &dir,
        &entry,
        "vec",
        "viz",
        &["-f", "json", "--only", "ambiguous"],
    );
    assert!(ok, "`--only ambiguous` failed: {err}");
    let only = stage_of(&out);
    assert_eq!(
        only["items"].as_array().expect("items").len(),
        rows_of(&full, "ambiguous").len(),
        "`--only ambiguous` must select exactly the rows the class column prints"
    );
    assert_eq!(
        only["counts"], full["counts"],
        "and the counts must keep describing the whole hop"
    );

    let (out, err, ok) = run_hop_in(
        &dir,
        &entry,
        "vec",
        "viz",
        &["-f", "json", "--only", "branch"],
    );
    assert!(ok, "an empty class is a reading, not a failure: {err}");
    let empty = stage_of(&out);
    assert!(
        empty["items"].as_array().expect("items").is_empty(),
        "this hop has no `branch`, so the filter must select nothing"
    );
    assert_eq!(empty["counts"], full["counts"]);

    let (out, err, ok) = run_hop_in(
        &dir,
        &entry,
        "vec",
        "viz",
        &["-f", "json", "--only", "nope"],
    );
    assert!(!ok, "a word the class column cannot print must fail loudly");
    assert!(
        out.is_empty() || (out.contains("\"error\"") && !out.contains("\"items\"")),
        "and must not print a readout: {out}"
    );
    for word in SIX_WORDS.iter().chain(DIAG_WORDS) {
        assert!(
            err.contains(word),
            "the error must list every word the column can print, `{word}` is missing: {err}"
        );
    }
}

// ── A wiring site is a set, and a statement reaches what it writes ──

/// Two chips wired in a chain, so one pin is written by two statements.
///
/// `u1.P.A` is written by the first statement and again by the third: the row
/// exists once, but it was wired twice, and both facts are true of it.
const MERGE_SRC: &str = r#"
component CHIP {
    partno = "C"
    package = PKG.QFN8
    pins = [ io [1:2] = P{A, B} ]
}

module main {
    CHIP u1
    CHIP u2
    CHIP u3
    CHIP u4
    u1.P.A -> u2.P.A
    u3.P.A -> u4.P.A
    u1.P.A -> u3.P.A
}
"#;

/// One statement per class: two that wire a row, two that wire nothing at all,
/// and a header row the readout classifies by kind.
const CLASSES_SRC: &str = r#"
component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([n1, n2]) {
        n1 - this - n2
    }
}
component CHIP {
    partno = "C"
    package = PKG.QFN8
    pins = [ io [1:2] = P{A, B} ]
}
module main {
    io VDD
    io GND
    CAP c1(1)
    c1.Cap([VDD, GND])
    CAP c2(1)
    c2.Cap([VDD, GND])
    CHIP u1
    CHIP u2
    u1.P.A -> u2.P.A
    u1.P.A -> u2.P.B
}
"#;

/// Every row whose wiring set holds both of these lines, in the readout order.
fn rows_wired_at<'a>(stage: &'a Value, lines: &[u64]) -> Vec<&'a Value> {
    stage["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|i| {
            let set: Vec<u64> = i["loc_all"]
                .as_array()
                .map(|a| a.iter().filter_map(|p| p["line"].as_u64()).collect())
                .unwrap_or_default();
            set == lines
        })
        .collect()
}

/// A row records **every** site that reached it, in walk order.
///
/// Two members, and their sets overlap at different ends: `u1.P.A` is written
/// first and last, `u3.P.A` in the middle. A reader that kept only the first —
/// or only the last — site would put the same line on both rows and fail here.
#[test]
fn wiring_set_records_every_site_in_source_order() {
    let dir = scratch("sites");
    let entry = write_fixture("sites", MERGE_SRC);
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);

    let first = line_of(MERGE_SRC, "u1.P.A -> u2.P.A", 1) as u64;
    let second = line_of(MERGE_SRC, "u3.P.A -> u4.P.A", 1) as u64;
    let third = line_of(MERGE_SRC, "u1.P.A -> u3.P.A", 1) as u64;

    let early = rows_wired_at(&stage, &[first, third]);
    let late = rows_wired_at(&stage, &[second, third]);
    assert_eq!(early.len(), 1, "`u1.P.A` is wired at {first} and {third}");
    assert_eq!(late.len(), 1, "`u3.P.A` is wired at {second} and {third}");

    for (row, want) in [
        (early[0], vec![first, third]),
        (late[0], vec![second, third]),
    ] {
        let got: Vec<u64> = row["loc_all"]
            .as_array()
            .expect("loc_all is always an array")
            .iter()
            .map(|p| p["line"].as_u64().expect("a site has a line"))
            .collect();
        assert_eq!(got, want, "the set is in walk order, not sorted: {row}");
        // The shape never depends on the data: a one-element set is still an
        // array, so a consumer never has to switch on the cardinality.
        assert!(row["loc_all"].is_array());
        assert_eq!(row["via"], "wired");
    }
}

/// A second wiring site is added to the first, never put in place of it.
///
/// Same two rows as the lock above, read the other way round: `loc` is still the
/// **first** site, and it is on the row that also carries the later one — so the
/// later registration extended the set instead of winning it.
#[test]
fn a_second_wiring_site_does_not_replace_the_first() {
    let dir = scratch("sites-union");
    let entry = write_fixture("sites-union", MERGE_SRC);
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);

    let first = line_of(MERGE_SRC, "u1.P.A -> u2.P.A", 1) as u64;
    let second = line_of(MERGE_SRC, "u3.P.A -> u4.P.A", 1) as u64;
    let third = line_of(MERGE_SRC, "u1.P.A -> u3.P.A", 1) as u64;

    for (lines, want_loc) in [(vec![first, third], first), (vec![second, third], second)] {
        let rows = rows_wired_at(&stage, &lines);
        assert_eq!(rows.len(), 1, "{lines:?}");
        let row = rows[0];
        assert_eq!(
            row["loc"]["line"].as_u64(),
            Some(want_loc),
            "`loc` is the first site of the set, not the last one to arrive: {row}"
        );
        assert!(
            lines.contains(&want_loc),
            "the anchoring site is also in the set"
        );
    }
}

/// The declaration site is recorded even when the row was wired.
///
/// Two members: a point written by a statement, and a declaration whose instance
/// is wired by a later statement. Both must carry a non-null `decl_loc`, and the
/// instance's must be a different line from its wiring site — the two states are
/// recorded in parallel, not ranked.
#[test]
fn declaration_site_is_recorded_even_when_the_row_is_wired() {
    let dir = scratch("decl-parallel");
    let entry = write_fixture("decl-parallel", CLASSES_SRC);
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);

    // The two wiring statements, whose rows are the pins they name.
    let mut wired = 0;
    for needle in ["u1.P.A -> u2.P.A", "u1.P.A -> u2.P.B"] {
        let at = line_of(CLASSES_SRC, needle, 1) as u64;
        let rows: Vec<&Value> = stage["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|i| {
                i["loc_all"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|p| p["line"].as_u64() == Some(at)))
            })
            .collect();
        assert!(!rows.is_empty(), "nothing was wired at `{needle}`");
        for row in rows {
            assert_eq!(row["via"], "wired", "{row}");
            assert!(
                !row["decl_loc"].is_null(),
                "a point has a declaration site whatever wired it: {row}"
            );
            assert!(
                row["decl_loc"]["line"].as_u64() != Some(at),
                "the declaration and the wiring are two different sites: {row}"
            );
            wired += 1;
        }
    }
    assert!(wired >= 2, "the lock needs two members, saw {wired}");
}

/// The class follows what the statement reaches, and nothing else.
///
/// Three members, one per rule: a statement that reaches no row is a `drop`, one
/// that reaches a single row is a `carry`, one that reaches two or more is an
/// `expand`. The counts are read off the items, so a class computed from
/// anything but `to` fails here.
#[test]
fn clause_class_follows_what_it_reaches() {
    let dir = scratch("class-reaches");
    let entry = write_fixture("class-reaches", CLASSES_SRC);
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);

    let mut seen: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for item in stage["items"].as_array().expect("items") {
        let class = item["class"].as_str().unwrap_or("");
        if !matches!(class, "carry" | "expand" | "drop") {
            continue;
        }
        let reached = item["to"]
            .as_array()
            .expect("`to` is always an array")
            .len();
        let want = match reached {
            0 => "drop",
            1 => "carry",
            _ => "expand",
        };
        assert_eq!(
            class, want,
            "a statement reaching {reached} row(s) is a `{want}`: {item}"
        );
        assert!(
            item["key"]
                .as_str()
                .is_some_and(|k| k.contains("class-reaches")),
            "the fixture's own statements are the members: {item}"
        );
        *seen.entry(class).or_default() += 1;
    }
    for class in ["carry", "expand", "drop"] {
        assert!(
            seen.get(class).copied().unwrap_or(0) >= 2,
            "`{class}` has {:?} member(s) in this fixture",
            seen.get(class)
        );
    }
}

/// A `drop` reaches nothing — and that is the same thing as the layer being
/// empty, not a second reading that could disagree with it.
#[test]
fn drop_implies_the_layer_has_no_members() {
    let dir = scratch("drop-layer");
    let entry = write_fixture("drop-layer", CLASSES_SRC);
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);

    let drops = class_items(&stage, "drop");
    assert!(drops.len() >= 2, "two known losses, saw {}", drops.len());
    for d in &drops {
        assert!(d["to"].as_array().is_some_and(|t| t.is_empty()), "{d}");
        assert_eq!(d["layer"].as_u64(), Some(0), "{d}");
    }

    // The converse is not claimed: a statement that reaches rows may still
    // reach no *net* row, which is what `layer` counts. Both members of this
    // pair reach rows and neither has a net among them.
    let mut carries = 0;
    for item in stage["items"].as_array().expect("items") {
        if item["class"] == "carry" {
            assert_eq!(item["to"].as_array().map(Vec::len), Some(1), "{item}");
            carries += 1;
        }
    }
    assert!(carries >= 2, "the class must not be empty here: {carries}");
}

/// A statement that reaches nothing is a `drop`; the fixture proves one exists
/// even though every statement is spelled correctly.
#[test]
fn drop_still_fires_when_nothing_reaches_it() {
    let dir = scratch("drop-fires");
    let entry = write_fixture("drop-fires", CLASSES_SRC);
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);
    let c = counts_of(&stage);

    let a = line_of(CLASSES_SRC, "c1.Cap([VDD, GND])", 1) as u64;
    let b = line_of(CLASSES_SRC, "c2.Cap([VDD, GND])", 1) as u64;
    for at in [a, b] {
        let item = class_items(&stage, "drop")
            .into_iter()
            .find(|d| d["loc"]["line"].as_u64() == Some(at))
            .unwrap_or_else(|| panic!("nothing was lost at line {at}"));
        assert_eq!(item["to"], Value::Array(vec![]));
    }
    // The second sub-hop counts exactly these, and this fixture has no other
    // loss: the two faces have to agree, or one of them is reporting another
    // segment's fault.
    assert_eq!(count(c, "sub_hop_ast_p2"), count(c, "drop"));
    assert_eq!(count(c, "sub_hop_src_ast"), 0);
}

/// A `skip` is a verdict on a construct's kind, so it can never become a
/// `drop` — not even when it reaches nothing.
#[test]
fn skip_never_becomes_drop() {
    let dir = scratch("skip-drop");
    let entry = write_fixture("skip-drop", CLASSES_SRC);
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);

    let skips = class_items(&stage, "skip");
    let without: Vec<&&Value> = skips
        .iter()
        .filter(|s| s["to"].as_array().is_some_and(|t| t.is_empty()))
        .collect();
    assert!(
        skips.len() >= 2,
        "the classification must have members: {}",
        skips.len()
    );
    // Both halves are filled: a `skip` with rows (a header row the readout
    // classifies by kind) and one without. An empty half would let this pass
    // while the `to` list was broken.
    assert!(
        !without.is_empty(),
        "a `skip` with a downstream row is needed"
    );
    assert!(
        without.len() < skips.len(),
        "a `skip` with no downstream row is needed"
    );
    for s in &skips {
        assert_eq!(s["class"], "skip");
        assert_ne!(s["why"], DROP_NOTE, "{s}");
    }
    assert_eq!(
        count(counts_of(&stage), "drop"),
        class_items(&stage, "drop").len() as u64
    );
}

/// The root layer's connection statements are no longer reported as losses.
///
/// They were `drop` before this batch for a reason that had nothing to do with
/// the board: their endpoints live in *other* files, and a row could remember
/// only one position. Every one of them now reaches the rows it writes.
#[test]
fn root_layer_clauses_are_no_longer_dropped() {
    let dir = scratch("root-layer");
    let entry = hbl_entry();
    let (stdout, err, ok) = run_join_in(&dir, &entry, &["-f", "json"]);
    assert!(ok, "{err}");
    let stage = stage_of(&stdout);
    let src = std::fs::read_to_string(&entry).expect("read the entry source");

    let mut seen = 0;
    for needle in [
        "USB.vin -> V5V::DC(5V)",
        "V5V -> LDO{vin|vout} -> V3V3::DC(3.3V)",
        "V3V3 -> DCDC -> V1V2::DC(1.2V)",
        "MCU513.i2c().loadFlash(FLASH.SPI)",
        "MIC(V3V3).MIC ->",
    ] {
        let at = line_of(&src, needle, 1) as u64;
        let item = stage["items"]
            .as_array()
            .expect("items")
            .iter()
            .find(|i| {
                i["key"]
                    .as_str()
                    .is_some_and(|k| k.ends_with(&format!(":{at}")))
            })
            .unwrap_or_else(|| panic!("the readout has no clause at line {at}"));
        assert_ne!(
            item["class"], "drop",
            "a root-layer statement that wires the board is not a loss: {item}"
        );
        assert!(
            item["to"].as_array().is_some_and(|t| !t.is_empty()),
            "and it must name what it reached: {item}"
        );
        seen += 1;
    }
    assert!(seen >= 2, "the lock needs two members, saw {seen}");
}
