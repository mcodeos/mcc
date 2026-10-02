// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! System library management API -- PR-4a
//!
//! Generalize `mcb_init_system_lib()` hardcoded mcode logic into "load any library by name".
//!
//! ## Core API
//!
//! - [`mcb_load_lib`]: load a system library into the workspace lib cache
//! - [`mcb_unload_lib`]: unload from memory (no disk deletion)
//! - [`mcb_loaded_libs`]: list currently loaded system libraries
//! - [`mcb_lib_info`]: query definitions contained in a library
//!
//! ## Compatibility with old API
//!
//! `mcb_init_system_lib()` preserved, internally changed to call
//! `mcb_load_lib("mcode", mcode_dir)`.

use crate::db::cmie::tables as workspace;
use crate::db::defspace::LibBoundary;
use crate::db::infra::mc_code::McCode;
use crate::McIds;
use anyhow::Context as _;
use std::collections::HashSet;
use std::path::Path;
use tracing::{debug, info, warn};

// ── System library source cache ──
// Phase 5: moved per-world into `workspace::WORKSPACE.blibs` (the
// `mcc_blibs` process-global static is gone) — each world owns the libraries
// it loaded, so a world switch can never leak stale lib state.

/// System library basic info (snapshot from the workspace lib cache).
#[derive(Debug, Clone)]
pub struct LibInfo {
    pub name: String,
    pub root: String,
    pub module_count: usize,
    pub component_count: usize,
    pub interface_count: usize,
    pub enum_count: usize,
    pub total_symbols: usize,
    pub modules: Vec<String>,
    pub components: Vec<String>,
    pub interfaces: Vec<String>,
    pub enums: Vec<String>,
}

/// RAII guard for the process-wide side effects of `mcb_load_lib`.
///
/// `mcb_load_lib` can be re-entered: a library's dependency chain (or a
/// non-project `use` lazy load, use-design §19.5 rule 2) may trigger a nested
/// `mcb_load_lib` while an outer load is still running. Each entry sets the
/// system-lib-loading flag and resets the AST visit dedup flag; without a
/// guard the inner load's exit would clobber the outer load's state. The
/// guard saves both flags on entry and restores them on drop, so a nested load
/// returns the process to the exact state the outer load expects.
struct LibLoadGuard {
    visit_done: bool,
    system_loading: bool,
}

impl LibLoadGuard {
    fn new() -> Self {
        let guard = Self {
            visit_done: super::mc_code::AST_VISIT_DONE.load(std::sync::atomic::Ordering::SeqCst),
            system_loading: crate::cli::config::is_system_lib_loading(),
        };
        // Same side effects as the previous inline code: suppress trace output
        // while the library is loaded, and force a fresh AST visit pass.
        crate::cli::config::set_system_lib_loading(true);
        super::mc_code::mcb_reset_ast_visit_flag();
        guard
    }
}

impl Drop for LibLoadGuard {
    fn drop(&mut self) {
        super::mc_code::AST_VISIT_DONE.store(self.visit_done, std::sync::atomic::Ordering::SeqCst);
        crate::cli::config::set_system_lib_loading(self.system_loading);
    }
}

/// Find the on-disk root directory of a library, for non-project `use` lazy
/// loading (use-design §19.5 rule 2).
///
/// Single source of truth for library-root discovery, shared by the CLI and
/// the RPC/IDE path (RPC delegates here). The runtime system root is searched
/// first (always the data root: `MCC_SYSTEM_ROOT` env, then
/// `~/.mcode` default — see `mcc_set_system_root`), with `data_root()` as the
/// fallback. mcode resolves under each root (with a sibling fallback); other
/// libraries match versioned directories (`<name>@<version>`), then a bare
/// `<name>` directory.
pub fn resolve_lib_root(name: &str) -> Option<std::path::PathBuf> {
    let proj = crate::builder::mcb_get_project_root();
    let project_root = if proj.as_os_str().is_empty() {
        None
    } else {
        Some(proj.as_path())
    };
    resolve_lib_root_req(name, &VersionReq::Any, project_root)
}

