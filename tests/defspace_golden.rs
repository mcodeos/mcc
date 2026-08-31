// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase 0 golden samples (defspace-refactor-implementation.md Phase 0).
//!
//! These tests lock the pre-refactor behavior of the definition tables
//! BEFORE any read/write consolidation work starts ("first lock, then
//! change"). Each sample pins one observable behavior that the refactor
//! (Phases 1-3: read-side, write-side, single-table merge) must preserve:
//!
//! - P0.1: two-table coexistence and identity addressing — a project def and
//!   a system-lib def with the same name coexist under distinct (ident, uri)
//!   identities; the unified get_* lookup resolves each identity to its own
//!   table entry and the P5 system view exposes only the global (system-lib)
//!   table. (The workspace-first precedence itself is unit-tested in
//!   `src/db/defspace.rs` against a synthetic same-key collision, because the
//!   write side `remove_defines` cannot leave both tables holding one key.)
//! - P0.2: mcbase split — system-lib defs land in the global tables (P5
//!   visible), project defs land in the workspace tables (unified view only).
//! - P0.4: library load/unload symbol behavior — mcode defs keep global
//!   auto-visibility; third-party defs are removed from both tables after
//!   load (use-only visibility) while the boundary + symbol ledger stay
//!   recorded; unload drops the boundary.
//! - P0.5: reverse_deps — the "who uses me" file index (design §7.6) is
//!   rebuilt from the use table and survives a re-parse of the used file.
//!
//! P0.3 (method dispatch) is already locked by `tablea_dispatch_regression.rs`;
//! P0.6 is the baseline ledger recorded by the full regression run.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock() -> std::sync::MutexGuard<'static, ()> {
    // A panicked test poisons the shared lock; later tests must still run.
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Reset the mcc_* workspace for one test. The caller must hold `TEST_LOCK`.
fn reset_workspace() {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(Path::new(""));
    mcc::mcc_clear_workspace();
}

/// Component source with pins numbered 1..=len.
fn component_src(name: &str, pins: &[&str]) -> String {
    let lines: Vec<String> = pins
        .iter()
        .enumerate()
        .map(|(i, n)| format!("        {} = {}", i + 1, n))
        .collect();
    format!(
        "component {name}\n{{\n    pins = [\n{}\n    ]\n}}\n",
        lines.join("\n")
    )
}

/// Fresh temp root holding one library `<root>/<lib>/<lib>.mc` defining
/// `comp` with `pins`.
fn temp_lib_root(tag: &str, lib: &str, comp: &str, pins: &[&str]) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("mcc-defspace-golden-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lib_dir = dir.join(lib);
    std::fs::create_dir_all(&lib_dir).unwrap();
    std::fs::write(lib_dir.join(format!("{lib}.mc")), component_src(comp, pins)).unwrap();
    // Canonical root so the loader's prefix matching (blib spacer-name
    // collection) sees the same path form as the defs' canonical uris.
    dir.canonicalize().unwrap_or(dir)
}

