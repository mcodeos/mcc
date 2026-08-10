// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 namespace unification — [`InstFindInst`] trait, [`InstEntry`] enum,
//! and [`ExpansionContext`] for func body expansion name resolution.
//!
//! Phase 2.5 of the namespace refactoring plan.

use super::super::mc_comp::McComponentInst;
use super::super::mc_net::NetPoint;
use super::McModuleInst;
use crate::semantic::basic::mc_param::McParamBindings;

// ============================================================================
// InstEntry — Pass2 instance entry types
// ============================================================================

/// Pass2 analog of [`crate::McInstance`] — resolved instance in the
/// instantiation phase.
#[derive(Debug, Clone)]
pub enum InstEntry {
    /// A component instance (e.g. `R1`, `U1`)
    Component(InstRef),
    /// A sub-module instance
    SubModule(InstRef),
    /// A port connection point
    Port(NetPoint),
    /// A label connection point
    Label(NetPoint),
    /// A bus (collection of connection points)
    Bus(Vec<NetPoint>),
}

/// Lightweight reference to an instance — stores name and type tag
/// without borrowing from the parent module.
#[derive(Debug, Clone)]
pub struct InstRef {
    /// Instance name
    pub name: String,
    /// Type tag: "component" or "submodule"
    pub kind: InstRefKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstRefKind {
    Component,
    SubModule,
}

// ============================================================================
// InstFindInst trait
// ============================================================================

/// Pass2 namespace lookup trait — parallel to [`crate::HasFindInst`] but
/// operates on instantiated types instead of semantic definition types.
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
                return Some(InstEntry::Component(InstRef {
                    name: comp.name.clone(),
                    kind: InstRefKind::Component,
                }));
            }
        }

        // P4: sub-module instances
        for sub in &self.sub_modules {
            if sub.name == name {
                return Some(InstEntry::SubModule(InstRef {
                    name: sub.name.clone(),
                    kind: InstRefKind::SubModule,
                }));
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
/// Each segment is resolved via [`InstFindInst::find_inst`], and the result
/// becomes the scope for the next segment. Supports arbitrary nesting depth.
///
/// # Arguments
///
/// * `chain` — DOT-separated name segments (e.g. `["mcu513", "uC", "SPI"]`)
/// * `scope` — Starting scope (typically a module instance or expansion context)
///
/// # Returns
///
/// The final [`InstEntry`] after resolving all segments, or `None` if any
/// segment fails to resolve.
///
/// # Examples
///
/// - `["uC", "VDD"]` on a module scope → Component pin NetPoint
/// - `["mcu513", "uC", "SPI", "SCLK"]` → nested sub-module → component → bus → member
pub fn resolve_inst_chain(chain: &[String], scope: &dyn InstFindInst) -> Option<InstEntry> {
    if chain.is_empty() {
        return None;
    }

    let mut current: Option<InstEntry> = None;

    for segment in chain.iter() {
        let entry = scope.find_inst(segment)?;

        match &entry {
            InstEntry::Component(_) | InstEntry::SubModule(_) => {
                // Compound types: return them for caller to handle member access
                // (component pins, sub-module ports etc.)
                current = Some(entry);
                break;
            }
            InstEntry::Port(_) | InstEntry::Label(_) | InstEntry::Bus(_) => {
                // Terminal types: port/label/bus don't have sub-members
                current = Some(entry);
                break;
            }
        }
    }

    // If no match found but we resolved some segments, return the entry
    current
}

/// Resolve a DOT chain against a [`McModuleInst`] scope with full sub-module
/// and component traversal support.
///
/// Unlike [`resolve_inst_chain`], this function can traverse into sub-module
/// instances to resolve deeper chains like `"mcu513.uC.VDD"`.
pub fn resolve_inst_chain_in_module(chain: &[String], module: &McModuleInst) -> Option<InstEntry> {
    if chain.is_empty() {
        return None;
    }

    let mut seg_idx = 0;
    let mut current_module: Option<&McModuleInst> = Some(module);

    while seg_idx < chain.len() && current_module.is_some() {
        let mod_inst = current_module?;
        let segment = &chain[seg_idx];

        match mod_inst.find_inst(segment)? {
            InstEntry::SubModule(_sub_ref) => {
                // Find the actual sub-module instance for deeper traversal
                if let Some(sub) = mod_inst.sub_modules.iter().find(|s| s.name == *segment) {
                    current_module = Some(sub);
                    seg_idx += 1;
                    continue;
                }
                return Some(InstEntry::SubModule(InstRef {
                    name: segment.clone(),
                    kind: InstRefKind::SubModule,
                }));
            }
            InstEntry::Component(comp_ref) => {
                // Component: resolve remaining segments as pin access
                if seg_idx + 1 < chain.len() {
                    if let Some(comp) = mod_inst.components.iter().find(|c| c.name == comp_ref.name)
                    {
                        // Try direct pin name lookup for remaining segments
                        let remaining = chain[seg_idx + 1..].join(".");
                        if let Some(pin) = comp.pins.get(&remaining) {
                            return Some(InstEntry::Port(pin.clone()));
                        }
                        // Try each remaining segment individually
                        for member_seg in &chain[seg_idx + 1..] {
                            if let Some(pin) = comp.pins.get(member_seg) {
                                return Some(InstEntry::Port(pin.clone()));
                            }
                        }
                    }
                    return Some(InstEntry::Component(comp_ref));
                }
                return Some(InstEntry::Component(comp_ref));
            }
            other => return Some(other),
        }
    }

    None
}
