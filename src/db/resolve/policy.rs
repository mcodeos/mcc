// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §5.4.3 resolution policy: class name → CMIE definition.
//!
//!   ① RefDefMap name_index[(F, name)] — P3 (own file) + P4 (use chain), use-aware
//!   ② registry system segment name-only lookup — P5 (per-world system lib)
//!
//! There is deliberately NO workspace-wide name-only scan: a definition in a
//! workspace file is visible from F only when F defines it (P3) or `use`s it
//! (P4). Everything else falls through to the per-world system library (P5).

use super::use_chain_reaches;
use crate::ast::sem::McSemSymbols;
use crate::db::cmie::tables as workspace;
use crate::db::defregistry::{DefKind, DefValue};
use crate::db::infra::init::interface_lookup;
use crate::semantic::common::{uri_intern, UriId};
use crate::{McCMIE, McIds, McSpaceName, McURI};
use tracing::trace;
/// Policy kind code (0=component, 1=module, 2=interface, 3=enum) → the
/// registry definition kind it resolves.
fn def_kind_of_cmie(cmie_kind: u8) -> Option<DefKind> {
    Some(match cmie_kind {
        0 => DefKind::Component,
        1 => DefKind::Module,
        2 => DefKind::Interface,
        3 => DefKind::Enum,
        _ => return None,
    })
}

/// URI-scoped member-set match in one kind's registry segments (project
/// domain first, then the per-world system name index — the same two
/// segments the physical workspace tables + system segment mirrored before
/// the read-side migration).
///
/// The exact-key lookups can miss when `name` was rebuilt from a string:
/// `McIds::from(&str)` wraps the whole text in a single `Ida` segment, while
/// a dotted AST name such as `DCDC.LP3220AB5F` produces
/// `[Ida("DCDC"), DotIda("LP3220AB5F")]`. `McIds` equality is
/// segment-structure-sensitive (`normalized_eq_hash`), but both forms display
/// identically, so `are_equivalent` (§8.7 — member-set comparison) recovers
/// the same definition under an explicit `uri_ok` gate; equal member sets
/// denote the same physical member, so a miss can never turn into a wrong
/// hit. Every candidate is URI-scoped — this is never a workspace-wide
/// name-only scan (§5.4.5).
pub(crate) fn find_in_table_scoped(
    cmie_kind: u8,
    name_str: &str,
    uri_ok: impl Fn(&UriId) -> bool,
) -> Option<McCMIE> {
    let Some(kind) = def_kind_of_cmie(cmie_kind) else {
        return None;
    };
    let query_ids = McIds::from(name_str);
    let eq = |ident: &McIds| crate::semantic::basic::equivalent::are_equivalent(ident, &query_ids);
    let ds = crate::definition_space();
    let project_hit: Option<McCMIE> = match kind {
        DefKind::Component => ds
            .workspace_components()
            .into_iter()
            .find(|(sn, _)| eq(&sn.ident) && uri_ok(&sn.uri))
            .map(|(_, c)| McCMIE::Component(c)),
        DefKind::Module => ds
            .workspace_modules()
            .into_iter()
            .find(|(sn, _)| eq(&sn.ident) && uri_ok(&sn.uri))
            .map(|(_, m)| McCMIE::Module(m)),
        DefKind::Interface => ds
            .workspace_interfaces()
            .into_iter()
            .find(|(sn, _)| eq(&sn.ident) && uri_ok(&sn.uri))
            .map(|(_, i)| McCMIE::Interface(i)),
        DefKind::Enum => ds
            .workspace_enums()
            .into_iter()
            .find(|(sn, _)| eq(&sn.ident) && uri_ok(&sn.uri))
            .map(|(_, e)| McCMIE::Enum(e)),
        _ => None,
    };
    project_hit.or_else(|| find_in_system_scoped(kind, name_str, &uri_ok))
}

