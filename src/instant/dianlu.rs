// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §12.2: DianLu — the core circuit object: one instantiation of one entry
//! module.
//!
//! The physical model is built by the pipeline (member_set → classify →
//! resolve_reference → materialize) into an instance tree (`McModuleInst`,
//! which carries the vector grouping nodes `McVectorInst` and the lane /
//! connectivity structures). A `DianLu` is the owning object for ONE such
//! instantiation: it holds the tree plus the lazily derived flat projection
//! view (`InstTable`). `flatten()` is the single one-way projection exit
//! (invariant B) — it derives the flat view from the already-built tree and
//! never re-enters instantiation, and it runs the flat electrical net checks
//! (§11.4) once.
//!
//! This replaces the previous shape where the flat build entry
//! (`mcb_pass2_flat`) re-ran the whole instantiation just to flatten — the
//! structural cause of double-instantiation (and of the GAP2 double-report
//! that diagnostic dedup then papered over). One instantiation = one DianLu;
//! tree-only consumers read [`Self::tree`], flat consumers call
//! [`Self::flatten`].

use super::arena::NodeArena;
use super::descriptions::DescriptionLayer;
use super::insttab::InstTable;
use super::lane::{collect_stmt_trunks, derive_nets, finalize_net_ids, Net, NetId, PointId, Trunk};
use super::mc_mod::McModuleInst;
use super::overlays::Overlays;
use crate::db::diagnostic::diagnostic::Diagnostic;
use crate::instant::identity::{anchored_child_key, CircuitKey, IdentityRegistry};
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::nettab::NetTableStore;
use crate::McSpaceName;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// One instantiation of one entry module — the physical model plus its flat
/// projection view (design §12.2, code name `DianLu`).
pub struct DianLu {
    /// Instance tree (modelling layer): instances, vector grouping nodes
    /// (`McVectorInst`), lanes (`ConnectionInst`) and net connectivity.
    tree: McModuleInst,
    /// Starting id for the flat table (typically 1000).
    start_id: u32,
    /// Lazily built flat projection view; `None` until the first `flatten()`.
    table: Option<InstTable>,
    /// Flat electrical net-check diagnostics (§11.4), produced once by
    /// `flatten()` and returned to the caller for logging (Phase A: DianLu
    /// never writes to the workspace diagnostic manager itself).
    net_diags: Vec<Diagnostic>,
    /// Phase C1: per-build identity registry (canonical path ↔ `NodeId`),
    /// rebuilt from the frozen tree's companion node ids on construction.
    /// Consumers resolve `node_id` by path (or vice versa) through here.
    identity: IdentityRegistry,
    /// Phase C: companion arena storage (`HashMap<NodeId, Node>` + root +
    /// parent/children edges), laid down incrementally by the construction
    /// builder and moved in here on construction. The arena is the sole
    /// structural store — `McModuleInst` carries no children Vecs.
    arena: NodeArena,
    /// Phase C S3: the instance store — every component / sub-module's
    /// modelling-layer content, keyed by arena node id. Same carrier rule as
    /// the arena: built incrementally by the construction builder, moved in on
    /// construction. The [`TreeView`](crate::instant::inststore::TreeView)
    /// resolves children through arena edges + these values.
    store: InstanceStore,
    /// Phase D: statement-level lane layer (design §11.3 ③) — one [`Trunk`]
    /// per connection statement, collected from the frozen tree on
    /// construction. The structured statement storage the derived electrical
    /// nets (union-find) and the drawing / layout walks consume.
    lanes: Vec<Trunk>,
    /// Phase D: net layer (design §11.3 ③) — the union-find equivalence
    /// classes derived from `lanes`, rebuilt on construction. Derived index,
    /// never primary storage: the projection `NetTable` stays authoritative.
    nets: Vec<Net>,
    /// §11.5.2 read-side API: point → owning net reverse index, built after
    /// `finalize_net_ids` so ids are final. Derived index, never primary
    /// storage — a point belongs to exactly one union-find net, so the map is
    /// exact. Backs [`DianLu::point_net`] / [`DianLu::point_fanout`].
    point_net: HashMap<PointId, NetId>,
    /// Phase D: the circuit-wide frozen string net-table store produced
    /// during construction (`McModuleInst` never carries `NetPoint` — the
    /// projection layer only). The flat projection and every tree-level string
    /// net consumer read their per-module tables from here, keyed by canonical
    /// module path (`main`, `main.ldo`, ...).
    net_table: Rc<RefCell<NetTableStore>>,
    /// Phase E: circuit-level derived overlay (design §3/§4, plan §9 E) —
    /// the label → net annotation (`labels`) and the `name_index` /
    /// `point_index` lookup caches (design §5 D5), derived per build from the
    /// frozen tree and its net layer. Pure annotation overlay: never
    /// participates in identity.
    overlays: Overlays,
    /// Phase G: description layer (design §12, plan §9 G) — the class
    /// template instantiations of this circuit (func expansion groups, bus
    /// groups, interface member bindings), derived per build. Content
    /// addressed: no independent identity (the same discipline as lanes).
    descriptions: DescriptionLayer,
    /// Phase F: circuit → def dependency edges (plan §9 F, design §12.6) —
    /// every definition-space resolution this instantiation performed
    /// (entry module + each class resolved at the `mcb_get_cmie` /
    /// `resolve_system` bridge), frozen at construction. The def→circuits
    /// reverse index (invalidation domain) is the CircuitWorld's (Phase G).
    circuit_deps: Vec<McSpaceName>,
}