/// Resolve a library root honoring a version requirement and the project tier.
///
/// Precedence: the project's `<root>/libs` directory (never consulted for
/// mcode — the official library is global-only), then the global roots
/// (sticky system root, then the data root). An `Exact` pin only matches the
/// `<name>@<version>` directory spelling the pin; it never silently falls
/// back to a different version. `Any` keeps every legacy fallback.
pub fn resolve_lib_root_req(
    name: &str,
    req: &VersionReq,
    project_root: Option<&Path>,
) -> Option<std::path::PathBuf> {
    if let Some(proj) = project_root {
        if name != "mcode" {
            let libs = crate::cli::datadir::project_libs_dir(proj);
            if let Some(found) = find_lib_dir_pinned(&libs, name, req) {
                return Some(found);
            }
        }
    }

    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    let sys = crate::builder::mcb_get_system_root();
    if !sys.as_os_str().is_empty() {
        roots.push(sys);
    }
    let data = crate::cli::datadir::data_root();
    if !roots.iter().any(|r| *r == data) {
        roots.push(data);
    }
    for root in roots {
        if let Some(found) = find_lib_dir_pinned(&root, name, req) {
            return Some(found);
        }
    }
    None
}

/// A `[dependencies]` version requirement from a project manifest.
///
/// `"*"` (or empty) = any installed copy; anything else is an exact pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionReq {
    Any,
    Exact(String),
}

/// Parse a raw pin string into a [`VersionReq`].
pub fn parse_version_req(s: &str) -> VersionReq {
    match s.trim() {
        "" | "*" => VersionReq::Any,
        v => VersionReq::Exact(v.to_string()),
    }
}

/// Search a single root directory for a library honoring a version
/// requirement. See [`resolve_lib_root_req`] for the tiering.
pub fn find_lib_dir_pinned(
    root: &Path,
    name: &str,
    req: &VersionReq,
) -> Option<std::path::PathBuf> {
    match req {
        VersionReq::Exact(v) => {
            let pinned = root.join(format!("{name}@{v}"));
            if pinned.is_dir() {
                return Some(pinned);
            }
            // mcode keeps the legacy unversioned layout as an exact-pin
            // fallback: the bare working-copy mcode satisfies any pin rather
            // than failing the load.
            if name == "mcode" {
                let bare = root.join("mcode");
                if bare.exists() {
                    return Some(bare);
                }
                let sibling = root.join("..").join("mcode");
                if sibling.exists() {
                    return Some(sibling);
                }
            }
            None
        }
        VersionReq::Any => find_lib_dir(root, name),
    }
}

/// Search a single root directory for a library by name.
fn find_lib_dir(root: &Path, name: &str) -> Option<std::path::PathBuf> {
    if name == "mcode" {
        let p = root.join("mcode");
        if p.exists() {
            return Some(p);
        }
        let sibling = root.join("..").join("mcode");
        if sibling.exists() {
            return Some(sibling);
        }
        return None;
    }
    if root.exists() {
        // Highest versioned copy first (deterministic), then bare `<name>`.
        if let Some(found) = highest_versioned_dir(root, name) {
            return Some(found);
        }
        let bare = root.join(name);
        if bare.exists() {
            return Some(bare);
        }
    }
    None
}