/// P5 system-library fallback of [`find_in_table_scoped`]: O(1) name-index
/// candidates of one kind, then the same `eq` + `uri_ok` gate on each. The
/// registry's live value is fetched by id, so no full-segment scan runs on
/// every class reference.
fn find_in_system_scoped(
    kind: DefKind,
    name_str: &str,
    uri_ok: &impl Fn(&UriId) -> bool,
) -> Option<McCMIE> {
    let query_ids = McIds::from(name_str);
    let eq = |ident: &McIds| crate::semantic::basic::equivalent::are_equivalent(ident, &query_ids);
    crate::db::defregistry::system_name_hits(name_str)
        .into_iter()
        .filter(|h| h.kind == kind)
        .find_map(|h| {
            let (sn, def) = crate::db::defregistry::live_entry_by_id(h.id)?;
            if !(eq(&sn.ident) && uri_ok(&sn.uri)) {
                return None;
            }
            match (kind, def) {
                (DefKind::Component, DefValue::Component(c)) => Some(McCMIE::Component(c)),
                (DefKind::Module, DefValue::Module(m)) => Some(McCMIE::Module(m)),
                (DefKind::Interface, DefValue::Interface(i)) => Some(McCMIE::Interface(i)),
                (DefKind::Enum, DefValue::Enum(e)) => Some(McCMIE::Enum(e)),
                _ => None,
            }
        })
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
    if cmie_kind == crate::ast::sem::CmieKind::UNKNOWN {
        // §5.4.6 A3: RefDefMap entries for class refs are matched with an
        // UNKNOWN kind (see matching.rs) — this is the normal state, not a
        // stale-map inconsistency. Resolve by exact key against every table;
        // still URI-scoped, never a name-only scan. The string-level fallback
        // recovers dotted names whose `McIds` segment form differs from the
        // AST-built table key (same def URI scope).
        return crate::query::lookup::find_in_project_tables(space_name)
            .or_else(|| find_scoped_by_name(&space_name.ident, |u| *u == space_name.uri));
    }
    let ds = crate::definition_space();
    match cmie_kind {
        0 => ds
            .get_component(space_name)
            .map(McCMIE::Component)
            .or_else(|| find_in_table_scoped(0, &name_str, |u| *u == space_name.uri)),
        1 => ds
            .get_module(space_name)
            .map(McCMIE::Module)
            .or_else(|| find_in_table_scoped(1, &name_str, |u| *u == space_name.uri)),
        2 => interface_lookup(space_name)
            .map(|i| McCMIE::Interface(i))
            .or_else(|| find_in_table_scoped(2, &name_str, |u| *u == space_name.uri)),
        3 => ds
            .get_enum(space_name)
            .map(McCMIE::Enum)
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
            .or_else(|| Self::resolve_visibility(from_uri, name))
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
            // §6.3: search all scopes in name_to_declare_id for ClassRef entries.
            // ★ P0: use the reverse name index instead of a linear scan over
            // the whole `name_to_declare_id` table (was ~340us/call on the
            // mcode library's symbol table).
            let decl_id = sem
                .local_table
                .name_to_declare_ids
                .get(&name_str)
                .and_then(|scopes| scopes.first())
                .and_then(|(fid, cid, fnid)| {
                    sem.local_table
                        .name_to_declare_id
                        .get(&(*fid, *cid, *fnid, name_str.clone()))
                        .map(|(id, _)| *id)
                });
            let id_hit = decl_id
                .and_then(|did| map.get(crate::ast::sem::SymbolKind::ClassRef, u32::from(did)));
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
        let own_hit = Self::resolve_own_file(from_uri, name);
        if let Some(cmie) = own_hit {
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
            // Phase 6 (§13 delta 2): the visibility table is an O(1) P4 hit
            // derived from the same use edges; the chain walk below remains
            // the fallback for any table miss.
            let vis_hit = Self::resolve_visibility(from_uri, name);
            if let Some(cmie) = vis_hit {
                return Some(cmie);
            }
            let chain_hit = Self::resolve_use_chain(from_uri, name);
            if let Some(cmie) = chain_hit {
                return Some(cmie);
            }
        }

        // ④ P5: mcode system library only.
        Self::resolve_system(name)
    }

    /// P3 exact-key lookup: `name` defined in `from_uri` itself. Used as a
    /// fallback before the file's RefDefMap is consolidated (create_lapper).
    /// Reads the registry's project domain by exact key — the same entries
    /// the physical workspace tables mirrored for project defs (read-side
    /// migration); system-lib defs are never P3 for a referencing file.
    fn resolve_own_file(from_uri: &McURI, name: &McIds) -> Option<McCMIE> {
        let space = McSpaceName::new(name, from_uri.clone());
        let ds = crate::definition_space();
        let exact = ds
            .get_workspace_component(&space)
            .map(McCMIE::Component)
            .or_else(|| ds.get_workspace_module(&space).map(McCMIE::Module))
            .or_else(|| ds.get_workspace_interface(&space).map(McCMIE::Interface))
            .or_else(|| ds.get_workspace_enum(&space).map(McCMIE::Enum));
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

    /// Phase 6 (§13 delta 2): O(1) visibility-table hit. The table is derived
    /// from each file's `uselist` + `as_id` / `impt_ids` at parse_nsp time,
    /// so a hit is exactly the P4 target the scope-chain walk would produce
    /// (own-file shadowing already applied by the spacenames derivation).
    /// A miss — or an unloaded file, whose stale entries must not resurrect
    /// it — falls through to the chain walk unchanged.
    fn resolve_visibility(from_uri: &McURI, name: &McIds) -> Option<McCMIE> {
        let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
        let loaded = workspace::WORKSPACE.mcodes.contains_key(&canonical)
            || crate::db::infra::context::lookup_parsing_uses(&McURI::from(canonical.as_str()))
                .is_some();
        if !loaded {
            return None;
        }
        let sn = workspace::WORKSPACE
            .visibility
            .get(&(canonical, name.to_string()))?;
        crate::db::defregistry::cmie_by_identity(&sn)
    }

    /// P4 use-chain lookup while the referencing file's RefDefMap is not yet
    /// consolidated (see `resolve_class_locked` ③). Same P4 visibility rule
    /// as the name_index — only the source is different: the raw `uselist`
    /// walk instead of the merged map.
    fn resolve_use_chain(from_uri: &McURI, name: &McIds) -> Option<McCMIE> {
        let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
        find_scoped_by_name(name, |u| use_chain_reaches(&canonical, u.as_uri().as_ref()))
    }

    /// P5 lookup — the per-world system library (mcode etc.) by name only.
    /// O(1) name-index hits in kind-priority order (component → module →
    /// interface → enum), mirroring the pre-index per-kind scan order.
    pub fn resolve_system(name: &McIds) -> Option<McCMIE> {
        let name_str = name.to_string();
        let mut hit: Option<McCMIE> = None;
        for h in crate::db::defregistry::system_name_hits(&name_str) {
            let Some((_, def)) = crate::db::defregistry::live_entry_by_id(h.id) else {
                continue;
            };
            match (h.kind, def) {
                (DefKind::Component, DefValue::Component(c)) => {
                    hit = Some(McCMIE::Component(c));
                    break;
                }
                (DefKind::Module, DefValue::Module(m)) => {
                    hit = Some(McCMIE::Module(m));
                    break;
                }
                (DefKind::Interface, DefValue::Interface(i)) => {
                    hit = Some(McCMIE::Interface(i));
                    break;
                }
                (DefKind::Enum, DefValue::Enum(e)) => {
                    hit = Some(McCMIE::Enum(e));
                    break;
                }
                _ => continue,
            }
        }
        // Phase F (plan §9 F): record the circuit→def dependency edge here
        // too — this is the P5 tail of `resolve_class` and the direct
        // fallback of the enum-shadowing / re-entrancy paths that bypass the
        // `mcb_get_cmie` record point. No-op outside an instantiation window.
        if let Some(cmie) = &hit {
            crate::instant::deps::record_cmie(cmie);
        }
        hit
    }

    /// Interface-only class resolution for interface-binding syntax
    /// (`X::Y(role)` in pin options). The binding requires the class to be an
    /// interface; when a name collides across kinds (e.g. the mcode library
    /// defines both `component USB.MINIB` and `interface USB.MINIB`), the
    /// kind-blind [`Self::resolve_class`] resolves to the component and
    /// rejects the binding. This lookup follows the same visibility rules
    /// (P3 same file → P4 use chain → P5 mcode system library) restricted to
    /// the interface table, so the binding prefers the interface definition.
    pub fn resolve_interface(from_uri: &McURI, name: &McIds) -> Option<McCMIE> {
        let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
        let from_uri = &McURI::from(canonical.as_str());
        let name_str = name.to_string();

        // P3: interfaces defined in the referencing file itself. Exact-key
        // lookup first, then the string-level same-file fallback for dotted
        // names whose McIds segment form differs from the AST-built key.
        let space = McSpaceName::new(name, from_uri.clone());
        if let Some(i) = crate::definition_space().get_interface(&space) {
            return Some(McCMIE::Interface(i));
        }
        let canonical_id = uri_intern(&canonical);
        if let Some(i) = find_in_table_scoped(2, &name_str, |u| *u == canonical_id) {
            return Some(i);
        }

        // P4: interfaces reachable through the referencing file's use chain.
        if let Some(i) = find_in_table_scoped(2, &name_str, |u| {
            use_chain_reaches(&canonical, u.as_uri().as_ref())
        }) {
            return Some(i);
        }

        // P5: per-world system-library interfaces (name-only, interfaces table only).
        for (sn, i) in crate::db::defregistry::system_interfaces() {
            if sn.ident.to_string() == name_str {
                return Some(McCMIE::Interface(i));
            }
        }
        None
    }
}
