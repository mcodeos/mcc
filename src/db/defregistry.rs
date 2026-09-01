// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Definition-layer write side + identity registry (design defspace §9 Phase B / §5 D11).
//!
//! [`insert`] / [`remove_by_uri`] / [`remove_by_uris`] are the single write
//! entry for the definition tables, and the registry below is the single
//! definition identity table: a persistent integer [`DefId`] per canonical
//! key `(uri, ident)` (the [`McSpaceName`]), append-only with tombstones.
//! Phase 3 keeps the physical workspace tables as a compatibility
//! materialization for the remaining direct readers (system-lib visibility
//! gates, the lib ledger, the workspace lifecycle) — the definition-space
//! read views (`defspace.rs`) now read this registry instead.
//!
//! D11 semantics (design §5 / §9 Phase B):
//! - Identity is stable across loads: re-parsing a file reuses the same
//!   [`DefId`] for the same `(uri, ident)` key.
//! - Removal is a tombstone: the key stays registered, the data drops
//!   (`data: None`), so "deleted" is distinguishable from "never existed"
//!   for the later checkpoint/diff work (Phase 9).
//! - [`LoadDomain`] tags where each def lives (`Project | SystemLib(name)`);
//!   the whole registry is per-world state (Phase 5) — system-lib defs live
//!   in the active world's registry and follow world create / switch / unload.
//!
//! Routing to the physical workspace tables (a compatibility materialization
//! for the direct workspace-table readers) is faithful to the pre-refactor
//! behavior:
//! - Module defs always land in the workspace module table — module parsing
//!   runs over `WORKSPACE.mcodes` regardless of the source domain.
//! - Component / Interface / Enum / Define defs land in the workspace table
//!   for `Project`. System-library defs are **not** mirrored into the
//!   process-global `global::mcc_*` tables anymore (Phase 5) — the registry
//!   is their only storage, so cross-world library state can never go stale.

use crate::db::cmie::tables as workspace;
use crate::semantic::component::McComponent;
use crate::semantic::mc_define::McDefineDef;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::{McCMIE, McSpaceName};
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// Compact persistent identity of one definition (design §5 D11). Process-local
/// while the single active workspace is the only world; per-world in the
/// target design (Phase 5).
pub type DefId = u32;

/// Which world a definition belongs to (design §4). Serialized inside a
/// checkpoint (Phase 9) so a diff can report a def changing worlds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LoadDomain {
    /// Project / user files.
    Project,
    /// A system library, named (`mcode` today).
    SystemLib(String),
}

/// The six definition kinds, one per AST top-level class template
/// (design §13.2) plus the function-template addressing entries
/// (design §12.1 / §13.6 delta 1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum DefKind {
    Component,
    Module,
    Interface,
    Enum,
    Define,
    Func,
}

/// A function template's addressing entry (design §12.1 / §13.6 delta 1).
///
/// The `McFunction` itself stays embedded in the host def's `funcs` table —
/// the AST parse form is unchanged — and this entry gives it a stable
/// [`DefId`] plus a host link so dispatch, goto-def and diff can address it.
/// Registered automatically by [`insert`] for every method / module func of a
/// component / module def, keyed by the qualified name `"HOST.NAME"` in the
/// host's file.
#[derive(Clone)]
pub struct FuncDef {
    /// [`DefId`] of the host component / module def.
    pub host: DefId,
    /// The function name within the host. Read by the func-addressing tests
    /// today; Phase 7's DefMemberId ledger and Phase 9's checkpoint/diff
    /// consume it alongside `host`.
    #[allow(dead_code)]
    pub name: String,
}

/// Tagged definition value: one [`insert`] writes any of the six kinds.
#[derive(Clone)]
pub enum DefValue {
    Component(Arc<McComponent>),
    Module(Arc<McModule>),
    Interface(Arc<McInterface>),
    Enum(Arc<McEnumDef>),
    Define(Arc<McDefineDef>),
    Func(FuncDef),
}

impl DefValue {
    fn kind(&self) -> DefKind {
        match self {
            DefValue::Component(_) => DefKind::Component,
            DefValue::Module(_) => DefKind::Module,
            DefValue::Interface(_) => DefKind::Interface,
            DefValue::Enum(_) => DefKind::Enum,
            DefValue::Define(_) => DefKind::Define,
            DefValue::Func(_) => DefKind::Func,
        }
    }
}

/// Outcome of an [`insert`]: the caller turns a duplicate into the matching
/// duplicate diagnostic at the declaration node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    Duplicate,
}

/// One registry entry: the identity (stable [`DefId`] + canonical key) plus
/// the live data or a tombstone. `data: None` marks a removed definition —
/// the key stays registered (append-only identity, D11).
pub struct DefEntry {
    /// The persistent identity of this entry (D11). The arena is keyed by
    /// this id and the checkpoint journal reads it per entry — identities
    /// stay comparable across loads.
    #[allow(dead_code)] // read by the Phase-9 checkpoint journal, wired in a later phase
    pub id: DefId,
    pub kind: DefKind,
    pub sn: McSpaceName,
    pub domain: LoadDomain,
    pub data: Option<DefValue>,
}

static NEXT_DEF_ID: AtomicU32 = AtomicU32::new(0);

/// Canonical key → its [`DefId`]s. Append-only: keys are never removed, so an
/// identity survives load/unload cycles ("deleted vs never existed" is
/// decidable). A key maps to a small vector because one `(uri, ident)` may
/// legally hold several kinds — a same-named component and interface in one
/// file coexist exactly as the per-kind physical tables allowed.
static KEY_TO_ID: LazyLock<DashMap<McSpaceName, Vec<DefId>>> = LazyLock::new(DashMap::new);

/// [`DefId`] → entry data. The arena holds the current data per identity;
/// data is re-materialized on re-parse while the identity stays stable
/// (D11: identity in the registry, data fresh in the arena).
static ARENA: LazyLock<DashMap<DefId, DefEntry>> = LazyLock::new(DashMap::new);

/// One live system-library identity registered under a display-form name in
/// the system name index (the P5 name-only lookup surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemNameHit {
    pub kind: DefKind,
    pub id: DefId,
}

/// Display-form name → live system-library identities. Kept exactly in sync
/// with the registry's live system segment by the mutation points (domain
/// transitions in [`register`], tombstones, world reset); gives the P5
/// name-only lookups (`resolve_system`, `find_in_table_scoped`, the enum
/// helpers, `component_by_class`) O(1) instead of a full registry scan on
/// every class reference. Function-template entries are host members, not
/// class names, so they stay out of the index.
static SYSTEM_NAME_INDEX: LazyLock<DashMap<String, Vec<SystemNameHit>>> =
    LazyLock::new(DashMap::new);

