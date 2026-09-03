// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::defspace::SourceDomain;
use crate::db::infra::mc_code::McCode;
use crate::McURI;
use dashmap;
use std::collections::HashSet;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tracing::{trace, warn};

use crate::build::pass1::canonicalize_project_uri;
use crate::db::infra::init::*;
// === pub fn mcb_add(uri: &McURI) { ===
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

        let binding = &workspace::WORKSPACE.mcodes;
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
        // §12.1 DefinitionSpace manifest: a single project file load.
        workspace::WORKSPACE
            .sources
            .insert(canonical_uri.clone(), SourceDomain::Project);
    }
    // T6-②: a single disk-file load round is self-contained (parse_pass1
    // derives modules too) — stamp one journal version when it changed the
    // definition space.
    crate::db::defregistry::checkpoint_if_changed();
}

// === pub fn mcb_add_from_string(uri: &McURI, content: &str) { ===
/// Load file from memory string (no disk dependency)
/// uri is virtual path (e.g., /mcc/s01/file.mc), content is .mc file content
/// Note: caller must set log flags via `mcc_reset()` before calling
pub fn mcb_add_from_string(uri: &McURI, content: &str) {
    let canonical_uri = canonicalize_project_uri(uri);
    tracing::info!(target: "mcc::lsp", "mcb_add_from_string: uri={:?} -> canonical={:?}", uri, canonical_uri);

    if let Some(mut mcfile) = McCode::new_from_string(&canonical_uri, content) {
        let already_exists = {
            let binding = &workspace::WORKSPACE.mcodes;
            binding.contains_key(&canonical_uri)
        };
        tracing::info!(target: "mcc::lsp", "mcb_add_from_string: already_exists={}", already_exists);
        if already_exists {
            remove_defines(&canonical_uri);
            // Also clear diagnostics for this file
            workspace::WORKSPACE
                .diagnostics
                .lock()
                .unwrap()
                .clear_file(&canonical_uri);
            tracing::info!(target: "mcc::lsp", "mcb_add_from_string: cleared diagnostics for {}", canonical_uri);
        }

        mcfile.parse_ast_from_string(content);
        mcfile.parse_nsp();
        mcfile.parse_pass1_types();
        // Module parsing is owned by mcb_parse_all_modules() (which every
        // caller of mcb_add_from_string invokes right after). Keeping the
        // module parse out of this file-level pass preserves the invariant
        // `modules_parsed == false` for files freshly parsed in this round,
        // so mcb_parse_all_modules' stale-diagnostic sweep does not wipe this
        // file's fresh parser/use-stage diagnostics before the topo loop
        // re-derives its modules and lapper.

        let binding = &workspace::WORKSPACE.mcodes;
        if already_exists {
            binding.insert(canonical_uri.clone(), mcfile);
        } else {
            binding.insert(canonical_uri.clone(), mcfile);
        }
        // §12.1 DefinitionSpace manifest: an in-memory project source load.
        workspace::WORKSPACE
            .sources
            .insert(canonical_uri.clone(), SourceDomain::Project);
        tracing::info!(target: "mcc::lsp", "mcb_add_from_string: added to workspace, keys count={}, all_keys={:?}",
            binding.len(), binding.iter().map(|e| e.key().clone()).collect::<Vec<_>>());
    } else {
        tracing::warn!(target: "mcc::lsp", "mcb_add_from_string: McCode::new_from_string returned None");
    }
}

// === System-library loading progress (interactive terminals only) ===

/// Library name currently being loaded (set by `mcb_load_lib`).
pub(crate) static CURRENT_LIB_NAME: Mutex<Option<String>> = Mutex::new(None);

/// Number of files parsed so far in the current system-library load.
static LIB_FILES_PARSED: AtomicUsize = AtomicUsize::new(0);

/// Characters written by the last progress line, for precise line clearing.
static LAST_PROGRESS_LEN: AtomicUsize = AtomicUsize::new(0);

/// Set (or clear) the library name reported by the loading progress line.
pub(crate) fn set_current_lib(name: Option<String>) {
    *CURRENT_LIB_NAME.lock().unwrap() = name;
    LIB_FILES_PARSED.store(0, Ordering::Relaxed);
    LAST_PROGRESS_LEN.store(0, Ordering::Relaxed);
}

