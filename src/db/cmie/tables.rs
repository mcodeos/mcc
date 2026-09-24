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
//!
//! A workspace is identified by its **root path** (canonicalized — see [`world_key`]).
//! It is deliberately not identified by anything derived from the directory *name*: two
//! unrelated projects routinely share a basename (`~/a/hbl` and `~/b/hbl`, a real board
//! and a frozen test copy of it), and a name-keyed identity silently merges their
//! definition tables.

use crate::db::diagnostic::diagnostic::DiagnosticManager;
use crate::db::infra::mc_code::McCode;
use crate::semantic::capability::McCapability;
use crate::semantic::component::McComponent;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::{McSpaceName, McURI};
use dashmap::DashMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use tracing::{debug, info};

// WorkspaceKind

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceKind {
    Project,
}

impl Default for WorkspaceKind {
    fn default() -> Self {
        Self::Project
    }
}

// WorkspaceMeta -- per-workspace metadata

/// Per-workspace metadata. A workspace **is** its root: `root` is the identity,
/// not a label. `None` is the anonymous world — no project is open, so the
/// tables hold only standalone files and loaded libraries.
#[derive(Debug, Clone)]
pub struct WorkspaceMeta {
    pub root: Option<PathBuf>,
    pub kind: WorkspaceKind,
}

impl Default for WorkspaceMeta {
    fn default() -> Self {
        Self {
            root: None,
            kind: WorkspaceKind::Project,
        }
    }
}

/// The identity of a world: its root path, canonicalized so that two spellings
/// of one directory (`./x`, a symlink, a trailing slash) name one world. A path
/// that does not resolve — a root planned but not yet created — falls back to
/// its literal form. Equality is always over the whole path, so two different
/// projects can never collide on a shared basename.
fn world_key(root: &Option<PathBuf>) -> Option<PathBuf> {
    root.as_ref()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
}

// Visibility entry types (plan 9.02 §5 T11 / §12.2 visibility materialization)

/// Priority layer of a visibility-table winner. The table stores one winner
/// per `(file, symbol)`; the layer states which priority produced it — own-file
/// declarations (P3, `parse_cmie_names` ran last and displaced the import) or
/// the merged use chain (P4, imports + cascade). P5 is deliberately absent:
/// system defs are not per-file rows — an entry being present at all is what
/// shadows the system library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisLayer {
    /// The winner is the file's own declaration (P3).
    OwnFile,
    /// The winner came in through a `use` (default/named import, alias, or
    /// cascade) (P4).
    UseImport,
}

/// One visible candidate: the definition identity plus the layer that put it
/// there. The `DefId` is not stored — at `sync_visibility` time (LSP edit
/// path) the target def may not be registered yet, so the id is resolved at
/// read time through the registry, where the canonical key revives under the
/// same `DefId`.
#[derive(Debug, Clone)]
pub struct VisCandidate {
    pub name: McSpaceName,
    pub layer: VisLayer,
}

/// One materialized visibility row: the winner plus the candidates it
/// displaced. A displaced candidate is recorded at the displacing insert
/// (own-file declaration over an import, later import over an earlier one), so
/// the table — not just the derivation transcript — carries the alias/shadow
/// relations into the table (plan §5 T11).
#[derive(Debug, Clone)]
pub struct VisEntry {
    pub winner: VisCandidate,
    pub shadowed: Vec<McSpaceName>,
}

// WorkspaceSnapshot -- for save/restore when switching workspaces

struct WorkspaceSnapshot {
    meta: WorkspaceMeta,
    mcodes: DashMap<McURI, McCode>,
    modules: DashMap<McSpaceName, Arc<McModule>>,
    components: DashMap<McSpaceName, Arc<McComponent>>,
    interfaces: DashMap<McSpaceName, Arc<McInterface>>,
    enums: DashMap<McSpaceName, Arc<McEnumDef>>,
    capabilities: DashMap<McSpaceName, Arc<McCapability>>,
    diagnostics: DiagnosticManager,
    // §12.1 DefinitionSpace manifest (loaded source domains + lib boundary).
    sources: DashMap<McURI, crate::db::defspace::SourceDomain>,
    libs: DashMap<String, crate::db::defspace::LibBoundary>,
    // Phase 5: per-world library state — the loaded lib source cache
    // (mcc_blibs) and the registry's system-library def segment both follow
    // the world they were loaded into.
    blibs: DashMap<String, McCode>,
    system_defs: Vec<crate::db::defregistry::SystemDefSnapshot>,
    // Phase 6 (§13 delta 2) / T11 (plan 9.02 §5): per-world visibility
    // index — (from_file, symbol) → the materialized visible candidate
    // (winner identity + priority layer + displaced candidates). Derived from
    // each file's spacenames (uselist + as_id + impt_ids) at parse_nsp time.
    visibility: DashMap<(McURI, String), VisEntry>,
    // Phase 8 (D14): per-world def resolution edges (out + rev dependents).
    refgraph: crate::db::refgraph::DefRefGraph,
}

