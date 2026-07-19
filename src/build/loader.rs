// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::db::infra::mc_code::McCode;
use crate::{McSpaceName, McURI};
use dashmap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
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

// === pub fn mcb_add_from_string(uri: &McURI, content: &str) { ===
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

// === pub fn mcb_loaded_file_count() -> usize { ===
/// Get number of loaded files
pub fn mcb_loaded_file_count() -> usize {
    workspace::WORKSPACE.mcodes.borrow().len()
}

// === pub fn mcb_print_loaded_files() { ===
/// Print list of loaded files
pub fn mcb_print_loaded_files() {
    for _entry in workspace::WORKSPACE.mcodes.borrow().iter() {}
}

// === pub fn mcb_remove(uri: &McURI) { ===
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

// === fn remove_defines(uri: &McURI) { ===
pub(crate) fn remove_defines(uri: &McURI) {
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