/// P5 name-only priority: component → module → interface → enum → define
/// (mirrors the `resolve_system` / `kind_of` ordering). Func entries are
/// host members and never enter the index, but the match stays exhaustive.
fn kind_priority(kind: DefKind) -> u8 {
    match kind {
        DefKind::Component => 0,
        DefKind::Module => 1,
        DefKind::Interface => 2,
        DefKind::Enum => 3,
        DefKind::Define => 4,
        DefKind::Func => 5,
    }
}

fn system_index_add(name: &str, kind: DefKind, id: DefId) {
    SYSTEM_NAME_INDEX
        .entry(name.to_string())
        .or_default()
        .push(SystemNameHit { kind, id });
}

fn system_index_remove(name: &str, kind: DefKind, id: DefId) {
    let mut drop_key = false;
    if let Some(mut hits) = SYSTEM_NAME_INDEX.get_mut(name) {
        hits.retain(|h| !(h.kind == kind && h.id == id));
        drop_key = hits.is_empty();
    }
    if drop_key {
        SYSTEM_NAME_INDEX.remove(name);
    }
}

/// Re-sync the system name index after a registry entry's live domain
/// changes. `was_live_system` is the entry's live-system state BEFORE the
/// mutation; `now_domain` is its domain AFTER.
fn sync_system_index(
    name: &str,
    kind: DefKind,
    id: DefId,
    was_live_system: bool,
    now_domain: &LoadDomain,
) {
    if kind == DefKind::Func {
        return;
    }
    let now_system = matches!(now_domain, LoadDomain::SystemLib(_));
    if !was_live_system && now_system {
        system_index_add(name, kind, id);
    } else if was_live_system && !now_system {
        system_index_remove(name, kind, id);
    }
}

/// Insert one definition. CMIE kinds treat an occupied live key as a
/// duplicate (the previous value stays); the module kind **overwrites** —
/// module parsing runs as a re-derive across parse rounds and replaces this
/// file's prior entry instead of firing a spurious DUP_MODULE (the file-local
/// duplicate check lives in `parse_pass1_modules`). A tombstoned key is
/// revived with the new data under the same [`DefId`] (D11).
///
/// The physical workspace/global tables are written in parallel as a
/// compatibility materialization: the remaining direct readers of the global
/// system tables (visibility gates, lib ledger) still read them, and the
/// workspace lifecycle (snapshot / switch / clear) still owns them.
pub fn insert(sn: &McSpaceName, domain: LoadDomain, def: DefValue) -> InsertOutcome {
    let kind = def.kind();
    let outcome = register(sn, kind, &domain, &def);
    if outcome == InsertOutcome::Inserted {
        // Function templates derive from the host's `funcs` table: register
        // the addressing entries (design §12.1) for a fresh host. Kept in
        // sync by `register_host_funcs` — stale entries are tombstoned first.
        if kind == DefKind::Component || kind == DefKind::Module {
            if let Some(host_id) = def_id(sn, kind) {
                register_host_funcs(sn, host_id, &def, &domain);
            }
        }
    }
    write_physical(sn, &domain, def);
    outcome
}

/// Register the identity + data in the registry (Phase 3 identity layer).
///
/// Precedence rules for an occupied live `(key, kind)`:
/// - A project def **overrides** a same-key system-lib def. The mcode lib
///   loads first, then a project file re-declares the identity; the workspace
///   def must shadow the system def (workspace-first, P0.1). The reverse
///   (system lib displacing a project def) is a duplicate, and so is any
///   same-domain re-insert (the caller turns it into the DUP diagnostic).
/// - A module re-derive always replaces this file's prior entry.
/// - A tombstone is revived with the new data under the same [`DefId`] (D11).
fn register(sn: &McSpaceName, kind: DefKind, domain: &LoadDomain, def: &DefValue) -> InsertOutcome {
    let mut ids = KEY_TO_ID.entry(sn.clone()).or_default();
    for &id in ids.iter() {
        let mut entry = ARENA.get_mut(&id).expect("arena holds every registered id");
        if entry.kind != kind {
            continue;
        }
        // Capture the live-system state before the mutation so the system
        // name index can follow the domain transition below.
        let was_live_system =
            entry.data.is_some() && matches!(entry.domain, LoadDomain::SystemLib(_));
        if kind == DefKind::Module {
            entry.data = Some(def.clone());
            entry.domain = domain.clone();
            sync_system_index(
                &sn.ident.to_string(),
                kind,
                id,
                was_live_system,
                &entry.domain,
            );
            return InsertOutcome::Inserted;
        }
        match &entry.data {
            None => {
                entry.data = Some(def.clone());
                entry.domain = domain.clone();
                sync_system_index(
                    &sn.ident.to_string(),
                    kind,
                    id,
                    was_live_system,
                    &entry.domain,
                );
                return InsertOutcome::Inserted;
            }
            Some(_) => {
                if was_live_system && matches!(domain, LoadDomain::Project) {
                    entry.data = Some(def.clone());
                    entry.domain = LoadDomain::Project;
                    sync_system_index(&sn.ident.to_string(), kind, id, true, &entry.domain);
                    return InsertOutcome::Inserted;
                }
                return InsertOutcome::Duplicate;
            }
        }
    }
    let id = NEXT_DEF_ID.fetch_add(1, Ordering::Relaxed);
    ids.push(id);
    ARENA.insert(
        id,
        DefEntry {
            id,
            kind,
            sn: sn.clone(),
            domain: domain.clone(),
            data: Some(def.clone()),
        },
    );
    if matches!(domain, LoadDomain::SystemLib(_)) && kind != DefKind::Func {
        system_index_add(&sn.ident.to_string(), kind, id);
    }
    InsertOutcome::Inserted
}

/// Compatibility write into the physical workspace tables. Phase 5 keeps the
/// system-library defs registry-only (see the module doc), so only project
/// defs land here — plus modules from any domain, because module parsing runs
/// over `WORKSPACE.mcodes` regardless of source domain (the module table is a
/// per-world table, so this is still world-local). A lib's "use-only" sweep
/// in `mcb_load_lib` tombstones its registry entries instead. Duplicates keep
/// the existing value (occupied entry); modules always overwrite (re-derive).
fn write_physical(sn: &McSpaceName, domain: &LoadDomain, def: DefValue) {
    match def {
        DefValue::Module(def) => {
            workspace::WORKSPACE.modules.insert(sn.clone(), def);
        }
        // System-library non-module defs are registry-only (Phase 5).
        non_module if matches!(domain, LoadDomain::SystemLib(_)) => {
            let _ = non_module;
        }
        DefValue::Component(def) => {
            insert_one(&workspace::WORKSPACE.components, sn.clone(), def);
        }
        DefValue::Interface(def) => {
            insert_one(&workspace::WORKSPACE.interfaces, sn.clone(), def);
        }
        DefValue::Enum(def) => {
            insert_one(&workspace::WORKSPACE.enums, sn.clone(), def);
        }
        DefValue::Define(def) => {
            insert_one(&workspace::WORKSPACE.defines, sn.clone(), def);
        }
        // Func entries are registry-only addressing metadata (design §12.1):
        // the host def holds the actual McFunction, so there is no physical
        // table to mirror.
        DefValue::Func(_) => {}
    }
}

