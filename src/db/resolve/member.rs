// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Member resolution within a resolved CMIE class (`A.B` member access).
//!
//! Class lookup itself goes through [`Resolver::resolve_class`] (P3→P4→P5);
//! this module resolves the member (func / pin / enum value) inside the
//! resolved class and reports where it is defined.

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
    let cmie = Resolver::resolve_class_locked(from_uri, &ids, sem)?;
    member_of(&cmie, member_name)
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
