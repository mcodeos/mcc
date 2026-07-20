// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::query::lookup::{find_component_uri, mcb_find_module_uri};
use crate::{McCMIE, McIds, McSpaceName, McURI};
use std::cell::RefCell;
use std::collections::HashSet;
use tracing::trace;

thread_local! {
    static CMIE_RESOLVING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

use crate::query::lookup::find_by_name_in_project_tables;
use crate::query::lookup::find_in_project_tables;
use tracing::warn;
/// Resolve a CMIE class name to its definition using the RefDefMap Use table (§5).
///
/// Single lookup path: RefDefMap name_index → O(1).
/// If the file hasn't been parsed yet, triggers on-demand pass1 parsing first.
#[allow(unused_assignments)]
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
    // RefDefMap lookup — ID-based first (§6.3), name_index fallback (§5).
    // ═══════════════════════════════════════════════════════════════
    let mut has_refdefmap = false;
    if let Some(mcfile) = workspace::WORKSPACE.mcodes.get(uri) {
        if let Ok(sym) = mcfile.symbols.lock() {
            has_refdefmap = sym.ref_def_map.is_some();
            if let Some(ref map) = sym.ref_def_map {
                // §6.3: ID-based lookup — name_to_declare_id gives decl_id,
                // then RefDefMap.get((ClassRef, decl_id)) gives (uri, span).
                let decl_id = sym
                    .local_table
                    .name_to_declare_id
                    .get(&(uri.clone(), String::new(), name_str.clone()))
                    .copied();
                let id_hit = decl_id.and_then(|did| {
                    map.get(
                        crate::ast::ast_semantic::SymbolKind::ClassRef,
                        u32::from(did),
                    )
                });
                let entry = id_hit.or_else(|| {
                    // §5: name_index fallback (P3+P4+P5)
                    map.get_by_name(uri, &name_str)
                });
                if let Some(entry) = entry {
                    let def_uri = map
                        .files
                        .get(entry.file_id as usize)
                        .cloned()
                        .unwrap_or_default();
                    trace!(target: "mcc::mcb_get_cmie", name = %name_str, def_uri = %def_uri, "RefDefMap hit");
                    let space_name = McSpaceName::new(class_name, def_uri.clone());
                    if let Some(cmie) = find_in_project_tables(&space_name) {
                        return Some(cmie);
                    }
                    if let Some(cmie) = find_by_name_in_project_tables(class_name) {
                        return Some(cmie);
                    }
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // RefDefMap miss: file not yet parsed or class not found.
    // Trigger on-demand parsing, then retry RefDefMap.
    // ═══════════════════════════════════════════════════════════════
    if !has_refdefmap {
        // File hasn't been pass1-parsed yet — parse now
        if let Some(mut mcfile) = workspace::WORKSPACE.mcodes.get_mut(uri) {
            if !mcfile.pass1_complete {
                mcfile.parse_pass1_types();
            }
            if !mcfile.modules_parsed {
                // save/restore current_uri to avoid side effects
                let prev = crate::current_uri::get();
                crate::current_uri::set(uri);
                mcfile.parse_pass1_modules();
                crate::current_uri::set(&prev);
            }
            // Retry RefDefMap after parsing
            if let Ok(sym) = mcfile.symbols.lock() {
                if let Some(ref map) = sym.ref_def_map {
                    if let Some(entry) = map.get_by_name(uri, &name_str) {
                        let def_uri = map
                            .files
                            .get(entry.file_id as usize)
                            .cloned()
                            .unwrap_or_default();
                        let space_name = McSpaceName::new(class_name, def_uri);
                        if let Some(cmie) = find_in_project_tables(&space_name) {
                            return Some(cmie);
                        }
                        if let Some(cmie) = find_by_name_in_project_tables(class_name) {
                            return Some(cmie);
                        }
                    }
                }
            }
        }
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
