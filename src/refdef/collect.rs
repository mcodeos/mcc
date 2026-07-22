// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Ref collection from funcall arguments.
//!
//! Extracted from `db/infra/mc_code.rs` (see design doc §16).

use crate::ast::ast_semantic::{DeclareId, LocalSymbolTable};
use crate::ast::ast_node::AstNode;
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
    use crate::ast::c_macros::{
        MCAST_ID, MCAST_IDA, MCAST_IDS, MCAST_OPD_SQUARE_VEC, MCAST_SQUARE_VEC,
    };
    let mut result = Vec::new();
    let ntype = arg_node.get_type();
    match ntype {
        MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC => {
            // Iterate members: [VDD_3V3, GND] → VDD_3V3, GND
            let mut cur = arg_node.get_sub_node();
            while let Some(member) = cur {
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
                let span = (arg_node.get_pos() as usize)
                    ..((arg_node.get_pos() + arg_node.get_len()) as usize);
                let sp = scope_path_from_scope_str(file_uri, enclosing);
                let decl_id = lookup_declare_id(local_table, &name, &sp);
                if let Some(did) = decl_id {
                    result.push((span, did));
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
    def_map: &std::collections::HashMap<(crate::refdef::SymbolKind, u32), crate::refdef::SourceLocation>,
    decl_id: crate::ast::ast_semantic::DeclareId,
) -> crate::refdef::SymbolKind {
    use crate::refdef::SymbolKind;
    let raw_id = u32::from(decl_id);

    // Try specific def types first (higher confidence match)
    let candidates: &[(SymbolKind, SymbolKind)] = &[
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
