// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase C (storage back-migration) of the dianlu-tree refactor (implementation
//! plan §9 C / design §4, D6/D7): [`NodeArena`] — the arena storage layer.
//!
//! The modelling tree (`McModuleInst`) is a non-recursive object whose
//! structure lives in this flat arena: `HashMap<NodeId, Node>` plus a `root`
//! and parent/children edges. Nodes carry only integer ids and flat data — no
//! Rust references — so the arena is snapshot-able / serializable (design §4),
//! offers O(1) node access by id, child lists, and a deterministic level-order
//! walk, and is the sole structural store the flatten / export / viz walks
//! consume.
//!
//! The arena is laid down **incrementally** by the construction-time builder
//! (Phase C S3, [`InstantiationBuilder`](crate::instant::mc_mod::builder::InstantiationBuilder)):
//! each `add_component` / `add_submodule` / port / vector interning appends the
//! child edge through [`NodeArena::add_child_grouped`]. The instance content
//! for those children (the modelling-layer values) lives in the companion
//! [`InstanceStore`](crate::instant::inststore::InstanceStore).

use std::collections::{HashMap, VecDeque};

use crate::instant::identity::NodeId;

/// The four node kinds of the circuit arena (design §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A module instance (the circuit root is also a `Module`).
    Module,
    /// A component / device instance.
    Device,
    /// An io port of a module.
    Port,
    /// A vector grouping node (declared `c[1:2]`).
    Vector,
}

/// One arena node (design §4): integer identity plus flat data, no references.
///
/// Phase C S3-D cleanup: the node carries only the structural fields the
/// arena consumers read (TreeView walks `kind` + `children`/`parent` edges;
/// instance content — the class def, pins, vector shape — resolves from the
/// companion [`InstanceStore`](crate::instant::inststore::InstanceStore)).
/// The `def`/`pins`/`shape` payload an earlier step wrote here had no
/// production readers and was removed.
///
/// `PartialEq` is derived for equality assertions on the arena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The node's per-build identity.
    pub id: NodeId,
    /// Node kind.
    pub kind: NodeKind,
    /// Parent node id; `None` only for the root.
    pub parent: Option<NodeId>,
    /// Child node ids in deterministic tree order: ports, vectors,
    /// components, then sub-modules — matching the identity-resume walk
    /// (dianlu.rs `resume_module` / builder.rs `resume_tree`).
    pub children: Vec<NodeId>,
    /// Display name (instance name / port name / vector base).
    pub name: String,
}

/// Arena storage for one circuit: `HashMap<NodeId, Node>` plus a root, with
/// parent/children edges forming the tree view (design §4 / D6).
///
/// `PartialEq` is derived for the Phase C S3 cross-check (see [`Node`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeArena {
    nodes: HashMap<NodeId, Node>,
    root: NodeId,
}

impl NodeArena {
    /// Construct an empty arena rooted at `root`. `pub(crate)`: the Phase C
    /// construction-time builder lays the arena down incrementally from this
    /// root (the module being built) — the arena is the sole structural store.
    pub(crate) fn new(root: NodeId) -> Self {
        NodeArena {
            nodes: HashMap::new(),
            root,
        }
    }

    /// Insert a node, replacing any node at the same id (idempotent for the
    /// construction-time path — the same node is never laid down twice).
    pub(crate) fn insert(&mut self, node: Node) {
        self.nodes.insert(node.id, node);
    }

    /// Mutable node access by id (construction-time parent fixing: a
    /// sub-module node is laid down by its own builder with a `None` parent
    /// and the parent builder rewires it when the sub-module is added).
    pub(crate) fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Append `child` to `parent`'s children in the deterministic grouped tree
    /// order — ports, vectors, components, then sub-modules — matching the
    /// identity-resume walk (dianlu.rs `resume_module`). Idempotent: a child
    /// already present is left in place (sub-module re-entry / resume).
    ///
    /// Phase C S3: the construction-time builder appends through here, so the
    /// arena is the sole structural store from build time on.
    pub(crate) fn add_child_grouped(&mut self, parent: NodeId, child: NodeId, kind: NodeKind) {
        // Idempotency + insertion position computed against immutable reads
        // (no borrow conflict with the final `get_mut`).
        let children: Vec<NodeId> = self.children(parent).unwrap_or(&[]).to_vec();
        if children.contains(&child) {
            return;
        }
        // Each new node is inserted at the end of its own group's run, so a
        // group keeps its creation order while the inter-group layout stays
        // ports, vectors, devices, sub-modules (the post-freeze walk's order).
        let pos = match kind {
            NodeKind::Port => children
                .iter()
                .take_while(|c| {
                    self.node(**c)
                        .map(|n| n.kind == NodeKind::Port)
                        .unwrap_or(false)
                })
                .count(),
            NodeKind::Vector => children
                .iter()
                .take_while(|c| {
                    self.node(**c)
                        .map(|n| matches!(n.kind, NodeKind::Port | NodeKind::Vector))
                        .unwrap_or(false)
                })
                .count(),
            NodeKind::Device => children
                .iter()
                .position(|c| {
                    self.node(*c)
                        .map(|n| n.kind == NodeKind::Module)
                        .unwrap_or(false)
                })
                .unwrap_or(children.len()),
            NodeKind::Module => children.len(),
        };
        if let Some(node) = self.nodes.get_mut(&parent) {
            node.children.insert(pos, child);
        }
    }
}

