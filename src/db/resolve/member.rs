// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Member resolution within a resolved CMIE class (`A.B` member access).
//!
//! Class lookup itself goes through [`Resolver::resolve_class`] (P3→P4→P5);
//! this module resolves the member (func / pin / enum value) inside the
//! resolved class and reports where it is defined.

use super::policy::same_name_cmies;
use crate::ast::sem::{McSemSymbols, SymbolKind};
use crate::db::resolve::Resolver;
use crate::{McCMIE, McIds, McURI};

/// Same as [`resolve_cmie_member`], but for callers that already hold the
/// referencing file's `symbols` lock (`create_lapper`). The class lookup runs
/// through [`Resolver::resolve_class_locked`], which reads the RefDefMap from
/// the caller's `sem` instead of re-locking the same file's symbols
/// (std Mutex is not reentrant — re-locking would self-deadlock).
pub(crate) fn resolve_cmie_member_locked(
    class_name: &str,
    member_name: &str,
    from_uri: &McURI,
    sem: &McSemSymbols,
) -> Option<(McURI, std::ops::Range<usize>, SymbolKind)> {
    let ids = McIds::from(class_name);
    let winner = Resolver::resolve_class_locked(from_uri, &ids, sem);
    if let Some(hit) = winner.as_ref().and_then(|c| member_of(c, member_name)) {
        return Some(hit);
    }
    // The winner is picked by `NameIndexCandidate::policy_key`, whose family
    // preference ranks the enum family before the class family — so for a
    // coexisting `component CAP` + `enum CAP` the bare name resolves to the
    // enum (which `CAP.X5R` needs), and a *member* lookup such as
    // `CAP(...).Cap(...)` would miss on that winner alone. A member is
    // kind-specific, so a miss is not yet a miss: walk the whole same-name
    // bucket in the same policy order and take the first candidate that
    // actually declares the member. Same rule `visibility.rs` applies for
    // P3/P4 — check the bucket, never just the winner.
    same_name_cmies(from_uri, &ids, sem)
        .iter()
        .find_map(|cmie| member_of(cmie, member_name))
}

/// Match `member_name` against a resolved class definition.
fn member_of(
    cmie: &McCMIE,
    member_name: &str,
) -> Option<(McURI, std::ops::Range<usize>, SymbolKind)> {
    match cmie {
        McCMIE::Component(comp) => {
            if let Some(func) = comp.funcs.find(member_name) {
                let span = func.span.clone()?;
                return Some((comp.uri.clone(), span, SymbolKind::FuncRef));
            }
        }
        McCMIE::Module(mod_def) => {
            if let Some(func) = mod_def.funcs.find(member_name) {
                let span = func.span.clone()?;
                return Some((mod_def.uri.clone(), span, SymbolKind::FuncRef));
            }
        }
        McCMIE::Enum(enum_def) => {
            for value in &enum_def.values {
                if value.name.to_string() == member_name {
                    let span = value.span[0] as usize..value.span[1] as usize;
                    return Some((enum_def.uri.clone(), span, SymbolKind::EnumValRef));
                }
            }
        }
        McCMIE::Interface(iface) => {
            // Interface member pins: resolve to the precise pin span
            // (goto-definition) instead of falling through silently.
            if let Some(range) = iface.pins.pin_name_spans.get(member_name) {
                return Some((iface.uri.clone(), range.clone(), SymbolKind::PinNameRef));
            }
            if let Some(range) = iface.pins.pin_id_spans.get(member_name) {
                return Some((iface.uri.clone(), range.clone(), SymbolKind::PinIdRef));
            }
        }
    }
    None
}
