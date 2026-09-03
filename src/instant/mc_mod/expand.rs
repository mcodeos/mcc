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
use crate::semantic::basic::mc_param::McParamBindings;
use crate::semantic::scope::{ResolveScope, ScopeChain};

// ============================================================================
// Instance-layer scope units (§3.5)
// ============================================================================
// The units below read *instance-layer* tables (instantiation output), so
// they live here instead of `semantic::scope` — only the composition
// mechanism (`ScopeChain` / `ResolveScope`) is shared across layers.
//
// Deviation note: mechanism B resolves into [`InstEntry`], not [`NetPoint`]
// — [`InstEntry::Component`]/[`InstEntry::SubModule`] carry the recursive
// terminals that the overlay chain resolver needs; `NetPoint` is
// terminal-only and would break arbitrary-depth DOT resolution.

// Instance-layer scope units (T = NetPoint). `resolve_name` (their former
// production consumer) was removed; these are kept as test-covered behavior
// for the P1/P2/P3 resolution chain (see the `*_scope_resolves_*` tests).
#[allow(dead_code)]
struct FuncBindingsScope<'a> {
    instance: &'a McComponentInst,
    param_bindings: &'a McParamBindings,
}

#[allow(dead_code)]
impl<'a> FuncBindingsScope<'a> {
    fn new(instance: &'a McComponentInst, param_bindings: &'a McParamBindings) -> Self {
        Self {
            instance,
            param_bindings,
        }
    }
}

impl ResolveScope<NetPoint> for FuncBindingsScope<'_> {
    fn resolve(&self, name: &str) -> Option<NetPoint> {
        for binding in self.param_bindings.iter() {
            if let Some(param_name) = binding.declare.get_primary_name() {
                if param_name == name {
                    // Warn when a func param shadows a component pin with the same
                    // name (design §7.2.3: user-visible, migrated into the
                    // diagnostic system; position unknown at the instance layer,
                    // so anchored at the file start (0,0) like PULLUP_DEGENERATE).
                    if self.instance.pins.get(name).is_some() {
                        crate::db::diagnostic::diagnostic::diagnostic_log(
                            crate::errcodes::FUNC_PARAM_SHADOWS_PIN,
                            crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                            0,
                            0,
                            &crate::errcodes::format_msg(
                                crate::errcodes::FUNC_PARAM_SHADOWS_PIN,
                                &[&name, &self.instance.name],
                            ),
                            &[],
                        );
                    }
                    return Some(NetPoint::with_owner(
                        &format!("{}.{}", self.instance.name, name),
                        &self.instance.name,
                        crate::semantic::common::IOType::None,
                    ));
                }
            }
        }
        None
    }
}

/// P2: instance pins.
#[allow(dead_code)]
struct InstancePinsScope<'a> {
    pins: &'a HashMap<String, NetPoint>,
}

#[allow(dead_code)]
impl<'a> InstancePinsScope<'a> {
    fn new(pins: &'a HashMap<String, NetPoint>) -> Self {
        Self { pins }
    }
}

impl ResolveScope<NetPoint> for InstancePinsScope<'_> {
    fn resolve(&self, name: &str) -> Option<NetPoint> {
        self.pins.get(name).cloned()
    }
}

/// P3: parent module labels.
#[allow(dead_code)]
struct ParentLabelsScope<'a> {
    labels: &'a HashMap<String, NetPoint>,
}

#[allow(dead_code)]
impl<'a> ParentLabelsScope<'a> {
    fn new(labels: &'a HashMap<String, NetPoint>) -> Self {
        Self { labels }
    }
}

impl ResolveScope<NetPoint> for ParentLabelsScope<'_> {
    fn resolve(&self, name: &str) -> Option<NetPoint> {
        self.labels.get(name).cloned()
    }
}

/// P3: parent module ports.
#[allow(dead_code)]
struct ParentPortsScope<'a> {
    ports: &'a [PortInst],
}

#[allow(dead_code)]
impl<'a> ParentPortsScope<'a> {
    fn new(ports: &'a [PortInst]) -> Self {
        Self { ports }
    }
}

impl ResolveScope<NetPoint> for ParentPortsScope<'_> {
    fn resolve(&self, name: &str) -> Option<NetPoint> {
        self.ports
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.net_point.clone())
    }
}

/// Component pins (mechanism B single-level resolution).
struct ComponentPinsScope<'a> {
    pins: &'a HashMap<String, NetPoint>,
}

