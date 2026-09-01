// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase C (storage back-migration) of the dianlu-tree refactor (implementation
//! plan §9 C / design §4, D6/D7): [`NodeArena`] — the arena storage layer.
//!
//! The modelling tree (`McModuleInst`) is a recursive ownership tree whose
//! nodes carry `name + Rust reference` — no stable id for global access, so
//! flatten / export / viz walks recurse by hand and nothing is reachable in
//! O(1) from an arbitrary node id. This module adds the arena as a **companion
//! data layer** rebuilt from the frozen tree ([`build_node_arena`]):
//! `HashMap<NodeId, Node>` plus a `root` and parent/children edges.
//!
//! Two-track migration (plan §9 C item 3): the tree stays authoritative and
//! every consumer keeps working unchanged; the arena offers O(1) node access
//! by id, child lists, and a deterministic level-order walk — the storage the
//! flatten / export / viz walks migrate onto in later steps, one consumer at a
//! time. Nodes carry only integer ids and flat data — no Rust references — so
//! the arena is snapshot-able / serializable (design §4).

use std::collections::{HashMap, VecDeque};

use crate::db::defregistry::{def_id, DefId, DefKind};
use crate::db::member_ledger::DefMemberId;
use crate::instant::identity::NodeId;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_mod::McModuleInst;
use crate::McSpaceName;

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
/// `#[allow(dead_code)]`: the companion layer lands ahead of its consumers by
/// design (two-track migration) — the fields are written by
/// [`build_node_arena`] and read by the flatten / export / viz walks in later
/// Phase C steps.
#[allow(dead_code)]
#[derive(Debug, Clone)]
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
    /// Definition-space id of the instance's class, when the def is
    /// registered (best-effort; `None` for ports/vectors and for defs not in
    /// the registry, e.g. test stubs).
    pub def: Option<DefId>,
    /// Device only: the component class's pin ordinals in declaration order
    /// (append-only member ledger, invariant C).
    pub pins: Vec<DefMemberId>,
    /// Vector only: optional 2D+ declared shape (1D vectors omit it).
    pub shape: Option<Vec<usize>>,
}

/// Node queries — same migration-surface rationale as [`NodeArena`]'s
/// accessors; exercised by the arena unit tests today, consumed by the
/// flatten / export / viz walks in later steps.
#[allow(dead_code)]
impl Node {
    /// Whether this node is the circuit root.
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }
}

/// Arena storage for one circuit: `HashMap<NodeId, Node>` plus a root, with
/// parent/children edges forming the tree view (design §4 / D6).
#[derive(Debug, Clone)]
pub struct NodeArena {
    nodes: HashMap<NodeId, Node>,
    root: NodeId,
}

impl NodeArena {
    fn new(root: NodeId) -> Self {
        NodeArena {
            nodes: HashMap::new(),
            root,
        }
    }

    fn insert(&mut self, node: Node) {
        self.nodes.insert(node.id, node);
    }

    fn children_push(&mut self, parent: NodeId, child: NodeId) {
        if let Some(node) = self.nodes.get_mut(&parent) {
            node.children.push(child);
        }
    }
}

/// Accessor surface — the migration target for the flatten / export / viz
/// walks in later Phase C steps. The companion layer lands ahead of its
/// consumers by design (two-track migration), so these stay partially unused
/// until the first consumer switches to arena edges.
#[allow(dead_code)]
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

    /// Level-order (BFS) walk from the root — the deterministic traversal
    /// the flatten / export / viz consumers migrate onto.
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

/// Rebuild the arena from a frozen tree (design §4 / plan §9 C item 2): walk
/// the modelling tree in the same order as the identity-resume walk and lay
/// down every node with its `(parent, children)` edges. `def` / `pins` are
/// best-effort resolutions — the arena stays a faithful structural copy of
/// the tree even when a def is not registered (e.g. test stubs).
///
/// Panics if any tree node lacks its Phase C1 companion `node_id` — the
/// frozen tree always carries one (construction interning invariant).
pub(crate) fn build_node_arena(tree: &McModuleInst) -> NodeArena {
    let root = tree
        .node_id
        .expect("Phase C1 invariant: the circuit root carries a node_id");
    let mut arena = NodeArena::new(root);
    build_module(&mut arena, tree, None);
    arena
}

