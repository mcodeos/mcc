// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 namespace unification — [`InstEntry`] enum, the instance-layer
//! scope units and [`ExpansionContext`] for func body expansion name
//! resolution.
//!
//! Phase 2.5 of the namespace refactoring plan.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use super::super::inststore::TreeView;
use super::super::mc_bus::McBusInst;
use super::super::mc_comp::McComponentInst;
use super::super::mc_net::{NetPoint, PortInst};
use super::super::nettab::NetTableStore;
use super::builder::InstantiationBuilder;
use super::McModuleInst;
use crate::semantic::scope::{ResolveScope, ScopeChain};

// Instance-layer scope units (§3.5)
// The units below read *instance-layer* tables (instantiation output), so
// they live here instead of `semantic::scope` — only the composition
// mechanism (`ScopeChain` / `ResolveScope`) is shared across layers.
//
// Deviation note: mechanism B resolves into [`InstEntry`], not [`NetPoint`]
// — [`InstEntry::Component`]/[`InstEntry::SubModule`] carry the recursive
// terminals that the overlay chain resolver needs; `NetPoint` is
// terminal-only and would break arbitrary-depth DOT resolution.

/// Module ports (mechanism B P1).
struct ModulePortsScope<'a> {
    ports: &'a [PortInst],
}

impl<'a> ModulePortsScope<'a> {
    fn new(ports: &'a [PortInst]) -> Self {
        Self { ports }
    }
}

impl ResolveScope<InstEntry> for ModulePortsScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        self.ports
            .iter()
            .find(|p| p.name == name)
            .map(|p| InstEntry::Port(p.net_point.clone()))
    }
}

/// Module labels (mechanism B P2).
struct ModuleLabelsScope<'a> {
    labels: &'a HashMap<String, NetPoint>,
}

impl<'a> ModuleLabelsScope<'a> {
    fn new(labels: &'a HashMap<String, NetPoint>) -> Self {
        Self { labels }
    }
}

impl ResolveScope<InstEntry> for ModuleLabelsScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        self.labels.get(name).map(|l| InstEntry::Label(l.clone()))
    }
}

/// Module component instances (mechanism B P3). Phase C S3 store-backed: the
/// scope holds `Rc` handles resolved from the instance store (the tree no
/// longer carries a `components` Vec); the resolver clones only the match.
struct ModuleComponentsScope<'a> {
    components: &'a [Rc<McComponentInst>],
}

impl<'a> ModuleComponentsScope<'a> {
    fn new(components: &'a [Rc<McComponentInst>]) -> Self {
        Self { components }
    }
}

impl ResolveScope<InstEntry> for ModuleComponentsScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        self.components
            .iter()
            .find(|c| c.name == name)
            .map(|c| InstEntry::Component(Arc::new(c.as_ref().clone())))
    }
}

/// Module sub-module instances (mechanism B P4). Phase C S3 store-backed (same
/// rationale as [`ModuleComponentsScope`]).
struct ModuleSubModulesScope<'a> {
    sub_modules: &'a [Rc<McModuleInst>],
}

impl<'a> ModuleSubModulesScope<'a> {
    fn new(sub_modules: &'a [Rc<McModuleInst>]) -> Self {
        Self { sub_modules }
    }
}

impl ResolveScope<InstEntry> for ModuleSubModulesScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        self.sub_modules
            .iter()
            .find(|s| s.name == name)
            .map(|s| InstEntry::SubModule(Arc::new(s.as_ref().clone())))
    }
}

/// Module buses (mechanism B P5) — bus members resolve to `NetPoint`s from
/// the module label table.
struct ModuleBusesScope<'a> {
    buses: &'a HashMap<String, McBusInst>,
    labels: &'a HashMap<String, NetPoint>,
}

impl<'a> ModuleBusesScope<'a> {
    fn new(buses: &'a HashMap<String, McBusInst>, labels: &'a HashMap<String, NetPoint>) -> Self {
        Self { buses, labels }
    }
}

