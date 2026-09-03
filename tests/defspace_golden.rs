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

/// Phase 5: per-world library loading (defspace-refactor-implementation.md
/// Phase 5, P0.4 assertion extended to two worlds).
///
/// The system-library segment lives in the active world's registry and
/// follows world create / switch. A library loaded in world A must be
/// invisible in world B, and switching back to A must restore its defs —
/// A-world lib changes never pollute B (the stale-server root cause).
#[test]
fn p06_dual_world_lib_isolation() {
    let _lock = lock();
    reset_workspace();

    // World "default": load an mcode-like library with GOLD_LED.
    let root_a = temp_lib_root("p06-a", "mcode", "GOLD_LED", &["A"]);
    mcc::mcc_set_system_root(&root_a);
    assert!(mcc::mcb_load_lib("mcode", &root_a.join("mcode")));
    let ds = mcc::definition_space();
    assert!(
        ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_LED"),
        "world A sees its own loaded lib"
    );

    // Create + switch to world B (fresh): A's lib is gone from the registry.
    let root_b =
        std::env::temp_dir().join(format!("mcc-defspace-golden-p06b-{}", std::process::id()));
    std::fs::create_dir_all(&root_b).unwrap();
    assert!(mcc::workspace_create(
        "worldB",
        mcc::WorkspaceKind::Project,
        &root_b,
    ));
    let ds = mcc::definition_space();
    assert!(
        !ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_LED"),
        "world B does not see world A's library (per-world isolation)"
    );

    // World B loads its own library with GOLD_BTN.
    let root_b2 = temp_lib_root("p06-b", "mcode", "GOLD_BTN", &["A"]);
    mcc::mcc_set_system_root(&root_b2);
    assert!(mcc::mcb_load_lib("mcode", &root_b2.join("mcode")));
    let ds = mcc::definition_space();
    assert!(
        ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_BTN"),
        "world B sees its own loaded lib"
    );

    // Switch back to world A: A's lib is restored, B's lib is gone.
    assert!(mcc::workspace_switch("default"));
    let ds = mcc::definition_space();
    assert!(
        ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_LED"),
        "world A's lib restored on switch back"
    );
    assert!(
        !ds.system_components()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "GOLD_BTN"),
        "world B's lib does not leak into world A"
    );
}

