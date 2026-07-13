// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Workspace abstraction layer — PR-3D core.
//!
//! ## Design rationale
//!
//! Before PR-2, all project data lived in `global.rs`'s `prj_*` lazy_static singletons,
//! only one project could run at a time.
//!
//! PR-3 packages `prj_*` into [`WorkspaceTables`], managed by [`WorkspaceManager`].
//! System tables (`mcc_*`) are unchanged, shared by all workspaces.
//!
//! Currently still single-process: `WORKSPACE` is a global singleton, internally holding
//! a set of DashMaps as **active workspace** data. When switching workspaces, first snapshot
//! current data, then restore target workspace's snapshot (or create empty tables). After
//! PR-4 daemonization, each workspace holds independent tables, routed via RPC to the
//! corresponding workspace.
//!
//! ## Migration guide
//!
//! All `global::prj_X.borrow()` calls become `workspace::WORKSPACE.X.borrow()`:
//!
//! ```text
//! global::prj_mcodes.borrow()     →  workspace::WORKSPACE.mcodes.borrow()
//! global::prj_modules.borrow()    →  workspace::WORKSPACE.modules.borrow()
//! global::prj_components.borrow() →  workspace::WORKSPACE.components.borrow()
//! global::prj_interfaces.borrow() →  workspace::WORKSPACE.interfaces.borrow()
//! global::prj_enums.borrow()      →  workspace::WORKSPACE.enums.borrow()
//! ```
//!
//! `diagnostic_manager` also migrated, path:
//! ```text
//! diagnostic::diagnostic_manager.borrow()     →  workspace::WORKSPACE.diagnostics.borrow()
//! diagnostic::diagnostic_manager.borrow_mut() →  workspace::WORKSPACE.diagnostics.borrow_mut()
//! ```

use crate::ast::ast_semantic::{DeclareId, Span};
use crate::builder::diagnostic::DiagnosticManager;
use crate::builder::mc_code::McCode;
use crate::builder::util::MultiThreadRefCell;
use crate::core::component::McComponent;
use crate::core::mc_enum::McEnumDef;
use crate::core::mc_ifs::McInterface;
use crate::core::module::McModule;
use crate::{McSpaceName, McURI};
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

// ============================================================================
// WorkspaceKind
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceKind {
    Project,
}

impl Default for WorkspaceKind {
    fn default() -> Self {
        Self::Project
    }
}

// ============================================================================
// WorkspaceMeta — per-workspace metadata
// ============================================================================

#[derive(Debug, Clone)]
pub struct WorkspaceMeta {
    pub id: String,
    pub kind: WorkspaceKind,
    pub root: PathBuf,
    pub entry: Option<String>,
    pub top_module: Option<String>,
}

impl Default for WorkspaceMeta {
    fn default() -> Self {
        Self {
            id: "default".into(),
            kind: WorkspaceKind::Project,
            root: PathBuf::from("."),
            entry: None,
            top_module: None,
        }
    }
}

// ============================================================================
// WorkspaceSnapshot — for save/restore when switching workspaces
// ============================================================================

struct WorkspaceSnapshot {
    meta: WorkspaceMeta,
    mcodes: DashMap<McURI, McCode>,
    modules: DashMap<McSpaceName, Arc<McModule>>,
    components: DashMap<McSpaceName, Arc<McComponent>>,
    interfaces: DashMap<McSpaceName, Arc<McInterface>>,
    enums: DashMap<McSpaceName, Arc<McEnumDef>>,
    diagnostics: DiagnosticManager,
}

// ============================================================================
// WorkspaceManager (singleton)
// ============================================================================

pub struct WorkspaceManager {
    pub(crate) mcodes: MultiThreadRefCell<DashMap<McURI, McCode>>,
    pub(crate) modules: MultiThreadRefCell<DashMap<McSpaceName, Arc<McModule>>>,
    pub(crate) components: MultiThreadRefCell<DashMap<McSpaceName, Arc<McComponent>>>,
    pub(crate) interfaces: MultiThreadRefCell<DashMap<McSpaceName, Arc<McInterface>>>,
    pub(crate) enums: MultiThreadRefCell<DashMap<McSpaceName, Arc<McEnumDef>>>,
    pub(crate) diagnostics: MultiThreadRefCell<DiagnosticManager>,

