// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Query API for RefDefMap — goto-def, hover, find-references.
//!
//! Provides a clean facade for pass2 and LSP consumers (see design doc §16).

use crate::refdef::types::{RefDefEntry, RefDefMap, SymbolKind};

/// Result of a goto-definition lookup.
#[derive(Debug, Clone)]
pub struct GotoDefResult {
    pub file_uri: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub def_kind: SymbolKind,
}

/// F12 goto-def: resolve (ref_kind, ref_id) → definition location.
/// Returns None if the ref is not found in the map.
pub fn goto_def(map: &RefDefMap, ref_kind: SymbolKind, ref_id: u32) -> Option<GotoDefResult> {
    map.get(ref_kind, ref_id).map(|entry| {
        let file_uri = map
            .files
            .get(entry.def_loc.file_id as usize)
            .cloned()
            .unwrap_or_default();
        GotoDefResult {
            file_uri,
            byte_start: entry.def_loc.byte_start,
            byte_end: entry.def_loc.byte_end,
            def_kind: entry.def_kind,
        }
    })
}

/// Name-based lookup: find a definition by (file_uri, class_name).
/// Used for cross-file Use-table resolution (P3/P4/P5 priority).
pub fn goto_def_by_name(
    map: &RefDefMap,
    file_uri: &str,
    class_name: &str,
) -> Option<GotoDefResult> {
    map.get_by_name(file_uri, class_name).map(|entry| {
        let file_uri = map
            .files
            .get(entry.def_loc.file_id as usize)
            .cloned()
            .unwrap_or_default();
        GotoDefResult {
            file_uri,
            byte_start: entry.def_loc.byte_start,
            byte_end: entry.def_loc.byte_end,
            def_kind: entry.def_kind,
        }
    })
}

/// Find all references to a definition (requires def_to_refs reverse index).
pub fn find_refs(
    map: &RefDefMap,
    def_kind: SymbolKind,
    file_id: u32,
    byte_start: u32,
    byte_end: u32,
) -> &[(SymbolKind, u32)] {
    map.get_refs_for_def(def_kind, file_id, byte_start, byte_end)
}

/// Look up an entry by (ref_kind, ref_id) — raw access for advanced consumers.
pub fn lookup(map: &RefDefMap, ref_kind: SymbolKind, ref_id: u32) -> Option<&RefDefEntry> {
    map.get(ref_kind, ref_id)
}

/// Iterate all entries in the RefDefMap for diagnostic/debug purposes.
pub fn iter_entries(map: &RefDefMap) -> impl Iterator<Item = (&(SymbolKind, u32), &RefDefEntry)> {
    map.entries.iter()
}

/// Get the file URI for a file_id in the RefDefMap's file table.
pub fn file_uri(map: &RefDefMap, file_id: u32) -> Option<&str> {
    map.files.get(file_id as usize).map(|s| s.as_str())
}

/// Search all loaded RefDefMaps for a definition by name (cross-file).
/// Used by LSP goto-def when the ref is in a different file.
pub fn search_all_by_name(name: &str) -> Option<(String, String)> {
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
