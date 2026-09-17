// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! A directory is a **container of entries**, not a definition space.
//!
//! Regression for the second shape of the reported false positive. The first
//! (a workspace identified by its directory *basename*, `workspace_identity.rs`)
//! is a different cause of the same symptom; this one is about the many projects
//! a single folder holds:
//!
//! ```text
//! $ mcc check /tmp/batchrepro          # two unrelated projects, one folder
//! CMIE component 'RA' defined in both '…/projB/src/main.mc' and '…/projA/src/main.mc'
//! CMIE module 'main' defined in both '…/projB/src/main.mc' and '…/projA/src/main.mc'
//! ```
//!
//! Both files declare `component RA` and `module main` because both are
//! complete little projects, and a folder of complete little projects is
//! exactly what `tests/` in this repo is. The batch used to load every `.mc`
//! file under the target into **one** definition space, so the folder's files
//! were checked against each other and every shared name was reported.
//!
//! What a folder means is decided in one place (`discover_entries`): a
//! subdirectory a `project.toml` names is one entry, and each `.mc` file
//! elsewhere is one. Nothing about a file's name, depth or content may be
//! special-cased to keep it out of the batch or out of a sibling's space.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::path::{Path, PathBuf};

use mcc::check::duplicate::DuplicateCmieCheck;
use mcc::check::{CheckAccumulator, ValidationCheck};
use mcc::{BuildEntry, WorkspaceKind};

/// A complete, self-contained two-instance module plus the component it uses.
///
/// Deliberately identical in every project the tests write: the duplicate check
/// fires on shared *names*, so a folder of these is the reported case, and any
/// separation the loader achieves has to come from the container being read
/// correctly rather than from the files happening to differ.
const PROJECT: &str = "component RA\n{\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\
                       \nmodule main\n{\n    RA R1\n    RA R2\n\n    R1.A -> R2.A\n    R1.B -> R2.B\n}\n";

/// A module instantiating a class nothing defines, so the file carries a
/// diagnostic whatever else changes.
const WARNS: &str = "module SHARED\n{\n    NoSuchClass u1\n}\n";

fn temp_base(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mcc-entry-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Write `PROJECT` to `dir/<rel>`, creating parents.
fn write_mc(dir: &Path, rel: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("entry file has a parent"))
        .expect("create entry dir");
    std::fs::write(&path, PROJECT).expect("write .mc file");
}

/// Write a manifest naming `entry` (relative to `dir`).
fn write_manifest(dir: &Path, name: &str, entry: &str) {
    std::fs::create_dir_all(dir).expect("create manifest dir");
    std::fs::write(
        dir.join("project.toml"),
        format!(
            "[project]\nname = \"{name}\"\nversion = \"1.0.0\"\n\
             entry = \"{entry}\"\ntop_module = \"main\"\n"
        ),
    )
    .expect("write project.toml");
}

/// Entry worlds as strings, for spelling-independent comparison (on macOS
/// `/var` is a symlink to `/private/var`).
fn worlds(entries: &[BuildEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e.world.to_string_lossy().to_string())
        .collect()
}

/// URIs of every module in the active definition space — the table the
/// duplicate check reads, and therefore what E5001 is computed from.
fn module_uris() -> Vec<String> {
    mcc::definition_space()
        .workspace_modules()
        .iter()
        .map(|(sn, _)| sn.uri.to_string())
        .collect()
}

/// The E5001 rows the cross-file duplicate check would report for the active
/// world. Asking the check directly rather than reading the diagnostic store
/// keeps the assertion on the code the report carries, independent of which
/// files a load round happened to re-derive.
fn cross_file_duplicates() -> Vec<u32> {
    let mut acc = CheckAccumulator::new();
    DuplicateCmieCheck.run_post_parse(&mut acc);
    acc.results
        .iter()
        .filter(|r| r.code == mcc::errcodes::DUP_CMIE_CROSS_FILE)
        .map(|r| r.code)
        .collect()
}

/// Every diagnostic the active world carries, keyed the way a merged report
/// keys them: `(code, uri, pos, message)`.
fn diag_keys() -> Vec<(u32, String, u32, String)> {
    let mut keys: Vec<_> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.loc.uri.to_string(), d.loc.pos, d.msg.clone()))
        .collect();
    keys.sort();
    keys
}

