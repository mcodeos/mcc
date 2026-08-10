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