impl DianLu {
    /// Wrap an already-instantiated tree. The model is authoritative; the flat
    /// projection is derived lazily via [`Self::flatten`]. The per-build
    /// identity registry is rebuilt from the tree's companion node ids, and
    /// the labeled nets are interned into it (D9).
    pub fn new(
        tree: McModuleInst,
        start_id: u32,
        net_store: Rc<RefCell<NetTableStore>>,
        circuit_deps: Vec<McSpaceName>,
        arena: NodeArena,
        store: InstanceStore,
    ) -> Self {
        // Phase C S3-D: resume the frozen tree's ids through the store-backed
        // view (the tree's `components` / `sub_modules` Vec fields are gone).
        let view = TreeView::new(&arena, &store);
        let mut identity = build_identity_registry(&tree, &view);
        Self::assemble(
            tree,
            start_id,
            net_store,
            circuit_deps,
            &mut identity,
            arena,
            store,
        )
    }

    /// Phase G (D10): wrap an already-instantiated tree whose node ids were
    /// interned into `registry` (a CircuitWorld-persistent registry). The
    /// frozen tree's `(path, id)` pairs are resumed into it (idempotent), the
    /// labeled nets are interned into it (D9), and the registry — now carrying
    /// the circuit's full identity — is cloned into the DianLu as its frozen
    /// view. The caller keeps the authoritative registry across rebuilds, so
    /// re-instantiation continues on the same id namespace (D1).
    pub(crate) fn new_with_registry(
        tree: McModuleInst,
        start_id: u32,
        net_store: Rc<RefCell<NetTableStore>>,
        circuit_deps: Vec<McSpaceName>,
        registry: &mut IdentityRegistry,
        arena: NodeArena,
        store: InstanceStore,
    ) -> Self {
        // Phase C S3-D: resume through the store-backed view (see `Self::new`).
        let view = TreeView::new(&arena, &store);
        resume_module(registry, &tree.name, &tree, &view);
        Self::assemble(
            tree,
            start_id,
            net_store,
            circuit_deps,
            registry,
            arena,
            store,
        )
    }

    /// Shared construction: derive the arena, lane layer, net layer (with D9
    /// persistent net ids finalized into `identity`), and overlay from the
    /// frozen tree; the DianLu keeps a clone of `identity` as its frozen view.
    fn assemble(
        tree: McModuleInst,
        start_id: u32,
        net_store: Rc<RefCell<NetTableStore>>,
        circuit_deps: Vec<McSpaceName>,
        identity: &mut IdentityRegistry,
        arena: NodeArena,
        store: InstanceStore,
    ) -> Self {
        // Phase C: the incremental arena the builder laid down during
        // construction is the sole structural store.
        let lanes = collect_stmt_trunks(&tree, &arena, &store);
        let mut nets = derive_nets(&lanes);
        finalize_net_ids(&mut nets, identity);
        // §11.5.2 read API reverse index: built after `finalize_net_ids` so the
        // ids are final (labeled nets interned, unlabeled assigned).
        let point_net: HashMap<PointId, NetId> = nets
            .iter()
            .flat_map(|n| n.points.iter().map(move |p| (*p, n.id)))
            .collect();
        let identity = identity.clone();
        // Phase C S3-D: the frozen-side overlay/description derivation resolves
        // children through the view (arena edges + store), not the tree Vecs.
        let view = TreeView::new(&arena, &store);
        let overlays = Overlays::derive(&tree, &nets, &view);
        let descriptions =
            DescriptionLayer::derive(&tree, &lanes, &overlays, &net_store.borrow(), &view);
        DianLu {
            tree,
            start_id,
            table: None,
            net_diags: Vec::new(),
            identity,
            arena,
            store,
            lanes,
            nets,
            point_net,
            net_table: net_store,
            overlays,
            descriptions,
            circuit_deps,
        }
    }