impl ResolveScope<InstEntry> for ModuleBusesScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        let bus = self.buses.get(name)?;
        let points: Vec<NetPoint> = bus
            .members
            .iter()
            .filter_map(|m| self.labels.get(m).cloned())
            .collect();
        Some(InstEntry::Bus(points))
    }
}

// InstEntry — Pass2 instance entry types

/// Pass2 analog of [`crate::McInstance`] — resolved instance in the
/// instantiation phase.
///
/// Uses [`Arc`] for compound types ([`McComponentInst`], [`McModuleInst`])
/// so that [`resolve_chain_overlay`] can carry a sub-module across DOT-chain
/// segments without lifetime constraints.
///
/// `Port`/`Label`/`Bus` payloads are read only by the terminal-resolution
/// tests (`resolve_*_terminal`); production code matches on the variant and
/// ignores the payload (a terminal stops DOT descent).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum InstEntry {
    /// A component instance (e.g. `R1`, `U1`) — holds the actual instance
    /// for pin-level resolution.
    Component(Arc<McComponentInst>),
    /// A sub-module instance — holds the actual instance so the overlay
    /// chain resolver can descend arbitrarily deep.
    SubModule(Arc<McModuleInst>),
    /// A port connection point (terminal — no further DOT resolution)
    Port(NetPoint),
    /// A label connection point (terminal — no further DOT resolution)
    Label(NetPoint),
    /// A bus (collection of connection points; terminal)
    Bus(Vec<NetPoint>),
}

// ExpansionContext

/// Pass2 func body expansion name resolver.
///
/// Provides name resolution during component function body expansion.
/// Only the expanded instance is consulted directly: func params are
/// substituted via [`substitute_stmt`](crate::instant::mc_mod::subst), and
/// parent-scope resolution goes through the overlay-aware chain resolver
/// ([`resolve_chain_overlay`]) instead.
pub struct ExpansionContext<'a> {
    /// The component instance being expanded
    pub instance: &'a McComponentInst,
}

impl<'a> ExpansionContext<'a> {
    /// Create a new expansion context.
    pub fn new(instance: &'a McComponentInst) -> Self {
        Self { instance }
    }
}

// Overlay-aware resolution (Phase E)

/// Resolve `name` inside `tree`'s module scope with the Phase E overlay:
/// ports and components and sub-modules come from the tree; labels and buses
/// come from `scratch` when provided (a builder under construction — its
/// scratch is newer than any frozen fragment), otherwise from the store's
/// frozen fragment for `path`.
fn module_find_overlay(
    tree: &McModuleInst,
    path: &str,
    scratch: Option<(&HashMap<String, NetPoint>, &HashMap<String, McBusInst>)>,
    store: &NetTableStore,
    view: &TreeView,
    name: &str,
) -> Option<InstEntry> {
    let (labels, buses): (&HashMap<String, NetPoint>, &HashMap<String, McBusInst>) = match scratch {
        Some((l, b)) => (l, b),
        None => (store.labels_of(path), store.buses_of(path)),
    };
    // Phase C S3: the tree carries no children — components / sub-modules
    // resolve store-backed from the view (only the match is cloned).
    let comps: Vec<Rc<McComponentInst>> =
        view.components(tree).map(|c| Rc::new(c.clone())).collect();
    let subs: Vec<Rc<McModuleInst>> = view.sub_modules(tree).map(|s| Rc::new(s.clone())).collect();
    // Named local so the chain (borrowing `comps` / `subs`) drops before them.
    let chain = ScopeChain::new(vec![
        Box::new(ModulePortsScope::new(&tree.ports)),
        Box::new(ModuleLabelsScope::new(labels)),
        Box::new(ModuleComponentsScope::new(&comps)),
        Box::new(ModuleSubModulesScope::new(&subs)),
        Box::new(ModuleBusesScope::new(buses, labels)),
    ]);
    chain.resolve(name)
}

