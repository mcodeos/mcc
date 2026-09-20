// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `-o/--output` belongs to the **command**, not to one of its faces.
//!
//! Three stage-family commands had grown the same defect: their **text** face
//! honoured `-o` and their **structured** face silently ignored it, because the
//! envelope call hardcoded the path to `None` instead of passing the caller's.
//! So `mcc show stage viz -o out.txt` wrote a file and
//! `mcc show stage viz -f json -o out.json` printed to stdout and wrote nothing —
//! one command, two behaviours, decided by `-f`. `join` and `trace` carried the
//! identical pair (`write_text` honours the flag, the envelope call did not).
//!
//! `show lapper` was the mirror image: its **structured** face went through the
//! shared projection emitter and honoured `-o`, while both of its **text**
//! branches printed straight to stdout and never read the flag at all.
//!
//! The lock is written as a **redirect, not a rewrite**: the bytes `-o` puts in
//! the file must be the bytes the same command prints without it. That is the
//! property both defects broke, and it does not pin any product's content.
//!
//! ⚠ Two wall-clock fields have to be normalised before that comparison, and both
//! are pre-existing, not introduced here: the products' own generation stamps,
//! and `summary.elapsed_ms`, the envelope's one field that is not derived from
//! the input. Whether a product should carry its generation time is CIMP §1 U92.
//!
//! `parse --viz` keeps its own law here too: the outlet it takes when `-o` is
//! **absent**, derived from the source path and the payload's format.

use std::path::{Path, PathBuf};
use std::process::Command;

fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

/// A fresh, **empty** directory to run a CLI invocation in — the readout must
/// not depend on where it is run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-outpath-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc --local <args…>` from `cwd`; returns `(stdout, stderr, exit_ok)`.
///
/// `--local` on every call: without it a running `mcc start` service answers
/// instead, on its own world (`skills/mcc/reference/pipeline.md` §5.3).
fn run_mcc(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut full = vec!["--local"];
    full.extend_from_slice(args);
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&full)
        .output()
        .expect("run mcc");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

/// The one field that is not derived from the input, normalised away.
///
/// It is not part of what this file locks; it would otherwise make every
/// comparison fail for a reason the test is not about. **Every** occurrence of
/// the clock is zeroed, whatever face carries it and wherever the value sits:
/// the structured faces print one compact line, so `"elapsed_ms": 3` lands
/// **mid-line** (a line-start match misses it — that is exactly the red one
/// verification round caught), and the yaml face spells it `elapsed_ms: 3`
/// unquoted. The export products used to need the same treatment for their
/// generation-time line, which CIMP §1 U92 ruled out — they are now compared
/// whole, stamps included, because there are none.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    while let Some(i) = s[pos..].find("elapsed_ms") {
        let start = pos + i;
        out.push_str(&s[pos..start]);
        let tail = &s[start..];
        // The key up to and including its colon: quoted on the JSON faces,
        // bare on the yaml face — copied verbatim either way.
        let colon = tail.find(':').expect("elapsed_ms is always a key");
        out.push_str(&tail[..=colon]);
        let after = tail[colon + 1..].trim_start();
        let digits = after
            .len()
            - after
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len();
        assert!(digits > 0, "elapsed_ms carries no number: {after}");
        out.push('0');
        pos = start + colon + 1 + (tail[colon + 1..].len() - after.len()) + digits;
    }
    out.push_str(&s[pos..]);
    out
}

/// One command's `-o` contract: the flag redirects the product, it does not
/// change it, and it is read on **both** faces.
///
/// `args` is the invocation **without** `-o` and without the target; `tail` is
/// the target as that command documents it (`-F <entry>` for the stage family,
/// a bare path for `show lapper`); `face` is empty for the text face or the
/// `-f`/`--json` pair for a structured one. The two faces must differ in nothing
/// but *where the bytes go*.
fn assert_output_flag_redirects(
    cwd: &Path,
    name: &str,
    args: &[&str],
    face: &[&str],
    tail: &[&str],
) {
    let mut plain: Vec<&str> = args.to_vec();
    plain.extend_from_slice(face);
    plain.extend_from_slice(tail);
    let (stdout, stderr, ok) = run_mcc(cwd, &plain);
    assert!(ok, "`mcc {}` failed: {stderr}", plain.join(" "));
    assert!(
        stdout.len() > 100,
        "`mcc {}` printed {} bytes to stdout without `-o`, so there is no product \
         to redirect and the assertion below would be vacuous",
        plain.join(" "),
        stdout.len()
    );

    let out_file = cwd.join(format!("{name}-{}.out", face.len()));
    let _ = std::fs::remove_file(&out_file);
    let path_str = out_file.to_str().expect("output path").to_string();

    let mut with_o: Vec<&str> = args.to_vec();
    with_o.extend_from_slice(face);
    with_o.push("-o");
    with_o.push(&path_str);
    with_o.extend_from_slice(tail);
    let (stdout_o, stderr_o, ok_o) = run_mcc(cwd, &with_o);
    assert!(ok_o, "`mcc {}` failed: {stderr_o}", with_o.join(" "));

    let face_name = if face.is_empty() {
        "text"
    } else {
        "structured"
    };
    assert!(
        stdout_o.is_empty(),
        "`mcc {}` still printed {} bytes to stdout with `-o` given ({face_name} face)",
        with_o.join(" "),
        stdout_o.len()
    );
    let written = std::fs::read_to_string(&out_file).unwrap_or_else(|e| {
        panic!(
            "`mcc {}` wrote no file at {}: {e} — the {face_name} face ignores `-o`",
            with_o.join(" "),
            out_file.display()
        )
    });
    assert_eq!(
        normalize(&written).trim_end_matches('\n'),
        normalize(&stdout).trim_end_matches('\n'),
        "`mcc {}` wrote a different product to the file than it prints without `-o`",
        with_o.join(" ")
    );
}

