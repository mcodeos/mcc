// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §5.4.3 resolution policy: class name → CMIE definition.
//!
//!   ① RefDefMap name_index[(F, name)] — P3 (own file) + P4 (use chain), use-aware
//!   ② global::mcc_* name-only lookup — P5 (mcode system library)
//!
//! There is deliberately NO workspace-wide name-only scan: a definition in a
//! workspace file is visible from F only when F defines it (P3) or `use`s it
//! (P4). Everything else falls through to the mcode system library (P5).

use super::use_chain_reaches;
use crate::ast::ast_semantic::McSemSymbols;
use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::db::infra::init::interface_lookup;
use crate::semantic::common::{uri_intern, UriId};
use crate::{McCMIE, McIds, McSpaceName, McURI};
use tracing::trace;

/// URI-scoped string-level match in one kind's tables (workspace, then global).
///
/// The exact-key lookups can miss when `name` was rebuilt from a string:
/// `McIds::from(&str)` wraps the whole text in a single `Ida` segment, while
/// a dotted AST name such as `DCDC.LP3220AB5F` produces
/// `[Ida("DCDC"), DotIda("LP3220AB5F")]`. `McIds` equality is
/// segment-structure-sensitive (`normalized_eq_hash`), but both forms display
/// identically, so matching the display form under an explicit `uri_ok` gate
/// recovers the same definition. Every candidate is URI-scoped — this is
/// never a workspace-wide name-only scan (§5.4.5).
fn find_in_table_scoped(
    cmie_kind: u8,
    name_str: &str,
    uri_ok: impl Fn(&UriId) -> bool,
) -> Option<McCMIE> {
    match cmie_kind {
        0 => workspace::WORKSPACE
            .components
            .iter()
            .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
            .map(|e| McCMIE::Component(e.value().clone()))
            .or_else(|| {
                global::mcc_components
                    .iter()
                    .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
                    .map(|e| McCMIE::Component(e.value().clone()))
            }),
        1 => workspace::WORKSPACE
            .modules
            .iter()
            .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
            .map(|e| McCMIE::Module(e.value().clone()))
            .or_else(|| {
                global::mcc_modules
                    .iter()
                    .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
                    .map(|e| McCMIE::Module(e.value().clone()))
            }),
        2 => workspace::WORKSPACE
            .interfaces
            .iter()
            .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
            .map(|e| McCMIE::Interface(e.value().clone()))
            .or_else(|| {
                global::mcc_interfaces
                    .iter()
                    .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
                    .map(|e| McCMIE::Interface(e.value().clone()))
            }),
        3 => workspace::WORKSPACE
            .enums
            .iter()
            .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
            .map(|e| McCMIE::Enum(e.value().clone()))
            .or_else(|| {
                global::mcc_enums
                    .iter()
                    .find(|e| e.key().ident.to_string() == name_str && uri_ok(&e.key().uri))
                    .map(|e| McCMIE::Enum(e.value().clone()))
            }),
        _ => None,
    }
}

/// Kind-blind [`find_in_table_scoped`] across all four kinds, in priority
/// order (components → modules → interfaces → enums). Used by the P3
/// same-file and P4 use-chain fallbacks, which do not constrain the kind.
fn find_scoped_by_name(name: &McIds, uri_ok: impl Fn(&UriId) -> bool) -> Option<McCMIE> {
    let name_str = name.to_string();
    (0u8..=3).find_map(|k| find_in_table_scoped(k, &name_str, &uri_ok))
}

/// Single-table lookup keyed by the exact `McSpaceName` (ident + uri).
/// URI-scoped — never a name-only workspace scan.
fn lookup_cmie_by_kind(cmie_kind: u8, space_name: &McSpaceName) -> Option<McCMIE> {
    let name_str = space_name.ident.to_string();
    if cmie_kind == crate::ast::ast_semantic::CmieKind::UNKNOWN {
        // §5.4.6 A3: RefDefMap entries for class refs are matched with an
        // UNKNOWN kind (see matching.rs) — this is the normal state, not a
        // stale-map inconsistency. Resolve by exact key against every table;
        // still URI-scoped, never a name-only scan. The string-level fallback
        // recovers dotted names whose `McIds` segment form differs from the
        // AST-built table key (same def URI scope).
        return crate::query::lookup::find_in_project_tables(space_name)
            .or_else(|| find_scoped_by_name(&space_name.ident, |u| *u == space_name.uri));
    }
    match cmie_kind {
        0 => workspace::WORKSPACE
            .components
            .get(space_name)
            .or_else(|| global::mcc_components.get(space_name))
            .map(|c| McCMIE::Component(c.clone()))
            .or_else(|| find_in_table_scoped(0, &name_str, |u| *u == space_name.uri)),
        1 => workspace::WORKSPACE
            .modules
            .get(space_name)
            .or_else(|| global::mcc_modules.get(space_name))
            .map(|m| McCMIE::Module(m.clone()))
            .or_else(|| find_in_table_scoped(1, &name_str, |u| *u == space_name.uri)),
        2 => interface_lookup(space_name)
            .map(|i| McCMIE::Interface(i))
            .or_else(|| find_in_table_scoped(2, &name_str, |u| *u == space_name.uri)),
        3 => global::mcc_enums
            .get(space_name)
            .or_else(|| workspace::WORKSPACE.enums.get(space_name))
            .map(|e| McCMIE::Enum(e.clone()))
            .or_else(|| find_in_table_scoped(3, &name_str, |u| *u == space_name.uri)),
        _ => None,
    }
}