// WorkspaceManager (singleton)

pub struct WorkspaceManager {
    pub(crate) mcodes: DashMap<McURI, McCode>,
    pub(crate) modules: DashMap<McSpaceName, Arc<McModule>>,
    pub(crate) components: DashMap<McSpaceName, Arc<McComponent>>,
    pub(crate) interfaces: DashMap<McSpaceName, Arc<McInterface>>,
    pub(crate) enums: DashMap<McSpaceName, Arc<McEnumDef>>,
    pub(crate) capabilities: DashMap<McSpaceName, Arc<McCapability>>,
    pub(crate) diagnostics: Mutex<DiagnosticManager>,

    /// The active world. Its `root` is the identity — see [`world_key`].
    meta: Mutex<WorkspaceMeta>,

    /// Parked worlds, keyed by the same canonical root that identifies the
    /// active one (`None` = the anonymous world).
    saved: Mutex<HashMap<Option<PathBuf>, WorkspaceSnapshot>>,

    // LSP tables -- extracted to db/symbol/workspace.rs
    pub(crate) lsp: crate::db::symbol::workspace::LspTables,

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
    // Phase 6 (§13 delta 2) / T11 (plan 9.02 §5): per-world visibility
    // index — (from_file, symbol) → the materialized visible candidate.
    // resolve_class reads it for pre-consolidation P3/P4 hits (table first,
    // own-file/chain walk demoted to fallback); the snapshot/restore and
    // clear paths move it with the world.
    pub(crate) visibility: DashMap<(McURI, String), VisEntry>,
    // Phase 8 (D14): per-world def resolution edges (out + rev dependents),
    // recorded at the single resolution bridge.
    pub(crate) refgraph: crate::db::refgraph::DefRefGraph,
    // Phase 5 (T3, defspace-id-core-plan): this world's definition registry —
    // the whole definition identity layer (id counter, key→id index, entry
    // arena, system name index, member ledgers, checkpoint journal). The free
    // defregistry API serves the active (process-global) world's registry;
    // every instance owns its own state, so world create / switch / unload
    // drive the lifecycle (snapshot / tombstone / restore) on the instance
    // that owns the world. World-scoped reads and writes reach it through
    // [`WorkspaceManager::registry`].
    registry: crate::db::defregistry::RegistryState,
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
            capabilities: DashMap::new(),
            diagnostics: Mutex::new(DiagnosticManager::new()),
            meta: Mutex::new(WorkspaceMeta::default()),
            saved: Mutex::new(HashMap::new()),
            lsp: crate::db::symbol::workspace::LspTables::new(),
            sources: DashMap::new(),
            libs: DashMap::new(),
            blibs: DashMap::new(),
            visibility: DashMap::new(),
            refgraph: crate::db::refgraph::DefRefGraph::new(),
            registry: crate::db::defregistry::RegistryState::default(),
        }
    }

    /// This world's definition registry (T3). World-scoped reads and the
    /// world-scoped write entry ([`RegistryState::insert`]) go through here;
    /// the process-global free defregistry API serves the active world.
    pub(crate) fn registry(&self) -> &crate::db::defregistry::RegistryState {
        &self.registry
    }

    // Query

    #[allow(dead_code)]
    pub fn active_kind(&self) -> WorkspaceKind {
        self.meta.lock().unwrap().kind.clone()
    }

    /// The active world's root — its identity, in canonical form. `None` is the
    /// anonymous world: no project is open.
    pub fn active_root(&self) -> Option<PathBuf> {
        self.meta.lock().unwrap().root.clone()
    }

    pub fn active_meta(&self) -> WorkspaceMeta {
        self.meta.lock().unwrap().clone()
    }

    /// Look up a component by its class name (ident string).
    /// Checks the registry's project (workspace) view first, then falls back
    /// to the per-world system-library name index (Phase 5) — the two
    /// segments the physical workspace table + system index mirrored before
    /// the read-side migration. The physical-table scan used to return an
    /// arbitrary same-name winner; the sorted registry enumeration is
    /// deterministic.
    /// Returns `None` if no component with that class name is registered.
    pub fn component_by_class(&self, class_name: &str) -> Option<Arc<McComponent>> {
        for (sn, comp) in crate::definition_space().workspace_components() {
            if sn.ident.to_string() == class_name {
                return Some(comp);
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

    /// Every world this manager holds — the active one first, then the parked
    /// ones. Addressed by root, the same key `switch_to` / `remove` take.
    pub fn list(&self) -> Vec<(Option<PathBuf>, WorkspaceKind)> {
        let mut result = Vec::new();
        {
            let m = self.meta.lock().unwrap();
            result.push((m.root.clone(), m.kind.clone()));
        }
        for entry in self.saved.lock().unwrap().iter() {
            result.push((entry.0.clone(), entry.1.meta.kind.clone()));
        }
        result
    }

    // Clear current active workspace

    pub fn clear_active(&self) {
        self.mcodes.clear();
        self.modules.clear();
        self.components.clear();
        self.interfaces.clear();
        self.enums.clear();
        self.capabilities.clear();
        self.lsp.class_table.lock().unwrap().clear();
        self.diagnostics.lock().unwrap().clear();
        self.sources.clear();
        self.libs.clear();
        self.blibs.clear();
        self.visibility.clear();
        self.refgraph.clear();
        // This world's own definition registry drops every identity — project
        // and loaded system libs alike — as tombstones (Phase 5 makes the
        // libs per-world): a later re-load revives them under the same DefId,
        // and the "deleted vs never existed" distinction survives (design §9
        // Phase B). Each instance tombstones its own registry, so isolated
        // test workspaces never touch the process-global one.
        self.registry.mark_all_tombstones();
        // T6-②: world clear / switch-away round end — the tombstone sweep
        // changed the definition space; stamp it so the drop is diffable.
        self.registry.checkpoint_if_changed();
    }

    // Switch world (create it if it is new)

    /// Make `root` the active world, creating it if it is new.
    ///
    /// Opening and creating are one operation because they are one question —
    /// "is this the world I am already in?" — and this is the only place that
    /// answers it. Callers that keep their own notion of "same workspace" will
    /// drift from this one; they must not. [`reset_to`](Self::reset_to) is the
    /// other transition and asks nothing: it is for worlds that are throwaway.
    ///
    /// The world being left is parked, so a caller can come back to it.
    ///
    /// Returns whether the active world actually changed: a repeat call for the
    /// root that is already active is a no-op.
    ///
    /// The root is stored in its canonical form, so the active world's root is
    /// always the same value that keys it in `saved` — one spelling per
    /// directory, whichever one the caller happened to pass in.
    pub fn switch_to(&self, root: Option<PathBuf>, kind: WorkspaceKind) -> bool {
        let key = world_key(&root);
        if self.active_root() == key {
            return false;
        }

        self.snapshot_active();

        let parked = self.saved.lock().unwrap().remove(&key);
        match parked {
            Some(snapshot) => self.restore_snapshot(snapshot),
            None => {
                self.clear_active();
                *self.meta.lock().unwrap() = WorkspaceMeta { root: key, kind };
            }
        }

        info!(target: "mcc::workspace", root = ?self.active_root(), "switched to workspace");
        true
    }

    // Reset world (drop the outgoing one instead of parking it)

    /// Make `root` the active world **without** parking the outgoing one, and
    /// evict whatever world is parked under `root`.
    ///
    /// [`switch_to`](Self::switch_to) keeps what it leaves so a caller can come
    /// back; a directory batch never comes back. It visits one throwaway world
    /// per entry — a folder of unrelated `.mc` files is a container of
    /// definition spaces, not one — and has already taken everything it needs
    /// from each world before moving on, so parking them would keep a whole
    /// directory's projects alive for a reader that will never arrive.
    ///
    /// Evicting `root`'s parked world keeps the "one key = at most one world"
    /// invariant that `switch_to` maintains: a snapshot taken before the batch
    /// must not outlive the world this call builds under the same key.
    ///
    /// Always clears, even when `root` is already active — "am I already here?"
    /// is exactly the question a batch does not ask.
    pub fn reset_to(&self, root: Option<PathBuf>, kind: WorkspaceKind) {
        let key = world_key(&root);
        self.saved.lock().unwrap().remove(&key);
        self.clear_active();
        *self.meta.lock().unwrap() = WorkspaceMeta { root: key, kind };
        info!(target: "mcc::workspace", root = ?self.active_root(), "reset to workspace");
    }

    // Remove world

    pub fn remove(&self, root: &Option<PathBuf>) -> bool {
        let key = world_key(root);
        if self.active_root() == key {
            return false;
        }
        self.saved.lock().unwrap().remove(&key).is_some()
    }

    // Internal: snapshot / restore

    fn snapshot_active(&self) {
        let meta = self.meta.lock().unwrap().clone();
        let key = world_key(&meta.root);

        let mcodes = clone_and_clear(&self.mcodes);
        let modules = clone_and_clear(&self.modules);
        let components = clone_and_clear(&self.components);
        let interfaces = clone_and_clear(&self.interfaces);
        let enums = clone_and_clear(&self.enums);
        let capabilities = clone_and_clear(&self.capabilities);
        let sources = clone_and_clear(&self.sources);
        let libs = clone_and_clear(&self.libs);
        let blibs = clone_and_clear(&self.blibs);
        let visibility = clone_and_clear(&self.visibility);
        let refgraph = self.refgraph.clone();
        self.refgraph.clear();
        // Phase 5: the registry's system-library segment follows the world.
        // Captured from this world's own registry before it is tombstoned by
        // `clear_active`.
        let system_defs = self.registry.snapshot_system();
        let diagnostics = self.diagnostics.lock().unwrap().take();

        let snap = WorkspaceSnapshot {
            meta,
            mcodes,
            modules,
            components,
            interfaces,
            enums,
            capabilities,
            diagnostics,
            sources,
            libs,
            blibs,
            system_defs,
            visibility,
            refgraph,
        };

        debug!(target: "mcc::workspace", root = ?key, "snapshot saved");
        self.saved.lock().unwrap().insert(key, snap);
    }

    fn restore_snapshot(&self, snap: WorkspaceSnapshot) {
        self.clear_active();

        *self.meta.lock().unwrap() = snap.meta;

        fill_dashmap(&self.mcodes, snap.mcodes);
        fill_dashmap(&self.modules, snap.modules);
        fill_dashmap(&self.components, snap.components);
        fill_dashmap(&self.interfaces, snap.interfaces);
        fill_dashmap(&self.enums, snap.enums);
        fill_dashmap(&self.capabilities, snap.capabilities);
        fill_dashmap(&self.sources, snap.sources);
        fill_dashmap(&self.libs, snap.libs);
        fill_dashmap(&self.blibs, snap.blibs);
        fill_dashmap(&self.visibility, snap.visibility);
        // Rebuild the restored world's ref graph from its out edges
        // (record() reconstructs the rev side), and the use-line face from
        // its pairs (record_use_line reconstructs use_rev).
        let refgraph = snap.refgraph;
        self.refgraph.clear();
        for (from, tos) in refgraph.out_pairs() {
            for to in tos {
                self.refgraph.record(&from, &to);
            }
        }
        for (from, tos) in refgraph.use_pairs() {
            for to in tos {
                self.refgraph.record_use_line(&from, &to);
            }
        }

        *self.diagnostics.lock().unwrap() = snap.diagnostics;

        // Re-register the restored definitions in this world's own registry
        // (project domain, no physical write — the tables above are already
        // filled). Tombstoned identities revive under their original DefId; a
        // system def shadowed by this workspace is taken over
        // (workspace-first).
        self.registry.restore_workspace(
            &self.components,
            &self.modules,
            &self.interfaces,
            &self.enums,
            &self.capabilities,
        );
        // Phase 5: restore the world's system-library segment alongside its
        // project defs.
        self.registry.restore_system(snap.system_defs);
        // T6-②: world-restore round end — the restored defs revived in this
        // world's registry; stamp one version capturing the switched-to world.
        self.registry.checkpoint_if_changed();
    }
}

// lazy_static singleton

pub(crate) static WORKSPACE: LazyLock<WorkspaceManager> = LazyLock::new(WorkspaceManager::new);

// DiagnosticManager extension

impl DiagnosticManager {
    pub fn take(&mut self) -> Self {
        std::mem::replace(self, DiagnosticManager::new())
    }
}

// DashMap helpers: clone_and_clear / fill

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

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    /// A root that does not exist on disk, so `world_key` keeps it verbatim and
    /// the test does not depend on the machine's filesystem.
    fn root(path: &str) -> Option<PathBuf> {
        Some(PathBuf::from(path))
    }

    #[test]
    fn def_cmie__default_workspace() {
        let mgr = WorkspaceManager::new();
        assert_eq!(mgr.active_root(), None);
        assert_eq!(mgr.active_kind(), WorkspaceKind::Project);
        assert_eq!(mgr.list().len(), 1);
    }

    #[test]
    fn def_cmie__switch_to_new_workspace() {
        let mgr = WorkspaceManager::new();

        assert!(mgr.switch_to(root("/projects/hbl"), WorkspaceKind::Project));
        assert_eq!(mgr.active_root(), root("/projects/hbl"));
        assert_eq!(mgr.active_kind(), WorkspaceKind::Project);

        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&(root("/projects/hbl"), WorkspaceKind::Project)));
    }

    #[test]
    fn def_cmie__switch_preserves_data() {
        let mgr = WorkspaceManager::new();

        mgr.mcodes
            .insert("test.mc".to_string(), McCode::new_empty());
        assert_eq!(mgr.mcodes.len(), 1);

        mgr.switch_to(root("/projects/proj1"), WorkspaceKind::Project);
        assert_eq!(mgr.mcodes.len(), 0);

        assert!(mgr.switch_to(None, WorkspaceKind::Project));
        assert_eq!(mgr.mcodes.len(), 1);
    }

    /// A repeat call for the root already active is a no-op — including the
    /// anonymous world, which is a root like any other (`None`).
    #[test]
    fn def_cmie__same_root_is_a_noop() {
        let mgr = WorkspaceManager::new();
        assert!(!mgr.switch_to(None, WorkspaceKind::Project));

        assert!(mgr.switch_to(root("/projects/proj1"), WorkspaceKind::Project));
        assert!(!mgr.switch_to(root("/projects/proj1"), WorkspaceKind::Project));
    }

    /// The reported bug, reduced: two roots that share a basename must be two
    /// worlds. Keying either the active world or its snapshot by the directory
    /// *name* merged them, so `proj-b`'s definitions landed in `proj-a`'s tables.
    #[test]
    fn def_cmie__same_basename_different_root_is_a_different_world() {
        let mgr = WorkspaceManager::new();

        let a = root("/projects/a/hbl");
        let b = root("/projects/b/hbl");

        mgr.switch_to(a.clone(), WorkspaceKind::Project);
        mgr.mcodes.insert("a.mc".to_string(), McCode::new_empty());

        assert!(mgr.switch_to(b.clone(), WorkspaceKind::Project));
        assert_eq!(mgr.mcodes.len(), 0, "b inherited a's definitions");

        assert!(mgr.switch_to(a.clone(), WorkspaceKind::Project));
        assert_eq!(mgr.mcodes.len(), 1, "a's snapshot was overwritten by b");

        // Two worlds parked under two distinct keys, not one.
        assert_eq!(mgr.list().len(), 3);
    }

    #[test]
    fn def_cmie__remove_saved_workspace() {
        let mgr = WorkspaceManager::new();
        mgr.switch_to(root("/projects/proj1"), WorkspaceKind::Project);
        assert!(mgr.remove(&None));
        assert_eq!(mgr.list().len(), 1);
        // The active world cannot be removed out from under itself.
        assert!(!mgr.remove(&root("/projects/proj1")));
    }

    /// A reset is the transition for throwaway worlds: what it leaves must be
    /// dropped, not parked. Parking would be worse than a leak here — the
    /// batch's outgoing world is empty (`clear_active` on entry), so parking it
    /// would overwrite a legitimate snapshot sitting under the same key.
    #[test]
    fn def_cmie__reset_drops_the_world_it_leaves() {
        let mgr = WorkspaceManager::new();
        mgr.mcodes.insert("a.mc".to_string(), McCode::new_empty());

        mgr.reset_to(root("/projects/proj1"), WorkspaceKind::Project);
        assert_eq!(mgr.active_root(), root("/projects/proj1"));
        assert_eq!(mgr.mcodes.len(), 0, "reset inherited the outgoing world");
        // Only the active world is listed: nothing was parked.
        assert_eq!(mgr.list().len(), 1);

        // And moving back does not resurrect what the reset dropped.
        assert!(mgr.switch_to(None, WorkspaceKind::Project));
        assert_eq!(mgr.mcodes.len(), 0);
    }

    /// One key = at most one world. A snapshot parked before the batch must not
    /// outlive the world a reset builds under the same key.
    #[test]
    fn def_cmie__reset_evicts_the_world_parked_under_its_own_root() {
        let mgr = WorkspaceManager::new();
        mgr.switch_to(root("/projects/proj1"), WorkspaceKind::Project);
        mgr.mcodes.insert("old.mc".to_string(), McCode::new_empty());
        mgr.switch_to(None, WorkspaceKind::Project);
        assert_eq!(mgr.list().len(), 2, "proj1 is parked");

        mgr.reset_to(root("/projects/proj1"), WorkspaceKind::Project);
        assert_eq!(mgr.mcodes.len(), 0, "reset inherited the parked world");
        // proj1's snapshot is evicted, and nothing replaced it: the anonymous
        // world is active (restored above), not parked.
        assert_eq!(mgr.list().len(), 1);

        // The pre-reset snapshot is gone: coming back finds an empty world.
        mgr.switch_to(None, WorkspaceKind::Project);
        assert!(mgr.switch_to(root("/projects/proj1"), WorkspaceKind::Project));
        assert_eq!(mgr.mcodes.len(), 0, "the pre-reset snapshot came back");
    }

    /// U235 write-path world attribution: a def inserted through the
    /// world-scoped [`WorkspaceManager::insert_def`] lives in the world that
    /// was active at write time — the outgoing switch tombstones it (its
    /// world is parked), a foreign world cannot see it, and the restore path
    /// revives it under the SAME [`db::defregistry::DefId`] (D11 identity
    /// stability across the snapshot transport).
    #[test]
    fn def_registry__write_entries_follow_the_active_world_across_a_switch() {
        use crate::db::defregistry::{DefKind, DefValue, InsertOutcome, LoadDomain};
        use crate::semantic::mc_enum::{McEnumDef, McEnumValue};

        let mgr = WorkspaceManager::new();
        let a = root("/projects/u235/owned");
        let b = root("/projects/u235/foreign");
        let sn = crate::McSpaceName {
            ident: crate::McIds::from("U235_SWITCH"),
            uri: crate::semantic::common::uri_intern("/projects/u235/owned/src.mc"),
        };
        let def = DefValue::Enum(std::sync::Arc::new(McEnumDef {
            name: sn.ident.clone(),
            span: [0, 3],
            values: vec![McEnumValue {
                name: crate::McIds::from("A"),
                span: [0, 3],
            }],
            uri: sn.uri.to_string(),
        }));

        mgr.switch_to(a.clone(), WorkspaceKind::Project);
        assert_eq!(
            mgr.insert_def(&sn, LoadDomain::Project, def),
            InsertOutcome::Inserted
        );
        let id = mgr
            .registry()
            .def_id(&sn, DefKind::Enum)
            .expect("the def is live in the world it was written to");

        // A foreign world must not see (or inherit) the def.
        mgr.switch_to(b.clone(), WorkspaceKind::Project);
        assert!(
            mgr.registry().def_id(&sn, DefKind::Enum).is_none(),
            "the def did not leak into the world switched to"
        );

        // Switching back restores the parked world: the identity revives in
        // place — same DefId, same physical row.
        assert!(mgr.switch_to(a, WorkspaceKind::Project));
        assert_eq!(
            mgr.registry().def_id(&sn, DefKind::Enum),
            Some(id),
            "the restore path revives the def under its original id"
        );
        assert!(
            mgr.enums.contains_key(&sn),
            "the physical mirror rides the same snapshot transport"
        );
    }
}
