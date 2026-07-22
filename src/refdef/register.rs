// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Def registration and name→DeclareId lookup.
//!
//! Extracted from `db/infra/mc_code.rs` (see design doc §16).

use crate::ast::ast_semantic::{DeclareId, LocalSymbolTable, McSemSymbols};
use crate::refdef::types::{intern, SourceLocation, SymbolKind};
use crate::McURI;

// ── Scope path helper ──

/// Build a ScopePath from a scope string and file URI.
/// "US513" → module,  "US513.i2c" → func-in-module,  "" → file-level.
pub fn scope_path_from_scope_str(uri: &McURI, scope: &str) -> crate::ScopePath {
    if scope.is_empty() {
        crate::ScopePath::file_level(uri)
    } else if let Some(dot_pos) = scope.rfind('.') {
        let container = &scope[..dot_pos];
        let func = &scope[dot_pos + 1..];
        crate::ScopePath::func_in_module(uri, container, func)
    } else {
        crate::ScopePath::module(uri, scope)
    }
}

// ── Def registration ──

pub fn register_def(
    sem: &mut McSemSymbols,
    uri: &McURI,
    container: &str,
    func: Option<&str>,
    name: &str,
    span: std::ops::Range<usize>,
    def_kind: SymbolKind,
) -> (DeclareId, SourceLocation) {
    let file_id = intern(&mut sem.file_table, uri.as_str());
    let container_id = if container.is_empty() {
        0
    } else {
        intern(&mut sem.container_table, container)
    };
    let func_id = func
        .filter(|f| !f.is_empty())
        .map(|f| intern(&mut sem.func_table, f))
        .unwrap_or(0);
    let scope = match func {
        Some(f) if !f.is_empty() => format!("{container}.{f}"),
        _ => container.to_string(),
    };
    let loc = SourceLocation {
        file_id,
        container_id,
        func_id,
        byte_start: span.start as u32,
        byte_end: span.end as u32,
    };
    let decl_id =
        sem.local_table
            .add_declare_with_name(uri, loc, Some(name.to_string()), Some(&scope));
    sem.def_map.insert((def_kind, decl_id.raw()), loc);
    (decl_id, loc)
}

// ── Name → DeclareId lookup ──

/// Resolve a name to its DeclareId within a container scope.
///
/// ## Lookup priority (higher shadows lower):
///   P1: current func scope — func params, func body labels
///   P2: current container  — module/component/interface/enum internal defs
///
/// Internal defs (ports, instances, labels, funcs) are container-scoped
/// and do NOT leak to file-level or cross-file visibility (§3.2.2).
/// There is intentionally NO P3/P4/P5 fallback — those levels are for
/// CMIE class names (component/module/interface/enum/define) resolved
/// via `mcb_get_cmie`, not for port/instance refs.
pub fn lookup_declare_id(
    local: &LocalSymbolTable,
    name: &str,
    scope_path: &crate::ScopePath,
) -> Option<DeclareId> {
    let ref_scope = scope_path.scope_key();

    // P1: exact scope match — scope identified by scope string via scope_index
    if let Some((id, _)) = local.lookup_by_scope_name(&ref_scope, name) {
        return Some(id);
    }

    // P2: container-level match — when inside a func, fall back to
    //   the parent container (module/component) scope
    if scope_path.func.is_some() {
        let container_scope = &scope_path.container.name;
        if let Some((id, _)) = local.lookup_by_scope_name(container_scope, name) {
            return Some(id);
        }
    }

    None
}