/// build.full regression (Phase 5): a world reset (`mcc_clear_workspace`)
/// tombstones the whole per-world registry, so a fresh world must
/// re-establish the mcode auto-load or its classes (e.g. `enum PKG` from
/// package.mc) go unresolved. `handle_build_full` now does this through
/// `load_libs_rpc`, which auto-includes mcode like the CLI's `collect_libs`.
/// Locked through the real E3157 resolution path (lapper enum refs).
#[test]
fn p07_world_reset_reloads_mcode() {
    let _lock = lock();
    reset_workspace();

    // Self-contained mcode library: entry file aggregates an enum file
    // (mirrors the real mcode lib where mcode.mc pub-uses package.mc).
    let dir = std::env::temp_dir().join(format!("mcc-defspace-golden-p07-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mcode_dir = dir.join("mcode");
    std::fs::create_dir_all(&mcode_dir).unwrap();
    std::fs::write(mcode_dir.join("mcode.mc"), "pub use ./package.mc\n").unwrap();
    std::fs::write(
        mcode_dir.join("package.mc"),
        "enum PKG\n{\n    DIP8,\n    SOIC8\n}\n",
    )
    .unwrap();
    let root = dir.canonicalize().unwrap_or(dir);
    mcc::mcc_set_system_root(&root);
    assert!(mcc::mcb_load_lib("mcode", &root.join("mcode")));

    // A project file referencing the mcode enum from an attribute value
    // (the same shape as mclibs/clock/mcp7940m.mc's `package = PKG.DIP8`).
    let p_uri = "/virtual/p07_pkg.mc".to_string();
    let src = "component X\n{\n    package = PKG.DIP8\n}\n";

    // With mcode loaded the enum class resolves (no E3157).
    mcc::mcc_load_from_string(&p_uri, src);
    assert!(
        !mcc::mcc_diagnose_all()
            .iter()
            .any(|d| d.code == mcc::errcodes::INST_CLASS_UNRESOLVED),
        "PKG resolves while mcode is loaded"
    );

    // A world reset (build.full) tombstones the whole registry — mcode is gone.
    mcc::mcc_clear_workspace();
    mcc::mcc_load_from_string(&p_uri, src);
    assert!(
        mcc::mcc_diagnose_all()
            .iter()
            .any(|d| d.code == mcc::errcodes::INST_CLASS_UNRESOLVED),
        "without reloading mcode a world reset leaves enum PKG unresolved (the stale-server defect)"
    );

    // load_libs_rpc now re-establishes mcode after the reset (auto-load
    // contract, mirrors CLI collect_libs) — the class resolves again.
    mcc::mcb_load_lib_by_name("mcode");
    mcc::mcc_load_from_string(&p_uri, src);
    assert!(
        !mcc::mcc_diagnose_all()
            .iter()
            .any(|d| d.code == mcc::errcodes::INST_CLASS_UNRESOLVED),
        "reloading mcode after the reset restores PKG resolution"
    );
}

/// T1 (G1) determinism regression: `dump_symbols_f12_text` is a pure
/// function of the file + library inputs — two clean runs of the same
/// inputs must produce byte-identical output, symbol `id=` values and
/// section order included. The symbol layer's global declare-id counter is a
/// process-run allocator (boot value 1), so an in-process second run is
/// simulated with a full reset (`mcc_init_no_lib`, ClearScope::Full), which
/// now also rewinds that counter — otherwise the second load allocates
/// shifted ids and the dump depends on parse history (the historical
/// run-to-run shuffle, lapper-improvement-plan P2.4).
#[test]
fn p08_f12_dump_deterministic_across_clean_loads() {
    let _lock = lock();

    let root = temp_lib_root("p08", "mcode", "GOLD_DET", &["A", "K"]);
    let uri: mcc::McURI = "/virtual/p08_det_use.mc".to_string();
    // Project module instantiating the lib component: the body drives
    // class-name REF registration into the file's lapper, so the dump covers
    // local defs and system-lib resolution alike.
    let src = "module main\n{\n    GOLD_DET u1\n}\n";

    // Round 1 (run 1).
    reset_workspace();
    mcc::mcc_set_system_root(&root);
    assert!(
        mcc::mcb_load_lib("mcode", &root.join("mcode")),
        "mcode lib loads in the boot world"
    );
    mcc::mcc_load_from_string(&uri, src);
    let dump1 = mcc::dump_symbols_f12_text(&uri).expect("f12 dump (round 1)");

    // Round 2 (an independent second run): full reset, same inputs.
    reset_workspace();
    mcc::mcc_set_system_root(&root);
    assert!(
        mcc::mcb_load_lib("mcode", &root.join("mcode")),
        "mcode lib loads in the fresh run"
    );
    mcc::mcc_load_from_string(&uri, src);
    let dump2 = mcc::dump_symbols_f12_text(&uri).expect("f12 dump (round 2)");

    assert_eq!(
        dump1, dump2,
        "two clean runs of the same inputs must produce byte-identical F12 \
         symbol dumps (id= values and section order included)"
    );
}

/// P9 (abstract-variant-capability-plan P1): a capability def registers as
/// its own def kind under the declaring file identity — reachable through the
/// typed registry lookups, mirrored in the workspace lifecycle tables, and
/// enumerated by the workspace/unified views. Declared signals land in the
/// capability's signal table (module-port machinery) and funcs parse at load
/// time into its func table (the data `register_host_funcs` mirrors). A
/// capability body clause outside the allowed set (signal decls + funcs) is a
/// §3.1 body violation, reported at the load.
#[test]
fn p09_capability_container_registers_and_rejects_foreign_body_clauses() {
    let _lock = lock();
    reset_workspace();

    let good_uri = "/virtual/p09_good_cap.mc".to_string();
    let good = r#"
capability DecoupledPower
{
    ps VCC
    ps GND
    io VBUS

    func Idle([ref])
    {
        ref -> VBUS
    }
}
"#;
    mcc::mcc_load_from_string(&good_uri, good);

    let ds = mcc::definition_space();
    let sn = mcc::McSpaceName::new(&mcc::McIds::from("DecoupledPower"), good_uri.clone());
    let cap = ds
        .get_capability(&sn)
        .expect("capability registers under its declaring identity");
    assert_eq!(cap.name.to_string(), "DecoupledPower");
    assert_eq!(
        cap.signals.iter_ports().count(),
        3,
        "declared ps/io signals land in the capability signal table"
    );
    assert_eq!(
        cap.funcs.len(),
        1,
        "func parses at load time into the capability func table"
    );
    assert!(
        cap.funcs.find("Idle").is_some(),
        "the func is addressable by name on the capability def"
    );

    // Workspace + unified views both see the project capability.
    assert!(
        ds.get_workspace_capability(&sn).is_some(),
        "workspace-only view resolves the project capability"
    );
    assert!(
        ds.all_capabilities()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "DecoupledPower"),
        "unified enumeration includes the capability"
    );
    assert!(
        ds.workspace_capabilities()
            .iter()
            .any(|(k, _)| k.ident.to_string() == "DecoupledPower"),
        "workspace enumeration includes the capability"
    );

    // Clean load: no CAPABILITY_BODY_INVALID (nor any error) for the good body.
    let good_diags = mcc::mcc_diagnose(&good_uri);
    assert!(
        !good_diags
            .iter()
            .any(|d| d.code == mcc::errcodes::CAPABILITY_BODY_INVALID),
        "well-formed capability body must not report CAPABILITY_BODY_INVALID"
    );

    // An attribute clause inside a capability body is outside §3.1's allowed
    // set (signal declarations + funcs only).
    let bad_uri = "/virtual/p09_bad_cap.mc".to_string();
    let bad = r#"
capability BadCap
{
    name = "not allowed in a capability"
}
"#;
    mcc::mcc_load_from_string(&bad_uri, bad);
    let bad_diags = mcc::mcc_diagnose(&bad_uri);
    assert!(
        bad_diags
            .iter()
            .any(|d| d.code == mcc::errcodes::CAPABILITY_BODY_INVALID),
        "attribute clause inside a capability body reports \
         CAPABILITY_BODY_INVALID; got {:?}",
        bad_diags
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );

    // §3.2 self-consistency: a func body name that is neither a declared
    // signal, a func param, nor a func-local instance is an unresolved ref at
    // load (the capability is never instantiated, so no E3136-style finish
    // recheck would ever catch it).
    let loose_uri = "/virtual/p09_loose_cap.mc".to_string();
    let loose = r#"
capability LooseCap
{
    io VBUS

    func Idle()
    {
        NOSUCH -> VBUS
    }
}
"#;
    mcc::mcc_load_from_string(&loose_uri, loose);
    let loose_diags = mcc::mcc_diagnose(&loose_uri);
    assert!(
        loose_diags
            .iter()
            .any(|d| d.code == mcc::errcodes::CAPABILITY_FUNC_UNRESOLVED_REF),
        "a func body name outside the capability name set reports \
         CAPABILITY_FUNC_UNRESOLVED_REF; got {:?}",
        loose_diags
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
}

/// P10 (abstract-variant-capability-plan P2 §4.2/§5): capability adoption
/// (`component X :: Cap`) link/consistency verdicts.
///
/// Each scenario is a self-contained load (capability + adopter in one file).
/// The post-parse adoption check runs for the re-derived file, so the
/// diagnostics land on that file's uri. `mcc_diagnose` returns per-uri diags;
/// `any_code` scopes the check without naming the returned diagnostic type.
fn any_code<'a, T: 'a>(
    diags: impl IntoIterator<Item = &'a T>,
    code: u32,
    code_of: impl Fn(&T) -> u32,
) -> bool {
    diags.into_iter().any(|d| code_of(d) == code)
}

#[test]
fn p10_adoption_consistency_and_func_ambiguity() {
    let _lock = lock();
    reset_workspace();

    // ── Conformant adopter: every capability-declared signal is realized by
    //    an adopter member with a compatible direction, so no §4.2/§5 error.
    let ok_uri = "/virtual/p10_ok.mc".to_string();
    let ok_src = r#"
capability Pwr
{
    ps VCC
    ps GND
    io VBUS
}

abstract component Powered :: Pwr
{
    pins = [
        ps 1 = VCC
        ps 2 = GND
        io 3 = VBUS
    ]
}
"#;
    mcc::mcc_load_from_string(&ok_uri, ok_src);
    let ok = mcc::mcc_diagnose(&ok_uri);
    assert!(
        !any_code(&ok, mcc::errcodes::CAPABILITY_SIGNAL_MISSING, |d| d.code),
        "conformant adopter reports CAPABILITY_SIGNAL_MISSING: {:?}",
        ok.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !any_code(&ok, mcc::errcodes::ADOPTS_NON_CAPABILITY, |d| d.code)
            && !any_code(&ok, mcc::errcodes::ADOPTED_FUNC_AMBIGUOUS, |d| d.code),
        "conformant adopter reports an adoption error: {:?}",
        ok.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );

    // ── `ps`-capability ↔ `in`-rail leniency: a power signal is satisfied by
    //    an `in` member (the library convention `in [VCC,GND]::DC(...)`), not
    //    only by another `ps` rail.
    let rail_uri = "/virtual/p10_in_rail.mc".to_string();
    let rail_src = r#"
capability Rail
{
    ps VDD
}

abstract component InRail :: Rail
{
    pins = [
        in 1 = VDD
    ]
}
"#;
    mcc::mcc_load_from_string(&rail_uri, rail_src);
    let rail = mcc::mcc_diagnose(&rail_uri);
    assert!(
        !any_code(&rail, mcc::errcodes::CAPABILITY_SIGNAL_MISSING, |d| d.code),
        "a `ps` capability signal matches an `in` power-rail member; got {:?}",
        rail.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );

    // ── Missing signal: adopter forgets `io VBUS` the capability declares.
    let miss_uri = "/virtual/p10_missing.mc".to_string();
    let miss_src = r#"
capability Pwr2
{
    ps VCC
    ps GND
    io VBUS
}

abstract component Slim :: Pwr2
{
    pins = [
        ps 1 = VCC
        ps 2 = GND
    ]
}
"#;
    mcc::mcc_load_from_string(&miss_uri, miss_src);
    let miss = mcc::mcc_diagnose(&miss_uri);
    assert!(
        any_code(&miss, mcc::errcodes::CAPABILITY_SIGNAL_MISSING, |d| d.code),
        "adopter missing a capability-declared signal reports \
         CAPABILITY_SIGNAL_MISSING; got {:?}",
        miss.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    let msg = miss
        .iter()
        .find(|d| d.code == mcc::errcodes::CAPABILITY_SIGNAL_MISSING)
        .map(|d| d.msg.clone())
        .unwrap_or_default();
    assert!(
        msg.contains("VBUS"),
        "missing-signal message names the absent member; got: {msg}"
    );

    // ── Direction conflict: capability declares `out DRV`, adopter exposes
    //    it `in` — present by name but not direction-compatible.
    let dir_uri = "/virtual/p10_dir.mc".to_string();
    let dir_src = r#"
capability Driver
{
    out DRV
}

abstract component WrongIn :: Driver
{
    pins = [
        in 1 = DRV
    ]
}
"#;
    mcc::mcc_load_from_string(&dir_uri, dir_src);
    let dir = mcc::mcc_diagnose(&dir_uri);
    assert!(
        any_code(&dir, mcc::errcodes::CAPABILITY_SIGNAL_MISSING, |d| d.code),
        "direction-incompatible member reports CAPABILITY_SIGNAL_MISSING; got {:?}",
        dir.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    let dir_msg = dir
        .iter()
        .find(|d| d.code == mcc::errcodes::CAPABILITY_SIGNAL_MISSING)
        .map(|d| d.msg.clone())
        .unwrap_or_default();
    assert!(
        dir_msg.contains("out") && dir_msg.contains("in"),
        "direction hint contrasts capability and adopter directions; got: {dir_msg}"
    );

    // ── `::` on a non-capability (here another abstract component) is the
    //    wrong-operator error, not an unresolved class.
    let noncap_uri = "/virtual/p10_noncap.mc".to_string();
    let noncap_src = r#"
abstract component AbsRail
{
    pins = [
        ps 1 = VCC
    ]
}

component WrongCapUse :: AbsRail
{
    pins = [
        ps 1 = VCC
    ]
}
"#;
    mcc::mcc_load_from_string(&noncap_uri, noncap_src);
    let noncap = mcc::mcc_diagnose(&noncap_uri);
    assert!(
        any_code(&noncap, mcc::errcodes::ADOPTS_NON_CAPABILITY, |d| d.code),
        "`::` on an abstract component reports ADOPTS_NON_CAPABILITY; got {:?}",
        noncap
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );

    // ── §5 conflict: two adopted capabilities share a func name the host does
    //    not override → ADOPTED_FUNC_AMBIGUOUS.
    let amb_uri = "/virtual/p10_amb.mc".to_string();
    let amb_src = r#"
capability GuardA
{
    io VA

    func Kick([ref])
    {
        ref -> VA
    }
}

capability GuardB
{
    io VB

    func Kick([ref])
    {
        ref -> VB
    }
}

component TwinLock :: GuardA, GuardB
{
    pins = [
        io 1 = VA
        io 2 = VB
    ]
}
"#;
    mcc::mcc_load_from_string(&amb_uri, amb_src);
    let amb = mcc::mcc_diagnose(&amb_uri);
    assert!(
        any_code(&amb, mcc::errcodes::ADOPTED_FUNC_AMBIGUOUS, |d| d.code),
        "two adopted caps sharing a func name report ADOPTED_FUNC_AMBIGUOUS; got {:?}",
        amb.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !any_code(&amb, mcc::errcodes::CAPABILITY_SIGNAL_MISSING, |d| d.code),
        "the ambiguity case realizes all signals; got {:?}",
        amb.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );

    // ── Same two caps, but the host overrides `Kick` with its own func: the
    //    name is resolved by the own func, so no ambiguity is reported.
    let ovr_uri = "/virtual/p10_override.mc".to_string();
    let ovr_src = r#"
capability GuardC
{
    io VC

    func Kick([ref])
    {
        ref -> VC
    }
}

capability GuardD
{
    io VD

    func Kick([ref])
    {
        ref -> VD
    }
}

component TwinOpen :: GuardC, GuardD
{
    pins = [
        io 1 = VC
        io 2 = VD
    ]

    func Kick([ref])
    {
        ref -> VC
    }
}
"#;
    mcc::mcc_load_from_string(&ovr_uri, ovr_src);
    let ovr = mcc::mcc_diagnose(&ovr_uri);
    assert!(
        !any_code(&ovr, mcc::errcodes::ADOPTED_FUNC_AMBIGUOUS, |d| d.code),
        "a host override resolves the shared name, no ADOPTED_FUNC_AMBIGUOUS; got {:?}",
        ovr.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
}

/// P11 (abstract-variant-capability-plan P2 §2.4): an instance-method call on
/// a placed host resolves to an ADOPTED capability func and expands its body
/// against the host's member surface. `Fetcher` declares one signal `VBUS`
/// plus `func Strap([ref]) { ref -> VBUS }`; concrete `FetcherChip :: Fetcher`
/// realizes `VBUS` on pin 1. `main` instantiates it and calls `U1.Strap(REF)`
/// — with the effective-method fall-through the call wires the module net to
/// the host member (a net touching both `REF` and `U1`); without it the call
/// finds no method and produces no such product.
#[test]
fn p11_adopted_capability_func_dispatches_on_host_instance() {
    let _lock = lock();
    reset_workspace();

    let uri = mcc::McURI::from("/virtual/p11_adopt_dispatch.mc");
    let src = r#"
capability Fetcher
{
    io VBUS

    func Strap([ref])
    {
        ref -> VBUS
    }
}

component FetcherChip :: Fetcher
{
    pins = [
        io 1 = VBUS
    ]
}

module main
{
    io REF
    FetcherChip U1

    func M()
    {
        U1.Strap(REF)
    }
}
"#;
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) =
        mcc::mcc_build_flat(&mcc::McIds::from("main"), &uri, 1000).expect("flat build");

    let mut nets: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        nets.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }

    assert!(
        nets.iter().any(|n| n.contains("U1") && n.contains("REF")),
        "the adopted capability func 'Strap' wired the module net onto the host \
         member (net touching both U1 and REF); netlist:\n{}",
        nets.join("\n")
    );
}

/// P12 (abstract-variant-capability-plan P3 §3.2/§6.1): placing an instance of
/// an `abstract component` marks that InstEntry `unselected` and emits the ERC
/// warning `ABSTRACT_PART_UNSELECTED`; a concrete component instance stays
/// `unselected == false` and clean. The marker is the def's `is_abstract`
/// only — abstract defs may legally carry a reference partno (spec §6), so no
/// `partno` sentinel gates the warning (no-hardcoding).
#[test]
fn p12_abstract_instance_unselected_erc() {
    let _lock = lock();

    // ── Abstract + concrete side by side: the abstract row is unselected and
    //    warns; the concrete row is clean. Both pins wired so the only
    //    abstract-variant signal in the mix is the unselected W.
    reset_workspace();
    let mix_uri = "/virtual/p12_abstract_mix.mc".to_string();
    let mix_src = r#"
component Buf
{
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

abstract component ABuf
{
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

module main
{
    io A
    io Y
    Buf c1
    ABuf u1
    A -> c1.A
    A -> u1.A
    c1.Y -> Y
    u1.Y -> Y
}
"#;
    mcc::mcc_load_from_string(&mix_uri, mix_src);
    let (_, table) =
        mcc::mcc_build_flat(&mcc::McIds::from("main"), &mix_uri, 1000).expect("flat build");
    let diags = mcc::mcc_diagnose_all();

    let c1_id = table
        .get_id_by_path("main.c1")
        .expect("concrete row present");
    let u1_id = table
        .get_id_by_path("main.u1")
        .expect("abstract row present");
    assert!(
        !table.get_entry(c1_id).unwrap().unselected,
        "a concrete instance is never unselected"
    );
    assert!(
        table.get_entry(u1_id).unwrap().unselected,
        "an abstract instance carries the unselected marker"
    );

    let hit = diags
        .iter()
        .find(|d| d.code == mcc::errcodes::ABSTRACT_PART_UNSELECTED);
    assert!(
        hit.is_some(),
        "placing an abstract component emits ABSTRACT_PART_UNSELECTED; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    let msg = hit.unwrap().msg.clone();
    assert!(
        msg.contains("main.u1") && msg.contains("BOM"),
        "unselected warning names the abstract instance and the BOM action; got: {msg}"
    );

    // ── Concrete-only module: nothing is unselected and no warning fires.
    reset_workspace();
    let conc_uri = "/virtual/p12_concrete_only.mc".to_string();
    let conc_src = r#"
component Buf
{
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

module main
{
    io A
    io Y
    Buf c1
    A -> c1.A
    c1.Y -> Y
}
"#;
    mcc::mcc_load_from_string(&conc_uri, conc_src);
    let (_, table2) =
        mcc::mcc_build_flat(&mcc::McIds::from("main"), &conc_uri, 1000).expect("flat build");
    let diags2 = mcc::mcc_diagnose_all();
    assert!(
        !any_code(&diags2, mcc::errcodes::ABSTRACT_PART_UNSELECTED, |d| d.code),
        "concrete-only module emits no ABSTRACT_PART_UNSELECTED; got {:?}",
        diags2
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    let c1b_id = table2
        .get_id_by_path("main.c1")
        .expect("concrete row present");
    assert!(
        !table2.get_entry(c1b_id).unwrap().unselected,
        "a concrete instance is never unselected"
    );
}

/// P13 (abstract-variant-capability-plan P4 §7): link-time variant
/// materialization. `component BBuf : ABuf` clones the abstract base's data
/// surface (pins/spec) under the variant's own identity, applies the child's
/// attribute overrides (partno, spec.* leaves), records `variant_of`, and the
/// resulting def is a *concrete* component — placeable, `is_abstract ==
/// false`, not `unselected`. The parse-time data locks and the
/// `VARIANT_BASE_NON_ABSTRACT` link error are asserted on the same shapes.
#[test]
fn p13_variant_materializes_from_abstract_base() {
    let _lock = lock();
    reset_workspace();

    let uri = "/virtual/p13_variant.mc".to_string();
    let src = r#"
abstract component ABuf
{
    package = PKG.SOIC8
    spec.HBM = ±0kV

    pins = [
        in 1 = A
        out 2 = Y
    ]
}

component BBuf : ABuf
{
    partno = "BB-1"
    spec.HBM = 4.5kV
}

module main
{
    io A
    io Y
    BBuf u1
    A -> u1.A
    u1.Y -> Y
}
"#;
    mcc::mcc_load_from_string(&uri, src);
    let diags = mcc::mcc_diagnose(&uri);
    assert!(
        !any_code(&diags, mcc::errcodes::VARIANT_BASE_NON_ABSTRACT, |d| d.code)
            && !any_code(&diags, mcc::errcodes::ABSTRACT_DERIVES_ABSTRACT, |d| d.code)
            && !any_code(
                &diags,
                mcc::errcodes::VARIANT_REDECLARES_PINS_PARAMS_FUNCS,
                |d| d.code
            ),
        "a conformant variant/base pair loads clean; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );

    let ds = mcc::definition_space();
    let base_sn = mcc::McSpaceName::new(&mcc::McIds::from("ABuf"), uri.clone());
    let variant_sn = mcc::McSpaceName::new(&mcc::McIds::from("BBuf"), uri.clone());
    let base = ds
        .get_component(&base_sn)
        .expect("abstract base registered");
    let variant = ds.get_component(&variant_sn).expect("variant registered");

    // ── §7.1/§7.2: the materialized variant is concrete, self-sufficient and
    //    identity-correct — base data under the child's own name/uri.
    assert!(base.is_abstract, "the base stays abstract");
    assert!(!variant.is_abstract, "the variant materializes concrete");
    assert_eq!(variant.name.to_string(), "BBuf");
    assert_eq!(variant.uri.to_string(), uri);
    assert!(
        variant.variant_base.is_some(),
        "the materialized def keeps its own `: ABuf` provenance"
    );
    assert!(
        variant.pins.count() == base.pins.count() && variant.pins.count() == 2,
        "variant inherits the base pin table; got {} vs base {}",
        variant.pins.count(),
        base.pins.count()
    );

    // ── Attr overlay (§7.2): base-only attrs stay, child overrides win,
    //    spec.* leaves are per-item (dotted id = one leaf). Lookup is by
    //    display-line scan, not `find(&McIds::from("spec.HBM"))`: a dotted
    //    attr key is stored as `[Ida(spec), DotIda(HBM)]` while the `From<&str>`
    //    key keeps the dot inside one Ida segment, so structural equality never
    //    matches (the code's own spec reads parse the same AST shape).
    let attr_of = |c: &mcc::McComponent, id: &str| {
        c.attrs
            .find(&mcc::McIds::from(id))
            .map(|a| format!("{a}"))
            .unwrap_or_default()
    };
    let spec_lines = |c: &mcc::McComponent| {
        c.attrs
            .iter()
            .map(|a| format!("{a}"))
            .filter(|s| s.contains("spec.HBM"))
            .collect::<Vec<_>>()
    };
    assert!(
        attr_of(variant.as_ref(), "package").contains("SOIC8"),
        "an unoverridden base attr is inherited"
    );
    let v_spec = spec_lines(variant.as_ref());
    assert!(
        v_spec.len() == 1 && v_spec[0].contains("4.5kV") && !v_spec[0].contains('±'),
        "the child spec leaf replaced the base's (one row, overridden); got: {v_spec:?}"
    );
    let b_spec = spec_lines(base.as_ref());
    assert!(
        b_spec.len() == 1 && b_spec[0].contains("0kV") && !b_spec[0].contains("4.5kV"),
        "the base's own spec leaf is untouched (still its ±0 range); got: {b_spec:?}"
    );
    let partno = attr_of(variant.as_ref(), "partno");
    assert!(
        partno.contains("BB-1"),
        "the child's partno override is applied; got: {partno}"
    );

    // ── §6.1: a placed variant is a normal concrete instance — not unselected.
    let (_, table) =
        mcc::mcc_build_flat(&mcc::McIds::from("main"), &uri, 1000).expect("flat build");
    let u1_id = table
        .get_id_by_path("main.u1")
        .expect("variant row present");
    assert!(
        !table.get_entry(u1_id).unwrap().unselected,
        "a materialized variant instance is selected (never ABSTRACT_PART_UNSELECTED)"
    );
    let build_diags = mcc::mcc_diagnose_all();
    assert!(
        !any_code(&build_diags, mcc::errcodes::ABSTRACT_PART_UNSELECTED, |d| d
            .code),
        "placing a variant emits no unselected warning; got {:?}",
        build_diags
            .iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );

    // ── Parse-time data locks (0.2/§7.2): writing pins/params/funcs on a
    //    variant, `abstract` + `:` together, and `:` + `::` together.
    reset_workspace();
    let bad1 = "/virtual/p13_lock_pins.mc".to_string();
    mcc::mcc_load_from_string(
        &bad1,
        &format!("{src}\n\ncomponent CBad : ABuf\n{{\n    pins = [\n        in 1 = A\n    ]\n}}\n"),
    );
    assert!(
        any_code(
            &mcc::mcc_diagnose(&bad1),
            mcc::errcodes::VARIANT_REDECLARES_PINS_PARAMS_FUNCS,
            |d| d.code
        ),
        "a variant that writes pins reports VARIANT_REDECLARES_PINS_PARAMS_FUNCS"
    );

    reset_workspace();
    let bad2 = "/virtual/p13_lock_abstract.mc".to_string();
    let src2 = r#"
abstract component ABase
{
    pins = [
        in 1 = A
    ]
}

abstract component CBad2 : ABase
{
}
"#;
    mcc::mcc_load_from_string(&bad2, src2);
    assert!(
        any_code(
            &mcc::mcc_diagnose(&bad2),
            mcc::errcodes::ABSTRACT_DERIVES_ABSTRACT,
            |d| d.code
        ),
        "`abstract` + `:` reports ABSTRACT_DERIVES_ABSTRACT"
    );

    reset_workspace();
    let bad3 = "/virtual/p13_lock_both.mc".to_string();
    let src3 = r#"
capability SomeCap
{
    io VA
}

component CBad3 : ABase :: SomeCap
{
}
"#;
    // Needs the abstract base too, so include it.
    let src3 = format!("{src2}{src3}");
    mcc::mcc_load_from_string(&bad3, &src3);
    assert!(
        any_code(
            &mcc::mcc_diagnose(&bad3),
            mcc::errcodes::VARIANT_ADOPTS,
            |d| d.code
        ),
        "`:` + `::` together reports VARIANT_ADOPTS"
    );

    // ── VARIANT_BASE_NON_ABSTRACT: `: Concrete` (and the hint fires).
    reset_workspace();
    let bad4 = "/virtual/p13_base_concrete.mc".to_string();
    let src4 = r#"
component RealR
{
    pins = [
        in 1 = A
    ]
}

component WVariant : RealR
{
    partno = "W-1"
}
"#;
    mcc::mcc_load_from_string(&bad4, src4);
    let d4 = mcc::mcc_diagnose(&bad4);
    assert!(
        any_code(&d4, mcc::errcodes::VARIANT_BASE_NON_ABSTRACT, |d| d.code),
        "`:` on a concrete component reports VARIANT_BASE_NON_ABSTRACT; got {:?}",
        d4.iter()
            .map(|d| (d.code, d.msg.clone()))
            .collect::<Vec<_>>()
    );
    let m4 = d4
        .iter()
        .find(|d| d.code == mcc::errcodes::VARIANT_BASE_NON_ABSTRACT)
        .map(|d| d.msg.clone())
        .unwrap_or_default();
    assert!(
        m4.contains("WVariant"),
        "VARIANT_BASE_NON_ABSTRACT names the variant; got: {m4}"
    );
}

/// P14 (abstract-variant-capability plan P4 §4.3 / §8.1 / §8.3): the typed
/// derivation/adoption relations served off the registry ledgers — typed
/// `variant_base_of` / `cluster_of` / `adopted_capabilities_of` / `adopters_of`
/// root queries plus the `defs.relations` RPC surface. One file carries an
/// abstract base + a materialized variant and an abstract capability host, so
/// every reverse edge has a live answer.
#[test]
fn p14_variant_and_adoption_relation_queries() {
    let _lock = lock();
    reset_workspace();
    let uri = "/virtual/p14_relations.mc".to_string();
    let src = r#"
capability Pwr
{
    ps VCC
}

abstract component ABuf
{
    package = PKG.SOIC8
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

component BBuf : ABuf
{
    partno = "BB-1"
}

abstract component CapHost :: Pwr
{
    pins = [
        ps 1 = VCC
    ]
}
"#;
    mcc::mcc_load_from_string(&uri, src);
    // Drive the load round so `sync_derivation_edges` fills the ledgers before
    // the queries read them (mcc_load_from_string registers defs; the parse
    // pass that re-derives runs on the diagnose call, as in p10/p13).
    let _ = mcc::mcc_diagnose(&uri);

    let sn = |name: &str| mcc::McSpaceName::new(&mcc::McIds::from(name), uri.clone());
    let abuf_id = mcc::def_id(&sn("ABuf"), mcc::DefKind::Component).expect("ABuf def id");
    let bbuf_id = mcc::def_id(&sn("BBuf"), mcc::DefKind::Component).expect("BBuf def id");
    let caphost_id = mcc::def_id(&sn("CapHost"), mcc::DefKind::Component).expect("CapHost def id");
    let pwr_id = mcc::def_id(&sn("Pwr"), mcc::DefKind::Capability).expect("Pwr def id");

    // ── §8.1/§8.3 reverse edges on the ledgers.
    assert_eq!(
        mcc::variant_base_of(bbuf_id),
        Some(abuf_id),
        "the variant's variant_of edge points at the abstract base"
    );
    assert_eq!(
        mcc::variant_base_of(abuf_id),
        None,
        "an abstract base is itself never a variant (chain length ≤ 1)"
    );
    assert_eq!(
        mcc::cluster_of(abuf_id),
        vec![bbuf_id],
        "cluster_of(ABuf) is exactly the materialized variant BBuf"
    );
    assert!(
        mcc::cluster_of(bbuf_id).is_empty(),
        "a concrete variant has no further variants"
    );
    assert_eq!(
        mcc::adopted_capabilities_of(caphost_id),
        vec![pwr_id],
        "adopted_capabilities_of(CapHost) = [Pwr] in declaration order"
    );
    assert!(
        mcc::adopted_capabilities_of(bbuf_id).is_empty(),
        "a pure variant adopts nothing of its own"
    );
    assert_eq!(
        mcc::adopters_of(pwr_id),
        vec![caphost_id],
        "adopters_of(Pwr) = [CapHost]"
    );

    // ── The RPC surface reports the same relations by (name, uri).
    let rel = |params: serde_json::Value| {
        mcc::rpc::handlers::handle_defs_relations(Some(params)).expect("defs.relations resolves")
    };
    let r = rel(serde_json::json!({ "name": "ABuf", "uri": uri }));
    assert_eq!(r["kind"].as_str(), Some("component"));
    assert_eq!(r["relations"]["isAbstract"].as_bool(), Some(true));
    let cluster = r["relations"]["cluster"].as_array().expect("cluster list");
    assert_eq!(cluster.len(), 1);
    assert_eq!(cluster[0]["name"].as_str(), Some("BBuf"));

    let r = rel(serde_json::json!({ "name": "BBuf", "uri": uri }));
    assert_eq!(r["relations"]["isAbstract"].as_bool(), Some(false));
    assert_eq!(
        r["relations"]["declaredBase"].as_str(),
        Some("ABuf"),
        "the variant reports its own `: Base` clause"
    );
    assert_eq!(
        r["relations"]["variantBase"]["name"].as_str(),
        Some("ABuf"),
        "the resolved abstract base is linked"
    );

    let r = rel(serde_json::json!({
        "name": "Pwr", "uri": uri, "kind": "capability"
    }));
    assert_eq!(r["kind"].as_str(), Some("capability"));
    let adopters = r["relations"]["adopters"]
        .as_array()
        .expect("adopters list");
    assert_eq!(adopters.len(), 1);
    assert_eq!(adopters[0]["name"].as_str(), Some("CapHost"));

    let r = rel(serde_json::json!({ "name": "CapHost", "uri": uri }));
    assert_eq!(r["relations"]["declaredAdopts"][0].as_str(), Some("Pwr"));
    assert_eq!(
        r["relations"]["adoptedCapabilities"][0]["name"].as_str(),
        Some("Pwr")
    );
    assert!(
        r["relations"]["cluster"]
            .as_array()
            .is_some_and(|c| c.is_empty()),
        "CapHost is not a variant base, so its cluster is empty"
    );

    // Unknown def answers null defId and empty relations — never an error.
    let r = rel(serde_json::json!({ "name": "Nope", "uri": uri }));
    assert_eq!(r["defId"], serde_json::Value::Null);
    assert_eq!(r["relations"], serde_json::json!({}));
}

/// P15 (abstract-variant-capability plan P4 §7.2/§4.2 "base edit -> variant
/// refresh"): a *base-file* re-parse must propagate to an already-materialized
/// variant in another file. The variant's own declared overrides (`partno`,
/// `spec.HBM`) are the only attrs allowed to override the base; inherited
/// attrs (`package`) must track the freshly parsed base — re-materialization reads
/// the declared child shell ([`declared_variants`] ledger), never the previous
/// round's base clone, or a stale `package` would re-clobber the new base.
#[test]
fn p15_base_edit_propagates_to_materialized_variant() {
    let _lock = lock();
    reset_workspace();

    // Two real files so a re-parse of the base file alone leaves the variant's
    // file untouched (the cross-file stale-overlay case p13 cannot reach).
    let dir = std::env::temp_dir().join(format!("mcc-defspace-golden-p15-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let base_src = r#"
abstract component ABuf
{
    package = PKG.SOIC8
    spec.HBM = ±0kV
    pins = [
        in 1 = A
        out 2 = Y
    ]
}
"#;
    let var_src = r#"
use ./b.mc

component BBuf : ABuf
{
    partno = "BB-1"
    spec.HBM = 4.5kV
}
"#;
    std::fs::write(dir.join("b.mc"), base_src).unwrap();
    std::fs::write(dir.join("v.mc"), var_src).unwrap();
    let b_uri = canon(&dir.join("b.mc"));
    let v_uri = canon(&dir.join("v.mc"));

    // Round 1 — load base then variant, drive the derivation round.
    mcc::mcc_load_from_string(&b_uri, base_src);
    mcc::mcc_load_from_string(&v_uri, var_src);
    let _ = mcc::mcc_diagnose_all();

    let ds = mcc::definition_space();
    let sn = |name: &str| mcc::McSpaceName::new(&mcc::McIds::from(name), v_uri.clone());
    let base_sn = mcc::McSpaceName::new(&mcc::McIds::from("ABuf"), b_uri.clone());
    let attr_of = |c: &mcc::McComponent, id: &str| {
        c.attrs
            .find(&mcc::McIds::from(id))
            .map(|a| format!("{a}"))
            .unwrap_or_default()
    };
    let spec_of = |c: &mcc::McComponent| {
        c.attrs
            .iter()
            .map(|a| format!("{a}"))
            .filter(|s| s.contains("spec.HBM"))
            .collect::<Vec<_>>()
    };
    let variant = ds
        .get_component(&sn("BBuf"))
        .expect("variant materialized across files");
    assert_eq!(
        variant.pins.count(),
        2,
        "cross-file variant inherits the base pin table"
    );
    assert!(
        attr_of(variant.as_ref(), "package").contains("SOIC8"),
        "round 1 inherits the base package"
    );
    assert!(
        spec_of(variant.as_ref())[0].contains("4.5kV"),
        "round 1 keeps the declared spec override"
    );

    // Round 2 — re-parse ONLY the base file (its package + spec leaf change).
    // The variant file is not reloaded, so its registry row is still the round-1
    // materialized def; re-materialization must still track the fresh base.
    let base_src2 = base_src
        .replace("PKG.SOIC8", "PKG.SOIC16")
        .replace("±0kV", "±2kV");
    mcc::mcc_load_from_string(&b_uri, &base_src2);
    let _ = mcc::mcc_diagnose_all();

    let ds = mcc::definition_space();
    let base = ds.get_component(&base_sn).expect("fresh base");
    assert!(
        attr_of(base.as_ref(), "package").contains("SOIC16"),
        "the base file edit landed on the base def"
    );
    assert!(
        spec_of(base.as_ref())[0].contains("2kV"),
        "the base spec leaf edit landed"
    );
    let variant = ds
        .get_component(&sn("BBuf"))
        .expect("variant still materialized after base edit");
    assert!(
        attr_of(variant.as_ref(), "package").contains("SOIC16"),
        "the inherited package tracked the base edit (was: {:?})",
        attr_of(variant.as_ref(), "package")
    );
    assert!(
        spec_of(variant.as_ref()).len() == 1 && spec_of(variant.as_ref())[0].contains("4.5kV"),
        "the declared spec override survives a base edit (was: {:?})",
        spec_of(variant.as_ref())
    );
    assert_eq!(
        variant.pins.count(),
        2,
        "the pin table is unchanged by the base attr edit"
    );
}
