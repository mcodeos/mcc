// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Go-to-definition — resolve a symbol name to its definition location.
//!
//! Extracted from `rpc/handlers/defs.rs` (handle_def).

use crate::query::iterators::{
    mcb_iter_components, mcb_iter_enums, mcb_iter_interfaces, mcb_iter_modules,
};
use crate::{McCMIE, McIds, McSpaceName, McURI};
use serde_json::{json, Value};

/// Fast path: search RefDefMap name_index across all loaded files (§7.4).
/// Returns (def_uri_str, def_kind_name) if found, None otherwise.
fn find_def_in_refdefmap(name: &str) -> Option<(String, String)> {
    let workspace = &crate::db::cmie::tables::WORKSPACE;
    for entry in workspace.mcodes.iter() {
        let mcfile = entry.value();
        if let Ok(sym) = mcfile.symbols.lock() {
            if let Some(ref map) = sym.ref_def_map {
                if let Some(def_entry) = map.get_by_name(&mcfile.uri, name) {
                    let def_uri = map
                        .files
                        .get(def_entry.def_loc.file_id as usize)
                        .cloned()
                        .unwrap_or_default();
                    let def_kind = def_entry.def_kind.kind_name().to_string();
                    return Some((def_uri, def_kind));
                }
            }
        }
    }
    None
}

/// Low-level: find a definition by name across components/modules/interfaces/enums.
/// Returns the CMIE and its URI string. Used by both `resolve` (JSON) and
/// `find_def_by_name` (RPC handlers).
///
/// Tries RefDefMap fast path first (§7.4), falls back to O(n) project table scan.
pub fn find_def_by_name_raw(name: &str) -> Option<(McCMIE, String)> {
    // ★ Fast path: RefDefMap lookup (§7.4)
    if let Some((def_uri, _def_kind)) = find_def_in_refdefmap(name) {
        let ident = McIds::from(name);
        let uri_obj = McURI::from(def_uri.as_str());
        if let Some(cmie) = crate::get_def(&ident, &uri_obj) {
            return Some((cmie, def_uri));
        }
    }

    // Fallback: O(n) scan across all project tables
    let iterators: [Vec<(String, String)>; 4] = [
        mcb_iter_components(),
        mcb_iter_modules(),
        mcb_iter_interfaces(),
        mcb_iter_enums(),
    ];
    for items in &iterators {
        if let Some((matched, uri)) = items.iter().find(|(n, _)| n == name) {
            let ident = McIds::from(matched.as_str());
            let uri_obj = McURI::from(uri.as_str());
            if let Some(cmie) = crate::get_def(&ident, &uri_obj) {
                return Some((cmie, uri.clone()));
            }
        }
    }
    None
}

/// Find a definition by name, restricted to the visibility set V(F) of the
/// cursor file `from_uri` (§5.4): P3 (own file) + P4 (use chain) + P5 (mcode).
/// Never returns a definition from a file that F has not `use`d.
pub fn find_def_by_name_in_file(name: &str, from_uri: &str) -> Option<(McCMIE, String)> {
    let from_uri_obj = McURI::from(from_uri);

    // ① P3 + P4: the file's own symbols / use chain (RefDefMap name_index).
    // The file's symbols lock is released before `get_def`: class resolution
    // (Resolver) re-locks the same file's symbols, and std Mutex is not
    // reentrant — holding it across the call would self-deadlock.
    let def_uri = {
        let mcfile = crate::db::cmie::tables::WORKSPACE.mcodes.get(&from_uri_obj);
        let mut def_uri = String::new();
        if let Some(mcfile) = mcfile {
            if let Ok(sym) = mcfile.symbols.lock() {
                if let Some(ref map) = sym.ref_def_map {
                    if let Some(entry) = map.get_by_name(&from_uri_obj, name) {
                        def_uri = map
                            .files
                            .get(entry.def_loc.file_id as usize)
                            .cloned()
                            .unwrap_or_default();
                    }
                }
            }
        }
        def_uri
    };
    if !def_uri.is_empty() {
        let ident = McIds::from(name);
        if let Some(cmie) = crate::get_def(&ident, &McURI::from(def_uri.as_str())) {
            return Some((cmie, def_uri));
        }
    }

    // ② P5: mcode system library.
    let ident = McIds::from(name);
    let cmie = crate::db::resolve::Resolver::resolve_system(&ident)?;
    let uri = cmie_uri(&cmie)?;
    Some((cmie, uri))
}