    /// The per-build identity registry (canonical path ↔ node id).
    pub fn identity(&self) -> &IdentityRegistry {
        &self.identity
    }

    /// The companion arena (design §4 / D6): `HashMap<NodeId, Node>` + root +
    /// parent/children edges laid down by the construction builder.
    ///
    /// Consumers hold the arena + store alongside the tree and read children
    /// through a [`TreeView`](crate::instant::inststore::TreeView)
    /// (design §4 — the tree is a view over arena edges + store values).
    pub fn arena(&self) -> &NodeArena {
        &self.arena
    }

    /// The instance tree (modelling layer), not the flat projection.
    pub fn tree(&self) -> &McModuleInst {
        &self.tree
    }

    /// Phase C S3: the instance store — every component / sub-module's
    /// modelling-layer content, keyed by arena node id. Consumers that hold a
    /// [`TreeView`](crate::instant::inststore::TreeView) read children
    /// through arena edges + these values instead of the tree's (now-removed)
    /// Vec fields.
    pub fn store(&self) -> &InstanceStore {
        &self.store
    }

    /// Phase D statement-level lane layer: one [`Trunk`] per connection
    /// statement (design §11.3 ③), collected from the frozen tree.
    pub fn lanes(&self) -> &[Trunk] {
        &self.lanes
    }

    /// Phase D net layer: the union-find equivalence classes derived from the
    /// lane layer (design §11.3 ③). Derived index — the projection
    /// `NetTable` remains the authoritative flat netlist.
    pub fn nets(&self) -> &[Net] {
        &self.nets
    }

    /// The §11.5.2 read-side point → net reverse index (built at assemble).
    pub(crate) fn point_net_index(&self) -> &HashMap<PointId, NetId> {
        &self.point_net
    }

    /// The circuit-wide frozen string net-table store (Phase D). Tree-level
    /// string-net consumers that hold a `DianLu` read per-module tables here,
    /// keyed by canonical module path.
    pub fn net_store(&self) -> Rc<RefCell<NetTableStore>> {
        self.net_table.clone()
    }

    /// The circuit-level derived overlay (Phase E): the label → net
    /// annotation plus the `name_index` / `point_index` lookup caches. Pure
    /// annotation layer, derived per build; consumers never mutate it.
    pub fn overlays(&self) -> &Overlays {
        &self.overlays
    }

    /// The description layer (Phase G): the class template instantiations of
    /// this circuit — func expansion groups, bus groups and interface member
    /// bindings. Content addressed (no independent identity), derived per
    /// build; consumers never mutate it.
    pub fn descriptions(&self) -> &DescriptionLayer {
        &self.descriptions
    }

    /// The circuit → def dependency edges (Phase F): every definition-space
    /// resolution this instantiation performed (entry module + each class
    /// resolved at the `mcb_get_cmie` / `resolve_system` bridge), in
    /// resolution order. Frozen at construction — the caller (CircuitWorld,
    /// Phase G) builds the def→circuits reverse index from these.
    pub fn deps(&self) -> &[McSpaceName] {
        &self.circuit_deps
    }

    /// Consume the object, discarding any flat projection.
    pub fn into_tree(self) -> McModuleInst {
        self.tree
    }

    /// Consume the object into (tree, table). Panics if `flatten` has not been
    /// called — the projection is derived exactly once, by [`Self::flatten`],
    /// so callers must project before taking the parts.
    pub fn into_parts(self) -> (McModuleInst, InstTable) {
        let table = self
            .table
            .expect("DianLu::into_parts: flatten() must run first");
        (self.tree, table)
    }

    /// The flat projection view, if `flatten` has been called.
    pub fn table(&self) -> Option<&InstTable> {
        self.table.as_ref()
    }

