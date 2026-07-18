// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::builder::global;
use crate::builder::lib_mgr;
use crate::builder::mc_code::McCode;
use crate::builder::workspace;
use crate::instant::mc_mod::McModuleInst;
use crate::{McCMIE, McIds, McModule, ParserResult};
use crate::{McSpaceName, McURI};
use std::cell::RefCell;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, trace, warn};

// Re-entry guard: records (class_name, uri) pairs currently being parsed,
// prevents mcb_get_cmie → parse_pass1_modules → McModule::new → mcb_get_cmie infinite recursion

thread_local! {
    static CMIE_RESOLVING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub type MccProjectTree = McModuleInst;

//1. Namespace idx: load first
/// (1) System base library (all):        global::spacenames: HashMap<CMIE String, McSpaceName>
/// (2) use imported (each mc_code):     mc_code::spacenames:  HashMap<CMIE String, McSpaceName> | mark which ones not found
///     //- mc library
///     - 3rd library
///     - User library         libs/...
///     - User project file

//2. File loading: lazy
//  - mcode to global::mcc_blibs (first traverse file list, only load namespace, load CMIE when used)
//  - 3rd to   workspace::WORKSPACE.mcodes (load when used)
//  - project to   workspace::WORKSPACE.mcodes (load all)

//3. Project parsing flow:
// (1) System startup, only load base idx (load files, ast, get all idx)
// (2) Load all project files: load idx for nsp (load files, ast, get idx), then pass1, when found used in parsing, actually load (get CMIE)

//1. Instantiation  Find nsp resource/base resource; found, create one by one; recursive operation;(query builder table first; parse if not found)
//2. Syntax processing  Parse directly; find nsp/base;

// uri usage scenarios 1. File (./relative path/file name.mc | ~/.mcode/libxxx/abc.mc ) 2. File-internal jump path
// 1. cleanup : system cleanup load, only keep entry list
// 2. load : load one by one

pub fn mcb_set_system_root(path: &Path) {
    *global::mcc_system_root.borrow_mut() = path.to_path_buf();
}
pub fn mcb_set_project_root(path: &Path) {
    *global::mcc_project_root.borrow_mut() = path.to_path_buf();
}
pub fn mcb_get_system_root() -> PathBuf {
    global::mcc_system_root.borrow().clone()
}
pub fn mcb_get_project_root() -> PathBuf {
    global::mcc_project_root.borrow().clone()
}

pub fn mcb_canonicalize_uri(uri: &McURI) -> String {
    canonicalize_project_uri(uri)
}

pub fn mcb_init() {
    global::mcc_blibs.borrow().clear();
    global::mcc_components.borrow().clear();
    global::mcc_modules.borrow().clear();
    global::mcc_interfaces.borrow().clear();
    global::mcc_enums.borrow().clear();

    workspace::WORKSPACE.clear_active();
    // System library loading is uniformly handled by mcb_init_system_lib()
}

pub fn mcb_workspace_clear() {
    workspace::WORKSPACE.clear_active();
}

/// Load project file (single file, not recursive)
pub fn mcb_add(uri: &McURI) {
    let canonical_uri = canonicalize_project_uri(uri);

    let file_to_add = if Path::new(&canonical_uri).is_absolute() {
        canonical_uri.clone()
    } else {
        mcb_get_project_root()
            .join(&canonical_uri)
            .to_string_lossy()
            .to_string()
    };

    if let Some(mut mcfile) = McCode::new(&file_to_add, false) {
        mcfile.parse_ast(); // step 1
        mcfile.parse_nsp(); // step 2
        mcfile.parse_pass1(); // step 3

        let binding = workspace::WORKSPACE.mcodes.borrow();
        let entry: dashmap::Entry<'_, _, McCode> = binding.entry(canonical_uri.clone());
        match entry {
            dashmap::Entry::Occupied(mut occupied_entry) => {
                // update pass
                remove_defines(&canonical_uri);
                occupied_entry.insert(mcfile);
            }
            dashmap::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(mcfile);
            }
        }
    }
}

/// Load file from memory string (no disk dependency)
/// uri is virtual path (e.g., /mcc/s01/file.mc), content is .mc file content
/// Note: caller must set log flags via `mcc_reset()` before calling
pub fn mcb_add_from_string(uri: &McURI, content: &str) {
    let canonical_uri = canonicalize_project_uri(uri);
    tracing::info!(target: "mcc::lsp", "mcb_add_from_string: uri={:?} -> canonical={:?}", uri, canonical_uri);

    if let Some(mut mcfile) = McCode::new_from_string(&canonical_uri, content) {
        let already_exists = {
            let binding = workspace::WORKSPACE.mcodes.borrow();
            binding.contains_key(&canonical_uri)
        };
        tracing::info!(target: "mcc::lsp", "mcb_add_from_string: already_exists={}", already_exists);
        if already_exists {
            remove_defines(&canonical_uri);
            // Also clear diagnostics for this file
            workspace::WORKSPACE
                .diagnostics
                .borrow_mut()
                .clear_file(&canonical_uri);
            tracing::info!(target: "mcc::lsp", "mcb_add_from_string: cleared diagnostics for {}", canonical_uri);
        }

        mcfile.parse_ast_from_string(content);
        mcfile.parse_nsp();
        mcfile.parse_pass1_types();
        mcfile.parse_pass1_modules(); // ★ Fix: Also parse modules to register instance symbols and build lapper

        let binding = workspace::WORKSPACE.mcodes.borrow();
        if already_exists {
            binding.insert(canonical_uri.clone(), mcfile);
        } else {
            binding.insert(canonical_uri.clone(), mcfile);
        }
        tracing::info!(target: "mcc::lsp", "mcb_add_from_string: added to workspace, keys count={}, all_keys={:?}", 
            binding.len(), binding.iter().map(|e| e.key().clone()).collect::<Vec<_>>());
    } else {
        tracing::warn!(target: "mcc::lsp", "mcb_add_from_string: McCode::new_from_string returned None");
    }
}

// ============================================================================
// Phase 1: Recursive dependency loading
// ============================================================================

/// Recursively load project files and all their dependencies
///
/// Starting from entry file, parse use statements, recursively load all dependency files,
/// ensure dependency files complete pass1 parsing before being referenced.
///
/// # Parameters
/// - `uri`: Entry file URI (relative to project root)
///
/// # Example
/// ```ignore
/// let mut loaded = HashSet::new();
/// mcb_add_recursive(&"hbl.mc".to_string(), &mut loaded);
/// ```
pub fn mcb_add_recursive(uri: &McURI, loaded: &mut HashSet<String>, is_system_lib: bool) {
    // 1. Normalize path, avoid duplicate loading
    let canonical_uri = canonicalize_project_uri(uri);
    trace!(target: "mcc::builder", uri = %uri, canonical = %canonical_uri, is_system_lib, "load: enter");

    if loaded.contains(&canonical_uri) {
        trace!(target: "mcc::builder", canonical = %canonical_uri, "load: skip (already loaded)");
        return;
    }

    // 2. Construct full file path
    let file_path = if Path::new(&canonical_uri).is_absolute() {
        PathBuf::from(&canonical_uri)
    } else {
        mcb_get_project_root().join(&canonical_uri)
    };

    let file_str = match file_path.to_str() {
        Some(s) => s.to_string(),
        None => {
            warn!(target: "mcc::builder", path = ?file_path, "load: non-utf8 path, skip");
            return;
        }
    };

    // 3. Create and parse file
    let mut mcfile = match McCode::new(&file_str, is_system_lib) {
        Some(f) => f,
        None => {
            warn!(target: "mcc::builder", file = %file_str, "load: McCode::new failed");
            return;
        }
    };

    // 4. Parse AST
    trace!(target: "mcc::builder", file = %file_str, "load: parse_ast");
    mcfile.parse_ast();

    // 5. Parse namespace (build spacenames and uselist)
    trace!(target: "mcc::builder", file = %file_str, "load: parse_nsp");
    mcfile.parse_nsp();

    // 5.5. First insert file into prj_mcodes (so when parse_pass1_types() calls mcb_get_cmie to lookup Interface, it can find current file's spacenames in prj_mcodes)
    workspace::WORKSPACE
        .mcodes
        .borrow()
        .insert(canonical_uri.clone(), mcfile.clone());

    // 6. Mark as loaded (before recursion to prevent circular dependencies)
    loaded.insert(canonical_uri.clone());

    // 7. Recursively load all dependencies (this will first complete parse_pass1_types() for dependencies)
    let deps: Vec<McURI> = mcfile.uselist.iter().map(|u| u.uri.clone()).collect();
    if !deps.is_empty() {
        trace!(target: "mcc::builder", file = %file_str, deps = deps.len(), "load: recurse into deps");
    }

    for dep_uri in deps {
        mcb_add_recursive(&dep_uri, loaded, is_system_lib);
    }

    // 8. After all dependencies are loaded, parse this file's CMIE definitions
    // Check pass1_complete flag to determine if parsing is needed
    let need_parse = !mcfile.pass1_complete;
    if need_parse {
        trace!(target: "mcc::builder", file = %file_str, "load: parse_pass1_types");
        crate::current_uri::set(&canonical_uri);
        remove_defines(&canonical_uri);
        mcfile.parse_pass1_types();
        // Update spacenames in prj_mcodes
        workspace::WORKSPACE
            .mcodes
            .borrow()
            .entry(canonical_uri.clone())
            .and_modify(|entry| entry.spacenames.clone_from(&mcfile.spacenames));
    }
    // Note: create_lapper is called at the end of parse_pass1_modules() inside parse_pass1_types.
    trace!(
        target: "mcc::builder",
        file = %file_str,
        "load: done"
    );

    // 9. Update project file table (replace pre-inserted empty file with parsed file)
    if let dashmap::Entry::Occupied(mut occupied_entry) = workspace::WORKSPACE
        .mcodes
        .borrow()
        .entry(canonical_uri.clone())
    {
        occupied_entry.insert(mcfile);
    }
}