/// `show stage` — the command the defect was first recorded on: text wrote the
/// file, `-f json` did not.
#[test]
fn show_stage_honours_output_on_both_faces() {
    let cwd = scratch("show-stage");
    let e = hbl_entry();
    let e = e.to_str().expect("fixture path");
    let tail = ["-F", e];

    assert_output_flag_redirects(&cwd, "stage-text", &["show", "stage", "viz"], &[], &tail);
    assert_output_flag_redirects(
        &cwd,
        "stage-json",
        &["show", "stage", "viz"],
        &["-f", "json"],
        &tail,
    );
    // A second segment and a second structured format: one pair could pass by
    // accident, and `-f yaml` is a different renderer on the same envelope.
    assert_output_flag_redirects(
        &cwd,
        "stage-yaml",
        &["show", "stage", "p2"],
        &["-f", "yaml"],
        &tail,
    );
}

/// `join` — the same pair, found by reading every `emit_envelope` call site
/// after the `show stage` one surfaced.
#[test]
fn join_honours_output_on_both_faces() {
    let cwd = scratch("join");
    let e = hbl_entry();
    let e = e.to_str().expect("fixture path");
    let tail = ["-F", e];

    assert_output_flag_redirects(&cwd, "join-text", &["join", "vec", "viz"], &[], &tail);
    assert_output_flag_redirects(
        &cwd,
        "join-json",
        &["join", "p2", "vec"],
        &["-f", "json"],
        &tail,
    );
}

/// `trace` — the same pair again.
#[test]
fn trace_honours_output_on_both_faces() {
    let cwd = scratch("trace");
    let e = hbl_entry();
    let e = e.to_str().expect("fixture path");
    let tail = ["-F", e];

    assert_output_flag_redirects(&cwd, "trace-text", &["trace", "main.V1V2"], &[], &tail);
    assert_output_flag_redirects(
        &cwd,
        "trace-json",
        &["trace", "main.V1V2"],
        &["-f", "json"],
        &tail,
    );
}

/// Copy a fixture project tree into `dst`.
///
/// `tests/fixtures/hbl` is a project root (`project.toml` plus `src/`,
/// `symbols/`, `baseline/`); `parse` resolves the modules beside the entry, so
/// the whole root has to come along. The copy is what keeps this test from
/// writing into the repository — the outlet is derived from the source path.
fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dir");
    for entry in std::fs::read_dir(src).expect("read fixture dir") {
        let entry = entry.expect("dir entry");
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
        }
    }
}