/// Canonical absolute path. Resolves /var -> /private/var on macOS so the
/// test's own keys match the loader's canonicalized keys.
fn canon(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// P0.1: a project def and a system-lib def with the SAME name coexist under
/// distinct (ident, uri) identities. Each identity resolves through the
/// unified get_* lookup to its own table entry; the P5 system view exposes
/// only the global (system-lib) table; the unified all_* enumeration keeps
/// both entries (two-table coexistence).
#[test]
fn p01_two_table_coexistence_and_identity_addressing() {
    let _lock = lock();
    reset_workspace();

    // Self-contained mcode library on disk: GOLD_LED with 1 pin.
    let root = temp_lib_root("p01", "mcode", "GOLD_LED", &["A"]);
    mcc::mcc_set_system_root(&root);
    assert!(
        mcc::mcb_load_lib("mcode", &root.join("mcode")),
        "mcode lib loads"
    );

    // Project file declaring the same-named component under a virtual uri.
    let p_uri = "/virtual/p01_gold_led.mc".to_string();
    mcc::mcc_load_from_string(&p_uri, &component_src("GOLD_LED", &["A", "K"]));

    let ds = mcc::definition_space();
    let l_uri = canon(&root.join("mcode").join("mcode.mc"));
    let sn_p = mcc::McSpaceName::new(&mcc::McIds::from("GOLD_LED"), p_uri.clone());
    let sn_l = mcc::McSpaceName::new(&mcc::McIds::from("GOLD_LED"), l_uri.clone());

    // Project identity -> the workspace (project) def.
    let proj = ds.get_component(&sn_p).expect("project identity resolves");
    assert_eq!(proj.uri, p_uri, "project def is the workspace entry");
    assert_eq!(proj.pins.pins.len(), 2);

    // Library identity -> the global (system-lib) def.
    let lib = ds.get_component(&sn_l).expect("library identity resolves");
    assert_eq!(lib.uri, l_uri, "library def is the system-lib entry");
    assert_eq!(lib.pins.pins.len(), 1);

    // P5 system view reflects ONLY the global table: the library entry is
    // present, the project entry (workspace table) is not.
    let sys_components = ds.system_components();
    let sys_hits: Vec<_> = sys_components
        .iter()
        .filter(|(k, _)| k.ident.to_string() == "GOLD_LED")
        .collect();
    assert_eq!(
        sys_hits.len(),
        1,
        "system view holds the single library identity"
    );
    assert_eq!(sys_hits[0].1.uri, l_uri);

    // Unified enumeration keeps BOTH identities (two-table coexistence).
    let all_components = ds.all_components();
    let all_hits: Vec<_> = all_components
        .iter()
        .filter(|(k, _)| k.ident.to_string() == "GOLD_LED")
        .collect();
    assert_eq!(
        all_hits.len(),
        2,
        "both table entries coexist in the unified view"
    );
}

/// P0.2: mcbase split — a system-lib def lands in the GLOBAL tables (visible
/// through the P5 system view, `SourceDomain::SystemLib`); a project def
/// lands in the WORKSPACE tables (invisible to the system view, visible to
/// the unified view, `SourceDomain::Project`).
#[test]
fn p02_mcbase_split_system_lib_to_global_project_to_workspace() {
    let _lock = lock();
    reset_workspace();

    let root = temp_lib_root("p02", "mcode", "GOLD_LED", &["A"]);
    mcc::mcc_set_system_root(&root);
    assert!(mcc::mcb_load_lib("mcode", &root.join("mcode")));

    // System-lib side: def lands in the global tables.
    let ds = mcc::definition_space();
    let l_uri = canon(&root.join("mcode").join("mcode.mc"));
    assert!(
        ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_LED"),
        "system-lib def lands in the global tables (P5 visible)"
    );
    assert_eq!(
        ds.source_of(&l_uri),
        Some(mcc::SourceDomain::SystemLib("mcode".into())),
        "library file carries its load domain"
    );

    // Project side: def lands in the workspace tables.
    let p_uri = "/virtual/p02_gold_res.mc".to_string();
    mcc::mcc_load_from_string(&p_uri, &component_src("GOLD_RES", &["A"]));
    let ds = mcc::definition_space();
    let sn_p = mcc::McSpaceName::new(&mcc::McIds::from("GOLD_RES"), p_uri.clone());
    assert_eq!(ds.source_of(&p_uri), Some(mcc::SourceDomain::Project));
    assert!(
        !ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_RES"),
        "project def is NOT in the global tables (mcbase=false)"
    );
    assert!(
        ds.get_component(&sn_p).is_some(),
        "project def resolves through the unified view"
    );
    assert!(
        ds.all_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_RES"),
        "project def is visible in the workspace table"
    );
}

/// P0.4: library load/unload symbol behavior — mcode defs keep global
/// auto-visibility; third-party defs are removed from BOTH tables right after
/// load (use-only visibility) while the lib boundary + symbol ledger stay
/// recorded; unload drops the boundary and the blib entry.
#[test]
fn p04_lib_load_unload_symbol_visibility() {
    let _lock = lock();
    reset_workspace();

    // mcode: global auto-visibility.
    let mcode_root = temp_lib_root("p04-mcode", "mcode", "GOLD_LED", &["A"]);
    mcc::mcc_set_system_root(&mcode_root);
    assert!(mcc::mcb_load_lib("mcode", &mcode_root.join("mcode")));
    let ds = mcc::definition_space();
    assert!(
        !ds.system_components().is_empty(),
        "mcode defs keep global visibility"
    );
    assert!(mcc::mcb_loaded_libs().contains(&"mcode".to_string()));

    // Third-party lib: defs removed after load (use-only), boundary kept.
    let acme_root = temp_lib_root("p04-acme", "acme", "GOLD_BTN", &["A"]);
    assert!(mcc::mcb_load_lib("acme", &acme_root.join("acme")));
    let ds = mcc::definition_space();
    assert!(ds.lib("acme").is_some(), "lib boundary recorded");
    assert!(mcc::mcb_loaded_libs().contains(&"acme".to_string()));
    let info = mcc::mcb_lib_info("acme").expect("blib symbol ledger exists");
    assert!(
        info.total_symbols >= 1,
        "blib ledger keeps the def (collected before the table removal)"
    );
    assert!(
        !ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_BTN"),
        "third-party def removed from the global tables (use-only visibility)"
    );
    assert!(
        !ds.all_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_BTN"),
        "third-party def removed from the workspace tables too"
    );

    // Unload: boundary + blib entry disappear; mcode is untouched.
    assert!(mcc::mcb_unload_lib("acme"));
    let ds = mcc::definition_space();
    assert!(ds.lib("acme").is_none(), "unload drops the lib boundary");
    assert!(!mcc::mcb_loaded_libs().contains(&"acme".to_string()));
    assert!(
        ds.lib("mcode").is_some(),
        "unloading acme leaves mcode untouched"
    );
}

/// P0.5: reverse_deps — the "who uses me" file index (design §7.6) is built
/// from the use table and survives a re-parse of the used file, so the LSP
/// dirty-file propagation can find the affected files.
#[test]
fn p05_reverse_deps_tracks_who_uses_me() {
    let _lock = lock();
    reset_workspace();

    // Real files on disk: `use` resolution requires the target on disk.
    let dir = std::env::temp_dir().join(format!("mcc-defspace-golden-p05-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("b.mc"), component_src("GOLD_IC", &["A"])).unwrap();
    std::fs::write(
        dir.join("a.mc"),
        "use ./b.mc\n\nmodule main\n{\n    io A\n    io GND\n    A -> GND\n}\n",
    )
    .unwrap();

    let b_uri = canon(&dir.join("b.mc"));
    let a_uri = canon(&dir.join("a.mc"));
    mcc::mcc_load_from_string(&b_uri, &component_src("GOLD_IC", &["A"]));
    mcc::mcc_load_from_string(&a_uri, &std::fs::read_to_string(dir.join("a.mc")).unwrap());

    let ds = mcc::definition_space();
    assert_eq!(
        ds.reverse_deps(&b_uri),
        Some(vec![a_uri.clone()]),
        "a uses b: reverse deps records the edge"
    );

    // Re-parse b with new content: the use index must survive.
    mcc::mcc_load_from_string(&b_uri, &component_src("GOLD_IC", &["A", "B"]));
    let ds = mcc::definition_space();
    assert_eq!(
        ds.reverse_deps(&b_uri),
        Some(vec![a_uri]),
        "reverse deps survive a re-parse of the used file"
    );
}