/// Recursively lay one module node and its ports / vectors / components /
/// sub-modules down in the arena.
fn build_module(arena: &mut NodeArena, module: &McModuleInst, parent: Option<NodeId>) -> NodeId {
    let id = module
        .node_id
        .expect("Phase C1 invariant: a frozen module node carries a node_id");
    arena.insert(Node {
        id,
        kind: NodeKind::Module,
        parent,
        children: Vec::new(),
        name: module.name.clone(),
        def: module_def_id(module),
        pins: Vec::new(),
        shape: None,
    });

    for port in &module.ports {
        let pid = port
            .node_id
            .expect("Phase C1 invariant: a frozen port node carries a node_id");
        arena.insert(Node {
            id: pid,
            kind: NodeKind::Port,
            parent: Some(id),
            children: Vec::new(),
            name: port.name.clone(),
            def: None,
            pins: Vec::new(),
            shape: None,
        });
        arena.children_push(id, pid);
    }

    for vec in &module.vectors {
        let vid = vec
            .node_id
            .expect("Phase C1 invariant: a frozen vector node carries a node_id");
        arena.insert(Node {
            id: vid,
            kind: NodeKind::Vector,
            parent: Some(id),
            children: Vec::new(),
            name: vec.base.clone(),
            def: None,
            pins: Vec::new(),
            shape: vec.shape.clone(),
        });
        arena.children_push(id, vid);
    }

    for comp in &module.components {
        let cid = comp
            .node_id
            .expect("Phase C1 invariant: a frozen component node carries a node_id");
        arena.insert(Node {
            id: cid,
            kind: NodeKind::Device,
            parent: Some(id),
            children: Vec::new(),
            name: comp.name.clone(),
            def: component_def_id(comp),
            pins: component_pins(comp),
            shape: None,
        });
        arena.children_push(id, cid);
    }

    for sub in &module.sub_modules {
        let sub_id = build_module(arena, sub, Some(id));
        arena.children_push(id, sub_id);
    }

    id
}

/// Best-effort `DefId` of a module instance's class (module def registry).
fn module_def_id(module: &McModuleInst) -> Option<DefId> {
    def_id(
        &McSpaceName::new(&module.def.name, module.def_uri.clone()),
        DefKind::Module,
    )
}

/// Best-effort `DefId` of a component instance's class (component def
/// registry).
fn component_def_id(comp: &McComponentInst) -> Option<DefId> {
    def_id(
        &McSpaceName::new(&comp.def.name, comp.def.uri.clone()),
        DefKind::Component,
    )
}

/// Arena-driven sub-module iterator (design §4 — the tree is a view over
/// arena edges): the Module-kind `children` ids of the module's node drive
/// the traversal, and the sub-module data is fetched from the aligned tree
/// node. The two orders coincide (both are the module's build order); a
/// `debug_assert` guards the 1:1 alignment on every call. Consumers that hold
/// a `NodeArena` switch their `for sub in &inst.sub_modules` recursion to
/// this iterator (Phase C two-track migration).
pub fn arena_sub_modules<'a>(
    arena: &'a NodeArena,
    inst: &'a McModuleInst,
) -> impl Iterator<Item = &'a McModuleInst> + 'a {
    let module_id = inst
        .node_id
        .expect("Phase C1 invariant: a frozen module carries a node_id");
    let module_children: Vec<NodeId> = arena
        .children(module_id)
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|cid| {
            arena
                .node(*cid)
                .map(|n| n.kind == NodeKind::Module)
                .unwrap_or(false)
        })
        .collect();
    debug_assert_eq!(
        module_children.len(),
        inst.sub_modules.len(),
        "Phase C invariant: arena Module children align 1:1 with the tree's sub_modules"
    );
    module_children
        .into_iter()
        .zip(inst.sub_modules.iter())
        .map(|(_, sub)| sub)
}

