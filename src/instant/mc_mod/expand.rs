// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 namespace unification — [`InstFindInst`] trait, [`InstEntry`] enum,
//! and [`ExpansionContext`] for func body expansion name resolution.
//!
//! Phase 2.5 of the namespace refactoring plan.

use std::collections::HashMap;
use std::sync::Arc;

use super::super::mc_bus::McBusInst;
use super::super::mc_comp::McComponentInst;
use super::super::mc_net::{NetPoint, PortInst};
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
// Deviation note: mechanism B (InstFindInst) resolves into [`InstEntry`],
// not [`NetPoint`] — [`InstEntry::Component`]/[`InstEntry::SubModule`] carry
// the recursive terminals that `resolve_inst_chain` needs; `NetPoint` is
// terminal-only and would break arbitrary-depth DOT resolution.

/// P1: func param bindings — returns a `NetPoint` owned by the expanded
/// instance. Keeps the shadow warning when a param hides a same-named pin
/// (original `ExpansionContext::resolve_name` behavior, §3.5).
struct FuncBindingsScope<'a> {
    instance: &'a McComponentInst,
    param_bindings: &'a McParamBindings,
}

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
struct InstancePinsScope<'a> {
    pins: &'a HashMap<String, NetPoint>,
}

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
struct ParentLabelsScope<'a> {
    labels: &'a HashMap<String, NetPoint>,
}

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
struct ParentPortsScope<'a> {
    ports: &'a [PortInst],
}

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

/// Module component instances (mechanism B P3).
struct ModuleComponentsScope<'a> {
    components: &'a [McComponentInst],
}

impl<'a> ModuleComponentsScope<'a> {
    fn new(components: &'a [McComponentInst]) -> Self {
        Self { components }
    }
}

impl ResolveScope<InstEntry> for ModuleComponentsScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        self.components
            .iter()
            .find(|c| c.name == name)
            .map(|c| InstEntry::Component(Arc::new(c.clone())))
    }
}

/// Module sub-module instances (mechanism B P4).
struct ModuleSubModulesScope<'a> {
    sub_modules: &'a [McModuleInst],
}

impl<'a> ModuleSubModulesScope<'a> {
    fn new(sub_modules: &'a [McModuleInst]) -> Self {
        Self { sub_modules }
    }
}

impl ResolveScope<InstEntry> for ModuleSubModulesScope<'_> {
    fn resolve(&self, name: &str) -> Option<InstEntry> {
        self.sub_modules
            .iter()
            .find(|s| s.name == name)
            .map(|s| InstEntry::SubModule(Arc::new(s.clone())))
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
/// so that [`resolve_inst_chain`] can recursively call
/// [`InstFindInst::find_inst`] on sub-modules without lifetime constraints.
#[derive(Debug, Clone)]
pub enum InstEntry {
    /// A component instance (e.g. `R1`, `U1`) — holds the actual instance
    /// for pin-level resolution.
    Component(Arc<McComponentInst>),
    /// A sub-module instance — holds the actual instance so
    /// [`InstFindInst::find_inst`] can recurse arbitrarily deep.
    SubModule(Arc<McModuleInst>),
    /// A port connection point (terminal — no further DOT resolution)
    Port(NetPoint),
    /// A label connection point (terminal — no further DOT resolution)
    Label(NetPoint),
    /// A bus (collection of connection points; terminal)
    Bus(Vec<NetPoint>),
}

// ============================================================================
// InstFindInst trait
// ============================================================================

/// Pass2 namespace lookup trait — parallel to [`crate::HasFindInst`] but
/// operates on instantiated types instead of semantic definition types.
///
/// # Design
///
/// The priority chain mirrors Pass1 [`HasFindInst`]:
///   - [`McModuleInst`]: ports → labels → components → sub_modules → buses
///   - [`McComponentInst`]: pins only
///
/// [`InstEntry::SubModule`] holds an [`Arc<McModuleInst>`], enabling
/// recursive DOT-chain resolution via [`resolve_inst_chain`] with no
/// depth limit — unlike the previous ad-hoc 2-level scope drilling in
/// `funccall.rs`.
pub trait InstFindInst {
    /// Look up a name in the instance namespace.
    fn find_inst(&self, name: &str) -> Option<InstEntry>;
}

// ============================================================================
// ExpansionContext
// ============================================================================

/// Pass2 func body expansion name resolver.
///
/// Provides name resolution during component function body expansion
/// with the same priority as [`McComponent::find_inst_with_span`].
///
/// Priority chain:
/// 1. func params (param bindings)
/// 2. instance pins
/// 3. parent scope (module ports, labels)
pub struct ExpansionContext<'a> {
    /// The component instance being expanded
    pub instance: &'a McComponentInst,
    /// Parameter bindings from the instantiation site
    pub param_bindings: &'a McParamBindings,
    /// Parent module scope for resolving external references
    pub parent_scope: &'a McModuleInst,
}