    /// The flat electrical net-check diagnostics (§11.4), cached by the first
    /// `flatten` and returned to the caller who owns logging. Empty until the
    /// projection has run. Read here instead of re-calling `flatten` when the
    /// projection already exists.
    pub fn net_diags(&self) -> &[Diagnostic] {
        &self.net_diags
    }

    /// One-way projection (invariant B): derive the flat `InstTable` from the
    /// already-built tree — never re-instantiate. Cached; subsequent calls are
    /// no-ops. Runs the flat electrical net checks (§11.4) once and returns
    /// their diagnostics to the caller, who owns logging (Phase A: DianLu
    /// performs no global writes; the `current_uri` context is the caller's).
    pub fn flatten(&mut self) -> Vec<Diagnostic> {
        self.flatten_with_prefix(None)
    }

    /// Like [`Self::flatten`], but marks every entry under `synthetic_prefix`
    /// (a virtual-instantiation wrapper module, e.g. `VIRT_XTAL4`) as synthetic
    /// during the projection, so the unwired/pin-count checks skip synthetic
    /// instances in a standalone component/interface view.
    pub fn flatten_with_prefix(&mut self, synthetic_prefix: Option<&str>) -> Vec<Diagnostic> {
        if self.table.is_none() {
            // Phase C: the arena drives the flatten traversal (arena children
            // edges source the sub-module order, design §4 — the tree is a
            // view over arena edges + store values).
            let mut table = InstTable::from_module_inst_with_arena(
                &self.tree,
                self.start_id,
                &self.arena,
                &self.store,
                self.net_table.clone(),
            );
            if let Some(prefix) = synthetic_prefix {
                table.mark_synthetic_by_path_prefix(prefix);
            }
            // Flat electrical checks run once during the projection (§11.4
            // flat entry); their diagnostics are returned, never logged here.
            let results = crate::semantic::validation::nets::run_net_checks(&table);
            self.net_diags =
                crate::semantic::validation::nets::net_results_to_diagnostics(&results);
            self.table = Some(table);
        }
        self.net_diags.clone()
    }
}

/// Rebuild the per-build identity registry from a frozen tree's companion
/// node ids (Phase C1): walk the modelling tree in the same order as the
/// flat projection and resume every `(canonical path, node id)` pair. The
/// result is identical to the construction-time registry — same path, same
/// id (per-build determinism).
///
/// Phase C S3-D: the walk resolves children through the store-backed `view`
/// (the tree's `components` / `sub_modules` Vec fields are gone).
fn build_identity_registry(tree: &McModuleInst, view: &TreeView) -> IdentityRegistry {
    let mut reg = IdentityRegistry::new(CircuitKey::new(&tree.def_uri.to_string(), &tree.name));
    resume_module(&mut reg, &tree.name, tree, view);
    reg
}

/// Recursive resume over one module's scope: the module node itself, its
/// ports, its vectors, its components (leaf nodes), and its sub-modules
/// (recursive). Auto-named devices resume under their "source span + role"
/// anchor key (plan §9 G item 5) so a rebuild keeps their ids stable even
/// when a sibling insertion renumbers the counter name.
///
/// Phase C S3-D: children resolve through the store-backed `view` — the
/// caller (`DianLu::new` / `new_with_registry`) holds the companion arena +
/// instance store produced by the same build, so the frozen tree's ids
/// resume exactly as the old Vec walk did.
fn resume_module(reg: &mut IdentityRegistry, path: &str, module: &McModuleInst, view: &TreeView) {
    if let Some(id) = module.node_id {
        reg.resume(path, id);
    }
    for port in &module.ports {
        if let Some(id) = port.node_id {
            reg.resume(&format!("{path}.{}", port.name), id);
        }
    }
    for vec in &module.vectors {
        if let Some(id) = vec.node_id {
            reg.resume(&format!("{path}.{}", vec.base), id);
        }
    }
    for comp in view.components(module) {
        if let Some(id) = comp.node_id {
            let key = anchored_child_key(reg, path, &comp.name, comp.anchor);
            reg.resume(&key, id);
        }
    }
    for sub in view.sub_modules(module) {
        let sub_path = format!("{path}.{}", sub.name);
        if let Some(id) = sub.node_id {
            let key = anchored_child_key(reg, path, &sub.name, sub.anchor);
            reg.resume(&key, id);
        }
        resume_module(reg, &sub_path, sub, view);
    }
}
