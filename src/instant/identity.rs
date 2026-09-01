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

use crate::instant::lane::NetId;
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

/// The role of an auto-named instance (plan §9 G item 5 / §115): the role
/// half of the "source span + role" identity key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoNameRole {
    /// Real anonymous device (reference-designator style `_C1`, `_R2`).
    Normal,
    /// Internal isolation node (`@_phantom_<name>_<n>`, never a device).
    Phantom,
    /// Stub for an unrecognized class name (`@?<name>_<n>`).
    Stub,
}

/// The stable identity anchor of an auto-named instance: source span (the
/// construction byte offset) + role. User-written names carry no anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AutoAnchor {
    /// The naming role (which prefix family generated the name).
    pub role: AutoNameRole,
    /// Construction site byte offset — the source-span half of the key.
    /// Phantom/stub isolation nodes report 0 (no distinguishable anchor).
    pub offset: u32,
}

/// The identity key for a child instance (plan §9 G item 5 / §115): real
/// anonymous devices anchor by (role, construction offset) instead of their
/// counter name, so the same construction site always yields the same id —
/// a rebuild of the same circuit (or a sibling appended after it) never
/// renumbers existing devices.
///
/// The anchor key records the name of the device that owns it, so a rebuild
/// recognizes the same device across builds (same name, same offset → reuse
/// the anchor) while an iterated call emitting several instances from one
/// statement (same offset, different counter names) falls back to the name
/// path for the siblings. Nodes without an anchor (user-written names,
/// phantom/stub isolation nodes) always use the name path.
///
/// Construction (`add_component` / `add_submodule`) and resume
/// (`resume_tree` / `resume_module`) must call this against the same registry
/// state so both sides reproduce the same key for the same node; the owner
/// record is written here so both sides stay consistent.
pub fn anchored_child_key(
    reg: &mut IdentityRegistry,
    module_path: &str,
    name: &str,
    anchor: Option<AutoAnchor>,
) -> String {
    if let Some(a) = anchor {
        if a.role == AutoNameRole::Normal {
            let key = format!("{module_path}.normal@{}", a.offset);
            match reg.anchor_owner(&key) {
                // Fresh anchor: claim it for this device.
                None => {
                    reg.anchor_owner.insert(key.clone(), name.to_string());
                    return key;
                }
                // The same device across a rebuild: reuse the anchor key.
                Some(owner) if owner == name => return key,
                // Another device claims the offset (iterated sibling):
                // fall back to the name path.
                Some(_) => {}
            }
        }
    }
    format!("{module_path}.{name}")
}