impl<'a> ExpansionContext<'a> {
    /// Create a new expansion context.
    pub fn new(
        instance: &'a McComponentInst,
        param_bindings: &'a McParamBindings,
        parent_scope: &'a McModuleInst,
    ) -> Self {
        Self {
            instance,
            param_bindings,
            parent_scope,
        }
    }

    /// Resolve a name to a [`NetPoint`] using the component priority chain.
    ///
    /// Priority (instance-layer scope chain, mechanism A, §7.2):
    /// 1. func params (matched against binding declare names)
    /// 2. instance pins (`self.instance.pins`)
    /// 3. parent scope — module labels and ports
    pub fn resolve_name(&self, name: &str) -> Option<NetPoint> {
        ScopeChain::new(vec![
            Box::new(FuncBindingsScope::new(self.instance, self.param_bindings)),
            Box::new(InstancePinsScope::new(&self.instance.pins)),
            Box::new(ParentLabelsScope::new(&self.parent_scope.labels)),
            Box::new(ParentPortsScope::new(&self.parent_scope.ports)),
        ])
        .resolve(name)
    }
}

// ============================================================================
// impl InstFindInst for McComponentInst
// ============================================================================

impl InstFindInst for McComponentInst {
    fn find_inst(&self, name: &str) -> Option<InstEntry> {
        // Component instances only have pin-level resolution.
        ScopeChain::new(vec![Box::new(ComponentPinsScope::new(&self.pins))]).resolve(name)
    }
}

// ============================================================================
// impl InstFindInst for McModuleInst
// ============================================================================

impl InstFindInst for McModuleInst {
    fn find_inst(&self, name: &str) -> Option<InstEntry> {
        // Instance-layer category chain (mechanism B) mirroring the Pass1
        // `McModule` priority: ports → labels → components → sub_modules →
        // buses.
        ScopeChain::new(vec![
            Box::new(ModulePortsScope::new(&self.ports)),
            Box::new(ModuleLabelsScope::new(&self.labels)),
            Box::new(ModuleComponentsScope::new(&self.components)),
            Box::new(ModuleSubModulesScope::new(&self.sub_modules)),
            Box::new(ModuleBusesScope::new(&self.buses, &self.labels)),
        ])
        .resolve(name)
    }
}

// ============================================================================
// resolve_inst_chain — DOT chain recursive resolution
// ============================================================================