/// Recursively resolve a DOT-separated name chain with the Phase E overlay.
///
/// The top module's overlay comes from `top_labels` / `top_buses` (the
/// builder scratch — a module under construction carries its labels/buses
/// there, not in the store); a sub-module descent reads the sub-module's
/// overlay from its frozen fragment in `store` (keyed by the sub's canonical
/// path `{top_path}.{name}`), mirroring the tree-carried chain recursion of
/// the scope-chain composition.
pub(crate) fn resolve_chain_overlay(
    chain: &[String],
    top: &McModuleInst,
    top_path: &str,
    top_labels: &HashMap<String, NetPoint>,
    top_buses: &HashMap<String, McBusInst>,
    store: &NetTableStore,
    view: &TreeView,
) -> Option<InstEntry> {
    if chain.is_empty() {
        return None;
    }

    let mut current = module_find_overlay(
        top,
        top_path,
        Some((top_labels, top_buses)),
        store,
        view,
        &chain[0],
    )?;

    for seg in &chain[1..] {
        current = match &current {
            // SubModule: recurse with the sub-module's own overlay fragment.
            InstEntry::SubModule(sub) => {
                let sub_path = format!("{top_path}.{}", sub.name);
                module_find_overlay(sub, &sub_path, None, store, view, seg)?
            }
            // Component: resolve via its pins
            InstEntry::Component(comp) => {
                if let Some(pin) = comp.pins.get(seg) {
                    InstEntry::Port(pin.clone())
                } else {
                    return None;
                }
            }
            // Terminal types: Port/Label/Bus don't support further DOT resolution
            InstEntry::Port(_) | InstEntry::Label(_) | InstEntry::Bus(_) => {
                return None;
            }
        };
    }
    Some(current)
}

impl InstantiationBuilder {
    /// Phase E overlay-aware scope-chain resolution (Pass2 P0-3): the current
    /// module's labels/buses come from the builder scratch; sub-module
    /// descent reads the sub-module's overlay from the shared store.
    pub(super) fn resolve_chain(&self, chain: &[String]) -> Option<InstEntry> {
        let store = self.net_store.borrow();
        let arena = self.arena.borrow();
        let inst_store = self.store.borrow();
        let view = TreeView::new(&arena, &inst_store);
        resolve_chain_overlay(
            chain,
            &self.tree,
            &self.current_path,
            &self.labels,
            &self.buses,
            &store,
            &view,
        )
    }
}

// §7 Vector expansion matching (eval.md §7) — pure functions

/// §7 Expansion match result: the completed state of one matching layer.
#[derive(Debug, Clone)]
pub struct ExpandMatch {
    /// The pairs, kept in **lhs vector order** (§11.2 invariant 4).
    pub pairs: Vec<(NetPoint, NetPoint)>,
}

/// §11.3 Vector expansion matching (eval.md §11.3): pairs the two expanded
/// point lists, keeping both sides' declaration (vector) order.
///
/// Pairing law (interface-connect rule, ruling of 2026-09-19): the wiring
/// order is a fact declared by each side's interface/role member table, not a
/// conclusion the compiler may re-derive. Ordinal k on the two sides is the
/// SAME wire; member names are each side's local view and are **never a
/// matching criterion**. The former name-first priority that realigned
/// same-named members across different declaration orders is removed —
/// crossing between two roles (e.g. UART DCE/DTE, SPI master/slave) is
/// declared by writing the two member lists in corresponding ordinal order,
/// not repaired by name lookup.
///
/// 1. **Count correspondence**: both sides have the same total point count →
///    positional zip in declaration order (§3.1). No sorting, no name lookup:
///    both sides already carry their declared (vector) order, so the zip is
///    exactly "ordinal k = ordinal k". When both sides are fully named and
///    every pair's names differ, the zip signals D5 (bus-order check in
///    group.rs) — a hint, never a pairing input.
/// 2. **Count mismatch**: implicit auto-expansion is **forbidden** (the §7
///    explicit `*` rule: `[*cannon.UART[1:2,6], ...]` must be expanded as an
///    explicit list) — returns `None`; the caller reports the shape mismatch
///    (vec-dianlu §5.3.3: illegal ⇒ E4007, **no** broadcast / pair-by-min
///    truncation recovery).
///
/// The N:N pairing path of `create_connection` in group.rs. Replaces the
/// P2-4/P4.2 sorted-zip implementation (eval.md §11).
pub fn expand_match(lhs: &[NetPoint], rhs: &[NetPoint]) -> Option<ExpandMatch> {
    if lhs.is_empty() || rhs.is_empty() || lhs.len() != rhs.len() {
        return None;
    }

    // ── Count correspondence: positional zip in declaration order (§11.3),
    // stated by the one core both pairing faces cite (matching.rs
    // `positional_pairs`). Member names are labels, never a pairing criterion
    // (top-level rule) and never a misalignment signal - E4052 retired (design
    // doc section 6.1 D3): under the positional law a crossed writing is legal.
    let pairs: Vec<(NetPoint, NetPoint)> = super::matching::positional_pairs(lhs.len(), rhs.len())
        .into_iter()
        .map(|(l, r)| (lhs[l].clone(), rhs[r].clone()))
        .collect();
    Some(ExpandMatch { pairs })
}