/// Phase 1b: all component/interface/enum are registered, now parse all modules
///
/// To avoid Mutex deadlock (parse_pass1_modules -> mcb_get_cmie -> prj_mcodes.borrow),
/// we extract files from the map, parse outside the lock, then re-insert.
///
/// ★ Fix: Parse modules in dependency order (topological sort based on uselist).
/// Without this, DashMap iteration is unordered, so hbl.mc modules could be parsed
/// before power.mc modules are registered, causing "definition not found" errors.
pub fn mcb_parse_all_modules() {
    // 1. Collect all URIs and their dependencies
    let mut uri_deps: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for entry in workspace::WORKSPACE.mcodes.borrow().iter() {
        let uri = entry.key().clone();
        // ★ Fix: Canonicalize dependency URIs so they match the map keys.
        // Without this, raw URIs like "./power.mc" won't match canonicalized
        // keys like "/abs/path/power.mc", causing topo sort to treat all files
        // as having zero deps → random parse order → "definition not found" errors.
        let deps: Vec<String> = entry
            .value()
            .uselist
            .iter()
            .map(|u| canonicalize_project_uri(&u.uri))
            .collect();
        uri_deps.insert(uri, deps);
    }

    // 2. Topological sort: dependencies first
    let mut sorted_uris = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let all_uris: Vec<String> = uri_deps.keys().cloned().collect();

    fn topo_visit(
        uri: &str,
        uri_deps: &std::collections::HashMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        sorted: &mut Vec<String>,
    ) {
        if visited.contains(uri) {
            return;
        }
        visited.insert(uri.to_string());
        if let Some(deps) = uri_deps.get(uri) {
            for dep in deps {
                topo_visit(dep, uri_deps, visited, sorted);
            }
        }
        sorted.push(uri.to_string());
    }

    for uri in &all_uris {
        topo_visit(uri, &uri_deps, &mut visited, &mut sorted_uris);
    }

    // 3. Parse modules in dependency order
    // Use remove+insert instead of clone+insert to avoid AstNode ownership issues.
    // Clone creates a shallow AstNode copy (owned=false) that dangles when the
    // original (owned=true) is dropped during insert replacement.
    for uri in sorted_uris {
        let mcfile_opt = workspace::WORKSPACE
            .mcodes
            .borrow()
            .remove(&uri)
            .map(|(_k, v)| v);

        if let Some(mut mcfile) = mcfile_opt {
            crate::current_uri::set(&uri);
            mcfile.parse_pass1_modules();
            workspace::WORKSPACE.mcodes.borrow().insert(uri, mcfile);
        }
    }

    // ★ Validation: run PostParse checks after all modules parsed (once)
    {
        use crate::builder::diagnostic::{diagnostic_log, DiagnosticLevel};
        use crate::core::validation::{CheckRegistry, PostParseContext};
        use std::sync::LazyLock;
        static POST_PARSE_RUN: LazyLock<std::sync::Mutex<bool>> =
            LazyLock::new(|| std::sync::Mutex::new(false));
        let mut flag = POST_PARSE_RUN.lock().unwrap_or_else(|e| e.into_inner());
        if !*flag {
            *flag = true;
            let ctx = PostParseContext::new();
            let registry = CheckRegistry::with_defaults();
            let saved_uri = crate::current_uri::try_get();
            for r in registry.run_post_parse(&ctx) {
                // Switch current_uri to the file this diagnostic belongs to
                if let Some(ref uri) = r.uri {
                    crate::current_uri::set(&McURI::from(uri.as_str()));
                }
                let level = match r.severity {
                    crate::core::validation::CheckSeverity::Error => DiagnosticLevel::Error,
                    crate::core::validation::CheckSeverity::Warning => DiagnosticLevel::Warning,
                    crate::core::validation::CheckSeverity::Info => DiagnosticLevel::Info,
                    crate::core::validation::CheckSeverity::Hint => DiagnosticLevel::Hint,
                };
                let (pos, len) = r
                    .span
                    .as_ref()
                    .map(|s| (s.start as u32, (s.end - s.start) as u32))
                    .unwrap_or((0, 0));
                diagnostic_log(r.code, level, pos, len, &r.message, &[]);
            }
            // Restore previous current_uri (or reset)
            match saved_uri {
                Some(ref uri) => crate::current_uri::set(uri),
                None => crate::current_uri::reset(),
            }
        }
    }
}

/// Normalize project file URI
///
/// Handle relative and absolute paths, return canonical path in unified format
pub(crate) fn canonicalize_project_uri(uri: &McURI) -> String {
    let path = Path::new(uri);

    // If absolute path, try to normalize
    if path.is_absolute() {
        return path
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| uri.clone());
    }

    // Relative path, join project root and normalize
    let full_path = mcb_get_project_root().join(path);
    full_path
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| uri.clone())
}

/// Get number of loaded files
pub fn mcb_loaded_file_count() -> usize {
    workspace::WORKSPACE.mcodes.borrow().len()
}

/// Print list of loaded files
pub fn mcb_print_loaded_files() {
    for _entry in workspace::WORKSPACE.mcodes.borrow().iter() {}
}

/// Unload project file
pub fn mcb_remove(uri: &McURI) {
    let canonical_uri = canonicalize_project_uri(uri);

    remove_defines(uri);
    if canonical_uri != *uri {
        remove_defines(&canonical_uri);
    }

    let binding = workspace::WORKSPACE.mcodes.borrow();
    binding.remove(uri);
    if canonical_uri != *uri {
        binding.remove(&canonical_uri);
    }

    let extra_keys: Vec<String> = binding
        .iter()
        .filter(|entry| {
            let key = entry.key();
            key.ends_with(uri)
                || uri.ends_with(key)
                || key.ends_with(&canonical_uri)
                || canonical_uri.ends_with(key)
        })
        .map(|entry| entry.key().clone())
        .collect();
    for key in extra_keys {
        binding.remove(&key);
    }
}
pub fn mcb_query<'a>(uri: &McURI) -> Option<ParserResult> {
    let binding = workspace::WORKSPACE.mcodes.borrow();
    let canonical_uri = canonicalize_project_uri(uri);

    if let Some(mcfile) = binding.get(&canonical_uri) {
        return Some(ParserResult {
            sem_tokens: mcfile.tokens.clone(),
            sem_symbols: mcfile.symbols.clone(),
        });
    }

    if let Some(mcfile) = binding.get(uri) {
        return Some(ParserResult {
            sem_tokens: mcfile.tokens.clone(),
            sem_symbols: mcfile.symbols.clone(),
        });
    }

    for entry in binding.iter() {
        let key = entry.key();
        if key.ends_with(uri)
            || uri.ends_with(key)
            || key.ends_with(&canonical_uri)
            || canonical_uri.ends_with(key)
        {
            return Some(ParserResult {
                sem_tokens: entry.tokens.clone(),
                sem_symbols: entry.symbols.clone(),
            });
        }
    }

    None
}

fn remove_defines(uri: &McURI) {
    // Note: DashMap's iter() is read-only iteration, won't block write operations, suitable for collecting keys to delete first

    // workspace tables
    let to_remove: Vec<McSpaceName> = workspace::WORKSPACE
        .components
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        workspace::WORKSPACE.components.borrow().remove(&space_name);
    }

    let to_remove: Vec<McSpaceName> = workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        workspace::WORKSPACE.modules.borrow().remove(&space_name);
    }

    let to_remove: Vec<McSpaceName> = workspace::WORKSPACE
        .interfaces
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        workspace::WORKSPACE.interfaces.borrow().remove(&space_name);
    }

    let to_remove: Vec<McSpaceName> = workspace::WORKSPACE
        .enums
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        workspace::WORKSPACE.enums.borrow().remove(&space_name);
    }

    let to_remove: Vec<McSpaceName> = workspace::WORKSPACE
        .defines
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        workspace::WORKSPACE.defines.borrow().remove(&space_name);
    }

    // global tables (system lib registrations)
    let to_remove: Vec<McSpaceName> = global::mcc_components
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        global::mcc_components.borrow().remove(&space_name);
    }

    let to_remove: Vec<McSpaceName> = global::mcc_modules
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        global::mcc_modules.borrow().remove(&space_name);
    }

    let to_remove: Vec<McSpaceName> = global::mcc_interfaces
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        global::mcc_interfaces.borrow().remove(&space_name);
    }

    let to_remove: Vec<McSpaceName> = global::mcc_enums
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().clone())
        .collect();
    for space_name in to_remove {
        global::mcc_enums.borrow().remove(&space_name);
    }

    // Clear diagnostics for this file so they don't accumulate across edits
    workspace::WORKSPACE
        .diagnostics
        .borrow_mut()
        .clear_file(uri);
}

/// Pass2: Instantiation entry point
///
/// Find target module definition from global module table, create McModuleInst and execute instantiation.
/// Supports exact match and URI suffix match (solves canonical path vs relative path inconsistency).
pub(crate) fn mcb_pass2(entry: &McSpaceName) -> Result<MccProjectTree, Box<dyn Error>> {
    // FIX: Extract module def from prj_modules and DROP the MutexGuard
    // BEFORE calling inst.instantiate(). instantiate() internally calls
    // mcb_get_cmie() -> prj_modules.borrow() which would deadlock if the
    // lock is still held (std::sync::Mutex is NOT reentrant).
    //
    // We avoid returning DashMap Ref temporaries from block expressions,
    // which would extend their borrow lifetime past the MutexGuard drop.
    let matched_uri;
    let target_module_def;

    {
        let binding = workspace::WORKSPACE.modules.borrow();

        // 1. Exact match
        let exact = binding
            .get(entry)
            .map(|r| (entry.uri.clone(), r.value().clone()));

        if let Some((uri, def)) = exact {
            matched_uri = uri;
            target_module_def = def;
        } else {
            // 2. Suffix match fallback ("hbl.mc" vs "/abs/path/to/hbl.mc")
            let suffix = binding
                .iter()
                .find(|e| {
                    e.key().ident == entry.ident
                        && (e.key().uri.ends_with(&entry.uri) || entry.uri.ends_with(&e.key().uri))
                })
                .map(|e| (e.key().uri.clone(), e.value().clone()));

            if let Some((uri, def)) = suffix {
                matched_uri = uri;
                target_module_def = def;
            } else {
                let available: Vec<String> = binding
                    .iter()
                    .map(|e| format!("{}@{}", e.key().ident, e.key().uri))
                    .collect();
                return Err(format!(
                    "Target module not found: {} (uri={})\n  Available modules: [{}]",
                    entry.ident,
                    entry.uri,
                    available.join(", ")
                )
                .into());
            }
        }
    } // binding (MutexGuard) dropped here, BEFORE instantiate()

    let mut inst = McModuleInst::new(&entry.ident.to_string(), target_module_def);

    crate::current_uri::set(&matched_uri);

    inst.instantiate()
        .map_err(|e| -> Box<dyn Error> { Box::new(e) })?;

    Ok(inst)
}

/// Pass2 + Flatten: Instantiate and generate flattened instance table (Step 7)
///
/// First execute mcb_pass2 to build McModuleInst tree,
/// then flatten it into InstTable one-dimensional table.
pub fn mcb_pass2_flat(
    entry: &McSpaceName,
    start_id: u32,
) -> Result<(MccProjectTree, crate::instant::inst_table::InstTable), Box<dyn Error>> {
    let inst = mcb_pass2(entry)?;
    let table = crate::instant::inst_table::InstTable::from_module_inst(&inst, start_id);
    // ★ Electrical checks after pass2
    let net_results = crate::core::check::nets::run_net_checks(&table);
    let saved_uri = crate::current_uri::try_get();
    for r in &net_results {
        // Switch to the file this diagnostic belongs to
        if !r.uri.is_empty() {
            crate::current_uri::set(&crate::McURI::from(r.uri.as_str()));
        }
        let level = match r.severity {
            "error" => crate::builder::diagnostic::DiagnosticLevel::Error,
            "info" => crate::builder::diagnostic::DiagnosticLevel::Info,
            _ => crate::builder::diagnostic::DiagnosticLevel::Warning,
        };
        crate::builder::diagnostic::diagnostic_log(r.code, level, r.pos, 0, &r.message, &[]);
    }
    // Restore previous current_uri
    match saved_uri {
        Some(ref uri) => crate::current_uri::set(uri),
        None => crate::current_uri::reset(),
    }
    Ok((inst, table))
}

