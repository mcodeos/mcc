// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! QuickFix edit derivation for the style-guide gates (U327).
//!
//! The stored diagnostic carries no fix payload: at serialization time this
//! module derives the rename edits for a fired style gate (codes 5070/5071/5072)
//! from the diagnostic's own `(code, file, message)` plus the RefDef tables.
//! The flagged name is recovered from the diagnostic message's first
//! single-quoted segment — the three gate messages quote exactly the flagged
//! name (style.rs), and the format is pinned by unit test below.
//!
//! A fix renames **every exact occurrence** of the flagged name, never just
//! the declaration: mcode compares names exactly with zero case folding
//! (01-lexical §2), so a declaration-only edit would split nets, orphan call
//! sites and dangle role/enum references. Def sites come from the flagged
//! file's `def_names`/`def_map` (real names captured at `register_def`);
//! occurrence sites come from every loaded file's `ref_entries`, whose
//! `(ref_kind, declare_id)` rows carry the span inline. This deliberately
//! bypasses the refs-panel whitelist (`is_whitelisted_ref_kind`) and the
//! who-uses refgraph prefilter: ports, funcs and enum values are type-level
//! noise for the panel (so their edges are never recorded), but they are
//! exactly the faces a rename must reach.
//!
//! Exemptions need no extra logic: the fix exists only where the gate fired,
//! so §2.1 (pin names, datasheet names) never reaches a fix.

use crate::db::cmie::tables::WORKSPACE;
use crate::refdef::types::SymbolKind;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Quick-fix payload for one diagnostic, or `None` when the diagnostic is not
/// a style gate with a derivable rename (role values included: `RoleDef`
/// registers an empty name, so the declaration cannot be pinned — the role
/// half of 5071 stays fix-free until the def carries its name).
pub fn fix_payload(d: &crate::db::diagnostic::diagnostic::Diagnostic) -> Option<Value> {
    let name = quoted_name(&d.msg)?;
    let replacement = corrected_spelling(d.code, &name)?;
    let edits = collect_edits(d.code, d.loc.uri.as_str(), &name, &replacement)?;
    if edits.is_empty() {
        return None;
    }
    Some(json!({
        "title": format!("Rename '{name}' to '{replacement}'"),
        "edits": edits,
    }))
}