/// Per-build identity registry: deterministic `path ↔ NodeId` interning with
/// append-only id records and tombstone semantics (design §4 / D6). Since
/// Phase G (D9/D10) the registry is a persistent per-world field: it also
/// interns labeled net names (`label ↔ NetId`, D9) so a circuit's named nets
/// carry stable ids across rebuilds, and its alive set feeds the per-circuit
/// checkpoints (design §11.5.1).
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
    /// D9 net identity: net label (the net's name attribute) → current
    /// `NetId`. A tombstoned label keeps its stale id here; [`Self::intern_net`]
    /// overwrites it with a fresh id on re-interning (rename = tombstone +
    /// fresh id, the same discipline as nodes).
    net_by_label: HashMap<String, NetId>,
    /// Append-only net id → label records. Tombstoned net ids keep their
    /// record (a deleted net is distinguishable from one that never existed).
    net_label_of: Vec<(NetId, String)>,
    /// Deleted net ids (tombstones) — never reused.
    net_tombstones: HashSet<NetId>,
    /// Next net id to allocate (monotonic, starts at 1 — `NetId(0)` is never
    /// allocated by the registry, so the derived space head stays free).
    next_net: u32,
    /// Anchor key (`main.ldo.normal@<offset>`) → the name of the device that
    /// currently owns it (plan §9 G item 5). Lets [`anchored_child_key`]
    /// distinguish "same device across a rebuild" (same name, reuse the
    /// anchor) from "iterated sibling" (different name, same offset — name
    /// path instead).
    anchor_owner: HashMap<String, String>,
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
            net_by_label: HashMap::new(),
            net_label_of: Vec::new(),
            net_tombstones: HashSet::new(),
            next_net: 1,
            anchor_owner: HashMap::new(),
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

    /// The name of the device that currently owns `anchor_key`, if any.
    pub fn anchor_owner(&self, anchor_key: &str) -> Option<&str> {
        self.anchor_owner.get(anchor_key).map(String::as_str)
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

    // ========================================================================
    // D9 net identity — labeled nets get a persistent `NetId` (the label is
    // the net's name attribute; same label = same net). Same tombstone
    // discipline as node ids: ids monotonic, never reused, delete is a
    // tombstone.
    // ========================================================================

    /// Deterministic interning of a net label: return the `NetId` for `label`,
    /// allocating a fresh id on first sight. A re-interned (tombstoned) label
    /// gets a **fresh** id — a renamed net is a different net object, exactly
    /// as a renamed node is.
    pub fn intern_net(&mut self, label: &str) -> NetId {
        if let Some(&id) = self.net_by_label.get(label) {
            if !self.net_tombstones.contains(&id) {
                return id;
            }
        }
        let id = NetId(self.next_net);
        self.next_net += 1;
        self.net_by_label.insert(label.to_string(), id);
        self.net_label_of.push((id, label.to_string()));
        self.net_tombstones.remove(&id);
        id
    }

    /// Current `NetId` for `label`, if alive (not tombstoned).
    pub fn net_id_of(&self, label: &str) -> Option<NetId> {
        self.net_by_label
            .get(label)
            .copied()
            .filter(|id| !self.net_tombstones.contains(id))
    }

    /// The label of `id` (latest record; a tombstoned net keeps its label).
    pub fn net_label_of(&self, id: NetId) -> Option<&str> {
        self.net_label_of
            .iter()
            .rev()
            .find(|(i, _)| *i == id)
            .map(|(_, l)| l.as_str())
    }

    /// Every live (label, id) pair, sorted by label — the snapshot of what the
    /// registry currently names.
    pub fn live_nets(&self) -> Vec<(String, NetId)> {
        let mut out: Vec<(String, NetId)> = self
            .net_by_label
            .iter()
            .filter(|(_, id)| !self.net_tombstones.contains(id))
            .map(|(l, id)| (l.clone(), *id))
            .collect();
        out.sort();
        out
    }

    /// Tombstone every interned label not present in `active` — a net whose
    /// name disappeared from the circuit is deleted (its id is never reused;
    /// a re-appearing name allocates a fresh id).
    pub fn reconcile_net_labels(&mut self, active: &HashSet<String>) {
        let stale: Vec<NetId> = self
            .net_by_label
            .iter()
            .filter(|(l, id)| !active.contains(*l) && !self.net_tombstones.contains(id))
            .map(|(_, id)| *id)
            .collect();
        for id in stale {
            self.delete_net(id);
        }
    }

    /// Tombstone `id`: its label record stays, its id is never reused.
    pub fn delete_net(&mut self, id: NetId) {
        self.net_tombstones.insert(id);
    }

    /// The next id the net allocator will hand out. Unlabeled nets carry no
    /// stable key, so they receive build-scoped ids from here (past the
    /// interned range) — their cross-build identity comes from the checkpoint
    /// net snapshots + overlap matching (D9), never from the id itself.
    pub fn next_net_id(&self) -> NetId {
        NetId(self.next_net)
    }

    /// Every alive (canonical path, node id) pair — the node half of a
    /// checkpoint's alive set (design §11.5.1).
    pub fn alive_paths(&self) -> Vec<(String, NodeId)> {
        self.path_to_id
            .iter()
            .filter(|(_, id)| !self.tombstones.contains(id))
            .map(|(p, id)| (p.clone(), *id))
            .collect()
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