/// Get cmie in current file uri
///
/// Lookup order:
/// 1. mcode system library (globally unique)
/// 2. Current file's spacenames (defined in this file or imported via use)
/// 3. Spacename directly constructed from current file uri
/// 4. Iterate through all loaded files' spacenames (handle transitive dependencies)
/// 5. Directly lookup by name in global table
pub(crate) fn mcb_get_cmie(class_name: &McIds, uri: &McURI) -> Option<McCMIE> {
    let name_str = class_name.to_string();

    // ========== Re-entry guard ==========
    // Prevent mcb_get_cmie → parse_pass1_modules → McModule::new → mcb_get_cmie infinite recursion
    let guard_key = format!("{name_str}@{uri}");
    let is_reentrant = CMIE_RESOLVING.with(|set| !set.borrow_mut().insert(guard_key.clone()));
    if is_reentrant {
        warn!(
            target: "mcc::mcb_get_cmie",
            name = %name_str,
            uri = %uri,
            "reentrant call detected, breaking recursion"
        );
        return None;
    }
    // Auto-remove on function exit (using scopeguard pattern)
    struct CmieGuard(String);
    impl Drop for CmieGuard {
        fn drop(&mut self) {
            CMIE_RESOLVING.with(|set| set.borrow_mut().remove(&self.0));
        }
    }
    let _guard = CmieGuard(guard_key);

    // ═══════════════════════════════════════════════════════════════
    // Tier 1–3: Local scope lookup (current file → use chain → project)
    // Must run BEFORE library lookup so local definitions take priority.
    // ═══════════════════════════════════════════════════════════════

    let project_root = mcb_get_project_root();
    let project_root_str = project_root.to_string_lossy().to_string();

    // Helper: check if a candidate URI is "local" (under project root or = current URI).
    let is_local_uri = |u: &McURI| -> bool {
        let s = u.as_str();
        s == uri.as_str() || s.starts_with(&project_root_str)
    };

    // Track whether the name exists in local scope (for library-shadow warning).
    let name_found_in_local = false;

    // ── Tier 1: Current file's own definitions ─────────────────────
    if let Some(mcfile) = workspace::WORKSPACE.mcodes.borrow().get(uri) {
        if let Some(space_name) = mcfile.value().spacenames.get(class_name) {
            if let Some(cmie) = find_in_project_tables(space_name) {
                let _ = name_found_in_local; // reserved for future shadowing check
                return Some(cmie);
            }
        }
    }

    // ── Tier 2: $use chain (project-local imports) ─────────────────
    {
        let use_uris: Vec<String> =
            if let Some(mcfile) = workspace::WORKSPACE.mcodes.borrow().get(uri) {
                mcfile
                    .value()
                    .uselist
                    .iter()
                    .map(|u| canonicalize_project_uri(&u.uri))
                    .collect()
            } else {
                Vec::new()
            };

        for use_uri in &use_uris {
            if let Some(use_file) = workspace::WORKSPACE.mcodes.borrow().get(use_uri) {
                if let Some(space_name) = use_file.value().spacenames.get(class_name) {
                    if let Some(cmie) = find_in_project_tables(space_name) {
                        if is_local_uri(&space_name.uri) {
                            let _ = name_found_in_local; // shadowing tracking reserved
                            return Some(cmie);
                        }
                    }
                }
                // Name-exact match in use-file spacenames
                for (key, value) in use_file.value().spacenames.iter() {
                    if key.to_string() == name_str {
                        if let Some(cmie) = find_in_project_tables(value) {
                            if is_local_uri(&value.uri) {
                                let _ = name_found_in_local; // shadowing tracking reserved
                                return Some(cmie);
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Tier 3: All loaded project files (same name, local URI) ────
    {
        let space_name = McSpaceName::new(class_name, uri.clone());
        if let Some(cmie) = find_in_project_tables(&space_name) {
            let _ = name_found_in_local; // shadowing tracking reserved
            return Some(cmie);
        }
        for entry in workspace::WORKSPACE.mcodes.borrow().iter() {
            if let Some(space_name) = entry.value().spacenames.get(class_name) {
                if is_local_uri(&space_name.uri) {
                    if let Some(cmie) = find_in_project_tables(space_name) {
                        let _ = name_found_in_local; // shadowing tracking reserved
                        return Some(cmie);
                    }
                }
            }
        }
        // Global tables by name (local URI only)
        if let Some(cmie) = find_by_name_in_project_tables(class_name) {
            // Check whether the found cmie comes from a local URI
            let is_local = match &cmie {
                McCMIE::Component(c) => is_local_uri(&c.uri),
                McCMIE::Module(m) => is_local_uri(&m.uri),
                McCMIE::Interface(i) => is_local_uri(&i.uri),
                McCMIE::Enum(e) => is_local_uri(&e.uri),
            };
            if is_local {
                let _ = name_found_in_local; // shadowing tracking reserved
                return Some(cmie);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Tier 4: Library lookup (mcode + system dependencies)
    // Before returning, warn if the same name exists in local scope.
    // ═══════════════════════════════════════════════════════════════

    let mut found_in_blib: Option<(crate::builder::mc_code::McCode, McSpaceName)> = None;
    for entry in global::mcc_blibs.borrow().iter() {
        if entry.value().spacenames.get(class_name).is_some() {
            found_in_blib = Some((
                entry.value().clone(),
                entry.value().spacenames.get(class_name).unwrap().clone(),
            ));
            break;
        }
    }
    if let Some((mcode, space_name)) = found_in_blib.as_ref() {
        // Backtrack: check whether local scope already has this name
        if name_found_in_local {
            warn!(target: "mcc::resolve",
                "library definition '{}' from {} shadows local definition", name_str, space_name.uri);
        }
        if let Some(cmie) = find_in_project_tables(space_name) {
            return Some(cmie);
        }
        if let Some(found_comp) = global::mcc_components.borrow().get(space_name) {
            return Some(McCMIE::Component(found_comp.clone()));
        }
        if let Some(found_mod) = global::mcc_modules.borrow().get(space_name) {
            return Some(McCMIE::Module(found_mod.clone()));
        }
        if let Some(found_ifs) = global::mcc_interfaces.borrow().get(space_name) {
            return Some(McCMIE::Interface(found_ifs.clone()));
        }
        if let Some(found_enum) = global::mcc_enums.borrow().get(space_name) {
            return Some(McCMIE::Enum(found_enum.clone()));
        }
        {
            let mcodes = workspace::WORKSPACE.mcodes.borrow();
            let existing = mcodes.get(&space_name.uri).map(|e| e.value().clone());
            drop(mcodes);
            if let Some(mut existing) = existing {
                return existing.parse_cmie_single(&space_name.ident);
            }
        }
        if let Some(mut mcfile) = McCode::new(&space_name.uri, true) {
            mcfile.parse_ast_quiet();
            let result = mcfile.parse_cmie_single(&space_name.ident);
            workspace::WORKSPACE
                .mcodes
                .borrow()
                .insert(space_name.uri.clone(), mcfile);
            return result;
        }
        let _ = mcode;
    }

    // Fallback library lookup (when prj_mcodes is empty)
    let mcode_key = "mcode".to_string();
    if let Some(mcode) = global::mcc_blibs.borrow().get(&mcode_key) {
        for (_, space_name) in mcode.spacenames.iter() {
            if space_name.ident.to_string() == class_name.to_string() {
                if name_found_in_local {
                    warn!(target: "mcc::resolve",
                        "library definition '{}' shadows local definition", name_str);
                }
                let def_uri = &space_name.uri;
                if let Some(mut mcfile) = McCode::new(def_uri, true) {
                    mcfile.parse_ast_quiet();
                    mcfile.parse_nsp();
                    let result = mcfile.parse_cmie_single(&space_name.ident);
                    workspace::WORKSPACE
                        .mcodes
                        .borrow()
                        .insert(space_name.uri.clone(), mcfile);
                    return result;
                }
            }
        }
    }

    // ========== 2. Search current file's spacenames (exact match only) ==========
    let mut use_uris_for_step2c: Vec<String> = Vec::new();

    if let Some(mcfile) = workspace::WORKSPACE.mcodes.borrow().get(uri) {
        // 2a. Exact match
        if let Some(space_name) = mcfile.value().spacenames.get(class_name) {
            if let Some(cmie) = find_in_project_tables(space_name) {
                return Some(cmie);
            }
        }

        use_uris_for_step2c = mcfile
            .value()
            .uselist
            .iter()
            .map(|u| canonicalize_project_uri(&u.uri))
            .collect();
    }

    // 2c: Search through use-chain imported files' spacenames
    for use_uri in &use_uris_for_step2c {
        if let Some(use_file) = workspace::WORKSPACE.mcodes.borrow().get(use_uri) {
            // 2c-i. Exact match in use-imported file's spacenames
            if let Some(space_name) = use_file.value().spacenames.get(class_name) {
                if let Some(cmie) = find_in_project_tables(space_name) {
                    return Some(cmie);
                }
            }
            // 2c-ii. Exact match by name in use-imported file's spacenames
            for (key, value) in use_file.value().spacenames.iter() {
                let key_str = key.to_string();
                if key_str == name_str {
                    if let Some(cmie) = find_in_project_tables(value) {
                        return Some(cmie);
                    }
                }
            }

            // 2c-iii. Also try using the use_uri as the definition's URI
            let use_space_name = McSpaceName::new(class_name, use_uri.clone());
            if let Some(cmie) = find_in_project_tables(&use_space_name) {
                return Some(cmie);
            }
        }
    }

    // ========== 3. Direct lookup using class_name + uri construction ==========
    {
        let space_name = McSpaceName::new(class_name, uri.clone());
        if let Some(cmie) = find_in_project_tables(&space_name) {
            return Some(cmie);
        }
    }

    // ========== 4. Iterate through all loaded files' spacenames (exact match) ==========
    for entry in workspace::WORKSPACE.mcodes.borrow().iter() {
        // 4a. Exact match
        if let Some(space_name) = entry.value().spacenames.get(class_name) {
            if let Some(cmie) = find_in_project_tables(space_name) {
                return Some(cmie);
            }
        }
    }

    // ========== 5. Directly search by name in global table ==========
    // [Diagnostic] Step5: search by name in global table directly
    // eprintln!("[DIAG mcb_get_cmie] Step5: find_by_name for '{}'", name_str);
    if let Some(cmie) = find_by_name_in_project_tables(class_name) {
        return Some(cmie);
    }

    // [Diagnostic] Step 6: on-demand module resolution
    // eprintln!(
    //     "[DIAG mcb_get_cmie] Step6: on-demand resolution for '{}'",
    //     name_str
    // );
    // ========== 6. On-demand module resolution ==========
    // When definition not found in any table, it may be because
    // parse_pass1_modules hasn't run yet for the defining file.
    // Check spacenames to find which file should define it,
    // then trigger on-demand parse_pass1_modules for that file.
    {
        let mut target_uri: Option<String> = None;

        // 6a. Check current file's spacenames
        if let Some(mcfile) = workspace::WORKSPACE.mcodes.borrow().get(uri) {
            if let Some(space_name) = mcfile.value().spacenames.get(class_name) {
                trace!(
                    target: "mcc::mcb_get_cmie",
                    spacename_uri = %space_name.uri,
                    "step2a: exact match found"
                );
                target_uri = Some(space_name.uri.clone());
            }
        }

        // 6b. Trigger on-demand module parsing for the defining file
        if let Some(ref def_uri) = target_uri {
            let def_uri_canonical = canonicalize_project_uri(def_uri);

            let mcfile_clone = {
                let prj_mcodes = workspace::WORKSPACE.mcodes.borrow();
                prj_mcodes
                    .get(&def_uri_canonical)
                    .or_else(|| prj_mcodes.get(def_uri))
                    .map(|entry| entry.value().clone())
            };

            if let Some(mut mcfile) = mcfile_clone {
                let has_defs = workspace::WORKSPACE.modules.borrow().iter().any(|entry| {
                    entry.key().uri == def_uri_canonical || entry.key().uri == *def_uri
                }) || workspace::WORKSPACE
                    .interfaces
                    .borrow()
                    .iter()
                    .any(|entry| {
                        entry.key().uri == def_uri_canonical || entry.key().uri == *def_uri
                    })
                    || global::mcc_interfaces.borrow().iter().any(|entry| {
                        entry.key().uri == def_uri_canonical || entry.key().uri == *def_uri
                    });

                if !has_defs {
                    if global::mcc_parsing_modules
                        .insert(def_uri_canonical.clone(), ())
                        .is_some()
                    {
                        debug!(
                            target: "mcc::mcb_get_cmie",
                            uri = %def_uri_canonical,
                            "on-demand module parse"
                        );
                        let prev_uri = crate::current_uri::get();
                        crate::current_uri::set(&def_uri_canonical);
                        mcfile.parse_pass1_modules();
                        crate::current_uri::set(&prev_uri);
                        workspace::WORKSPACE
                            .mcodes
                            .borrow()
                            .insert(def_uri_canonical.clone(), mcfile);
                        global::mcc_parsing_modules.remove(&def_uri_canonical);
                    } else {
                        debug!(
                            target: "mcc::mcb_get_cmie",
                            uri = %def_uri_canonical,
                            "already being parsed, retry lookup"
                        );
                    }
                }

                if let Some(cmie) = find_by_name_in_project_tables(class_name) {
                    return Some(cmie);
                }
            } else if workspace::WORKSPACE.mcodes.borrow().get(def_uri).is_none()
                && workspace::WORKSPACE
                    .mcodes
                    .borrow()
                    .get(&def_uri_canonical)
                    .is_none()
            {
            } else if workspace::WORKSPACE.mcodes.borrow().get(def_uri).is_none()
                && workspace::WORKSPACE
                    .mcodes
                    .borrow()
                    .get(&def_uri_canonical)
                    .is_none()
            {
                // File not in mcodes yet, need to load and parse it
                let mcfile = McCode::new(def_uri, false);
                if let Some(mut mcfile) = mcfile {
                    debug!(
                        target: "mcc::mcb_get_cmie",
                        uri = %def_uri_canonical,
                        "loading file for on-demand parse"
                    );
                    mcfile.parse_ast();
                    mcfile.parse_nsp();
                    let prev_uri = crate::current_uri::get();
                    crate::current_uri::set(&def_uri_canonical);
                    mcfile.parse_pass1_modules();
                    crate::current_uri::set(&prev_uri);
                    workspace::WORKSPACE
                        .mcodes
                        .borrow()
                        .insert(def_uri_canonical.clone(), mcfile);

                    // Retry lookup after on-demand parse
                    if let Some(cmie) = find_by_name_in_project_tables(class_name) {
                        return Some(cmie);
                    }
                }
            }
        }
    }

    // [Diagnostic] all steps failed, output complete state information
    // eprintln!(
    //     "[DIAG mcb_get_cmie] === ALL STEPS FAILED === class_name='{}', uri='{}'",
    //     name_str, uri
    // );
    {
        let _mod_keys: Vec<String> = workspace::WORKSPACE
            .modules
            .borrow()
            .iter()
            .map(|e| format!("{}@{}", e.key().ident, e.key().uri))
            .collect();
        let _comp_keys: Vec<String> = workspace::WORKSPACE
            .components
            .borrow()
            .iter()
            .map(|e| format!("{}@{}", e.key().ident, e.key().uri))
            .collect();
        let _ifs_keys: Vec<String> = workspace::WORKSPACE
            .interfaces
            .borrow()
            .iter()
            .map(|e| format!("{}@{}", e.key().ident, e.key().uri))
            .collect();
        let _mcode_keys: Vec<String> = workspace::WORKSPACE
            .mcodes
            .borrow()
            .iter()
            .map(|e| e.key().clone())
            .collect();
        // eprintln!(
        //     "[DIAG] prj_mcodes keys({})={:?}",
        //     mcode_keys.len(),
        //     mcode_keys
        // );
        // eprintln!("[DIAG] prj_modules({})={:?}", mod_keys.len(), mod_keys);
        // eprintln!("[DIAG] prj_components({})={:?}", comp_keys.len(), comp_keys);
        // eprintln!("[DIAG] prj_interfaces({})={:?}", ifs_keys.len(), ifs_keys);
    }
    None
}

/// Look up CMIE in current file uri, also return URI of defining file
///
/// This is enhanced version of `mcb_get_cmie`, used in Pass2 instantiation when
/// both definition and source file information are needed.
/// For module type, source_uri is used to set submodule's def_uri,
/// ensuring current_uri context is correct during recursive instantiation.
pub(crate) fn mcb_get_cmie_with_uri(class_name: &McIds, uri: &McURI) -> Option<(McCMIE, McURI)> {
    let cmie = mcb_get_cmie(class_name, uri)?;

    // Find URI of defining file
    let source_uri = match &cmie {
        McCMIE::Module(_) => mcb_find_module_uri(class_name).unwrap_or_else(|| uri.clone()),
        McCMIE::Component(_) => {
            // Components also need correct URI, but component instantiation doesn't involve recursive context switching
            find_component_uri(class_name).unwrap_or_else(|| uri.clone())
        }
        McCMIE::Interface(_) => uri.clone(),
        McCMIE::Enum(_) => uri.clone(),
    };

    Some((cmie, source_uri))
}

/// Unified lookup for pass1/pass2 and F12 — returns (uri, span) for goto-def.
/// Reuses Tier 1–4 resolution from mcb_get_cmie.
pub fn unified_lookup(class_name: &str, from_uri: &McURI) -> Option<(McURI, Span)> {
    let ids = McIds::from(class_name);
    let (cmie, source_uri) = mcb_get_cmie_with_uri(&ids, from_uri)?;
    let span = match &cmie {
        McCMIE::Component(c) => c.span.clone(),
        McCMIE::Module(m) => m.span.clone(),
        McCMIE::Interface(i) => i.span.clone(),
        McCMIE::Enum(e) => e.span[0] as usize..e.span[1] as usize,
    };
    Some((source_uri, span))
}

/// Priority-based lookup using ScopePath.
///
/// Searches in 5 levels for a class-level (component/module/interface/enum/function)
/// definition matching `name`. Returns (uri, span, container_kind).
///
/// Priority: P1 (exact scope) → P2 (same container) → P3 (same file) →
///           P4 (use chain)   → P5 (project/libs).
pub fn unified_lookup_with_scope(
    name: &str,
    scope_path: &crate::ScopePath,
) -> Option<(McURI, Range<usize>, crate::ContainerKind)> {
    // P1-P2: search within current scope (container-aware)
    let ids = McIds::from(name);
    let (cmie, source_uri) = mcb_get_cmie_with_uri(&ids, &scope_path.uri)?;
    let span = match &cmie {
        McCMIE::Component(c) => c.span.clone(),
        McCMIE::Module(m) => m.span.clone(),
        McCMIE::Interface(i) => i.span.clone(),
        McCMIE::Enum(e) => e.span[0] as usize..e.span[1] as usize,
    };
    let kind = match &cmie {
        McCMIE::Component(_) => crate::ContainerKind::Component,
        McCMIE::Module(_) => crate::ContainerKind::Module,
        McCMIE::Interface(_) => crate::ContainerKind::Interface,
        McCMIE::Enum(_) => crate::ContainerKind::Enum,
    };
    Some((source_uri, span, kind))
}

/// Extended lookup: find a class definition, then optionally look up a sub-element
/// within it. Combines Phase 1 (parent container) and Phase 2 (sub-element) for
/// compound identifiers like `uC.PA1`.
pub fn lookup_with_sub(
    class_name: &str,
    sub_name: Option<&str>,
    sub_kind: Option<crate::SubElementKind>,
    from_uri: &McURI,
) -> Option<(McURI, Range<usize>)> {
    let (parent_uri, parent_span) = unified_lookup(class_name, from_uri)?;
    match (sub_name, sub_kind) {
        (Some(sub), Some(kind)) => {
            lookup_sub_def(&parent_uri, None, kind, sub).map(|span| (parent_uri, span))
        }
        _ => Some((parent_uri, parent_span)),
    }
}

/// Enumerate all visible symbols at a given ScopePath.
///
/// Searches in priority order (innermost → outermost):
///   1. Current function (params, labels)
///   2. Current container (ports, instances, functions)
///   3. Current file (modules, components, interfaces, enums)
///   4. Project files + use chain
///   5. System library (mcode)
///   6. Third-party libs
///
/// Returns up to `filter.limit` results, optionally filtered by kind and prefix.
pub fn unified_lookup_all(
    scope_path: &crate::ScopePath,
    filter: &crate::ScopeFilter,
) -> Vec<crate::LookupResult> {
    let max = filter.limit.unwrap_or(100);
    let mut results: Vec<crate::LookupResult> = Vec::new();

    // P1-P3: collect from workspace containers at this file
    collect_from_file(scope_path, filter, &mut results, max);

    // P4: project index (via mcb_get_cmie with all class names)
    collect_from_project(filter, &mut results, max);

    // P5-P6: system library + third-party (deferred to future enhancement)

    results.truncate(max);
    results
}

/// Collect symbols from the current file's containers.
fn collect_from_file(
    scope_path: &crate::ScopePath,
    filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    let uri = &scope_path.uri;
    let uri_str = uri.as_str();

    // Scan modules
    if filter
        .kind
        .map_or(true, |k| k == crate::ContainerKind::Module)
    {
        for entry in workspace::WORKSPACE.modules.borrow().iter() {
            if entry.key().uri.as_str() != uri_str {
                continue;
            }
            let m = entry.value();
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri: uri.clone(),
                    span: m.span.start..m.span.end,
                    kind: crate::LookupSymbolKind::Module,
                    container: Some(crate::ContainerInfo::new(
                        crate::ContainerKind::Module,
                        &m.name.to_string(),
                    )),
                    scope: m.name.to_string(),
                    name: m.name.to_string(),
                },
            );
            // Collect module ports and labels
            collect_module_symbols(m, scope_path, filter, results, max);
        }
    }

    // Scan components
    if filter
        .kind
        .map_or(true, |k| k == crate::ContainerKind::Component)
    {
        for entry in workspace::WORKSPACE.components.borrow().iter() {
            if entry.key().uri.as_str() != uri_str {
                continue;
            }
            let c = entry.value();
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri: uri.clone(),
                    span: c.span.start..c.span.end,
                    kind: crate::LookupSymbolKind::Component,
                    container: Some(crate::ContainerInfo::new(
                        crate::ContainerKind::Component,
                        &c.name.to_string(),
                    )),
                    scope: c.name.to_string(),
                    name: c.name.to_string(),
                },
            );
            // Collect component params, pins, funcs
            collect_component_symbols(c, scope_path, filter, results, max);
        }
    }
}

/// Collect ports, labels, instances from a module's insts.
fn collect_module_symbols(
    m: &crate::McModule,
    scope_path: &crate::ScopePath,
    filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    for (name, span) in m.insts.port_spans().iter() {
        if let Some(spans) = span.first() {
            let kind = if m.insts.get_label_kind(name) == crate::LabelKind::Explicit {
                crate::LookupSymbolKind::Label
            } else {
                crate::LookupSymbolKind::Port
            };
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri: scope_path.uri.clone(),
                    span: spans.clone(),
                    kind,
                    container: Some(scope_path.container.clone()),
                    scope: scope_path.scope_key(),
                    name: name.clone(),
                },
            );
        }
    }
    // Module funcs
    for func in m.funcs.iter() {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span: 0..0, // funcs don't have individual spans
                kind: crate::LookupSymbolKind::Function,
                container: Some(scope_path.container.clone()),
                scope: format!("{}.{}", scope_path.container.name, func.name),
                name: func.name.to_string(),
            },
        );
    }
}

/// Collect params, pins, funcs from a component.
fn collect_component_symbols(
    c: &crate::McComponent,
    scope_path: &crate::ScopePath,
    filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    let scope = scope_path.scope_key();
    // Component params
    for (name, span) in c.params.iter_defs_with_span() {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span,
                kind: crate::LookupSymbolKind::Param,
                container: Some(scope_path.container.clone()),
                scope: scope.clone(),
                name: name.to_string(),
            },
        );
    }
    // Component pins
    for (name, span) in &c.pins.pin_name_spans {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span: span.clone(),
                kind: crate::LookupSymbolKind::Pin,
                container: Some(scope_path.container.clone()),
                scope: scope.clone(),
                name: name.clone(),
            },
        );
    }
    // Component funcs
    for func in c.funcs.iter() {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span: 0..0,
                kind: crate::LookupSymbolKind::Function,
                container: Some(scope_path.container.clone()),
                scope: format!("{}.{}", scope, func.name),
                name: func.name.to_string(),
            },
        );
    }
}

