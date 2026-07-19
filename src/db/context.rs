// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Database-backed implementations of the semantic context traits.
//!
//! Provides concrete implementations of [`crate::semantic::context`] traits
//! backed by the global workspace / system tables in `db/`.

use crate::ast::ast_semantic::{DeclareId, Span};
use crate::semantic::context::{DiagnosticSeverity, DiagnosticSink, NameResolver, SymbolRegistry};
use crate::{McCMIE, McIds, McURI};

// ============================================================================
// DbContext — single struct implementing all three traits
// ============================================================================

pub struct DbContext;

fn mk_span(pos: u32, len: u32) -> Span {
    pos as usize..(pos + len) as usize
}

impl NameResolver for DbContext {
    fn resolve(&self, class_name: &McIds, from_uri: &McURI) -> Option<(McCMIE, McURI)> {
        crate::db::cmie::cmie::mcb_get_cmie_with_uri(class_name, from_uri)
    }

    fn resolve_system(&self, class_name: &McIds) -> Option<McCMIE> {
        use crate::db::infra::global;
        let name_str = class_name.to_string();
        for entry in global::mcc_components.borrow().iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Component(entry.value().clone()));
            }
        }
        for entry in global::mcc_modules.borrow().iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Module(entry.value().clone()));
            }
        }
        for entry in global::mcc_interfaces.borrow().iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Interface(entry.value().clone()));
            }
        }
        for entry in global::mcc_enums.borrow().iter() {
            if entry.key().ident.to_string() == name_str {
                return Some(McCMIE::Enum(entry.value().clone()));
            }
        }
        None
    }
}

impl SymbolRegistry for DbContext {
    fn register_instance_decl(
        &self,
        uri: &str,
        scope: Option<&str>,
        name: &str,
        pos: u32,
        len: u32,
    ) -> u32 {
        let span = mk_span(pos, len);
        let id = crate::db::cmie::tables::WORKSPACE
            .global_inst_table
            .lock()
            .map(|mut t| t.add(uri, scope, name, span))
            .unwrap_or_default();
        id.raw()
    }

    fn register_instance_ref(
        &self,
        uri: &str,
        decl_id: u32,
        scope: Option<&str>,
        pos: u32,
        len: u32,
    ) {
        let span = mk_span(pos, len);
        let _ = crate::db::cmie::tables::WORKSPACE
            .global_inst_table
            .lock()
            .map(|mut t| t.add_ref(DeclareId::from_raw(decl_id), uri, scope, span));
    }

    fn lookup_instance_decl(&self, uri: &str, name: &str, scope: Option<&str>) -> Option<u32> {
        crate::db::cmie::tables::WORKSPACE
            .global_inst_table
            .lock()
            .ok()
            .and_then(|t| t.get(uri, scope, name))
            .map(|id| id.raw())
    }

    fn register_declare_class(&self, uri: &str, class_name: &str, pos: u32, len: u32) {
        let span = mk_span(pos, len);
        let _ = crate::db::cmie::tables::WORKSPACE
            .global_class_table
            .lock()
            .map(|mut t| {
                t.insert(
                    (
                        uri.to_string(),
                        crate::ContainerKind::Component,
                        class_name.to_string(),
                    ),
                    (DeclareId::from_raw(0), span),
                )
            });
    }

    fn find_refs(&self, name: &str) -> Vec<(String, String, (u32, u32))> {
        crate::query::refs::mcb_get_refs(name)
            .into_iter()
            .map(|(uri, scope, span)| (uri, scope, (span.start as u32, span.end as u32)))
            .collect()
    }
}

impl DiagnosticSink for DbContext {
    fn report(
        &self,
        code: u32,
        severity: DiagnosticSeverity,
        _uri: &str,
        pos: u32,
        len: u32,
        message: &str,
        _suggestions: &[String],
    ) {
        let level = match severity {
            DiagnosticSeverity::Hint => crate::db::diagnostic::diagnostic::DiagnosticLevel::Hint,
            DiagnosticSeverity::Info => crate::db::diagnostic::diagnostic::DiagnosticLevel::Info,
            DiagnosticSeverity::Warning => crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
            DiagnosticSeverity::Error => crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
        };
        crate::db::diagnostic::diagnostic::diagnostic_log(code, level, pos, len, message, &[]);
    }
}

// ============================================================================
// Singleton
// ============================================================================

/// The global database context — used when no trait injection is needed.
pub static DB: DbContext = DbContext;
