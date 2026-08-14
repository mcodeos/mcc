// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 namespace unification — [`InstFindInst`] trait, [`InstEntry`] enum,
//! and [`ExpansionContext`] for func body expansion name resolution.
//!
//! Phase 2.5 of the namespace refactoring plan.

use std::sync::Arc;

use super::super::mc_comp::McComponentInst;
use super::super::mc_net::NetPoint;
use super::McModuleInst;
use crate::semantic::basic::mc_param::McParamBindings;

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
    /// Priority:
    /// 1. func params (matched against binding declare names)
    /// 2. instance pins (`self.instance.pins`)
    /// 3. parent scope — module labels and ports
    pub fn resolve_name(&self, name: &str) -> Option<NetPoint> {
        // P1: param bindings
        for binding in self.param_bindings.iter() {
            if let Some(param_name) = binding.declare.get_primary_name() {
                if param_name == name {
                    // Warn when a func param shadows a component pin with the same name
                    if self.instance.pins.get(name).is_some() {
                        tracing::warn!(
                            "Func param '{}' shadows pin of component '{}' in function body expansion — param takes priority",
                            name,
                            self.instance.name
                        );
                    }
                    // Param found — return as a NetPoint with owner = this instance
                    return Some(NetPoint::with_owner(
                        &format!("{}.{}", self.instance.name, name),
                        &self.instance.name,
                        crate::semantic::common::IOType::None,
                    ));
                }
            }
        }

        // P2: instance pins
        if let Some(pin) = self.instance.pins.get(name) {
            return Some(pin.clone());
        }

        // P3: parent scope — module labels
        if let Some(label) = self.parent_scope.labels.get(name) {
            return Some(label.clone());
        }

        // P3: parent scope — module ports (search ports vector)
        for port in &self.parent_scope.ports {
            if port.name == name {
                return Some(port.net_point.clone());
            }
        }

        None
    }
}

// ============================================================================
// impl InstFindInst for McComponentInst
// ============================================================================

impl InstFindInst for McComponentInst {
    fn find_inst(&self, name: &str) -> Option<InstEntry> {
        // Component instances only have pin-level resolution
        if let Some(pin) = self.pins.get(name) {
            return Some(InstEntry::Port(pin.clone()));
        }
        None
    }
}

// ============================================================================
// impl InstFindInst for McModuleInst
// ============================================================================

impl InstFindInst for McModuleInst {
    fn find_inst(&self, name: &str) -> Option<InstEntry> {
        // Priority mirrors HasFindInst for McModule:
        // P1: ports
        for port in &self.ports {
            if port.name == name {
                return Some(InstEntry::Port(port.net_point.clone()));
            }
        }

        // P2: explicit labels
        if let Some(label) = self.labels.get(name) {
            return Some(InstEntry::Label(label.clone()));
        }

        // P3: component instances
        for comp in &self.components {
            if comp.name == name {
                return Some(InstEntry::Component(Arc::new(comp.clone())));
            }
        }

        // P4: sub-module instances
        for sub in &self.sub_modules {
            if sub.name == name {
                return Some(InstEntry::SubModule(Arc::new(sub.clone())));
            }
        }

        // P5: buses — resolve member names to NetPoints from labels
        if let Some(bus) = self.buses.get(name) {
            let points: Vec<NetPoint> = bus
                .members
                .iter()
                .filter_map(|m| self.labels.get(m).cloned())
                .collect();
            return Some(InstEntry::Bus(points));
        }

        None
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
/// * `chain` — DOT-separated name segments (e.g. `["mcu513", "uC", "VDD"]`)
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
/// - `["mcu513", "uC"]` on module → `InstEntry::Component(uC_arc)`
/// - `["mcu513"]` on parent module → `InstEntry::SubModule(mcu513_arc)`
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
    /// The pairs (rule 1 keeps the original `lhs` order; rule 2 stable-sorts by
    /// member name before zipping).
    pub pairs: Vec<(NetPoint, NetPoint)>,
    /// True when, after pairing, every pair's two member names differ
    /// (signals D5 BUS_ORDER_MISMATCH). Possible only when both sides carry
    /// non-empty member names and pairing went through rule 2.
    pub all_members_mismatched: bool,
}

/// §7 Vector expansion matching (eval.md §7): pairs the two expanded point
/// lists.
///
/// Expansion order:
/// 1. **Member-name correspondence** (preferred): every point on both sides has
///    a non-empty, unique `member_name`, and all names can be paired one-to-one
///    → pair by name (left-side order preserved, deterministic).
/// 2. **Count correspondence**: both sides have the same total point count →
///    stable-sort by member name (only when both sides are named) then zip by
///    position; the sort removes misalignment caused by declaration-order
///    differences and produces the D5 signal.
/// 3. **Explicit expansion `*`**: when the count structures differ, implicit
///    auto-expansion is **forbidden** (the §7 explicit `*` rule:
///    `[*cannon.UART[1:2,6], ...]` must be expanded as an explicit list) —
///    returns `None`, leaving broadcast / truncation recovery to the caller.
///
/// Replacement implementation (P4.2): `try_match_by_member_name` + sorted zip
/// (the N:N pairing path of `create_connection` in group.rs
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

