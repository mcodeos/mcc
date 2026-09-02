// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase C (dual-track removal, plan §9 C item 4 / design §4, §12.1):
//! [`InstanceStore`] + [`TreeView`] — the arena + instance-store view over a
//! circuit's structural storage.
//!
//! The construction-time builder lays the store down incrementally;
//! [`TreeView`] resolves `sub_modules` / `components` through arena `children`
//! edges + store values. `McModuleInst` carries no children Vecs — the arena
//! is the sole structural store and the store the sole instance-content store.

use std::collections::HashMap;
use std::rc::Rc;

use crate::instant::arena::{Node, NodeArena, NodeKind};
use crate::instant::identity::NodeId;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_mod::McModuleInst;

/// One instance's modelling-layer content (plan §9 C item 4). Content-only:
/// children live in the arena edges, never here (design §4 — minimal, flat,
/// snapshot-able nodes, no Rust references).
///
/// Values are held behind `Rc` so the shared `Rc<RefCell<InstanceStore>>`
/// construction store can hand cheap owned handles out of a transient borrow
/// (the builder's `find_component` / `find_submodule` resolve through arena
/// children + store and return the cloned `Rc`, never a deep copy or a borrow
/// past a `Ref` guard). Read accessors deref transparently.
#[derive(Debug, Clone)]
pub enum NodeInstance {
    /// A module instance (the circuit root is also a `Module`).
    Module(Rc<McModuleInst>),
    /// A component / device instance.
    Component(Rc<McComponentInst>),
}

/// The instance store — `HashMap<NodeId, NodeInstance>` holding every
/// modelling-layer instance content, keyed by the arena node id (plan §9 C
/// item 4, design §12.1). Beside the arena, the sole instance-content store;
/// the construction-time builder appends to it.
#[derive(Debug, Clone, Default)]
pub struct InstanceStore {
    instances: HashMap<NodeId, NodeInstance>,
}

impl InstanceStore {
    /// Insert (or replace) the instance content at `id`. Idempotent for
    /// sub-module re-entry (a node is added once per arena child edge).
    pub(crate) fn insert(&mut self, id: NodeId, inst: NodeInstance) {
        self.instances.insert(id, inst);
    }

    /// The instance content at `id`, if any.
    pub(crate) fn get(&self, id: NodeId) -> Option<&NodeInstance> {
        self.instances.get(&id)
    }

    /// The module instance content at `id`, if the node is a module
    /// (Phase C S3-C: derefs the stored `Rc` — callers holding a plain
    /// [`&InstanceStore`] get a reference as long as the store borrow).
    pub(crate) fn module(&self, id: NodeId) -> Option<&McModuleInst> {
        match self.get(id) {
            Some(NodeInstance::Module(m)) => Some(m.as_ref()),
            _ => None,
        }
    }

    /// The component instance content at `id`, if the node is a device
    /// (same deref rule as [`Self::module`]).
    pub(crate) fn component(&self, id: NodeId) -> Option<&McComponentInst> {
        match self.get(id) {
            Some(NodeInstance::Component(c)) => Some(c.as_ref()),
            _ => None,
        }
    }

    /// The module instance at `id` as a cheap owned `Rc` handle (Phase C
    /// S3-C: construction reads resolve through the store and clone the `Rc`
    /// out of a transient `Ref` borrow — no deep copy, no borrow past the
    /// guard).
    pub(crate) fn module_rc(&self, id: NodeId) -> Option<Rc<McModuleInst>> {
        match self.get(id) {
            Some(NodeInstance::Module(m)) => Some(m.clone()),
            _ => None,
        }
    }

    /// The component instance at `id` as a cheap owned `Rc` handle (same
    /// Phase C S3-C rationale as [`Self::module_rc`]).
    pub(crate) fn component_rc(&self, id: NodeId) -> Option<Rc<McComponentInst>> {
        match self.get(id) {
            Some(NodeInstance::Component(c)) => Some(c.clone()),
            _ => None,
        }
    }
}

/// §12.1 `TreeView` — the arena + instance-store view over the circuit's
/// structural storage. Consumers read `sub_modules` / `components` /
/// `children` / `parent` / `node` through this view instead of recursing the
/// modelling tree.
///
/// Store-backed: `sub_modules` / `components` filter the arena `children`
/// edges by node kind and resolve the instance content from the store.
pub struct TreeView<'a> {
    arena: &'a NodeArena,
    store: &'a InstanceStore,
}

impl<'a> TreeView<'a> {
    /// Arena + instance-store view over the circuit (Phase C S3): children
    /// resolve through arena edges, instance content through the store.
    pub fn new(arena: &'a NodeArena, store: &'a InstanceStore) -> Self {
        TreeView { arena, store }
    }

    /// The underlying arena.
    pub fn arena(&self) -> &'a NodeArena {
        self.arena
    }

    /// The underlying instance store.
    pub fn store(&self) -> &'a InstanceStore {
        self.store
    }

    /// The circuit root node id.
    pub fn root(&self) -> NodeId {
        self.arena.root()
    }

    /// O(1) node access by id.
    pub fn node(&self, id: NodeId) -> Option<&'a Node> {
        self.arena.node(id)
    }

    /// Child ids of `id` in deterministic tree order (ports, vectors,
    /// components, sub-modules).
    pub fn children(&self, id: NodeId) -> Option<&'a [NodeId]> {
        self.arena.children(id)
    }

    /// Parent of `id` (`None` for the root).
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.arena.parent(id)
    }

    /// Sub-module instances of `inst` in build order: the Module-kind arena
    /// children of the module's node, each resolved from the store.
    pub fn sub_modules(&self, inst: &McModuleInst) -> impl Iterator<Item = &'a McModuleInst> + 'a {
        let store: &'a InstanceStore = self.store;
        child_ids(self.arena, inst, NodeKind::Module)
            .into_iter()
            .filter_map(move |cid| store.module(cid))
    }

    /// Component instances of `inst` in build order: the Device-kind arena
    /// children of the module's node, each resolved from the store.
    pub fn components(
        &self,
        inst: &McModuleInst,
    ) -> impl Iterator<Item = &'a McComponentInst> + 'a {
        let store: &'a InstanceStore = self.store;
        child_ids(self.arena, inst, NodeKind::Device)
            .into_iter()
            .filter_map(move |cid| store.component(cid))
    }

    /// The component instance content at a node id (Phase C S3-D: the
    /// `group_products` NodeId bucketers resolve through this instead of
    /// indexing the tree's removed Vec).
    pub fn component(&self, id: NodeId) -> Option<&'a McComponentInst> {
        self.store.component(id)
    }

    /// The module instance content at a node id (same S3-D role as
    /// [`Self::component`]).
    pub fn module(&self, id: NodeId) -> Option<&'a McModuleInst> {
        self.store.module(id)
    }
}

/// The `kind`-kind arena children ids of a module node, in build order
/// (`children` edges are already grouped and creation-ordered — the
/// construction-time builder appends through `add_child_grouped`).
fn child_ids(arena: &NodeArena, inst: &McModuleInst, kind: NodeKind) -> Vec<NodeId> {
    let module_id = inst
        .node_id
        .expect("Phase C1 invariant: a frozen module carries a node_id");
    arena
        .children(module_id)
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|cid| arena.node(*cid).map(|n| n.kind == kind).unwrap_or(false))
        .collect()
}
