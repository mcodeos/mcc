// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Minimal CLI locks for `impact` / `import` — the two words the U379 usage
//! census found with **zero** consumers: no test, no script, no external caller
//! touched them, so nothing would notice if the faces rotted.
//!
//! The lock is deliberately minimal — it pins the *contract*, not any product
//! content:
//!
//! - both words answer with their own versioned payload (`schema_version`,
//!   `impact.1.0` / `import.1.0`), not the shared command envelope — that
//!   split is world-repartition-design §7 D4, still an open item, so the
//!   schema tag is the one stable thing to hold;
//! - `impact` distinguishes "the sym names nothing" (exit 1) from "the world
//!   cannot be built" (exit 2) — the two answers demand different reactions;
//! - `import` round-trips against `export`: the file `export netlist` just
//!   wrote reads back with zero changes (exit 0), and a perturbed artifact
//!   reads back as drift (exit 1, `count > 0`).
//!
//! Nothing here pins which nets exist or how many consumers a def has — any
//! fixture edit keeps these green as long as the words keep answering.

use std::path::{Path, PathBuf};
use std::process::Command;

fn hbl() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// A fresh scratch directory, unique per test, so parallel shards never share.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-impact-import-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc --local <args…>` from `cwd`; returns `(stdout, exit_code)`.
///
/// `--local` on every call: without it a running `mcc start` service answers
/// instead, on its own world.
fn run_mcc(cwd: &Path, args: &[&str]) -> (String, Option<i32>) {
    let mut full = vec!["--local"];
    full.extend_from_slice(args);
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&full)
        .output()
        .expect("run mcc");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        out.status.code(),
    )
}

/// `impact` answers with its own versioned payload, not the command envelope.
#[test]
fn impact_json_report_carries_schema_version() {
    let (stdout, code) = run_mcc(&hbl(), &["impact", "main", "-f", "json"]);
    assert_eq!(code, Some(0), "impact main should succeed: {stdout}");
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("impact -f json emits one JSON object");
    assert_eq!(report["schema_version"], "impact.1.0");
    assert_eq!(report["sym"], "main");
}

/// A sym that names nothing is exit 1 — not a build failure (2), not success.
#[test]
fn impact_unresolved_symbol_exits_1() {
    let (_, code) = run_mcc(&hbl(), &["impact", "no_such_def_xyz", "-f", "json"]);
    assert_eq!(code, Some(1), "unresolved sym is exit 1, got {code:?}");
}

/// The file `export netlist` just wrote reads back as agreement: zero changes,
/// same versioned payload family. (The exit code is `count + diagnostics`
/// together — the hbl fixture world is not diagnostic-clean, so exit 0 is not
/// part of this lock; the drift test below pins the exit-1 half.)
#[test]
fn import_round_trip_agreement_has_zero_changes() {
    let dir = scratch("agree");
    let artifact = dir.join("hbl.net");
    let (stdout, code) = run_mcc(
        &hbl(),
        &[
            "export",
            "netlist",
            "-o",
            artifact.to_str().expect("utf-8 temp path"),
        ],
    );
    assert_eq!(code, Some(0), "export netlist should succeed: {stdout}");
    assert!(artifact.metadata().expect("artifact written").len() > 0);

    let (stdout, code) = run_mcc(
        &hbl(),
        &[
            "import",
            artifact.to_str().expect("utf-8 temp path"),
            "--from",
            "netlist",
            "-f",
            "json",
        ],
    );
    let (stdout, code) = run_mcc(
        &hbl(),
        &[
            "import",
            artifact.to_str().expect("utf-8 temp path"),
            "--from",
            "netlist",
            "-f",
            "json",
        ],
    );
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .expect("import -f json emits one JSON object even when it exits 1");
    assert_eq!(report["schema_version"], "import.1.0");
    assert_eq!(report["count"], 0, "agreement means zero changes: {stdout}");
    if code != Some(0) {
        // The exit came from world diagnostics, not from the round-trip.
        assert!(
            report["diagnostics"].as_u64().expect("diagnostics is a number") > 0,
            "exit {code:?} with zero changes must come from world diagnostics: {stdout}"
        );
    }
}

/// A perturbed artifact is drift: exit 1 and a non-zero change count.
#[test]
fn import_detects_drift() {
    let dir = scratch("drift");
    let artifact = dir.join("hbl.net");
    let (_, code) = run_mcc(
        &hbl(),
        &[
            "export",
            "netlist",
            "-o",
            artifact.to_str().expect("utf-8 temp path"),
        ],
    );
    assert_eq!(code, Some(0));

    let text = std::fs::read_to_string(&artifact).expect("read exported netlist");
    let perturbed = text.replace("V3V3", "V3V3_DRIFT");
    assert_ne!(text, perturbed, "fixture must contain the net we rename");
    std::fs::write(&artifact, perturbed).expect("write perturbed netlist");

    let (stdout, code) = run_mcc(
        &hbl(),
        &[
            "import",
            artifact.to_str().expect("utf-8 temp path"),
            "--from",
            "netlist",
            "-f",
            "json",
        ],
    );
    assert_eq!(code, Some(1), "drift must be exit 1: {stdout}");
    let report: serde_json::Value =
        serde_json::from_str(&stdout).expect("import -f json emits one JSON object");
    assert!(
        report["count"].as_u64().expect("count is a number") > 0,
        "drift means a non-zero change count: {stdout}"
    );
}
