// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! System library management API -- PR-4a
//!
//! Generalize `mcb_init_system_lib()` hardcoded mcode logic into "load any library by name".
//!
//! ## Core API
//!
//! - [`mcb_load_lib`]: load a system library into `mcc_blibs`
//! - [`mcb_unload_lib`]: unload from memory (no disk deletion)
//! - [`mcb_loaded_libs`]: list currently loaded system libraries
//! - [`mcb_lib_info`]: query definitions contained in a library
//!
//! ## Compatibility with old API
//!
//! `mcb_init_system_lib()` preserved, internally changed to call `mcb_load_lib("mcode", mcode_dir)`.

use crate::db::cmie::tables as workspace;
use crate::db::defspace::LibBoundary;
use crate::db::infra::global;
use crate::db::infra::mc_code::McCode;
use crate::{McIds, McSpaceName};
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;
use tracing::{debug, info, warn};

// ── System library source cache ──
#[allow(non_upper_case_globals)]
pub(crate) static mcc_blibs: LazyLock<DashMap<String, McCode>> = LazyLock::new(DashMap::new);

/// System library basic info (snapshot from mcc_blibs).
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
        if let Some(found) = find_lib_dir(&root, name) {
            return Some(found);
        }
    }
    None
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
        let prefix = format!("{name}@");
        if let Ok(entries) = std::fs::read_dir(root) {
            for e in entries.flatten() {
                let fname = e.file_name().to_string_lossy().to_string();
                if fname.starts_with(&prefix) && e.path().is_dir() {
                    return Some(e.path());
                }
            }
        }
        let bare = root.join(name);
        if bare.exists() {
            return Some(bare);
        }
    }
    None
}

