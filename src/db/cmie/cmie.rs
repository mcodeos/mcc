// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::resolve::{cmie_uri, Resolver};
use crate::{McCMIE, McIds, McSpaceName, McURI};
use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    static CMIE_RESOLVING: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

use tracing::warn;

/// Resolve a CMIE class name to its definition (§7), delegating the P3→P4→P5
/// visibility policy to [`Resolver::resolve_class`] (see `db/resolve/policy.rs`).
///
/// This function is a thin wrapper that adds re-entry protection; the policy
/// itself lives in one place so pass1 / pass2 / LSP all behave identically.
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
            "reentrant call detected, falling back to system-library lookup"
        );
        return Resolver::resolve_system(class_name);
    }
    struct CmieGuard(String);
    impl Drop for CmieGuard {
        fn drop(&mut self) {
            CMIE_RESOLVING.with(|set| set.borrow_mut().remove(&self.0));
        }
    }
    let _guard = CmieGuard(guard_key);

    Resolver::resolve_class(uri, class_name)
}

pub(crate) fn mcb_get_cmie_with_uri(class_name: &McIds, uri: &McURI) -> Option<(McCMIE, McURI)> {
    let cmie = mcb_get_cmie(class_name, uri)?;
    // The definition itself carries its source URI — never re-resolve by
    // name (a workspace-wide name-only scan would violate §5.4.5 and could
    // return a same-named def from an unrelated file).
    let source_uri = cmie_uri(&cmie).unwrap_or_else(|| uri.clone());
    Some((cmie, source_uri))
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
        uri: crate::semantic::common::uri_intern(uri),
    };

    // Search workspace enums first, then global (unified definition-space view;
    // design §12.4 rule 1).
    if let Some(entry) = crate::definition_space().get_enum(&space_name) {
        return Some(entry);
    }

    // Fallback: cross-file enums. §5.4 — a workspace enum is visible only
    // when its defining file is reachable through `uri`'s use chain, never
    // by bare name (a name-only scan could hit an unrelated same-named enum).
    for (sn, def) in crate::definition_space().workspace_enums() {
        if sn.ident.to_string() == family_name
            && crate::db::resolve::use_chain_reaches(uri, sn.uri.as_uri().as_ref())
        {
            return Some(def);
        }
    }
    // Fallback: per-world system-library enums (Phase 5 — name-only match,
    // any loaded system lib). O(1) name-index candidates, then pick the
    // enum kind.
    for hit in crate::db::defregistry::system_name_hits(&family_name) {
        if hit.kind != crate::db::defregistry::DefKind::Enum {
            continue;
        }
        if let Some((_, def)) = crate::db::defregistry::live_entry_by_id(hit.id) {
            if let crate::db::defregistry::DefValue::Enum(e) = def {
                return Some(e);
            }
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
    for (sn, _) in crate::definition_space().workspace_enums() {
        if sn.ident.to_string() == class_name {
            return true;
        }
    }
    // Search per-world system-library enums (Phase 5) — O(1) name index.
    crate::db::defregistry::system_name_hits(class_name)
        .iter()
        .any(|h| h.kind == crate::db::defregistry::DefKind::Enum)
}

/// Check whether `value_name` is a valid member of the given enum class.
pub(crate) fn is_enum_member(class_name: &str, value_name: &str) -> bool {
    // Search workspace enums
    for (sn, def) in crate::definition_space().workspace_enums() {
        if sn.ident.to_string() == class_name {
            return def.values.iter().any(|v| v.name.to_string() == value_name);
        }
    }
    // Search per-world system-library enums (Phase 5) — O(1) name index.
    for hit in crate::db::defregistry::system_name_hits(class_name) {
        if hit.kind != crate::db::defregistry::DefKind::Enum {
            continue;
        }
        if let Some((_, def)) = crate::db::defregistry::live_entry_by_id(hit.id) {
            if let crate::db::defregistry::DefValue::Enum(e) = def {
                return e.values.iter().any(|v| v.name.to_string() == value_name);
            }
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
    for (sn, def) in crate::definition_space().workspace_enums() {
        let class_name = sn.ident.to_string();
        let has_value = def.values.iter().any(|v| v.name.to_string() == value_name);
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
    // Search per-world system-library enums (Phase 5)
    for (sn, def) in crate::definition_space().system_enums() {
        let class_name = sn.ident.to_string();
        let has_value = def.values.iter().any(|v| v.name.to_string() == value_name);
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