#[cfg(test)]
mod expand_match_tests {
    use super::*;
    use crate::semantic::common::IOType;

    fn pt(path: &str, member: Option<&str>) -> NetPoint {
        let mut p = NetPoint::new(path, IOType::None, None);
        p.member_name = member.map(|s| s.to_string());
        p
    }

    // ── Pairing law (2026-09-19 ruling): names are never a matching
    // criterion — the zip is positional, in declaration order. ──

    #[test]
    fn mat_expand__positional_zip_in_declaration_order() {
        // SPI-like perspectives with different local names: each side's list
        // is in its own declared order and the zip is by position. Crossing
        // (MISO↔SO, MOSI↔SI) is declared by writing the lists in
        // corresponding ordinal order, not repaired here.
        let lhs = vec![
            pt("uC.SPI.1", Some("SCLK")),
            pt("uC.SPI.2", Some("CS")),
            pt("uC.SPI.3", Some("MISO")),
            pt("uC.SPI.4", Some("MOSI")),
        ];
        let rhs = vec![
            pt("flash.SPI.1", Some("SCLK")),
            pt("flash.SPI.2", Some("CS")),
            pt("flash.SPI.3", Some("SO")),
            pt("flash.SPI.4", Some("SI")),
        ];
        let m = expand_match(&lhs, &rhs).expect("equal-count zip");
        assert!(!m.pairs.iter().all(|(l, r)| l.member_name != r.member_name));
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("uC.SPI.1", "flash.SPI.1"),
                ("uC.SPI.2", "flash.SPI.2"),
                ("uC.SPI.3", "flash.SPI.3"),
                ("uC.SPI.4", "flash.SPI.4"),
            ]
        );
    }

    #[test]
    fn mat_expand__same_names_out_of_order_must_not_realign() {
        // The anti-regression lock for the ruling: identical member names on
        // swapped positions must NOT be paired by name. Ordinal k on the two
        // sides is the same wire; the all-differ zip signals D5 instead.
        let lhs = vec![pt("l.VDD", Some("VDD")), pt("l.GND", Some("GND"))];
        let rhs = vec![pt("r.GND", Some("GND")), pt("r.VDD", Some("VDD"))];
        let m = expand_match(&lhs, &rhs).expect("equal-count zip");
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, vec![("l.VDD", "r.GND"), ("l.GND", "r.VDD")]);
        assert!(m.pairs.iter().all(|(l, r)| l.member_name != r.member_name));
    }

    #[test]
    fn mat_expand__duplicate_member_names_zip_positionally() {
        // Duplicate names are irrelevant now — the zip never reads names for
        // pairing.
        let lhs = vec![pt("a.1", Some("X")), pt("a.2", Some("Y"))];
        let rhs = vec![pt("b.1", Some("X")), pt("b.2", Some("X"))];
        let m = expand_match(&lhs, &rhs).expect("equal-count zip");
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, vec![("a.1", "b.1"), ("a.2", "b.2")]);
    }

    #[test]
    fn mat_expand__missing_member_name_still_zips_positionally() {
        // A point without a member name pairs by position like any other.
        let lhs = vec![pt("a.1", Some("X")), pt("a.2", None)];
        let rhs = vec![pt("b.1", Some("X")), pt("b.2", Some("Y"))];
        let m = expand_match(&lhs, &rhs).expect("equal-count zip");
        assert_eq!(m.pairs.len(), 2);
    }

    // ── §11.3: total-count correspondence (positional zip) ──

    #[test]
    fn mat_expand__partial_name_match_is_not_repaired() {
        // One name coincides at its position, the rest differ: the zip stays
        // positional regardless — and every pair still differs, so D5 fires.
        let lhs = vec![pt("l.GND", Some("GND")), pt("l.VDD", Some("VDD"))];
        let rhs = vec![pt("r.VDD_3V3", Some("VDD_3V3")), pt("r.GND", Some("GND"))];
        let m = expand_match(&lhs, &rhs).expect("equal-count zip");
        assert!(m.pairs.iter().all(|(l, r)| l.member_name != r.member_name));
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, vec![("l.GND", "r.VDD_3V3"), ("l.VDD", "r.GND")]);
    }

    #[test]
    fn mat_expand__total_count_positional_without_names() {
        // No member names → zip in declaration order.
        let lhs = vec![pt("R1.1", None), pt("R1.2", None)];
        let rhs = vec![pt("R2.1", None), pt("R2.2", None)];
        let m = expand_match(&lhs, &rhs).expect("positional zip");
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, vec![("R1.1", "R2.1"), ("R1.2", "R2.2")]);
    }

    #[test]
    fn mat_expand__total_count_all_mismatched_signals_d5() {
        // No name matches at all → every pair's member names differ → D5 signal.
        let lhs = vec![pt("l.1", Some("A")), pt("l.2", Some("B"))];
        let rhs = vec![pt("r.1", Some("C")), pt("r.2", Some("D"))];
        let m = expand_match(&lhs, &rhs).expect("total-count zip");
        assert!(m.pairs.iter().all(|(l, r)| l.member_name != r.member_name));
    }

    // ── §7 rule 3: count mismatch → None (implicit expansion forbidden) ──

    #[test]
    fn mat_expand__count_mismatch_returns_none() {
        let lhs = vec![pt("a.1", Some("X")), pt("a.2", Some("Y"))];
        let rhs = vec![
            pt("b.1", Some("X")),
            pt("b.2", Some("Y")),
            pt("b.3", Some("Z")),
        ];
        assert!(expand_match(&lhs, &rhs).is_none());
        assert!(expand_match(&rhs, &lhs).is_none());
    }

    #[test]
    fn mat_expand__empty_side_returns_none() {
        assert!(expand_match(&[], &[pt("a.1", None)]).is_none());
    }
}