/// Tombstone this host's previously-registered function entries, then
/// register the current ones from the host's `funcs` table — keeps the
/// addressing entries exactly in sync with the host across re-derive rounds
/// (modules) and reloads (components, whose uri-level tombstone already
/// cleared the funcs). Removed funcs stay tombstoned; survivors revive under
/// the same [`DefId`] (D11).
fn register_host_funcs(sn: &McSpaceName, host_id: DefId, host: &DefValue, domain: &LoadDomain) {
    let stale: Vec<DefId> = ARENA
        .iter()
        .filter_map(|e| match &e.data {
            Some(DefValue::Func(f)) if f.host == host_id => Some(*e.key()),
            _ => None,
        })
        .collect();
    for id in stale {
        if let Some(mut e) = ARENA.get_mut(&id) {
            e.data = None;
        }
    }
    let host_name = sn.ident.to_string();
    match host {
        DefValue::Component(comp) => {
            for f in comp.funcs.iter() {
                register_func_entry(sn, &host_name, &f.name.to_string(), host_id, domain);
            }
        }
        DefValue::Module(module) => {
            for f in module.funcs.iter() {
                register_func_entry(sn, &host_name, &f.name.to_string(), host_id, domain);
            }
        }
        _ => {}
    }
}

fn register_func_entry(
    sn: &McSpaceName,
    host_name: &str,
    func_name: &str,
    host_id: DefId,
    domain: &LoadDomain,
) {
    let fsn = McSpaceName {
        ident: crate::McIds::from(format!("{host_name}.{func_name}")),
        uri: sn.uri.clone(),
    };
    register(
        &fsn,
        DefKind::Func,
        domain,
        &DefValue::Func(FuncDef {
            host: host_id,
            name: func_name.to_string(),
        }),
    );
}

/// Remove every definition of any kind whose defining file matches `uri`.
/// The registry keeps each identity as a tombstone (deleted ≠ never existed);
/// the physical tables drop the entries.
pub fn remove_by_uri(uri: &str) {
    tombstone_by_uri(uri);
    remove_by_uri_from(&workspace::WORKSPACE.components, uri);
    remove_by_uri_from(&workspace::WORKSPACE.modules, uri);
    remove_by_uri_from(&workspace::WORKSPACE.interfaces, uri);
    remove_by_uri_from(&workspace::WORKSPACE.enums, uri);
    remove_by_uri_from(&workspace::WORKSPACE.defines, uri);
}

/// Remove every definition whose defining file is one of `uris`
/// (third-party-lib unload sweep).
pub fn remove_by_uris(uris: &HashSet<String>) {
    tombstone_by_uris(uris);
    remove_by_uris_from(&workspace::WORKSPACE.components, uris);
    remove_by_uris_from(&workspace::WORKSPACE.modules, uris);
    remove_by_uris_from(&workspace::WORKSPACE.interfaces, uris);
    remove_by_uris_from(&workspace::WORKSPACE.enums, uris);
    remove_by_uris_from(&workspace::WORKSPACE.defines, uris);
}

fn tombstone_by_uri(uri: &str) {
    let keys: Vec<McSpaceName> = KEY_TO_ID
        .iter()
        .filter(|e| e.key().uri == uri)
        .map(|e| e.key().clone())
        .collect();
    for key in keys {
        tombstone_key(&key);
    }
}

fn tombstone_by_uris(uris: &HashSet<String>) {
    let keys: Vec<McSpaceName> = KEY_TO_ID
        .iter()
        .filter(|e| uris.contains(e.key().uri.as_uri().as_ref()))
        .map(|e| e.key().clone())
        .collect();
    for key in keys {
        tombstone_key(&key);
    }
}

fn tombstone_key(key: &McSpaceName) {
    if let Some(ids) = KEY_TO_ID.get(key) {
        for id in ids.iter() {
            if let Some(mut e) = ARENA.get_mut(id) {
                let was_live_system =
                    e.data.is_some() && matches!(e.domain, LoadDomain::SystemLib(_));
                e.data = None;
                if was_live_system {
                    system_index_remove(&key.ident.to_string(), e.kind, *id);
                }
            }
        }
    }
}

/// Full process reset: drop every registered identity and its data. Used by
/// the full state clear (`clear_state(ClearScope::Full)`); the append-only
/// identity journal and the checkpoint journal both start over with a clean
/// slate.
pub fn clear_all() {
    KEY_TO_ID.clear();
    ARENA.clear();
    SYSTEM_NAME_INDEX.clear();
    NEXT_DEF_ID.store(0, Ordering::Relaxed);
    JOURNAL.lock().unwrap().clear();
    NEXT_VERSION.store(1, Ordering::Relaxed);
}

/// Tombstone every live definition — project and system-library alike — the
/// active world is being cleared or switched away. Phase 5 makes the system
/// libraries per-world: a world owns its own loaded libs, so switching away
/// drops them with the world (a later `mcb_load_lib` re-registers them under
/// the world, and a snapshot restore revives them via [`restore_system`]).
/// Called from `WorkspaceManager::clear_active`.
pub fn mark_all_tombstones() {
    let keys: Vec<McSpaceName> = KEY_TO_ID.iter().map(|e| e.key().clone()).collect();
    for key in keys {
        tombstone_key(&key);
    }
}

// ============================================================================
// Phase 9 — registry journal, checkpoint, and def-space diff (design §9 E / §10)
// ============================================================================

/// One lightweight, serializable description of a registry identity — the
/// checkpoint/diff record form (design §10). [`DefValue`] is not serializable
/// (it wraps `Arc<McComponent>` and friends), so a checkpoint captures the
/// identity set — id, kind, canonical key, domain, liveness — never the
/// payload. Daemon/RPC and process-restart DefId alignment (§5 D11) read this
/// form; the arena stays the live-data home.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegistryEntrySnapshot {
    /// The persistent identity (D11) — stable across loads, so a diff can
    /// tell the same def apart across checkpoints.
    pub id: DefId,
    pub kind: DefKind,
    /// Display-form identifier of the canonical key (the `ident` half of
    /// [`McSpaceName`]).
    pub ident: String,
    /// Resolved uri of the canonical key (the `uri` half).
    pub uri: String,
    /// Which world this def lived in at the checkpoint.
    pub domain: LoadDomain,
    /// `true` = live data at the checkpoint; `false` = tombstone (deleted,
    /// distinguishable from never existed — D11).
    pub alive: bool,
}

