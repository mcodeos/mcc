// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Workspace abstraction layer -- PR-3D core.
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

use crate::db::diagnostic::diagnostic::DiagnosticManager;
use crate::db::infra::mc_code::McCode;
use crate::semantic::component::McComponent;
use crate::semantic::mc_define::McDefineDef;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::{McSpaceName, McURI};
use dashmap::DashMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
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
// WorkspaceMeta -- per-workspace metadata
// ============================================================================

#[derive(Debug, Clone)]
pub struct WorkspaceMeta {
    pub id: String,
    pub kind: WorkspaceKind,
    pub root: PathBuf,
}

impl Default for WorkspaceMeta {
    fn default() -> Self {
        Self {
            id: "default".into(),
            kind: WorkspaceKind::Project,
            root: PathBuf::from("."),
        }
    }
}

// ============================================================================
// WorkspaceSnapshot -- for save/restore when switching workspaces
// ============================================================================

struct WorkspaceSnapshot {
    meta: WorkspaceMeta,
    mcodes: DashMap<McURI, McCode>,
    modules: DashMap<McSpaceName, Arc<McModule>>,
    components: DashMap<McSpaceName, Arc<McComponent>>,
    interfaces: DashMap<McSpaceName, Arc<McInterface>>,
    enums: DashMap<McSpaceName, Arc<McEnumDef>>,
    defines: DashMap<McSpaceName, Arc<McDefineDef>>,
    diagnostics: DiagnosticManager,
    // §12.1 DefinitionSpace manifest (loaded source domains + lib boundary).
    sources: DashMap<McURI, crate::db::defspace::SourceDomain>,
    libs: DashMap<String, crate::db::defspace::LibBoundary>,
    // Phase 5: per-world library state — the loaded lib source cache
    // (mcc_blibs) and the registry's system-library def segment both follow
    // the world they were loaded into.
    blibs: DashMap<String, McCode>,
    system_defs: Vec<crate::db::defregistry::SystemDefSnapshot>,
}

// ============================================================================
// WorkspaceManager (singleton)
// ============================================================================

pub struct WorkspaceManager {
    pub(crate) mcodes: DashMap<McURI, McCode>,
    pub(crate) modules: DashMap<McSpaceName, Arc<McModule>>,
    pub(crate) components: DashMap<McSpaceName, Arc<McComponent>>,
    pub(crate) interfaces: DashMap<McSpaceName, Arc<McInterface>>,
    pub(crate) enums: DashMap<McSpaceName, Arc<McEnumDef>>,
    pub(crate) defines: DashMap<McSpaceName, Arc<McDefineDef>>,
    pub(crate) diagnostics: Mutex<DiagnosticManager>,

    meta: Mutex<WorkspaceMeta>,

    saved: Mutex<HashMap<String, WorkspaceSnapshot>>,

    // LSP tables -- extracted to db/symbol/workspace.rs
    pub(crate) lsp: crate::db::symbol::workspace::LspTables,

    /// ★ §7.6: Reverse dependency index — "who uses me".
    /// When file B's CMIE defs change, iterate `reverse_deps[B]` to find
    /// affected files whose Use table needs rebuilding.
    pub(crate) reverse_deps: DashMap<McURI, Vec<McURI>>,

    // §12.1 DefinitionSpace manifest: which sources are loaded and into which
    // domain (project vs system lib), plus the loaded library boundary.
    // Maintained by the loader chain (loader.rs / libmgr.rs); read through the
    // `DefinitionSpace` view — "what is loaded, into which domain, where the
    // boundary is" (design §12.1).
    pub(crate) sources: DashMap<McURI, crate::db::defspace::SourceDomain>,
    pub(crate) libs: DashMap<String, crate::db::defspace::LibBoundary>,

    // Phase 5: per-world system-library source cache (moved from the
    // process-global `libmgr::mcc_blibs`). Each world owns the libraries it
    // loaded, so a switch can never leak a stale lib into another world.
    pub(crate) blibs: DashMap<String, crate::db::infra::mc_code::McCode>,
}

impl WorkspaceManager {
    /// Build a fresh, empty workspace. Tests use this to construct an isolated
    /// [`DefinitionSpace`](crate::db::defspace::DefinitionSpace) without touching
    /// the process-global `WORKSPACE`.
    pub(crate) fn new() -> Self {
        Self {
            mcodes: DashMap::new(),
            modules: DashMap::new(),
            components: DashMap::new(),
            interfaces: DashMap::new(),
            enums: DashMap::new(),
            defines: DashMap::new(),
            diagnostics: Mutex::new(DiagnosticManager::new()),
            meta: Mutex::new(WorkspaceMeta::default()),
            saved: Mutex::new(HashMap::new()),
            lsp: crate::db::symbol::workspace::LspTables::new(),
            reverse_deps: DashMap::new(),
            sources: DashMap::new(),
            libs: DashMap::new(),
            blibs: DashMap::new(),
        }
    }