/// Extract the defining URI from a resolved CMIE. The definition itself is
/// the single source of truth — never re-resolve its URI by name.
pub(crate) fn cmie_uri(cmie: &McCMIE) -> Option<String> {
    match cmie {
        McCMIE::Component(c) => Some(c.uri.to_string()),
        McCMIE::Module(m) => Some(m.uri.to_string()),
        McCMIE::Interface(i) => Some(i.uri.to_string()),
        McCMIE::Enum(e) => Some(e.uri.to_string()),
    }
}

/// The single class-name resolution entry point (§5.4.3).
///
/// `from_uri` is the file containing the reference; resolution is relative to
/// that file's visibility set V(F) = P3(F) ∪ P4(F) ∪ P5.
pub struct Resolver;

impl Resolver {
    /// Resolve `name` in the context of `from_uri`.
    ///
    /// ① RefDefMap (§6.3 → §5): ID-based ClassRef via `name_to_declare_id`,
    ///    then name-based Use table (covers P3 + P4). A hit also implements P5
    ///    shadowing: when V(F) already contains the name, the mcode copy is
    ///    never reached.
    /// ② P3 exact-key fallback: the referencing file's own tables, keyed by
    ///    `McSpaceName { name, from_uri }` — used while the file's RefDefMap is
    ///    being consolidated (create_lapper runs before consolidate sets the
    ///    name_index). This is an O(1) URI-scoped lookup, NOT a name-only scan.
    /// ③ P5: mcode system library only (no workspace name-only scan).
    pub fn resolve_class(from_uri: &McURI, name: &McIds) -> Option<McCMIE> {
        // Workspace tables are keyed by canonical URIs (loader inserts under
        // `canonicalize_project_uri`), but callers such as the parse CLI may
        // pass the raw path (`/tmp/...` vs `/private/tmp/...`). Canonicalize
        // once up front so the P3/P4 lookups below hit the same keys — this is
        // the design-compliant replacement for the removed workspace-wide
        // name-only scan (§5.4.5).
        let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
        let from_uri = &McURI::from(canonical.as_str());
        if let Some(mcfile) = workspace::WORKSPACE.mcodes.get(from_uri) {
            if let Ok(sym) = mcfile.symbols.lock() {
                if let Some(cmie) = Self::resolve_class_locked(from_uri, name, &sym) {
                    return Some(cmie);
                }
            }
        }
        // The referencing file is not loaded (or its symbols lock is
        // poisoned): fall through to P3/P4/P5.
        Self::resolve_own_file(from_uri, name)
            .or_else(|| Self::resolve_use_chain(from_uri, name))
            .or_else(|| Self::resolve_system(name))
    }