    // ── Priority (1): member-name correspondence — pair by name ─────────
    // Names must be unique on both sides (duplicates → ambiguous by-name
    // pairing; fall back to total-count correspondence).
    if lhs_all_named && rhs_all_named && lhs.len() == rhs.len() {
        let lhs_unique = lhs
            .iter()
            .map(|p| p.member_name.as_deref().unwrap())
            .collect::<std::collections::HashSet<_>>()
            .len()
            == lhs.len();
        let rhs_unique = rhs
            .iter()
            .map(|p| p.member_name.as_deref().unwrap())
            .collect::<std::collections::HashSet<_>>()
            .len()
            == rhs.len();
        if lhs_unique && rhs_unique {
            let mut pairs = Vec::with_capacity(lhs.len());
            let mut all_found = true;
            for l in lhs {
                let name = l.member_name.as_deref().unwrap();
                match rhs.iter().find(|r| r.member_name.as_deref() == Some(name)) {
                    Some(r) => pairs.push((l.clone(), r.clone())),
                    None => {
                        all_found = false;
                        break;
                    }
                }
            }
            if all_found {
                return Some(ExpandMatch {
                    pairs,
                    all_members_mismatched: false,
                });
            }
        }
    }

    // ── Priority (2): total-count correspondence ────────────────────────
    if lhs.len() == rhs.len() {
        let mut ls: Vec<&NetPoint> = lhs.iter().collect();
        let mut rs: Vec<&NetPoint> = rhs.iter().collect();
        // Both sides named → sort by member name then zip (deterministic;
        // also feeds the D5 check).
        if lhs_all_named && rhs_all_named {
            ls.sort_by_key(|p| p.member_name.as_deref());
            rs.sort_by_key(|p| p.member_name.as_deref());
        }
        let pairs: Vec<(NetPoint, NetPoint)> = ls
            .iter()
            .zip(rs.iter())
            .map(|(l, r)| ((*l).clone(), (*r).clone()))
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
        // total-count (sorted zip).
        let lhs = vec![pt("a.1", Some("X")), pt("a.2", Some("Y"))];
        let rhs = vec![pt("b.1", Some("X")), pt("b.2", Some("X"))];
        let m = expand_match(&lhs, &rhs).expect("falls back to total-count zip");
        // Both sides named → sorted zip: X, Y with X, X
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

    // ── §7 rule 2: total-count correspondence (sorted zip) ──

    #[test]
    fn total_count_sorted_zip_pairs_matching_names() {
        // Rule 1 fails (VDD has no matching name on rhs) → rule 2:
        // stable-sort by name then zip, aligning GND↔GND instead of pairing
        // by declaration position.
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
        // After sorting, every pair's member names differ → D5 signal.
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
