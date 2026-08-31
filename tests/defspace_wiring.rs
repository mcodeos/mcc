// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! DefinitionSpace end-to-end wiring (design §12.1).
//!
//! The loader chain (`mcc_load_from_string` → `mcb_add_from_string`) records
//! each loaded source into the workspace source manifest; the [`DefinitionSpace`]
//! view reads that manifest. These tests lock the wiring through the real global
//! workspace — the lib unit tests in `src/db/cmie/defspace.rs` deliberately avoid
//! the global (the parallel mc_code/buildcmd tests share it), so this separate
//! binary (own process, serialized by `TEST_LOCK`) is where the global path is
//! exercised end to end.

use std::path::Path;
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Reset the mcc_* workspace for one test. The caller must hold `TEST_LOCK`.
fn reset_workspace() {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(Path::new(""));
    mcc::mcc_clear_workspace();
}

/// A project file loaded through the loader chain is recorded in the manifest
/// as a project source and is visible through the definition space.
#[test]
fn loaded_project_source_is_in_the_manifest() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();

    let uri = "/mcc/defspace-wiring.mc".to_string();
    let src = "module main {\n    io A\n    io GND\n    A -> GND\n}";
    mcc::mcc_load_from_string(&uri, src);

    let ds = mcc::definition_space();
    assert_eq!(
        ds.source_of(&uri),
        Some(mcc::SourceDomain::Project),
        "mcc_load_from_string must record the file as a project source"
    );
    assert!(
        ds.is_project_source(&uri),
        "a project source is not a system lib"
    );
    assert!(
        ds.sources().any(|(u, _)| u == uri),
        "the manifest enumerates the loaded file"
    );

    // The loaded definitions resolve through the unified view (workspace first).
    let sn = mcc::McSpaceName::new(&mcc::McIds::from("main"), uri);
    assert!(
        ds.get_module(&sn).is_some(),
        "a loaded module definition resolves through the definition space"
    );
}

/// Clearing the workspace wipes the source manifest along with the definition
/// tables — nothing is left behind for the next load.
#[test]
fn clear_workspace_wipes_the_manifest() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    reset_workspace();

    let uri = "/mcc/defspace-wiring-clear.mc".to_string();
    mcc::mcc_load_from_string(&uri, "module main {\n    io A\n}");
    assert!(mcc::definition_space().is_project_source(&uri));

    mcc::mcc_clear_workspace();

    let ds = mcc::definition_space();
    assert_eq!(
        ds.source_of(&uri),
        None,
        "clearing the workspace must clear the source manifest"
    );
    assert_eq!(ds.sources().count(), 0, "no sources remain after clear");
    assert_eq!(ds.libs().count(), 0, "no lib boundaries remain after clear");
}