    /// Variant of [`Self::resolve_class`] for callers that already hold the
    /// referencing file's `symbols` lock (e.g. `create_lapper` builds the
    /// lapper under a single `McSemSymbols` guard). Reading the RefDefMap from
    /// the caller's `sem` avoids a re-entrant `symbols.lock()` — the file's
    /// symbols is a `std::sync::Mutex`, which is not reentrant and would
    /// self-deadlock the calling thread.
    pub fn resolve_class_locked(
        from_uri: &McURI,
        name: &McIds,
        sem: &McSemSymbols,
    ) -> Option<McCMIE> {
        let name_str = name.to_string();

        // ① RefDefMap resolution (P3 + P4)
        if let Some(ref map) = sem.ref_def_map {
            // §6.3: search all scopes in name_to_declare_id for ClassRef entries
            let decl_id = sem
                .local_table
                .name_to_declare_id
                .iter()
                .find(|((_fid, _cid, _fnid, n), _)| n.as_str() == name_str)
                .map(|(_, (id, _))| *id);
            let id_hit = decl_id.and_then(|did| {
                map.get(
                    crate::ast::ast_semantic::SymbolKind::ClassRef,
                    u32::from(did),
                )
            });
            // §5: name-based Use table lookup
            let entry = id_hit.or_else(|| map.get_by_name(from_uri, &name_str));
            if let Some(entry) = entry {
                let def_uri =
                    crate::semantic::common::uri_of_file_id(entry.def_loc.file_id).to_string();
                trace!(target: "mcc::mcb_get_cmie", name = %name_str, def_uri = %def_uri, cmie_kind = entry.cmie_kind, "RefDefMap hit");
                // §5.4.6 A3: the RefDefMap entry must match a live table entry
                // by exact key — a stale map is an inconsistency to report, not
                // a reason to fall through to a name-only scan. The map's
                // interned file URIs can carry the raw path form (e.g. /tmp vs
                // /private/tmp on macOS) while workspace keys are canonical, so
                // canonicalize the def URI before the exact-key lookup.
                let space_name = if def_uri.is_empty() {
                    McSpaceName::new(name, def_uri.clone())
                } else {
                    McSpaceName::new(
                        name,
                        crate::build::pass1::canonicalize_project_uri(&def_uri),
                    )
                };
                if let Some(cmie) = lookup_cmie_by_kind(entry.cmie_kind, &space_name) {
                    return Some(cmie);
                }
            }
        }

        // ② P3: the referencing file's own definitions by exact key.
        if let Some(cmie) = Self::resolve_own_file(from_uri, name) {
            return Some(cmie);
        }

        // ③ P4: the referencing file's use chain — only while its RefDefMap
        // is not yet consolidated. Instance/class resolution runs inside
        // `McModule::new` and `create_lapper`, both before
        // `consolidate_ref_def_map` sets the name_index, so the map-based
        // P4 path (①) is unavailable there. The raw `uselist` walk applies
        // the same P4 visibility rule (see visibility.rs); when the map
        // exists it is authoritative and ① already covered P4.
        if sem.ref_def_map.is_none() {
            if let Some(cmie) = Self::resolve_use_chain(from_uri, name) {
                return Some(cmie);
            }
        }

        // ④ P5: mcode system library only.
        Self::resolve_system(name)
    }

    /// P3 exact-key lookup: `name` defined in `from_uri` itself. Used as a
    /// fallback before the file's RefDefMap is consolidated (create_lapper).
    fn resolve_own_file(from_uri: &McURI, name: &McIds) -> Option<McCMIE> {
        let space = McSpaceName::new(name, from_uri.clone());
        let exact = workspace::WORKSPACE
            .components
            .get(&space)
            .map(|c| McCMIE::Component(c.clone()))
            .or_else(|| {
                workspace::WORKSPACE
                    .modules
                    .get(&space)
                    .map(|m| McCMIE::Module(m.clone()))
            })
            .or_else(|| {
                workspace::WORKSPACE
                    .interfaces
                    .get(&space)
                    .map(|i| McCMIE::Interface(i.clone()))
            })
            .or_else(|| {
                workspace::WORKSPACE
                    .enums
                    .get(&space)
                    .map(|e| McCMIE::Enum(e.clone()))
            });
        if let Some(cmie) = exact {
            return Some(cmie);
        }
        // String-level same-file fallback: dotted names whose `McIds` segment
        // form (AST-built table key) differs from the lookup form
        // (`McIds::from(&str)`) display identically — see
        // `find_in_table_scoped`. Scoped to `from_uri` (P3), never a
        // workspace-wide name-only scan (§5.4.5).
        let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
        let canonical_id = uri_intern(&canonical);
        find_scoped_by_name(name, |u| *u == canonical_id)
    }

    /// P4 use-chain lookup while the referencing file's RefDefMap is not yet
    /// consolidated (see `resolve_class_locked` ③). Same P4 visibility rule
    /// as the name_index — only the source is different: the raw `uselist`
    /// walk instead of the merged map.
    fn resolve_use_chain(from_uri: &McURI, name: &McIds) -> Option<McCMIE> {
        let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
        find_scoped_by_name(name, |u| use_chain_reaches(&canonical, u.as_uri().as_ref()))
    }

    /// P5 lookup — global (mcode system library) tables by name only.
    pub fn resolve_system(name: &McIds) -> Option<McCMIE> {
        let name_str = name.to_string();
        for entry in global::mcc_components.iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Component(entry.value().clone()));
            }
        }
        for entry in global::mcc_modules.iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Module(entry.value().clone()));
            }
        }
        for entry in global::mcc_interfaces.iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Interface(entry.value().clone()));
            }
        }
        for entry in global::mcc_enums.iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Enum(entry.value().clone()));
            }
        }
        None
    }
}
