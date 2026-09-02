// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Ref collection from funcall arguments.
//!
//! Extracted from `db/infra/mc_code.rs` (see design doc §16).

use crate::ast::node::AstNode;
use crate::ast::sem::{DeclareId, LocalSymbolTable};
use crate::refdef::register::{lookup_declare_id, scope_path_from_scope_str};
use crate::McURI;

/// Recursively scan funcall argument nodes for identifier refs (SQUARE_VEC members,
/// bare IDs). Each found identifier is name-looked up via the local symbol table.
///
/// Returns `Vec<(span, DeclareId)>` — the caller labels them with the appropriate
/// SymbolKind (FuncParamRef by default, subject to §4.3 ref-type dispatch).
pub fn collect_funccall_arg_refs(
    arg_node: &AstNode,
    local_table: &LocalSymbolTable,
    file_uri: &McURI,
    enclosing: &str,
) -> Vec<(std::ops::Range<usize>, DeclareId)> {
    use crate::ast::macros::{
        MCAST_ID, MCAST_IDA, MCAST_IDS, MCAST_OPD_LEFTARROW, MCAST_OPD_MINUS, MCAST_OPD_PLUS,
        MCAST_OPD_RIGHTARROW, MCAST_OPD_SQUARE_VEC, MCAST_SQUARE_VEC,
    };
    let mut result = Vec::new();
    let ntype = arg_node.get_type();
    match ntype {
        MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC => {
            // Iterate members: [VDD_3V3, GND] → VDD_3V3, GND
            let mut cur = arg_node.get_sub_node();
            while let Some(member) = cur {
                // ★ A member may be a full connection expression, e.g.
                // `[dc.VDD_3V3 -> wm7121.VCC]`: the member node is an arrow
                // whose McIds name covers only the base chain (`dc.VDD_3V3`)
                // while its span covers the whole expression. A flat name
                // lookup would then register a whole-expression ref pointing
                // at the base member def (wrong span). Recurse so each arrow
                // operand is handled by its own branch instead.
                if matches!(
                    member.get_type(),
                    MCAST_OPD_RIGHTARROW | MCAST_OPD_LEFTARROW | MCAST_OPD_MINUS | MCAST_OPD_PLUS
                ) {
                    let mut sub_refs =
                        collect_funccall_arg_refs(&member, local_table, file_uri, enclosing);
                    result.append(&mut sub_refs);
                    cur = member.get_next();
                    continue;
                }
                let ids_node = member.get_sub_node().unwrap_or_else(|| member.clone());
                if let Some(ids) = crate::semantic::basic::mc_ids::McIds::new(&ids_node) {
                    let name = ids.to_string();
                    let span = (ids_node.get_pos() as usize)
                        ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                    let sp = scope_path_from_scope_str(file_uri, enclosing);
                    let decl_id = lookup_declare_id(local_table, &name, &sp);
                    tracing::info!(target: "mcc::lsp",
                        "FCALL_ARG_REF: member='{name}' span=[{},{}] enclosing='{enclosing}' decl_id={}",
                        span.start, span.end,
                        decl_id.map(|d| u32::from(d) as i64).unwrap_or(-1)
                    );
                    if let Some(did) = decl_id {
                        result.push((span, did));
                    }
                }
                cur = member.get_next();
            }
        }
        MCAST_ID | MCAST_IDA | MCAST_IDS => {
            if let Some(ids) = crate::semantic::basic::mc_ids::McIds::new(arg_node) {
                let name = ids.to_string();
                let start = arg_node.get_pos() as usize;
                let sp = scope_path_from_scope_str(file_uri, enclosing);
                let base = ids.root_name().unwrap_or_else(|| name.clone());

                // ★ §3.4.3 (rev): per-segment chain resolution.
                //   - `MIC{P,N}` (named curly bus, whole reference): one ref on the
                //     base span → BusDef(MIC). Members are not separately registered
                //     here; the whole-bus reference points at the bus def.
                //   - `MIC.P` (dot member): TWO refs — base segment `MIC` →
                //     BusDef(MIC), member segment `P` → full-name lookup `MIC.P`
                //     (member BusMemberDef registered at declaration), falling
                //     back to the base `MIC` when the member isn't defined.
                if let Some((bus, _members)) = ids.as_bus() {
                    let base_span = start..(start + bus.len());
                    let decl_id = lookup_declare_id(local_table, &bus, &sp);
                    tracing::info!(target: "mcc::lsp",
                        "FCALL_ARG_REF: member='{name}' (lookup='{bus}') span=[{},{}] enclosing='{enclosing}' decl_id={}",
                        base_span.start, base_span.end,
                        decl_id.map(|d| u32::from(d) as i64).unwrap_or(-1)
                    );
                    if let Some(did) = decl_id {
                        result.push((base_span, did));
                    }
                } else if ids.count() >= 2 && !ids.is_square_only() {
                    // Dot-member form: `MIC.P` → register base + member segments.
                    let base_len = base.len();
                    let member_start = start + base_len + 1;
                    let member_end = start + name.len();
                    let member_span = member_start..member_end;
                    let base_span = start..(start + base_len);
                    let base_start = base_span.start;
                    let base_end = base_span.end;
                    let base_id = lookup_declare_id(local_table, &base, &sp);
                    if let Some(did) = base_id {
                        result.push((base_span, did));
                    }
                    let member_id = lookup_declare_id(local_table, &name, &sp)
                        .or_else(|| lookup_declare_id(local_table, &base, &sp));
                    tracing::info!(target: "mcc::lsp",
                        "FCALL_ARG_REF: member='{name}' (dot) base='{base}' base_span=[{},{}] member_span=[{},{}] decl_id={}",
                        base_start, base_end, member_span.start, member_span.end,
                        member_id.map(|d| u32::from(d) as i64).unwrap_or(-1)
                    );
                    if let Some(did) = member_id {
                        result.push((member_span, did));
                    }
                } else {
                    // Plain single identifier.
                    let span = start..(start + name.len());
                    let decl_id = lookup_declare_id(local_table, &name, &sp);
                    tracing::info!(target: "mcc::lsp",
                        "FCALL_ARG_REF: member='{name}' span=[{},{}] enclosing='{enclosing}' decl_id={}",
                        span.start, span.end,
                        decl_id.map(|d| u32::from(d) as i64).unwrap_or(-1)
                    );
                    if let Some(did) = decl_id {
                        result.push((span, did));
                    }
                }
            }
        }
        _ => {
            // Recurse into children
            if let Some(sub) = arg_node.get_sub_node() {
                let mut cur = Some(sub);
                while let Some(child) = cur {
                    let mut child_refs =
                        collect_funccall_arg_refs(&child, local_table, file_uri, enclosing);
                    result.append(&mut child_refs);
                    cur = child.get_next();
                }
            }
        }
    }
    result
}

