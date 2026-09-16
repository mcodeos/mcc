// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! A workspace is identified by its **root**, never by its directory name.
//!
//! Regression for the reported false positive:
//!
//! ```text
//! CMIE module 'main' defined in both
//!   '…/mcc/tests/fixtures/hbl/src/hbl.mc' and
//!   '…/mcs/hbl/src/hbl.mc'. The latter shadows the former.   [E5001]
//! ```
//!
//! Both directories basename to `hbl`. While the workspace identity *was* that
//! basename, opening the second project did not create a second world — it
//! merged the second project's definitions into the first project's live
//! tables, and the cross-file duplicate check then reported every name the two
//! projects happen to share (starting with `module main`, which every project
//! has).
//!
//! The two projects here are unrelated throwaway directories that share only a
//! name. Nothing about the *content* may be special-cased to keep them apart.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::path::PathBuf;

use mcc::{McURI, WorkspaceKind};

/// Write a minimal self-contained project into `root` and return `root` in
/// canonical form. Nothing in it needs the system library to resolve.
fn write_project(root: &std::path::Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(root.join("src")).expect("create temp project dir");
    std::fs::write(
        root.join("project.toml"),
        format!(
            "[project]\nname = \"{name}\"\nversion = \"1.0.0\"\n\
             entry = \"src/hbl.mc\"\ntop_module = \"main\"\n"
        ),
    )
    .expect("write project.toml");
    std::fs::write(
        root.join("src/hbl.mc"),
        "component RES2\n{\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\
         \nmodule main\n{\n    RES2 R1\n    RES2 R2\n\n    R1.A -> R2.A\n    R1.B -> R2.B\n}\n",
    )
    .expect("write src/hbl.mc");
    root.canonicalize().expect("canonicalize temp project")
}

/// A throwaway project whose directory is named `hbl`, under a parent that
/// distinguishes it from its twin. `parent` must be unique per project.
fn temp_project(parent: &str) -> PathBuf {
    let base = temp_base(parent);
    let _ = std::fs::remove_dir_all(&base);
    write_project(&base.join("hbl"), parent)
}

/// A throwaway directory under the temp dir, named after `tag`.
fn temp_base(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mcc-wsident-{tag}-{}", std::process::id()))
}

