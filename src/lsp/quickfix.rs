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
//! occurrence sites come from the shared layer2 pairing machine — every
//! loaded file's `RefDefMap::def_to_refs` queried by the true def key
//! `(def_kind, file_id, span)`, spans read back from each file's symbol
//! lapper. This is the same machine the refs panel reads (`find_at`),
//! joined here without the panel's display exemptions (`is_panel_ref_kind`
//! is a panel display policy) and without the who-uses refgraph prefilter:
//! the fix face is `def_ref_kinds` below, and a rename must reach every
//! exact occurrence.
//!
//! Exemptions need no extra logic: the fix exists only where the gate fired,
//! so §2.1 (pin names, datasheet names) never reaches a fix.

use crate::db::cmie::tables::WORKSPACE;
use crate::refdef::types::SymbolKind;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

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

/// The span a rename edit touches for one occurrence row. Member-only rows
/// (plain operands) cover exactly the name; whole-chain rows (`psu.vin`,
/// recorded for hover/goto-def over the full chain) are narrowed to the
/// member tail — and only when the covered text really ends in `.{name}`,
/// so a row that names something else is dropped rather than guessed.
fn member_span(text: &str, start: usize, stop: usize, name: &str) -> Option<(usize, usize)> {
    let covered = text.get(start..stop)?;
    if covered == name {
        return Some((start, stop));
    }
    if covered.ends_with(&format!(".{name}")) {
        return Some((stop - name.len(), stop));
    }
    None
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
    // value carries the numeric def-file id alongside the file string — the
    // def key `def_to_refs` was keyed by is (def_kind, file_id, span), and
    // `file_id 0` means the flagged file itself (same sentinel layer2 uses).
    // The symbols guard is dropped before the workspace scan below — the scan
    // re-locks the same file's mutex, and std Mutex is not reentrant.
    let mut defs: HashMap<(SymbolKind, u32), (String, u32, usize, usize)> = HashMap::new();
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
                let file_id = if loc.file_id != 0 {
                    loc.file_id
                } else {
                    crate::semantic::common::uri_intern(uri).0
                };
                defs.insert(
                    (*kind, *decl_id),
                    (file, file_id, loc.byte_start as usize, loc.byte_end as usize),
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
    // face is consumed plainly (net-ref rows on the declaration's id) and
    // through member chains (`psu.vin`): pass1 mints the consumer-side
    // chain-hit def with the def file's id and the true declaration's span,
    // so layer2 pairs every chain row under the true def key — the same
    // pairing the refs panel reads (U342 probe: the cross-file rows already
    // pair, and the hbl corpus switch is lossless). A func face is the same
    // shape: `inst.Enable(...)` call sites register FuncRef rows paired under
    // the true FuncDef key for both corpus call shapes — named-instance
    // member call and inline two-pin chain — and an in-body `this.f(...)`
    // call is not legal syntax (E2082), so there is no third consumer face
    // to dangle (U341 probe). Enum values stay suppressed: a qualified value
    // use (`diel = Grade.good`) registers no ref row at all, and the
    // EnumValDef span covers the whole value list, so a rename could not even
    // place the declaration edit safely.
    if defs.keys().any(|(kind, _)| kind == &SymbolKind::EnumValDef) {
        return None;
    }

    // ② Occurrences: the shared layer2 pairing machine. Every loaded file's
    // RefDefMap answers `def_to_refs[(def_kind, file_id, span)]` with every
    // `(ref_kind, ref_id)` that resolved to that def — the same query the
    // refs panel issues (`find_at`). The fix face is `ref_kinds` (from
    // `def_ref_kinds`), not the panel's display exemptions: a rename must
    // reach exactly the ref faces the gate's def kinds pair with.
    let def_keys: Vec<(SymbolKind, u32, u32, u32)> = defs
        .iter()
        .map(|((kind, _), (_, file_id, start, end))| {
            (*kind, *file_id, *start as u32, *end as u32)
        })
        .collect();
    let mut refs: BTreeSet<(u8, u32)> = BTreeSet::new();
    for entry in WORKSPACE.mcodes.iter() {
        let Ok(sym) = entry.value().symbols.lock() else {
            continue;
        };
        let Some(m) = sym.ref_def_map.as_ref() else {
            continue;
        };
        for (def_kind, file_id, start, end) in &def_keys {
            for &(rk, rid) in m.get_refs_for_def(*def_kind, *file_id, *start, *end) {
                if ref_kinds.contains(&rk) {
                    refs.insert((rk as u8, rid));
                }
            }
        }
    }

    // (file, start, end) → replacement; BTreeMap dedups (a def span may also
    // appear as a ref interval) and imposes the apply order.
    let mut edits: BTreeMap<(String, usize, usize), String> = BTreeMap::new();
    for (file, _, start, end) in defs.values() {
        edits.insert((file.clone(), *start, *end), replacement.to_string());
    }
    // ②a Occurrence spans: (kind, id) → [(uri, start, stop)] from each file's
    // symbol lapper. Lapper ids are workspace-unique DeclareIds, so the key is
    // global and the same symbol maps to its span in every file — the same
    // scan the panel's span leg runs. Chain-consumer rows carry the
    // whole-chain span (`psu.vin`); a rename touches only the flagged member
    // segment.
    let mut span_index: HashMap<(u8, u32), Vec<(String, usize, usize)>> = HashMap::new();
    for entry in WORKSPACE.mcodes.iter() {
        let file_uri = entry.key().to_string();
        let Ok(sym) = entry.value().symbols.lock() else {
            continue;
        };
        for iv in sym.symbol_lapper.iter() {
            span_index
                .entry((iv.val.kind, iv.val.id))
                .or_default()
                .push((file_uri.clone(), iv.start, iv.stop));
        }
    }
    for (rk, rid) in refs {
        let Some(spans) = span_index.get(&(rk, rid)) else {
            continue;
        };
        for (ref_uri, start, stop) in spans {
            let Some(mc) = WORKSPACE.mcodes.get(&crate::McURI::from(ref_uri.as_str())) else {
                continue;
            };
            let Some((start, stop)) = member_span(mc.content.as_str(), *start, *stop, &name)
            else {
                continue;
            };
            edits.insert((ref_uri.clone(), start, stop), replacement.to_string());
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
    fn port_face_fix_renames_member_chain_consumers_member_segment_only() {
        // A module port is consumed plainly (same file) and through a
        // cross-file member chain (`psu.vin`). The chain row pairs under the
        // true def key in `def_to_refs` (pass1 mints the consumer-side
        // chain-hit def with the def file's id), and the whole-chain row's
        // edit narrows to the member segment: renaming `vin` must never
        // clobber the `psu.` base. Real files on disk: `use ./psu.mc`
        // resolution requires the target (same constraint as the loader
        // tests in db/infra/mc_code.rs).
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK.lock().expect("lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();
        let dir = std::env::temp_dir().join(format!("mcc-u341-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let def_src = "module PSU\n{\n    in vin::DC(5V)\n    vin - VOUT\n}\n";
        let use_src = "use ./psu.mc\n\nmodule main\n{\n    PSU psu\n    psu.vin - VOUT\n}\n";
        let def_path = dir.join("psu.mc");
        let main_path = dir.join("main.mc");
        std::fs::write(&def_path, def_src).unwrap();
        std::fs::write(&main_path, use_src).unwrap();
        let def_uri: McURI = format!("file://{}", def_path.canonicalize().unwrap().display());
        let main_uri: McURI = format!("file://{}", main_path.canonicalize().unwrap().display());
        crate::mcc_load_from_string(&def_uri, def_src);
        crate::mcc_load_from_string(&main_uri, use_src);
        crate::mcc_build(&crate::McIds::from("main"), &main_uri).expect("build main failed");
        // The style gate fires per built module's own file, so the PSU module
        // needs its own build pass before psu.mc carries E5070.
        crate::mcc_build(&crate::McIds::from("PSU"), &def_uri).expect("build PSU failed");
        crate::build::pass1::mcb_parse_all_modules();

        // Diagnostic loc.uri stores the bare file path (no scheme), so the
        // lookup keys on the raw path while the ref tables key on file://.
        let diag = diag_for(
            crate::errcodes::NAME_NET_NOT_UPPER_SNAKE,
            def_path.canonicalize().unwrap().display().to_string().as_str(),
            "vin",
        );
        let payload = fix_payload(&diag).expect("port face carries a fix once chain rows pair");
        let edits = payload["edits"].as_array().unwrap();
        // Declaration + plain use in the def file, member segment only in the
        // consumer file (len("vin") == 3).
        assert_eq!(edits.len(), 3, "edits: {edits:?}");
        let by_file: std::collections::BTreeMap<&str, Vec<&Value>> =
            edits.iter().fold(Default::default(), |mut m, e| {
                m.entry(e["file"].as_str().unwrap())
                    .or_default()
                    .push(e);
                m
            });
        // Edit file keys are bare paths (same shape as diagnostic loc.uri).
        let def_key = def_path.canonicalize().unwrap().display().to_string();
        let use_key = main_path.canonicalize().unwrap().display().to_string();
        let def_file = &by_file[def_key.as_str()];
        assert_eq!(def_file.len(), 2);
        for e in def_file {
            assert_eq!(e["len"].as_u64().unwrap(), 3);
            assert_eq!(e["replacement"], "VIN");
        }
        let use_file = &by_file[use_key.as_str()];
        assert_eq!(use_file.len(), 1);
        assert_eq!(use_file[0]["len"].as_u64().unwrap(), 3);
        assert_eq!(use_file[0]["replacement"], "VIN");
        // The edit must sit on the member segment (`vin` of `psu.vin`), never
        // on the whole chain or the base instance.
        let covered = &use_src[use_file[0]["pos"].as_u64().unwrap() as usize
            ..use_file[0]["pos"].as_u64().unwrap() as usize + 3];
        assert_eq!(covered, "vin");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enum_value_stays_fix_free_until_member_chain_refs_index_consumers() {
        // Probed (U341): a qualified value use (`diel = Grade.good` in a
        // component header) registers no ref row at all in the consuming sem,
        // and the EnumValDef def_map span covers the whole value list
        // (`good,\n    bad` — two values share one span), so neither the
        // consumers nor even the declaration edit can be placed soundly.
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
    fn func_fix_renames_cross_file_call_sites_member_segment_only() {
        // A func is consumed through member calls: same-file rows and
        // cross-file rows pair under the true FuncDef key in `def_to_refs`
        // (U342 probe — pass1 mints the consumer-side fcall def with the def
        // file's id). Both corpus shapes — named-instance member call and
        // inline two-pin chain — must carry an edit, each on the member
        // segment only. Real files on disk: `use ./tiny.mc` resolution
        // requires the target (same constraint as the loader tests in
        // db/infra/mc_code.rs).
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK.lock().expect("lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();
        let dir = std::env::temp_dir().join(format!("mcc-u341-func-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let def_src = "component TINY\n{\n    name = \"T\"\n    pins = [\n        1 = A, \"a\"\n        2 = B, \"b\"\n    ]\n\n    func enable([net1, net2])\n    {\n        net1 - this - net2\n    }\n}\n\nmodule local\n{\n    TINY t9\n    t9.enable([n1, n2])\n}\n";
        let use_src = "use ./tiny.mc\n\nmodule main\n{\n    TINY t\n    t.enable([vin, vout])\n    TINY(10k).enable([vdd, gnd])\n}\n";
        let def_path = dir.join("tiny.mc");
        let main_path = dir.join("main.mc");
        std::fs::write(&def_path, def_src).unwrap();
        std::fs::write(&main_path, use_src).unwrap();
        let def_uri: McURI = format!("file://{}", def_path.canonicalize().unwrap().display());
        let main_uri: McURI = format!("file://{}", main_path.canonicalize().unwrap().display());
        crate::mcc_load_from_string(&def_uri, def_src);
        crate::mcc_load_from_string(&main_uri, use_src);
        crate::mcc_build(&crate::McIds::from("main"), &main_uri).expect("build main failed");
        crate::build::pass1::mcb_parse_all_modules();

        // E5072 fires on the declaration file, which stores the bare path.
        let diag = diag_for(
            crate::errcodes::NAME_FUNC_NOT_UPPER_INITIAL,
            def_path.canonicalize().unwrap().display().to_string().as_str(),
            "enable",
        );
        let payload = fix_payload(&diag).expect("func face carries a fix once call rows pair");
        assert_eq!(payload["title"], "Rename 'enable' to 'Enable'");
        let edits = payload["edits"].as_array().unwrap();
        // Declaration + same-file call in the def file, both consumer calls in
        // the consumer file.
        assert_eq!(edits.len(), 4, "edits: {edits:?}");
        let def_key = def_path.canonicalize().unwrap().display().to_string();
        let use_key = main_path.canonicalize().unwrap().display().to_string();
        for e in edits {
            assert_eq!(e["replacement"], "Enable");
            let src = if e["file"].as_str().unwrap() == def_key { def_src } else { use_src };
            let start = e["pos"].as_u64().unwrap() as usize;
            let end = start + e["len"].as_u64().unwrap() as usize;
            assert_eq!(&src[start..end], "enable", "every edit sits on the member segment");
        }
        let per_file =
            |key: &str| edits.iter().filter(|e| e["file"].as_str().unwrap() == key).count();
        assert_eq!(per_file(&def_key), 2, "declaration + same-file call: {edits:?}");
        assert_eq!(per_file(&use_key), 2, "both consumer call sites: {edits:?}");
        std::fs::remove_dir_all(&dir).ok();
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