// ── The container ──

/// Two unrelated projects in one folder are two definition spaces, and neither
/// reports the other's names.
///
/// This is `/tmp/batchrepro` reduced: `projA/src/main.mc` and
/// `projB/src/main.mc`, each declaring `component RA` and `module main`.
#[test]
fn entry__a_directory_is_a_container_of_one_world_per_file() {
    let _lock = common::lock();
    common::init_no_lib();
    mcc::workspace_switch_to(None, WorkspaceKind::Project);

    let root = temp_base("container");
    write_mc(&root, "projA/src/main.mc");
    write_mc(&root, "projB/src/main.mc");

    let entries = mcc::discover_entries(&root, None);
    assert_eq!(
        entries.len(),
        2,
        "one world per `.mc` file, not one for the folder: {entries:#?}"
    );
    let worlds = worlds(&entries);
    assert_ne!(
        worlds[0], worlds[1],
        "two files must never share a world — a shared key is how they merge"
    );

    // Each is built in its own world, and the duplicate check — which reads the
    // live world's tables — sees one `main` at a time.
    let mut seen = Vec::new();
    mcc::mcc_for_each_entry(&root, None, &|_| vec![], |e| {
        assert_eq!(
            cross_file_duplicates(),
            Vec::<u32>::new(),
            "'{}' is reported as duplicating a sibling's definitions — the false E5001",
            e.entry.display()
        );
        let uris = module_uris();
        assert_eq!(
            uris.len(),
            1,
            "the active world holds exactly this entry's module: {uris:?}"
        );
        seen.push(uris[0].clone());
    });
    seen.sort();
    assert_eq!(seen.len(), 2, "both entries were visited");
    assert_ne!(seen[0], seen[1], "each world reported its own module");

    let _ = std::fs::remove_dir_all(&root);
}