    meta: MultiThreadRefCell<WorkspaceMeta>,

    saved: MultiThreadRefCell<HashMap<String, WorkspaceSnapshot>>,

    // ★ LSP: Shared global instance declaration table (cross-file)
    pub(crate) global_inst_table: Mutex<GlobalInstTable>,

    // ★ LSP: Shared global class table (cross-file lookups)
    // (uri_where_defined, class_name) -> (class_id, target_span)
    pub(crate) global_class_table: Mutex<HashMap<(String, String), (DeclareId, Span)>>,

    // ★ LSP: Declare class references (uri -> [(decl_span, class_id, target_uri, target_span)])
    // Used when file is being parsed and not yet in workspace mcodes
    pub(crate) global_declare_class_refs:
        Mutex<HashMap<String, Vec<(Span, DeclareId, String, Span)>>>,
}

#[derive(Default)]
pub struct GlobalInstTable {
    counter: DeclareId,
    name_to_id: HashMap<(String, String, String), DeclareId>, // (uri, scope, name) -> decl_id
    id_to_span: HashMap<DeclareId, (String, String, Span)>,   // decl_id -> (uri, scope, span)
    refs: HashMap<DeclareId, Vec<(String, String, Span)>>, // decl_id -> [(uri, scope, span), ...]
}