/// One versioned registry checkpoint (design §10). Each load/change appends a
/// `(version, full identity snapshot)` record to the journal; [`diff_versions`]
/// answers "what changed in the definition space" between any two of them.
/// Fully serializable, so daemon/RPC and a process restart can re-align DefIds
/// with cached / external references (§5 D11).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub version: u64,
    /// Every registered identity at this version — live and tombstoned — so a
    /// diff can classify added / removed / modified per [`DefId`] without any
    /// external state.
    pub entries: Vec<RegistryEntrySnapshot>,
}

#[allow(dead_code)] // Phase-9 checkpoint journal API; unit-tested, wired in a later phase
impl Checkpoint {
    /// Serialize to a JSON string — the disk / RPC form (daemon, process
    /// restart). Infallible: every field is JSON-clean.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("checkpoint serialization cannot fail")
    }

    /// Deserialize a [`Checkpoint`] from [`Checkpoint::to_json`] output.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// One def's change between two checkpoints (design §10): added, removed, or
/// modified — compared by [`DefId`].
#[allow(dead_code)] // Phase-9 checkpoint diff item; unit-tested, wired in a later phase
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DefChange {
    pub id: DefId,
    pub kind: DefChangeKind,
    /// The identity description on the older side (`None` when added).
    pub before: Option<RegistryEntrySnapshot>,
    /// The identity description on the newer side (`None` when removed).
    pub after: Option<RegistryEntrySnapshot>,
}

#[allow(dead_code)] // Phase-9 checkpoint diff item; unit-tested, wired in a later phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DefChangeKind {
    /// Not usable at t1 (unregistered or tombstoned), live at t2.
    Added,
    /// Live at t1, not usable at t2 (tombstoned or unregistered).
    Removed,
    /// Live on both sides, but kind / key / domain changed.
    Modified,
}

/// Monotonic checkpoint version; the journal itself is append-only.
static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

/// Append-only checkpoint journal (design §10). A full state reset
/// ([`clear_all`]) starts it over.
static JOURNAL: LazyLock<Mutex<Vec<Checkpoint>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Capture a versioned snapshot of the whole registry — every registered
/// identity, live and tombstoned — and append it to the journal (design §10).
/// Returns the new checkpoint so a caller can diff it against any earlier
/// captured one. The def-layer mutation surfaces and the daemon/RPC layer call
/// it when a persistable "definition space as of version N" is needed.
#[allow(dead_code)] // Phase-9 checkpoint API; unit-tested, wired in a later phase
pub fn checkpoint() -> Checkpoint {
    let version = NEXT_VERSION.fetch_add(1, Ordering::Relaxed);
    let mut entries: Vec<RegistryEntrySnapshot> = ARENA
        .iter()
        .map(|e| RegistryEntrySnapshot {
            id: e.id,
            kind: e.kind,
            ident: e.sn.ident.to_string(),
            uri: e.sn.uri.as_uri().to_string(),
            domain: e.domain.clone(),
            alive: e.data.is_some(),
        })
        .collect();
    entries.sort_by_key(|e| e.id);
    let cp = Checkpoint { version, entries };
    JOURNAL.lock().unwrap().push(cp.clone());
    cp
}

/// Diff two checkpoints (design §10): every def whose identity-set or
/// liveness changed between them, ordered by [`DefId`].
///
/// - **Added**: not usable at `t1` (unregistered or tombstoned) → live at `t2`.
/// - **Removed**: live at `t1` → not usable at `t2` (tombstoned or unregistered).
/// - **Modified**: live on both sides, but kind / key / domain changed.
///
/// Unchanged defs (same description, same liveness) do not appear.
#[allow(dead_code)] // Phase-9 checkpoint API; unit-tested, wired in a later phase
pub fn diff_versions(t1: &Checkpoint, t2: &Checkpoint) -> Vec<DefChange> {
    let a: HashMap<DefId, &RegistryEntrySnapshot> = t1.entries.iter().map(|e| (e.id, e)).collect();
    let b: HashMap<DefId, &RegistryEntrySnapshot> = t2.entries.iter().map(|e| (e.id, e)).collect();
    let mut ids: Vec<DefId> = a.keys().chain(b.keys()).copied().collect();
    ids.sort_unstable();
    ids.dedup();
    let mut changes = Vec::new();
    for id in ids {
        let before = a.get(&id).copied();
        let after = b.get(&id).copied();
        let change = match (before, after) {
            (None, Some(after_e)) => Some(DefChange {
                id,
                kind: DefChangeKind::Added,
                before: None,
                after: Some(after_e.clone()),
            }),
            (Some(before_e), None) => Some(DefChange {
                id,
                kind: DefChangeKind::Removed,
                before: Some(before_e.clone()),
                after: None,
            }),
            (Some(before_e), Some(after_e)) => {
                if before_e.alive && !after_e.alive {
                    Some(DefChange {
                        id,
                        kind: DefChangeKind::Removed,
                        before: Some(before_e.clone()),
                        after: Some(after_e.clone()),
                    })
                } else if !before_e.alive && after_e.alive {
                    Some(DefChange {
                        id,
                        kind: DefChangeKind::Added,
                        before: Some(before_e.clone()),
                        after: Some(after_e.clone()),
                    })
                } else if before_e.alive
                    && after_e.alive
                    && (before_e.kind != after_e.kind
                        || before_e.ident != after_e.ident
                        || before_e.uri != after_e.uri
                        || before_e.domain != after_e.domain)
                {
                    Some(DefChange {
                        id,
                        kind: DefChangeKind::Modified,
                        before: Some(before_e.clone()),
                        after: Some(after_e.clone()),
                    })
                } else {
                    None
                }
            }
            (None, None) => None,
        };
        if let Some(c) = change {
            changes.push(c);
        }
    }
    changes
}

/// The files (uris) touched by a diff — the "which files changed" half of the
/// Phase 9 question ("which defs / files changed"). Sorted, de-duplicated.
#[allow(dead_code)] // Phase-9 checkpoint API; unit-tested, wired in a later phase
pub fn changed_files(changes: &[DefChange]) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for c in changes {
        if let Some(e) = &c.before {
            files.push(e.uri.clone());
        }
        if let Some(e) = &c.after {
            files.push(e.uri.clone());
        }
    }
    files.sort();
    files.dedup();
    files
}

/// One system-library definition captured by [`snapshot_system`] for a world
/// switch. The value is the live [`DefValue`] (func entries are excluded — a
/// snapshot restore re-derives them from their host via [`register_host_funcs`]).
#[derive(Clone)]
pub struct SystemDefSnapshot {
    pub sn: McSpaceName,
    pub kind: DefKind,
    pub domain: LoadDomain,
    pub def: DefValue,
}

/// Capture every live system-library definition (any named lib) — the
/// per-world library state that must follow a world switch. Called from
/// `WorkspaceManager::snapshot_active`.
pub fn snapshot_system() -> Vec<SystemDefSnapshot> {
    ARENA
        .iter()
        .filter(|e| {
            e.data.is_some()
                && e.kind != DefKind::Func
                && matches!(e.domain, LoadDomain::SystemLib(_))
        })
        .map(|e| SystemDefSnapshot {
            sn: e.sn.clone(),
            kind: e.kind,
            domain: e.domain.clone(),
            def: e.data.clone().unwrap(),
        })
        .collect()
}

