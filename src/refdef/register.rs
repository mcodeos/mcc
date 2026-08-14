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
/// There is no P3 name-only fallback: a name-only HashMap scan is
/// non-deterministic and produces random cross-container hits. A miss returns
/// None; the fix for a miss is registering the def with its proper scope.
/// CMIE class names (component/module/interface/enum/define) are resolved
/// via `mcb_get_cmie`, not by this function.
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

    // No P3. A name-only HashMap scan would be non-deterministic (random
    // cross-container hits like `GND` → an unrelated component's GND). When
    // P1/P2 miss, return None so goto-definition fails cleanly instead of
    // jumping to an arbitrary same-named def. The correct fix for a miss is
    // registering the def with its proper scope, not a random fallback.

    None
}

/// Emit a "not found" diagnostic for a reference that could not be resolved after
/// the full P1–P5 lookup chain. Call this at the *final* miss point — after the
/// caller has exhausted `lookup_declare_id` (P1/P2) plus any structural /
/// class-name fallbacks (P3→P4→P5 via `mcb_get_cmie` / member-chain resolution).
///
/// Mirrors the design docs (`name-space-global.md` §1.3, `name-space-internal.md`
/// §1.3): all levels miss → `Unresolved / diagnostic error`, which must surface instead
/// of being silently dropped.
pub fn report_unresolved_ref(span: &std::ops::Range<usize>, name: &str) {
    crate::db::diagnostic::diagnostic::dlog_error_at(
        crate::db::diagnostic::errcodes::SYMBOL_NOT_FOUND,
        span.start as u32,
        span.end.saturating_sub(span.start) as u32,
        &format!("cannot find '{name}'"),
    );
}