/// Production resolution of component pins is inline in
/// [`resolve_chain_overlay`] (a `Component` segment reads its pin table
/// directly); this scope unit is kept as test-covered behavior for the
/// single-level resolution chain.
#[allow(dead_code)]
impl<'a> ComponentPinsScope<'a> {
    fn new(pins: &'a HashMap<String, NetPoint>) -> Self {
        Self { pins }
    }
}

impl ResolveScope<InstEntry> for ComponentPinsScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        self.pins.get(name).map(|p| InstEntry::Port(p.clone()))
    }
}

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

// ============================================================================
// InstEntry — Pass2 instance entry types
// ============================================================================

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

// ============================================================================
// ExpansionContext
// ============================================================================

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

// ============================================================================
// Overlay-aware resolution (Phase E)
// ============================================================================

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

// ============================================================================
// §7 Vector expansion matching (eval.md §7) — pure functions
// ============================================================================

/// §7 Expansion match result: the completed state of one matching layer.
#[derive(Debug, Clone)]
pub struct ExpandMatch {
    /// The pairs, kept in **lhs vector order** (§11.2 invariant 4).
    pub pairs: Vec<(NetPoint, NetPoint)>,
    /// True when, after pairing, every pair's two member names differ
    /// (signals D5 BUS_ORDER_MISMATCH). Possible only when both sides carry
    /// non-empty member names and no name matched during pairing.
    pub all_members_mismatched: bool,
}

/// §11.3 Vector expansion matching (eval.md §11.3): pairs the two expanded
/// point lists, keeping both sides' declaration (vector) order.
///
/// Pairing priority:
/// 1. **Member-name correspondence** (preferred): every point on both sides has
///    a non-empty, unique `member_name` → pair by name in **lhs declaration
///    order**; names that find no same-named rhs partner fall back to a
///    positional zip against the remaining rhs points. The result stays in lhs
///    vector order (no alphabetical re-sorting).
/// 2. **Count correspondence**: both sides have the same total point count →
///    positional zip in declaration order (§3.1). No sorting: both sides
///    already carry their vector order. A fully name-mismatched zip signals D5.
/// 3. **Explicit expansion `*`**: when the count structures differ, implicit
///    auto-expansion is **forbidden** (the §7 explicit `*` rule:
///    `[*cannon.UART[1:2,6], ...]` must be expanded as an explicit list) —
///    returns `None`, leaving broadcast / truncation recovery to the caller.
///
/// The N:N pairing path of `create_connection` in group.rs. Replaces the
/// P2-4/P4.2 sorted-zip implementation (eval.md §11).
pub fn expand_match(lhs: &[NetPoint], rhs: &[NetPoint]) -> Option<ExpandMatch> {
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }

    let lhs_all_named = lhs
        .iter()
        .all(|p| p.member_name.as_deref().is_some_and(|n| !n.is_empty()));
    let rhs_all_named = rhs
        .iter()
        .all(|p| p.member_name.as_deref().is_some_and(|n| !n.is_empty()));

    // Names must be unique on both sides (duplicates → ambiguous by-name
    // pairing; fall back to count correspondence).
    let lhs_unique = lhs_all_named
        && lhs
            .iter()
            .map(|p| p.member_name.as_deref().unwrap())
            .collect::<std::collections::HashSet<_>>()
            .len()
            == lhs.len();
    let rhs_unique = rhs_all_named
        && rhs
            .iter()
            .map(|p| p.member_name.as_deref().unwrap())
            .collect::<std::collections::HashSet<_>>()
            .len()
            == rhs.len();

    // ── Priority (1): member-name correspondence (§11.3 step 1) ─────────
    if lhs.len() == rhs.len() && lhs_unique && rhs_unique {
        let rhs_by_name: HashMap<&str, usize> = rhs
            .iter()
            .enumerate()
            .map(|(i, p)| (p.member_name.as_deref().unwrap(), i))
            .collect();
        let mut used = vec![false; rhs.len()];
        let mut slots: Vec<Option<(NetPoint, NetPoint)>> = vec![None; lhs.len()];

        // Pass 1: pair by member name, in lhs declaration order.
        for (i, l) in lhs.iter().enumerate() {
            if let Some(&j) = l.member_name.as_deref().and_then(|n| rhs_by_name.get(n)) {
                slots[i] = Some((l.clone(), rhs[j].clone()));
                used[j] = true;
            }
        }
        // Pass 2: names with no partner fall back to positional zip against
        // the remaining rhs points (lhs order preserved).
        let mut rj = 0;
        for (i, l) in lhs.iter().enumerate() {
            if slots[i].is_some() {
                continue;
            }
            while rj < rhs.len() && used[rj] {
                rj += 1;
            }
            if rj < rhs.len() {
                slots[i] = Some((l.clone(), rhs[rj].clone()));
                used[rj] = true;
                rj += 1;
            }
        }
        let pairs: Vec<(NetPoint, NetPoint)> = slots.into_iter().flatten().collect();
        let all_members_mismatched = pairs
            .iter()
            .all(|(l, r)| l.member_name.as_deref() != r.member_name.as_deref());
        return Some(ExpandMatch {
            pairs,
            all_members_mismatched,
        });
    }

    // ── Priority (2): total-count correspondence — positional zip in
    // declaration order (§11.3 step 2); feeds the D5 check when both sides
    // are named but no name matches.
    if lhs.len() == rhs.len() {
        let pairs: Vec<(NetPoint, NetPoint)> = lhs
            .iter()
            .zip(rhs.iter())
            .map(|(l, r)| (l.clone(), r.clone()))
            .collect();
        let all_members_mismatched = lhs_all_named
            && rhs_all_named
            && pairs
                .iter()
                .all(|(l, r)| l.member_name.as_deref() != r.member_name.as_deref());
        return Some(ExpandMatch {
            pairs,
            all_members_mismatched,
        });
    }

    // ── Count mismatch: implicit auto-expansion is illegal (§7 explicit `*`) ──
    None
}

