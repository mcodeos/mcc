// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! A manifest-less folder build reports **every entry it built**.
//!
//! CIMP §1 U95 (ruled 2026-09-18): `pass2.net_checks` covers the folder, not
//! only the entry whose tree the envelope carries. b3491 had taken the opposite
//! reading — "the field describes the one circuit the envelope holds" — and paid
//! for it with that batch's single **reduction**, because the local folder face
//! built its non-first entries tree-only (`mcc_virtual_build_with_nets`: no
//! flatten, so there were no flat net checks to report). The ruling was to pay
//! the flatten on both faces instead. This file locks the local face; the
//! daemon's half is in-crate
//! (`cli_buildcmd__build_full_directory_batch_reports_net_erc_truth`).
//!
//! Rows carry the `uri` of the file a finding is located in, so a folder's set
//! is attributed entry by entry. A row never names the entry it was *reached*
//! through, and should not: a check belongs to the file it is located in.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// First entry: two buffers with their outputs merged → `driver-conflict`.
const A_MC: &str =
    "component BUFA {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n\
                    module main {\n    BUFA b1\n    BUFA b2\n    b1.Y -> b2.Y\n}\n";

/// Second entry: one part, nothing wired → the unwired-pin family. Its own
/// component name keeps the two entries' symbol spaces apart.
const B_MC: &str =
    "component BUFB {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n\
                    module other {\n    BUFB b1\n}\n";

/// A fresh, **empty** directory to run a CLI invocation in, so nothing the
/// command reads can depend on where the test happens to be.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-dirnet-{name}-{}-{:?}",
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
/// instead, and this lock is about what the local face reports.
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

/// The flat electrical net checks one `build -f json` carries, plus that build's
/// stderr.
fn build_rows(cwd: &Path, target: &Path) -> (Vec<Value>, String) {
    let t = target.to_str().expect("fixture path");
    let (out, err, ok) = run_mcc(cwd, &["build", "-f", "json", t]);
    assert!(ok, "`mcc build {t}` failed:\n{err}");
    let env: Value = serde_json::from_str(&out).expect("build envelope is JSON");
    let rows = env["result"]["pass2"]["net_checks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    (rows, err)
}

/// A row's identity as data: the whole row, key order aside.
fn row_key(r: &Value) -> String {
    serde_json::to_string(r).expect("row is JSON")
}

/// The folder's rows are exactly what its entries report one by one — and the
/// console section is rendered from those rows.
///
/// The first entry is the one whose tree the envelope carries (`pass2.top` is
/// `main`), so a row from the second entry proves the field is no longer gated
/// on the carried circuit.
#[test]
fn folder_net_checks_cover_every_entry_it_built() {
    let cwd = scratch("cover");
    let folder = scratch("cover-entries");
    std::fs::write(folder.join("a.mc"), A_MC).expect("write a.mc");
    std::fs::write(folder.join("b.mc"), B_MC).expect("write b.mc");

    let (folder_rows, err) = build_rows(&cwd, &folder);
    let (a_rows, _) = build_rows(&cwd, &folder.join("a.mc"));
    let (b_rows, _) = build_rows(&cwd, &folder.join("b.mc"));

    // The fixture must exercise the comparison: an entry that reports nothing
    // would make "both entries are covered" true by accident.
    for (name, rows) in [("a.mc", &a_rows), ("b.mc", &b_rows)] {
        assert!(
            rows.len() >= 2,
            "`{name}` must carry its own checks for this lock to mean anything: {rows:?}"
        );
    }

    // Each entry's own reading, then the folder's — the second is the first two
    // in build order, not a subset and not a duplicate.
    let mut expected: Vec<String> = a_rows.iter().map(row_key).collect();
    expected.extend(b_rows.iter().map(row_key));
    let got: Vec<String> = folder_rows.iter().map(row_key).collect();
    assert_eq!(
        got, expected,
        "the folder build must report every entry's rows, in build order"
    );

    let uris: BTreeSet<&str> = folder_rows
        .iter()
        .filter_map(|r| r["uri"].as_str())
        .collect();
    assert_eq!(
        uris.len(),
        2,
        "the rows are located in both entries' files: {uris:?}"
    );

    // What the console shows is what the payload carries (one renderer, and it
    // is fed the envelope's field).
    assert!(
        err.contains(&format!(
            "=== Electrical Net Checks ({} issues) ===",
            folder_rows.len()
        )),
        "the section header must count the rows the payload carries: {err}"
    );
}