/// Re-register a captured system-library segment (world restore). Host defs
/// revive under their original [`DefId`] (D11 tombstone revival) and their
/// func entries are re-derived, mirroring [`restore_workspace`]. Called from
/// `WorkspaceManager::restore_snapshot`.
pub(crate) fn restore_system(entries: Vec<SystemDefSnapshot>) {
    for e in entries {
        match &e.def {
            DefValue::Component(_) | DefValue::Module(_) => {
                register(&e.sn, e.kind, &e.domain, &e.def);
                if let Some(host_id) = def_id(&e.sn, e.kind) {
                    register_host_funcs(&e.sn, host_id, &e.def, &e.domain);
                }
            }
            _ => {
                register(&e.sn, e.kind, &e.domain, &e.def);
            }
        }
    }
}

/// Re-register a restored workspace snapshot's five definition tables as
/// project-domain entries, without touching the physical tables (they are
/// refilled directly by `restore_snapshot`). Called from
/// `WorkspaceManager::restore_snapshot` when switching back to a saved
/// workspace: tombstoned identities revive under their original [`DefId`]
/// (D11) and a live system-lib entry is shadowed (workspace-first). The
/// snapshot is authoritative — any conflicting live project entry (not
/// possible in the single-active-workspace flow) keeps its place.
pub(crate) fn restore_workspace(
    components: &DashMap<McSpaceName, Arc<McComponent>>,
    modules: &DashMap<McSpaceName, Arc<McModule>>,
    interfaces: &DashMap<McSpaceName, Arc<McInterface>>,
    enums: &DashMap<McSpaceName, Arc<McEnumDef>>,
    defines: &DashMap<McSpaceName, Arc<McDefineDef>>,
) {
    for e in components.iter() {
        register(
            e.key(),
            DefKind::Component,
            &LoadDomain::Project,
            &DefValue::Component(e.value().clone()),
        );
        if let Some(host_id) = def_id(e.key(), DefKind::Component) {
            register_host_funcs(
                e.key(),
                host_id,
                &DefValue::Component(e.value().clone()),
                &LoadDomain::Project,
            );
        }
    }
    for e in modules.iter() {
        register(
            e.key(),
            DefKind::Module,
            &LoadDomain::Project,
            &DefValue::Module(e.value().clone()),
        );
        if let Some(host_id) = def_id(e.key(), DefKind::Module) {
            register_host_funcs(
                e.key(),
                host_id,
                &DefValue::Module(e.value().clone()),
                &LoadDomain::Project,
            );
        }
    }
    for e in interfaces.iter() {
        register(
            e.key(),
            DefKind::Interface,
            &LoadDomain::Project,
            &DefValue::Interface(e.value().clone()),
        );
    }
    for e in enums.iter() {
        register(
            e.key(),
            DefKind::Enum,
            &LoadDomain::Project,
            &DefValue::Enum(e.value().clone()),
        );
    }
    for e in defines.iter() {
        register(
            e.key(),
            DefKind::Define,
            &LoadDomain::Project,
            &DefValue::Define(e.value().clone()),
        );
    }
}

// ============================================================================
// Read API — the single-table definition view (design §9 Phase B step 4)
// ============================================================================

/// Domain filter for whole-table enumeration.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DomainFilter {
    /// Every domain (unified view).
    Any,
    /// Project (workspace) definitions only.
    Project,
    /// System-library definitions only (any named lib).
    System,
}

fn filter_matches(domain: &LoadDomain, filter: DomainFilter) -> bool {
    match filter {
        DomainFilter::Any => true,
        DomainFilter::Project => matches!(domain, LoadDomain::Project),
        DomainFilter::System => matches!(domain, LoadDomain::SystemLib(_)),
    }
}

/// The live value of one `(key, kind)` identity, any domain.
fn live_entry(sn: &McSpaceName, kind: DefKind) -> Option<DefValue> {
    live_entry_in(sn, kind, DomainFilter::Any)
}

/// The live value of one `(key, kind)` identity restricted to a domain.
fn live_entry_in(sn: &McSpaceName, kind: DefKind, filter: DomainFilter) -> Option<DefValue> {
    let ids = KEY_TO_ID.get(sn)?;
    for id in ids.iter() {
        if let Some(e) = ARENA.get(id) {
            if e.kind == kind && filter_matches(&e.domain, filter) {
                return e.data.clone();
            }
        }
    }
    None
}

/// Enumerate every live definition of `kind` under `filter`.
fn enumerate(kind: DefKind, filter: DomainFilter) -> Vec<(McSpaceName, DefValue)> {
    ARENA
        .iter()
        .filter(|e| e.kind == kind && e.data.is_some() && filter_matches(&e.domain, filter))
        .map(|e| (e.sn.clone(), e.data.clone().unwrap()))
        .collect()
}

fn peel_components(items: Vec<(McSpaceName, DefValue)>) -> Vec<(McSpaceName, Arc<McComponent>)> {
    items
        .into_iter()
        .filter_map(|(sn, d)| match d {
            DefValue::Component(c) => Some((sn, c)),
            _ => None,
        })
        .collect()
}

fn peel_modules(items: Vec<(McSpaceName, DefValue)>) -> Vec<(McSpaceName, Arc<McModule>)> {
    items
        .into_iter()
        .filter_map(|(sn, d)| match d {
            DefValue::Module(m) => Some((sn, m)),
            _ => None,
        })
        .collect()
}

fn peel_interfaces(items: Vec<(McSpaceName, DefValue)>) -> Vec<(McSpaceName, Arc<McInterface>)> {
    items
        .into_iter()
        .filter_map(|(sn, d)| match d {
            DefValue::Interface(i) => Some((sn, i)),
            _ => None,
        })
        .collect()
}

fn peel_enums(items: Vec<(McSpaceName, DefValue)>) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
    items
        .into_iter()
        .filter_map(|(sn, d)| match d {
            DefValue::Enum(e) => Some((sn, e)),
            _ => None,
        })
        .collect()
}

fn peel_defines(items: Vec<(McSpaceName, DefValue)>) -> Vec<(McSpaceName, Arc<McDefineDef>)> {
    items
        .into_iter()
        .filter_map(|(sn, d)| match d {
            DefValue::Define(d) => Some((sn, d)),
            _ => None,
        })
        .collect()
}

/// Look up a component by its `McSpaceName` (any domain).
pub fn get_component(sn: &McSpaceName) -> Option<Arc<McComponent>> {
    match live_entry(sn, DefKind::Component)? {
        DefValue::Component(c) => Some(c),
        _ => None,
    }
}