/// Collect symbols from the project index (cross-file).
fn collect_from_project(
    filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    // Component classes
    for entry in workspace::WORKSPACE.components.borrow().iter() {
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        if !results.iter().any(|r: &crate::LookupResult| r.name == name) {
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri,
                    span: entry.value().span.start..entry.value().span.end,
                    kind: crate::LookupSymbolKind::Component,
                    container: None,
                    scope: String::new(),
                    name,
                },
            );
        }
    }
    // Module classes
    for entry in workspace::WORKSPACE.modules.borrow().iter() {
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        if !results.iter().any(|r: &crate::LookupResult| r.name == name) {
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri,
                    span: entry.value().span.start..entry.value().span.end,
                    kind: crate::LookupSymbolKind::Module,
                    container: None,
                    scope: String::new(),
                    name,
                },
            );
        }
    }
    // Interfaces
    for entry in workspace::WORKSPACE.interfaces.borrow().iter() {
        let name = entry.key().ident.to_string();
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: entry.key().uri.clone(),
                span: entry.value().span.start..entry.value().span.end,
                kind: crate::LookupSymbolKind::Interface,
                container: None,
                scope: String::new(),
                name,
            },
        );
    }
    // Enums
    for entry in workspace::WORKSPACE.enums.borrow().iter() {
        let name = entry.key().ident.to_string();
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: entry.key().uri.clone(),
                span: entry.value().span[0] as usize..entry.value().span[1] as usize,
                kind: crate::LookupSymbolKind::Enum,
                container: None,
                scope: String::new(),
                name,
            },
        );
    }
}