/// Print a single self-overwriting progress line to stderr while a system
/// library is parsed file by file. A carriage return keeps everything on one
/// line; each new file overwrites the previous one. Active only on
/// interactive terminals so piped / CI / JSON-RPC output stays clean.
pub(crate) fn print_lib_progress(path: &str) {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let n = LIB_FILES_PARSED.fetch_add(1, Ordering::Relaxed) + 1;
    let name = CURRENT_LIB_NAME
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "lib".to_string());
    // Keep the line readable: show the path tail (ASCII-safe char slicing).
    let shown = if path.chars().count() > 60 {
        let tail: String = path.chars().skip(path.chars().count() - 60).collect();
        format!("...{tail}")
    } else {
        path.to_string()
    };
    let line = format!("loading lib {name}: {n} {shown} ...");
    let len = line.chars().count();
    eprint!("\r{line}");
    let _ = std::io::stderr().flush();
    LAST_PROGRESS_LEN.store(len, Ordering::Relaxed);
}

/// Clear the progress line (overwrite with spaces) once the library load ends.
pub(crate) fn clear_lib_progress() {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let len = LAST_PROGRESS_LEN.load(Ordering::Relaxed);
    if len > 0 {
        eprint!("\r{}\r", " ".repeat(len));
        let _ = std::io::stderr().flush();
    }
}

// === pub fn mcb_add_recursive(uri: &McURI, loaded: &mut HashSet<String>, is_system_li ===
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
/// mcb_add_recursive(&"main.mc".to_string(), &mut loaded);
/// ```
pub fn mcb_add_recursive(uri: &McURI, loaded: &mut HashSet<String>, is_system_lib: bool) {
    // 1. Normalize path, avoid duplicate loading
    let canonical_uri = canonicalize_project_uri(uri);
    trace!(target: "mcc::builder", uri = %uri, canonical = %canonical_uri, is_system_lib, "load: enter");

    if loaded.contains(&canonical_uri) {
        trace!(target: "mcc::builder", canonical = %canonical_uri, "load: skip (already loaded)");
        return;
    }

    // Optimization: a file whose types are already registered in the workspace
    // (pass1_complete) needs no disk re-read. Re-reading would replace the
    // in-memory entry with fresh disk state, wiping any synthetic virtual
    // modules (VIRT_*) installed since load and resetting modules_parsed —
    // which forces mcb_parse_all_modules to re-derive the whole workspace on
    // every load_project call (the "repeated loads get slower" regression).
    // The workspace entry is authoritative once loaded; edits arrive through
    // mcb_add_from_string / mcb_add, not through re-loading from disk. CLI
    // builds start from a fresh workspace, so nothing is skipped there; this
    // guard only short-circuits repeated server-side load_project calls.
    // A workspace entry with pass1_complete == false means an earlier load
    // aborted mid-parse — fall through and re-load from disk.
    if workspace::WORKSPACE
        .mcodes
        .get(&canonical_uri)
        .map(|e| e.pass1_complete)
        .unwrap_or(false)
    {
        trace!(target: "mcc::builder", canonical = %canonical_uri, "load: skip (already in workspace, pass1 complete)");
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

    // Single-line progress on interactive terminals while a system library
    // (e.g. mcode) is parsed file by file.
    if is_system_lib {
        print_lib_progress(&file_str);
    }

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

    // 5. Collect direct uses (cheap scan, no recursive traversal).
    //    This populates uselist so we know which dependencies to recurse into.
    let current_path = match PathBuf::from(&file_str).parent() {
        Some(p) => p.to_path_buf(),
        None => {
            warn!(target: "mcc::builder", file = %file_str, "load: cannot get parent path");
            return;
        }
    };
    mcfile.uselist = mcfile.collect_direct_uses(&current_path);

    // 5.5. First insert file into prj_mcodes (so dependency cycle detection works,
    //      and when parse_pass1_types() calls mcb_get_cmie to lookup Interface,
    //      it can find current file's spacenames in prj_mcodes).
    //      Note: spacenames are empty at this point — they will be computed
    //      in step 8 after all dependencies are loaded.
    workspace::WORKSPACE
        .mcodes
        .insert(canonical_uri.clone(), mcfile.clone());

    // §12.1 DefinitionSpace manifest: record which domain this source was
    // loaded into — project, or a system library (named by the loader's
    // current-lib context, set by `mcb_load_lib`).
    let domain = if is_system_lib {
        SourceDomain::SystemLib(
            CURRENT_LIB_NAME
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "mcode".to_string()),
        )
    } else {
        SourceDomain::Project
    };
    workspace::WORKSPACE
        .sources
        .insert(canonical_uri.clone(), domain);

    // 6. Mark as loaded (before recursion to prevent circular dependencies)
    loaded.insert(canonical_uri.clone());

    // 7. Recursively load all dependencies FIRST.
    //    This ensures dependencies' spacenames are computed before we
    //    compute the current file's spacenames.
    let deps: Vec<McURI> = mcfile.uselist.iter().map(|u| u.uri.clone()).collect();
    if !deps.is_empty() {
        trace!(target: "mcc::builder", file = %file_str, deps = deps.len(), "load: recurse into deps");
    }

    for dep_uri in deps {
        mcb_add_recursive(&dep_uri, loaded, is_system_lib);
    }

    // 8. After all dependencies are loaded, compute this file's spacenames
    //    using the dependencies' already-resolved spacenames from the workspace.
    //    This is a non-recursive lookup — unlike the old parse_nsp() which
    //    re-traversed the entire use graph independently (Defect 12).
    trace!(target: "mcc::builder", file = %file_str, "load: parse_nsp_from_deps");
    mcfile.parse_nsp_from_deps();

    // 9. After all dependencies are loaded, parse this file's CMIE definitions
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
            .entry(canonical_uri.clone())
            .and_modify(|entry| entry.spacenames.clone_from(&mcfile.spacenames));
    }
    // Note: the symbol lapper is not built here — parse_pass1_types only
    // registers CMIE definitions. Module parsing and create_lapper happen in
    // mcb_parse_all_modules(), which every project-load entry point calls.
    trace!(
        target: "mcc::builder",
        file = %file_str,
        "load: done"
    );

    // 10. Update project file table (replace pre-inserted empty file with parsed file)
    if let dashmap::Entry::Occupied(mut occupied_entry) =
        workspace::WORKSPACE.mcodes.entry(canonical_uri.clone())
    {
        occupied_entry.insert(mcfile);
    }
}

