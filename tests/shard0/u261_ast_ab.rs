// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U261 S0 baseline gate (mcd/doc/grammar/c-ffi-to-rust-migration-design.md
//! §3 S0): the tree-equivalence A/B comparator, run in its C/C self-check
//! mode. Every corpus file is parsed twice through the same in-process
//! protocol (fresh workspace → visit capture on → `mcc_load_from_string` →
//! take the visit JSON, then read the diagnostics), and the two runs must
//! agree on
//!
//! 1. the visit JSON (the `show ast --format json` face, byte-stable modulo
//!    the `elapsed_ms` summary field, which is wall-clock output and is
//!    stripped before the comparison — same exception the byte-identical
//!    rendering locks already make), and
//! 2. the diagnostics multiset, keyed on (code, level, row, col, message).
//!
//! S0 acceptance is zero diff on both faces for every corpus file. The same
//! comparator later arbitrates the S1/S2 Rust-frontend phases: the C engine's
//! output is baseline A, the replacement's output is candidate B, and the
//! corpus must stay at zero diff through every swap step (design doc §3,
//! "同形绞替").
//!
//! Corpus (design doc §3): the grammar source-of-truth corpus in mcast
//! `test/*.mc`, this workspace's `tests/corpus/*.mc`, and the hbl board
//! fixture (`tests/fixtures/hbl`, its project `use` graph resolves relative
//! to each file's own path only when a project root is in play — here every
//! file parses standalone, which is exactly the parse-level face S0 pins).

#![allow(non_snake_case)]

use crate::common;

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One corpus file's parse fingerprint: the captured visit JSON (`None` for
/// a file that parses no module at all), volatile fields stripped, plus the
/// diagnostics multiset.
struct Fingerprint {
    tree: Option<Value>,
    diags: BTreeMap<DiagKey, usize>,
}

/// Multiset key for one diagnostic: code, severity, position, formatted
/// message. Positions are included — the comparator's job is to prove the
/// parse is bit-for-bit reproducible, and a code/message match at a different
/// span is a real diff.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DiagKey {
    code: u32,
    severity: i32,
    row: u32,
    col: u32,
    msg: String,
}

/// Recursively drop run-variant fields from a visit JSON value. `elapsed_ms`
/// is the only one today (`show_ast`'s summary, src/cmds/show.rs); strip it
/// at every level so a later engine version cannot smuggle the field deeper.
fn strip_volatile(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.remove("elapsed_ms");
            for (_, child) in map.iter_mut() {
                strip_volatile(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_volatile(item);
            }
        }
        _ => {}
    }
}

/// Structural diff of two JSON values with a readable path for every
/// mismatch. Returns the mismatch list; empty means equivalent. Object key
/// order is irrelevant (serde_json maps compare by content); array order is
/// significant (the visit JSON's arrays are child/sibling chains).
fn diff_json(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            for (k, va) in ma {
                match mb.get(k) {
                    Some(vb) => diff_json(&format!("{path}.{k}"), va, vb, out),
                    None => out.push(format!("{path}: key `{k}` missing in B")),
                }
            }
            for k in mb.keys() {
                if !ma.contains_key(k) {
                    out.push(format!("{path}: key `{k}` missing in A"));
                }
            }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.len() != ab.len() {
                out.push(format!("{path}: length {} vs {}", aa.len(), ab.len()));
            }
            for (i, (va, vb)) in aa.iter().zip(ab.iter()).enumerate() {
                diff_json(&format!("{path}[{i}]"), va, vb, out);
            }
        }
        _ => {
            if a != b {
                out.push(format!("{path}: {a} vs {b}"));
            }
        }
    }
}

/// One parse run over `uri`/`src` in a fresh workspace: enable the visit
/// capture, parse, take the tree for this URI, then read the diagnostics.
/// A comment-only file (mcast/test/invalid.mc) parses no module and captures
/// nothing — that is a legitimate empty tree, so the capture is `Option` and
/// the gate counts how many files captured a real tree (the non-vacuity
/// assert below keeps a mass capture failure from reading as a green run).
fn run_once(uri: &str, src: &str) -> Fingerprint {
    common::reset();
    mcc::set_ast_visit_json(true);
    mcc::clear_ast_visit_json();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let tree = mcc::take_ast_visit_json_for(uri);
    let mut diags: BTreeMap<DiagKey, usize> = BTreeMap::new();
    for d in mcc::mcc_diagnose_all() {
        *diags
            .entry(DiagKey {
                code: d.code,
                severity: d.level.as_lsp_severity(),
                row: d.loc.row,
                col: d.loc.col,
                msg: d.msg,
            })
            .or_insert(0) += 1;
    }
    Fingerprint { tree, diags }
}

fn corpus_roots() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    vec![
        root.join("../mcast/test"),
        root.join("tests/corpus"),
        root.join("tests/fixtures/hbl"),
    ]
}

fn collect_mc_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // A missing corpus root is a setup error, not a green skip: the gate
        // must never quietly compare nothing.
        Err(e) => panic!("corpus root {} unreadable: {e}", dir.display()),
    };
    for entry in entries {
        let entry = entry.expect("read_dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_mc_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("mc") {
            out.push(path);
        }
    }
}

/// The S0 gate: C parse vs C parse, every corpus file, both faces, zero diff.
#[test]
fn u261_s0__comparator_self_run_is_zero_diff_over_the_corpus() {
    let _guard = common::lock();

    let mut files = Vec::new();
    for root in corpus_roots() {
        collect_mc_files(&root, &mut files);
    }
    files.sort();
    assert!(
        files.len() >= 30,
        "corpus shrank to {} files — the gate must not quietly compare nothing",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut captured = 0usize;
    let mut with_diags = 0usize;
    for path in &files {
        let uri = path.canonicalize().unwrap_or_else(|e| {
            panic!("cannot canonicalize {}: {e}", path.display());
        });
        let uri = uri.to_string_lossy().into_owned();
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        let mut run = |uri: &str, src: &str| {
            let fp = run_once(uri, src);
            let empty = Value::Null;
            let mut tree = fp.tree.clone().unwrap_or(empty);
            strip_volatile(&mut tree);
            (fp.tree.is_some(), tree, fp.diags)
        };
        let (got_a, tree_a, diags_a) = run(&uri, &src);
        let (got_b, tree_b, diags_b) = run(&uri, &src);
        captured += usize::from(got_a && got_b);
        with_diags += usize::from(!diags_a.is_empty());

        let mut diffs = Vec::new();
        diff_json("$", &tree_a, &tree_b, &mut diffs);
        if diags_a != diags_b {
            let only_a: Vec<_> = diags_a.keys().collect();
            let only_b: Vec<_> = diags_b.keys().collect();
            diffs.push(format!("diagnostics multiset differs: A={only_a:?} B={only_b:?}"));
        }
        if !diffs.is_empty() {
            failures.push(format!("{}:\n  {}", uri, diffs.join("\n  ")));
        }
    }

    assert!(
        captured >= 30,
        "only {captured} of {} corpus files produced a visit tree — a mass capture \
         failure must not read as a green run",
        files.len()
    );
    // Visible summary: how much the gate actually compared. A corpus whose
    // diagnostics face is entirely silent would still gate the tree face,
    // but the number belongs in the evidence, not in an assumption.
    println!(
        "u261 S0: {} files, {captured} with a visit tree, {with_diags} with diagnostics",
        files.len()
    );
    assert!(
        failures.is_empty(),
        "C/C self-run must be zero diff over the corpus (S0 acceptance), but {} file(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
