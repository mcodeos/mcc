// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Minimal lock for `proj` — the one word the U379 usage census found with a
//! single action (`create`), zero tests, and one dead external caller
//! (`mcs/hbl/run.sh` invoked a `proj open` that does not exist). The ruling
//! kept the word and fixed the script; this lock keeps the surviving action
//! honest: `proj create` scaffolds a directory with a `project.toml`, and the
//! word's face stays `create`-only.

use std::path::Path;
use std::process::Command;

fn scratch() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-proj-lock-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn run_mcc(cwd: &Path, args: &[&str]) -> (String, Option<i32>) {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run mcc");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        out.status.code(),
    )
}

/// `proj create <dir>` scaffolds the directory and writes a project.toml.
#[test]
fn proj_create_scaffolds_project_toml() {
    let dir = scratch();
    let project = dir.join("my-project");
    let (stdout, code) = run_mcc(&dir, &["proj", "create", "my-project"]);
    assert_eq!(code, Some(0), "proj create should succeed: {stdout}");
    assert!(
        project.join("project.toml").is_file(),
        "project.toml must exist after create"
    );
}

/// The word has exactly one action: anything else must fail loudly, not
/// silently no-op (the dead `proj open` caller relied on the opposite).
#[test]
fn proj_rejects_unknown_action() {
    let dir = scratch();
    let (_, code) = run_mcc(&dir, &["proj", "open", "."]);
    assert_ne!(code, Some(0), "proj open does not exist and must fail");
}
