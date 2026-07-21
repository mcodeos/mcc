// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::query::lookup::{find_component_uri, mcb_find_module_uri};
use crate::{McCMIE, McIds, McSpaceName, McURI};
use std::cell::RefCell;
use std::collections::HashSet;
use tracing::trace;

thread_local! {
    static CMIE_RESOLVING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

use crate::query::lookup::find_by_name_in_project_tables;
use tracing::warn;

/// Direct single-table lookup using `cmie_kind` from RefDefMap entry.
/// Eliminates the 8-DashMap probing of `find_in_project_tables`.
fn lookup_cmie_by_kind(cmie_kind: u8, space_name: &McSpaceName) -> Option<McCMIE> {
    match cmie_kind {
        0 => workspace::WORKSPACE
            .components
            .get(space_name)
            .or_else(|| global::mcc_components.get(space_name))
            .map(|c| McCMIE::Component(c.clone())),
        1 => workspace::WORKSPACE
            .modules
            .get(space_name)
            .or_else(|| global::mcc_modules.get(space_name))
            .map(|m| McCMIE::Module(m.clone())),
        2 => workspace::WORKSPACE
            .interfaces
            .get(space_name)
            .or_else(|| global::mcc_interfaces.get(space_name))
            .map(|i| McCMIE::Interface(i.clone())),
        3 => global::mcc_enums
            .get(space_name)
            .or_else(|| workspace::WORKSPACE.enums.get(space_name))
            .map(|e| McCMIE::Enum(e.clone())),
        _ => None, // UNKNOWN — caller falls back to find_in_project_tables
    }
}

/// Resolve a CMIE class name to its definition using RefDefMap (§7).
///
/// Lookup path:
///   1. RefDefMap ID-based (O(1))
///   2. RefDefMap name_index / Use table (O(1))
///   3. Single DashMap.get via cmie_kind (O(1))
///   4. Re-entry: fall back to name-only search
///   5. RefDefMap miss: trigger on-demand parsing, then retry
#[allow(unused_assignments)]
pub(crate) fn mcb_get_cmie(class_name: &McIds, uri: &McURI) -> Option<McCMIE> {
    let name_str = class_name.to_string();

    // ========== Re-entry guard ==========
    let guard_key = format!("{name_str}@{uri}");
    let is_reentrant = CMIE_RESOLVING.with(|set| !set.borrow_mut().insert(guard_key.clone()));
    if is_reentrant {
        warn!(
            target: "mcc::mcb_get_cmie",
            name = %name_str,
            uri = %uri,
            "reentrant call detected, falling back to name-only lookup"
        );
        return find_by_name_in_project_tables(class_name);
    }
    struct CmieGuard(String);
    impl Drop for CmieGuard {
        fn drop(&mut self) {
            CMIE_RESOLVING.with(|set| set.borrow_mut().remove(&self.0));
        }
    }
    let _guard = CmieGuard(guard_key);

    // ═══════════════════════════════════════════════════════════════
    // RefDefMap resolution (§6.3 → §5 fallback)
    // §6.3: ID-based ClassRef lookup via name_to_declare_id (all scopes).
    // §5:   Name-based fallback via Use table (P3→P4→P5 priority).
    // ═══════════════════════════════════════════════════════════════
    if let Some(mcfile) = workspace::WORKSPACE.mcodes.get(uri) {
        if let Ok(sym) = mcfile.symbols.lock() {
            if let Some(ref map) = sym.ref_def_map {
                // §6.3: search all scopes in name_to_declare_id for ClassRef entries
                let decl_id = sym
                    .local_table
                    .name_to_declare_id
                    .iter()
                    .find(|((u, _s, name), _)| u == uri && name.as_str() == name_str)
                    .map(|(_, &id)| id);
                let id_hit = decl_id.and_then(|did| {
                    map.get(
                        crate::ast::ast_semantic::SymbolKind::ClassRef,
                        u32::from(did),
                    )
                });
                // §5: name-based Use table lookup
                let entry = id_hit.or_else(|| map.get_by_name(uri, &name_str));
                if let Some(entry) = entry {
                    let def_uri = map
                        .files
                        .get(entry.file_id as usize)
                        .cloned()
                        .unwrap_or_default();
                    trace!(target: "mcc::mcb_get_cmie", name = %name_str, def_uri = %def_uri, cmie_kind = entry.cmie_kind, "RefDefMap hit");
                    let space_name = McSpaceName::new(class_name, def_uri.clone());
                    if let Some(cmie) = lookup_cmie_by_kind(entry.cmie_kind, &space_name) {
                        return Some(cmie);
                    }
                    if let Some(cmie) = crate::query::lookup::find_in_project_tables(&space_name) {
                        return Some(cmie);
                    }
                }
            }
        }
    }

    // RefDefMap miss or not yet built: fall back to old name-only search
    find_by_name_in_project_tables(class_name)
}

pub(crate) fn mcb_get_cmie_with_uri(class_name: &McIds, uri: &McURI) -> Option<(McCMIE, McURI)> {
    let cmie = mcb_get_cmie(class_name, uri)?;
    let source_uri = match &cmie {
        McCMIE::Module(_) => mcb_find_module_uri(class_name).unwrap_or_else(|| uri.clone()),
        McCMIE::Component(_) => find_component_uri(class_name).unwrap_or_else(|| uri.clone()),
        McCMIE::Interface(_) => uri.clone(),
        McCMIE::Enum(_) => uri.clone(),
    };
    Some((cmie, source_uri))
}