/// Add result if prefix matches and limit not reached.
fn add_result(results: &mut Vec<crate::LookupResult>, max: usize, result: crate::LookupResult) {
    if results.len() >= max {
        return;
    }
    results.push(result);
}

// ============================================================================
// Phase 2: Sub-element lookup (container-internal)
// ============================================================================

/// Kinds of sub-elements that can be looked up within a parent container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubElementKind {
    /// Component pin (e.g. `PA1` within `MCU.US513_20_F`)
    Pin,
    /// Module/component port in instances (e.g. `io VDD` within module)
    Port,
    /// Parameter declared in params section
    Param,
    /// Enum value within an enum definition
    EnumValue,
    /// Function defined within a module/component
    Func,
    /// Label (explicit or inline) within a module/component/function
    Label,
}

impl SubElementKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pin" => Some(Self::Pin),
            "port" => Some(Self::Port),
            "param" => Some(Self::Param),
            "enum_value" => Some(Self::EnumValue),
            "func" => Some(Self::Func),
            "label" => Some(Self::Label),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pin => "pin",
            Self::Port => "port",
            Self::Param => "param",
            Self::EnumValue => "enum_value",
            Self::Func => "func",
            Self::Label => "label",
        }
    }
}

/// Phase 2 lookup: find a sub-element (pin, port, param, enum value, func, label)
/// within a parent container identified by its definition URI and optional name.
///
/// Returns the byte range of the sub-element within the container's source file.
pub fn lookup_sub_def(
    parent_uri: &McURI,
    container_name: Option<&str>,
    kind: SubElementKind,
    name: &str,
) -> Option<Range<usize>> {
    let uri_str = parent_uri.as_str();

    // ── Components ──
    for entry in workspace::WORKSPACE.components.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_component(entry.value(), kind, name) {
            return Some(span);
        }
    }
    for entry in global::mcc_components.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_component(entry.value(), kind, name) {
            return Some(span);
        }
    }

    // ── Modules ──
    for entry in workspace::WORKSPACE.modules.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_module(entry.value(), kind, name) {
            return Some(span);
        }
    }
    for entry in global::mcc_modules.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_module(entry.value(), kind, name) {
            return Some(span);
        }
    }

    // ── Interfaces ──
    for entry in workspace::WORKSPACE.interfaces.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_interface(entry.value(), kind, name) {
            return Some(span);
        }
    }
    for entry in global::mcc_interfaces.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_interface(entry.value(), kind, name) {
            return Some(span);
        }
    }

    // ── Enums ──
    for entry in workspace::WORKSPACE.enums.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_enum(entry.value(), kind, name) {
            return Some(span);
        }
    }
    for entry in global::mcc_enums.borrow().iter() {
        let key = entry.key();
        if key.uri.as_str() != uri_str {
            continue;
        }
        if let Some(cn) = container_name {
            if key.ident.to_string() != cn {
                continue;
            }
        }
        if let Some(span) = lookup_in_enum(entry.value(), kind, name) {
            return Some(span);
        }
    }

    None
}

/// Helper: find a param def span by name using the public iterator.
fn find_param_def_span(
    params: &crate::core::basic::mc_param::McParamDeclares,
    name: &str,
) -> Option<Range<usize>> {
    for (n, span) in params.iter_defs_with_span() {
        if n == name {
            return Some(span);
        }
    }
    None
}

/// Helper: find a param port span by name using the public iterator.
fn find_param_port_span(
    params: &crate::core::basic::mc_param::McParamDeclares,
    name: &str,
) -> Option<Range<usize>> {
    for (n, span) in params.iter_ports_with_span() {
        if n == name {
            return Some(span);
        }
    }
    None
}

/// Look up a sub-element within a [`McComponent`].
fn lookup_in_component(
    comp: &crate::core::component::McComponent,
    kind: SubElementKind,
    name: &str,
) -> Option<Range<usize>> {
    match kind {
        SubElementKind::Pin => comp.pins.pin_name_spans.get(name).cloned(),
        SubElementKind::Port | SubElementKind::Label => {
            // Component-level insts (labels, buses)
            comp.insts.get_port_span(name)
        }
        SubElementKind::Param => find_param_def_span(&comp.params, name),
        SubElementKind::Func => {
            // Function span: we don't have a span on McFunction, so return None.
            // Callers should use the lapper entry for function definitions.
            None
        }
        SubElementKind::EnumValue => None,
    }
}

/// Look up a sub-element within a [`McModule`].
fn lookup_in_module(module: &McModule, kind: SubElementKind, name: &str) -> Option<Range<usize>> {
    match kind {
        SubElementKind::Pin => None,
        SubElementKind::Port | SubElementKind::Label => {
            // Module ports: try insts port_spans first, then params port_spans
            if let Some(span) = module.insts.get_port_span(name) {
                return Some(span);
            }
            find_param_port_span(&module.params, name)
        }
        SubElementKind::Param => find_param_def_span(&module.params, name),
        SubElementKind::Func => {
            // Function definition span — return None (use lapper)
            None
        }
        SubElementKind::EnumValue => None,
    }
}

/// Look up a sub-element within a [`McInterface`].
fn lookup_in_interface(
    iface: &crate::core::mc_ifs::McInterface,
    kind: SubElementKind,
    name: &str,
) -> Option<Range<usize>> {
    match kind {
        SubElementKind::Pin => iface.pins.pin_name_spans.get(name).cloned(),
        SubElementKind::Port | SubElementKind::Label => find_param_port_span(&iface.params, name),
        SubElementKind::Param => find_param_def_span(&iface.params, name),
        SubElementKind::Func => None,
        SubElementKind::EnumValue => None,
    }
}