/// Look up a module by its `McSpaceName` (any domain).
pub fn get_module(sn: &McSpaceName) -> Option<Arc<McModule>> {
    match live_entry(sn, DefKind::Module)? {
        DefValue::Module(m) => Some(m),
        _ => None,
    }
}

/// Look up an interface by its `McSpaceName` (any domain).
pub fn get_interface(sn: &McSpaceName) -> Option<Arc<McInterface>> {
    match live_entry(sn, DefKind::Interface)? {
        DefValue::Interface(i) => Some(i),
        _ => None,
    }
}

/// Look up an enum by its `McSpaceName` (any domain).
pub fn get_enum(sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
    match live_entry(sn, DefKind::Enum)? {
        DefValue::Enum(e) => Some(e),
        _ => None,
    }
}

/// Look up a define by its `McSpaceName` (any domain).
pub fn get_define(sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
    match live_entry(sn, DefKind::Define)? {
        DefValue::Define(d) => Some(d),
        _ => None,
    }
}

/// The [`DefId`] of a live `(key, kind)` identity, any domain. Needed by
/// callers that address a def by id (host links of function templates).
pub fn def_id(sn: &McSpaceName, kind: DefKind) -> Option<DefId> {
    let ids = KEY_TO_ID.get(sn)?;
    for id in ids.iter() {
        if let Some(e) = ARENA.get(id) {
            if e.kind == kind && e.data.is_some() {
                return Some(*id);
            }
        }
    }
    None
}

/// Look up a function-template addressing entry by its qualified key
/// `(uri, "HOST.NAME")` (design §12.1). The func-addressing surface of
/// Phase 4: exercised by the `func_entries_mirror_host_funcs_across_reload`
/// test today; Phase 7/9 diff work reads it.
#[allow(dead_code)]
pub fn get_func(sn: &McSpaceName) -> Option<FuncDef> {
    match live_entry(sn, DefKind::Func)? {
        DefValue::Func(f) => Some(f),
        _ => None,
    }
}

/// Every live function-template entry of a host def (design §12.1 addressing)
/// — mirrors the host's own `funcs` table, so callers can assert consistency
/// between the two. Same consumers as [`get_func`].
#[allow(dead_code)]
pub fn funcs_of_host(sn: &McSpaceName, host_kind: DefKind) -> Vec<(McSpaceName, FuncDef)> {
    let Some(host_id) = def_id(sn, host_kind) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in ARENA.iter() {
        if let Some(DefValue::Func(f)) = &e.data {
            if f.host == host_id {
                out.push((e.sn.clone(), f.clone()));
            }
        }
    }
    out
}

/// Look up a component by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_component(sn: &McSpaceName) -> Option<Arc<McComponent>> {
    match live_entry_in(sn, DefKind::Component, DomainFilter::Project)? {
        DefValue::Component(c) => Some(c),
        _ => None,
    }
}

/// Look up a module by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_module(sn: &McSpaceName) -> Option<Arc<McModule>> {
    match live_entry_in(sn, DefKind::Module, DomainFilter::Project)? {
        DefValue::Module(m) => Some(m),
        _ => None,
    }
}

/// Look up an interface by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_interface(sn: &McSpaceName) -> Option<Arc<McInterface>> {
    match live_entry_in(sn, DefKind::Interface, DomainFilter::Project)? {
        DefValue::Interface(i) => Some(i),
        _ => None,
    }
}

/// Look up an enum by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_enum(sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
    match live_entry_in(sn, DefKind::Enum, DomainFilter::Project)? {
        DefValue::Enum(e) => Some(e),
        _ => None,
    }
}

/// Look up a define by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_define(sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
    match live_entry_in(sn, DefKind::Define, DomainFilter::Project)? {
        DefValue::Define(d) => Some(d),
        _ => None,
    }
}

/// Enumerate every live component definition (any domain).
pub fn all_components() -> Vec<(McSpaceName, Arc<McComponent>)> {
    peel_components(enumerate(DefKind::Component, DomainFilter::Any))
}

/// Enumerate every live module definition (any domain).
pub fn all_modules() -> Vec<(McSpaceName, Arc<McModule>)> {
    peel_modules(enumerate(DefKind::Module, DomainFilter::Any))
}

/// Enumerate every live interface definition (any domain).
pub fn all_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> {
    peel_interfaces(enumerate(DefKind::Interface, DomainFilter::Any))
}

/// Enumerate every live enum definition (any domain).
pub fn all_enums() -> Vec<(McSpaceName, Arc<McEnumDef>)> {
    peel_enums(enumerate(DefKind::Enum, DomainFilter::Any))
}

/// Enumerate every live define definition (any domain).
pub fn all_defines() -> Vec<(McSpaceName, Arc<McDefineDef>)> {
    peel_defines(enumerate(DefKind::Define, DomainFilter::Any))
}

/// Enumerate every project (workspace) component definition.
pub fn workspace_components() -> Vec<(McSpaceName, Arc<McComponent>)> {
    peel_components(enumerate(DefKind::Component, DomainFilter::Project))
}

/// Enumerate every project (workspace) module definition.
pub fn workspace_modules() -> Vec<(McSpaceName, Arc<McModule>)> {
    peel_modules(enumerate(DefKind::Module, DomainFilter::Project))
}

/// Enumerate every project (workspace) interface definition.
pub fn workspace_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> {
    peel_interfaces(enumerate(DefKind::Interface, DomainFilter::Project))
}

/// Enumerate every project (workspace) enum definition.
pub fn workspace_enums() -> Vec<(McSpaceName, Arc<McEnumDef>)> {
    peel_enums(enumerate(DefKind::Enum, DomainFilter::Project))
}

/// Enumerate every project (workspace) define definition.
pub fn workspace_defines() -> Vec<(McSpaceName, Arc<McDefineDef>)> {
    peel_defines(enumerate(DefKind::Define, DomainFilter::Project))
}

/// Enumerate every system-library component definition (P5 visibility).
pub fn system_components() -> Vec<(McSpaceName, Arc<McComponent>)> {
    peel_components(enumerate(DefKind::Component, DomainFilter::System))
}

/// Enumerate every system-library module definition (P5 visibility).
pub fn system_modules() -> Vec<(McSpaceName, Arc<McModule>)> {
    peel_modules(enumerate(DefKind::Module, DomainFilter::System))
}

/// Enumerate every system-library interface definition (P5 visibility).
pub fn system_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> {
    peel_interfaces(enumerate(DefKind::Interface, DomainFilter::System))
}

/// Enumerate every system-library enum definition (P5 visibility).
pub fn system_enums() -> Vec<(McSpaceName, Arc<McEnumDef>)> {
    peel_enums(enumerate(DefKind::Enum, DomainFilter::System))
}

