// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Go-to-definition — resolve a symbol name to its definition location.
//!
//! Extracted from `rpc/handlers/defs.rs` (handle_def).

use crate::query::iterators::{
    mcb_iter_components, mcb_iter_enums, mcb_iter_interfaces, mcb_iter_modules,
};
use crate::{McCMIE, McIds, McURI};
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
                        .get(def_entry.file_id as usize)
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

/// Resolve a symbol name to its definition, returning structured JSON.
/// Looks across components, modules, interfaces, and enums.
pub fn resolve(name: &str) -> Option<Value> {
    let (cmie, uri) = find_def_by_name_raw(name)?;
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