/// `parse --viz` / `parse --viz-json` — the **default** outlet, taken when `-o`
/// is absent: a file beside the source, whose extension follows the payload.
///
/// Both modes used to derive `<stem>.html`, so `--viz-json` overwrote what
/// `--viz` had just written and left a JSON payload under an HTML name, in the
/// source tree. The two writers now share one naming law, and this locks the two
/// properties that law has: the name follows the payload, and the two modes
/// cannot collide.
///
/// `parse --viz … --top <m>` is a **second** writer (`run_viz`), and both
/// writers share one write rule: `-o` overrides *where* a payload goes, never
/// *whether* one is written, and the payload's format only picks the name. That
/// rule used to hold for the all-modules writer and not for `run_viz`, whose
/// JSON arm wrote nothing at all — so `--viz-json --top <m>` computed the
/// document and dropped it. Steps C and D exercise the second writer's two arms.
#[test]
fn parse_viz_default_outlet_follows_the_payload() {
    let cwd = scratch("parse-viz");
    let root = cwd.join("hbl");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl"),
        &root,
    );
    let entry = root.join("src/hbl.mc");
    let entry_str = entry.to_str().expect("fixture path").to_string();
    let beside_html = root.join("src/hbl.html");
    let beside_json = root.join("src/hbl.json");

    // A. `--viz`: beside the source, `.html`, and the payload really is HTML.
    let (_, stderr, ok) = run_mcc(&cwd, &["parse", &entry_str, "--viz"]);
    assert!(ok, "`mcc parse --viz` failed: {stderr}");
    assert!(
        stderr.contains("[viz] wrote"),
        "`mcc parse --viz` reported no write: {stderr}"
    );
    let html = std::fs::read_to_string(&beside_html).unwrap_or_else(|e| {
        panic!(
            "`mcc parse --viz` did not write {}: {e}",
            beside_html.display()
        )
    });
    assert!(
        html.starts_with("<!DOCTYPE html>"),
        "`mcc parse --viz` wrote {} bytes that are not an HTML document",
        html.len()
    );

    // B. `--viz-json`: a *different* name, and the payload really is JSON.
    let (_, stderr, ok) = run_mcc(&cwd, &["parse", &entry_str, "--viz-json"]);
    assert!(ok, "`mcc parse --viz-json` failed: {stderr}");
    assert!(
        stderr.contains("[viz] wrote"),
        "`mcc parse --viz-json` reported no write: {stderr}"
    );
    let json = std::fs::read_to_string(&beside_json).unwrap_or_else(|e| {
        panic!(
            "`mcc parse --viz-json` did not write {} — the two modes still share \
             one outlet name: {e}",
            beside_json.display()
        )
    });
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("`mcc parse --viz-json` wrote a payload that is not JSON: {e}"));
    assert!(
        parsed.get("root_bid").is_some(),
        "`mcc parse --viz-json` wrote JSON that is not a viz document: {}",
        &json[..json.len().min(120)]
    );

    // …and the JSON run did not clobber the HTML one.
    let html_after = std::fs::read_to_string(&beside_html).expect("html still beside source");
    assert_eq!(
        html_after,
        html,
        "`mcc parse --viz-json` overwrote `{}` — the two modes collide",
        beside_html.display()
    );

    // C. The second writer, HTML arm: same outlet, no file left from step A.
    std::fs::remove_file(&beside_html).expect("remove html for step C");
    let (_, stderr, ok) = run_mcc(&cwd, &["parse", &entry_str, "--viz", "--top", "main"]);
    assert!(ok, "`mcc parse --viz --top main` failed: {stderr}");
    let html_top = std::fs::read_to_string(&beside_html)
        .expect("`mcc parse --viz --top main` wrote no file beside the source");
    assert!(
        html_top.starts_with("<!DOCTYPE html>"),
        "`mcc parse --viz --top main` wrote something that is not an HTML document"
    );

    // D. The same writer's JSON arm: it writes, under its own name, and it does
    //    not touch what step C wrote.
    std::fs::remove_file(&beside_json).expect("remove json for step D");
    let (_, stderr, ok) = run_mcc(&cwd, &["parse", &entry_str, "--viz-json", "--top", "main"]);
    assert!(ok, "`mcc parse --viz-json --top main` failed: {stderr}");
    let json_top = std::fs::read_to_string(&beside_json).unwrap_or_else(|e| {
        panic!(
            "`mcc parse --viz-json --top main` wrote no file beside the source — the \
             payload was computed and dropped: {e}"
        )
    });
    let parsed: serde_json::Value = serde_json::from_str(&json_top).unwrap_or_else(|e| {
        panic!("`mcc parse --viz-json --top main` wrote a payload that is not JSON: {e}")
    });
    assert!(
        parsed.get("root_bid").is_some(),
        "`mcc parse --viz-json --top main` wrote JSON that is not a viz document"
    );
    assert_eq!(
        std::fs::read_to_string(&beside_html).expect("html still beside source"),
        html_top,
        "the JSON arm overwrote `{}` — the two payloads still collide",
        beside_html.display()
    );
}

/// `show lapper` — the mirror image: the structured face honoured `-o` and the
/// **text** face printed to stdout regardless.
///
/// The file is a positional here, not `-F`: `show lapper -F <file>` is a
/// different form and is rejected ("requires an entity name"), so the target is
/// passed the way the command documents it.
#[test]
fn show_lapper_honours_output_on_both_faces() {
    let cwd = scratch("show-lapper");
    let e = hbl_entry();
    let e = e.to_str().expect("fixture path");
    let tail = [e];

    assert_output_flag_redirects(&cwd, "lapper-text", &["show", "lapper"], &[], &tail);
    assert_output_flag_redirects(
        &cwd,
        "lapper-json",
        &["show", "lapper"],
        &["-f", "json"],
        &tail,
    );
}
