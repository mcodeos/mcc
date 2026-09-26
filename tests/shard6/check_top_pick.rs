// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Implicit top-module picking for `mcc check` (U305④). A first-row pick
//! over the `(uri, ident)`-sorted registry view hands the top to whichever
//! module sorts first — for a helper named `SUB` that is the helper, built
//! silently while the real top's instances never register. The pick excludes
//! modules instantiated in-file; ties error loudly and `--top` overrides.
//!
//! Binary-level harness (`CARGO_BIN_EXE_mcc`): the check face reads process
//! globals (`cli::globals()`), so an in-process call would need
//! `set_globals` — shared state races with parallel tests. Each test gets
//! its own temp directory (parallel suites sharing one pid temp dir have
//! overwritten each other into false greens before — U291).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const CHIP: &str =
    "component CHIP\n{\n    pins = [\n        io 1 = A\n        io 2 = B\n    ]\n}\n";

/// `SUB` instantiates `CHIP`; `main` instantiates `SUB`. `SUB` sorts before
/// `main`, which is exactly the shape the old first-row pick hijacked.
const HIERARCHY: &str =
    "module SUB\n{\n    CHIP u1\n    io A\n    A -> u1.1\n}\n\nmodule main\n{\n    SUB s1\n}\n";

/// `SUB` and `main` are both roots — nothing is instantiated anywhere, so
/// the helper filter cannot break the tie.
const TWO_ROOTS: &str =
    "module SUB\n{\n    CHIP u1\n    io A\n    A -> u1.1\n}\n\nmodule main\n{\n    CHIP u2\n}\n";

/// Write `body` into a fresh temp dir and return the file path. A per-test
/// directory: two tests sharing one fixture path race under parallel runs.
fn fixture(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mcc-check-top-pick-{}", name));
    fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("proj.mc");
    fs::write(&path, format!("{}\n{}", CHIP, body)).expect("write fixture");
    path
}

fn check(path: &PathBuf, extra: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args(["check"])
        .args(extra)
        .arg(path)
        .output()
        .expect("run mcc check");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// A helper instantiated in-file is not a top candidate: `main` is built,
/// its full instance tree registers, and the connection through the module
/// boundary lands (`1 of 2`). Under the old pick this file built `SUB`
/// instead — clean, with `main`'s diagnostics never generated.
#[test]
fn check_toppick__helper_instantiated_infile_is_excluded() {
    let path = fixture("hierarchy", HIERARCHY);
    let (rc, out, err) = check(&path, &[]);
    assert_eq!(rc, 0, "out: {}\nerr: {}", out, err);
    assert!(!out.contains("cannot pick the top module"), "out: {}", out);
    assert!(
        out.contains("'main.s1.u1' has 1 of 2 pins connected"),
        "main was not the built top:\n{}",
        out
    );
}

/// Two disjoint roots stay ambiguous: loud error naming the candidates and
/// asking for `--top` — never a silent lexicographic guess.
#[test]
fn check_toppick__ambiguous_roots_error_loudly() {
    let path = fixture("two_roots", TWO_ROOTS);
    let (rc, out, err) = check(&path, &[]);
    assert_ne!(rc, 0, "out: {}", out);
    assert!(err.contains("cannot pick the top module"), "err: {}", err);
    assert!(err.contains("candidates (SUB, main)"), "err: {}", err);
    assert!(err.contains("pass --top"), "err: {}", err);
}

/// `--top` resolves the ambiguity — check reads the global flag and builds
/// the named module.
#[test]
fn check_toppick__top_flag_overrides() {
    let path = fixture("two_roots_flag", TWO_ROOTS);
    let (rc, out, err) = check(&path, &["--top", "SUB"]);
    assert_eq!(rc, 0, "out: {}\nerr: {}", out, err);
    assert!(
        out.contains("'SUB.u1' has 1 of 2 pins connected"),
        "SUB was not the built top:\n{}",
        out
    );
}