/// The corrected spelling a gate's fix applies. Mirrors the gate predicates in
/// style.rs: the two UPPER_SNAKE gates judge any ASCII lowercase anywhere, the
/// func gate only the first letter.
fn corrected_spelling(code: u32, name: &str) -> Option<String> {
    use crate::errcodes::{
        NAME_FUNC_NOT_UPPER_INITIAL, NAME_NET_NOT_UPPER_SNAKE, NAME_ROLE_ENUM_NOT_UPPER_SNAKE,
    };
    match code {
        NAME_NET_NOT_UPPER_SNAKE | NAME_ROLE_ENUM_NOT_UPPER_SNAKE => {
            if name.chars().any(|c| c.is_ascii_lowercase()) {
                Some(name.to_ascii_uppercase())
            } else {
                None
            }
        }
        NAME_FUNC_NOT_UPPER_INITIAL => {
            let mut chars = name.chars();
            let first = chars.next()?;
            if first.is_ascii_lowercase() {
                Some(first.to_ascii_uppercase().to_string() + chars.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// First single-quoted segment of the message — the flagged name the gate
/// messages always carry.
fn quoted_name(msg: &str) -> Option<String> {
    let start = msg.find('\'')? + 1;
    let rest = &msg[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

/// Def kinds a gate's fix renames, and the ref kinds that locate occurrences.
/// A body net label registers as an implicit NetDef/NetRef (the net_items
/// builder), a port row as PortDef/PortRef. 5071 covers only the enum-value
/// half: role values register an empty def name (mc_code.rs RoleDef arm), so
/// they cannot be pinned by name.
fn def_ref_kinds(code: u32) -> Option<(&'static [SymbolKind], &'static [SymbolKind])> {
    use crate::refdef::types::SymbolKind::*;
    match code {
        crate::errcodes::NAME_NET_NOT_UPPER_SNAKE => {
            Some((&[LabelDef, NetDef, PortDef], &[LabelRef, NetRef, PortRef]))
        }
        crate::errcodes::NAME_ROLE_ENUM_NOT_UPPER_SNAKE => {
            Some((&[EnumValDef], &[EnumValRef]))
        }
        crate::errcodes::NAME_FUNC_NOT_UPPER_INITIAL => Some((&[FuncDef], &[FuncRef])),
        _ => None,
    }
}

/// Every exact occurrence of the flagged name as wire-ready edits:
/// `(file, pos, len, line, column, end_line, end_column, replacement)`,
/// sorted by (file, pos) — the order a client applies them in.
fn collect_edits(code: u32, uri: &str, name: &str, replacement: &str) -> Option<Vec<Value>> {
    let (def_kinds, ref_kinds) = def_ref_kinds(code)?;

    // ① Pin the declaration(s): the flagged file's def_names → def_map. The
    // symbols guard is dropped before the workspace scan below — the scan
    // re-locks the same file's mutex, and std Mutex is not reentrant.
    let mut defs: HashMap<(SymbolKind, u32), (String, usize, usize)> = HashMap::new();
    {
        // Bind the definition-space guard so the borrowed SourceFile outlives
        // the statement (same trap references.rs ① documents).
        let ds = crate::definition_space();
        let mcfile = ds.source_file_tolerant(&crate::McURI::from(uri))?;
        let sym = mcfile.symbols.lock().ok()?;
        for ((kind, decl_id), def_name) in sym.def_names.iter() {
            if !def_kinds.contains(kind) || def_name != name {
                continue;
            }
            if let Some(loc) = sym.def_map.get(&(*kind, *decl_id)) {
                let file = if loc.file_id != 0 {
                    crate::semantic::common::uri_of_file_id(loc.file_id).to_string()
                } else {
                    uri.to_string()
                };
                defs.insert(
                    (*kind, *decl_id),
                    (file, loc.byte_start as usize, loc.byte_end as usize),
                );
            }
        }
    }
    if defs.is_empty() {
        return None;
    }

    // Soundness gate: a fix may only fire when the indexed occurrence set is
    // provably complete. Net labels and free nets are module-local by
    // construction, so their NetRef/LabelRef rows are the whole story. A port
    // face, an enum value or a func is consumed through member chains
    // (`psu.vin`, `Pkg.DIP8`, `inst.Enable`) and plain-phrase member operands
    // never reach `iter_net_refs` — the consumer refs are not indexed today,
    // so a rename would silently dangle them. Those faces stay fix-free until
    // member-chain ref registration lands (U327 ledger).
    if defs.keys().any(|(kind, _)| {
        matches!(
            kind,
            SymbolKind::PortDef | SymbolKind::EnumValDef | SymbolKind::FuncDef
        )
    }) {
        return None;
    }

    // ② Occurrences: every loaded file's ref_entries rows whose DeclareId
    // resolves to a pinned def. DeclareIds are workspace-unique, so the id
    // alone is the join key — the def side and the ref side carry different
    // SymbolKinds for the same symbol (`NetDef` vs `NetRef`).
    let ids: HashSet<u32> = defs.keys().map(|(_, id)| *id).collect();
    // (file, start, end) → replacement; BTreeMap dedups (a def span may also
    // appear as a ref interval) and imposes the apply order.
    let mut edits: BTreeMap<(String, usize, usize), String> = BTreeMap::new();
    for (file, start, end) in defs.values() {
        edits.insert((file.clone(), *start, *end), replacement.to_string());
    }
    for entry in WORKSPACE.mcodes.iter() {
        let file_uri = entry.key().to_string();
        let Ok(sym) = entry.value().symbols.lock() else {
            continue;
        };
        for (kind, decl_id, start, stop) in sym.ref_entries.iter() {
            if ref_kinds.contains(kind) && ids.contains(decl_id) {
                edits.insert(
                    (file_uri.clone(), *start, *stop),
                    replacement.to_string(),
                );
            }
        }
    }
    if edits.is_empty() {
        return None;
    }

    // ③ 1-based line/column per edit endpoint, from each file's own line
    // index — the same conversion and fallback the stored Location uses.
    let mut out: Vec<Value> = Vec::with_capacity(edits.len());
    for ((file, start, end), text) in edits {
        let index = WORKSPACE
            .mcodes
            .get(&crate::McURI::from(file.as_str()))
            .and_then(|mc| mc.line_index.clone());
        let Some(index) = index else {
            continue;
        };
        let (line, column) = crate::db::infra::mc_code::line_col_or_first(&index, start as u32);
        let (end_line, end_column) = crate::db::infra::mc_code::line_col_or_first(&index, end as u32);
        out.push(json!({
            "file": file,
            "pos": start,
            "len": end - start,
            "line": line,
            "column": column,
            "end_line": end_line,
            "end_column": end_column,
            "replacement": text,
        }));
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::McURI;

    #[test]
    fn gate_messages_quote_the_flagged_name() {
        // The three canon messages from style.rs — quoted_name() must recover
        // the flagged name from each, or every fix silently disappears.
        let net = "Net/port name 'vin' is not UPPER_SNAKE; the style guide spells net labels and port faces UPPER_SNAKE (mcode-style §2 #3).";
        let role = "Role value 'osc' is not UPPER_SNAKE; the style guide spells role and enum values UPPER_SNAKE (mcode-style §2 #6).";
        let func = "Function name 'pullup' does not start with an uppercase letter; functions are class-level behavior and take the class's uppercase-initial form (mcode-style §2 #9).";
        assert_eq!(quoted_name(net).as_deref(), Some("vin"));
        assert_eq!(quoted_name(role).as_deref(), Some("osc"));
        assert_eq!(quoted_name(func).as_deref(), Some("pullup"));
    }

    #[test]
    fn corrected_spelling_matches_gate_predicates() {
        assert_eq!(
            corrected_spelling(crate::errcodes::NAME_NET_NOT_UPPER_SNAKE, "Vcc24"),
            Some("VCC24".to_string())
        );
        assert_eq!(
            corrected_spelling(crate::errcodes::NAME_NET_NOT_UPPER_SNAKE, "VCC"),
            None
        );
        assert_eq!(
            corrected_spelling(crate::errcodes::NAME_ROLE_ENUM_NOT_UPPER_SNAKE, "dip8"),
            Some("DIP8".to_string())
        );
        assert_eq!(
            corrected_spelling(crate::errcodes::NAME_FUNC_NOT_UPPER_INITIAL, "pullup"),
            Some("Pullup".to_string())
        );
        // Only the first letter is judged: `loadFlash` is conformant as written.
        assert_eq!(
            corrected_spelling(crate::errcodes::NAME_FUNC_NOT_UPPER_INITIAL, "LoadFlash"),
            None
        );
        assert_eq!(corrected_spelling(crate::errcodes::OPEN_LEAD, "x"), None);
    }

    /// In-process workspace builder — mirrors the references.rs test flow:
    /// string-load every file, build the entry, then parse all modules (that
    /// is what fills ref_entries with the occurrence spans).
    fn build_workspace(
        files: &[(&str, &str)],
        entry: &str,
    ) -> std::sync::MutexGuard<'static, ()> {
        // The workspace is process-global: the guard must stay held across the
        // assertions too, or a parallel test's clear_workspace pulls the tables
        // (and the line index) out from under this test.
        let guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK.lock().expect("lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();
        for (uri, src) in files {
            let mc_uri: McURI = uri.to_string();
            crate::mcc_load_from_string(&mc_uri, src);
            if *uri == entry {
                crate::mcc_build(&crate::McIds::from("main"), &mc_uri).expect("build failed");
            }
        }
        crate::build::pass1::mcb_parse_all_modules();
        guard
    }

    fn first_diag(code: u32, uri: &str) -> crate::db::diagnostic::diagnostic::Diagnostic {
        crate::mcc_diagnose(&McURI::from(uri))
            .into_iter()
            .find(|d| d.code == code)
            .unwrap_or_else(|| panic!("expected E{code} for {uri}"))
    }

    fn diag_for(code: u32, uri: &str, name: &str) -> crate::db::diagnostic::diagnostic::Diagnostic {
        crate::mcc_diagnose(&McURI::from(uri))
            .into_iter()
            .find(|d| {
                d.code == code
                    && d.loc.uri.as_str() == uri
                    && d.msg.contains(&format!("'{name}'"))
            })
            .unwrap_or_else(|| panic!("expected E{code} for '{name}' in {uri}"))
    }

    fn spans(payload: &Value) -> Vec<(String, u64)> {
        payload["edits"]
            .as_array()
            .expect("edits array")
            .iter()
            .map(|e| (e["file"].as_str().unwrap_or("").to_string(), e["pos"].as_u64().unwrap_or(0)))
            .collect()
    }

    #[test]
    fn net_label_fix_renames_every_occurrence_in_the_module() {
        let src = "module main\n{\n    in vin::DC(5V)\n    gnd -> vout\n    vout -> gnd\n}\n";
        let _guard = build_workspace(&[("/mcc/u327_net.mc", src)], "/mcc/u327_net.mc");
        let diag = diag_for(
            crate::errcodes::NAME_NET_NOT_UPPER_SNAKE,
            "/mcc/u327_net.mc",
            "vout",
        );
        let payload = fix_payload(&diag).expect("net label fix");
        assert_eq!(payload["title"], "Rename 'vout' to 'VOUT'");

        // Every source occurrence of the flagged label must carry an edit —
        // a declaration-only fix would split the net (exact name compare).
        let wanted: Vec<u64> = src
            .match_indices("vout")
            .map(|(i, _)| {
                // Skip occurrences inside the corrected form quoted in the
                // title — the source has none, so all matches are the label.
                i as u64
            })
            .collect();
        let got: Vec<u64> = spans(&payload)
            .into_iter()
            .filter(|(f, _)| f == "/mcc/u327_net.mc")
            .map(|(_, p)| p)
            .collect();
        for pos in wanted {
            assert!(
                got.contains(&pos),
                "occurrence at {pos} missing from fix edits {got:?}"
            );
        }
        for e in payload["edits"].as_array().unwrap() {
            assert_eq!(e["replacement"], "VOUT");
        }
    }

    #[test]
    fn port_face_stays_fix_free_until_member_chain_refs_index_consumers() {
        // A module port's cross-file consumer names it through a member chain
        // (`p.vin`), and plain-phrase member operands never reach
        // `iter_net_refs` — measured: the consumer file registers no ref row.
        // Renaming only the declaration would dangle that consumer (exact
        // name compare), so the port face carries no fix.
        let def_src = "module PSU\n{\n    in vin::DC(5V)\n    vin - VOUT\n}\n\nmodule main\n{\n}\n";
        let _guard = build_workspace(&[("/mcc/u327_psu.mc", def_src)], "/mcc/u327_psu.mc");
        let diag = first_diag(crate::errcodes::NAME_NET_NOT_UPPER_SNAKE, "/mcc/u327_psu.mc");
        assert!(
            fix_payload(&diag).is_none(),
            "port faces must not carry a fix while member-chain consumers are unindexed"
        );
    }

    #[test]
    fn enum_value_stays_fix_free_until_member_chain_refs_index_consumers() {
        // Same soundness boundary as the port face: an enum value consumed
        // through a qualified chain is unindexed today, so the declaration
        // cannot be renamed in isolation.
        let src = "enum dielectric\n{\n    x7r,\n    fast\n}\n\nmodule main\n{\n}\n";
        let _guard = build_workspace(&[("/mcc/u327_enum.mc", src)], "/mcc/u327_enum.mc");
        let diag = first_diag(
            crate::errcodes::NAME_ROLE_ENUM_NOT_UPPER_SNAKE,
            "/mcc/u327_enum.mc",
        );
        assert!(
            fix_payload(&diag).is_none(),
            "enum values must not carry a fix while member-chain consumers are unindexed"
        );
    }

    #[test]
    fn func_stays_fix_free_until_member_chain_refs_index_call_sites() {
        // Same soundness boundary: a func's call sites are member chains
        // (`inst.Enable(...)`), unindexed in the pass1 ref tables today.
        let src = "component TINY\n{\n    name = \"T\"\n    pins = [1 = A]\n\n    func enable([net1, net2])\n    {\n        net1 - this - net2\n    }\n}\n\nmodule main\n{\n}\n";
        let _guard = build_workspace(&[("/mcc/u327_func.mc", src)], "/mcc/u327_func.mc");
        let diag = first_diag(crate::errcodes::NAME_FUNC_NOT_UPPER_INITIAL, "/mcc/u327_func.mc");
        assert!(
            fix_payload(&diag).is_none(),
            "funcs must not carry a fix while member-chain call sites are unindexed"
        );
    }

    #[test]
    fn role_value_half_stays_fix_free() {
        // RoleDef registers an empty def name (mc_code.rs RoleDef arm), so the
        // declaration cannot be pinned by name — no fix rather than a partial
        // rename that would break the exact-compare face.
        let src = "interface XTAL(role)\n{\n    pins = [1 = XIN]\n\n    role Osc\n    {\n        peer = Res\n    }\n\n    role Res\n    {\n        peer = Osc\n    }\n}\n\nmodule main\n{\n}\n";
        let _guard = build_workspace(&[("/mcc/u327_role.mc", src)], "/mcc/u327_role.mc");
        let diag = first_diag(
            crate::errcodes::NAME_ROLE_ENUM_NOT_UPPER_SNAKE,
            "/mcc/u327_role.mc",
        );
        assert!(
            fix_payload(&diag).is_none(),
            "role values must not carry a fix until RoleDef carries its name"
        );
    }

    #[test]
    fn non_style_diagnostics_carry_no_fix() {
        let src = "module main\n{\n    in vin::DC(5V)\n    gnd -> vout\n}\n";
        let _guard = build_workspace(&[("/mcc/u327_none.mc", src)], "/mcc/u327_none.mc");
        for d in crate::mcc_diagnose(&McURI::from("/mcc/u327_none.mc")) {
            if !matches!(
                d.code,
                crate::errcodes::NAME_NET_NOT_UPPER_SNAKE
                    | crate::errcodes::NAME_ROLE_ENUM_NOT_UPPER_SNAKE
                    | crate::errcodes::NAME_FUNC_NOT_UPPER_INITIAL
            ) {
                assert!(
                    fix_payload(&d).is_none(),
                    "E{} must not carry a fix",
                    d.code
                );
            }
        }
    }
}