/// A `project.toml` names one entry for the directory it sits in — and the walk
/// stops there, because the project's other `.mc` files belong to that entry's
/// `use` closure and not to worlds of their own.
#[test]
fn entry__a_project_manifest_names_one_entry_and_is_not_descended() {
    let _lock = common::lock();
    common::init_no_lib();
    mcc::workspace_switch_to(None, WorkspaceKind::Project);

    let root = temp_base("manifest");
    write_manifest(&root.join("proj"), "proj", "src/main.mc");
    write_mc(&root, "proj/src/main.mc");
    write_mc(&root, "proj/src/extra.mc");
    write_mc(&root, "loose.mc");

    let entries = mcc::discover_entries(&root, None);
    assert_eq!(
        entries.len(),
        2,
        "the project is one entry and the loose file is another: {entries:#?}"
    );

    let project = root.join("proj").canonicalize().expect("canonicalize proj");
    let manifest_entry = entries
        .iter()
        .find(|e| e.world == project)
        .unwrap_or_else(|| panic!("the manifest did not name the project dir: {entries:#?}"));
    assert_eq!(
        manifest_entry.entry,
        project.join("src/main.mc"),
        "the entry is the file the manifest names"
    );
    assert_eq!(
        manifest_entry.scope, project,
        "a project resolves against its own root"
    );

    let loose = entries
        .iter()
        .find(|e| e.world != project)
        .expect("the loose file is its own entry");
    assert_eq!(
        loose.entry,
        root.canonicalize()
            .expect("canonicalize root")
            .join("loose.mc"),
        "the loose entry is the file itself"
    );
    assert!(
        !entries.iter().any(|e| e.entry.ends_with("extra.mc")),
        "a file inside the project must not become a world: the project's `.mc` \
         files are its entry's closure: {entries:#?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A loose entry resolves against **the directory that was walked**, which is
/// always a directory.
///
/// A file-valued scope silently breaks everything that joins a relative path
/// onto the project root — `./`-relative `use` resolution, the canonical URI,
/// and the `symbols/manifest.toml` lookup that SVG symbols come from.
#[test]
fn entry__the_named_root_is_the_resolution_base() {
    let _lock = common::lock();
    common::init_no_lib();

    let root = temp_base("scope");
    write_mc(&root, "deep/nested/leaf.mc");

    let entries = mcc::discover_entries(&root, None);
    assert_eq!(entries.len(), 1, "{entries:#?}");
    let e = &entries[0];
    assert!(
        e.scope.is_dir(),
        "the resolution base must be a directory, not a file: {:?}",
        e.scope
    );
    assert_eq!(
        e.scope,
        root.canonicalize().expect("canonicalize root"),
        "a loose file resolves against the directory that was walked, not its own parent"
    );

    // `--entry` names one entry and replaces the walk.
    let picked = mcc::discover_entries(&root, Some("deep/nested/leaf.mc"));
    assert_eq!(
        picked.len(),
        1,
        "`--entry` is a restriction, not an addition"
    );
    assert_eq!(picked[0].entry, e.entry);

    let _ = std::fs::remove_dir_all(&root);
}

/// A file two entries reach is one file, and this is what lets a folder's report
/// be about the folder: wherever `shared.mc` is reached it is diagnosed the same
/// way, so folding the entries' reports together by
/// `(code, uri, pos, message)` yields one row per problem rather than one per
/// entry that happens to include the file.
#[test]
fn entry__a_shared_file_is_diagnosed_alike_in_every_world_that_reaches_it() {
    let _lock = common::lock();
    common::init_no_lib();
    mcc::workspace_switch_to(None, WorkspaceKind::Project);

    let root = temp_base("merge");
    // `shared.mc` is reached by `user.mc`'s `use` closure and is also — being a
    // `.mc` file in a manifest-free folder — an entry of its own.
    std::fs::write(root.join("shared.mc"), WARNS).expect("write shared.mc");
    std::fs::write(
        root.join("user.mc"),
        "use ./shared.mc\n\nmodule main\n{\n}\n",
    )
    .expect("write user.mc");

    let entries = mcc::discover_entries(&root, None);
    assert_eq!(entries.len(), 2, "{entries:#?}");
    let shared = root
        .canonicalize()
        .expect("canonicalize root")
        .join("shared.mc");

    // The rows each world reports for the shared file, i.e. every row a merge
    // would have to fold.
    let mut per_world: Vec<Vec<(u32, String, u32, String)>> = Vec::new();
    mcc::mcc_for_each_entry(&root, None, &|_| vec![], |_| {
        per_world.push(
            diag_keys()
                .into_iter()
                .filter(|(_, uri, _, _)| Path::new(uri) == shared)
                .collect(),
        );
    });

    assert_eq!(per_world.len(), 2, "both entries were visited");
    for (i, keys) in per_world.iter().enumerate() {
        assert!(
            !keys.is_empty(),
            "entry {i} reached `shared.mc` but reported nothing for it — the \
             closure overlap this test is about did not happen: {per_world:#?}"
        );
    }
    assert_eq!(
        per_world[0], per_world[1],
        "the same file was diagnosed differently in two worlds, so no key can \
         fold them: {per_world:#?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A batch is a detour, not a move: the world the caller came in with is the
/// active one when it returns.
#[test]
fn workspace__a_batch_leaves_the_active_world_where_it_found_it() {
    let _lock = common::lock();
    common::init_no_lib();

    let caller = temp_base("caller")
        .canonicalize()
        .expect("canonicalize caller");
    let batch = temp_base("batchdir");
    write_mc(&batch, "x.mc");
    write_mc(&batch, "y.mc");

    mcc::workspace_switch_to(Some(caller.clone()), WorkspaceKind::Project);
    let before = mcc::workspace_root();
    assert!(
        before.is_some(),
        "the caller has a world for the batch to come back to"
    );

    let mut visited = 0usize;
    mcc::mcc_for_each_entry(&batch, None, &|_| vec![], |_| visited += 1);
    assert_eq!(visited, 2, "the batch visited both entries");
    assert_eq!(
        mcc::workspace_root(),
        before,
        "the batch left the caller's world active"
    );
    assert_eq!(
        mcc::workspace_root(),
        Some(caller.clone()),
        "and it is the caller's world, not one of the batch's"
    );

    let _ = std::fs::remove_dir_all(&caller);
    let _ = std::fs::remove_dir_all(&batch);
}