// === pub fn mcb_add_directory_recursive(root: &Path, loaded: &mut HashSet<String>) ===
/// Recursively load every `.mc` file under `root` (directory batch mode for a
/// build target with no project manifest).
///
/// Each file is loaded as its own entry with its own `use` closure (so a file
/// in a subfolder that `use ./utils/mcu`-style references a sibling resolves
/// against its own directory, matching browse mode). A shared `loaded` set
/// deduplicates files reachable through multiple closures. Callers run
/// `mcb_parse_all_modules` afterwards (see `mcc_load_directory_all`).
pub fn mcb_add_directory_recursive(root: &Path, loaded: &mut HashSet<String>) {
    for f in collect_mc_files(root) {
        let uri = McURI::from(f.to_string_lossy().as_ref());
        mcb_add_recursive(&uri, loaded, false);
    }
}

/// Recursively collect every `.mc` file under `root`, skipping hidden
/// directories (leading `.`). Sorted for deterministic order.
pub fn collect_mc_files(root: &Path) -> Vec<PathBuf> {
    fn walk(current: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|ext| ext == "mc") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

// === pub fn mcb_loaded_file_count() -> usize { ===
/// Get number of loaded files
pub fn mcb_loaded_file_count() -> usize {
    workspace::WORKSPACE.mcodes.len()
}

// === pub fn mcb_print_loaded_files() { ===
/// Print list of loaded files
pub fn mcb_print_loaded_files() {
    for _entry in workspace::WORKSPACE.mcodes.iter() {}
}

// === pub fn mcb_remove(uri: &McURI) { ===
/// Unload project file
pub fn mcb_remove(uri: &McURI) {
    let canonical_uri = canonicalize_project_uri(uri);

    remove_defines(uri);
    if canonical_uri != *uri {
        remove_defines(&canonical_uri);
    }

    let binding = &workspace::WORKSPACE.mcodes;
    binding.remove(uri);
    if canonical_uri != *uri {
        binding.remove(&canonical_uri);
    }

    // §12.1 DefinitionSpace manifest: drop the source entry.
    workspace::WORKSPACE.sources.remove(uri);
    if canonical_uri != *uri {
        workspace::WORKSPACE.sources.remove(&canonical_uri);
    }

    let extra_keys: Vec<String> = binding
        .iter()
        .filter(|entry| uri_equivalent(entry.key(), uri.as_str(), &canonical_uri))
        .map(|entry| entry.key().clone())
        .collect();
    for key in extra_keys {
        binding.remove(&key);
    }
    // T6-②: file-remove round end — stamp one journal version when the
    // removal tombstoned any definition (design §10: each load/change).
    crate::db::defregistry::checkpoint_if_changed();
}

// === fn remove_defines(uri: &McURI) { ===
/// Remove every definition this project file registered, from both physical
/// tables and the registry's project layer — delegated to the single write
/// entry (defregistry.rs). T8 (M2): the project layer is tombstoned while a
/// live same-key system-lib def the file was shadowing survives as the read
/// fallback (deleting a project source file never destroys mcode data).
pub(crate) fn remove_defines(uri: &McURI) {
    crate::db::defregistry::remove_project_by_uri(uri.as_str());
}