/// Look up a sub-element within a [`McEnumDef`].
fn lookup_in_enum(
    enum_def: &crate::core::mc_enum::McEnumDef,
    kind: SubElementKind,
    name: &str,
) -> Option<Range<usize>> {
    match kind {
        SubElementKind::EnumValue => {
            for value in &enum_def.values {
                if value.name.to_string() == name {
                    return Some(value.span[0] as usize..value.span[1] as usize);
                }
            }
            None
        }
        _ => None,
    }
}

/// Find source URI of component definition
fn find_component_uri(class_name: &McIds) -> Option<McURI> {
    let name_str = class_name.to_string();
    for entry in workspace::WORKSPACE.components.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(entry.key().uri.clone());
        }
    }
    None
}

/// Look up CMIE in project global table (via McSpaceName)
fn find_in_project_tables(space_name: &McSpaceName) -> Option<McCMIE> {
    let canonical_uri = canonicalize_project_uri(&space_name.uri);
    let canonical_space_name = McSpaceName {
        ident: space_name.ident.clone(),
        uri: canonical_uri,
    };
    // eprintln!(
    //     "[DIAG find_in_project_tables] searching ident='{}', uri='{}' -> canonical='{}'",
    //     space_name.ident.to_string(),
    //     space_name.uri,
    //     canonical_space_name.uri
    // );
    if let Some(comp) = workspace::WORKSPACE
        .components
        .borrow()
        .get(&canonical_space_name)
    {
        return Some(McCMIE::Component(comp.clone()));
    }
    if let Some(comp) = global::mcc_components.borrow().get(&canonical_space_name) {
        return Some(McCMIE::Component(comp.clone()));
    }
    if let Some(module) = workspace::WORKSPACE
        .modules
        .borrow()
        .get(&canonical_space_name)
    {
        return Some(McCMIE::Module(module.clone()));
    }
    if let Some(module) = global::mcc_modules.borrow().get(&canonical_space_name) {
        return Some(McCMIE::Module(module.clone()));
    }
    if let Some(ifs) = workspace::WORKSPACE
        .interfaces
        .borrow()
        .get(&canonical_space_name)
    {
        return Some(McCMIE::Interface(ifs.clone()));
    }
    if let Some(ifs) = global::mcc_interfaces.borrow().get(&canonical_space_name) {
        return Some(McCMIE::Interface(ifs.clone()));
    }
    if let Some(enum_def) = global::mcc_enums.borrow().get(&canonical_space_name) {
        return Some(McCMIE::Enum(enum_def.clone()));
    }
    if let Some(enum_def) = workspace::WORKSPACE
        .enums
        .borrow()
        .get(&canonical_space_name)
    {
        return Some(McCMIE::Enum(enum_def.clone()));
    }
    None
}

/// Look up directly in the global table by name (ignoring URI)
fn find_by_name_in_project_tables(class_name: &McIds) -> Option<McCMIE> {
    // eprintln!(
    //     "[DIAG find_by_name_in_project_tables] searching name='{}'",
    //     class_name.to_string()
    // );
    let name_str = class_name.to_string();

    // Check components (exact match)
    for entry in workspace::WORKSPACE.components.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Component(entry.value().clone()));
        }
    }
    for entry in global::mcc_components.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Component(entry.value().clone()));
        }
    }

    // Check modules (exact match)
    for entry in workspace::WORKSPACE.modules.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Module(entry.value().clone()));
        }
    }
    for entry in global::mcc_modules.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Module(entry.value().clone()));
        }
    }

    // Check interfaces
    for entry in workspace::WORKSPACE.interfaces.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Interface(entry.value().clone()));
        }
    }
    for entry in global::mcc_interfaces.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Interface(entry.value().clone()));
        }
    }

    // Check enums
    for entry in global::mcc_enums.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Enum(entry.value().clone()));
        }
    }
    for entry in workspace::WORKSPACE.enums.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Enum(entry.value().clone()));
        }
    }

    None
}

/// Find the source URI of a module definition (for setting current_uri context in Pass2)
///
/// Look up by name in prj_modules, return the URI of the file containing the module definition.
/// This is critical for cross-file module instantiation: symbol resolution inside submodules
/// must occur in the context of their defining file.
pub(crate) fn mcb_find_module_uri(class_name: &McIds) -> Option<McURI> {
    let name_str = class_name.to_string();
    for entry in workspace::WORKSPACE.modules.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(entry.key().uri.clone());
        }
    }
    None
}

pub fn mcb_print() {
    // Print system-level Interfaces (mcode directory)
    global::mcc_interfaces
        .borrow()
        .iter()
        .for_each(|interface| {
            println!("{}", interface.value().as_ref());
        });

    // Print project-level Interfaces
    workspace::WORKSPACE
        .interfaces
        .borrow()
        .iter()
        .for_each(|interface| {
            println!("{}", interface.value().as_ref());
        });

    workspace::WORKSPACE
        .components
        .borrow()
        .iter()
        .for_each(|component| {
            println!("{}", component.value().as_ref());
        });

    workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .for_each(|module| {
            println!("{}", module.value().as_ref());
        });

    // global::mcc_enums.borrow().iter().for_each(|enum_def| {
    //     println!("{:#?}", enum_def.value().as_ref());
    // });

    workspace::WORKSPACE
        .enums
        .borrow()
        .iter()
        .for_each(|enum_def| {
            println!("{}", enum_def.value().as_ref());
        });
}

// ============================================================================
// 🔑 New: function dedicated to printing Lines information
// ============================================================================

/// Print Lines information for all modules (used for drawing-side debugging)
pub fn mcb_print_lines() {
    let modules = workspace::WORKSPACE.modules.borrow();

    if modules.is_empty() {
        println!("⚠️  prj_modules is empty, no module definitions found");
        return;
    }

    println!("╠════════════════════════════════════════════════════════════════╣");
    println!(
        "║  Found {} modules                                              ",
        modules.len()
    );

    for entry in modules.iter() {
        let space_name = entry.key();
        let module_def = entry.value();

        println!("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("┃ Module: {}", module_def.name);
        println!("┃ URI: {}", space_name.uri);
        println!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        // Interface
        println!("┃ Interface:");
        println!(
            "┃   inputs:  {:?}",
            module_def
                .insts
                .inputs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        println!(
            "┃   outputs: {:?}",
            module_def
                .insts
                .outputs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        println!(
            "┃   bidirs:  {:?}",
            module_def
                .insts
                .bidirs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );

        // Symbol Table
        println!(
            "┃ Symbol Table ({} symbols):",
            module_def.insts.iter().count()
        );
        for (key, ident) in module_def.insts.iter() {
            println!("┃   - {} : {}", key, ident.get_name());
        }

        // Lines
        println!("┃ Lines ({} connections):", module_def.lines.len());

        for (i, line) in module_def.lines.iter().enumerate() {
            println!("┃");
            println!("┃   ┌─── Line[{i}] ───────────────────────────────");
            print_phrase_internal(line, "┃   │  ");
            println!("┃   └──────────────────────────────────────────────");
        }

        println!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }
}

/// Print an McPhrase
fn print_phrase_internal(phrase: &crate::core::basic::mc_phrase::McPhrase, prefix: &str) {
    use crate::core::basic::mc_endpoint::McEndpoint;
    use crate::core::basic::mc_phrase::McPhrase;
    match phrase {
        McPhrase::Series(phrases) => {
            if phrases.is_empty() {
                println!("{prefix}(empty seq)");
                return;
            }
            for (i, p) in phrases.iter().enumerate() {
                if i > 0 {
                    println!("{prefix}    │");
                    println!("{prefix}    â–¼");
                }
                print_phrase_internal(p, prefix);
            }
        }
        McPhrase::Parallel(phrases) => {
            println!("{}(Parallel {})", prefix, phrases.len());
            for (i, p) in phrases.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  [{i}]:"));
            }
        }
        McPhrase::Closure(c) => {
            println!("{}(closure {} lines)", prefix, c.body.len());
            for (i, p) in c.body.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  body[{i}]:"));
            }
        }
        McPhrase::Group(g) => {
            println!("{}(group {} items)", prefix, g.opds.len());
            for (i, p) in g.opds.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  [{i}]:"));
            }
        }
        McPhrase::FuncCall(f) => {
            // Check if it is a pre-closure pattern
            let is_pre_closure = if let Some(c) = &f.caller {
                if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                    let func_name_str = inner_fc.func_name.to_string();
                    func_name_str
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_uppercase())
                } else {
                    false
                }
            } else {
                false
            };

            print!("{prefix}(funcall: ");
            if let Some(c) = &f.caller {
                if is_pre_closure {
                    // Pre-closure: print ClassName(params) -> MethodName
                    if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                        print!("{}(", inner_fc.func_name);
                        let inner_params: Vec<String> =
                            inner_fc.params.iter().map(|p| format!("{p}")).collect();
                        print!("{})", inner_params.join(", "));
                    }
                    print!(" -> ");
                } else {
                    print_phrase_internal(c, "");
                    print!(".");
                }
            }
            print!("{}", f.func_name);
            let param_strs: Vec<String> = f.params.iter().map(|p| format!("{p}")).collect();
            // If in pre-closure mode, skip the leading "_" placeholder
            let display_params = if is_pre_closure && param_strs.first() == Some(&"_".to_string()) {
                &param_strs[1..]
            } else {
                &param_strs
            };
            print!("({})", display_params.join(", "));
            println!(")");
        }
        McPhrase::Member(inner, endpoint) => {
            print_phrase_internal(inner, prefix);
            println!("{prefix}    .{endpoint}");
        }
        McPhrase::Endpoint(McEndpoint::Node { input, output }) => {
            let input_str: Vec<String> = input.iter().map(|e| format!("{e}")).collect();
            let output_str: Vec<String> = output.iter().map(|e| format!("{e}")).collect();
            println!(
                "{}(node: {{{} | {}}})",
                prefix,
                input_str.join(", "),
                output_str.join(", ")
            );
        }
        McPhrase::Endpoint(ep) => {
            println!("{prefix}(endpoint: {ep})");
        }
        McPhrase::Multiple(phrases) => {
            println!("{}(multiple {} items)", prefix, phrases.len());
            for (i, p) in phrases.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  [{i}]:"));
            }
        }
        McPhrase::Transposed(inner) => {
            print!("{prefix}(transposed: ");
            print_phrase_internal(inner, "");
            println!(")");
        }
        McPhrase::Lead => {
            println!("{prefix}(lead)");
        }
    }
}

/// Get the number of all modules (for debugging)
pub fn mcb_module_count() -> usize {
    workspace::WORKSPACE.modules.borrow().len()
}

/// Get the name of the first module (for auto-detecting the top-level module)
pub fn mcb_get_first_module_name() -> Option<String> {
    workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .next()
        .map(|entry| entry.key().ident.to_string())
}

/// Get module name by matching URI suffix
pub fn mcb_get_module_name_by_uri(uri: &McURI) -> Option<String> {
    workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .find(|entry| entry.key().uri.ends_with(uri) || uri.ends_with(&entry.key().uri))
        .map(|entry| entry.key().ident.to_string())
}

