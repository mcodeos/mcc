// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage-readout design §7 phase three (batch 3a) acceptance: `mcc join src p2`.
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

/// Run `mcc join src p2 …` from `cwd` and return `(stdout, stderr, ok)`.
///
/// `--local` because the readout is local-only: with an `mcc start` service
/// running, a delegated invocation prints nothing at all (§5.3 last ⚠ — the same
/// trap `show stage p2` has).
fn run_join_in(cwd: &Path, entry: &Path, extra: &[&str]) -> (String, String, bool) {
    let mut args: Vec<&str> = vec!["--local", "join", "src", "p2"];
    args.extend_from_slice(extra);
    let entry = entry.to_str().expect("fixture path");
    args.push("-F");
    args.push(entry);
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&args)
        .output()
        .expect("run mcc join src p2");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
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
        assert!(
            out.stdout.is_empty(),
            "and it must not print a readout: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}