/// Canonical form, so the comparison against URIs the loader recorded is
/// spelling-independent (on macOS `/var` is a symlink to `/private/var`).
fn canonical(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
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

fn entry_uri(root: &PathBuf) -> McURI {
    McURI::from(root.join("src/hbl.mc").to_string_lossy().to_string())
}

#[test]
fn workspace__root_is_the_identity_not_the_directory_name() {
    let _lock = common::lock();
    common::init_no_lib();
    // Start from a known world (the anonymous one) so the assertions below do
    // not depend on which test ran first in this process.
    mcc::workspace_switch_to(None, WorkspaceKind::Project);

    let a = temp_project("a");
    let b = temp_project("b");
    assert_eq!(
        a.file_name(),
        b.file_name(),
        "the test is only meaningful while the two roots share a basename"
    );
    assert_ne!(a, b);

    // World A.
    assert!(
        mcc::workspace_switch_to(Some(a.clone()), WorkspaceKind::Project),
        "first switch to a new root is a change"
    );
    mcc::mcc_load_project(&entry_uri(&a));
    let uris = module_uris();
    assert!(
        uris.iter().any(|u| u.contains(&canonical(&a))),
        "world A's module is loaded: {uris:?}"
    );

    // World B — a *different* root that happens to be named `hbl` too.
    assert!(
        mcc::workspace_switch_to(Some(b.clone()), WorkspaceKind::Project),
        "a second root with the same basename is a different world"
    );
    mcc::mcc_load_project(&entry_uri(&b));
    let uris = module_uris();
    assert!(
        uris.iter().any(|u| u.contains(&canonical(&b))),
        "world B's module is loaded: {uris:?}"
    );
    assert!(
        !uris.iter().any(|u| u.contains(&canonical(&a))),
        "world A's module leaked into world B — this is the reported E5001: {uris:?}"
    );

    // Back to world A: its tables return, and B's do not.
    assert!(
        mcc::workspace_switch_to(Some(a.clone()), WorkspaceKind::Project),
        "switching back is a change"
    );
    let uris = module_uris();
    assert!(
        uris.iter().any(|u| u.contains(&canonical(&a))),
        "world A's module is restored: {uris:?}"
    );
    assert!(
        !uris.iter().any(|u| u.contains(&canonical(&b))),
        "world B's module leaked into world A: {uris:?}"
    );

    // A repeat call for the active root is a no-op, so opening files one after
    // another in one project never churns the world.
    assert!(
        !mcc::workspace_switch_to(Some(a.clone()), WorkspaceKind::Project),
        "the active root is already active"
    );
}

/// The same defect through the path the editor actually takes: `sem` on a file
/// auto-loads its project, and the auto-load decides which world that file
/// belongs to. The caller used to compare roots while the callee it delegated
/// to compared directory names — they disagreed in exactly this case.
#[test]
fn workspace__auto_load_does_not_merge_same_basename_projects() {
    let _lock = common::lock();
    common::init_no_lib();
    mcc::workspace_switch_to(None, WorkspaceKind::Project);

    let a = temp_project("sema");
    let b = temp_project("semb");

    // The editor sets the opened folder as the project root (`set_project_root`,
    // what the client calls on a folder change), then asks for a file's
    // semantic data. `find_project_root` prefers that configured root.
    for root in [&a, &b] {
        mcc::mcc_set_project_root(root);
        let uri = entry_uri(root).to_string();
        mcc::rpc::handlers::handle_sem(Some(serde_json::json!({ "uri": uri })))
            .unwrap_or_else(|e| panic!("sem on {}: {e:?}", uri));
    }

    // B is the active world; only B's definitions may be in the tables.
    let uris = module_uris();
    assert!(
        uris.iter().any(|u| u.contains(&canonical(&b))),
        "world B's module is loaded: {uris:?}"
    );
    assert!(
        !uris.iter().any(|u| u.contains(&canonical(&a))),
        "world A's module survived B's auto-load — this is the reported E5001: {uris:?}"
    );
}

/// The reported case as the editor produces it: one folder is open, and a file
/// in a *sibling* folder is opened alongside it.
///
/// The open folder is the editor's working root — but only for the files under
/// it. When it claimed every file the editor opened, the second project's
/// definitions joined the first project's world, and since both are named `hbl`
/// the duplicate check reported `module main` in both (E5001). The two projects
/// here differ in nothing but their path, which is the whole point.
#[test]
fn workspace__a_file_outside_the_open_folder_keeps_its_own_world() {
    let _lock = common::lock();
    common::init_no_lib();
    mcc::workspace_switch_to(None, WorkspaceKind::Project);

    // The folder the editor has open, holding a project named `hbl`…
    let open = temp_base("open");
    let _ = std::fs::remove_dir_all(&open);
    let inside = write_project(&open.join("hbl"), "inside");
    let open = open.canonicalize().expect("canonicalize open folder");

    // …and a different project, named `hbl` too, somewhere else entirely.
    let outside = temp_project("sibling");
    assert_eq!(inside.file_name(), outside.file_name());
    assert!(
        !outside.starts_with(&open),
        "the second project must sit outside the open folder"
    );

    mcc::mcc_set_project_root(&open);
    for root in [&inside, &outside] {
        let uri = entry_uri(root).to_string();
        mcc::rpc::handlers::handle_sem(Some(serde_json::json!({ "uri": uri })))
            .unwrap_or_else(|e| panic!("sem on {uri}: {e:?}"));
    }

    // The sibling is active, on its own root; the open folder's world is parked.
    let uris = module_uris();
    assert!(
        uris.iter().any(|u| u.contains(&canonical(&outside))),
        "the sibling's module is loaded: {uris:?}"
    );
    assert!(
        !uris.iter().any(|u| u.contains(&canonical(&inside))),
        "a project outside the open folder joined its world — this is the reported E5001: {uris:?}"
    );
    assert_eq!(
        mcc::workspace_root().as_deref(),
        Some(outside.as_path()),
        "the active world is the sibling's root, not the open folder"
    );
}