/// Every live system-library identity whose display-form name is `name`, in
/// kind-priority order (component → module → interface → enum → define).
///
/// O(1) via the system name index for the common display-form match; a miss
/// falls back to a segment-structure-equivalent scan (e.g. curly vs dotted
/// idents) so the pre-index semantics are preserved exactly.
pub fn system_name_hits(name: &str) -> Vec<SystemNameHit> {
    if let Some(hits) = SYSTEM_NAME_INDEX.get(name) {
        let mut hits = hits.value().clone();
        hits.sort_by_key(|h| kind_priority(h.kind));
        return hits;
    }
    // Rare: no display-form entry — segment-form equivalent idents (curly vs
    // dot) still match under `are_equivalent`, mirroring the consumers.
    let query = crate::McIds::from(name);
    let mut hits: Vec<SystemNameHit> = ARENA
        .iter()
        .filter(|e| {
            e.data.is_some()
                && e.kind != DefKind::Func
                && matches!(e.domain, LoadDomain::SystemLib(_))
                && crate::semantic::basic::equivalent::are_equivalent(&e.sn.ident, &query)
        })
        .map(|e| SystemNameHit {
            kind: e.kind,
            id: *e.key(),
        })
        .collect();
    hits.sort_by_key(|h| kind_priority(h.kind));
    hits
}

/// The identity + live value of one [`DefId`] — direct arena access for the
/// system name index hits (the caller has already resolved the key).
pub fn live_entry_by_id(id: DefId) -> Option<(McSpaceName, DefValue)> {
    ARENA.get(&id).and_then(|e| {
        let def = e.data.clone()?;
        Some((e.sn.clone(), def))
    })
}

/// Resolve a definition identity to its live class value, in kind-priority
/// order (component → module → interface → enum — the same ordering as the
/// P3/P4 `find_scoped_by_name` in `db/resolve/policy.rs`). O(1) identity
/// lookup covering project and system-lib defs alike; the Phase 6
/// visibility-table hit path resolves through here, and a miss keeps the
/// caller's scope-chain fallback intact.
pub fn cmie_by_identity(sn: &McSpaceName) -> Option<McCMIE> {
    let ids = KEY_TO_ID.get(sn)?;
    for kind in [
        DefKind::Component,
        DefKind::Module,
        DefKind::Interface,
        DefKind::Enum,
    ] {
        for id in ids.iter() {
            let Some(entry) = ARENA.get(id) else {
                continue;
            };
            if entry.kind != kind {
                continue;
            }
            let Some(def) = &entry.data else {
                continue;
            };
            match def {
                DefValue::Component(c) => return Some(McCMIE::Component(c.clone())),
                DefValue::Module(m) => return Some(McCMIE::Module(m.clone())),
                DefValue::Interface(i) => return Some(McCMIE::Interface(i.clone())),
                DefValue::Enum(e) => return Some(McCMIE::Enum(e.clone())),
                _ => {}
            }
        }
    }
    None
}

/// Does the system library (not the workspace) define this identity, as any
/// class kind (component / module / interface / enum)?
pub fn system_contains(sn: &McSpaceName) -> bool {
    let Some(ids) = KEY_TO_ID.get(sn) else {
        return false;
    };
    for id in ids.iter() {
        if let Some(e) = ARENA.get(id) {
            if e.data.is_some()
                && matches!(e.domain, LoadDomain::SystemLib(_))
                && matches!(
                    e.kind,
                    DefKind::Component | DefKind::Module | DefKind::Interface | DefKind::Enum
                )
            {
                return true;
            }
        }
    }
    false
}

/// The kind of a live definition under `sn`, if any — any domain, module
/// priority (mirrors the old lib-ledger `contains_key` chain).
pub fn kind_of(sn: &McSpaceName) -> Option<DefKind> {
    for kind in [
        DefKind::Module,
        DefKind::Component,
        DefKind::Interface,
        DefKind::Enum,
        DefKind::Define,
    ] {
        if live_entry(sn, kind).is_some() {
            return Some(kind);
        }
    }
    None
}

/// Every live definition whose file uri contains `prefix` — the lib-ledger
/// symbol collection in `mcb_load_lib` (replaces the ten physical-table
/// prefix scans). Function-template entries are host members, not top-level
/// definitions, so they stay out of the ledger.
pub fn spacenames_by_uri_prefix(prefix: &str) -> Vec<McSpaceName> {
    ARENA
        .iter()
        .filter(|e| e.data.is_some() && e.kind != DefKind::Func && e.sn.uri.contains(prefix))
        .map(|e| e.sn.clone())
        .collect()
}

// ============================================================================
// Physical-table helpers (compatibility materialization)
// ============================================================================

fn insert_one<K, V>(table: &DashMap<K, V>, key: K, value: V) -> InsertOutcome
where
    K: Eq + Hash + Clone,
{
    match table.entry(key) {
        dashmap::Entry::Occupied(_) => InsertOutcome::Duplicate,
        dashmap::Entry::Vacant(vacant) => {
            vacant.insert(value);
            InsertOutcome::Inserted
        }
    }
}

fn remove_by_uri_from<T>(table: &DashMap<McSpaceName, Arc<T>>, uri: &str) {
    let to_remove: Vec<McSpaceName> = table
        .iter()
        .filter(|e| e.key().uri == uri)
        .map(|e| e.key().clone())
        .collect();
    for key in to_remove {
        table.remove(&key);
    }
}