// Instance-layer scope unit tests (§3.5) — each unit is exercised with its
// own input (field-level slices/maps), so no full `McModuleInst` is required
// except where a real sub-module instance is needed for resolution.

#[cfg(test)]
mod inst_scope_tests {
    use super::*;
    use crate::instant::arena::{Node, NodeArena, NodeKind};
    use crate::instant::identity::{CircuitKey, IdentityRegistry};
    use crate::instant::inststore::{InstanceStore, NodeInstance};
    use crate::semantic::basic::mc_ids::McIds;
    use crate::semantic::basic::mc_param::McParamBindings;
    use crate::semantic::basic::mc_paramd::McParamDeclares;
    use crate::semantic::common::IOType;
    use crate::semantic::component::mc_attr::McAttributes;
    use crate::semantic::component::mc_layout::McLayout;
    use crate::semantic::component::mc_pins::McPins;
    use crate::semantic::component::McComponent;
    use crate::semantic::mc_inst::McInstances;
    use crate::semantic::module::McModule;
    use crate::{McFunctions, McURI};

    /// A `NetPoint` with no owner and no IO type (sufficient for field-level tests).
    fn np(path: &str) -> NetPoint {
        NetPoint::new(path, IOType::None, None)
    }

    /// Minimal component instance backed by an empty stub definition.
    fn comp_inst(name: &str) -> McComponentInst {
        McComponentInst {
            name: name.to_string(),
            def: Arc::new(McComponent {
                name: McIds::from("STUB"),
                params: McParamDeclares::new(),
                pins: McPins::new(),
                attrs: McAttributes::new(),
                funcs: McFunctions::new(),
                insts: McInstances::new(),
                uri: McURI::default(),
                layout: McLayout {
                    left: Vec::new(),
                    right: Vec::new(),
                    top: Vec::new(),
                    bottom: Vec::new(),
                },
                cond_pins: Vec::new(),
                cond_attrs: Vec::new(),
                span: crate::ast::sem::Span { start: 0, end: 0 },
                anon_counter: 1,
                is_abstract: false,
                variant_base: None,
                adopts: Vec::new(),
            }),
            params: McParamBindings::new(),
            raw_params: Vec::new(),
            pins: HashMap::new(),
            cond_pin_names: HashMap::new(),
            cond_pin_attrs: HashMap::new(),
            cond_attrs: Vec::new(),
            resolved_attrs: Vec::new(),
            nc_pins: Default::default(),
            nc: false,
            degraded: false,
            origin: Default::default(),
            expansion_id: None,
            node_id: None,
            anchor: None,
            cond_eval_errors: Vec::new(),
            cond_author_errors: Vec::new(),
        }
    }

