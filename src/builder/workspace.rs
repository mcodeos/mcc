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
use std::sync::Arc;
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
