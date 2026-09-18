// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! A dedup id must move when the payload it identifies moves (CIMP §1 U94).
//!
//! Two ids in the `sem` path are read as "the data is unchanged, skip the
//! recompute". Neither promised that:
//!
//! * `result_id` — the one the extension keys its token and symbol caches on —
//!   hashed four scalars of the token stream (count, total length, first and
//!   last position) while gating a *symbol-table* rebuild too.
//! * `ref_def_map.result_id` hashed three array lengths plus four fields of one
//!   entry, leaving `def_name`, `ref_id`, `def_kind`, `container_id` and
//!   `cmie_kind` outside the id.
//!
//! Both are now the content fingerprint of the payload they are sent with
//! ([`mcc::ast::sem`]'s `payload_fingerprint`), which is the property these
//! tests pin. Measured on the real fixture before the change: the token id
//! missed all three renames below and the map id missed two of them, each time
//! with the payload demonstrably different — a skip that keeps stale symbols
//! and reports nothing.
//!
//! ⚠ The rename must be **equal length**: that is what leaves the token stream
//! (`(type, position, length)` per token) with the same count, total length and
//! endpoints, which is exactly the case the old ids could not see. A test that
//! renamed across lengths would have passed against the defect.
//!
//! The fixture is copied to a scratch directory first — the workspace is global
//! state and these tests edit source files.

use std::path::PathBuf;
use std::sync::Mutex;

/// The `mcc_*` workspace is global state, so tests in this file are serialized
/// (same discipline as `root_layer_anchor.rs`). The shard runs with
/// `--test-threads=1` besides.
static LOCK: Mutex<()> = Mutex::new(());

fn hbl_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// A fresh directory for this run, so a previous run cannot be read as state.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-dedupid-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).expect("create target dir");
    for entry in std::fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("read dir entry");
        let to = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy file");
        }
    }
}

/// The five sources of the fixture, as `(path, pristine text)`.
fn sources(proj: &std::path::Path) -> Vec<(String, String)> {
    ["hbl.mc", "power.mc", "us513.mc", "periph.mc", "bom.mc"]
        .iter()
        .map(|f| {
            let p = proj.join("src").join(f);
            (
                p.to_string_lossy().to_string(),
                std::fs::read_to_string(&p).expect("read fixture source"),
            )
        })
        .collect()
}

/// Rewrite every source through `edit`, reload the project and return the `sem`
/// response.
///
/// The edit is applied to **all** sources, so a rename stays a rename of one
/// symbol and does not turn into a batch of unresolved references.
fn sem_after_edit(
    pristine: &[(String, String)],
    entry: &String,
    edit: impl Fn(&str) -> String,
) -> serde_json::Value {
    for (path, text) in pristine {
        std::fs::write(path, edit(text)).expect("write edited source");
    }
    for (path, _) in pristine {
        mcc::mcc_remove(path);
    }
    mcc::mcc_load_project(entry);
    mcc::rpc::handlers::try_lookup_sem(&[entry.to_string()]).expect("sem payload")
}

fn token_id(sem: &serde_json::Value) -> serde_json::Value {
    sem["result_id"].clone()
}

fn map_id(sem: &serde_json::Value) -> serde_json::Value {
    sem["symbols"]["ref_def_map"]["result_id"].clone()
}

fn symbols(sem: &serde_json::Value) -> String {
    sem["symbols"].to_string()
}

/// Load the scratch copy of the fixture and return `(project root, entry uri)`,
/// with the pristine sources already read.
fn loaded_fixture(name: &str) -> (PathBuf, String, Vec<(String, String)>) {
    let proj = scratch(name).join("hbl");
    copy_dir(&hbl_dir(), &proj);
    let entry = proj.join("src/hbl.mc").to_string_lossy().to_string();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&proj);
    mcc::mcc_load_project(&entry);

    let pristine = sources(&proj);
    (proj, entry, pristine)
}

/// An equal-length rename of a real symbol moves both ids.
///
/// Three renames, because they land in different places: a class name used
/// across files (`MIC`), a class name whose instances are referenced (`FLASH`)
/// and an instance declaration (`SPEAKER_M`). All three left the token id fixed
/// before this change, and the latter two left the map id fixed as well.
#[test]
fn an_equal_length_rename_moves_the_dedup_ids() {
    let _guard = LOCK.lock().unwrap();
    let (_proj, entry, pristine) = loaded_fixture("rename");

    let baseline = mcc::rpc::handlers::try_lookup_sem(&[entry.clone()]).expect("sem payload");
    let base_symbols = symbols(&baseline);
    let base_token_id = token_id(&baseline);
    let base_map_id = map_id(&baseline);

    for (label, needle, repl) in [
        ("MIC -> MIX", "MIC", "MIX"),
        ("FLASH -> FLSHX", "FLASH", "FLSHX"),
        ("SPEAKER_M -> SPEAKER_N", "SPEAKER_M", "SPEAKER_N"),
    ] {
        assert_eq!(
            needle.len(),
            repl.len(),
            "{label}: the rename must keep the byte length"
        );
        let after = sem_after_edit(&pristine, &entry, |t| t.replace(needle, repl));
        assert_ne!(
            symbols(&after),
            base_symbols,
            "{label}: the rename did not move the symbol payload, so this case \
             proves nothing about the id"
        );
        assert_ne!(
            token_id(&after),
            base_token_id,
            "{label}: the payload moved and `result_id` did not, so the \
             extension skips the rebuild and keeps the previous file's symbols"
        );
        assert_ne!(
            map_id(&after),
            base_map_id,
            "{label}: the payload moved and `ref_def_map.result_id` did not"
        );
    }
}

/// The control: the id does not move when nothing moved.
///
/// Without this an id that always changes would pass the test above, and every
/// load would recompute — the defect traded for the performance the dedup
/// exists to buy. Reloading the same sources is the case the consumer is
/// actually asking about when it reads the id.
#[test]
fn a_reload_without_edits_keeps_the_dedup_ids() {
    let _guard = LOCK.lock().unwrap();
    let (_proj, entry, pristine) = loaded_fixture("reload");

    let first = mcc::rpc::handlers::try_lookup_sem(&[entry.clone()]).expect("sem payload");
    let second = sem_after_edit(&pristine, &entry, |t| t.to_string());

    assert_eq!(
        symbols(&second),
        symbols(&first),
        "an unedited reload must reproduce the symbol payload"
    );
    assert_eq!(
        token_id(&second),
        token_id(&first),
        "an unedited reload moved `result_id`"
    );
    assert_eq!(
        map_id(&second),
        map_id(&first),
        "an unedited reload moved `ref_def_map.result_id`"
    );
}