/// Recursively resolve a DOT-separated name chain against a starting scope.
///
/// Each segment is resolved via [`InstFindInst::find_inst`]. When the result
/// is a [`InstEntry::SubModule`], the next segment is resolved against that
/// sub-module's own [`InstFindInst`] impl — and so on, to arbitrary depth.
///
/// # Arguments
///
/// * `chain` — DOT-separated name segments (e.g. `["mcu", "uC", "VDD"]`)
/// * `scope` — Starting scope (typically a [`McModuleInst`])
///
/// # Returns
///
/// The final [`InstEntry`] after resolving all segments, or `None` if any
/// segment fails to resolve.
///
/// # Examples
///
/// - `["uC", "VDD"]` on module → `InstEntry::Port(pin_netpoint)`
/// - `["mcu", "uC"]` on module → `InstEntry::Component(uC_arc)`
/// - `["mcu"]` on parent module → `InstEntry::SubModule(mcu_arc)`
pub fn resolve_inst_chain(chain: &[String], scope: &dyn InstFindInst) -> Option<InstEntry> {
    if chain.is_empty() {
        return None;
    }

    // Resolve first segment
    let mut current = scope.find_inst(&chain[0])?;

    // Recurse into remaining segments
    for seg in &chain[1..] {
        current = match &current {
            // SubModule: recurse via its own InstFindInst impl
            InstEntry::SubModule(sub) => sub.find_inst(seg)?,
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

/// §11.3: Pair a port's member names (declaration order) with actual argument
/// lanes — by name first, positional fallback for the rest (mirrors
/// [`expand_match`] priority 1). Returns, in member order, the index into
/// `arg_lanes` paired with each member.
pub fn pair_members_to_lanes(members: &[String], arg_lanes: &[NetPoint]) -> Vec<usize> {
    let mut result: Vec<usize> = Vec::with_capacity(members.len());
    let mut used = vec![false; arg_lanes.len()];
    // Pass 1: pair by name.
    for m in members {
        let hit = arg_lanes.iter().position(|a| {
            let n = a
                .member_name
                .as_deref()
                .unwrap_or_else(|| a.path.rsplit('.').next().unwrap_or(&a.path));
            n == m.as_str()
        });
        match hit {
            Some(j) if !used[j] => {
                result.push(j);
                used[j] = true;
            }
            _ => result.push(usize::MAX),
        }
    }
    // Pass 2: positional fallback for names with no partner.
    let mut rj = 0;
    for r in result.iter_mut() {
        if *r != usize::MAX {
            continue;
        }
        while rj < arg_lanes.len() && used[rj] {
            rj += 1;
        }
        if rj < arg_lanes.len() {
            *r = rj;
            used[rj] = true;
            rj += 1;
        }
    }
    result
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
                span: crate::ast::ast_semantic::Span { start: 0, end: 0 },
            }),
            params: McParamBindings::new(),
            pins: HashMap::new(),
            cond_pin_names: HashMap::new(),
            cond_attrs: Vec::new(),
            resolved_attrs: Vec::new(),
            nc: false,
            degraded: false,
            origin: Default::default(),
            expansion_id: None,
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
        let components = vec![comp_inst_with_pins("R1", &[("1", IOType::None)])];
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
        let sub_modules = vec![sub];
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

    // ── Composition — `InstFindInst` impls and DOT-chain recursion ──

    /// `McComponentInst::find_inst` resolves pins only.
    #[test]
    fn component_find_inst_resolves_pin() {
        let inst = comp_inst_with_pins("U1", &[("VDD", IOType::Power)]);
        match inst.find_inst("VDD").expect("pin should resolve") {
            InstEntry::Port(p) => assert_eq!(p.path, "U1.VDD"),
            other => panic!("expected InstEntry::Port, got {other:?}"),
        }
        assert!(inst.find_inst("GND").is_none());
    }

    /// `McModuleInst::find_inst` follows the ports → labels → components →
    /// sub_modules → buses priority: a name present in both `ports` and
    /// `components` resolves to the port.
    #[test]
    fn module_find_inst_priority_ports_over_components() {
        let mut m = McModuleInst::new("main", Arc::new(McModule::test_stub("main")));
        m.ports.push(PortInst::new("SIG", IOType::InOut));
        m.components
            .push(comp_inst_with_pins("SIG", &[("1", IOType::None)]));
        m.labels.insert("N_SIG".to_string(), np("N_SIG"));
        match m.find_inst("SIG").expect("port should win") {
            InstEntry::Port(p) => assert_eq!(p.path, "SIG"),
            other => panic!("expected port to shadow component/label, got {other:?}"),
        }
        // A label-only name still resolves through the labels category.
        match m.find_inst("N_SIG").expect("label should resolve") {
            InstEntry::Label(l) => assert_eq!(l.path, "N_SIG"),
            other => panic!("expected InstEntry::Label, got {other:?}"),
        }
    }

    /// `resolve_inst_chain` recurses through a sub-module to a port at
    /// arbitrary DOT depth.
    #[test]
    fn resolve_inst_chain_reaches_submodule_port() {
        let mut sub = McModuleInst::new("mcu513", Arc::new(McModule::test_stub("mcu")));
        sub.ports.push(PortInst::new("VDD", IOType::Power));
        let mut m = McModuleInst::new("main", Arc::new(McModule::test_stub("main")));
        m.sub_modules.push(sub);
        let chain = vec!["mcu513".to_string(), "VDD".to_string()];
        match resolve_inst_chain(&chain, &m).expect("chain should resolve") {
            InstEntry::Port(p) => assert_eq!(p.path, "VDD"),
            other => panic!("expected InstEntry::Port, got {other:?}"),
        }
        // Terminal types do not support further DOT resolution.
        let bad_chain = vec!["mcu513".to_string(), "VDD".to_string(), "X".to_string()];
        assert!(resolve_inst_chain(&bad_chain, &m).is_none());
    }
}
