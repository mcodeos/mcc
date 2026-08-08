// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_semantic::SymbolKind;
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
                    .find(|((_fid, _cid, _fnid, name), _)| name.as_str() == name_str)
                    .map(|(_, (id, _))| *id);
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
                        .get(entry.def_loc.file_id as usize)
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

/// Resolve a member access on a CMIE instance via the class definition.
///
/// `mcb_get_cmie` handles P3→P4→P5 class lookup — same-file and cross-file
/// are treated identically. Returns the definition location and the appropriate
/// Ref SymbolKind. The caller creates a local Def via `register_def` and uses
/// the resulting DeclareId for Layer 2 Ref→Def matching.
///
/// e.g., `RES(10kΩ).Pullup(...)` → class="RES", member="Pullup"
///       → returns (res.mc, pullup_span_in_res_mc, FuncRef)
pub(crate) fn resolve_cmie_member(
    class_name: &str,
    member_name: &str,
    from_uri: &McURI,
) -> Option<(McURI, std::ops::Range<usize>, SymbolKind)> {
    let ids = McIds::from(class_name);
    let cmie = mcb_get_cmie(&ids, from_uri)?;

    match &cmie {
        McCMIE::Component(comp) => {
            if let Some(func) = comp.funcs.find(member_name) {
                let span = func.span.clone()?;
                return Some((comp.uri.clone(), span, SymbolKind::FuncRef));
            }
        }
        McCMIE::Module(mod_def) => {
            if let Some(func) = mod_def.funcs.find(member_name) {
                let span = func.span.clone()?;
                return Some((mod_def.uri.clone(), span, SymbolKind::FuncRef));
            }
        }
        McCMIE::Enum(enum_def) => {
            for value in &enum_def.values {
                if value.name.to_string() == member_name {
                    let span = value.span[0] as usize..value.span[1] as usize;
                    return Some((enum_def.uri.clone(), span, SymbolKind::EnumValRef));
                }
            }
        }
        // TODO: Interface ports
        _ => {}
    }
    None
}

// ============================================================================
// Scoped Enum Resolution
// ============================================================================

/// Find an enum whose name matches the component family name.
///
/// Example: `component CAP.CER` has family name `"CAP"`; this function looks
/// for `enum CAP` in workspace or global tables.
pub(crate) fn find_scoped_enum_for_component(
    comp_name: &McIds,
    uri: &McURI,
) -> Option<std::sync::Arc<crate::semantic::mc_enum::McEnumDef>> {
    let family_name = match comp_name.root_name() {
        Some(name) => name,
        None => return None,
    };

    let space_name = McSpaceName {
        ident: McIds::from(family_name.as_str()),
        uri: uri.clone(),
    };

    // Search workspace enums first, then global
    if let Some(entry) = workspace::WORKSPACE.enums.get(&space_name) {
        return Some(entry.value().clone());
    }
    if let Some(entry) = global::mcc_enums.get(&space_name) {
        return Some(entry.value().clone());
    }

    // Fallback: name-only search (for cross-file enums)
    for entry in workspace::WORKSPACE.enums.iter() {
        if entry.key().ident.to_string() == family_name {
            return Some(entry.value().clone());
        }
    }
    for entry in global::mcc_enums.iter() {
        if entry.key().ident.to_string() == family_name {
            return Some(entry.value().clone());
        }
    }

    None
}

/// Look up a bare identifier as a scoped enum value inside a component.
///
/// Returns `(uri, span, value_index)` if `bare_name` matches a value in the
/// enum that is scoped to the component identified by `comp_name`/`uri`.
pub(crate) fn lookup_scoped_enum_value(
    bare_name: &str,
    comp_name: &McIds,
    uri: &McURI,
) -> Option<(McURI, std::ops::Range<usize>, u32)> {
    let enum_def = find_scoped_enum_for_component(comp_name, uri)?;

    for (idx, value) in enum_def.values.iter().enumerate() {
        if value.name.to_string() == bare_name {
            let span = value.span[0] as usize..value.span[1] as usize;
            return Some((enum_def.uri.clone(), span, idx as u32));
        }
    }

    None
}

/// Check whether a class name refers to a known enum (in workspace or global tables).
pub(crate) fn is_enum_class_name(class_name: &str) -> bool {
    // Search workspace enums
    for entry in workspace::WORKSPACE.enums.iter() {
        if entry.key().ident.to_string() == class_name {
            return true;
        }
    }
    // Search global enums
    for entry in global::mcc_enums.iter() {
        if entry.key().ident.to_string() == class_name {
            return true;
        }
    }
    false
}

/// Check whether `value_name` is a valid member of the given enum class.
pub(crate) fn is_enum_member(class_name: &str, value_name: &str) -> bool {
    // Search workspace enums
    for entry in workspace::WORKSPACE.enums.iter() {
        if entry.key().ident.to_string() == class_name {
            return entry
                .value()
                .values
                .iter()
                .any(|v| v.name.to_string() == value_name);
        }
    }
    // Search global enums
    for entry in global::mcc_enums.iter() {
        if entry.key().ident.to_string() == class_name {
            return entry
                .value()
                .values
                .iter()
                .any(|v| v.name.to_string() == value_name);
        }
    }
    false
}

/// Resolve a bare identifier as an enum value by searching all known enums.
///
/// Returns `Some(class_name)` if `value_name` is a member of a known enum.
///
/// - If exactly one enum contains the value, return its class name.
/// - If multiple enums contain it, `prefer_class` is used as a tiebreaker
///   (e.g., the same-named component's enum).
/// - Returns `None` if no enum contains the value.
pub(crate) fn resolve_bare_enum_value(
    value_name: &str,
    prefer_class: Option<&str>,
) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();

    // Search workspace enums
    for entry in workspace::WORKSPACE.enums.iter() {
        let class_name = entry.key().ident.to_string();
        let has_value = entry
            .value()
            .values
            .iter()
            .any(|v| v.name.to_string() == value_name);
        if has_value {
            // Prefer exact match — return immediately
            if let Some(pref) = prefer_class {
                if class_name == pref {
                    return Some(class_name);
                }
            }
            candidates.push(class_name);
        }
    }
    // Search global enums
    for entry in global::mcc_enums.iter() {
        let class_name = entry.key().ident.to_string();
        let has_value = entry
            .value()
            .values
            .iter()
            .any(|v| v.name.to_string() == value_name);
        if has_value {
            if let Some(pref) = prefer_class {
                if class_name == pref {
                    return Some(class_name);
                }
            }
            candidates.push(class_name);
        }
    }

    // Return the unique candidate, or None if ambiguous/not found
    match candidates.len() {
        0 => None,
        1 => Some(candidates.into_iter().next().unwrap()),
        _ => {
            // Ambiguous: multiple enums have this value, and none matched prefer_class.
            // Return the first one (deterministic but arbitrary).
            Some(candidates.into_iter().next().unwrap())
        }
    }
}
