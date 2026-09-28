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
        // U342 def-site leg: the cursor sits on the declaration itself. The
        // covering interval already names the def kind, and its span is the
        // def key every file's `def_to_refs` was keyed by — pin it directly.
        // Func and port names never reach the class-level name index, so
        // without this leg the panel returned nothing for a cursor on their
        // declaration. An unpaired *ref* interval (is_ref) is not a def key —
        // it falls through to the name hint. The file string is canonicalized
        // the way the loader keys files (scheme-stripped real path): the
        // workspace key form the graph projection and the def-file ids were
        // built from.
        let def_file = {
            let bare = uri.strip_prefix("file://").unwrap_or(uri);
            let canon = crate::build::pass1::canonicalize_project_uri(&McURI::from(bare));
            if canon.is_empty() {
                bare.to_string()
            } else {
                canon
            }
        };
        match hit {
            Some(h) => Some((h.def_kind, h.file_uri, h.byte_start, h.byte_end)),
            None => sym
                .symbol_lapper
                .find(offset, offset + 1)
                .into_iter()
                .find(|iv| iv.start <= offset && offset < iv.stop)
                .and_then(|iv| {
                    SymbolKind::from_raw(iv.val.kind)
                        .filter(|kind| !kind.is_ref())
                        .map(|kind| (kind, def_file, iv.start as u32, iv.stop as u32))
                })
                .or_else(|| {
                    name_hint.and_then(|n| {
                        map.get_by_name(&mc_uri, n).map(|e| {
                            (
                                e.def_kind,
                                crate::semantic::common::uri_of_file_id(e.def_loc.file_id)
                                    .to_string(),
                                e.def_loc.byte_start,
                                e.def_loc.byte_end,
                            )
                        })
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
    // prefilter cannot drop results. P1: the whitelist gates which ref kinds
    // enter the panel — type-level noise is dropped here. Since U342 the
    // panel-eligible kinds include ports and funcs (the probe showed their
    // cross-file rows already pair; the old suppression hid paired rows),
    // while enum values carry the registration-miss exemption: qualified
    // uses in param/role positions register no ref row, so panel coverage
    // there is best-effort by construction.
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

    /// U342: a func is panel-eligible and its declaration is a valid cursor
    /// position. Probed (U342): layer2 already pairs the cross-file call rows
    /// — pass1 mints the consumer-side FuncDef with the def file's `file_id` —
    /// so once the whitelist lets FuncRef through, "find references" is
    /// exhaustive on both corpus call shapes (named-instance member call and
    /// inline two-pin chain). The def-site pinning leg is what makes the
    /// declaration itself answer: func names never reach the class-level name
    /// index, so the name-hint fallback cannot pin them.
    #[test]
    fn find_at_reaches_cross_file_func_call_sites_from_the_declaration() {
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .expect("lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();
        let dir = std::env::temp_dir().join(format!("mcc-u342-func-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let def_src = "component TINY\n{\n    name = \"T\"\n    pins = [\n        1 = A, \"a\"\n        2 = B, \"b\"\n    ]\n\n    func enable([net1, net2])\n    {\n        net1 - this - net2\n    }\n}\n";
        let use_src = "use ./tiny.mc\n\nmodule main\n{\n    TINY t\n    t.enable([vin, vout])\n    TINY(10k).enable([vdd, gnd])\n}\n";
        std::fs::write(dir.join("tiny.mc"), def_src).unwrap();
        std::fs::write(dir.join("main.mc"), use_src).unwrap();
        let def_uri: McURI =
            format!("file://{}", dir.join("tiny.mc").canonicalize().unwrap().display());
        let main_uri: McURI =
            format!("file://{}", dir.join("main.mc").canonicalize().unwrap().display());
        crate::mcc_load_from_string(&def_uri, def_src);
        crate::build::pass1::mcb_parse_all_modules();
        crate::mcc_load_from_string(&main_uri, use_src);
        crate::build::pass1::mcb_parse_all_modules();
        // Workspace keys (and the refs answers) carry the scheme-stripped
        // real path; this test passes bare paths as the cursor URI.
        let def_key: McURI = dir.join("tiny.mc").canonicalize().unwrap().display().to_string();
        let main_key: McURI = dir.join("main.mc").canonicalize().unwrap().display().to_string();

        // Cursor on the declaration: the def plus both cross-file call sites.
        let def_off = def_src.find("enable").unwrap();
        let items = find_at(&def_key, def_off, None);
        assert!(!items.is_empty(), "func declaration must resolve");
        let defs: Vec<&Value> = items
            .iter()
            .filter(|it| it["def"].as_bool().unwrap_or(false))
            .collect();
        assert_eq!(defs.len(), 1, "items: {items:?}");
        assert_eq!(defs[0]["uri"], def_key.as_str());
        assert_eq!(defs[0]["pos"].as_u64().unwrap() as usize, def_off);
        let ref_pos: Vec<usize> = items
            .iter()
            .filter(|it| it["def"].as_bool().unwrap_or(true) == false)
            .map(|it| it["pos"].as_u64().unwrap() as usize)
            .collect();
        let call1 = use_src.find("t.enable").unwrap() + 2;
        let call2 = use_src.find("(10k).enable").unwrap() + 6;
        assert!(
            ref_pos.contains(&call1) && ref_pos.contains(&call2),
            "both call shapes must be reported, got {ref_pos:?}"
        );

        // Cursor on a call site: the same answer set from the ref side.
        let items = find_at(&main_key, call1, None);
        let defs = items
            .iter()
            .filter(|it| it["def"].as_bool().unwrap_or(false))
            .count();
        assert_eq!(defs, 1, "items: {items:?}");
        assert!(
            items.iter().any(|it| {
                it["def"].as_bool().unwrap_or(false)
                    && it["uri"] == def_key.as_str()
                    && it["pos"].as_u64().unwrap() as usize == def_off
            }),
            "the def site must be among the answers, items: {items:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// U342: a module port consumed through a cross-file member chain
    /// (`psu.vin`) answers from both cursor positions — the declaration in
    /// the def file and the member segment of the chain row in the consumer.
    #[test]
    fn find_at_reaches_cross_file_port_member_chain_from_the_declaration() {
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .expect("lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();
        let dir = std::env::temp_dir().join(format!("mcc-u342-port-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let def_src = "module PSU\n{\n    in vin::DC(5V)\n    vin - VOUT\n}\n";
        let use_src = "use ./psu.mc\n\nmodule main\n{\n    PSU psu\n    psu.vin - VOUT\n}\n";
        std::fs::write(dir.join("psu.mc"), def_src).unwrap();
        std::fs::write(dir.join("main.mc"), use_src).unwrap();
        let def_uri: McURI =
            format!("file://{}", dir.join("psu.mc").canonicalize().unwrap().display());
        let main_uri: McURI =
            format!("file://{}", dir.join("main.mc").canonicalize().unwrap().display());
        crate::mcc_load_from_string(&def_uri, def_src);
        crate::build::pass1::mcb_parse_all_modules();
        crate::mcc_load_from_string(&main_uri, use_src);
        crate::build::pass1::mcb_parse_all_modules();
        // Unlike the func test, the cursor URI keeps the `file://` prefix the
        // LSP proxy sends — the answers must not depend on the input form.
        let def_key = dir.join("psu.mc").canonicalize().unwrap().display().to_string();
        let main_key = dir.join("main.mc").canonicalize().unwrap().display().to_string();

        // Cursor on the declaration: def plus the consumer's member segment.
        let def_off = def_src.find("vin").unwrap();
        let items = find_at(&def_uri, def_off, None);
        assert!(!items.is_empty(), "port declaration must resolve");
        let defs: Vec<&Value> = items
            .iter()
            .filter(|it| it["def"].as_bool().unwrap_or(false))
            .collect();
        assert_eq!(defs.len(), 1, "items: {items:?}");
        assert_eq!(defs[0]["uri"], def_key);
        assert_eq!(defs[0]["pos"].as_u64().unwrap() as usize, def_off);
        let member_off = use_src.find("psu.vin").unwrap() + 4;
        // The chain use is reported on its whole-chain span (`psu.vin`):
        // the panel reports the use site, the member-segment narrowing is
        // the rename-fix face's job (quickfix), not the panel's.
        let chain_off = use_src.find("psu.vin").unwrap();
        assert!(
            items.iter().any(|it| {
                it["def"].as_bool().unwrap_or(true) == false
                    && it["uri"] == main_key
                    && it["pos"].as_u64().unwrap() as usize == chain_off
            }),
            "the consumer's chain use must be reported, items: {items:?}"
        );

        // Cursor on the member segment: the same answer set from the ref side.
        let items = find_at(&main_uri, member_off, None);
        let defs = items
            .iter()
            .filter(|it| it["def"].as_bool().unwrap_or(false))
            .count();
        assert_eq!(defs, 1, "items: {items:?}");
        assert!(
            items.iter().any(|it| {
                it["def"].as_bool().unwrap_or(false)
                    && it["uri"] == def_key
                    && it["pos"].as_u64().unwrap() as usize == def_off
            }),
            "the def site must be among the answers, items: {items:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