/// Get the number of loaded components
pub fn mcb_component_count() -> usize {
    workspace::WORKSPACE.components.borrow().len()
}

/// Get all module names in a specific file (by URI)
pub fn mcb_get_modules_in_file(uri: &McURI) -> Vec<String> {
    workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().ident.to_string())
        .collect()
}
/// Recursively scan all .mc files in the directory
fn scan_mc_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip doc directory (documentation is not .mc definitions)
            if path.file_name().is_some_and(|n| n == "doc") {
                continue;
            }
            result.extend(scan_mc_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "mc") {
            result.push(path);
        }
    }
    result
}

/// Initialize system library: automatically scan all .mc files in the mcode/ directory
///
/// The system library does not require use statements; all definitions are globally available.
/// Similar to Python's builtins or C's standard header preloading.
///
/// system_root convention:
///   - MCC_SYSTEM_ROOT points to the data root directory
///   - System library is under system_root/mcode/
///   - If the environment variable is not set, defaults to ~/.mcode/mcode
///
/// Config-based loading:
///   - Check `libs.load` config (mcc.yaml or project.toml)
///   - If empty: do not load mcode by default
///   - If contains "mcode": load mcode library
pub fn mcb_init_system_lib() {
    use crate::cli::config::should_load_mcode;

    debug!(target: "mcc::sysinit", "start");

    // Check config to decide whether to load mcode
    let project_root = mcb_get_project_root();
    let project_root_ref: Option<&std::path::Path> = if project_root.as_os_str().is_empty() {
        None
    } else {
        Some(&project_root)
    };

    if !should_load_mcode(project_root_ref) {
        debug!(target: "mcc::sysinit", "mcode not in libs.load config, skipping");
        if !global::mcc_blibs.borrow().contains_key("mcode") {
            global::mcc_blibs
                .borrow_mut()
                .insert("mcode".to_string(), McCode::new_empty());
        }
        debug!(target: "mcc::sysinit", "system lib init done (skipped)");
        return;
    }

    let system_root = mcb_get_system_root();
    let mcode_root = if system_root.as_os_str().is_empty() {
        dirs::home_dir()
            .map(|h| h.join(".mcode").join("mcode"))
            .unwrap_or_else(|| PathBuf::from(".mcode/mcode"))
    } else {
        system_root.join("mcode")
    };
    trace!(target: "mcc::sysinit", root = ?mcode_root, "got mcode root");

    if mcode_root.exists() {
        lib_mgr::mcb_load_lib("mcode", &mcode_root);
        debug!(target: "mcc::sysinit", "system lib loaded");
    } else {
        debug!(target: "mcc::sysinit", "mcode directory not found, registering builtins only");
        if !global::mcc_blibs.borrow().contains_key("mcode") {
            global::mcc_blibs
                .borrow_mut()
                .insert("mcode".to_string(), McCode::new_empty());
        }
    }

    debug!(target: "mcc::sysinit", "system lib init done");
}
pub fn mcb_interface_count() -> usize {
    workspace::WORKSPACE.interfaces.borrow().len() + global::mcc_interfaces.borrow().len()
}

// ============================================================================
// PR-3A: Definition traversal API — for CLI envelope's DefinitionsIndex use
// ============================================================================

/// Iterate all registered project module definitions, return (name, uri) pairs.
pub fn mcb_iter_modules() -> Vec<(String, String)> {
    workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .map(|entry| (entry.key().ident.to_string(), entry.key().uri.clone()))
        .collect()
}

/// Like `mcb_iter_modules` but includes source span for LSP goto-def.
pub fn mcb_iter_modules_with_span() -> Vec<(String, String, [usize; 2])> {
    workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .map(|entry| {
            let span = &entry.value().span;
            (
                entry.key().ident.to_string(),
                entry.key().uri.clone(),
                [span.start, span.end],
            )
        })
        .collect()
}