#[cfg(test)]
mod expand_match_tests {
    use super::*;
    use crate::semantic::common::IOType;

    fn pt(path: &str, member: Option<&str>) -> NetPoint {
        let mut p = NetPoint::new(path, IOType::None);
        p.member_name = member.map(|s| s.to_string());
        p
    }

    // ── §7 rule 1: member-name correspondence (pair by name) ──

    #[test]
    fn by_name_matches_and_preserves_left_order() {
        // uC.SPI(SCLK, CS, MOSI, MISO) vs flash.SPI(CS, MISO, MOSI, SCLK)
        let lhs = vec![
            pt("uC.SPI.1", Some("SCLK")),
            pt("uC.SPI.2", Some("CS")),
            pt("uC.SPI.3", Some("MOSI")),
            pt("uC.SPI.4", Some("MISO")),
        ];
        let rhs = vec![
            pt("flash.SPI.1", Some("CS")),
            pt("flash.SPI.2", Some("MISO")),
            pt("flash.SPI.3", Some("MOSI")),
            pt("flash.SPI.4", Some("SCLK")),
        ];
        let m = expand_match(&lhs, &rhs).expect("by-name match should succeed");
        assert!(!m.all_members_mismatched);
        // Pairs in lhs order: SCLK↔SCLK, CS↔CS, MOSI↔MOSI, MISO↔MISO
        let expect: Vec<(&str, &str)> = vec![
            ("uC.SPI.1", "flash.SPI.4"),
            ("uC.SPI.2", "flash.SPI.1"),
            ("uC.SPI.3", "flash.SPI.3"),
            ("uC.SPI.4", "flash.SPI.2"),
        ];
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, expect);
    }

    #[test]
    fn by_name_skips_on_duplicate_member() {
        // Duplicate name on rhs → by-name pairing is ambiguous; fall back to
        // total-count positional zip.
        let lhs = vec![pt("a.1", Some("X")), pt("a.2", Some("Y"))];
        let rhs = vec![pt("b.1", Some("X")), pt("b.2", Some("X"))];
        let m = expand_match(&lhs, &rhs).expect("falls back to total-count zip");
        // Positional zip in declaration order: X, Y with X, X
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, vec![("a.1", "b.1"), ("a.2", "b.2")]);
    }

    #[test]
    fn by_name_skips_on_missing_name() {
        // Any point missing a member name → fall back to total-count.
        let lhs = vec![pt("a.1", Some("X")), pt("a.2", None)];
        let rhs = vec![pt("b.1", Some("X")), pt("b.2", Some("Y"))];
        let m = expand_match(&lhs, &rhs).expect("total-count zip");
        assert_eq!(m.pairs.len(), 2);
    }

    // ── §7 rule 1: names unique and one-to-one → keep lhs order ──

    #[test]
    fn by_name_unique_matching_preserves_lhs_order() {
        // Names unique and one-to-one on both sides → rule 1 pairs in lhs
        // order (deterministic). The old implementation
        // (try_match_by_member_name) iterated a HashMap and produced random
        // order; keeping lhs order here is a behavior improvement.
        let lhs = vec![pt("l.VDD", Some("VDD")), pt("l.GND", Some("GND"))];
        let rhs = vec![pt("r.GND", Some("GND")), pt("r.VDD", Some("VDD"))];
        let m = expand_match(&lhs, &rhs).expect("rule-1 by-name match");
        assert!(!m.all_members_mismatched);
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, vec![("l.VDD", "r.VDD"), ("l.GND", "r.GND")]);
    }

    // ── §11.3 rule 1: partial name match → by-name first, positional fallback ──

    #[test]
    fn partial_by_name_then_positional_fallback() {
        // Only some names match (SPI-like: lhs declares SCLK/MOSI/CSN/MISO,
        // rhs carries CS/SCLK/MISO/MOSI). Name matches are paired by name,
        // the unmatched CSN/CS pair positionally; output stays in lhs order.
        let lhs = vec![
            pt("l.SCLK", Some("SCLK")),
            pt("l.MOSI", Some("MOSI")),
            pt("l.CSN", Some("CSN")),
            pt("l.MISO", Some("MISO")),
        ];
        let rhs = vec![
            pt("r.CS", Some("CS")),
            pt("r.SCLK", Some("SCLK")),
            pt("r.MISO", Some("MISO")),
            pt("r.MOSI", Some("MOSI")),
        ];
        let m = expand_match(&lhs, &rhs).expect("by-name + positional fallback");
        assert!(!m.all_members_mismatched);
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("l.SCLK", "r.SCLK"),
                ("l.MOSI", "r.MOSI"),
                ("l.CSN", "r.CS"),
                ("l.MISO", "r.MISO"),
            ]
        );
    }

    #[test]
    fn partial_by_name_keeps_lhs_order_when_unmatched_first() {
        // An unmatched lhs member in front must not reorder later name pairs.
        let lhs = vec![
            pt("l.A", Some("A")),
            pt("l.X", Some("X")),
            pt("l.B", Some("B")),
        ];
        let rhs = vec![
            pt("r.B", Some("B")),
            pt("r.A", Some("A")),
            pt("r.C", Some("C")),
        ];
        let m = expand_match(&lhs, &rhs).expect("by-name + positional fallback");
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        // A↔A, X↔C (positional), B↔B — all in lhs order.
        assert_eq!(got, vec![("l.A", "r.A"), ("l.X", "r.C"), ("l.B", "r.B")]);
    }

    // ── §11.3 rule 2: total-count correspondence (positional zip) ──

    #[test]
    fn total_count_by_name_then_positional_fallback() {
        // Rule 1 fires with partial matches: GND pairs by name, VDD falls
        // back positionally to VDD_3V3; output stays in lhs declaration order
        // (GND first). The old implementation sorted both sides and zipped.
        let lhs = vec![pt("l.GND", Some("GND")), pt("l.VDD", Some("VDD"))];
        let rhs = vec![pt("r.VDD_3V3", Some("VDD_3V3")), pt("r.GND", Some("GND"))];
        let m = expand_match(&lhs, &rhs).expect("total-count zip");
        assert!(!m.all_members_mismatched);
        let got: Vec<(&str, &str)> = m
            .pairs
            .iter()
            .map(|(l, r)| (l.path.as_str(), r.path.as_str()))
            .collect();
        assert_eq!(got, vec![("l.GND", "r.GND"), ("l.VDD", "r.VDD_3V3")]);
    }

    #[test]
    fn total_count_positional_without_names() {
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
    fn total_count_all_mismatched_signals_d5() {
        // No name matches at all → every pair's member names differ → D5 signal.
        let lhs = vec![pt("l.1", Some("A")), pt("l.2", Some("B"))];
        let rhs = vec![pt("r.1", Some("C")), pt("r.2", Some("D"))];
        let m = expand_match(&lhs, &rhs).expect("total-count zip");
        assert!(m.all_members_mismatched);
    }

    // ── §7 rule 3: count mismatch → None (implicit expansion forbidden) ──

    #[test]
    fn count_mismatch_returns_none() {
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
    fn empty_side_returns_none() {
        assert!(expand_match(&[], &[pt("a.1", None)]).is_none());
    }
}

// ============================================================================
// Instance-layer scope unit tests (§3.5) — each unit is exercised with its
// own input (field-level slices/maps), so no full `McModuleInst` is required
// except where a real sub-module instance is needed for resolution.
// ============================================================================

#[cfg(test)]
mod inst_scope_tests {
    use super::*;
    use crate::instant::arena::{Node, NodeArena, NodeKind};
    use crate::instant::identity::{CircuitKey, IdentityRegistry};
    use crate::instant::inststore::{InstanceStore, NodeInstance};
    use crate::semantic::basic::mc_ids::McIds;
    use crate::semantic::basic::mc_param::McParamValue;
    use crate::semantic::basic::mc_param_type::McParamType;
    use crate::semantic::basic::mc_paramd::{McParamDeclare, McParamDeclareKind, McParamDeclares};
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
        NetPoint::new(path, IOType::None)
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
            cond_attrs: Vec::new(),
            resolved_attrs: Vec::new(),
            nc: false,
            degraded: false,
            origin: Default::default(),
            expansion_id: None,
            node_id: None,
            anchor: None,
        }
    }

    /// Component instance with pre-populated pins.
    fn comp_inst_with_pins(name: &str, pins: &[(&str, IOType)]) -> McComponentInst {
        let mut inst = comp_inst(name);
        for (pid, io) in pins {
            inst.pins.insert(
                (*pid).to_string(),
                NetPoint::with_owner(&format!("{name}.{pid}"), name, io.clone()),
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

    /// A `McParamBindings` with a single positional binding `name -> n1`.
    fn one_binding(name: &str) -> McParamBindings {
        let declare = McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from(name)),
            param_type: McParamType::default(),
        };
        let mut declares = McParamDeclares::new();
        declares.push(declare);
        McParamBindings::bind_quiet(&declares, &[McParamValue::Ids(McIds::from("n1"))])
            .expect("single positional binding should succeed")
    }

    // ── Mechanism A (T = NetPoint) ──

    /// P1 func-bindings scope: a bound param name resolves to a `NetPoint`
    /// owned by the expanded instance; unknown names miss.
    #[test]
    fn func_bindings_scope_resolves_bound_param() {
        // Pin table holds "VDD" only — the param name "net" does not collide,
        // so no shadow diagnostic is emitted.
        let inst = comp_inst_with_pins("U1", &[("VDD", IOType::Power)]);
        let bindings = one_binding("net");
        let scope = FuncBindingsScope::new(&inst, &bindings);
        let hit = scope.resolve("net").expect("param should resolve");
        assert_eq!(hit.path, "U1.net");
        assert_eq!(hit.owner.as_deref(), Some("U1"));
        assert!(scope.resolve("other").is_none());
    }

    /// P2 instance-pins scope: reads the instance pin table directly.
    #[test]
    fn instance_pins_scope_resolves_pin() {
        let mut pins = HashMap::new();
        pins.insert("VDD".to_string(), np("U1.VDD"));
        let scope = InstancePinsScope::new(&pins);
        let hit = scope.resolve("VDD").expect("pin should resolve");
        assert_eq!(hit.path, "U1.VDD");
        assert!(scope.resolve("GND").is_none());
    }

    /// P3 parent-labels scope: reads the parent module label table.
    #[test]
    fn parent_labels_scope_resolves_label() {
        let mut labels = HashMap::new();
        labels.insert("N_5V".to_string(), np("N_5V"));
        let scope = ParentLabelsScope::new(&labels);
        let hit = scope.resolve("N_5V").expect("label should resolve");
        assert_eq!(hit.path, "N_5V");
        assert!(scope.resolve("N_GND").is_none());
    }

    /// P3 parent-ports scope: a port resolves to its stored `NetPoint`.
    #[test]
    fn parent_ports_scope_resolves_port() {
        let ports = vec![PortInst::new("CLK", IOType::Out)];
        let scope = ParentPortsScope::new(&ports);
        let hit = scope.resolve("CLK").expect("port should resolve");
        assert_eq!(hit.path, "CLK");
        assert!(scope.resolve("RST").is_none());
    }

    // ── Mechanism B (T = InstEntry) ──

    /// Component pins resolve to a terminal `InstEntry::Port`.
    #[test]
    fn component_pins_scope_resolves_port_entry() {
        let inst = comp_inst_with_pins("R1", &[("1", IOType::None), ("2", IOType::None)]);
        let scope = ComponentPinsScope::new(&inst.pins);
        match scope.resolve("1").expect("pin should resolve") {
            InstEntry::Port(p) => assert_eq!(p.path, "R1.1"),
            other => panic!("expected InstEntry::Port, got {other:?}"),
        }
        assert!(scope.resolve("3").is_none());
    }

    /// Module ports resolve to a terminal `InstEntry::Port`.
    #[test]
    fn module_ports_scope_resolves_port_entry() {
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
    fn module_labels_scope_resolves_label_entry() {
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
    fn module_components_scope_resolves_component_entry() {
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
    fn module_sub_modules_scope_resolves_submodule_entry() {
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
    fn module_buses_scope_resolves_members_from_labels() {
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
    fn overlay_chain_resolves_component_pin() {
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
    fn overlay_chain_priority_ports_over_components() {
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
    fn overlay_chain_reaches_submodule_port() {
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