    /// Component instance with pre-populated pins.
    fn comp_inst_with_pins(name: &str, pins: &[(&str, IOType)]) -> McComponentInst {
        let mut inst = comp_inst(name);
        for (pid, io) in pins {
            inst.pins.insert(
                (*pid).to_string(),
                NetPoint::with_owner(&format!("{name}.{pid}"), name, io.clone(), None),
            );
        }
        inst
    }

    /// Phase C S3 store fixture: intern `module`'s node and its direct
    /// component / sub-module children in a fresh arena + instance store,
    /// wire the arena edges, and return them. The caller builds the
    /// [`TreeView`] over the pair.
    fn store_fixture(
        module: &mut McModuleInst,
        components: Vec<McComponentInst>,
        sub_modules: Vec<McModuleInst>,
    ) -> (NodeArena, InstanceStore) {
        let mut reg = IdentityRegistry::new(CircuitKey::default());
        let mut store = InstanceStore::default();
        let root = reg.intern(&module.name);
        let mut arena = NodeArena::new(root);
        module.node_id = Some(root);
        arena.insert(Node {
            id: root,
            kind: NodeKind::Module,
            parent: None,
            children: Vec::new(),
            name: module.name.clone(),
        });
        for mut comp in components {
            let id = reg.intern(&format!("{}.{}", module.name, comp.name));
            comp.node_id = Some(id);
            arena.insert(Node {
                id,
                kind: NodeKind::Device,
                parent: Some(root),
                children: Vec::new(),
                name: comp.name.clone(),
            });
            arena.add_child_grouped(root, id, NodeKind::Device);
            store.insert(id, NodeInstance::Component(Rc::new(comp)));
        }
        for mut sub in sub_modules {
            let id = reg.intern(&format!("{}.{}", module.name, sub.name));
            sub.node_id = Some(id);
            arena.insert(Node {
                id,
                kind: NodeKind::Module,
                parent: Some(root),
                children: Vec::new(),
                name: sub.name.clone(),
            });
            arena.add_child_grouped(root, id, NodeKind::Module);
            store.insert(id, NodeInstance::Module(Rc::new(sub)));
        }
        (arena, store)
    }

    // ── Mechanism B (T = InstEntry) ──