    // ================================================================
    // Query
    // ================================================================

    /// Currently-active workspace id / kind — used by tests.
    #[allow(dead_code)]
    pub fn active_id(&self) -> String {
        self.meta.lock().unwrap().id.clone()
    }

    #[allow(dead_code)]
    pub fn active_kind(&self) -> WorkspaceKind {
        self.meta.lock().unwrap().kind.clone()
    }

    pub fn active_root(&self) -> PathBuf {
        self.meta.lock().unwrap().root.clone()
    }

    pub fn active_meta(&self) -> WorkspaceMeta {
        self.meta.lock().unwrap().clone()
    }

    /// Look up a component by its class name (ident string).
    /// Checks workspace project tables first, then falls back to the per-world
    /// system-library registry segment (Phase 5).
    /// Returns `None` if no component with that class name is registered.
    pub fn component_by_class(&self, class_name: &str) -> Option<Arc<McComponent>> {
        for entry in self.components.iter() {
            if entry.key().ident.to_string() == class_name {
                return Some(entry.value().clone());
            }
        }
        // Fallback: per-world system-library components (registry segment) —
        // O(1) name-index candidates, then the component kind.
        for hit in crate::db::defregistry::system_name_hits(class_name) {
            if hit.kind != crate::db::defregistry::DefKind::Component {
                continue;
            }
            if let Some((_, def)) = crate::db::defregistry::live_entry_by_id(hit.id) {
                if let crate::db::defregistry::DefValue::Component(c) = def {
                    return Some(c);
                }
            }
        }
        None
    }

    pub fn list(&self) -> Vec<(String, WorkspaceKind)> {
        let mut result = Vec::new();
        {
            let m = self.meta.lock().unwrap();
            result.push((m.id.clone(), m.kind.clone()));
        }
        for entry in self.saved.lock().unwrap().iter() {
            result.push((entry.0.clone(), entry.1.meta.kind.clone()));
        }
        result
    }

    // ================================================================
    // Clear current active workspace
    // ================================================================

    /// Is this the process-global active workspace? The definition registry is
    /// process-global state tied to the single active workspace, so the
    /// lifecycle sync (tombstone project defs / restore a snapshot) runs only
    /// for the real [`WORKSPACE`] — tests that construct isolated
    /// [`WorkspaceManager`]s must not touch it.
    fn is_process_global(&self) -> bool {
        std::ptr::eq(self, &*WORKSPACE)
    }

    pub fn clear_active(&self) {
        self.mcodes.clear();
        self.modules.clear();
        self.components.clear();
        self.interfaces.clear();
        self.enums.clear();
        self.defines.clear();
        self.reverse_deps.clear();
        self.lsp.class_table.lock().unwrap().clear();
        self.diagnostics.lock().unwrap().clear();
        self.sources.clear();
        self.libs.clear();
        self.blibs.clear();
        // The definition registry drops every identity — project and loaded
        // system libs alike — as tombstones (Phase 5 makes the libs
        // per-world): a later re-load revives them under the same DefId, and
        // the "deleted vs never existed" distinction survives (design §9
        // Phase B).
        if self.is_process_global() {
            crate::db::defregistry::mark_all_tombstones();
        }
    }

    // ================================================================
    // Create workspace
    // ================================================================

    pub fn create_and_switch(&self, id: String, kind: WorkspaceKind, root: PathBuf) -> bool {
        if self.meta.lock().unwrap().id == id {
            return false;
        }
        if self.saved.lock().unwrap().contains_key(&id) {
            return false;
        }

        self.snapshot_active();

        self.clear_active();
        *self.meta.lock().unwrap() = WorkspaceMeta {
            id: id.clone(),
            kind,
            root,
        };

        info!(target: "mcc::workspace", id = %id, "created and switched to new workspace");
        true
    }

    // ================================================================
    // Switch workspace
    // ================================================================
    // Auto-set project path when switching projects
    pub fn switch_to(&self, id: &str) -> bool {
        if self.meta.lock().unwrap().id == id {
            return false;
        }

        let snapshot = match self.saved.lock().unwrap().remove(id) {
            Some(s) => s,
            None => return false,
        };

        self.snapshot_active();

        self.restore_snapshot(snapshot);

        info!(target: "mcc::workspace", id = %id, "switched to workspace");
        true
    }

