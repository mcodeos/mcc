// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Hover information — provide type/definition info for a symbol.
//!
//! Two paths:
//!   (1) Position-aware (preferred): lapper span lookup at the cursor
//!       offset + RefDefMap exact goto-def — same data as F12. Resolves
//!       same-name defs (e.g. `enum CAP` vs `component CAP`) precisely.
//!   (2) Name-based: only for legacy callers that pass no position. Once a
//!       position is given there is **no fallback** — a miss returns `None`
//!       rather than risk mapping to a wrong same-name def.

use serde_json::Value;

/// Get hover information for a symbol at an optional byte offset in a file.
///
/// With a position, resolution is strictly position-aware via the lapper +
/// RefDefMap exact path (§7.4); a miss returns `None` — never a name-based
/// guess, since same-name defs (e.g. `enum CAP` vs `component CAP`) would be
/// misattributed. Without a position (legacy callers) the name-based path runs.
pub fn hover(name: &str, uri: &str, position: Option<usize>) -> Option<Value> {
    match position {
        Some(offset) => resolve_at(uri, offset),
        None => hover_by_name(name, uri),
    }
}

/// Name-based resolution for legacy callers without a cursor position.
///
/// Resolution is restricted to the cursor file's visibility set V(F) (§5.4):
/// P3 (own file) + P4 (use chain) + P5 (mcode). Never scans the whole
/// workspace by name — that would return defs from files F has not `use`d
/// (§5.4.5).
fn hover_by_name(name: &str, uri: &str) -> Option<Value> {
    // First try: resolve as a definition within V(F)
    if let Some(def) = super::gotodef::resolve_in_file(name, uri) {
        return Some(def);
    }

    // Second try: look up in semantic tokens
    let candidates = &[crate::McURI::from(uri)];
    if let Some(sem) = super::sem::try_lookup_sem(candidates) {
        // Check if any symbol matches this name
        if let Some(symbols) = sem.get("symbols") {
            if let Some(info) = symbols.get(name) {
                return Some(info.clone());
            }
        }
    }

    None
}

/// Position-aware resolution: lapper interval at `offset` + RefDefMap exact
/// goto-def (shared with goto-def, see `refdef::query::resolve_at`). Returns
/// the def location (file + byte span) plus the resolved def kind, e.g. an
/// enum head resolves to `EnumDef` and a component head to `ClassDef`.
fn resolve_at(uri: &str, offset: usize) -> Option<Value> {
    use crate::refdef::query::resolve_at as resolve_at_shared;

    let mc_uri = crate::McURI::from(uri);
    let mcfile = crate::db::cmie::tables::WORKSPACE.mcodes.get(&mc_uri)?;
    let sym = mcfile.symbols.lock().ok()?;
    let map = sym.ref_def_map.as_ref()?;
    let hit = resolve_at_shared(map, &sym.symbol_lapper, offset)?;

    Some(serde_json::json!({
        "kind": hit.def_kind.kind_name(),
        "uri": hit.file_uri,
        "byte_start": hit.byte_start,
        "byte_end": hit.byte_end,
    }))
}
