// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! LSP Symbol workspace tables — extracted from `db/cmie/tables.rs`.
//!
//! Bundles the three LSP-specific tables that were previously mixed into
//! [`WorkspaceManager`] alongside CMIE data.

use crate::ast::ast_semantic::{DeclareId, Span};
use crate::ContainerKind;
use crate::McIds;
use std::collections::HashMap;
use std::sync::Mutex;

// ============================================================================
// LspTables — bundles all LSP-specific state
// ============================================================================

pub struct LspTables {
    /// (uri, kind, class_name) → (class_id, target_span)
    pub class_table: Mutex<HashMap<(String, ContainerKind, String), (DeclareId, Span)>>,
    /// Declare class references: uri → [(decl_span, class_id, target_uri,
    /// target_span, class_name, cmie_kind)]. `class_name` is the AST-derived
    /// `McIds` captured at registration time, so lapper creation does not need
    /// to re-read the source from disk (which fails for in-memory buffers /
    /// virtual URIs) nor rebuild a flattened string. `cmie_kind` is the class's
    /// real kind (Component/Module/Interface/Enum, or 255 UNKNOWN) so hover can
    /// label an interface ref as `→ interface` instead of a generic `→ class`.
    pub declare_class_refs: Mutex<HashMap<String, Vec<(Span, DeclareId, String, Span, McIds, u8)>>>,
}

impl LspTables {
    pub fn new() -> Self {
        Self {
            class_table: Mutex::new(HashMap::new()),
            declare_class_refs: Mutex::new(HashMap::new()),
        }
    }
}
