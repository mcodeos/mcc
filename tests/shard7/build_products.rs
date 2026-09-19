// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc build --product <IDS>` — the file products of a build, and where they go.
//!
//! build-design §3.4 puts every file product under `<project-root>/build/`, and
//! §3.3 says the ids are the `mcc export <KIND>` tokens. This file locks the
//! three properties that pairing has to have to be worth anything:
//!
//! * **the location is the project root, not the caller's directory** — a
//!   product is a property of the project, so running the command from anywhere
//!   must put it in the same place;
//! * **a product is the same file twice**, because §3.7 discipline 4 holds of a
//!   file exactly as it holds of stdout (the export face's own lock is
//!   `product_order.rs`; this is the build face of the same discipline, and the
//!   two exist separately because one is a stream and one is a path);
//! * **the row carries the two spaces**, which is the whole reason `inst-list`
//!   was made a product: a bare list of names would not have needed one.
//!
//! ⚠ `inst-list` is not the only id; it is the one this file follows end to end.
//! The other three resolve through the same `ExportKind` table and the same
//! `build_payload` funnel, so their *content* is already locked by the export
//! tests — what is new here is only the file outlet, and that is
//! product-agnostic. `bom` is the exception that proves it: it is an
//! `ExportKind` and not a product, so the outlet has to refuse it by name.
//!
//! The default set is deliberately **empty** (the envelope is the report face and
//! keeps its own outlet), so "no `--product`" must leave no `build/` behind. That
//! is asserted rather than assumed: without it a future default could start
//! writing files on every build and no test would notice.

use std::path::{Path, PathBuf};
use std::process::Command;

fn hbl_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// A private copy of the fixture project, so a build can write into it.
///
/// The fixture itself is read-only by convention — several shards build it
/// concurrently, and a product written there would be one shard's file showing
/// up in another's tree.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-buildprod-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    copy_tree(&hbl_dir(), &dir);
    dir
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst");
    for entry in std::fs::read_dir(src).expect("read src") {
        let entry = entry.expect("entry");
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy file");
        }
    }
}

/// Run `mcc --local <args…>` from `cwd`; returns `(stdout, stderr, exit_ok)`.
///
/// `--local` on every call: without it a listening service answers instead, on
/// its own world and its own (possibly older) library.
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

/// Run a build that is expected to **reach the envelope**, and return its stdout.
///
/// The exit code is deliberately not the assertion. The fixture reports an
/// electrical error (E5414), and a build that reports errors still writes its
/// products — the error rides in the envelope and the exit code, exactly as
/// `mcc export` does not refuse to project a circuit it has complaints about.
/// So what must be checked is that the command **ran to the end**: an empty
/// stdout is a usage error or a panic, and would otherwise let "no product was
/// written" pass for the wrong reason.
fn run_build(cwd: &Path, args: &[&str]) -> String {
    let (out, err, _) = run_mcc(cwd, args);
    assert!(
        !out.is_empty(),
        "`mcc {}` produced no envelope — it did not run to the end: {err}",
        args.join(" ")
    );
    out
}