fn remove_by_uris_from<T>(table: &DashMap<McSpaceName, Arc<T>>, uris: &HashSet<String>) {
    let to_remove: Vec<McSpaceName> = table
        .iter()
        .filter(|e| uris.contains(e.key().uri.as_uri().as_ref()))
        .map(|e| e.key().clone())
        .collect();
    for key in to_remove {
        table.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::common::uri_intern;
    use crate::semantic::mc_enum::{McEnumDef, McEnumValue};
    use std::sync::Arc;

    /// A minimal system enum under a caller-chosen name + uri. The system
    /// name index tests only track index-vs-registry consistency, so the def
    /// payload itself is irrelevant.
    fn sys_enum(name: &str, uri: &str) -> (McSpaceName, DefValue) {
        let sn = McSpaceName {
            ident: crate::McIds::from(name),
            uri: uri_intern(uri),
        };
        let def = DefValue::Enum(Arc::new(McEnumDef {
            name: sn.ident.clone(),
            span: [0, 3],
            values: vec![McEnumValue {
                name: crate::McIds::from("A"),
                span: [0, 3],
            }],
            uri: uri.to_string(),
        }));
        (sn, def)
    }

    /// The system name index must stay exactly in sync with the registry's
    /// live system segment across the mutation points: fresh insert, tombstone
    /// (lib unload sweep), tombstone revival (re-load), and the workspace-first
    /// project shadow. Uses a unique name/uri so parallel lib tests are never
    /// disturbed.
    #[test]
    fn system_name_index_tracks_live_system_entries() {
        const NAME: &str = "SYS_INDEX_GOLD";
        const URI: &str = "/sys/index.mc";
        let (sn, def) = sys_enum(NAME, URI);
        let system = LoadDomain::SystemLib("mcode".into());

        // 1. Fresh system insert: the index sees it.
        assert_eq!(
            insert(&sn, system.clone(), def.clone()),
            InsertOutcome::Inserted
        );
        assert_eq!(
            system_name_hits(NAME).len(),
            1,
            "fresh system def is indexed"
        );
        assert!(system_contains(&sn), "registry agrees with the index");

        // 2. Tombstone (lib unload sweep): the index drops it with the entry.
        remove_by_uri(URI);
        assert!(
            system_name_hits(NAME).is_empty(),
            "tombstoned system def leaves the index"
        );
        assert!(!system_contains(&sn));

        // 3. Revive under the same key (re-load): the index follows.
        assert_eq!(
            insert(&sn, system.clone(), def.clone()),
            InsertOutcome::Inserted
        );
        assert_eq!(
            system_name_hits(NAME).len(),
            1,
            "revived system def re-indexed"
        );

        // 4. A project def shadows the system def (workspace-first): the
        // identity leaves the system segment and the index with it.
        assert_eq!(
            insert(&sn, LoadDomain::Project, def.clone()),
            InsertOutcome::Inserted
        );
        assert!(
            system_name_hits(NAME).is_empty(),
            "project shadowing evicts the system hit"
        );
        assert!(!system_contains(&sn), "the identity is a project def now");

        // Leave no residue for parallel tests.
        remove_by_uri(URI);
    }

    /// Monotonic suffix so parallel test threads never collide on a temp file
    /// name inside one process (pid covers cross-process runs).
    static TEST_FILE_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// Phase 9 golden diff: a def appearing (Added), shadowing across worlds
    /// (Modified), disappearing (Removed) and reviving (Added) must be
    /// reported per stable [`DefId`], and the touched files must be
    /// answerable. Assertions filter by this test's uri because lib tests run
    /// in parallel and share the registry.
    #[test]
    fn checkpoint_diff_reports_add_remove_modify() {
        const NAME: &str = "CP_DIFF_GOLD";
        const URI: &str = "/cp/diff_gold.mc";
        let (sn, def) = sys_enum(NAME, URI);
        let system = LoadDomain::SystemLib("mcode".into());
        let ours = |changes: Vec<DefChange>| -> Vec<DefChange> {
            changes
                .into_iter()
                .filter(|c| {
                    c.before.as_ref().is_some_and(|e| e.uri == URI)
                        || c.after.as_ref().is_some_and(|e| e.uri == URI)
                })
                .collect()
        };

        // t1: registry state as of now (any parallel test residue included).
        let t1 = checkpoint();

        // The enum appears.
        assert_eq!(
            insert(&sn, system.clone(), def.clone()),
            InsertOutcome::Inserted
        );
        let t2 = checkpoint();

        // A project def shadows the same identity (workspace-first): the same
        // DefId stays live but its world changes -> Modified.
        assert_eq!(
            insert(&sn, LoadDomain::Project, def.clone()),
            InsertOutcome::Inserted
        );
        let t3 = checkpoint();

        // Unload sweep tombstones it: live -> dead -> Removed.
        remove_by_uri(URI);
        let t4 = checkpoint();

        // Re-load revives it under the same key: dead -> live -> Added.
        assert_eq!(
            insert(&sn, LoadDomain::Project, def.clone()),
            InsertOutcome::Inserted
        );
        let t5 = checkpoint();

        let d1 = ours(diff_versions(&t1, &t2));
        assert_eq!(d1.len(), 1, "one def appeared");
        assert_eq!(d1[0].kind, DefChangeKind::Added);
        let after1 = d1[0].after.as_ref().unwrap();
        assert_eq!(after1.ident, NAME);
        assert_eq!(after1.uri, URI);
        assert!(after1.alive);
        assert!(d1[0].before.is_none());

        let d2 = ours(diff_versions(&t2, &t3));
        assert_eq!(d2.len(), 1, "one def modified");
        assert_eq!(d2[0].kind, DefChangeKind::Modified);
        assert_eq!(d2[0].id, d1[0].id, "the same DefId across checkpoints");
        assert_eq!(d2[0].before.as_ref().unwrap().domain, system);
        assert_eq!(d2[0].after.as_ref().unwrap().domain, LoadDomain::Project);

        let d3 = ours(diff_versions(&t3, &t4));
        assert_eq!(d3.len(), 1, "one def removed");
        assert_eq!(d3[0].kind, DefChangeKind::Removed);
        assert_eq!(d3[0].id, d1[0].id, "the same DefId stayed a tombstone");
        assert_eq!(d3[0].after.as_ref().unwrap().alive, false);

        let d4 = ours(diff_versions(&t4, &t5));
        assert_eq!(d4.len(), 1, "one def revived");
        assert_eq!(d4[0].kind, DefChangeKind::Added);
        assert_eq!(d4[0].id, d1[0].id, "revival reuses the stable DefId");

        // "Which files changed" is answerable from the diff.
        assert_eq!(changed_files(&d4), vec![URI.to_string()]);

        // An identity held live and unchanged across both sides is invisible
        // to the diff.
        assert!(ours(diff_versions(&t5, &checkpoint())).is_empty());

        // Leave no residue for parallel tests.
        remove_by_uri(URI);
    }

    /// Phase 9: a checkpoint round-trips through JSON in memory and through a
    /// real disk file (daemon/RPC persistence); the restored copy is
    /// identical — the process-restart DefId alignment surface (§5 D11).
    #[test]
    fn checkpoint_serializes_to_json_and_disk() {
        const NAME: &str = "CP_SERDE_GOLD";
        const URI: &str = "/cp/serde_gold.mc";
        let (sn, def) = sys_enum(NAME, URI);
        assert_eq!(
            insert(&sn, LoadDomain::SystemLib("mcode".into()), def.clone()),
            InsertOutcome::Inserted
        );
        let cp = checkpoint();

        // In-memory JSON round trip.
        let json = cp.to_json();
        assert!(json.contains(NAME), "json carries the ident");
        assert!(json.contains(URI), "json carries the uri");
        assert!(json.contains("\"alive\":true"), "json carries liveness");
        assert_eq!(
            Checkpoint::from_json(&json).expect("json round trips"),
            cp,
            "deserialized checkpoint is identical"
        );

        // Disk round trip (daemon/RPC persistence): write, read back, drop.
        let path = std::env::temp_dir().join(format!(
            "mcc_checkpoint_{}_{}.json",
            std::process::id(),
            TEST_FILE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::write(&path, &json).expect("write checkpoint to disk");
        let from_disk = std::fs::read_to_string(&path).expect("read checkpoint back");
        assert_eq!(
            Checkpoint::from_json(&from_disk).expect("disk json parses"),
            cp,
            "disk round trip is identical"
        );
        std::fs::remove_file(&path).expect("clean up the checkpoint file");

        // Leave no residue for parallel tests.
        remove_by_uri(URI);
    }
}