/// Accessor surface — consumed by the flatten / export / viz walks and the
/// [`TreeView`](crate::instant::inststore::TreeView) structure queries.
impl NodeArena {
    /// The circuit root node id.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// O(1) node access by id.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Child ids of `id` in deterministic tree order.
    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.nodes.get(&id).map(|n| n.children.as_slice())
    }

    /// Parent of `id` (`None` for the root).
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(&id).and_then(|n| n.parent)
    }

    /// Total node count (root + all descendants).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the arena has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Level-order (BFS) walk from the root — a deterministic flat traversal
    /// over the whole tree (design §4). Exercised by the arena unit tests;
    /// consumers needing a full walk (rather than a kind-filtered
    /// `children` pass) can use it directly.
    pub fn iter_level_order(&self) -> impl Iterator<Item = &Node> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut queue = VecDeque::new();
        queue.push_back(self.root);
        while let Some(id) = queue.pop_front() {
            if let Some(node) = self.nodes.get(&id) {
                out.push(node);
                queue.extend(node.children.iter().copied());
            }
        }
        out.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lay down the sample circuit directly (the Phase C construction-time
    /// pattern: `insert` each node, then `add_child_grouped` for the edges):
    ///
    /// ```text
    /// main (module, root)
    /// ├─ VDD (port)
    /// ├─ c   (vector, shape [2])
    /// ├─ c1  (device)
    /// └─ ldo (module)
    ///    ├─ VIN (port)
    ///    └─ cap1 (device)
    /// ```
    fn sample_arena() -> NodeArena {
        let root = NodeId(1);
        let mut arena = NodeArena::new(root);
        let (vdd, c, c1, ldo, vin, cap1) = (
            NodeId(2),
            NodeId(3),
            NodeId(4),
            NodeId(5),
            NodeId(6),
            NodeId(7),
        );
        // The module root is a first-class arena node (the construction-time
        // builder lays it down in mc_mod/builder.rs before any child edge).
        arena.insert(Node {
            id: root,
            kind: NodeKind::Module,
            parent: None,
            children: Vec::new(),
            name: "main".to_string(),
        });
        // Port / vector / device / module nodes: leaf groups first (children
        // edges added via `add_child_grouped`), then the sub-module root.
        arena.insert(Node {
            id: vdd,
            kind: NodeKind::Port,
            parent: Some(root),
            children: Vec::new(),
            name: "VDD".to_string(),
        });
        arena.insert(Node {
            id: c,
            kind: NodeKind::Vector,
            parent: Some(root),
            children: Vec::new(),
            name: "c".to_string(),
        });
        arena.insert(Node {
            id: c1,
            kind: NodeKind::Device,
            parent: Some(root),
            children: Vec::new(),
            name: "c1".to_string(),
        });
        // Sub-module subtree: its own root + a port + a device.
        arena.insert(Node {
            id: ldo,
            kind: NodeKind::Module,
            parent: Some(root),
            children: Vec::new(),
            name: "ldo".to_string(),
        });
        arena.insert(Node {
            id: vin,
            kind: NodeKind::Port,
            parent: Some(ldo),
            children: Vec::new(),
            name: "VIN".to_string(),
        });
        arena.insert(Node {
            id: cap1,
            kind: NodeKind::Device,
            parent: Some(ldo),
            children: Vec::new(),
            name: "cap1".to_string(),
        });
        // Edges, in a deliberately non-grouped order to prove `add_child_grouped`
        // regroups into ports, vectors, devices, modules.
        arena.add_child_grouped(ldo, cap1, NodeKind::Device);
        arena.add_child_grouped(ldo, vin, NodeKind::Port);
        arena.add_child_grouped(root, ldo, NodeKind::Module);
        arena.add_child_grouped(root, c1, NodeKind::Device);
        arena.add_child_grouped(root, c, NodeKind::Vector);
        arena.add_child_grouped(root, vdd, NodeKind::Port);
        arena
    }

    /// Find a node's id by name + kind (test helper; names are unique within
    /// this sample circuit).
    fn node_id_of(arena: &NodeArena, name: &str, kind: NodeKind) -> NodeId {
        arena
            .iter_level_order()
            .find(|n| n.name == name && n.kind == kind)
            .unwrap_or_else(|| panic!("node '{name}' ({kind:?}) not found in arena"))
            .id
    }

    /// A directly-constructed arena carries the full grouped structure: every
    /// node appears exactly once, `add_child_grouped` orders children
    /// ports → vectors → devices → sub-modules, parent / children edges
    /// round-trip, and the root is the module node with no parent.
    #[test]
    fn arena_direct_build_structure() {
        let arena = sample_arena();

        // 1 root + 1 port + 1 vector + 1 device + 1 sub (1 port + 1 device) = 7.
        assert_eq!(arena.len(), 7, "every node appears exactly once");

        let root_id = arena.root();
        let root = arena.node(root_id).expect("root node present");
        assert!(root.parent.is_none(), "root has no parent");
        assert_eq!(root.kind, NodeKind::Module);
        assert_eq!(root.name, "main");
        assert_eq!(
            root.children,
            vec![
                node_id_of(&arena, "VDD", NodeKind::Port),
                node_id_of(&arena, "c", NodeKind::Vector),
                node_id_of(&arena, "c1", NodeKind::Device),
                node_id_of(&arena, "ldo", NodeKind::Module),
            ],
            "children order: ports, vectors, components, sub-modules"
        );

        // Port node.
        let vdd = arena
            .node(node_id_of(&arena, "VDD", NodeKind::Port))
            .unwrap();
        assert_eq!(vdd.kind, NodeKind::Port);
        assert_eq!(vdd.parent, Some(root_id));

        // Vector node.
        let vec = arena
            .node(node_id_of(&arena, "c", NodeKind::Vector))
            .unwrap();
        assert_eq!(vec.kind, NodeKind::Vector);
        assert_eq!(vec.parent, Some(root_id));

        // Device node.
        let c1 = arena
            .node(node_id_of(&arena, "c1", NodeKind::Device))
            .unwrap();
        assert_eq!(c1.kind, NodeKind::Device);
        assert_eq!(c1.parent, Some(root_id));

        // Sub-module subtree.
        let ldo = arena
            .node(node_id_of(&arena, "ldo", NodeKind::Module))
            .unwrap();
        let vin = arena
            .node(node_id_of(&arena, "VIN", NodeKind::Port))
            .unwrap();
        let cap1 = arena
            .node(node_id_of(&arena, "cap1", NodeKind::Device))
            .unwrap();
        assert_eq!(vin.parent, Some(ldo.id));
        assert_eq!(cap1.parent, Some(ldo.id));
        assert_eq!(ldo.children, vec![vin.id, cap1.id]);

        // Parent/children round-trip for every node.
        for node in arena.iter_level_order() {
            if let Some(pid) = node.parent {
                let p = arena.node(pid).expect("parent node present");
                assert!(
                    p.children.contains(&node.id),
                    "node {} appears in its parent's children",
                    node.name
                );
            }
        }

        // Level-order walk visits every node exactly once.
        let visited: Vec<&Node> = arena.iter_level_order().collect();
        assert_eq!(visited.len(), 7, "level-order visits every node once");
        assert_eq!(
            visited[0].name, "main",
            "root is the first level-order node"
        );
    }

    /// `add_child_grouped` is idempotent: re-adding an existing child leaves
    /// the children list unchanged (the construction-time sub-module re-entry /
    /// resume path relies on this).
    #[test]
    fn arena_add_child_grouped_is_idempotent() {
        let mut arena = sample_arena();
        let root_id = arena.root();
        let before = arena.children(root_id).unwrap().to_vec();
        // Re-add every root child exactly as it is laid down.
        for cid in &before {
            let node = arena.node(*cid).unwrap();
            arena.add_child_grouped(root_id, *cid, node.kind);
        }
        assert_eq!(
            arena.children(root_id).unwrap(),
            before.as_slice(),
            "re-adding an existing child is a no-op"
        );
        assert_eq!(arena.len(), 7, "no duplicate nodes laid down");
    }
}