/// Extract the defining URI from a resolved CMIE.
fn cmie_uri(cmie: &McCMIE) -> Option<String> {
    match cmie {
        McCMIE::Component(c) => Some(c.uri.to_string()),
        McCMIE::Module(m) => Some(m.uri.to_string()),
        McCMIE::Interface(i) => Some(i.uri.to_string()),
        McCMIE::Enum(e) => Some(e.uri.to_string()),
    }
}

/// Resolve a symbol name to its definition, returning structured JSON.
/// Looks across components, modules, interfaces, and enums.
pub fn resolve(name: &str) -> Option<Value> {
    let (cmie, uri) = find_def_by_name_raw(name)?;
    cmie_to_value(name, cmie, uri)
}

/// Resolve a symbol name to its definition within the cursor file's visibility
/// set V(F) (§5.4). A miss returns `None` — never a cross-file guess.
pub fn resolve_in_file(name: &str, from_uri: &str) -> Option<Value> {
    let (cmie, uri) = find_def_by_name_in_file(name, from_uri)?;
    // Final guard: the resolved definition must lie in V(F) (§5.4). Both the
    // name_index hit (P3/P4) and the mcode lookup (P5) satisfy this by
    // construction; the check guards against any future name-based fallback.
    let def = McSpaceName::new(&McIds::from(name), McURI::from(uri.as_str()));
    if !crate::db::resolve::is_visible(&McURI::from(from_uri), &def) {
        return None;
    }
    cmie_to_value(name, cmie, uri)
}

fn cmie_to_value(name: &str, cmie: McCMIE, uri: String) -> Option<Value> {
    match cmie {
        McCMIE::Component(c) => Some(json!({
            "kind": "component", "name": name, "uri": uri,
            "pin_count": c.pins.pins.len(),
        })),
        McCMIE::Module(m) => Some(json!({
            "kind": "module", "name": name, "uri": uri,
            "instance_count": m.insts.iter().count(),
        })),
        McCMIE::Interface(i) => Some(json!({
            "kind": "interface", "name": name, "uri": uri,
            "pin_count": i.pins.pins.len(),
        })),
        McCMIE::Enum(e) => Some(json!({
            "kind": "enum", "name": name, "uri": uri,
            "value_count": e.values.len(),
        })),
    }
}

/// Strict position-aware goto-def: lapper interval at `offset` + RefDefMap
/// exact resolution (shared with hover). Returns the def location and its real
/// kind (`ClassDef` / `EnumDef` / ...) — never a name-based guess, which would
/// misattribute same-name defs such as `enum CAP` vs `component CAP`.
/// Returns `None` when the position has no registered interval or no map entry.
pub fn resolve_at_pos(uri: &str, offset: usize) -> Option<Value> {
    use crate::refdef::query::resolve_at;

    let mc_uri = McURI::from(uri);
    let mcfile = crate::db::cmie::tables::WORKSPACE.mcodes.get(&mc_uri)?;
    let sym = mcfile.symbols.lock().ok()?;
    let map = sym.ref_def_map.as_ref()?;
    let hit = resolve_at(map, &sym.symbol_lapper, offset)?;

    Some(json!({
        "kind": hit.def_kind.kind_name(),
        "uri": hit.file_uri,
        "byte_start": hit.byte_start,
        "byte_end": hit.byte_end,
    }))
}