/// Resolve the correct Ref kind for a funcall argument based on its def type.
///
/// Looks up `decl_id` in `def_map` to find the def's SymbolKind, then maps it to
/// the appropriate Ref kind. This replaces the old catch-all `FuncParamRef`
/// behaviour (see design doc §4.3).
///
/// Priority (first match wins):
///   LabelDef → LabelRef, BusDef → BusRef, PinNameDef → PinNameRef,
///   PinIdDef → PinIdRef, PinIfaceDef → PinIfaceRef, ParamDef → FuncParamRef,
///   PortDef → PortRef, InstDef → InstRef, fallback → FuncParamRef
pub fn resolve_arg_ref_kind(
    def_map: &std::collections::HashMap<
        (crate::refdef::SymbolKind, u32),
        crate::refdef::SourceLocation,
    >,
    decl_id: crate::ast::sem::DeclareId,
) -> crate::refdef::SymbolKind {
    use crate::refdef::SymbolKind;
    let raw_id = u32::from(decl_id);

    // Try specific def types first (higher confidence match)
    let candidates: &[(SymbolKind, SymbolKind)] = &[
        // ★ §3.4.3 (rev): bus member def is the most precise match.
        (SymbolKind::BusMemberDef, SymbolKind::BusMemberRef),
        (SymbolKind::LabelDef, SymbolKind::LabelRef),
        (SymbolKind::BusDef, SymbolKind::BusRef),
        (SymbolKind::PinNameDef, SymbolKind::PinNameRef),
        (SymbolKind::PinIdDef, SymbolKind::PinIdRef),
        (SymbolKind::PinIfaceDef, SymbolKind::PinIfaceRef),
        (SymbolKind::ParamDef, SymbolKind::FuncParamRef),
        (SymbolKind::PortDef, SymbolKind::PortRef),
        (SymbolKind::InstDef, SymbolKind::InstRef),
    ];

    for &(def_kind, ref_kind) in candidates {
        if def_map.contains_key(&(def_kind, raw_id)) {
            return ref_kind;
        }
    }

    // Fallback — should not happen if def_map is complete
    SymbolKind::FuncParamRef
}
