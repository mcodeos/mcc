// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase C1 of the dianlu-tree refactor (implementation plan §9 C / design
//! §4, D1/D6): per-build node identity — `IdentityRegistry`, the
//! deterministic `path ↔ NodeId` interning table with tombstone semantics.
//!
//! The modelling tree (`McModuleInst`) is a recursive ownership tree whose
//! nodes carry `name + Rust reference` — no stable identity, so re-entered
//! sub-modules produce dangling references, cross-tree references are
//! unstable, and nothing can be incrementally rebuilt or serialized. Phase C1
//! (identity first) attaches a per-build [`NodeId`] companion field to every
//! node and interns its canonical path (`main.ldo.c1`, member names, not
//! positional indices) in this registry, so *within one build* the same path
//! always yields the same ID.
//!
//! Discipline (shared with the def layer ledger, defspace invariant C):
//! - IDs are allocated monotonically and **never reused**;
//! - deletion is a **tombstone** — the path record stays (append-only), the
//!   ID stays reserved, and re-interning the path allocates a fresh ID, so a
//!   deleted node is distinguishable from one that never existed;
//! - [`Self::resume`] reloads an existing frozen tree's `(path, id)` pairs
//!   when a finished sub-module is lifted back into a builder (re-entry),
//!   keeping re-instantiated products on the same IDs.
//!
//! Cross-build persistence is out of scope here — the registry travels with
//! `CircuitWorld` (Phase G, D10); Phase C1 only guarantees per-build
//! determinism (same path → same ID within one instantiation).

use std::collections::{HashMap, HashSet};

/// Stable per-build node identity (design §4: arena node id).
///
/// `0` is reserved as [`NodeId::UNASSIGNED`]; real allocations start at 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    /// Sentinel for nodes not yet registered (e.g. freshly constructed
    /// `McComponentInst` / `McModuleInst` before `add_component` /
    /// `add_submodule` interns them).
    pub const UNASSIGNED: NodeId = NodeId(0);

    /// The reserved sentinel value.
    pub const fn is_unassigned(self) -> bool {
        self.0 == 0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "N{}", self.0)
    }
}

/// The canonical identity of one circuit (one instantiation of one entry
/// module) — the per-build namespace of the registry.
///
/// Different circuit keys = different physical objects (`main.c2` of two
/// different entries are different nodes). Phase G promotes the registry to
/// a multi-circuit namespace that survives across builds; per-build (Phase
/// C1) exactly one key is in use.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CircuitKey {
    /// Canonical entry file URI of the circuit (the module's `def_uri`).
    pub entry_uri: String,
    /// Top-level module name (`main`, `POWER`, ...).
    pub top: String,
}

impl CircuitKey {
    /// A circuit key from the top-level module instance.
    pub fn new(entry_uri: &str, top: &str) -> Self {
        CircuitKey {
            entry_uri: entry_uri.to_string(),
            top: top.to_string(),
        }
    }
}

impl Default for CircuitKey {
    fn default() -> Self {
        CircuitKey {
            entry_uri: String::new(),
            top: String::new(),
        }
    }
}

/// Per-build identity registry: deterministic `path ↔ NodeId` interning with
/// append-only id records and tombstone semantics (design §4 / D6).
#[derive(Debug, Clone)]
pub struct IdentityRegistry {
    /// The circuit this registry namespaces.
    circuit: CircuitKey,
    /// Canonical path → current node id. A tombstoned path keeps its stale id
    /// here; [`Self::intern`] overwrites it with a fresh id on re-interning.
    path_to_id: HashMap<String, NodeId>,
    /// Append-only id → path records. Tombstoned ids keep their record (a
    /// deleted node is distinguishable from one that never existed).
    id_to_path: Vec<(NodeId, String)>,
    /// Deleted node ids (tombstones) — never reused.
    tombstones: HashSet<NodeId>,
    /// Next id to allocate (monotonic, starts at 1).
    next: u32,
}

impl IdentityRegistry {
    /// Create an empty registry for `circuit`. IDs start at 1
    /// ([`NodeId::UNASSIGNED`] is never allocated).
    pub fn new(circuit: CircuitKey) -> Self {
        IdentityRegistry {
            circuit,
            path_to_id: HashMap::new(),
            id_to_path: Vec::new(),
            tombstones: HashSet::new(),
            next: 1,
        }
    }

    /// The circuit this registry namespaces.
    pub fn circuit(&self) -> &CircuitKey {
        &self.circuit
    }

    /// Deterministic interning: return the node id for `path`, allocating a
    /// fresh id on first sight.
    ///
    /// - Same path, alive → same id (per-build determinism).
    /// - Same path, tombstoned → a **fresh** id (old id never reused), and
    ///   the stale path record is overwritten (append-only history keeps the
    ///   tombstone record).
    pub fn intern(&mut self, path: &str) -> NodeId {
        if let Some(&id) = self.path_to_id.get(path) {
            if !self.tombstones.contains(&id) {
                return id;
            }
        }
        let id = NodeId(self.next);
        self.next += 1;
        self.path_to_id.insert(path.to_string(), id);
        self.id_to_path.push((id, path.to_string()));
        self.tombstones.remove(&id);
        id
    }

