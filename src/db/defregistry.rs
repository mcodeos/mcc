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
//! - [`LoadDomain`] tags where each def lives (`Project | SystemLib(name)`).
//!
//! The registry is **world-owned state** (T3, defspace-id-core-plan): the
//! whole identity layer lives in a [`RegistryState`] that the owning
//! [`WorkspaceManager`](crate::db::cmie::tables::WorkspaceManager) carries as
//! a field — system-lib defs live in the owning world's registry and follow
//! world create / switch / unload. The free-function surface of this module
//! reads/writes the **active world's** registry (the process-global
//! [`WORKSPACE`](crate::db::cmie::tables::WORKSPACE)) via [`active`], so the
//! caller files keep their process-wide semantics unchanged; workspace
//! lifecycle (snapshot / tombstone / restore) runs through each instance's own
//! `RegistryState`. Full per-world isolation of the caller surface is the
//! remaining Phase-5 step — see the T3 honest boundary in
//! `mcd/doc/plan/defspace-id-core-plan.md`.
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
use crate::db::defmember::{DefMemberId, MemberLedger};
use crate::semantic::component::McComponent;
use crate::semantic::mc_define::McDefineDef;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::{McCMIE, McSpaceName};
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Compact persistent identity of one definition (design §5 D11). Identities
/// are world-local now: each world's [`RegistryState`] allocates its own ids,
/// and the tombstone/revive flow keeps them stable within the world.
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
    /// T6 (G6 / D15.2): content fingerprint of the live `data` — a
    /// stable-reproducible, declaration-level hash (see
    /// [`content_fingerprint`]). Re-derived on every data write; `0` for a
    /// tombstone (diff ignores the fingerprint unless both sides are alive).
    /// The instance-layer circuit diff owns the body content, so the def
    /// fingerprint covers the def's own declaration surface.
    pub fingerprint: u64,
}

/// How many of the most recent checkpoints the journal retains (T6-②, O1):
/// the journal is a sliding window, not an unbounded log — every write still
/// lands on a consumer-held [`Checkpoint`] copy (fully serializable), so
/// [`diff_versions`] keeps working between any two captured versions even
/// after the older one has been truncated from the registry-side journal.
const JOURNAL_KEEP_LATEST: usize = 64;

/// The per-world definition registry — the whole identity layer of one world
/// (Phase 5 / T3 world-owned carrier): the id counter, the canonical-key
/// index, the entry arena, the system name index, and the Phase-9 checkpoint
/// journal.
///
/// The active world's registry is owned by the process-global
/// [`WORKSPACE`](crate::db::cmie::tables::WORKSPACE) (see [`active`]); every
/// [`WorkspaceManager`](crate::db::cmie::tables::WorkspaceManager) instance
/// carries its own `RegistryState` so world create / switch / unload drive the
/// lifecycle on the instance that owns the world. All mutation is interior
/// (DashMap / atomics / mutex), so the methods take `&self`.
pub(crate) struct RegistryState {
    /// Next free [`DefId`] (append-only; ids are never recycled within a
    /// world — a fresh world starts at 0 via [`Default`]).
    next_def_id: AtomicU32,
    /// Canonical key → its [`DefId`]s. Append-only: keys are never removed, so
    /// an identity survives load/unload cycles ("deleted vs never existed" is
    /// decidable). A key maps to a small vector because one `(uri, ident)` may
    /// legally hold several kinds — a same-named component and interface in
    /// one file coexist exactly as the per-kind physical tables allowed.
    key_to_id: DashMap<McSpaceName, Vec<DefId>>,
    /// [`DefId`] → entry data. The arena holds the current data per identity;
    /// data is re-materialized on re-parse while the identity stays stable
    /// (D11: identity in the registry, data fresh in the arena).
    arena: DashMap<DefId, DefEntry>,
    /// Display-form name → live system-library identities. Kept exactly in
    /// sync with the registry's live system segment by the mutation points
    /// (domain transitions in `register`, tombstones, world reset); gives the
    /// P5 name-only lookups (`resolve_system`, `find_in_table_scoped`, the
    /// enum helpers, `component_by_class`) O(1) instead of a full registry
    /// scan on every class reference. Function-template entries are host
    /// members, not class names, so they stay out of the index.
    system_name_index: DashMap<String, Vec<SystemNameHit>>,
    /// Monotonic checkpoint version; the journal itself is append-only.
    next_version: AtomicU64,
    /// Append-only checkpoint journal (design §10). A full state reset
    /// (`clear_all`) starts it over.
    journal: Mutex<Vec<Checkpoint>>,
    /// Per-def member account ledgers (T4, defspace-id-core-plan M1): the
    /// stable [`DefMemberId`] assignment of a def's member table (component
    /// pins, module io ports), keyed by the owning def's [`DefId`]. Written by
    /// `register` (components, from the parse artifact's declaration order)
    /// and by module instantiation (ports, from the built port table), each
    /// merge-by-name — a re-parse must never re-derive member ids from
    /// scratch. Read by the instance layer when a `PointId` pins a def
    /// member. Tombstoning a def keeps its ledger so a revive reuses the ids;
    /// only a full registry reset (`clear_all`) drops them.
    member_ledgers: DashMap<DefId, MemberLedger>,
    /// T6-②: whether a def-space mutation landed since the last checkpoint was
    /// captured. Set on every real write (fresh insert, revive, project
    /// override, module re-derive, live→tombstone); cleared by any capture.
    /// The loader seams call [`RegistryState::checkpoint_if_changed`], so a
    /// no-op re-parse (nothing changed) never stamps a version (O1), while
    /// every edit / load / remove / world switch round ends exactly one.
    mutated: AtomicBool,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            next_def_id: AtomicU32::new(0),
            key_to_id: DashMap::new(),
            arena: DashMap::new(),
            system_name_index: DashMap::new(),
            next_version: AtomicU64::new(1),
            journal: Mutex::new(Vec::new()),
            member_ledgers: DashMap::new(),
            mutated: AtomicBool::new(false),
        }
    }
}

/// The active world's registry — the one owned by the process-global
/// [`WORKSPACE`](crate::db::cmie::tables::WORKSPACE). The free-function API
/// below serves this registry, preserving the single-active-world semantics
/// the caller files were written against; per-instance lifecycle goes through
/// `WorkspaceManager::registry`.
fn active() -> &'static RegistryState {
    &workspace::WORKSPACE.registry
}

/// One live system-library identity registered under a display-form name in
/// the system name index (the P5 name-only lookup surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemNameHit {
    pub kind: DefKind,
    pub id: DefId,
}

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

// ============================================================================
// RegistryState — write side
// ============================================================================

impl RegistryState {
    /// Registry-only side of [`insert`]: register the identity + data and
    /// re-derive the host's function entries. The physical-table write stays
    /// in the free [`insert`] wrapper (it needs the workspace tables).
    fn insert(&self, sn: &McSpaceName, domain: &LoadDomain, def: &DefValue) -> InsertOutcome {
        let kind = def.kind();
        let outcome = self.register(sn, kind, domain, def);
        if outcome == InsertOutcome::Inserted {
            // Function templates derive from the host's `funcs` table:
            // register the addressing entries (design §12.1) for a fresh
            // host. Kept in sync by `register_host_funcs` — stale entries
            // are tombstoned first.
            if kind == DefKind::Component || kind == DefKind::Module {
                if let Some(host_id) = self.def_id(sn, kind) {
                    self.register_host_funcs(sn, host_id, def, domain);
                }
            }
        }
        outcome
    }