    // ================================================================
    // Remove workspace
    // ================================================================

    pub fn remove(&self, id: &str) -> bool {
        if self.meta.lock().unwrap().id == id {
            return false;
        }
        self.saved.lock().unwrap().remove(id).is_some()
    }

    // ================================================================
    // Internal: snapshot / restore
    // ================================================================

    fn snapshot_active(&self) {
        let meta = self.meta.lock().unwrap().clone();
        let id = meta.id.clone();

        let mcodes = clone_and_clear(&self.mcodes);
        let modules = clone_and_clear(&self.modules);
        let components = clone_and_clear(&self.components);
        let interfaces = clone_and_clear(&self.interfaces);
        let enums = clone_and_clear(&self.enums);
        let defines = clone_and_clear(&self.defines);
        let sources = clone_and_clear(&self.sources);
        let libs = clone_and_clear(&self.libs);
        let blibs = clone_and_clear(&self.blibs);
        // Phase 5: the registry's system-library segment follows the world.
        // Captured before the registry is tombstoned by `clear_active`.
        let system_defs = if self.is_process_global() {
            crate::db::defregistry::snapshot_system()
        } else {
            Vec::new()
        };
        let diagnostics = self.diagnostics.lock().unwrap().take();

        let snap = WorkspaceSnapshot {
            meta,
            mcodes,
            modules,
            components,
            interfaces,
            enums,
            defines,
            diagnostics,
            sources,
            libs,
            blibs,
            system_defs,
        };

        debug!(target: "mcc::workspace", id = %id, "snapshot saved");
        self.saved.lock().unwrap().insert(id, snap);
    }

    fn restore_snapshot(&self, snap: WorkspaceSnapshot) {
        self.clear_active();

        *self.meta.lock().unwrap() = snap.meta;

        fill_dashmap(&self.mcodes, snap.mcodes);
        fill_dashmap(&self.modules, snap.modules);
        fill_dashmap(&self.components, snap.components);
        fill_dashmap(&self.interfaces, snap.interfaces);
        fill_dashmap(&self.enums, snap.enums);
        fill_dashmap(&self.defines, snap.defines);
        fill_dashmap(&self.sources, snap.sources);
        fill_dashmap(&self.libs, snap.libs);
        fill_dashmap(&self.blibs, snap.blibs);

        *self.diagnostics.lock().unwrap() = snap.diagnostics;

        // Re-register the restored definitions in the registry (project
        // domain, no physical write — the tables above are already filled).
        // Tombstoned identities revive under their original DefId; a system
        // def shadowed by this workspace is taken over (workspace-first).
        if self.is_process_global() {
            crate::db::defregistry::restore_workspace(
                &self.components,
                &self.modules,
                &self.interfaces,
                &self.enums,
                &self.defines,
            );
            // Phase 5: restore the world's system-library segment alongside
            // its project defs.
            crate::db::defregistry::restore_system(snap.system_defs);
        }
    }
}

// ============================================================================
// lazy_static singleton
// ============================================================================

pub(crate) static WORKSPACE: LazyLock<WorkspaceManager> = LazyLock::new(WorkspaceManager::new);

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

/// Transfer all entries from `map` into a new DashMap, leaving `map` empty.
///
/// Uses `remove` to transfer ownership of values, avoiding the Clone-based
/// shallow copy that would produce dangling AstNode pointers (Defect 5).
/// The `V: Clone` bound is retained for compatibility but is not actually used.
fn clone_and_clear<K, V>(map: &DashMap<K, V>) -> DashMap<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    let new_map = DashMap::with_capacity(map.len());
    // Collect keys first, then remove+insert to transfer ownership.
    // This avoids the Clone path for AstNode (owned=false copies that
    // dangle when the original owned=true is dropped by clear()).
    let keys: Vec<K> = map.iter().map(|e| e.key().clone()).collect();
    for key in keys {
        if let Some((_, v)) = map.remove(&key) {
            new_map.insert(key, v);
        }
    }
    new_map
}

fn fill_dashmap<K, V>(map: &DashMap<K, V>, source: DashMap<K, V>)
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    for entry in source.iter() {
        map.insert(entry.key().clone(), entry.value().clone());
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
            .insert("test.mc".to_string(), McCode::new_empty());
        assert_eq!(mgr.mcodes.len(), 1);

        mgr.create_and_switch("proj1".into(), WorkspaceKind::Project, PathBuf::from("."));
        assert_eq!(mgr.mcodes.len(), 0);

        assert!(mgr.switch_to("default"));
        assert_eq!(mgr.mcodes.len(), 1);
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
