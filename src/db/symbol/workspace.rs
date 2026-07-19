// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! LSP Symbol workspace tables — extracted from `db/cmie/tables.rs`.
//!
//! Bundles the three LSP-specific tables that were previously mixed into
//! [`WorkspaceManager`] alongside CMIE data.

use crate::ast::ast_semantic::{DeclareId, Span};
use crate::ContainerKind;
use std::collections::HashMap;
use std::sync::Mutex;

// ============================================================================
// LspTables — bundles all LSP-specific state
// ============================================================================

pub struct LspTables {
    /// Cross-file instance declaration table.
    pub inst_table: Mutex<GlobalInstTable>,
    /// (uri, kind, class_name) → (class_id, target_span)
    pub class_table: Mutex<HashMap<(String, ContainerKind, String), (DeclareId, Span)>>,
    /// Declare class references: uri → [(decl_span, class_id, target_uri, target_span)]
    pub declare_class_refs: Mutex<HashMap<String, Vec<(Span, DeclareId, String, Span)>>>,
}

impl LspTables {
    pub fn new() -> Self {
        Self {
            inst_table: Mutex::new(GlobalInstTable::default()),
            class_table: Mutex::new(HashMap::new()),
            declare_class_refs: Mutex::new(HashMap::new()),
        }
    }
}

// ============================================================================
// GlobalInstTable — cross-file instance declaration index
// ============================================================================

#[derive(Default)]
pub struct GlobalInstTable {
    counter: DeclareId,
    name_to_id: HashMap<(String, String, String), DeclareId>,
    id_to_span: HashMap<DeclareId, (String, String, Span)>,
    refs: HashMap<DeclareId, Vec<(String, String, Span)>>,
}

impl GlobalInstTable {
    pub fn add(&mut self, uri: &str, scope: Option<&str>, name: &str, span: Span) -> DeclareId {
        let scope_str = scope.unwrap_or("");
        let key = (uri.to_string(), scope_str.to_string(), name.to_string());
        if let Some(&id) = self.name_to_id.get(&key) {
            return id;
        }
        let id = self.counter;
        self.counter += 1;
        self.name_to_id.insert(key, id);
        self.id_to_span
            .insert(id, (uri.to_string(), scope_str.to_string(), span));
        id
    }

    pub fn get(&self, uri: &str, scope: Option<&str>, name: &str) -> Option<DeclareId> {
        let scope_str = scope.unwrap_or("");
        let key = (uri.to_string(), scope_str.to_string(), name.to_string());
        self.name_to_id.get(&key).copied()
    }

    pub fn get_span(&self, id: DeclareId) -> Option<(String, String, Span)> {
        self.id_to_span.get(&id).cloned()
    }

    pub fn get_decls_for_uri(&self, uri: &str) -> Vec<(DeclareId, String, Span)> {
        self.name_to_id
            .iter()
            .filter(|((u, _, _), _)| u == uri)
            .filter_map(|((_, scope, _), id)| {
                self.id_to_span
                    .get(id)
                    .map(|(_, _, span)| (*id, scope.clone(), span.clone()))
            })
            .collect()
    }

    pub fn add_ref(&mut self, decl_id: DeclareId, uri: &str, scope: Option<&str>, span: Span) {
        let scope_str = scope.unwrap_or("");
        self.refs
            .entry(decl_id)
            .or_default()
            .push((uri.to_string(), scope_str.to_string(), span));
    }

    pub fn get_refs(&self, decl_id: DeclareId) -> Vec<(String, String, Span)> {
        self.refs.get(&decl_id).cloned().unwrap_or_default()
    }

    pub fn find_decls_by_name(&self, name: &str) -> Vec<DeclareId> {
        self.name_to_id
            .iter()
            .filter(|((_, _, n), _)| n == name)
            .map(|(_, id)| *id)
            .collect()
    }

    pub fn get_all_refs_for_uri(&self, uri: &str) -> Vec<(DeclareId, String, Span)> {
        let mut result = Vec::new();
        for (decl_id, spans) in &self.refs {
            for (ref_uri, scope, span) in spans {
                if ref_uri == uri {
                    result.push((*decl_id, scope.clone(), span.clone()));
                }
            }
        }
        result
    }

    pub fn len(&self) -> u32 {
        self.counter.raw()
    }
}