/// True when `path` belongs to an already-loaded system library (e.g. mcode).
///
/// A library file that is re-entered through a project entry point (did_open /
/// load_project / sem on a file inside `~/.mcode/mcode`) must keep its
/// definitions in the global system tables. Otherwise `remove_defines` strips
/// its entries from the global tables while the re-parse registers them into
/// the active workspace, so the P5 system lookup loses the class and member
/// resolution breaks (E3071 for `CAP(...).Cap(_)`).
pub(crate) fn file_is_system_library(path: &Path) -> bool {
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
/// 4. Collect all definitions belonging to this library from workspace tables, register to blib's spacenames
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
    if mcc_blibs.contains_key(name) {
        if let Some(blib) = mcc_blibs.get(name) {
            if !blib.spacenames.is_empty() {
                info!(target: "mcc::lib", name = name, "load: already loaded, skip");
                return true;
            }
        }
        // No interfaces found, need to reload
        info!(target: "mcc::lib", name = name, "load: no interfaces found, will reload");
    }

    // Pre-insert empty blib entry (to avoid circular lookup issues)
    mcc_blibs.insert(name.to_string(), McCode::new_empty());

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

    // Collect all definitions belonging to this library from workspace tables, register to blib's spacenames
    let root_str = root.to_string_lossy().to_string();
    let mut lib_entry = McCode::new_empty();

    tracing::trace!(target: "mcc::lib", name = name, root_str = %root_str, "collecting spacenames with prefix");

    // Collect all definitions belonging to this library from workspace tables, register to blib's spacenames
    collect_spacenames_by_prefix(&workspace::WORKSPACE.components, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix(&workspace::WORKSPACE.modules, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix(&workspace::WORKSPACE.interfaces, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix(&workspace::WORKSPACE.enums, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix(&workspace::WORKSPACE.defines, &root_str, &mut lib_entry);

    // Collect all definitions belonging to this library from system tables, register to blib's spacenames
    collect_spacenames_by_prefix_global(&global::mcc_components, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix_global(&global::mcc_modules, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix_global(&global::mcc_interfaces, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix_global(&global::mcc_enums, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix_global(&global::mcc_defines, &root_str, &mut lib_entry);

    let symbol_count = lib_entry.spacenames.len();

    // §15: For non-mcode libraries, remove symbols from global/workspace tables.
    // mcode is the only exception that gets global auto-visibility.
    // Third-party libs should only be visible via explicit `use $::name`.
    if name != "mcode" {
        let uris: HashSet<String> = lib_entry
            .spacenames
            .values()
            .map(|sn| sn.uri.to_string())
            .collect();
        crate::db::defregistry::remove_by_uris(&uris);
        info!(
            target: "mcc::lib",
            name = name,
            uris_removed = uris.len(),
            "removed from global tables (use-only visibility)"
        );
    }

    // §12.1 DefinitionSpace manifest: record the loaded library boundary
    // (name + on-disk root + the uris it brought in).
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
        },
    );

    // Replace blib with new one
    mcc_blibs.insert(name.to_string(), lib_entry);

    info!(
        target: "mcc::lib",
        name = name,
        symbols = symbol_count,
        files_loaded = loaded.len(),
        elapsed_ms = t0.elapsed().as_millis() as u64,
        "loaded"
    );
    true
}

/// Unload system library from memory. Do not delete disk files.
///
/// 1. Remove entry from `mcc_blibs`
/// 2. Remove definitions from `mcc_*` system tables with uri containing library path
/// 3. Remove definitions from workspace tables with uri containing library path
pub fn mcb_unload_lib(name: &str) -> bool {
    let blib = match mcc_blibs.remove(name) {
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
            mcc_blibs.clear();
            global::mcc_components.clear();
            global::mcc_modules.clear();
            global::mcc_interfaces.clear();
            global::mcc_enums.clear();
            global::mcc_defines.clear();
            workspace::WORKSPACE.clear_active();
        }
        ClearScope::Lib => {
            let uris = uris.expect("ClearScope::Lib requires the library uri set");
            crate::db::defregistry::remove_by_uris(uris);
        }
    }
}

/// List all loaded system libraries in memory.
pub fn mcb_loaded_libs() -> Vec<String> {
    mcc_blibs.iter().map(|e| e.key().clone()).collect()
}

fn format_mc_ids(ids: &McIds) -> String {
    format!("{ids}")
}

/// Get system library information by name.
pub fn mcb_lib_info(name: &str) -> Option<LibInfo> {
    let blib = mcc_blibs.get(name)?;
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
        if workspace::WORKSPACE.modules.contains_key(space_name)
            || global::mcc_modules.contains_key(space_name)
        {
            module_count += 1;
            modules_list.push(format_mc_ids(&space_name.ident));
        } else if workspace::WORKSPACE.components.contains_key(space_name)
            || global::mcc_components.contains_key(space_name)
        {
            component_count += 1;
            components_list.push(format_mc_ids(&space_name.ident));
        } else if workspace::WORKSPACE.interfaces.contains_key(space_name)
            || global::mcc_interfaces.contains_key(space_name)
        {
            interface_count += 1;
            interfaces_list.push(format_mc_ids(&space_name.ident));
        } else if workspace::WORKSPACE.enums.contains_key(space_name)
            || global::mcc_enums.contains_key(space_name)
        {
            enum_count += 1;
            enums_list.push(format_mc_ids(&space_name.ident));
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
    let system_root = crate::mcb_get_system_root();
    let data_root = crate::cli::datadir::data_root();

    // Determine the actual root to use. Path-like names (absolute paths,
    // `a/b` forms, `.mc` files) resolve against the system root directly.
    // Bare library names go through the version-aware `resolve_lib_root`
    // (system root first, then data root; `<name>@<version>` directories are
    // matched before the bare `<name>` directory) so third-party libraries
    // installed as versioned directories load correctly. Both fall back to
    // data_root (never a hardcoded ~/.mcode) so discovery stays on the
    // unified data root (use-design §19.10 D4).
    let is_path_like = lib_name.contains('/')
        || lib_name.contains('\\')
        || lib_name.ends_with(".mc")
        || std::path::Path::new(lib_name).is_absolute();
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
        resolve_lib_root(lib_name).unwrap_or_else(|| data_root.join(lib_name))
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
}

// ============================================================================
// Internal helper functions
// ============================================================================

fn collect_spacenames_by_prefix<T>(
    table: &DashMap<McSpaceName, Arc<T>>,
    prefix: &str,
    lib_entry: &mut McCode,
) {
    for entry in table.iter() {
        if entry.key().uri.contains(prefix) {
            lib_entry
                .spacenames
                .insert(entry.key().ident.clone(), entry.key().clone());
        }
    }
}

fn collect_spacenames_by_prefix_global<T>(
    table: &DashMap<McSpaceName, Arc<T>>,
    prefix: &str,
    lib_entry: &mut McCode,
) {
    for entry in table.iter() {
        let uri = &entry.key().uri;
        if uri.contains(prefix) {
            lib_entry
                .spacenames
                .insert(entry.key().ident.clone(), entry.key().clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::find_lib_dir;
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
    fn find_lib_dir_prefers_versioned_dir() {
        let root = temp_root("versioned");
        let found = find_lib_dir(&root, "acme");
        assert_eq!(found, Some(root.join("acme@2.0")), "versioned dir wins");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_lib_dir_bare_dir_fallback() {
        let root = temp_root("bare");
        std::fs::remove_dir_all(root.join("acme@2.0")).unwrap();
        let found = find_lib_dir(&root, "acme");
        assert_eq!(found, Some(root.join("acme")), "bare dir fallback");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_lib_dir_mcode_subdir_and_sibling() {
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
    fn find_lib_dir_absent_returns_none() {
        let root = temp_root("absent");
        assert_eq!(find_lib_dir(&root, "nosuchlib"), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
