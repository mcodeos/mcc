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
use crate::refdef::types::SymbolKind;
use crate::semantic::common::uri_intern;
use crate::McURI;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// P1 refs whitelist — see [`crate::refdef::types::is_whitelisted_ref_kind`]
/// (canonical home since U234 tier ②; the kinds double as the def-edge
/// coverage contract behind the graph prefilter below).
pub use crate::refdef::types::is_whitelisted_ref_kind;

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
    // U234 tier ②: the scan is prefiltered to the def-refgraph's file
    // projection — every file with a recorded ref-point into the def's file
    // (plus the def file itself). The projection over-approximates by
    // construction (edge coverage = the whitelist, at RefDefMap::insert;
    // purge is ref-point-side so a not-yet-rebuilt referencing file keeps
    // its edges), and the exact def-key match below post-filters, so the
    // prefilter cannot drop results — including the Inst/Label refs the
    // whitelist lets through (D4). P1: the whitelist gates which ref kinds
    // enter the panel — type-level noise is dropped here.
    let mut candidate_files: std::collections::HashSet<String> =
        WORKSPACE.refgraph.dependent_files_of_file(&def_uri).into_iter().collect();
    candidate_files.insert(def_uri.clone());
    let mut refs: BTreeSet<(u8, u32)> = BTreeSet::new();
    for entry in WORKSPACE.mcodes.iter() {
        if !candidate_files.contains(entry.key().as_str()) {
            continue;
        }
        if let Ok(s) = entry.value().symbols.lock() {
            if let Some(m) = s.ref_def_map.as_ref() {
                for &(rk, rid) in m.get_refs_for_def(def_kind, def_file_id, def_start, def_end) {
                    if is_whitelisted_ref_kind(rk) {
                        refs.insert((rk as u8, rid));
                    }
                }
            }
        }
    }

    // ③ Source spans: (kind, id) → [(uri, start, stop)] via each file's
    // lapper. Interval ids are workspace-unique DeclareIds, so the key is
    // global and the same symbol maps to its span in every file. Same
    // candidate-file prefilter as ② — a ref's span can only live in a file
    // whose map contributed the ref.
    let mut index: HashMap<(u8, u32), Vec<(String, usize, usize)>> = HashMap::new();
    for entry in WORKSPACE.mcodes.iter() {
        if !candidate_files.contains(entry.key().as_str()) {
            continue;
        }
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

    // ④ Assemble: the definition itself plus every located ref. Each item
    // carries its `kind` so the frontend can badge the symbol type.
    let mut out: BTreeMap<(String, usize, usize), Value> = BTreeMap::new();
    out.insert(
        (def_uri.clone(), def_start as usize, def_end as usize),
        json!({
            "uri": def_uri,
            "scope": "",
            "pos": def_start,
            "end": def_end,
            "def": true,
            "kind": def_kind as u8,
        }),
    );
    for (rk, rid) in refs {
        if let Some(spans) = index.get(&(rk, rid)) {
            for (u, s, e) in spans {
                out.insert(
                    (u.clone(), *s, *e),
                    json!({ "uri": u, "scope": "", "pos": s, "end": e, "def": false, "kind": rk }),
                );
            }
        }
    }
    out.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1 refs whitelist regression: the panel must only ever receive
    /// netlist-meaningful kinds. A class reference resolves to the ClassDef
    /// and its ClassRef usage sites; the definition site is returned with
    /// `def: true`; every item carries a whitelisted `kind`. Enum defs
    /// resolve to their own site with no reference noise.
    #[test]
    fn find_at_respects_refs_whitelist() {
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .expect("lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let src = r#"
enum PKG { DIP8, SOIC8 }
component RES (ohm::UV.OHM)
{
    pins = [ 1 = 1, 2 = 2 ]
}
component CAP (cap::UV.CAP, volt::UV.VOLT, tolerance::UV.PERCENT)
{
    pins = [ 1 = 1, 2 = 2 ]
}
module main
{
    RES r1(10K)
    CAP c1
    r1.1 -> c1.1
    r1.2 -> V5V
    c1.2 -> GND
}
"#;
        let uri: McURI = "/mcc/refs-whitelist.mc".to_string();
        crate::mcc_load_from_string(&uri, src);
        crate::mcc_build(&crate::McIds::from("main"), &uri).expect("build failed");
        // App flow: load_project parses all modules AFTER string-loading the
        // entry, which is what builds the lapper with instance/label intervals.
        crate::build::pass1::mcb_parse_all_modules();

        // Class reference: the `CAP` in `CAP c1` resolves to the ClassDef;
        // the reverse index contributes the ClassRef at the usage site, and
        // the definition site is returned with def=true.
        let cap_off = src.find("CAP c1").expect("CAP c1");
        let cap_items = find_at(&uri, cap_off, Some("CAP"));
        assert!(!cap_items.is_empty(), "class ref must resolve");
        let cap_kinds: Vec<u8> = cap_items
            .iter()
            .map(|it| it["kind"].as_u64().unwrap_or(0) as u8)
            .collect();
        assert!(
            cap_kinds
                .iter()
                .all(|&k| is_whitelisted_ref_kind(SymbolKind::from_raw(k).expect("raw kind"))),
            "every CAP ref kind must be whitelisted, got {cap_kinds:?}"
        );
        let cap_defs = cap_items
            .iter()
            .filter(|it| it["def"].as_bool().unwrap_or(false))
            .count();
        assert_eq!(cap_defs, 1, "class ref must include the definition site");
        // The usage site `CAP c1` must be among the refs.
        assert!(
            cap_items.iter().any(|it| {
                it["pos"].as_u64().unwrap_or(0) as usize == cap_off
                    && it["def"].as_bool().unwrap_or(false) == false
            }),
            "class usage site must be reported as a reference"
        );

        // Enum definition: resolves to the EnumDef site; still whitelisted
        // and carries def=true — no type-level noise leaks in.
        let pkg_off = src.find("enum PKG").expect("enum PKG");
        let pkg_items = find_at(&uri, pkg_off, Some("PKG"));
        assert!(!pkg_items.is_empty(), "enum def must resolve");
        let pkg_kinds: Vec<u8> = pkg_items
            .iter()
            .map(|it| it["kind"].as_u64().unwrap_or(0) as u8)
            .collect();
        assert!(
            pkg_kinds
                .iter()
                .all(|&k| is_whitelisted_ref_kind(SymbolKind::from_raw(k).expect("raw kind"))),
            "every PKG kind must be whitelisted, got {pkg_kinds:?}"
        );
        assert_eq!(
            pkg_items
                .iter()
                .filter(|it| it["def"].as_bool().unwrap_or(false))
                .count(),
            1
        );
    }
}
