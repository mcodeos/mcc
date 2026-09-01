// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase C (dual-track removal, plan §9 C item 4 / design §4, §12.1):
//! [`InstanceStore`] + [`TreeView`] — the arena + instance-store view over a
//! circuit's structural storage.
//!
//! Stage 0 of the migration: the store and view types land ahead of their
//! consumers (the same two-track discipline the arena itself landed under).
//! The view is store-free at this stage — [`TreeView::sub_modules`] and
//! [`TreeView::components`] delegate to the arena zip iterators
//! ([`arena_sub_modules`](crate::instant::arena::arena_sub_modules) + the new
//! [`arena_components`](crate::instant::arena::arena_components)), which read
//! the aligned modelling-tree Vecs and `debug_assert` the 1:1 arena
//! isomorphism on every call. Stage 3 fills the store, the view resolves
//! children through arena edges + store values, and the `components` /
//! `sub_modules` Vec fields leave `McModuleInst`.

use std::collections::HashMap;

use crate::instant::arena::{arena_components, arena_sub_modules, Node, NodeArena};
use crate::instant::identity::NodeId;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_mod::McModuleInst;

/// One instance's modelling-layer content (plan §9 C item 4). Content-only:
/// children live in the arena edges, never here (design §4 — minimal, flat,
/// snapshot-able nodes, no Rust references).
///
/// `#[allow(dead_code)]`: the store lands ahead of its consumers by design
/// (two-track migration); Stage 3 constructs the variants when the builder
/// starts appending to the store.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum NodeInstance {
    /// A module instance (the circuit root is also a `Module`).
    Module(McModuleInst),
    /// A component / device instance.
    Component(McComponentInst),
}

/// The instance store — `HashMap<NodeId, NodeInstance>` holding every
/// modelling-layer instance content, keyed by the arena node id (plan §9 C
/// item 4, design §12.1). The single structural store beside the arena once
/// Phase C S3 deletes the `components` / `sub_modules` Vec fields from
/// `McModuleInst`; until then it is the construction-time sink that the
/// builder appends to.
///
/// `#[allow(dead_code)]`: same two-track rationale as [`NodeInstance`].
#[allow(dead_code)]
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

    /// The module instance content at `id`, if the node is a module.
    pub(crate) fn module(&self, id: NodeId) -> Option<&McModuleInst> {
        match self.get(id) {
            Some(NodeInstance::Module(m)) => Some(m),
            _ => None,
        }
    }

    /// The component instance content at `id`, if the node is a device.
    pub(crate) fn component(&self, id: NodeId) -> Option<&McComponentInst> {
        match self.get(id) {
            Some(NodeInstance::Component(c)) => Some(c),
            _ => None,
        }
    }
}

/// §12.1 `TreeView` — the arena + instance-store view over the circuit's
/// structural storage. Consumers read `sub_modules` / `components` /
/// `children` / `parent` / `node` through this view instead of recursing the
/// modelling-tree Vec fields.
///
/// Stage 0: store-free — `sub_modules` / `components` delegate to the arena
/// zip iterators (which read the aligned Vecs and `debug_assert` the 1:1
/// isomorphism). Stage 3: store-backed — the same methods filter arena
/// children by kind and resolve instance content from the store.
#[allow(dead_code)]
pub struct TreeView<'a> {
    arena: &'a NodeArena,
    store: Option<&'a InstanceStore>,
}

impl<'a> TreeView<'a> {
    /// Store-free view over the arena (Stage 0).
    pub(crate) fn new(arena: &'a NodeArena) -> Self {
        TreeView { arena, store: None }
    }

    /// The underlying arena.
    pub fn arena(&self) -> &'a NodeArena {
        self.arena
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

    /// Sub-module instances of `inst` in build order (Stage 0: the arena
    /// zip iterator over the aligned `sub_modules` Vec).
    pub fn sub_modules(
        &self,
        inst: &'a McModuleInst,
    ) -> impl Iterator<Item = &'a McModuleInst> + 'a {
        arena_sub_modules(self.arena, inst)
    }

    /// Component instances of `inst` in build order (Stage 0: the arena zip
    /// iterator over the aligned `components` Vec).
    pub fn components(
        &self,
        inst: &'a McModuleInst,
    ) -> impl Iterator<Item = &'a McComponentInst> + 'a {
        arena_components(self.arena, inst)
    }
}
