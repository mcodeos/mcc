// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Ref→Def matching engine — fill_refdef_layer2.
//!
//! Extracted from `db/infra/mc_code.rs` (see design doc §16).

use crate::refdef::types::{CmieKind, RefDefEntry, RefDefMap, SourceLocation, SymbolKind};
use crate::McURI;
use std::collections::HashMap;

/// Resolve a def name for `(def_kind, decl_id)`.
///
/// Only the caller's local `def_names` table (keyed in the caller's own id
/// space) is consulted. The def file's own `def_names` is keyed in the *def
/// file's* id space, so probing it with a caller-side decl_id can alias a
/// different same-id class (e.g. `CAP` vs `CAP.SAFETY` both holding id 9 in
/// different files) and return the wrong name. Cross-file defs are resolved
/// by the caller via gt reverse-lookup (see `McCode::def_name_for`), never by
/// id-guessing into another file's table. The name always comes from mcc's AST
/// at registration — never a text slice of the def line.
pub fn resolve_def_name(
    local_names: &HashMap<(SymbolKind, u32), String>,
    _def_uri: &str,
    def_kind: SymbolKind,
    decl_id: u32,
) -> String {
    local_names
        .get(&(def_kind, decl_id))
        .cloned()
        .unwrap_or_default()
}

/// Build RefDefMap Layer 2 inline from the freshly-built lapper.
/// Matches InstanceRef/LabelRef/FunctionRef/etc. to their defs via shared DeclareId.
/// Called at end of create_lapper() — no separate lapper re-scan.
pub fn fill_refdef_layer2(
    map: &mut RefDefMap,
    scope_map: &HashMap<(usize, usize), String>,
    def_map_src: &HashMap<(SymbolKind, u32), SourceLocation>,
    def_names: &HashMap<(SymbolKind, u32), String>,
    ref_entries: &[(SymbolKind, u32, usize, usize)],
    file_uri: &McURI,
    file_table: &[String],
) {
    // ★ Preserve original SourceLocation (including file_id for cross-file defs).
    // Old code mapped to (usize, usize) which dropped file_id and always used
    // file_uri — this broke cross-file FuncDef registered via register_def.
    let def_map: HashMap<(SymbolKind, u32), &SourceLocation> =
        def_map_src.iter().map(|(k, loc)| (*k, loc)).collect();

    // ★ A3: Match refs from pre-collected ref_entries instead of scanning lapper
    for &(ref_kind, decl_id, ref_start, ref_stop) in ref_entries {
        let candidate_defs: &[SymbolKind] = match ref_kind {
            SymbolKind::InstRef => &[SymbolKind::InstDef],
            // bare-identifier params register as UnknownDef via
            // param_def_kind; without UnknownDef here the PortRef entries for
            // untyped params were dropped by Layer 2 (upgrade_unknown_defs
            // runs after Layer 2 and could not resurrect them).
            // module square members (`[VDD_3V3,GND]::DC(3.3V)`)
            // and module-level labels (e.g. `GND`) register as LabelDef, but
            // their use sites resolve to PortRef via resolve_net_ref_kind —
            // LabelDef must be a candidate or those refs are dropped. Listed
            // last so a same-id PortDef/ParamDef wins when both exist.
            SymbolKind::PortRef => &[
                SymbolKind::PortDef,
                SymbolKind::ParamDef,
                SymbolKind::UnknownDef,
                SymbolKind::LabelDef,
            ],
            SymbolKind::LabelRef => &[SymbolKind::LabelDef],
            SymbolKind::FuncRef => &[SymbolKind::FuncDef],
            SymbolKind::BusRef => &[SymbolKind::BusDef], // ★ §16: exact match
            // ★ §3.4.3 (rev): bus member ref (`MIC.P` member segment) resolves
            // to the member def (precise span), not the whole bus.
            SymbolKind::BusMemberRef => &[SymbolKind::BusMemberDef],
            // FuncParamRef is the catch-all for funcall arguments whose
            // actual type (PinNameRef, LabelRef, InstRef, etc.) isn't
            // determined at lapper time.  Map to whatever def matches
            // the shared DeclareId in def_map.
            SymbolKind::FuncParamRef => &[
                SymbolKind::ParamDef,
                SymbolKind::PinNameDef,
                SymbolKind::PinIdDef,
                SymbolKind::PinIfaceDef,
                SymbolKind::PortDef,
                SymbolKind::LabelDef,
                SymbolKind::InstDef,
                SymbolKind::FuncDef,
                SymbolKind::ClassDef,
                SymbolKind::EnumDef,
                SymbolKind::EnumValDef,
                SymbolKind::RoleDef,
                SymbolKind::DefineDef,
                SymbolKind::AttrDef,
                SymbolKind::BusDef, // ★ R7: bus refs may resolve via FuncParamRef
                SymbolKind::BusMemberDef, // ★ §3.4.3 (rev): member refs too
                SymbolKind::UnknownDef, // ★ R7: untyped params
            ],
            SymbolKind::PinNameRef => &[SymbolKind::PinNameDef],
            SymbolKind::PinIdRef => &[SymbolKind::PinIdDef],
            SymbolKind::PinIfaceRef => &[SymbolKind::PinIfaceDef],
            SymbolKind::EnumRef => &[SymbolKind::EnumDef],
            SymbolKind::EnumValRef => &[SymbolKind::EnumValDef],
            SymbolKind::ClassRef => &[SymbolKind::ClassDef, SymbolKind::ClassRef],
            _ => &[],
        };
        // Try each candidate def kind
        let mut def_match: Option<(&SourceLocation, SymbolKind)> = None;
        for &dk in candidate_defs {
            if let Some(loc) = def_map.get(&(dk, decl_id)) {
                def_match = Some((loc, dk));
                break;
            }
        }
        if let Some((loc, def_kind)) = def_match {
            let def_start = loc.byte_start as usize;
            let def_stop = loc.byte_end as usize;
            if def_start == ref_start && def_stop == ref_stop {
                continue; // self-ref skip
            }
            // Use original file_id from SourceLocation for cross-file defs.
            // Falls back to current file_uri if file_id is 0 (same-file).
            let def_uri_str = if loc.file_id != 0 {
                let idx = loc.file_id as usize;
                if idx < file_table.len() {
                    file_table[idx].clone()
                } else {
                    file_uri.clone()
                }
            } else {
                file_uri.clone()
            };
            let fid = map.intern_file(&McURI::from(def_uri_str.as_str()));
            // Look up the scope by the DEF's position, not the ref's. The
            // scope_map is keyed by def spans (built from name_to_declare_id
            // source locations), so querying with (ref_start, ref_stop) always
            // missed and left container_id at 0 for every Layer 2 entry.
            let scope = scope_map
                .get(&(def_start, def_stop))
                .cloned()
                .unwrap_or_default();
            let cid = map.intern_container(&scope);
            // Name from the AST-driven def_names table (local id space only).
            // Every Layer 2 def id is minted locally by register_def, so a
            // local lookup always hits; never probe the def file's own table
            // with this id (different id space — see resolve_def_name).
            let def_name = resolve_def_name(def_names, &def_uri_str, def_kind, decl_id);
            map.insert(
                ref_kind,
                decl_id,
                RefDefEntry {
                    ref_kind,
                    ref_id: decl_id,
                    def_loc: SourceLocation {
                        file_id: fid as u32,
                        container_id: cid,
                        func_id: 0,
                        byte_start: def_start as u32,
                        byte_end: def_stop as u32,
                    },
                    def_kind,
                    cmie_kind: CmieKind::UNKNOWN,
                    def_name,
                },
            );
        }
    }

    // ── PortRef generation (§3.2.2 Rule 1) ──
    // ★ A3: Use pre-collected ref_entries instead of lapper scan
    let fid = map.intern_file(file_uri);
    let cid = map.intern_container("");
    for &(ref_kind, decl_id, ref_start, ref_stop) in ref_entries {
        if ref_kind != SymbolKind::InstRef {
            continue;
        }
        if let Some(loc) = def_map.get(&(SymbolKind::PortDef, decl_id)) {
            let def_start = loc.byte_start as usize;
            let def_stop = loc.byte_end as usize;
            if def_start == ref_start && def_stop == ref_stop {
                continue;
            }
            if map.entries.contains_key(&(SymbolKind::PortRef, decl_id)) {
                continue;
            }
            map.entries.remove(&(SymbolKind::InstRef, decl_id));
            map.insert(
                SymbolKind::PortRef,
                decl_id,
                RefDefEntry {
                    ref_kind: SymbolKind::ClassDef,
                    ref_id: 0,
                    def_loc: SourceLocation {
                        file_id: fid,
                        container_id: cid,
                        func_id: 0,
                        byte_start: def_start as u32,
                        byte_end: def_stop as u32,
                    },
                    def_kind: SymbolKind::PortDef,
                    cmie_kind: CmieKind::UNKNOWN,
                    def_name: resolve_def_name(def_names, file_uri, SymbolKind::PortDef, decl_id),
                },
            );
        }
    }

    // ── LabelRef generation ──
    // ★ A3: Use def_map + ref_entries instead of lapper scans
    let mut port_to_label: HashMap<u32, (u32, (usize, usize))> = HashMap::new();
    {
        // Build pos→label mapping from LabelDef entries in def_map
        let mut pos_to_label: HashMap<(usize, usize), u32> = HashMap::new();
        for ((kind, lid), loc) in def_map.iter() {
            if *kind == SymbolKind::LabelDef {
                let ds = loc.byte_start as usize;
                let de = loc.byte_end as usize;
                pos_to_label.insert((ds, de), *lid);
            }
        }
        // Cross-reference PortDef at same position → LabelDef
        for ((kind, pid), loc) in def_map.iter() {
            if *kind == SymbolKind::PortDef {
                let ds = loc.byte_start as usize;
                let de = loc.byte_end as usize;
                if let Some(&lid) = pos_to_label.get(&(ds, de)) {
                    port_to_label.insert(*pid, (lid, (ds, de)));
                }
            }
        }
    }
    if !port_to_label.is_empty() {
        let fid = map.intern_file(file_uri);
        let cid = map.intern_container("");
        for &(ref_kind, decl_id, _ref_start, _ref_stop) in ref_entries {
            if ref_kind != SymbolKind::PortRef {
                continue;
            }
            if let Some(&(lid, (def_start, def_stop))) = port_to_label.get(&decl_id) {
                if map.entries.contains_key(&(SymbolKind::LabelRef, lid)) {
                    continue;
                }
                map.insert(
                    SymbolKind::LabelRef,
                    lid,
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        def_loc: SourceLocation {
                            file_id: fid,
                            container_id: cid,
                            func_id: 0,
                            byte_start: def_start as u32,
                            byte_end: def_stop as u32,
                        },
                        def_kind: SymbolKind::LabelDef,
                        cmie_kind: CmieKind::UNKNOWN,
                        def_name: resolve_def_name(def_names, file_uri, SymbolKind::LabelDef, lid),
                    },
                );
            }
        }
    }

    // ── BusRef generation (§3.2.3) ──
    // Same pattern as LabelRef: when a BusDef is co-located with a PortDef,
    // and a PortRef references that position, generate a BusRef→BusDef entry.
    let mut port_to_bus: HashMap<u32, (u32, (usize, usize))> = HashMap::new();
    {
        let mut pos_to_bus: HashMap<(usize, usize), u32> = HashMap::new();
        for ((kind, bid), loc) in def_map.iter() {
            if *kind == SymbolKind::BusDef {
                let ds = loc.byte_start as usize;
                let de = loc.byte_end as usize;
                pos_to_bus.insert((ds, de), *bid);
            }
        }
        for ((kind, pid), loc) in def_map.iter() {
            if *kind == SymbolKind::PortDef {
                let ds = loc.byte_start as usize;
                let de = loc.byte_end as usize;
                if let Some(&bid) = pos_to_bus.get(&(ds, de)) {
                    port_to_bus.insert(*pid, (bid, (ds, de)));
                }
            }
        }
    }
    if !port_to_bus.is_empty() {
        let fid = map.intern_file(file_uri);
        let cid = map.intern_container("");
        for &(ref_kind, decl_id, _ref_start, _ref_stop) in ref_entries {
            if ref_kind != SymbolKind::PortRef {
                continue;
            }
            if let Some(&(bid, (def_start, def_stop))) = port_to_bus.get(&decl_id) {
                if map.entries.contains_key(&(SymbolKind::BusRef, bid)) {
                    continue;
                }
                map.insert(
                    SymbolKind::BusRef,
                    bid,
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        def_loc: SourceLocation {
                            file_id: fid,
                            container_id: cid,
                            func_id: 0,
                            byte_start: def_start as u32,
                            byte_end: def_stop as u32,
                        },
                        def_kind: SymbolKind::BusDef,
                        cmie_kind: CmieKind::UNKNOWN,
                        def_name: resolve_def_name(def_names, file_uri, SymbolKind::BusDef, bid),
                    },
                );
            }
        }
    }

    // ── LabelDef→BusDef upgrade (§3.2.4 #6) ──
    // When upgrade_label_to_bus promotes a Label to Bus, both LabelDef and
    // BusDef exist at the same position. Upgrade LabelRef→LabelDef to BusRef→BusDef.
    let mut label_to_bus: HashMap<u32, (u32, (usize, usize))> = HashMap::new();
    {
        let mut pos_to_bus: HashMap<(usize, usize), u32> = HashMap::new();
        for ((kind, bid), loc) in def_map.iter() {
            if *kind == SymbolKind::BusDef {
                let ds = loc.byte_start as usize;
                let de = loc.byte_end as usize;
                pos_to_bus.insert((ds, de), *bid);
            }
        }
        for ((kind, lid), loc) in def_map.iter() {
            if *kind == SymbolKind::LabelDef {
                let ds = loc.byte_start as usize;
                let de = loc.byte_end as usize;
                if let Some(&bid) = pos_to_bus.get(&(ds, de)) {
                    label_to_bus.insert(*lid, (bid, (ds, de)));
                }
            }
        }
    }
    if !label_to_bus.is_empty() {
        let mut upgrades: Vec<(u32, u32, usize, usize)> = Vec::new();
        for &(ref_kind, decl_id, _ref_start, _ref_stop) in ref_entries {
            if ref_kind != SymbolKind::LabelRef && ref_kind != SymbolKind::PortRef {
                continue;
            }
            if let Some(&(bid, (def_start, def_stop))) = label_to_bus.get(&decl_id) {
                upgrades.push((decl_id, bid, def_start, def_stop));
            }
        }
        if !upgrades.is_empty() {
            let fid = map.intern_file(file_uri);
            let cid = map.intern_container("");
            for (_old_lid, bid, def_start, def_stop) in upgrades {
                if map.entries.contains_key(&(SymbolKind::BusRef, bid)) {
                    continue;
                }
                map.insert(
                    SymbolKind::BusRef,
                    bid,
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        def_loc: SourceLocation {
                            file_id: fid,
                            container_id: cid,
                            func_id: 0,
                            byte_start: def_start as u32,
                            byte_end: def_stop as u32,
                        },
                        def_kind: SymbolKind::BusDef,
                        cmie_kind: CmieKind::UNKNOWN,
                        def_name: resolve_def_name(def_names, file_uri, SymbolKind::BusDef, bid),
                    },
                );
            }
        }
    }
}