    /// Module ports resolve to a terminal `InstEntry::Port`.
    #[test]
    fn mat_expand__module_ports_scope_resolves_port_entry() {
        let ports = vec![PortInst::new("VOUT", IOType::Out)];
        let scope = ModulePortsScope::new(&ports);
        match scope.resolve("VOUT").expect("port should resolve") {
            InstEntry::Port(p) => assert_eq!(p.path, "VOUT"),
            other => panic!("expected InstEntry::Port, got {other:?}"),
        }
        assert!(scope.resolve("VIN").is_none());
    }

    /// Module labels resolve to a terminal `InstEntry::Label`.
    #[test]
    fn mat_expand__module_labels_scope_resolves_label_entry() {
        let mut labels = HashMap::new();
        labels.insert("N_VDD".to_string(), np("N_VDD"));
        let scope = ModuleLabelsScope::new(&labels);
        match scope.resolve("N_VDD").expect("label should resolve") {
            InstEntry::Label(l) => assert_eq!(l.path, "N_VDD"),
            other => panic!("expected InstEntry::Label, got {other:?}"),
        }
        assert!(scope.resolve("N_GND").is_none());
    }

    /// Module components resolve to a recursive `InstEntry::Component` arc.
    #[test]
    fn mat_expand__module_components_scope_resolves_component_entry() {
        let components = vec![Rc::new(comp_inst_with_pins("R1", &[("1", IOType::None)]))];
        let scope = ModuleComponentsScope::new(&components);
        match scope.resolve("R1").expect("component should resolve") {
            InstEntry::Component(c) => assert_eq!(c.name, "R1"),
            other => panic!("expected InstEntry::Component, got {other:?}"),
        }
        assert!(scope.resolve("R2").is_none());
    }

    /// Module sub-modules resolve to a recursive `InstEntry::SubModule` arc.
    #[test]
    fn mat_expand__module_sub_modules_scope_resolves_submodule_entry() {
        let sub = McModuleInst::new("mcu513", Arc::new(McModule::test_stub("mcu")));
        let sub_modules = vec![Rc::new(sub)];
        let scope = ModuleSubModulesScope::new(&sub_modules);
        match scope.resolve("mcu513").expect("sub-module should resolve") {
            InstEntry::SubModule(s) => assert_eq!(s.name, "mcu513"),
            other => panic!("expected InstEntry::SubModule, got {other:?}"),
        }
        assert!(scope.resolve("mcu").is_none());
    }

    /// Module buses expand to the member `NetPoint`s via the label table;
    /// a member with no label entry is silently skipped.
    #[test]
    fn mat_expand__module_buses_scope_resolves_members_from_labels() {
        let mut buses = HashMap::new();
        buses.insert(
            "power".to_string(),
            McBusInst::new("power", vec!["VCC".to_string(), "GND".to_string()]),
        );
        let mut labels = HashMap::new();
        labels.insert("VCC".to_string(), np("N_VCC"));
        labels.insert("GND".to_string(), np("N_GND"));
        let scope = ModuleBusesScope::new(&buses, &labels);
        match scope.resolve("power").expect("bus should resolve") {
            InstEntry::Bus(points) => {
                assert_eq!(points.len(), 2);
                assert_eq!(points[0].path, "N_VCC");
                assert_eq!(points[1].path, "N_GND");
            }
            other => panic!("expected InstEntry::Bus, got {other:?}"),
        }
        assert!(scope.resolve("missing").is_none());
    }

    // ── Composition — overlay-aware DOT-chain resolution ──

    /// An empty store + scratch for the overlay resolver (no frozen
    /// fragments; labels/buses come from the passed scratch maps).
    fn empty_overlay() -> (
        NetTableStore,
        HashMap<String, NetPoint>,
        HashMap<String, McBusInst>,
    ) {
        (NetTableStore::new(), HashMap::new(), HashMap::new())
    }

