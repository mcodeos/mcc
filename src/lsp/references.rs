// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Find references — locate all usages of a symbol.
//!
//! The workspace `refs` RPC (and `mcc refs`) answers "where is this name used".
//! The authoritative data lives in the RefDefMap reverse index
//! (`def_to_refs`): `find_at` pins the definition under the cursor through the
//! strict position-aware path (shared with goto-def/hover), then every loaded
//! file's map contributes the `(ref_kind, ref_id)` pairs that resolved to that
//! definition. Each ref's source span comes from the file's symbol lapper —
//! lapper ids are workspace-unique DeclareIds, so `(kind, id)` identifies the
//! same symbol in every file, making the cross-file span scan exact.

use crate::db::cmie::tables::WORKSPACE;
use crate::semantic::common::uri_intern;
use crate::McURI;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Legacy name-based find-references (kept for `mcc refs <name>` and the
/// name-only RPC path). Scans the workspace local symbol tables for instance
/// references registered at parse time.
pub fn find(name: &str) -> Vec<Value> {
    let refs = crate::mcb_get_refs(name);
    refs.iter()
        .map(|(uri, scope, span)| {
            json!({
                "uri": uri,
                "scope": scope,
                "pos": span.start,
                "end": span.end,
            })
        })
        .collect()
}

/// Position-aware find-references. Resolves the definition under `offset` in
/// `uri` (strict RefDefMap path), then collects every reference to that
/// definition across all loaded files, plus the definition site itself.
///
/// `name_hint` is used only as a fallback when the position resolves to
/// nothing (e.g. the cursor sits on a declaration site the map keys by ref
/// kind, or on an identifier the lapper does not cover). The returned items
/// carry `"def": true` for the declaration, are sorted by `(uri, pos)` and
/// deduplicated — the same ref may appear in several files' maps.
pub fn find_at(uri: &str, offset: usize, name_hint: Option<&str>) -> Vec<Value> {
    use crate::refdef::query::resolve_at;

    let mc_uri = McURI::from(uri);
    // Bind the definition-space guard so the borrowed SourceFile outlives the
    // statement (calling source_file() on the temporary would free it early).
    let ds = crate::definition_space();
    let mcfile = match ds.source_file_tolerant(&mc_uri) {
        Some(f) => f,
        None => return Vec::new(),
    };
    // ① Pin the definition: strict position-aware resolution first, then the
    // visibility-aware name lookup for the current file. The symbols guard
    // must be dropped before the workspace scan below — the scan re-locks the
    // current file's symbols through WORKSPACE.mcodes, and std Mutex is not
    // reentrant, so holding the guard across it self-deadlocks.
    let def = {
        let sym = match mcfile.symbols.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let map = match sym.ref_def_map.as_ref() {
            Some(m) => m,
            None => return Vec::new(),
        };
        let hit = resolve_at(map, &sym.symbol_lapper, offset);
        match hit {
            Some(h) => Some((h.def_kind, h.file_uri, h.byte_start, h.byte_end)),
            None => name_hint.and_then(|n| {
                map.get_by_name(&mc_uri, n).map(|e| {
                    (
                        e.def_kind,
                        crate::semantic::common::uri_of_file_id(e.def_loc.file_id).to_string(),
                        e.def_loc.byte_start,
                        e.def_loc.byte_end,
                    )
                })
            }),
        }
    };
    let Some((def_kind, def_uri, def_start, def_end)) = def else {
        return Vec::new();
    };
    // The def key is (def_kind, global file id, span) — `intern_file` (and
    // `uri_intern`) share one append-only global table, so the key computed
    // here matches the keys every file's map was built with.
    let def_file_id = uri_intern(&def_uri).0;

    // ② Reverse index: every (ref_kind, ref_id) that resolved to the def.
    let mut refs: BTreeSet<(u8, u32)> = BTreeSet::new();
    for entry in WORKSPACE.mcodes.iter() {
        if let Ok(s) = entry.value().symbols.lock() {
            if let Some(m) = s.ref_def_map.as_ref() {
                for &(rk, rid) in m.get_refs_for_def(def_kind, def_file_id, def_start, def_end) {
                    refs.insert((rk as u8, rid));
                }
            }
        }
    }

    // ③ Source spans: (kind, id) → [(uri, start, stop)] via each file's
    // lapper. Interval ids are workspace-unique DeclareIds, so the key is
    // global and the same symbol maps to its span in every file.
    let mut index: HashMap<(u8, u32), Vec<(String, usize, usize)>> = HashMap::new();
    for entry in WORKSPACE.mcodes.iter() {
        let file_uri = entry.key().to_string();
        if let Ok(s) = entry.value().symbols.lock() {
            for iv in s.symbol_lapper.iter() {
                index.entry((iv.val.kind, iv.val.id)).or_default().push((
                    file_uri.clone(),
                    iv.start,
                    iv.stop,
                ));
            }
        }
    }

    // ④ Assemble: the definition itself plus every located ref.
    let mut out: BTreeMap<(String, usize, usize), Value> = BTreeMap::new();
    out.insert(
        (def_uri.clone(), def_start as usize, def_end as usize),
        json!({
            "uri": def_uri,
            "scope": "",
            "pos": def_start,
            "end": def_end,
            "def": true,
        }),
    );
    for (rk, rid) in refs {
        if let Some(spans) = index.get(&(rk, rid)) {
            for (u, s, e) in spans {
                out.insert(
                    (u.clone(), *s, *e),
                    json!({ "uri": u, "scope": "", "pos": s, "end": e, "def": false }),
                );
            }
        }
    }
    out.into_values().collect()
}
