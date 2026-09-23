// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc export <KIND> --json` speaks the **bare payload** — no envelope (U277).
//!
//! The envelope around the export JSON face is retired: the payload
//! (`kind` / `format` / `count` / `items`) is the whole answer, on stdout and
//! under `-o` alike. The byte stream is pinned in **declaration order**
//! (`{"kind":…,"format":…,"count":…,"items":…}`) because that order is the
//! mechanism behind the ruling's "two entries, one byte stream": the local
//! build and the RPC round-trip both serialize the same `ExportData` struct
//! through one emitter, while a `serde_json::Value` would alphabetize the keys.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn hbl_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-export-json-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch");
    dir
}

/// Run `mcc --local <args…>` from `cwd`; returns `(stdout, stderr, exit_ok)`.
///
/// `--local` on every call: without it a listening service answers instead, on
/// its own world and its own (possibly older) binary.
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

fn bom_json() -> String {
    let cwd = scratch("bom");
    let (out, err, ok) = run_mcc(&cwd, &["export", "bom", "--json", hbl_dir().to_str().unwrap()]);
    assert!(ok, "`mcc export bom --json` failed: {err}");
    out
}

/// The payload is the whole answer: exactly the four payload keys, none of the
/// envelope's (`jsonrpc` / `result` / `workspace` / `command` are gone), and
/// the count answers for the items it ships with.
#[test]
fn export_json__the_payload_is_the_whole_answer() {
    let out = bom_json();
    let payload: serde_json::Value = serde_json::from_str(&out).expect("stdout is one JSON value");
    let mut keys: Vec<&str> = payload
        .as_object()
        .expect("the payload is an object")
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["count", "format", "items", "kind"],
        "the face carries exactly the payload keys: {out}"
    );
    assert_eq!(payload["kind"], "bom");
    assert_eq!(payload["format"], "json");
    let items = payload["items"].as_array().expect("items is an array");
    assert!(!items.is_empty(), "the fixture export has rows: {out}");
    assert_eq!(payload["count"], items.len(), "count answers for items");
}

/// Declaration order in the raw bytes: `{"kind":…,"format":…,"count":…}` —
/// compact, envelope-free, and struct-ordered. This line is what makes the two
/// entry points byte-identical; a `Value`-shaped emission would alphabetize.
#[test]
fn export_json__the_bytes_are_the_payload_in_declaration_order() {
    let out = bom_json();
    assert!(
        out.starts_with(r#"{"kind":"bom","format":"json","count":"#),
        "the JSON face opens with the payload in declaration order:\n{}",
        out
    );
    assert!(!out.contains("jsonrpc"), "no envelope rides along:\n{out}");
}

/// `-o` carries the same bytes stdout does — the payload face honors the file
/// outlet exactly as the raw-artifact faces always have. `-o` is an exclusive
/// outlet (stdout stays empty), so the comparison is stdout run vs file run of
/// the same input; the emission is input-determined (the product-order lock),
/// so equal bytes mean one byte stream.
#[test]
fn export_json__the_file_face_carries_the_same_bytes() {
    let target = hbl_dir();

    let cwd = scratch("file-face");
    let (stdout, err, ok) = run_mcc(&cwd, &["export", "bom", "--json", target.to_str().unwrap()]);
    assert!(ok, "`mcc export bom --json` failed: {err}");
    assert!(!stdout.is_empty(), "the stdout face printed nothing");

    let cwd = scratch("file-face-o");
    let out_path = cwd.join("bom.json");
    let (stdout_o, err, ok) = run_mcc(
        &cwd,
        &[
            "export",
            "bom",
            "--json",
            "-o",
            out_path.to_str().unwrap(),
            target.to_str().unwrap(),
        ],
    );
    assert!(ok, "`mcc export bom --json -o` failed: {err}");
    assert_eq!(stdout_o, "", "-o owns the outlet; stdout stays empty");

    let file = std::fs::read_to_string(&out_path).expect("the -o file was written");
    assert_eq!(stdout, file, "stdout and the -o file are the same bytes");
}

/// `json-pretty` is the same payload, pretty-printed — not the raw artifact it
/// used to fall through to — so both JSON spellings speak the payload face.
#[test]
fn export_json__pretty_is_the_same_payload_pretty_printed() {
    let cwd = scratch("pretty");
    let (out, err, ok) = run_mcc(
        &cwd,
        &[
            "export",
            "bom",
            "--format",
            "json-pretty",
            hbl_dir().to_str().unwrap(),
        ],
    );
    assert!(ok, "`mcc export bom --format json-pretty` failed: {err}");
    let payload: serde_json::Value = serde_json::from_str(&out).expect("stdout is one JSON value");
    assert_eq!(payload["kind"], "bom");
    assert_eq!(payload["format"], "json-pretty");
    assert!(
        out.contains(r#""kind": "bom""#),
        "pretty spelling prints with indentation:\n{out}"
    );

    // Same rows as the compact face: only the spelling differs.
    let compact: serde_json::Value =
        serde_json::from_str(&bom_json()).expect("the compact face is one JSON value");
    assert_eq!(payload["items"], compact["items"]);
}