/// Device node pins: the component class's pin ordinals in declaration order.
/// `get_all_pins()` sorts by name, so the append-only member ledger (invariant
/// C) is the declaration-order source.
fn component_pins(comp: &McComponentInst) -> Vec<DefMemberId> {
    comp.def.pins.ledger.live_members().map(|m| m.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instant::identity::IdentityRegistry;
    use crate::instant::mc_comp::McComponentInst;
    use crate::instant::mc_mod::McVectorInst;
    use crate::instant::mc_net::PortInst;
    use crate::semantic::basic::mc_paramd::McParamDeclares;
    use crate::semantic::common::{IOType, McURI};
    use crate::semantic::component::mc_attr::McAttributes;
    use crate::semantic::component::mc_layout::McLayout;
    use crate::semantic::component::mc_pins::McPins;
    use crate::semantic::component::McComponent;
    use crate::semantic::mc_func::McFunctions;
    use crate::semantic::mc_inst::McInstances;
    use crate::semantic::module::McModule;
    use crate::McIds;
    use std::sync::Arc;

    /// Minimal component def stub (no pins) for arena tests.
    fn stub_comp(name: &str) -> McComponent {
        McComponent {
            name: McIds::from(name),
            params: McParamDeclares::new(),
            pins: McPins::new(),
            attrs: McAttributes::new(),
            funcs: McFunctions::new(),
            insts: McInstances::new(),
            layout: McLayout {
                left: Vec::new(),
                right: Vec::new(),
                top: Vec::new(),
                bottom: Vec::new(),
            },
            uri: McURI::default(),
            cond_pins: Vec::new(),
            cond_attrs: Vec::new(),
            span: crate::ast::ast_semantic::Span { start: 0, end: 0 },
            anon_counter: 0,
        }
    }

    /// Build a small frozen tree with Phase C1 companion node ids:
    ///
    /// ```text
    /// main (module)
    /// ├─ VDD (port)
    /// ├─ c   (vector, shape [2])
    /// ├─ c1  (device)
    /// └─ ldo (module)
    ///    ├─ VIN (port)
    ///    └─ cap1 (device)
    /// ```
    fn sample_tree() -> McModuleInst {
        let mut reg = IdentityRegistry::new(crate::instant::identity::CircuitKey::new(
            "/proj/main.mc",
            "main",
        ));
        let mut main = McModuleInst::new("main", Arc::new(McModule::test_stub("main")));
        main.node_id = Some(reg.intern("main"));

        let mut vdd = PortInst::with_members("VDD", IOType::In, Vec::new());
        vdd.node_id = Some(reg.intern("main.VDD"));
        main.ports.push(vdd);

        main.vectors.push(McVectorInst {
            base: "c".to_string(),
            member_names: vec!["c1".to_string(), "c2".to_string()],
            member_ids: vec!["c1".to_string(), "c2".to_string()],
            shape: Some(vec![2]),
            node_id: Some(reg.intern("main.c")),
        });

        let mut c1 = McComponentInst::new("c1", Arc::new(stub_comp("CAP")));
        c1.node_id = Some(reg.intern("main.c1"));
        main.components.push(c1);

        let mut ldo = McModuleInst::new("ldo", Arc::new(McModule::test_stub("ldo")));
        ldo.node_id = Some(reg.intern("main.ldo"));
        let mut vin = PortInst::with_members("VIN", IOType::In, Vec::new());
        vin.node_id = Some(reg.intern("main.ldo.VIN"));
        ldo.ports.push(vin);
        let mut cap1 = McComponentInst::new("cap1", Arc::new(stub_comp("CAP")));
        cap1.node_id = Some(reg.intern("main.ldo.cap1"));
        ldo.components.push(cap1);
        main.sub_modules.push(ldo);

        main
    }

    /// Find a node's id by name + kind (test helper; names are unique within
    /// this sample tree).
    fn node_id_of(arena: &NodeArena, name: &str, kind: NodeKind) -> NodeId {
        arena
            .iter_level_order()
            .find(|n| n.name == name && n.kind == kind)
            .unwrap_or_else(|| panic!("node '{name}' ({kind:?}) not found in arena"))
            .id
    }

    /// The arena rebuilt from the frozen tree is isomorphic to it: every tree
    /// node (module / port / vector / device) appears exactly once, parent /
    /// children edges round-trip, and the root is the module node with no
    /// parent.
    #[test]
    fn arena_isomorphic_to_frozen_tree() {
        let tree = sample_tree();
        let arena = build_node_arena(&tree);

        // 1 root + 1 port + 1 vector + 1 device + 1 sub (1 port + 1 device) = 7.
        assert_eq!(arena.len(), 7, "every tree node appears exactly once");

        let root_id = arena.root();
        let root = arena.node(root_id).expect("root node present");
        assert!(root.is_root(), "root has no parent");
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
        assert!(vdd.def.is_none(), "ports carry no def");

        // Vector node carries its declared shape.
        let vec = arena
            .node(node_id_of(&arena, "c", NodeKind::Vector))
            .unwrap();
        assert_eq!(vec.kind, NodeKind::Vector);
        assert_eq!(vec.shape, Some(vec![2]));
        assert_eq!(vec.parent, Some(root_id));

        // Device node.
        let c1 = arena
            .node(node_id_of(&arena, "c1", NodeKind::Device))
            .unwrap();
        assert_eq!(c1.kind, NodeKind::Device);
        assert_eq!(c1.parent, Some(root_id));
        assert!(c1.pins.is_empty(), "stub def has no pins");

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

    /// `def` resolution is best-effort: unregistered stubs resolve to `None`
    /// without disturbing the structural copy.
    #[test]
    fn arena_def_resolution_is_best_effort() {
        let tree = sample_tree();
        let arena = build_node_arena(&tree);
        // Stub defs are not in the def registry — every node resolves to None
        // (devices and modules) and the arena still carries the full tree.
        for node in arena.iter_level_order() {
            assert!(
                node.def.is_none(),
                "unregistered stub def resolves to None (node '{}')",
                node.name
            );
        }
        assert_eq!(arena.len(), 7);
    }
}