    /// The overlay chain resolver descends `U1` → pin `VDD` through the
    /// store-backed component category (Phase C S3).
    #[test]
    fn mat_expand__overlay_chain_resolves_component_pin() {
        let mut m = McModuleInst::new("main", Arc::new(McModule::test_stub("main")));
        let (arena, inst_store) = store_fixture(
            &mut m,
            vec![comp_inst_with_pins("U1", &[("VDD", IOType::Power)])],
            vec![],
        );
        let view = TreeView::new(&arena, &inst_store);
        let (net_store, labels, buses) = empty_overlay();
        let chain = vec!["U1".to_string(), "VDD".to_string()];
        match resolve_chain_overlay(&chain, &m, "main", &labels, &buses, &net_store, &view)
            .expect("chain should resolve")
        {
            InstEntry::Port(p) => assert_eq!(p.path, "U1.VDD"),
            other => panic!("expected InstEntry::Port, got {other:?}"),
        }
        let missing = vec!["U1".to_string(), "GND".to_string()];
        assert!(
            resolve_chain_overlay(&missing, &m, "main", &labels, &buses, &net_store, &view)
                .is_none()
        );
    }

    /// The overlay chain resolver follows the ports → labels → components →
    /// sub_modules → buses priority: a name present in both `ports` and
    /// `components` resolves to the port; the label category comes from the
    /// scratch overlay.
    #[test]
    fn mat_expand__overlay_chain_priority_ports_over_components() {
        let mut m = McModuleInst::new("main", Arc::new(McModule::test_stub("main")));
        m.ports.push(PortInst::new("SIG", IOType::InOut));
        let (arena, inst_store) = store_fixture(
            &mut m,
            vec![comp_inst_with_pins("SIG", &[("1", IOType::None)])],
            vec![],
        );
        let view = TreeView::new(&arena, &inst_store);
        let (net_store, mut labels, buses) = empty_overlay();
        labels.insert("N_SIG".to_string(), np("N_SIG"));
        match resolve_chain_overlay(
            &["SIG".to_string()],
            &m,
            "main",
            &labels,
            &buses,
            &net_store,
            &view,
        )
        .expect("port should win")
        {
            InstEntry::Port(p) => assert_eq!(p.path, "SIG"),
            other => panic!("expected port to shadow component/label, got {other:?}"),
        }
        // A label-only name still resolves through the overlay label category.
        match resolve_chain_overlay(
            &["N_SIG".to_string()],
            &m,
            "main",
            &labels,
            &buses,
            &net_store,
            &view,
        )
        .expect("label should resolve")
        {
            InstEntry::Label(l) => assert_eq!(l.path, "N_SIG"),
            other => panic!("expected InstEntry::Label, got {other:?}"),
        }
    }

    /// The overlay chain resolver recurses through a sub-module to a port at
    /// arbitrary DOT depth; the sub-module descent reads its overlay from the
    /// store fragment.
    #[test]
    fn mat_expand__overlay_chain_reaches_submodule_port() {
        let mut sub = McModuleInst::new("mcu513", Arc::new(McModule::test_stub("mcu")));
        sub.ports.push(PortInst::new("VDD", IOType::Power));
        let mut m = McModuleInst::new("main", Arc::new(McModule::test_stub("main")));
        let (arena, inst_store) = store_fixture(&mut m, vec![], vec![sub]);
        let view = TreeView::new(&arena, &inst_store);
        let (net_store, labels, buses) = empty_overlay();
        let chain = vec!["mcu513".to_string(), "VDD".to_string()];
        match resolve_chain_overlay(&chain, &m, "main", &labels, &buses, &net_store, &view)
            .expect("chain should resolve")
        {
            InstEntry::Port(p) => assert_eq!(p.path, "VDD"),
            other => panic!("expected InstEntry::Port, got {other:?}"),
        }
        // Terminal types do not support further DOT resolution.
        let bad_chain = vec!["mcu513".to_string(), "VDD".to_string(), "X".to_string()];
        assert!(
            resolve_chain_overlay(&bad_chain, &m, "main", &labels, &buses, &net_store, &view)
                .is_none()
        );
    }
}