    /// Reload an existing `(path, id)` pair (re-entry / frozen-tree rebuild).
    ///
    /// Idempotent: re-resuming the same pair keeps the registry stable. The
    /// monotonic allocator is advanced past any resumed id so subsequent
    /// [`Self::intern`] calls never collide.
    pub fn resume(&mut self, path: &str, id: NodeId) {
        self.path_to_id.insert(path.to_string(), id);
        if !self.id_to_path.iter().any(|(i, _)| *i == id) {
            self.id_to_path.push((id, path.to_string()));
        }
        if id.0 >= self.next {
            self.next = id.0 + 1;
        }
        self.tombstones.remove(&id);
    }

    /// Current node id for `path`, if alive (not tombstoned).
    pub fn node_id_of(&self, path: &str) -> Option<NodeId> {
        self.path_to_id
            .get(path)
            .copied()
            .filter(|id| !self.tombstones.contains(id))
    }

    /// Whether `path` has ever been registered (alive or tombstoned).
    pub fn contains(&self, path: &str) -> bool {
        self.path_to_id.contains_key(path)
    }

    /// Canonical path of `id` (latest record; append-only, so a tombstoned id
    /// keeps its path and is distinguishable from "never existed").
    pub fn path_of(&self, id: NodeId) -> Option<&str> {
        self.id_to_path
            .iter()
            .rev()
            .find(|(i, _)| *i == id)
            .map(|(_, p)| p.as_str())
    }

    /// Whether `id` has been deleted (tombstone — never reused).
    pub fn is_deleted(&self, id: NodeId) -> bool {
        self.tombstones.contains(&id)
    }

    /// Tombstone `id`: its record stays, its id is never reused, and a
    /// subsequent [`Self::intern`] of the same path allocates a fresh id.
    pub fn delete(&mut self, id: NodeId) {
        self.tombstones.insert(id);
    }

    /// Number of registered paths (alive or tombstoned).
    pub fn len(&self) -> usize {
        self.path_to_id.len()
    }

    /// Whether the registry has no registrations.
    pub fn is_empty(&self) -> bool {
        self.path_to_id.is_empty()
    }
}

impl Default for IdentityRegistry {
    fn default() -> Self {
        IdentityRegistry::new(CircuitKey::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> CircuitKey {
        CircuitKey::new("/proj/main.mc", "main")
    }

    #[test]
    fn intern_same_path_same_id() {
        let mut reg = IdentityRegistry::new(key());
        let a = reg.intern("main.c1");
        let a2 = reg.intern("main.c1");
        assert_eq!(a, a2, "same path must yield the same id");
        assert_eq!(reg.node_id_of("main.c1"), Some(a));
        assert_eq!(reg.path_of(a), Some("main.c1"));
        assert!(!a.is_unassigned());
    }

    #[test]
    fn intern_monotonic_no_reordering() {
        let mut reg = IdentityRegistry::new(key());
        let a = reg.intern("main.c1");
        let b = reg.intern("main.c2");
        let c = reg.intern("main.c3");
        assert!(a < b && b < c, "ids allocate monotonically, no renumbering");
        assert_eq!((a.0, b.0, c.0), (1, 2, 3));
        assert_eq!(reg.len(), 3);
    }

    #[test]
    fn delete_tombstone_no_reuse() {
        let mut reg = IdentityRegistry::new(key());
        let a = reg.intern("main.c1");
        reg.delete(a);
        assert!(reg.is_deleted(a));
        // The tombstone keeps its path record.
        assert_eq!(reg.path_of(a), Some("main.c1"));
        // Re-interning the same path must NOT reuse the tombstoned id.
        let a2 = reg.intern("main.c1");
        assert_ne!(a, a2, "tombstoned ids are never reused");
        assert!(a2 > a, "fresh id is monotonic past the tombstone");
        assert_eq!(reg.node_id_of("main.c1"), Some(a2));
        assert!(!reg.is_deleted(a2));
    }

    #[test]
    fn resume_reloads_existing_pairs() {
        let mut reg = IdentityRegistry::new(key());
        let a = reg.intern("main.c1");
        let b = reg.intern("main.c2");
        // Re-entry: reload the frozen tree's pairs, then keep allocating.
        let mut reg2 = IdentityRegistry::new(key());
        reg2.resume("main.c1", a);
        reg2.resume("main.c2", b);
        assert_eq!(reg2.node_id_of("main.c1"), Some(a));
        assert_eq!(reg2.node_id_of("main.c2"), Some(b));
        // Same path → same id after resume.
        assert_eq!(reg2.intern("main.c2"), b);
        // Fresh paths allocate past the resumed ids.
        let c = reg2.intern("main.c3");
        assert!(c > b, "resumed allocator advances past existing ids");
        assert_eq!(c.0, 3);
        // resume is idempotent.
        reg2.resume("main.c1", a);
        assert_eq!(reg2.len(), 3);
        assert_eq!(reg2.intern("main.c1"), a);
    }

    #[test]
    fn unassigned_sentinel_is_reserved() {
        assert!(NodeId::UNASSIGNED.is_unassigned());
        let mut reg = IdentityRegistry::new(key());
        let a = reg.intern("main.c1");
        assert!(!a.is_unassigned());
        assert_ne!(a, NodeId::UNASSIGNED);
    }
}
