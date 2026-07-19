// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::builder::*;
use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::db::infra::mc_code::McCode;
use crate::{McCMIE, McIds, McSpaceName, McURI};
use std::cell::RefCell;
use std::collections::HashSet;
use tracing::{debug, trace};

thread_local! {
    static CMIE_RESOLVING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

use crate::build::pass1::canonicalize_project_uri;
use crate::query::lookup::find_by_name_in_project_tables;
use crate::query::lookup::find_in_project_tables;
use tracing::warn;
// === pub(crate) fn mcb_get_cmie(class_name: &McIds, uri: &McURI) -> Option<McCMIE> { ===
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

    let mut found_in_blib: Option<(crate::db::infra::mc_code::McCode, McSpaceName)> = None;
    for entry in crate::builder::mcc_blibs.borrow().iter() {
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
    if let Some(mcode) = crate::builder::mcc_blibs.borrow().get(&mcode_key) {
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

// === fn drop(&mut self) { ===

// === pub(crate) fn mcb_get_cmie_with_uri(class_name: &McIds, uri: &McURI) -> Option<( ===
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
