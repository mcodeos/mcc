// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc show` reads the target the command **resolved**, not the argument it
//! was handed (CIMP §1 U93).
//!
//! A directory is a target: `prepare` hands it to `load_target`, which reads
//! `project.toml` for the entry file, or browses for the unique `.mc` file
//! declaring `module main` (`use-design.md` §19.5 rule 3). The world that gets
//! loaded is therefore the **entry file's** world, and `show dianlu <dir>` has
//! always printed exactly what `show dianlu <entry.mc>` prints.
//!
//! Three faces did not read that: `show all`, `show lapper` and `show ast`
//! derived a path of their own from the raw positional, so a directory reached
//! them as a **directory URI** — nothing was ever parsed under it. `lapper`
//! dumped the empty symbol table of that unparsed URI (99226 bytes of
//! `(none)`), `ast` printed nothing, `all` printed its `------ file ------`
//! header and stopped. All three exited 0 and said nothing on stderr: a
//! reading-shaped empty output, which is what `count: 0` names as the defect —
//! an empty reading is only a reading when the diagnostics say the input was
//! fine.
//!
//! Two more faces were outside the directory rule as well: `ast` and `lapper`
//! were missing from the list of targets whose positional is a path, so a
//! **directory** was not resolved for them at all, and a directory that
//! resolves to no entry (`browse: no .mc file declaring module main`) was never
//! reported.
//!
//! What is locked here is the **law**, not the products: the two readings must
//! be the same reading, and a target that loads nothing must say so. Neither
//! assertion pins any product's bytes, so a change to what `show lapper`
//! prints does not have to touch this file.
//!
//! ⚠ `show defs` is deliberately **not** in the same-world list below. It lists
//! the whole definition space layered by origin, so naming a file breaks that
//! file's definitions out under `-- file --` while naming a directory does not
//! — the same set of definitions, presented differently because the two runs
//! name different origins. That is a `defs` presentation question, not this
//! law's, and it is recorded as such rather than locked either way.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Faces whose positional is a **project target**: each resolves a directory
/// through the target law and must read the world it names.
const SAME_WORLD_FACES: &[&str] = &["all", "dianlu", "pwr", "pwrflow", "lapper", "ast"];

/// Every face whose positional is a path at all — the same list plus `defs`,
/// the one face that layers its output by the origin named.
const PATH_FACES: &[&str] = &["all", "defs", "dianlu", "pwr", "pwrflow", "lapper", "ast"];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A fresh, **empty** directory to run a CLI invocation in: the readout must
/// not depend on where it is run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-showtarget-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc --local show <face> <target>` from `cwd`; returns
/// `(stdout, stderr, exit_ok)`.
///
/// `--local` on every call: without it a running `mcc start` service answers
/// instead, on its own world (`skills/mcc/reference/pipeline.md` §5.3).
fn run_show(cwd: &Path, face: &str, target: &Path) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(["--local", "show", face])
        .arg(target)
        .output()
        .expect("run mcc show");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

/// A directory target names a project; the command resolves it to the entry
/// file and reads that world. The two invocations below name the *same* world,
/// so they must print the same reading.
///
/// This is the assertion the defect broke: `all` printed 19 bytes against the
/// directory and 675 against the file, `lapper` 99226 against the directory
/// and 120869 against the file, `ast` 0 against the directory and 15312
/// against the file.
#[test]
fn a_directory_target_reads_the_same_world_as_its_entry_file() {
    let cwd = scratch("same-world");
    let dir = fixture("hbl");
    let entry = dir.join("src/hbl.mc");
    assert!(dir.is_dir() && entry.is_file(), "fixture layout moved");

    for face in SAME_WORLD_FACES {
        let (dir_out, dir_err, dir_ok) = run_show(&cwd, face, &dir);
        let (file_out, file_err, file_ok) = run_show(&cwd, face, &entry);

        assert!(file_ok, "`show {face} <entry.mc>` failed: {file_err}");
        assert!(
            dir_ok,
            "`show {face} <dir>` failed — a directory is a target and `{face}` \
             reads the world it resolves to: {dir_err}"
        );
        assert!(
            !file_out.is_empty(),
            "`show {face} <entry.mc>` printed nothing, so the comparison below \
             would be vacuous"
        );
        assert_eq!(
            dir_out, file_out,
            "`show {face}` read a different world from a directory than from the \
             entry file that directory resolves to — the directory is being \
             looked up as a target in its own right"
        );
        assert_eq!(
            dir_err, file_err,
            "`show {face}` reported different diagnostics for a directory than \
             for the entry file it resolves to"
        );
    }
}

/// A directory that resolves to no entry is **not** an empty reading: the
/// command says what is wrong and fails.
///
/// Before the fix, `lapper` and `ast` accepted it and printed the empty symbol
/// table of a URI that was never parsed, with exit 0 and nothing on stderr.
#[test]
fn a_directory_that_resolves_to_no_entry_says_so() {
    let cwd = scratch("no-entry");
    let empty = cwd.join("empty-project");
    std::fs::create_dir_all(&empty).expect("create empty project dir");

    for face in PATH_FACES {
        let (out, err, ok) = run_show(&cwd, face, &empty);
        assert!(
            !ok,
            "`show {face} <dir with no entry>` reported success — the directory \
             resolves to nothing, and that is not a reading"
        );
        assert!(
            out.is_empty(),
            "`show {face} <dir with no entry>` printed {} bytes of reading-shaped \
             output: {out}",
            out.len()
        );
        assert!(
            err.contains("no `.mc` file declaring `module main`"),
            "`show {face} <dir with no entry>` did not report the entry \
             selection: {err}"
        );
    }
}

/// A path that does not exist is reported, not read as an empty world.
///
/// `lapper` and `ast` used to take the raw argument as their URI whether or not
/// the command had loaded anything under it.
#[test]
fn a_missing_path_is_reported() {
    let cwd = scratch("missing");
    let missing = cwd.join("definitely-not-here.mc");
    assert!(!missing.exists(), "scratch dir is not fresh");

    for face in PATH_FACES {
        let (out, err, ok) = run_show(&cwd, face, &missing);
        assert!(!ok, "`show {face} <missing path>` reported success: {err}");
        assert!(
            out.is_empty(),
            "`show {face} <missing path>` printed {} bytes: {out}",
            out.len()
        );
        assert!(
            err.contains("file not found"),
            "`show {face} <missing path>` did not report the missing file: {err}"
        );
    }
}

/// The face that takes a path but is not a project reader keeps its own
/// requirement: with no target at all, it asks for one instead of reading the
/// working directory.
///
/// `prepare` falls back to the cwd when it holds a project manifest, which is
/// right for `show component MCU`; a file dump is not that.
#[test]
fn a_path_face_with_no_target_asks_for_one() {
    let cwd = scratch("no-target");
    for face in ["lapper", "ast"] {
        let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
            .current_dir(&cwd)
            .args(["--local", "show", face])
            .output()
            .expect("run mcc show");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "`show {face}` with no target succeeded"
        );
        assert!(
            err.contains("requires an entity name"),
            "`show {face}` with no target did not ask for one: {err}"
        );
    }
}