impl GlobalInstTable {
    pub fn add(&mut self, uri: &str, scope: Option<&str>, name: &str, span: Span) -> DeclareId {
        let scope_str = scope.unwrap_or("");
        let key = (uri.to_string(), scope_str.to_string(), name.to_string());
        if let Some(&id) = self.name_to_id.get(&key) {
            return id; // Already registered, return existing id
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

    // ★ LSP: Get all instance declarations for a given URI, with scope info
    pub fn get_decls_for_uri(&self, uri: &str) -> Vec<(DeclareId, String, Span)> {
        self.name_to_id
            .iter()
            .filter(|((u, _scope, _name), _id)| u == uri)
            .filter_map(|((_, scope, _name), id)| {
                self.id_to_span
                    .get(id)
                    .map(|(_, _, span)| (*id, scope.clone(), span.clone()))
            })
            .collect()
    }

    // ★ LSP: Store instance references (for finding all usages)
    pub fn add_ref(&mut self, decl_id: DeclareId, uri: &str, scope: Option<&str>, span: Span) {
        let scope_str = scope.unwrap_or("");
        self.refs.entry(decl_id).or_insert_with(Vec::new).push((
            uri.to_string(),
            scope_str.to_string(),
            span,
        ));
    }

    pub fn get_refs(&self, decl_id: DeclareId) -> Vec<(String, String, Span)> {
        self.refs.get(&decl_id).cloned().unwrap_or_default()
    }

    /// M6: Find all decl_ids with the given name.
    pub fn find_decls_by_name(&self, name: &str) -> Vec<DeclareId> {
        self.name_to_id
            .iter()
            .filter(|((_, _, n), _)| n == name)
            .map(|(_, id)| *id)
            .collect()
    }

    // ★ LSP: Get all refs for all decls in a specific file
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

impl WorkspaceManager {
    fn new() -> Self {
        Self {
            mcodes: MultiThreadRefCell::new(DashMap::new()),
            modules: MultiThreadRefCell::new(DashMap::new()),
            components: MultiThreadRefCell::new(DashMap::new()),
            interfaces: MultiThreadRefCell::new(DashMap::new()),
            enums: MultiThreadRefCell::new(DashMap::new()),
            diagnostics: MultiThreadRefCell::new(DiagnosticManager::new()),
            meta: MultiThreadRefCell::new(WorkspaceMeta::default()),
            saved: MultiThreadRefCell::new(HashMap::new()),
            global_inst_table: Mutex::new(GlobalInstTable::default()),
            global_class_table: Mutex::new(HashMap::new()),
            global_declare_class_refs: Mutex::new(HashMap::new()),
        }
    }

    // ================================================================
    // Query
    // ================================================================

    pub fn active_id(&self) -> String {
        self.meta.borrow().id.clone()
    }

    pub fn active_kind(&self) -> WorkspaceKind {
        self.meta.borrow().kind.clone()
    }

    pub fn active_root(&self) -> PathBuf {
        self.meta.borrow().root.clone()
    }

    pub fn active_meta(&self) -> WorkspaceMeta {
        self.meta.borrow().clone()
    }

    /// Look up a component by its class name (ident string).
    /// Checks workspace project tables first, then falls back to global system tables.
    /// Returns `None` if no component with that class name is registered.
    pub fn component_by_class(&self, class_name: &str) -> Option<Arc<McComponent>> {
        for entry in self.components.borrow().iter() {
            if entry.key().ident.to_string() == class_name {
                return Some(entry.value().clone());
            }
        }
        // Fallback: check global system component table
        for entry in crate::builder::global::mcc_components.borrow().iter() {
            if entry.key().ident.to_string() == class_name {
                return Some(entry.value().clone());
            }
        }
        None
    }

    pub fn list(&self) -> Vec<(String, WorkspaceKind)> {
        let mut result = Vec::new();
        {
            let m = self.meta.borrow();
            result.push((m.id.clone(), m.kind.clone()));
        }
        for entry in self.saved.borrow().iter() {
            result.push((entry.0.clone(), entry.1.meta.kind.clone()));
        }
        result
    }

    // ================================================================
    // Clear current active workspace
    // ================================================================

    pub fn clear_active(&self) {
        self.mcodes.borrow().clear();
        self.modules.borrow().clear();
        self.components.borrow().clear();
        self.interfaces.borrow().clear();
        self.enums.borrow().clear();
        self.diagnostics.borrow_mut().clear();
    }

    // ================================================================
    // Create workspace
    // ================================================================

    pub fn create_and_switch(&self, id: String, kind: WorkspaceKind, root: PathBuf) -> bool {
        if self.meta.borrow().id == id {
            return false;
        }
        if self.saved.borrow().contains_key(&id) {
            return false;
        }

        self.snapshot_active();

        self.clear_active();
        *self.meta.borrow_mut() = WorkspaceMeta {
            id: id.clone(),
            kind,
            root,
            entry: None,
            top_module: None,
        };

        info!(target: "mcc::workspace", id = %id, "created and switched to new workspace");
        true
    }

    // ================================================================
    // Switch workspace
    // ================================================================
    // Auto-set project path when switching projects
    pub fn switch_to(&self, id: &str) -> bool {
        if self.meta.borrow().id == id {
            return false;
        }

        let snapshot = match self.saved.borrow_mut().remove(id) {
            Some(s) => s,
            None => return false,
        };

        self.snapshot_active();

        self.restore_snapshot(snapshot);

        // Note: mcb_set_project_root may cause tokio runtime deadlock, temporarily disabled
        // let root = self.meta.borrow().root.clone();
        // crate::builder::mcb_set_project_root(&root);

        info!(target: "mcc::workspace", id = %id, "switched to workspace");
        true
    }

    // ================================================================
    // Remove workspace
    // ================================================================

    pub fn remove(&self, id: &str) -> bool {
        if self.meta.borrow().id == id {
            return false;
        }
        self.saved.borrow_mut().remove(id).is_some()
    }

    // ================================================================
    // Internal: snapshot / restore
    // ================================================================

    fn snapshot_active(&self) {
        let meta = self.meta.borrow().clone();
        let id = meta.id.clone();

        // Optimization: clone data under read lock, then quickly clear
        // This reduces write lock hold time
        let mcodes = clone_and_clear(&self.mcodes);
        let modules = clone_and_clear(&self.modules);
        let components = clone_and_clear(&self.components);
        let interfaces = clone_and_clear(&self.interfaces);
        let enums = clone_and_clear(&self.enums);
        let diagnostics = self.diagnostics.borrow_mut().take();

        let snap = WorkspaceSnapshot {
            meta,
            mcodes,
            modules,
            components,
            interfaces,
            enums,
            diagnostics,
        };

        debug!(target: "mcc::workspace", id = %id, "snapshot saved");
        self.saved.borrow_mut().insert(id, snap);
    }

    fn restore_snapshot(&self, snap: WorkspaceSnapshot) {
        self.clear_active();

        *self.meta.borrow_mut() = snap.meta;

        // Optimization: use write lock to fill data
        fill_dashmap(&self.mcodes, snap.mcodes);
        fill_dashmap(&self.modules, snap.modules);
        fill_dashmap(&self.components, snap.components);
        fill_dashmap(&self.interfaces, snap.interfaces);
        fill_dashmap(&self.enums, snap.enums);

        *self.diagnostics.borrow_mut() = snap.diagnostics;
    }
}

// ============================================================================
// lazy_static singleton
// ============================================================================

lazy_static! {
    pub(crate) static ref WORKSPACE: WorkspaceManager = WorkspaceManager::new();
}

// ============================================================================
// DiagnosticManager extension
// ============================================================================

impl DiagnosticManager {
    pub fn take(&mut self) -> Self {
        std::mem::replace(self, DiagnosticManager::new())
    }
}

// ============================================================================
// DashMap helpers: clone_and_clear / fill
// ============================================================================

/// Optimized version: clone data under read lock, then clear
/// This reduces write lock hold time
fn clone_and_clear<K, V>(cell: &MultiThreadRefCell<DashMap<K, V>>) -> DashMap<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    let guard = cell.borrow();
    let new_map = DashMap::with_capacity(guard.len());
    for entry in guard.iter() {
        new_map.insert(entry.key().clone(), entry.value().clone());
    }
    // Release read lock then quickly clear
    drop(guard);
    // Clear under write lock (fast operation)
    cell.borrow().clear();
    new_map
}

fn fill_dashmap<K, V>(cell: &MultiThreadRefCell<DashMap<K, V>>, source: DashMap<K, V>)
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    let guard = cell.borrow();
    for entry in source.iter() {
        guard.insert(entry.key().clone(), entry.value().clone());
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workspace() {
        let mgr = WorkspaceManager::new();
        assert_eq!(mgr.active_id(), "default");
        assert_eq!(mgr.active_kind(), WorkspaceKind::Project);
    }

    #[test]
    fn create_and_switch_workspace() {
        let mgr = WorkspaceManager::new();

        assert!(mgr.create_and_switch(
            "hbl".into(),
            WorkspaceKind::Project,
            PathBuf::from("/projects/hbl"),
        ));
        assert_eq!(mgr.active_id(), "hbl");
        assert_eq!(mgr.active_kind(), WorkspaceKind::Project);

        let list = mgr.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn switch_preserves_data() {
        let mgr = WorkspaceManager::new();

        mgr.mcodes
            .borrow()
            .insert("test.mc".to_string(), McCode::new_empty());
        assert_eq!(mgr.mcodes.borrow().len(), 1);

        mgr.create_and_switch("proj1".into(), WorkspaceKind::Project, PathBuf::from("."));
        assert_eq!(mgr.mcodes.borrow().len(), 0);

        assert!(mgr.switch_to("default"));
        assert_eq!(mgr.mcodes.borrow().len(), 1);
    }

    #[test]
    fn cannot_create_duplicate() {
        let mgr = WorkspaceManager::new();
        mgr.create_and_switch("proj1".into(), WorkspaceKind::Project, PathBuf::from("."));
        assert!(!mgr.create_and_switch("proj1".into(), WorkspaceKind::Project, PathBuf::from(".")));
    }

    #[test]
    fn remove_saved_workspace() {
        let mgr = WorkspaceManager::new();
        mgr.create_and_switch("proj1".into(), WorkspaceKind::Project, PathBuf::from("."));
        assert!(mgr.remove("default"));
        assert_eq!(mgr.list().len(), 1);
        assert!(!mgr.remove("proj1"));
    }
}