/// Iterate all registered component definitions (including project and system lib).
pub fn mcb_iter_components() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = workspace::WORKSPACE
        .components
        .borrow()
        .iter()
        .chain(global::mcc_components.borrow().iter())
        .map(|entry| (entry.key().ident.to_string(), entry.key().uri.clone()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

/// Like `mcb_iter_components` but includes source span for LSP goto-def.
pub fn mcb_iter_components_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = workspace::WORKSPACE
        .components
        .borrow()
        .iter()
        .chain(global::mcc_components.borrow().iter())
        .map(|entry| {
            let span = &entry.value().span;
            (
                entry.key().ident.to_string(),
                entry.key().uri.clone(),
                [span.start, span.end],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

/// Iterate all registered project interface definitions.
pub fn mcb_iter_interfaces() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = workspace::WORKSPACE
        .interfaces
        .borrow()
        .iter()
        .chain(global::mcc_interfaces.borrow().iter())
        .map(|entry| (entry.key().ident.to_string(), entry.key().uri.clone()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

/// Like `mcb_iter_interfaces` but includes source span for LSP goto-def.
pub fn mcb_iter_interfaces_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = workspace::WORKSPACE
        .interfaces
        .borrow()
        .iter()
        .chain(global::mcc_interfaces.borrow().iter())
        .map(|entry| {
            let span = &entry.value().span;
            (
                entry.key().ident.to_string(),
                entry.key().uri.clone(),
                [span.start, span.end],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

/// Iterate all registered enum definitions (both workspace and system library).
pub fn mcb_iter_enums() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = Vec::new();

    // Workspace enums (project files)
    for entry in workspace::WORKSPACE.enums.borrow().iter() {
        items.push((entry.key().ident.to_string(), entry.key().uri.clone()));
    }

    // System library enums (e.g. enum PKG in mcode/package.mc)
    for entry in global::mcc_enums.borrow().iter() {
        items.push((entry.key().ident.to_string(), entry.key().uri.clone()));
    }

    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

/// Same as `mcb_iter_enums`, but also returns the class span
/// `[start, end)` of the `enum PKG { ... }` head — needed by LSP
/// gotodef to know where to land when jumping to the class itself.
/// Includes both workspace and system library enums.
pub fn mcb_iter_enums_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<(String, String, [usize; 2])> = Vec::new();

    // Workspace enums (project files)
    let enums_guard = workspace::WORKSPACE.enums.borrow();
    for entry in enums_guard.iter() {
        let s = entry.value().span;
        items.push((
            entry.key().ident.to_string(),
            entry.key().uri.clone(),
            [s[0] as usize, s[1] as usize],
        ));
    }
    drop(enums_guard);

    // System library enums (e.g. enum PKG in mcode/package.mc)
    let sys_enums_guard = global::mcc_enums.borrow();
    for entry in sys_enums_guard.iter() {
        let s = entry.value().span;
        items.push((
            entry.key().ident.to_string(),
            entry.key().uri.clone(),
            [s[0] as usize, s[1] as usize],
        ));
    }
    drop(sys_enums_guard);

    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

/// Iterate all enum value rows project-wide (both workspace and system library).
/// Returns `Vec<(class, value, uri, [u32;2])>` sorted by class then value.
pub fn mcb_iter_enum_values() -> Vec<(String, String, String, [u32; 2])> {
    let mut items: Vec<(String, String, String, [u32; 2])> = Vec::new();

    // Iterate workspace enums (project files)
    let enums_guard = workspace::WORKSPACE.enums.borrow();
    for entry in enums_guard.iter() {
        let class = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        let enum_def = entry.value();
        for v in enum_def.values.iter() {
            let value_name = v.name.to_string();
            items.push((class.clone(), value_name, uri.clone(), v.span));
        }
    }
    drop(enums_guard);

    // Iterate system library enums (e.g. enum PKG in mcode/package.mc)
    let sys_enums_guard = global::mcc_enums.borrow();
    for entry in sys_enums_guard.iter() {
        let class = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        let enum_def = entry.value();
        for v in enum_def.values.iter() {
            let value_name = v.name.to_string();
            items.push((class.clone(), value_name, uri.clone(), v.span));
        }
    }
    drop(sys_enums_guard);

    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    items
}

/// Iterate all module port definitions (ps/io/in/out).
/// Returns Vec of (port_name, iotype, module_name, uri).
pub fn mcb_iter_ports() -> Vec<(String, String, String, String)> {
    use crate::core::common::IOType;

    let mut ports: Vec<(String, String, String, String)> = Vec::new();

    for entry in workspace::WORKSPACE.modules.borrow().iter() {
        let module_name = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        let module = entry.value();

        for (name, iotype) in module.insts.iter_ports() {
            let io_name = match iotype {
                IOType::Power => "power".to_string(),
                IOType::In => "input".to_string(),
                IOType::Out => "output".to_string(),
                IOType::InOut => "inout".to_string(),
                IOType::Analog => "analog".to_string(),
                IOType::Label => "label".to_string(),
                IOType::Return | IOType::NonCon | IOType::None => continue, // Skip non-port declarations
            };
            ports.push((name.to_string(), io_name, module_name.clone(), uri.clone()));
        }
    }

    ports.sort_by(|a, b| a.0.cmp(&b.0));
    ports
}

pub fn mcb_debug_get_cmie(class_name: &McIds, uri: &McURI) {
    let name_str = class_name.to_string();
    eprintln!("╔══════════════════════════════════════════════════════╗");
    eprintln!("â•' DEBUG mcb_get_cmie                                  â•'");
    eprintln!("â•' class_name: {name_str:40} â•'");
    eprintln!("â•' uri:        {uri:40} â•'");
    eprintln!("╠══════════════════════════════════════════════════════╣");

    // Step 1: system lib
    let mcode_found = global::mcc_blibs
        .borrow()
        .get(&"mcode".to_string())
        .is_some();
    eprintln!("â•' Step 1: mcode system lib exists = {mcode_found}");
    // [Diagnostic] Step 1: search in mcode base library
    if let Some(mcode) = global::mcc_blibs.borrow().get(&"mcode".to_string()) {
        let has_entry = mcode.spacenames.get(class_name).is_some();
        eprintln!("â•'   spacenames.get({name_str}) = {has_entry}");
        if has_entry {
            eprintln!(
                "║   ⚠️  System library hit! may return system library version (empty module)"
            );
        }
    }

    // Step 2: prj_mcodes
    let mcodes_has_uri = workspace::WORKSPACE.mcodes.borrow().get(uri).is_some();
    eprintln!("â•' Step 2: prj_mcodes.get(uri) = {mcodes_has_uri}");
    if let Some(mcfile) = workspace::WORKSPACE.mcodes.borrow().get(uri) {
        let has_spacename = mcfile.value().spacenames.get(class_name).is_some();
        eprintln!("â•'   spacenames.get({name_str}) = {has_spacename}");
        if let Some(sn) = mcfile.value().spacenames.get(class_name) {
            let sn_val = sn.clone();
            eprintln!("â•'   SpaceName.ident = {}", sn_val.ident);
            eprintln!("â•'   SpaceName.uri   = {}", sn_val.uri);
            let found = find_in_project_tables(&sn_val);
            eprintln!("â•'   find_in_project_tables = {}", found.is_some());
            if let Some(McCMIE::Module(m)) = &found {
                eprintln!(
                    "║   ✅ Module found! lines={}, symbols={}",
                    m.lines.len(),
                    m.insts.iter().count()
                );
            }
        }
    }

    // Step 3: direct construct
    let direct_sn = McSpaceName::new(&class_name.clone(), uri.clone());
    let direct_found = find_in_project_tables(&direct_sn);
    eprintln!(
        "â•' Step 3: direct SpaceName({}, {}) = {}",
        name_str,
        uri,
        direct_found.is_some()
    );

    // Step 5: by name
    let by_name = find_by_name_in_project_tables(class_name);
    eprintln!("â•' Step 5: find_by_name = {}", by_name.is_some());
    if let Some(McCMIE::Module(m)) = &by_name {
        eprintln!(
            "║   ✅ Module found! lines={}, symbols={}",
            m.lines.len(),
            m.insts.iter().count()
        );
    }

    // Full prj_modules state
    let modules = workspace::WORKSPACE.modules.borrow();
    eprintln!("╠══════════════════════════════════════════════════════╣");
    eprintln!("║ prj_modules status: {} modules", modules.len());
    for entry in modules.iter() {
        let key = entry.key();
        let val = entry.value();
        eprintln!(
            "║   {} (uri={}) → lines={}, symbols={}",
            key.ident,
            key.uri,
            val.lines.len(),
            val.insts.iter().count()
        );
    }
    eprintln!("╚══════════════════════════════════════════════════════╝");
}

/// 🆕 New API: directly look up module by name from prj_modules (bypasses mcb_get_cmie's URI matching issue)
///
/// This is the most reliable way to get a module definition, accessing the global table directly.
/// When mcb_get_cmie fails due to URI mismatch, use this function as a fallback.
pub fn mcb_get_module_def_by_name(class_name: &McIds) -> Option<Arc<McModule>> {
    let name_str = class_name.to_string();

    // Exact match
    for entry in workspace::WORKSPACE.modules.borrow().iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(entry.value().clone());
        }
    }

    None
}

/// 🆕 New API: get module definition with diagnostic information
///
/// Returns (module, diagnostics) tuple
/// diagnostics contains all information during the lookup process for easier troubleshooting
pub fn mcb_get_module_with_diagnostics(
    class_name: &McIds,
    uri: &McURI,
) -> (Option<Arc<McModule>>, Vec<String>) {
    let mut diags = Vec::new();
    let name_str = class_name.to_string();

    // 1. First try the standard path
    if let Some(McCMIE::Module(module)) = mcb_get_cmie(class_name, uri) {
        if module.lines.is_empty() && module.insts.iter().count() == 0 {
            diags.push(
                "⚠️  mcb_get_cmie returned an empty module (lines=0, symbols=0), trying fallback"
                    .to_string(),
            );
            // Standard path returned an empty module, go to fallback
        } else {
            diags.push(format!(
                "✅ mcb_get_cmie success: lines={}, symbols={}",
                module.lines.len(),
                module.insts.iter().count()
            ));
            return (Some(module), diags);
        }
    } else {
        diags.push("❌ mcb_get_cmie returned None".to_string());
    }

    // 2. Fallback: look up directly by name
    if let Some(module) = mcb_get_module_def_by_name(class_name) {
        diags.push(format!(
            "✅ fallback mcb_get_module_def_by_name success: lines={}, symbols={}",
            module.lines.len(),
            module.insts.iter().count()
        ));
        return (Some(module), diags);
    }

    diags.push(format!("❌ fallback also did not find module '{name_str}'"));

    // 3. List all known modules for reference
    let modules = workspace::WORKSPACE.modules.borrow();
    diags.push(format!("Registered modules ({}):", modules.len()));
    for entry in modules.iter() {
        diags.push(format!(
            "  - {} @ {} (lines={}, symbols={})",
            entry.key().ident,
            entry.key().uri,
            entry.value().lines.len(),
            entry.value().insts.iter().count()
        ));
    }
    (None, diags)
}

// ============================================================================
// Instance symbol registration for LSP goto-definition/references
// ============================================================================

use crate::ast::ast_semantic::{DeclareId, Span};

/// 🆕 Register an instance declaration (definition) in the global symbol table
///
/// Called when parsing `TypeName instanceName` in module body.
/// Returns the declare_id which can be used to register references later.
pub fn mcb_register_instance_decl(
    uri: &McURI,
    span: Span,
    name: Option<String>,
    scope: Option<&str>,
) -> Option<DeclareId> {
    let uri_str = uri.as_str();
    let span_clone = span.clone();
    if let Some(n) = name {
        let mut table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
        let id = table.add(uri_str, scope, &n, span_clone);
        tracing::debug!(target: "mcc::lsp", "Registered inst decl: {} scope={:?} at {:?} -> id={:?}", n, scope, span, id);
        Some(id)
    } else {
        None
    }
}

/// 🆕 Look up declare_id by instance name
///
/// Returns the DeclareId for a given instance name, if registered.
pub fn mcb_lookup_instance_decl(uri: &McURI, name: &str, scope: Option<&str>) -> Option<DeclareId> {
    let uri_str = uri.as_str();
    let table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
    table.get(uri_str, scope, name)
}

/// 🆕 Register an instance reference in the global symbol table
///
/// Called when an instance name is used elsewhere in the module (e.g., `uC.i2c()`).
/// The reference is linked to the declaration via decl_id.
pub fn mcb_register_instance_ref(uri: &McURI, span: Span, decl_id: DeclareId, scope: Option<&str>) {
    let uri_str = uri.as_str();
    let span_clone = span.clone();
    let mut table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
    table.add_ref(decl_id, uri_str, scope, span);
    tracing::info!(target: "mcc::lsp", "Registered inst ref: decl_id={:?} scope={:?} at {:?}", decl_id, scope, span_clone);
}

/// M6: Get all references for a named declaration.
/// Returns Vec<(uri, scope, span)>.
pub fn mcb_get_refs(name: &str) -> Vec<(String, String, Span)> {
    let table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
    let decl_ids = table.find_decls_by_name(name);
    let mut results = Vec::new();
    for decl_id in &decl_ids {
        results.extend(table.get_refs(*decl_id));
    }
    results
}

/// 🆕 Register a class reference for goto-definition
///
/// Called when a class name is used in a declare statement (e.g., `MCU.US513_20_F uC`).
/// Registers the class reference so LSP can jump from the reference to the class definition.
pub fn mcb_register_declare_class(uri: &McURI, class_name: &str, span: Span) {
    // Step 1: Find (class_id, target_uri, target_span) — try global_class_table first
    // Priority: same URI as reference > other URIs (for duplicate class definitions)
    let uri_str = uri.to_string();
    let found = {
        let class_table = workspace::WORKSPACE.global_class_table.lock().unwrap();
        tracing::debug!(target: "mcc::lsp", "  register_declare_class: global_class_table size={}", class_table.len());

        // First try: exact URI match (same file as reference)
        let same_uri_result = class_table.iter().find_map(
            |((target_uri, _kind, name), &(class_id, ref target_span))| {
                if name == class_name && target_uri == &uri_str {
                    Some((class_id, target_uri.clone(), target_span.clone()))
                } else {
                    None
                }
            },
        );

        // Second try: different URI (fallback for cross-file references)
        let other_uri_result = if same_uri_result.is_none() {
            class_table.iter().find_map(
                |((target_uri, _kind, name), &(class_id, ref target_span))| {
                    if name == class_name && target_uri != &uri_str {
                        Some((class_id, target_uri.clone(), target_span.clone()))
                    } else {
                        None
                    }
                },
            )
        } else {
            None
        };

        let result = same_uri_result.or(other_uri_result);
        if result.is_none() {
            tracing::debug!(target: "mcc::lsp", "  register_declare_class: global_class_table miss for '{}'", class_name);
        } else {
            tracing::info!(target: "mcc::lsp", "  register_declare_class: global_class_table hit for '{}'", class_name);
        }
        result
    };

    // Step 2: Try workspace files' global tables if not found above
    let from_mcodes: Option<(DeclareId, String, Span)> = if found.is_none() {
        let binding = workspace::WORKSPACE.mcodes.borrow();
        let mut result = None;
        for entry in binding.iter() {
            if let Ok(sem) = entry.value().symbols.lock() {
                if let Ok(gt) = sem.global_table.lock() {
                    for ((file_uri, name), &cid) in gt.class_name_to_id.iter() {
                        if name == class_name {
                            if let Some((_, tspan)) = gt.class_id_to_span.get(&cid) {
                                result = Some((cid, file_uri.clone(), tspan.clone()));
                                break;
                            }
                        }
                    }
                }
            }
            if result.is_some() {
                break;
            }
        }
        result
    } else {
        None
    };

    let class_info = if let Some(info) = found {
        Some(info)
    } else {
        from_mcodes
    };

    // Step 3: Store in workspace-level table
    if let Some((class_id, target_uri, target_span)) = class_info {
        let span_clone = span.clone();
        let uri_str = uri.to_string();
        tracing::info!(target: "mcc::lsp", "  register_declare_class: storing ref decl_span={:?} -> class_id={:?} target={}", span_clone, class_id, target_uri);
        let mut refs = workspace::WORKSPACE
            .global_declare_class_refs
            .lock()
            .unwrap();
        refs.entry(uri_str)
            .or_default()
            .push((span, class_id, target_uri, target_span));
        tracing::info!(target: "mcc::lsp", "Registered declare_class: {} at {:?} -> class_id={:?}", class_name, span_clone, class_id);
    } else {
        // ★ Diagnostic: class definition not found — emit warning for IDE.
        crate::builder::diagnostic::diagnostic_log(
            1601,
            crate::builder::diagnostic::DiagnosticLevel::Error,
            span.start as u32,
            (span.end - span.start) as u32,
            &format!("class '{}' not found", class_name),
            &[],
        );
        // ★ LSP: Even without cross-file resolution, register the class-name
        // span as a declare_class entry in the lapper.  This lets mcext's
        // F12 handler pick it up and resolve via project index.
        tracing::info!(target: "mcc::lsp", "register_declare_class: {} not resolved cross-file, registering local span {:?} for lapper", class_name, span);
        let uri_str = uri.to_string();
        // Use a synthetic sentinel: target_uri="" and target_span=[0,0].
        // create_lapper will emit DeclareClass for this span; mcext's
        // project-index fallback will resolve the actual definition.
        let mut refs = workspace::WORKSPACE
            .global_declare_class_refs
            .lock()
            .unwrap();
        refs.entry(uri_str)
            .or_default()
            .push((span, DeclareId::default(), "".to_string(), 0..0));
    }
}