/// `-o` is not consulted, and neither is the working directory: the product is
/// written under the **project root**, whichever directory the command ran from.
#[test]
fn a_product_lands_under_the_project_root_not_the_caller() {
    let root = project("location");
    // Deliberately run from a *different* directory than the project root: a
    // product that followed the caller would land in `outside/` and pass a test
    // that ran from the root.
    let outside = root.parent().expect("parent").join(format!(
        "mcc-buildprod-outside-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&outside).expect("create outside dir");

    let root_arg = root.to_str().expect("root path");
    let out = run_build(&outside, &["build", root_arg, "--product", "inst-list"]);

    let product = root.join("build/inst-list.txt");
    assert!(
        product.is_file(),
        "no product at {}; the build answered with:\n{out}",
        product.display()
    );
    assert!(
        !outside.join("build/inst-list.txt").exists(),
        "the product followed the caller's directory"
    );
    assert!(
        std::fs::metadata(&product).expect("stat product").len() > 200,
        "the product is too small to be the list under test"
    );
}

/// The default set is the envelope alone, so nothing is written.
///
/// Locked because "writes no file" is the easy thing to lose: a default that
/// started emitting a product would be invisible in every other test.
#[test]
fn without_the_flag_no_product_is_written() {
    let root = project("default");
    let root_arg = root.to_str().expect("root path");
    run_build(&root, &["build", root_arg]);
    assert!(
        !root.join("build").exists(),
        "a build with no --product wrote into {}",
        root.join("build").display()
    );
}

/// §3.7 discipline 4, on the file outlet.
///
/// Two **processes**, not two calls: a container whose iteration order is drawn
/// from a per-process seed cannot be caught in one process, and the product is
/// compared whole — a stable line count with shuffled rows is the failure this
/// is looking for, not a difference in size.
#[test]
fn the_same_build_writes_the_same_product_twice() {
    let root = project("determinism");
    let root_arg = root.to_str().expect("root path");
    let product = root.join("build/inst-list.txt");

    run_build(&root, &["build", root_arg, "--product", "inst-list"]);
    let first = std::fs::read(&product).expect("read first product");
    run_build(&root, &["build", root_arg, "--product", "inst-list"]);
    let second = std::fs::read(&product).expect("read second product");

    assert!(!first.is_empty(), "the product came out empty");
    assert_eq!(
        first, second,
        "one input produced two different files — the product's order is not the input's"
    );
}

/// The row carries both spaces: a circuit-side coordinate (`node`, `path`) and
/// the def-space identity (`class.def`, `class.key`) — the identity a bare name
/// list cannot give, and the reason this became a product.
///
/// The extension follows the format, which is why the JSON face is a second
/// file rather than the same one in another spelling.
#[test]
fn the_json_product_carries_the_two_space_row() {
    let root = project("rows");
    let root_arg = root.to_str().expect("root path");
    run_build(
        &root,
        &["build", root_arg, "--product", "inst-list", "-f", "json"],
    );

    let path = root.join("build/inst-list.json");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no product at {}: {e}", path.display()));
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body).expect("the product is JSON");
    assert!(!rows.is_empty(), "the product carries no rows");

    // ⚠ Not "every row has a `def`": pin / bus / label rows have no definition
    // of their own and carry `null` there, which is a fact about the modelling
    // layer, not a defect of the product. What must hold of **every** row is the
    // pair that makes it a row at all — where it is in the circuit, and which
    // class it is.
    for row in &rows {
        for key in ["node", "path", "class"] {
            assert!(row.get(key).is_some(), "row without `{key}`: {row}");
        }
    }

    // And at least one row must carry the full definition identity, or the
    // product would be a name list wearing the contract's keys.
    let identified = rows.iter().filter(|r| {
        r["class"]["def"].is_u64()
            && r["class"]["key"]["ident"].is_string()
            && r["class"]["key"]["uri"].is_string()
    });
    assert!(
        identified.count() > 0,
        "no row carries `class.def` + `class.key`: the def space is missing from the product"
    );
}

/// `bom` is an `ExportKind` -- `mcc export bom` still answers -- but not a
/// build product: it was retired on 2026-09-14, when the pairing moved
/// downstream. Accepting it here would write the retired artifact into
/// `build/`, which is the one outcome the retirement rules out.
#[test]
fn a_retired_product_is_refused_by_name() {
    let root = project("retired");
    let root_arg = root.to_str().expect("root path");
    let (_, err, ok) = run_mcc(&root, &["build", root_arg, "--product", "bom"]);
    assert!(!ok, "a retired product was accepted: {err}");
    assert!(
        err.contains("bom"),
        "the refusal does not name the id it refuses: {err}"
    );
    assert!(
        !root.join("build").exists(),
        "the refusing build wrote a product anyway"
    );
}

/// A directory batch has no single build to project and no manifest to hang a
/// `build/` off, so it refuses. It must not report success while writing nothing.
#[test]
fn a_directory_batch_refuses_a_product() {
    let root = project("batch");
    // Remove the manifest: what is left is a folder of `.mc` files, which the
    // build command treats as a batch of independent targets.
    std::fs::remove_file(root.join("project.toml")).expect("remove manifest");
    let root_arg = root.to_str().expect("root path");

    let (_, err, ok) = run_mcc(&root, &["build", root_arg, "--product", "inst-list"]);
    assert!(
        !ok,
        "a directory batch accepted --product and reported success: {err}"
    );
    assert!(
        err.contains("--product"),
        "the refusal does not name the flag it is refusing: {err}"
    );
    assert!(
        !root.join("build").exists(),
        "the refusing build wrote a product anyway"
    );
}