    /// Register the identity + data (Phase 3 identity layer).
    ///
    /// Precedence rules for an occupied live `(key, kind)`:
    /// - A project def **overrides** a same-key system-lib def. The mcode lib
    ///   loads first, then a project file re-declares the identity; the
    ///   workspace def must shadow the system def (workspace-first, P0.1). The
    ///   reverse (system lib displacing a project def) is a duplicate, and so
    ///   is any same-domain re-insert (the caller turns it into the DUP
    ///   diagnostic).
    /// - A module re-derive always replaces this file's prior entry.
    /// - A tombstone is revived with the new data under the same [`DefId`]
    ///   (D11).
    fn register(
        &self,
        sn: &McSpaceName,
        kind: DefKind,
        domain: &LoadDomain,
        def: &DefValue,
    ) -> InsertOutcome {
        // T4: the component def's member sequence for the registry-owned
        // account ledger, captured before any mutation below — every path
        // that writes fresh data syncs the ledger from it (merge-by-name, so
        // a revive under the same key reuses the surviving member ids).
        let member_seq = Self::component_member_seq(def);
        let mut ids = self.key_to_id.entry(sn.clone()).or_default();
        for &id in ids.iter() {
            let mut entry = self
                .arena
                .get_mut(&id)
                .expect("arena holds every registered id");
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
                entry.fingerprint = content_fingerprint(def);
                self.mutated.store(true, Ordering::Relaxed);
                self.sync_system_index(
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
                    entry.fingerprint = content_fingerprint(def);
                    self.mutated.store(true, Ordering::Relaxed);
                    self.sync_system_index(
                        &sn.ident.to_string(),
                        kind,
                        id,
                        was_live_system,
                        &entry.domain,
                    );
                    if !member_seq.is_empty() {
                        self.sync_member_ledger(id, &member_seq);
                    }
                    return InsertOutcome::Inserted;
                }
                Some(_) => {
                    if was_live_system && matches!(domain, LoadDomain::Project) {
                        entry.data = Some(def.clone());
                        entry.domain = LoadDomain::Project;
                        entry.fingerprint = content_fingerprint(def);
                        self.mutated.store(true, Ordering::Relaxed);
                        self.sync_system_index(
                            &sn.ident.to_string(),
                            kind,
                            id,
                            true,
                            &entry.domain,
                        );
                        if !member_seq.is_empty() {
                            self.sync_member_ledger(id, &member_seq);
                        }
                        return InsertOutcome::Inserted;
                    }
                    return InsertOutcome::Duplicate;
                }
            }
        }
        let id = self.next_def_id.fetch_add(1, Ordering::Relaxed);
        ids.push(id);
        self.mutated.store(true, Ordering::Relaxed);
        self.arena.insert(
            id,
            DefEntry {
                id,
                kind,
                sn: sn.clone(),
                domain: domain.clone(),
                data: Some(def.clone()),
                fingerprint: content_fingerprint(def),
            },
        );
        if matches!(domain, LoadDomain::SystemLib(_)) && kind != DefKind::Func {
            self.system_index_add(&sn.ident.to_string(), kind, id);
        }
        if !member_seq.is_empty() {
            self.sync_member_ledger(id, &member_seq);
        }
        InsertOutcome::Inserted
    }

    /// T4 (defspace-id-core-plan M1): the member sequence of a fresh def
    /// payload. Components contribute their pins in source declaration order
    /// (the parse-side `decl_order`); every other def kind has no
    /// registry-owned member ledger here — module ports are synced when the
    /// module is instantiated (the port table is built there), and interface
    /// members / bus members / labels are content-addressed (declaration
    /// order only), never referenced by a stable member id (§2.5 threshold).
    fn component_member_seq(def: &DefValue) -> Vec<(String, String)> {
        match def {
            DefValue::Component(comp) => comp
                .pins
                .decl_order
                .iter()
                .map(|pid| {
                    let iotype = comp
                        .pins
                        .pins
                        .get(pid)
                        .map(|p| format!("{:?}", p.iotype))
                        .unwrap_or_default();
                    (pid.clone(), iotype)
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Merge the def's current member sequence into its append-only account
    /// ledger (invariant C / D13). Names still declared keep their original
    /// id (only the iotype label refreshes), new names append after the
    /// current high-water mark, and names that disappeared are tombstoned —
    /// inserting a member mid-table never shifts later members' ids.
    fn sync_member_ledger(&self, id: DefId, members: &[(String, String)]) {
        let mut ledger = self.member_ledgers.entry(id).or_default();
        let old_live: Vec<String> = ledger.live_members().map(|m| m.name.clone()).collect();
        let mut keep: HashSet<&str> = HashSet::with_capacity(members.len());
        for (name, iotype) in members {
            keep.insert(name.as_str());
            ledger.register(name, iotype);
        }
        for name in old_live {
            if !keep.contains(name.as_str()) {
                ledger.tombstone(&name);
            }
        }
    }

    /// The stable [`DefMemberId`] of a live def member (`name` of `kind`
    /// under `sn`), from the def's registry-owned account ledger (T4). `None`
    /// when the def is not a registered live identity or the name is not a
    /// live member (never declared, or retired by a tombstone).
    fn def_member_id_of(&self, sn: &McSpaceName, kind: DefKind, name: &str) -> Option<DefMemberId> {
        let id = self.def_id(sn, kind)?;
        self.member_ledgers.get(&id)?.id_of(name)
    }

    /// T4 (M1b): merge a module's just-built port table into the module
    /// def's registry-owned port ledger. Called from module instantiation
    /// once the port table is finalized — the registry is the single stable
    /// carrier, so a port inserted mid-declaration across def edits never
    /// shifts later ports' member ids (`resolve_point` prefers the ledger
    /// over the positional ordinal). Module trees whose def is not a
    /// registered identity (func-expanded synthetic modules) are skipped —
    /// their points keep the historical positional ordinal.
    fn sync_module_ports(&self, sn: &McSpaceName, ports: &[(String, String)]) {
        if let Some(id) = self.def_id(sn, DefKind::Module) {
            // Unconditional: an empty port list must still tombstone the
            // def's previously-live ports.
            self.sync_member_ledger(id, ports);
        }
    }

    /// Tombstone this host's previously-registered function entries, then
    /// register the current ones from the host's `funcs` table — keeps the
    /// addressing entries exactly in sync with the host across re-derive
    /// rounds (modules) and reloads (components, whose uri-level tombstone
    /// already cleared the funcs). Removed funcs stay tombstoned; survivors
    /// revive under the same [`DefId`] (D11).
    fn register_host_funcs(
        &self,
        sn: &McSpaceName,
        host_id: DefId,
        host: &DefValue,
        domain: &LoadDomain,
    ) {
        let stale: Vec<DefId> = self
            .arena
            .iter()
            .filter_map(|e| match &e.data {
                Some(DefValue::Func(f)) if f.host == host_id => Some(*e.key()),
                _ => None,
            })
            .collect();
        for id in stale {
            if let Some(mut e) = self.arena.get_mut(&id) {
                e.data = None;
            }
        }
        let host_name = sn.ident.to_string();
        match host {
            DefValue::Component(comp) => {
                for f in comp.funcs.iter() {
                    self.register_func_entry(sn, &host_name, &f.name.to_string(), host_id, domain);
                }
            }
            DefValue::Module(module) => {
                for f in module.funcs.iter() {
                    self.register_func_entry(sn, &host_name, &f.name.to_string(), host_id, domain);
                }
            }
            _ => {}
        }
    }

    fn register_func_entry(
        &self,
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
        self.register(
            &fsn,
            DefKind::Func,
            domain,
            &DefValue::Func(FuncDef {
                host: host_id,
                name: func_name.to_string(),
            }),
        );
    }

    /// Registry side of [`remove_by_uri`]: tombstone every definition of any
    /// kind whose defining file matches `uri` (the physical-table sweep stays
    /// in the free wrapper).
    fn remove_by_uri(&self, uri: &str) {
        let keys: Vec<McSpaceName> = self
            .key_to_id
            .iter()
            .filter(|e| e.key().uri == uri)
            .map(|e| e.key().clone())
            .collect();
        for key in keys {
            self.tombstone_key(&key);
        }
    }

    /// Registry side of [`remove_by_uris`] (third-party-lib unload sweep).
    fn remove_by_uris(&self, uris: &HashSet<String>) {
        let keys: Vec<McSpaceName> = self
            .key_to_id
            .iter()
            .filter(|e| uris.contains(e.key().uri.as_uri().as_ref()))
            .map(|e| e.key().clone())
            .collect();
        for key in keys {
            self.tombstone_key(&key);
        }
    }

    fn tombstone_key(&self, key: &McSpaceName) {
        if let Some(ids) = self.key_to_id.get(key) {
            for id in ids.iter() {
                if let Some(mut e) = self.arena.get_mut(id) {
                    let was_live_system =
                        e.data.is_some() && matches!(e.domain, LoadDomain::SystemLib(_));
                    let had_data = e.data.is_some();
                    e.data = None;
                    e.fingerprint = 0;
                    if had_data {
                        // A live identity became a tombstone — a def-space
                        // change the next checkpoint must record.
                        self.mutated.store(true, Ordering::Relaxed);
                    }
                    if was_live_system {
                        self.system_index_remove(&key.ident.to_string(), e.kind, *id);
                    }
                }
            }
        }
    }

    /// Full reset of this registry: drop every registered identity and its
    /// data. Used by the full state clear (`clear_state(ClearScope::Full)`);
    /// the append-only identity journal and the checkpoint journal both start
    /// over with a clean slate.
    fn clear_all(&self) {
        self.key_to_id.clear();
        self.arena.clear();
        self.system_name_index.clear();
        self.member_ledgers.clear();
        self.next_def_id.store(0, Ordering::Relaxed);
        self.journal.lock().unwrap().clear();
        self.next_version.store(1, Ordering::Relaxed);
        self.mutated.store(false, Ordering::Relaxed);
    }

    /// Tombstone every live definition — project and system-library alike —
    /// the owning world is being cleared or switched away. A world owns its
    /// own loaded libs, so switching away drops them with the world (a later
    /// `mcb_load_lib` re-registers them under the world, and a snapshot
    /// restore revives them via [`RegistryState::restore_system`]). Called
    /// from `WorkspaceManager::clear_active` on the instance's own registry.
    pub(crate) fn mark_all_tombstones(&self) {
        let keys: Vec<McSpaceName> = self.key_to_id.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            self.tombstone_key(&key);
        }
    }

    /// Build one versioned snapshot of the whole registry — every registered
    /// identity, live and tombstoned (design §10). The caller owns the
    /// version number: [`RegistryState::capture`] stamps it into the journal,
    /// while [`RegistryState::diff_since`] snapshots the live state at the
    /// current (unstamped) version to diff against a retained one.
    fn snapshot(&self, version: u64) -> Checkpoint {
        let mut entries: Vec<RegistryEntrySnapshot> = self
            .arena
            .iter()
            .map(|e| RegistryEntrySnapshot {
                id: e.id,
                kind: e.kind,
                ident: e.sn.ident.to_string(),
                uri: e.sn.uri.as_uri().to_string(),
                domain: e.domain.clone(),
                alive: e.data.is_some(),
                fingerprint: if e.data.is_some() {
                    Some(e.fingerprint)
                } else {
                    None
                },
            })
            .collect();
        entries.sort_by_key(|e| e.id);
        Checkpoint { version, entries }
    }

    /// Append one versioned snapshot to the journal (design §10: every
    /// load/change stamps `(version, alive-set)`), trim the journal to its
    /// sliding window, and reset the mutation flag. Returns the new
    /// checkpoint so a caller can diff it against any earlier captured one.
    fn capture(&self) -> Checkpoint {
        let version = self.next_version.fetch_add(1, Ordering::Relaxed);
        let cp = self.snapshot(version);
        let mut journal = self.journal.lock().unwrap();
        journal.push(cp.clone());
        let excess = journal.len().saturating_sub(JOURNAL_KEEP_LATEST);
        if excess > 0 {
            journal.drain(..excess);
        }
        drop(journal);
        self.mutated.store(false, Ordering::Relaxed);
        cp
    }

    /// Capture a versioned snapshot unconditionally — the daemon/RPC "capture
    /// the definition space as of now" entry and the unit-test primitive.
    /// Every capture (unconditional or conditional) resets the mutation flag,
    /// so a later no-op [`RegistryState::checkpoint_if_changed`] stays silent.
    pub(crate) fn checkpoint(&self) -> Checkpoint {
        self.capture()
    }

    /// The seam entry the def-layer change surfaces call at the end of a
    /// load / remove / world-switch round: stamp exactly one version when the
    /// round actually mutated the registry, and nothing otherwise (O1 — a
    /// no-op re-parse never inflates the journal).
    pub(crate) fn checkpoint_if_changed(&self) -> Option<Checkpoint> {
        if self.mutated.load(Ordering::Relaxed) {
            Some(self.capture())
        } else {
            None
        }
    }

    /// The most recently stamped checkpoint (the journal tail), if any.
    pub(crate) fn latest_checkpoint(&self) -> Option<Checkpoint> {
        self.journal.lock().unwrap().last().cloned()
    }

    /// Diff the definition space between a retained journal version and the
    /// live state, without stamping a new version (design §10 `diff_defs`).
    /// `Err` when `from_version` has fallen out of the sliding journal window
    /// — the caller must re-baseline against a fresh capture.
    pub(crate) fn diff_since(&self, from_version: u64) -> Result<Vec<DefChange>, String> {
        let journal = self.journal.lock().unwrap();
        let from = journal
            .iter()
            .find(|c| c.version == from_version)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "checkpoint version {from_version} is not retained (the journal keeps the latest {JOURNAL_KEEP_LATEST}); re-baseline with a fresh capture"
                )
            })?;
        let version = self.next_version.load(Ordering::Relaxed);
        let current = self.snapshot(version);
        drop(journal);
        Ok(diff_versions(&from, &current))
    }

    /// Capture every live system-library definition (any named lib) — the
    /// per-world library state that must follow a world switch. Called from
    /// `WorkspaceManager::snapshot_active` on the instance's own registry.
    pub(crate) fn snapshot_system(&self) -> Vec<SystemDefSnapshot> {
        self.arena
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

    /// Re-register a captured system-library segment (world restore). Host
    /// defs revive under their original [`DefId`] (D11 tombstone revival) and
    /// their func entries are re-derived, mirroring
    /// [`RegistryState::restore_workspace`]. Called from
    /// `WorkspaceManager::restore_snapshot`.
    pub(crate) fn restore_system(&self, entries: Vec<SystemDefSnapshot>) {
        for e in entries {
            match &e.def {
                DefValue::Component(_) | DefValue::Module(_) => {
                    self.register(&e.sn, e.kind, &e.domain, &e.def);
                    if let Some(host_id) = self.def_id(&e.sn, e.kind) {
                        self.register_host_funcs(&e.sn, host_id, &e.def, &e.domain);
                    }
                }
                _ => {
                    self.register(&e.sn, e.kind, &e.domain, &e.def);
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
        &self,
        components: &DashMap<McSpaceName, Arc<McComponent>>,
        modules: &DashMap<McSpaceName, Arc<McModule>>,
        interfaces: &DashMap<McSpaceName, Arc<McInterface>>,
        enums: &DashMap<McSpaceName, Arc<McEnumDef>>,
        defines: &DashMap<McSpaceName, Arc<McDefineDef>>,
    ) {
        for e in components.iter() {
            self.register(
                e.key(),
                DefKind::Component,
                &LoadDomain::Project,
                &DefValue::Component(e.value().clone()),
            );
            if let Some(host_id) = self.def_id(e.key(), DefKind::Component) {
                self.register_host_funcs(
                    e.key(),
                    host_id,
                    &DefValue::Component(e.value().clone()),
                    &LoadDomain::Project,
                );
            }
        }
        for e in modules.iter() {
            self.register(
                e.key(),
                DefKind::Module,
                &LoadDomain::Project,
                &DefValue::Module(e.value().clone()),
            );
            if let Some(host_id) = self.def_id(e.key(), DefKind::Module) {
                self.register_host_funcs(
                    e.key(),
                    host_id,
                    &DefValue::Module(e.value().clone()),
                    &LoadDomain::Project,
                );
            }
        }
        for e in interfaces.iter() {
            self.register(
                e.key(),
                DefKind::Interface,
                &LoadDomain::Project,
                &DefValue::Interface(e.value().clone()),
            );
        }
        for e in enums.iter() {
            self.register(
                e.key(),
                DefKind::Enum,
                &LoadDomain::Project,
                &DefValue::Enum(e.value().clone()),
            );
        }
        for e in defines.iter() {
            self.register(
                e.key(),
                DefKind::Define,
                &LoadDomain::Project,
                &DefValue::Define(e.value().clone()),
            );
        }
    }
}

// ============================================================================
// RegistryState — system name index (kept exactly in sync with the live
// system segment by the mutation points above)
// ============================================================================

impl RegistryState {
    fn system_index_add(&self, name: &str, kind: DefKind, id: DefId) {
        self.system_name_index
            .entry(name.to_string())
            .or_default()
            .push(SystemNameHit { kind, id });
    }

    fn system_index_remove(&self, name: &str, kind: DefKind, id: DefId) {
        let mut drop_key = false;
        if let Some(mut hits) = self.system_name_index.get_mut(name) {
            hits.retain(|h| !(h.kind == kind && h.id == id));
            drop_key = hits.is_empty();
        }
        if drop_key {
            self.system_name_index.remove(name);
        }
    }

    /// Re-sync the system name index after a registry entry's live domain
    /// changes. `was_live_system` is the entry's live-system state BEFORE the
    /// mutation; `now_domain` is its domain AFTER.
    fn sync_system_index(
        &self,
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
            self.system_index_add(name, kind, id);
        } else if was_live_system && !now_system {
            self.system_index_remove(name, kind, id);
        }
    }
}

// ============================================================================
// RegistryState — read API (design §9 Phase B step 4)
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

impl RegistryState {
    /// The live value of one `(key, kind)` identity, any domain.
    fn live_entry(&self, sn: &McSpaceName, kind: DefKind) -> Option<DefValue> {
        self.live_entry_in(sn, kind, DomainFilter::Any)
    }

    /// The live value of one `(key, kind)` identity restricted to a domain.
    fn live_entry_in(
        &self,
        sn: &McSpaceName,
        kind: DefKind,
        filter: DomainFilter,
    ) -> Option<DefValue> {
        let ids = self.key_to_id.get(sn)?;
        for id in ids.iter() {
            if let Some(e) = self.arena.get(id) {
                if e.kind == kind && filter_matches(&e.domain, filter) {
                    return e.data.clone();
                }
            }
        }
        None
    }

    /// Enumerate every live definition of `kind` under `filter`.
    ///
    /// Sorted by (uri, ident) so the result is deterministic across runs:
    /// the arena is a DashMap whose iteration order varies per process (each
    /// run seeds its own hash keys), which previously made the order of
    /// component / module / interface registration during lapper building —
    /// and therefore the order in which fresh symbol ids are allocated —
    /// nondeterministic.
    fn enumerate(&self, kind: DefKind, filter: DomainFilter) -> Vec<(McSpaceName, DefValue)> {
        let mut keyed: Vec<((Arc<str>, String), (McSpaceName, DefValue))> = self
            .arena
            .iter()
            .filter(|e| e.kind == kind && e.data.is_some() && filter_matches(&e.domain, filter))
            .map(|e| {
                let sn = e.sn.clone();
                (
                    (sn.uri_string(), sn.ident.to_string()),
                    (sn, e.data.clone().unwrap()),
                )
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        keyed.into_iter().map(|(_, item)| item).collect()
    }

    /// The [`DefId`] of a live `(key, kind)` identity, any domain. Needed by
    /// callers that address a def by id (host links of function templates).
    fn def_id(&self, sn: &McSpaceName, kind: DefKind) -> Option<DefId> {
        let ids = self.key_to_id.get(sn)?;
        for id in ids.iter() {
            if let Some(e) = self.arena.get(id) {
                if e.kind == kind && e.data.is_some() {
                    return Some(*id);
                }
            }
        }
        None
    }

    /// Look up a component by its `McSpaceName` (any domain).
    fn get_component(&self, sn: &McSpaceName) -> Option<Arc<McComponent>> {
        match self.live_entry(sn, DefKind::Component)? {
            DefValue::Component(c) => Some(c),
            _ => None,
        }
    }

    /// Look up a module by its `McSpaceName` (any domain).
    fn get_module(&self, sn: &McSpaceName) -> Option<Arc<McModule>> {
        match self.live_entry(sn, DefKind::Module)? {
            DefValue::Module(m) => Some(m),
            _ => None,
        }
    }

    /// Look up an interface by its `McSpaceName` (any domain).
    fn get_interface(&self, sn: &McSpaceName) -> Option<Arc<McInterface>> {
        match self.live_entry(sn, DefKind::Interface)? {
            DefValue::Interface(i) => Some(i),
            _ => None,
        }
    }

    /// Look up an enum by its `McSpaceName` (any domain).
    fn get_enum(&self, sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
        match self.live_entry(sn, DefKind::Enum)? {
            DefValue::Enum(e) => Some(e),
            _ => None,
        }
    }

    /// Look up a define by its `McSpaceName` (any domain).
    fn get_define(&self, sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
        match self.live_entry(sn, DefKind::Define)? {
            DefValue::Define(d) => Some(d),
            _ => None,
        }
    }

    /// Look up a function-template addressing entry by its qualified key
    /// `(uri, "HOST.NAME")` (design §12.1). The func-addressing surface of
    /// Phase 4: exercised by the `func_entries_mirror_host_funcs_across_reload`
    /// test today; Phase 7/9 diff work reads it.
    fn get_func(&self, sn: &McSpaceName) -> Option<FuncDef> {
        match self.live_entry(sn, DefKind::Func)? {
            DefValue::Func(f) => Some(f),
            _ => None,
        }
    }

    /// Every live function-template entry of a host def (design §12.1
    /// addressing) — mirrors the host's own `funcs` table, so callers can
    /// assert consistency between the two. Same consumers as [`insert`]'s host
    /// func registration.
    fn funcs_of_host(&self, sn: &McSpaceName, host_kind: DefKind) -> Vec<(McSpaceName, FuncDef)> {
        let Some(host_id) = self.def_id(sn, host_kind) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in self.arena.iter() {
            if let Some(DefValue::Func(f)) = &e.data {
                if f.host == host_id {
                    out.push((e.sn.clone(), f.clone()));
                }
            }
        }
        out
    }

    /// Look up a component by its `McSpaceName` in the project (workspace)
    /// domain.
    fn get_workspace_component(&self, sn: &McSpaceName) -> Option<Arc<McComponent>> {
        match self.live_entry_in(sn, DefKind::Component, DomainFilter::Project)? {
            DefValue::Component(c) => Some(c),
            _ => None,
        }
    }

    /// Look up a module by its `McSpaceName` in the project (workspace)
    /// domain.
    fn get_workspace_module(&self, sn: &McSpaceName) -> Option<Arc<McModule>> {
        match self.live_entry_in(sn, DefKind::Module, DomainFilter::Project)? {
            DefValue::Module(m) => Some(m),
            _ => None,
        }
    }

    /// Look up an interface by its `McSpaceName` in the project (workspace)
    /// domain.
    fn get_workspace_interface(&self, sn: &McSpaceName) -> Option<Arc<McInterface>> {
        match self.live_entry_in(sn, DefKind::Interface, DomainFilter::Project)? {
            DefValue::Interface(i) => Some(i),
            _ => None,
        }
    }

    /// Look up an enum by its `McSpaceName` in the project (workspace) domain.
    fn get_workspace_enum(&self, sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
        match self.live_entry_in(sn, DefKind::Enum, DomainFilter::Project)? {
            DefValue::Enum(e) => Some(e),
            _ => None,
        }
    }

    /// Look up a define by its `McSpaceName` in the project (workspace)
    /// domain.
    fn get_workspace_define(&self, sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
        match self.live_entry_in(sn, DefKind::Define, DomainFilter::Project)? {
            DefValue::Define(d) => Some(d),
            _ => None,
        }
    }

    /// Every live system-library identity whose display-form name is `name`,
    /// in kind-priority order (component → module → interface → enum →
    /// define).
    ///
    /// O(1) via the system name index for the common display-form match; a
    /// miss falls back to a segment-structure-equivalent scan (e.g. curly vs
    /// dotted idents) so the pre-index semantics are preserved exactly.
    fn system_name_hits(&self, name: &str) -> Vec<SystemNameHit> {
        if let Some(hits) = self.system_name_index.get(name) {
            let mut hits = hits.value().clone();
            hits.sort_by_key(|h| kind_priority(h.kind));
            return hits;
        }
        // Rare: no display-form entry — segment-form equivalent idents (curly
        // vs dot) still match under `are_equivalent`, mirroring the consumers.
        let query = crate::McIds::from(name);
        let mut hits: Vec<SystemNameHit> = self
            .arena
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

    /// The identity + live value of one [`DefId`] — direct arena access for
    /// the system name index hits (the caller has already resolved the key).
    fn live_entry_by_id(&self, id: DefId) -> Option<(McSpaceName, DefValue)> {
        self.arena.get(&id).and_then(|e| {
            let def = e.data.clone()?;
            Some((e.sn.clone(), def))
        })
    }

    /// Resolve a definition identity to its live class value, in kind-priority
    /// order (component → module → interface → enum — the same ordering as
    /// the P3/P4 `find_scoped_by_name` in `db/resolve/policy.rs`). O(1)
    /// identity lookup covering project and system-lib defs alike; the
    /// Phase 6 visibility-table hit path resolves through here, and a miss
    /// keeps the caller's scope-chain fallback intact.
    fn cmie_by_identity(&self, sn: &McSpaceName) -> Option<McCMIE> {
        let ids = self.key_to_id.get(sn)?;
        for kind in [
            DefKind::Component,
            DefKind::Module,
            DefKind::Interface,
            DefKind::Enum,
        ] {
            for id in ids.iter() {
                let Some(entry) = self.arena.get(id) else {
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

    /// Does the system library (not the workspace) define this identity, as
    /// any class kind (component / module / interface / enum)?
    fn system_contains(&self, sn: &McSpaceName) -> bool {
        let Some(ids) = self.key_to_id.get(sn) else {
            return false;
        };
        for id in ids.iter() {
            if let Some(e) = self.arena.get(id) {
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
    fn kind_of(&self, sn: &McSpaceName) -> Option<DefKind> {
        for kind in [
            DefKind::Module,
            DefKind::Component,
            DefKind::Interface,
            DefKind::Enum,
            DefKind::Define,
        ] {
            if self.live_entry(sn, kind).is_some() {
                return Some(kind);
            }
        }
        None
    }

    /// Every live definition whose file uri contains `prefix` — the lib-ledger
    /// symbol collection in `mcb_load_lib` (replaces the ten physical-table
    /// prefix scans). Function-template entries are host members, not
    /// top-level definitions, so they stay out of the ledger.
    fn spacenames_by_uri_prefix(&self, prefix: &str) -> Vec<McSpaceName> {
        self.arena
            .iter()
            .filter(|e| e.data.is_some() && e.kind != DefKind::Func && e.sn.uri.contains(prefix))
            .map(|e| e.sn.clone())
            .collect()
    }
}

// ============================================================================
// T6 — def content fingerprint (defspace-id-core-plan G6 / architecture D15.2)
// ============================================================================
//
// A stable-reproducible, declaration-level hash of a def's live data. D15.2
// takes the "currently stable-reproducible serialization approximation": the
// same def parsed twice must hash the same — across runs and processes (fixed
// FNV-1a constants, no RandomState) — so a checkpoint diff can classify a
// same-`DefId` content edit as `Modified` (the M4 content-blind gap closer).
// Strict canonical serialization (seed 4, §14.4) upgrades the approximation
// later without touching the consumers (the hash value itself is opaque).
//
// Scope note (honest boundary): the fingerprint covers the def's own
// declaration surface — identity, the pin table (components / interfaces, in
// source declaration order), module io ports and body declarations (the
// parse artifact's instance table carries both), params, attrs, funcs and
// layout. It deliberately stops at expression / statement internals: module
// body connections are circuit content owned by the instance layer's
// checkpoint diff (M3 / dianlu-tree), so a connection edit is reported
// there, not here. Source spans (byte positions) are excluded everywhere —
// an edit elsewhere in a file must never re-hash unrelated defs. Instance
// payloads are classified by variant only (never recursed), so retargeting a
// body instance under the same name is likewise instance-layer content.

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a_update(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// The leading discriminant of a Debug-formatted enum — the variant name
/// before any payload. Used only on small enums whose payloads are leaf
/// values; heavier payloads are classified by the dedicated matches below.
fn variant_tag<T: std::fmt::Debug>(value: &T) -> String {
    let s = format!("{value:?}");
    let head = s
        .find(|c: char| matches!(c, '(' | '{' | ' ' | '}'))
        .unwrap_or(s.len());
    s[..head].to_string()
}

/// Span-free discriminant of an [`McInstance`] — variant only, payloads are
/// never formatted (component/module templates and span maps would make a
/// Debug pass both heavy and position-dependent).
fn mc_inst_tag(inst: &crate::semantic::mc_inst::McInstance) -> &'static str {
    use crate::semantic::mc_inst::McInstance::*;
    match inst {
        Label(_) => "Label",
        List(_) => "List",
        Bus(_) => "Bus",
        BusRef { .. } => "BusRef",
        Interface(_) => "Interface",
        Component(_) => "Component",
        Module(_) => "Module",
        Unresolved { .. } => "Unresolved",
        Pins => "Pins",
        PinId(_) => "PinId",
        Attr(_) => "Attr",
        Func(_) => "Func",
        EnumVal { .. } => "EnumVal",
    }
}

/// Content fingerprint of a live def — FNV-1a over its declaration lines.
fn content_fingerprint(def: &DefValue) -> u64 {
    let mut h = FNV_OFFSET_BASIS;
    for line in declaration_lines(def) {
        h = fnv1a_update(h, line.as_bytes());
        h = fnv1a_update(h, b"\n");
    }
    h
}

/// Deterministic declaration lines of a def (see the fingerprint section
/// comment for the covered surface). Ordered containers keep their iteration
/// order (BTreeMap / declaration Vec), so the same def always yields the
/// same line set.
fn declaration_lines(def: &DefValue) -> Vec<String> {
    let mut lines = Vec::new();
    let push_pin_lines = |pins: &crate::semantic::component::mc_pins::McPins,
                          lines: &mut Vec<String>| {
        for pid in &pins.decl_order {
            match pins.pins.get(pid) {
                Some(p) => lines.push(format!(
                    "pin {pid} {:?} {} {} {}",
                    p.iotype,
                    p.names.join("|"),
                    p.active_low,
                    p.is_nc
                )),
                None => lines.push(format!("pin {pid}")),
            }
        }
    };
    let push_param_lines = |params: &crate::semantic::basic::mc_param::McParamDeclares,
                            lines: &mut Vec<String>| {
        for pd in params.iter() {
            lines.push(format!(
                "param {} {}",
                pd.display_name(),
                variant_tag(&pd.kind)
            ));
        }
    };
    let push_attr_lines = |attrs: &crate::semantic::component::mc_attr::McAttributes,
                           lines: &mut Vec<String>| {
        for a in attrs.iter() {
            lines.push(format!("attr {} {} {}", a.no, a.id, a.values.len()));
        }
    };
    let push_func_lines = |funcs: &crate::semantic::mc_func::McFunctions,
                           lines: &mut Vec<String>| {
        for f in funcs.iter() {
            lines.push(format!(
                "func {} {} {} {}",
                f.name,
                f.returns.kind_str(),
                f.params.len(),
                f.stmts.len()
            ));
        }
    };
    let push_inst_lines = |insts: &crate::semantic::mc_inst::McInstances,
                           lines: &mut Vec<String>| {
        for (name, (io, inst)) in insts.iter_with_iotype() {
            lines.push(format!("inst {name} {io:?} {}", mc_inst_tag(inst)));
        }
    };
    match def {
        DefValue::Component(c) => {
            lines.push(format!("def component {} {}", c.name, c.uri));
            push_pin_lines(&c.pins, &mut lines);
            push_param_lines(&c.params, &mut lines);
            push_attr_lines(&c.attrs, &mut lines);
            push_func_lines(&c.funcs, &mut lines);
            push_inst_lines(&c.insts, &mut lines);
            lines.push(format!(
                "layout {:?}|{:?}|{:?}|{:?}",
                c.layout.left, c.layout.right, c.layout.top, c.layout.bottom
            ));
            lines.push(format!("cond {} {}", c.cond_pins.len(), c.cond_attrs.len()));
        }
        DefValue::Module(m) => {
            lines.push(format!("def module {} {}", m.name, m.uri));
            push_param_lines(&m.params, &mut lines);
            push_func_lines(&m.funcs, &mut lines);
            push_inst_lines(&m.insts, &mut lines);
            lines.push(format!("stmts {}", m.stmts.len()));
        }
        DefValue::Interface(i) => {
            lines.push(format!("def interface {} {}", i.name, i.uri));
            push_pin_lines(&i.pins, &mut lines);
            push_param_lines(&i.params, &mut lines);
            push_attr_lines(&i.attrs, &mut lines);
            lines.push(format!("roles {}", i.roles.len()));
        }
        DefValue::Enum(e) => {
            lines.push(format!("def enum {} {}", e.name, e.uri));
            for v in &e.values {
                lines.push(format!("enum-val {}", v.name));
            }
        }
        DefValue::Define(d) => {
            lines.push(format!("def define {} {}", d.name, d.uri));
            push_attr_lines(&d.attrs, &mut lines);
        }
        DefValue::Func(f) => {
            lines.push(format!("func-entry {} {}", f.host, f.name));
        }
    }
    lines
}

// ============================================================================
// Free-function API — served by the active world's registry
// ============================================================================

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
    let outcome = active().insert(sn, &domain, &def);
    write_physical(sn, &domain, def);
    outcome
}

/// Remove every definition of any kind whose defining file matches `uri`.
/// The registry keeps each identity as a tombstone (deleted ≠ never existed);
/// the physical tables drop the entries.
pub fn remove_by_uri(uri: &str) {
    active().remove_by_uri(uri);
    remove_by_uri_from(&workspace::WORKSPACE.components, uri);
    remove_by_uri_from(&workspace::WORKSPACE.modules, uri);
    remove_by_uri_from(&workspace::WORKSPACE.interfaces, uri);
    remove_by_uri_from(&workspace::WORKSPACE.enums, uri);
    remove_by_uri_from(&workspace::WORKSPACE.defines, uri);
}

/// Remove every definition whose defining file is one of `uris`
/// (third-party-lib unload sweep).
pub fn remove_by_uris(uris: &HashSet<String>) {
    active().remove_by_uris(uris);
    remove_by_uris_from(&workspace::WORKSPACE.components, uris);
    remove_by_uris_from(&workspace::WORKSPACE.modules, uris);
    remove_by_uris_from(&workspace::WORKSPACE.interfaces, uris);
    remove_by_uris_from(&workspace::WORKSPACE.enums, uris);
    remove_by_uris_from(&workspace::WORKSPACE.defines, uris);
}

/// Full process reset: drop every registered identity and its data in the
/// active world's registry. Used by the full state clear
/// (`clear_state(ClearScope::Full)`); the append-only identity journal and
/// the checkpoint journal both start over with a clean slate.
pub fn clear_all() {
    active().clear_all();
}

/// Capture a versioned snapshot of the whole registry — every registered
/// identity, live and tombstoned — and append it to the journal (design §10).
/// The daemon/RPC capture entry: unconditional, so a consumer can always
/// re-baseline "the definition space as of now" (e.g. `defs.checkpoint`).
pub fn checkpoint() -> Checkpoint {
    active().checkpoint()
}

/// The loader seam entry over the active world: stamp exactly one version
/// when the just-finished load / remove / world-switch round mutated the
/// registry, and nothing on a no-op round (O1).
pub fn checkpoint_if_changed() -> Option<Checkpoint> {
    active().checkpoint_if_changed()
}

/// The most recently stamped checkpoint of the active world (journal tail).
pub fn latest_checkpoint() -> Option<Checkpoint> {
    active().latest_checkpoint()
}

/// Diff the active world's definition space between a retained journal
/// version and the live state, without stamping a new version (design §10
/// `diff_defs`). `Err` when `from_version` fell out of the sliding window —
/// the caller must re-baseline with [`checkpoint`].
pub fn diff_since(from_version: u64) -> Result<Vec<DefChange>, String> {
    active().diff_since(from_version)
}

// ── Unified definition view (any domain, one registry) ──

/// Look up a component by its `McSpaceName` (any domain).
pub fn get_component(sn: &McSpaceName) -> Option<Arc<McComponent>> {
    active().get_component(sn)
}

/// Look up a module by its `McSpaceName` (any domain).
pub fn get_module(sn: &McSpaceName) -> Option<Arc<McModule>> {
    active().get_module(sn)
}

/// Look up an interface by its `McSpaceName` (any domain).
pub fn get_interface(sn: &McSpaceName) -> Option<Arc<McInterface>> {
    active().get_interface(sn)
}

/// Look up an enum by its `McSpaceName` (any domain).
pub fn get_enum(sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
    active().get_enum(sn)
}

/// Look up a define by its `McSpaceName` (any domain).
pub fn get_define(sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
    active().get_define(sn)
}

/// The [`DefId`] of a live `(key, kind)` identity, any domain. Needed by
/// callers that address a def by id (host links of function templates).
pub fn def_id(sn: &McSpaceName, kind: DefKind) -> Option<DefId> {
    active().def_id(sn, kind)
}

/// The stable [`DefMemberId`] of a live def member (T4 registry-owned
/// account ledger). Component pins and module io ports are the two member
/// kinds an instance `PointId` references by id; the ledger survives
/// re-parses (merged by name in `register` / module instantiation), so a
/// mid-table insert across def edits never shifts later members' ids
/// (invariant C). `None` when the def is not a registered live identity or
/// the name is not a live member.
pub fn def_member_id_of(sn: &McSpaceName, kind: DefKind, name: &str) -> Option<DefMemberId> {
    active().def_member_id_of(sn, kind, name)
}

/// T4 (M1b): merge a module's just-built port table into the module def's
/// registry-owned port ledger. Called from module instantiation once the
/// port table is finalized; see [`RegistryState::sync_module_ports`].
pub(crate) fn sync_module_ports(sn: &McSpaceName, ports: &[(String, String)]) {
    active().sync_module_ports(sn, ports);
}

/// Look up a function-template addressing entry by its qualified key
/// `(uri, "HOST.NAME")` (design §12.1).
#[allow(dead_code)]
pub fn get_func(sn: &McSpaceName) -> Option<FuncDef> {
    active().get_func(sn)
}

/// Every live function-template entry of a host def (design §12.1 addressing).
#[allow(dead_code)]
pub fn funcs_of_host(sn: &McSpaceName, host_kind: DefKind) -> Vec<(McSpaceName, FuncDef)> {
    active().funcs_of_host(sn, host_kind)
}

/// Enumerate every live component definition (any domain).
pub fn all_components() -> Vec<(McSpaceName, Arc<McComponent>)> {
    peel_components(active().enumerate(DefKind::Component, DomainFilter::Any))
}

/// Enumerate every live module definition (any domain).
pub fn all_modules() -> Vec<(McSpaceName, Arc<McModule>)> {
    peel_modules(active().enumerate(DefKind::Module, DomainFilter::Any))
}

/// Enumerate every live interface definition (any domain).
pub fn all_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> {
    peel_interfaces(active().enumerate(DefKind::Interface, DomainFilter::Any))
}

/// Enumerate every live enum definition (any domain).
pub fn all_enums() -> Vec<(McSpaceName, Arc<McEnumDef>)> {
    peel_enums(active().enumerate(DefKind::Enum, DomainFilter::Any))
}

/// Enumerate every live define definition (any domain).
pub fn all_defines() -> Vec<(McSpaceName, Arc<McDefineDef>)> {
    peel_defines(active().enumerate(DefKind::Define, DomainFilter::Any))
}

// ── Workspace-only view ──

/// Look up a component by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_component(sn: &McSpaceName) -> Option<Arc<McComponent>> {
    active().get_workspace_component(sn)
}

/// Look up a module by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_module(sn: &McSpaceName) -> Option<Arc<McModule>> {
    active().get_workspace_module(sn)
}

/// Look up an interface by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_interface(sn: &McSpaceName) -> Option<Arc<McInterface>> {
    active().get_workspace_interface(sn)
}

/// Look up an enum by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_enum(sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
    active().get_workspace_enum(sn)
}

/// Look up a define by its `McSpaceName` in the project (workspace) domain.
pub fn get_workspace_define(sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
    active().get_workspace_define(sn)
}

/// Enumerate every project (workspace) component definition.
pub fn workspace_components() -> Vec<(McSpaceName, Arc<McComponent>)> {
    peel_components(active().enumerate(DefKind::Component, DomainFilter::Project))
}

/// Enumerate every project (workspace) module definition.
pub fn workspace_modules() -> Vec<(McSpaceName, Arc<McModule>)> {
    peel_modules(active().enumerate(DefKind::Module, DomainFilter::Project))
}

/// Enumerate every project (workspace) interface definition.
pub fn workspace_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> {
    peel_interfaces(active().enumerate(DefKind::Interface, DomainFilter::Project))
}

/// Enumerate every project (workspace) enum definition.
pub fn workspace_enums() -> Vec<(McSpaceName, Arc<McEnumDef>)> {
    peel_enums(active().enumerate(DefKind::Enum, DomainFilter::Project))
}

/// Enumerate every project (workspace) define definition.
pub fn workspace_defines() -> Vec<(McSpaceName, Arc<McDefineDef>)> {
    peel_defines(active().enumerate(DefKind::Define, DomainFilter::Project))
}

// ── System-library-only view (P5 visibility) ──

/// Every live system-library identity whose display-form name is `name`, in
/// kind-priority order (component → module → interface → enum → define).
pub fn system_name_hits(name: &str) -> Vec<SystemNameHit> {
    active().system_name_hits(name)
}

/// The identity + live value of one [`DefId`] — direct arena access for the
/// system name index hits (the caller has already resolved the key).
pub fn live_entry_by_id(id: DefId) -> Option<(McSpaceName, DefValue)> {
    active().live_entry_by_id(id)
}

/// Resolve a definition identity to its live class value, in kind-priority
/// order (component → module → interface → enum — the same ordering as the
/// P3/P4 `find_scoped_by_name` in `db/resolve/policy.rs`).
pub fn cmie_by_identity(sn: &McSpaceName) -> Option<McCMIE> {
    active().cmie_by_identity(sn)
}

/// Does the system library (not the workspace) define this identity, as any
/// class kind (component / module / interface / enum)?
pub fn system_contains(sn: &McSpaceName) -> bool {
    active().system_contains(sn)
}

/// Enumerate every system-library component definition (P5 visibility).
pub fn system_components() -> Vec<(McSpaceName, Arc<McComponent>)> {
    peel_components(active().enumerate(DefKind::Component, DomainFilter::System))
}

/// Enumerate every system-library module definition (P5 visibility).
pub fn system_modules() -> Vec<(McSpaceName, Arc<McModule>)> {
    peel_modules(active().enumerate(DefKind::Module, DomainFilter::System))
}

/// Enumerate every system-library interface definition (P5 visibility).
pub fn system_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> {
    peel_interfaces(active().enumerate(DefKind::Interface, DomainFilter::System))
}

/// Enumerate every system-library enum definition (P5 visibility).
pub fn system_enums() -> Vec<(McSpaceName, Arc<McEnumDef>)> {
    peel_enums(active().enumerate(DefKind::Enum, DomainFilter::System))
}

/// The kind of a live definition under `sn`, if any — any domain, module
/// priority (mirrors the old lib-ledger `contains_key` chain).
pub fn kind_of(sn: &McSpaceName) -> Option<DefKind> {
    active().kind_of(sn)
}

/// Every live definition whose file uri contains `prefix` — the lib-ledger
/// symbol collection in `mcb_load_lib` (replaces the ten physical-table
/// prefix scans).
pub fn spacenames_by_uri_prefix(prefix: &str) -> Vec<McSpaceName> {
    active().spacenames_by_uri_prefix(prefix)
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
    /// T6 (G6 / D15.2): the content fingerprint of the live data at the
    /// checkpoint (`None` for a tombstone). [`diff_versions`] compares it for
    /// two live snapshots of one [`DefId`] to classify a same-identity
    /// content edit as `Modified` — the M4 content-blind gap closer.
    #[serde(default)]
    pub fingerprint: Option<u64>,
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DefChange {
    pub id: DefId,
    pub kind: DefChangeKind,
    /// The identity description on the older side (`None` when added).
    pub before: Option<RegistryEntrySnapshot>,
    /// The identity description on the newer side (`None` when removed).
    pub after: Option<RegistryEntrySnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DefChangeKind {
    /// Not usable at t1 (unregistered or tombstoned), live at t2.
    Added,
    /// Live at t1, not usable at t2 (tombstoned or unregistered).
    Removed,
    /// Live on both sides, but kind / key / domain changed.
    Modified,
}

/// Diff two checkpoints (design §10): every def whose identity-set or
/// liveness changed between them, ordered by [`DefId`].
///
/// - **Added**: not usable at `t1` (unregistered or tombstoned) → live at `t2`.
/// - **Removed**: live at `t1` → not usable at `t2` (tombstoned or unregistered).
/// - **Modified**: live on both sides, but kind / key / domain changed, or the
///   live content fingerprint differs (T6 — a same-identity content edit).
///
/// Unchanged defs (same description, same liveness, same fingerprint) do not
/// appear.
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
                        || before_e.domain != after_e.domain
                        // T6 (G6): same identity, different live content —
                        // the fingerprint closes the M4 content-blind gap
                        // (kind/key/domain alone missed a same-id re-parse).
                        || before_e.fingerprint != after_e.fingerprint)
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

/// One system-library definition captured by [`RegistryState::snapshot_system`]
/// for a world switch. The value is the live [`DefValue`] (func entries are
/// excluded — a snapshot restore re-derives them from their host via
/// `register_host_funcs`).
#[derive(Clone)]
pub struct SystemDefSnapshot {
    pub sn: McSpaceName,
    pub kind: DefKind,
    pub domain: LoadDomain,
    pub def: DefValue,
}

// ============================================================================
// Physical-table helpers (compatibility materialization)
// ============================================================================

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

    /// These tests mutate the process-wide registry (insert / remove / full
    /// checkpoint snapshots) while parse-flow tests may wipe it entirely via
    /// `mcc_init_no_lib` (full state clear). Both sides serialize on the
    /// crate-wide parse lock so a concurrent full clear can never land inside
    /// a checkpoint window and corrupt the diff under test.
    use crate::db::infra::init::MCC_TEST_PARSE_LOCK;

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

    /// An enum def with an explicit value list — the T6 fingerprint tests
    /// edit the content between two checkpoints under one identity.
    fn enum_def_with_values(name: &str, uri: &str, values: &[&str]) -> DefValue {
        DefValue::Enum(Arc::new(McEnumDef {
            name: crate::McIds::from(name),
            span: [0, 3],
            values: values
                .iter()
                .map(|v| McEnumValue {
                    name: crate::McIds::from(*v),
                    span: [0, 3],
                })
                .collect(),
            uri: uri.to_string(),
        }))
    }

    /// T6 (G6) gold assertion: a same-`DefId` content edit is reported as
    /// `Modified` — the fingerprint closes the M4 content-blind gap (kind /
    /// key / domain alone cannot tell a re-parse with an edited body apart
    /// from a byte-identical one). Mirrors the real flow: register v1,
    /// checkpoint, then a re-parse tombstone + revive of the same key keeps
    /// the id while the content changes; a byte-identical revive stays quiet.
    #[test]
    fn diff_reports_same_id_content_edit_as_modified() {
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
        const NAME: &str = "T6_FP_GOLD";
        const URI: &str = "/sys/t6_fp_gold.mc";
        let sn = McSpaceName {
            ident: crate::McIds::from(NAME),
            uri: uri_intern(URI),
        };
        let system = LoadDomain::SystemLib("mcode".into());

        // v1: one enum value. Register + checkpoint the pre-edit world.
        assert_eq!(
            insert(&sn, system.clone(), enum_def_with_values(NAME, URI, &["A"])),
            InsertOutcome::Inserted
        );
        let t1 = checkpoint();
        let fp_v1 = t1
            .entries
            .iter()
            .find(|e| e.uri == URI)
            .expect("v1 is checkpointed")
            .fingerprint
            .expect("live snapshot carries a fingerprint");

        // Content edit under the same identity: a re-parse tombstone then
        // revives the key — the DefId survives, the content does not.
        remove_by_uri(URI);
        assert_eq!(
            insert(
                &sn,
                system.clone(),
                enum_def_with_values(NAME, URI, &["A", "B"])
            ),
            InsertOutcome::Inserted
        );
        let t2 = checkpoint();

        // Same id, both sides alive, fingerprint differs → one Modified.
        let edit_diff = diff_versions(&t1, &t2);
        let fp_changes: Vec<&DefChange> = edit_diff
            .iter()
            .filter(|c| matches!(c.kind, DefChangeKind::Modified))
            .collect();
        assert_eq!(
            fp_changes.len(),
            1,
            "the content edit must surface as exactly one Modified: {:?}",
            edit_diff
        );
        let change = fp_changes[0];
        let before = change.before.as_ref().unwrap();
        let after = change.after.as_ref().unwrap();
        assert!(before.alive);
        assert!(after.alive);
        assert_eq!(
            before.id, after.id,
            "the revive keeps the identity — this is a content edit, not a new def"
        );
        assert_ne!(fp_v1, 0);

        // A byte-identical revive (same content, same id) must stay quiet.
        remove_by_uri(URI);
        assert_eq!(
            insert(&sn, system, enum_def_with_values(NAME, URI, &["A", "B"])),
            InsertOutcome::Inserted
        );
        let t3 = checkpoint();
        let noop_diff = diff_versions(&t2, &t3);
        let unchanged: Vec<&DefChange> = noop_diff
            .iter()
            .filter(|c| matches!(c.kind, DefChangeKind::Modified))
            .collect();
        assert!(
            unchanged.is_empty(),
            "a byte-identical revive is not a content edit: {:?}",
            noop_diff
        );

        // Leave no residue for parallel tests.
        remove_by_uri(URI);
    }

    /// The system name index must stay exactly in sync with the registry's
    /// live system segment across the mutation points: fresh insert, tombstone
    /// (lib unload sweep), tombstone revival (re-load), and the workspace-first
    /// project shadow. Uses a unique name/uri so parallel lib tests are never
    /// disturbed.
    #[test]
    fn system_name_index_tracks_live_system_entries() {
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
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

    /// T4 invariant C (defspace-id-core-plan M1): the component pin ledger is
    /// registry-owned and merges by name across re-parses — a mid-table pin
    /// insert never shifts the later pins' member ids (the instance `PointId`
    /// stability D13 rests on this), vanished pins are tombstoned, and
    /// survivors keep their id across a remove + revive cycle. The second
    /// payload mirrors a def edit between two builds.
    #[test]
    fn component_pin_ledger_merges_by_name_across_reparse() {
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
        const URI: &str = "/t4/pin_ledger_gold.mc";
        const NAME: &str = "T4_PIN_LEDGER_GOLD";
        use crate::semantic::basic::mc_paramd::McParamDeclares;
        use crate::semantic::common::IOType;
        use crate::semantic::component::mc_attr::McAttributes;
        use crate::semantic::component::mc_layout::McLayout;
        use crate::semantic::component::mc_pins::{McPin, McPins};
        use crate::semantic::component::McComponent;
        use crate::semantic::mc_func::McFunctions;
        use crate::semantic::mc_inst::McInstances;
        use crate::McIds;

        let comp_with = |pins: &[&str]| {
            let mut mp = McPins::new();
            for pid in pins {
                mp.pins.insert(
                    pid.to_string(),
                    McPin {
                        iotype: IOType::In,
                        id: pid.to_string(),
                        names: vec![pid.to_string()],
                        values: Arc::new(vec![]),
                        active_low: false,
                        is_nc: false,
                    },
                );
                mp.decl_order.push(pid.to_string());
            }
            Arc::new(McComponent {
                name: McIds::from(NAME),
                params: McParamDeclares::new(),
                pins: mp,
                attrs: McAttributes::new(),
                funcs: McFunctions::new(),
                insts: McInstances::new(),
                layout: McLayout {
                    left: vec![],
                    right: vec![],
                    top: vec![],
                    bottom: vec![],
                },
                uri: URI.to_string(),
                cond_pins: vec![],
                cond_attrs: vec![],
                span: 0..0,
                anon_counter: 0,
            })
        };

        let sn = McSpaceName {
            ident: McIds::from(NAME),
            uri: uri_intern(URI),
        };
        let project = LoadDomain::Project;
        let member_id = |name: &str| def_member_id_of(&sn, DefKind::Component, name);

        // v1: pins 1,2,3 — ids follow declaration order (0,1,2).
        assert_eq!(
            insert(
                &sn,
                project.clone(),
                DefValue::Component(comp_with(&["1", "2", "3"]))
            ),
            InsertOutcome::Inserted
        );
        assert_eq!(member_id("1"), Some(DefMemberId(0)));
        assert_eq!(member_id("2"), Some(DefMemberId(1)));
        assert_eq!(member_id("3"), Some(DefMemberId(2)));

        // Def edit: pin "2a" inserted mid-table, "2" vanished. The survivors
        // ("1", "3") must keep their ids; "2" retires as a tombstone; the new
        // pin appends at the high-water mark.
        remove_by_uri(URI);
        assert_eq!(
            insert(
                &sn,
                project.clone(),
                DefValue::Component(comp_with(&["1", "2a", "3"]))
            ),
            InsertOutcome::Inserted
        );
        assert_eq!(member_id("1"), Some(DefMemberId(0)), "survivor id stable");
        assert_eq!(
            member_id("3"),
            Some(DefMemberId(2)),
            "later member id stable"
        );
        assert_eq!(member_id("2"), None, "vanished pin retired");
        assert_eq!(member_id("2a"), Some(DefMemberId(3)), "new pin appends");

        // A third edit restoring "2" re-declares it with a fresh id — the
        // identity-safe form of a rename (the old id is never reused).
        remove_by_uri(URI);
        assert_eq!(
            insert(
                &sn,
                project.clone(),
                DefValue::Component(comp_with(&["1", "2", "2a", "3"]))
            ),
            InsertOutcome::Inserted
        );
        assert_eq!(member_id("1"), Some(DefMemberId(0)));
        assert_eq!(
            member_id("2"),
            Some(DefMemberId(4)),
            "re-declared name gets a fresh id"
        );
        assert_eq!(member_id("2a"), Some(DefMemberId(3)));
        assert_eq!(member_id("3"), Some(DefMemberId(2)));

        // Leave no residue for parallel tests.
        remove_by_uri(URI);
    }

    /// Phase 9 golden diff: a def appearing (Added), shadowing across worlds
    /// (Modified), disappearing (Removed) and reviving (Added) must be
    /// reported per stable [`DefId`], and the touched files must be
    /// answerable. Assertions filter by this test's uri because lib tests run
    /// in parallel and share the registry.
    #[test]
    fn checkpoint_diff_reports_add_remove_modify() {
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
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
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
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