/// Highest `<name>@<version>` directory under `root`, semver-ordered
/// (non-numeric tails sort lowest). The system-`use` join uses this as a
/// fallback when no bare `<name>` directory exists (registry-design.md §4.2:
/// installed packs land as `<name>@<ver>/`). "Highest wins" is the P1 rule —
/// the version solver (§P2) replaces it once dependency resolution lands.
pub fn highest_versioned_dir(root: &Path, name: &str) -> Option<std::path::PathBuf> {
    let prefix = format!("{name}@");
    let mut best: Option<((u64, u64, u64), std::path::PathBuf)> = None;
    let entries = std::fs::read_dir(root).ok()?;
    for e in entries.flatten() {
        let fname = e.file_name().to_string_lossy().to_string();
        let Some(ver) = fname.strip_prefix(&prefix) else {
            continue;
        };
        if !e.path().is_dir() {
            continue;
        }
        let nums: Vec<u64> = ver
            .split('.')
            .map(|seg| seg.parse::<u64>().unwrap_or(0))
            .collect();
        let key = (
            nums.first().copied().unwrap_or(0),
            nums.get(1).copied().unwrap_or(0),
            nums.get(2).copied().unwrap_or(0),
        );
        if best.as_ref().map(|(k, _)| key > *k).unwrap_or(true) {
            best = Some((key, e.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// True when `path` belongs to an already-loaded system library (e.g. mcode).
///
/// A library file that is re-entered through a project entry point (did_open /
/// load_project / sem on a file inside `~/.mcode/mcode`) must keep its
/// definitions in the global system tables. Otherwise `remove_project_defs` strips
/// its entries from the global tables while the re-parse registers them into
/// the active workspace, so the P5 system lookup loses the class and member
/// resolution breaks (E3071 for `CAP(...).Cap(_)`).
pub fn file_is_system_library(path: &Path) -> bool {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    for name in mcb_loaded_libs() {
        if let Some(root) = resolve_lib_root(&name) {
            let root_canon = std::fs::canonicalize(&root).unwrap_or(root);
            if canon.starts_with(&root_canon) {
                return true;
            }
        }
    }
    false
}

/// Load a system library into memory.
///
/// `name`: library name (e.g., "mcode", "infineon")
/// `root`: library root directory, should contain `<name>.mc` as entry file
///
/// Process:
/// 1. Find `<root>/<name>.mc` entry file
/// 2. Pre-insert empty blib entry (avoid circular lookup issues)
/// 3. `mcb_add_recursive` load entry and all dependencies (is_system=true)
/// 4. Collect all definitions belonging to this library from workspace tables, register to blib's
/// spacenames
///
/// Returns `true` if load succeeded.
pub fn mcb_load_lib(name: &str, root: &Path) -> bool {
    let t0 = std::time::Instant::now();
    info!(
        target: "mcc::lib",
        name = name,
        root = ?root,
        "load: start"
    );
    let entry_basename = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let entry_file = root.join(format!("{entry_basename}.mc"));
    if !entry_file.exists() {
        warn!(
            target: "mcc::lib",
            name = name,
            path = ?entry_file,
            "entry file not found"
        );
        return false;
    }

    // If already loaded, check if it has any definitions (i.e. was properly loaded)
    if workspace::WORKSPACE.blibs.contains_key(name) {
        if let Some(blib) = workspace::WORKSPACE.blibs.get(name) {
            if !blib.spacenames.is_empty() {
                info!(target: "mcc::lib", name = name, "load: already loaded, skip");
                return true;
            }
        }
        // No interfaces found, need to reload
        info!(target: "mcc::lib", name = name, "load: no interfaces found, will reload");
    }

    // Pre-insert empty blib entry (to avoid circular lookup issues)
    workspace::WORKSPACE
        .blibs
        .insert(name.to_string(), McCode::new_empty());

    // Save/restore the loading side effects so nested `mcb_load_lib` calls
    // (diamond deps, use lazy loading) do not clobber the outer load's state.
    let _guard = LibLoadGuard::new();

    // Recursively load all dependencies (is_system=true)
    let uri = entry_file.to_string_lossy().to_string();
    let mut loaded = HashSet::new();
    crate::build::loader::set_current_lib(Some(name.to_string()));
    crate::build::loader::mcb_add_recursive(&uri, &mut loaded, true);
    crate::build::loader::clear_lib_progress();
    crate::build::loader::set_current_lib(None);

    debug!(
        target: "mcc::lib",
        name = name,
        files_loaded = loaded.len(),
        "recursive load complete"
    );

    // Collect all definitions belonging to this library from the single
    // definition registry (any domain), register to blib's spacenames.
    let root_str = root.to_string_lossy().to_string();
    let mut lib_entry = McCode::new_empty();
    tracing::trace!(target: "mcc::lib", name = name, root_str = %root_str, "collecting spacenames with prefix");
    for sn in crate::db::defregistry::spacenames_by_uri_prefix(&root_str) {
        lib_entry.spacenames.insert(sn.ident.clone(), sn);
    }

    let symbol_count = lib_entry.spacenames.len();

    // §15: For non-mcode libraries, tombstone their registry entries and drop
    // them from the workspace tables (within this world only).
    // mcode is the only exception that gets auto-visibility.
    // Third-party libs should only be visible via explicit `use $::name`.
    if name != "mcode" {
        let uris: HashSet<String> = lib_entry
            .spacenames
            .values()
            .map(|sn| sn.uri.to_string())
            .collect();
        workspace::WORKSPACE.remove_lib_defs_by_uris(&uris);
        // U234: the unload sweep also drops the libs' resolution edges.
        workspace::WORKSPACE.refgraph.purge_files(&uris);
        // U303: drop the loader's per-file entries too. They still claim
        // pass1_complete, so a later project `use` of one of these files is
        // short-circuited by the mcb_add_recursive fast path (loader.rs) and
        // the tombstoned defs never come back — the use-only revival stayed
        // shadowed for any dependency-loaded member, notably the lib's entry
        // file (the one file the dependency loop always loads).
        for u in &uris {
            workspace::WORKSPACE.mcodes.remove(u);
        }
        info!(
            target: "mcc::lib",
            name = name,
            uris_removed = uris.len(),
            "tombstoned in this world (use-only visibility)"
        );
    }

    // §12.1 DefinitionSpace manifest: record the loaded library boundary
    // (name + on-disk root + the uris it brought in), plus the model profile
    // cards from its `sim/` sidecar (worldmodel-design §7 W3).
    let profiles = crate::db::infra::model_profile::load_lib_profiles(root);
    if !profiles.invalid.is_empty() {
        for bad in &profiles.invalid {
            warn!(target: "mcc::lib", name = name, file = %bad.file, error = %bad.error, "model profile card invalid");
        }
    }
    info!(
        target: "mcc::lib",
        name = name,
        cards = profiles.cards.len(),
        "model profiles loaded"
    );
    workspace::WORKSPACE.libs.insert(
        name.to_string(),
        LibBoundary {
            name: name.to_string(),
            root: root.to_path_buf(),
            uris: lib_entry
                .spacenames
                .values()
                .map(|sn| sn.uri.to_string())
                .collect(),
            profiles,
        },
    );

    // Replace blib with new one
    workspace::WORKSPACE
        .blibs
        .insert(name.to_string(), lib_entry);

    info!(
        target: "mcc::lib",
        name = name,
        symbols = symbol_count,
        files_loaded = loaded.len(),
        elapsed_ms = t0.elapsed().as_millis() as u64,
        "loaded"
    );
    // T6-②: library load round end — the recursive add registered (or, for a
    // use-only third-party lib, tombstoned) defs above; stamp one journal
    // version when the round changed the definition space.
    workspace::WORKSPACE.registry().checkpoint_if_changed();
    true
}

/// Unload system library from memory. Do not delete disk files.
///
/// 1. Remove entry from the per-world lib cache (`WORKSPACE.blibs`)
/// 2. Tombstone definitions from the per-world registry with uri containing library path
/// 3. Remove definitions from workspace tables with uri containing library path
pub fn mcb_unload_lib(name: &str) -> bool {
    let blib = match workspace::WORKSPACE.blibs.remove(name) {
        Some((_, blib)) => blib,
        None => return false,
    };

    // §12.1 DefinitionSpace manifest: drop the library boundary.
    workspace::WORKSPACE.libs.remove(name);

    // Collect all uri prefixes in this library
    let uris: HashSet<String> = blib
        .spacenames
        .values()
        .map(|sn| sn.uri.to_string())
        .collect();

    // Remove all definitions with this uri prefixes in system tables and workspace tables
    clear_state(ClearScope::Lib, Some(&uris));

    info!(target: "mcc::lib", name = name, "unloaded");
    true
}

/// Scope of a state clear (consistency-convergence.md §2.4).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClearScope {
    /// Full reset: loaded libs, global tables, and the active workspace.
    Full,
    /// Unload one library: remove its definitions from global and workspace
    /// tables (requires the library's uri set).
    Lib,
}

/// Single state-clear entry point (consistency-convergence.md §2.4).
///
/// Replaces the hand-maintained clear lists in `mcb_init`, `clear_active`,
/// and `mcb_unload_lib` that overlapped on the same tables.
pub fn clear_state(scope: ClearScope, uris: Option<&HashSet<String>>) {
    match scope {
        ClearScope::Full => {
            workspace::WORKSPACE.blibs.clear();
            workspace::WORKSPACE.clear_active();
            // The definition registry is world-owned state on the active
            // workspace; a full reset starts its identity journal over with a
            // clean slate.
            workspace::WORKSPACE.registry().clear_all();
            // The symbol layer's declare-id ledger follows the full reset:
            // every table referencing those ids is rebuilt, so the key→id
            // interning must start over or a second clean load in this process
            // would number from where the first ended (T1 determinism).
            crate::ast::sem::reset_declare_id_space();
        }
        ClearScope::Lib => {
            let uris = uris.expect("ClearScope::Lib requires the library uri set");
            workspace::WORKSPACE.remove_lib_defs_by_uris(uris);
            // U234: the unload sweep also drops the libs' resolution edges.
            workspace::WORKSPACE.refgraph.purge_files(uris);
            // T6-②: library-unload round end — stamp one journal version when
            // the sweep tombstoned any definition.
            workspace::WORKSPACE.registry().checkpoint_if_changed();
        }
    }
}

/// List all loaded system libraries in memory.
pub fn mcb_loaded_libs() -> Vec<String> {
    workspace::WORKSPACE
        .blibs
        .iter()
        .map(|e| e.key().clone())
        .collect()
}

fn format_mc_ids(ids: &McIds) -> String {
    format!("{ids}")
}

/// Get system library information by name.
pub fn mcb_lib_info(name: &str) -> Option<LibInfo> {
    let blib = workspace::WORKSPACE.blibs.get(name)?;
    let sn = &blib.spacenames;

    let mut module_count = 0usize;
    let mut component_count = 0usize;
    let mut interface_count = 0usize;
    let mut enum_count = 0usize;

    let mut modules_list = Vec::new();
    let mut components_list = Vec::new();
    let mut interfaces_list = Vec::new();
    let mut enums_list = Vec::new();

    for (_, space_name) in sn.iter() {
        match crate::db::defregistry::kind_of(space_name) {
            Some(crate::db::defregistry::DefKind::Module) => {
                module_count += 1;
                modules_list.push(format_mc_ids(&space_name.ident));
            }
            Some(crate::db::defregistry::DefKind::Component) => {
                component_count += 1;
                components_list.push(format_mc_ids(&space_name.ident));
            }
            Some(crate::db::defregistry::DefKind::Interface) => {
                interface_count += 1;
                interfaces_list.push(format_mc_ids(&space_name.ident));
            }
            Some(crate::db::defregistry::DefKind::Enum) => {
                enum_count += 1;
                enums_list.push(format_mc_ids(&space_name.ident));
            }
            _ => {} // Define / no live entry: not counted in the lib info.
        }
    }

    modules_list.sort();
    components_list.sort();
    interfaces_list.sort();
    enums_list.sort();

    Some(LibInfo {
        name: name.to_string(),
        root: String::new(),
        module_count,
        component_count,
        interface_count,
        enum_count,
        total_symbols: sn.len(),
        modules: modules_list,
        components: components_list,
        interfaces: interfaces_list,
        enums: enums_list,
    })
}

/// Load a single library by name or path.
///
/// Supports absolute paths and `.mc` file forms (e.g. "mcode/mcode.mc"),
/// and skips libraries that are already truly loaded (interfaces counted).
/// Falls back to data_root when the system root is empty or the joined
/// path does not exist. Shared by the CLI and the RPC layer so that
/// non-project builds honor the global mcc.yaml [libs].load list.
pub fn mcb_load_lib_by_name(lib_name: &str) {
    let _ = mcb_load_lib_by_name_pinned(lib_name, &VersionReq::Any);
}

/// Load a single library by name, honoring a manifest version pin.
///
/// Same resolution as [`mcb_load_lib_by_name`] plus the `Exact` pin from a
/// project's `[dependencies]`. When the exact pin is unmet but *some* copy
/// resolves, the copy loads with a warning carrying the install hint — an
/// unmet pin degrades loudly, never to a silent version swap. Returns the
/// human-readable warning when one fired, for callers that surface load
/// diagnostics on stderr (the CLI); `None` when the load was clean.
pub fn mcb_load_lib_by_name_pinned(lib_name: &str, req: &VersionReq) -> Option<String> {
    let system_root = crate::mcb_get_system_root();
    let data_root = crate::cli::datadir::data_root();

    // Determine the actual root to use. Path-like names (absolute paths,
    // `a/b` forms, `.mc` files) resolve against the system root directly.
    // Bare library names go through the version-aware `resolve_lib_root_req`
    // (project libs first, then system root, then data root) so vendored
    // project libraries and versioned global installs both load correctly.
    // Both fall back to data_root (never a hardcoded ~/.mcode) so discovery
    // stays on the unified data root (use-design §19.10 D4).
    let is_path_like = lib_name.contains('/')
        || lib_name.contains('\\')
        || lib_name.ends_with(".mc")
        || std::path::Path::new(lib_name).is_absolute();
    let mut warning: Option<String> = None;
    let lib_path = if is_path_like {
        if system_root.as_os_str().is_empty() {
            data_root.join(lib_name)
        } else {
            let joined = system_root.join(lib_name);
            if !joined.exists() {
                data_root.join(lib_name)
            } else {
                joined
            }
        }
    } else {
        let proj = crate::builder::mcb_get_project_root();
        let project_root = if proj.as_os_str().is_empty() {
            None
        } else {
            Some(proj.as_path())
        };
        match resolve_lib_root_req(lib_name, req, project_root) {
            Some(found) => found,
            None => {
                // Pin unmet: fall back to any installed copy, loudly.
                let fallback = resolve_lib_root(lib_name);
                if let (VersionReq::Exact(v), Some(found)) = (req, fallback) {
                    let warn = format!(
                        "project pins {lib_name}@{v} but it is not installed; using {}. \
                         Run `mcc lib install {lib_name} --from <path>` (or set the pin to \"*\")",
                        found.display()
                    );
                    tracing::warn!(target: "mcc::lib", lib = lib_name, pin = v, "{}", warn);
                    warning = Some(warn);
                    found
                } else if let VersionReq::Exact(v) = req {
                    let warn = format!(
                        "library {lib_name}@{v} is not installed anywhere; \
                         run `mcc lib install {lib_name} --from <path>` (or set the pin to \"*\")"
                    );
                    tracing::warn!(target: "mcc::lib", lib = lib_name, pin = v, "{}", warn);
                    warning = Some(warn);
                    data_root.join(lib_name)
                } else {
                    data_root.join(lib_name)
                }
            }
        }
    };

    // Normalize: if lib_name is a .mc file path, extract the library name
    // and root directory. e.g. "mcode/mcode.mc" -> name="mcode", root=system_root/mcode
    let (name, root) = if lib_path.extension().map_or(false, |e| e == "mc") {
        let name = lib_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let root = lib_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(system_root);
        (name, root)
    } else {
        (lib_name.to_string(), lib_path)
    };

    // Check if the library truly loaded interfaces (built-in components do
    // not count; interfaces are required for the library to count as loaded).
    let lib_info = crate::mcb_lib_info(&name);
    let interface_count = lib_info.as_ref().map(|i| i.interface_count).unwrap_or(0);
    if root.exists() && (!crate::mcb_loaded_libs().contains(&name) || interface_count == 0) {
        tracing::info!(target: "mcc::lib",
            lib = name,
            path = ?root,
            "loading library");
        crate::mcb_load_lib(&name, &root);
    } else if !root.exists() {
        tracing::warn!(target: "mcc::lib",
            lib = name,
            "library not found in system root");
    }
    warning
}

// Internal helper functions

/// Install-scope guard: the global data root is official-library territory.
/// mcode installs there (as a versioned copy); third-party libraries vendor
/// into a project's `libs/` and never the other way round. Lives in the lib
/// crate so the RPC handlers and the CLI enforce one law.
pub fn ensure_install_scope(name: &str, target_root: &Path) -> anyhow::Result<()> {
    let global = target_root == crate::cli::datadir::data_root();
    if name == "mcode" && !global {
        anyhow::bail!(
            "lib install: mcode is the official library and installs into the global data \
             root; it is never vendored into a project"
        );
    }
    if name != "mcode" && global {
        anyhow::bail!(
            "lib install: the global data root is reserved for mcode; third-party \
             libraries install into <project>/libs (run inside a project)"
        );
    }
    Ok(())
}

/// Pure bare-directory install: copy library dir into `target_root` as
/// `<name>@<version>`. Returns (name@version, target path). The single
/// install core shared by the CLI (`cmds::lib`) and the RPC `lib.install`
/// handler; .mcl archives go through `cmds::pack::install_mcl_at`, which
/// reuses [`ensure_install_scope`] after pack.toml names the pack. Versions
/// normalize to the canonical two-segment `MAJOR.MINOR`.
pub fn install_lib_at(
    target_root: &Path,
    name: &str,
    from: &str,
    version: Option<&str>,
) -> anyhow::Result<(String, std::path::PathBuf)> {
    ensure_install_scope(name, target_root)?;
    let src = std::path::PathBuf::from(from);
    if !src.exists() {
        anyhow::bail!("lib install: source path does not exist '{}'", from);
    }

    let ver = crate::cli::datadir::normalize_version(version.unwrap_or("0.0.0"));
    let lib_name_ver = format!("{}@{}", name, ver);
    // Flat layout: install into <root>/<name>@<ver>
    let target = target_root.join(&lib_name_ver);

    if target.exists() {
        anyhow::bail!(
            "lib install: {} is already installed ({}). Run `uninstall` first to reinstall.",
            lib_name_ver,
            target.display()
        );
    }

    std::fs::create_dir_all(target_root).with_context(|| {
        format!(
            "lib install: cannot create install root {}",
            target_root.display()
        )
    })?;
    copy_dir_recursive(&src, &target).with_context(|| {
        format!(
            "lib install: failed to copy {} → {}",
            from,
            target.display()
        )
    })?;

    // The index only covers the global data root; project tiers list by scan.
    if target_root == crate::cli::datadir::data_root() {
        let _ = crate::cli::datadir::rebuild_index();
    }

    Ok((lib_name_ver, target))
}

fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{find_lib_dir, find_lib_dir_pinned, parse_version_req, resolve_lib_root_req, VersionReq};
    use std::path::PathBuf;

    /// Build a temp root populated with a bare `acme` lib and a versioned one.
    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcc-findlib-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("acme")).unwrap();
        std::fs::create_dir_all(dir.join("acme@2.0")).unwrap();
        std::fs::create_dir_all(dir.join("mcode")).unwrap();
        dir
    }

    #[test]
    fn def_libmgr__find_lib_dir_prefers_versioned_dir() {
        let root = temp_root("versioned");
        let found = find_lib_dir(&root, "acme");
        assert_eq!(found, Some(root.join("acme@2.0")), "versioned dir wins");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn def_libmgr__find_lib_dir_bare_dir_fallback() {
        let root = temp_root("bare");
        std::fs::remove_dir_all(root.join("acme@2.0")).unwrap();
        let found = find_lib_dir(&root, "acme");
        assert_eq!(found, Some(root.join("acme")), "bare dir fallback");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn def_libmgr__find_lib_dir_mcode_subdir_and_sibling() {
        let root = temp_root("mcode");
        let found = find_lib_dir(&root, "mcode");
        assert_eq!(found, Some(root.join("mcode")));
        let _ = std::fs::remove_dir_all(&root);

        // Sibling fallback: the root itself is a data root whose mcode lives
        // one level up.
        let root2 = std::env::temp_dir().join(format!("mcc-findlib-sib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root2);
        std::fs::create_dir_all(&root2).unwrap();
        std::fs::create_dir_all(root2.join("..").join("mcode")).unwrap();
        let found = find_lib_dir(&root2, "mcode");
        assert_eq!(found, Some(root2.join("..").join("mcode")));
        let _ = std::fs::remove_dir_all(&root2);
    }

    #[test]
    fn def_libmgr__find_lib_dir_absent_returns_none() {
        let root = temp_root("absent");
        assert_eq!(find_lib_dir(&root, "nosuchlib"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Build a temp project with a `libs/` tier holding one library.
    fn temp_project(tag: &str, lib: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcc-proj-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("libs").join(lib)).unwrap();
        dir
    }

    #[test]
    fn def_libmgr__parse_version_req_any_and_exact() {
        assert_eq!(parse_version_req("*"), VersionReq::Any);
        assert_eq!(parse_version_req(""), VersionReq::Any);
        assert_eq!(parse_version_req(" 0.1 "), VersionReq::Exact("0.1".into()));
    }

    #[test]
    fn def_libmgr__pinned_exact_selects_pinned_copy() {
        let root = temp_root("pin-exact");
        std::fs::create_dir_all(root.join("acme@1.0")).unwrap();
        // temp_root already made acme@2.0; the exact pin must beat it.
        let found = find_lib_dir_pinned(&root, "acme", &VersionReq::Exact("1.0".into()));
        assert_eq!(found, Some(root.join("acme@1.0")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn def_libmgr__pinned_exact_miss_never_swaps_version() {
        let root = temp_root("pin-miss");
        // Only acme@2.0 exists; pinning 9.9 must NOT return acme@2.0.
        let found = find_lib_dir_pinned(&root, "acme", &VersionReq::Exact("9.9".into()));
        assert_eq!(found, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn def_libmgr__pinned_mcode_falls_back_to_bare() {
        let root = temp_root("pin-mcode");
        std::fs::remove_dir_all(root.join("mcode")).unwrap();
        std::fs::create_dir_all(root.join("mcode")).unwrap();
        // No mcode@0.5 dir; the legacy bare working copy satisfies the pin.
        let found = find_lib_dir_pinned(&root, "mcode", &VersionReq::Exact("0.5".into()));
        assert_eq!(found, Some(root.join("mcode")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn def_libmgr__project_libs_tier_precedes_global() {
        // Unique name so the real ~/.mcode on this machine cannot interfere.
        let proj = temp_project("tier", "zztest_acme@1.0");
        let found = resolve_lib_root_req(
            "zztest_acme",
            &VersionReq::Exact("1.0".into()),
            Some(&proj),
        );
        assert_eq!(found, Some(proj.join("libs").join("zztest_acme@1.0")));
        // mcode is global-only: a project-tier mcode dir is never consulted.
        let proj2 = temp_project("tier-mcode", "mcode@0.5");
        let found2 = resolve_lib_root_req("mcode", &VersionReq::Any, Some(&proj2));
        if let Some(p) = found2 {
            assert!(
                !p.starts_with(proj2.join("libs")),
                "mcode must not resolve from the project tier"
            );
        }
        let _ = std::fs::remove_dir_all(&proj);
        let _ = std::fs::remove_dir_all(&proj2);
    }

    #[test]
    fn def_libmgr__any_resolves_highest_version() {
        let root = temp_root("any-high");
        std::fs::create_dir_all(root.join("acme@1.0")).unwrap();
        let found = find_lib_dir_pinned(&root, "acme", &VersionReq::Any);
        assert_eq!(found, Some(root.join("acme@2.0")), "Any picks the highest");
        let _ = std::fs::remove_dir_all(&root);
    }
}
