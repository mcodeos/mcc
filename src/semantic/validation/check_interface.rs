// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Interface binding validation.
//!
//! Checks:
//!   I4-ext — all interface pins are bound to physical pins in the component
//!   C4-ext — interface roles referenced in component must exist in the interface definition
//!   F3 — deprecated CMIE usage (component extends deprecated interface/component)

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashSet;

pub struct InterfaceCheck;

impl ValidationCheck for InterfaceCheck {
    fn name(&self) -> &'static str {
        "interface"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        check_iface_pin_completeness(acc); // I4-ext
        check_iface_role_exists(acc); // C4-ext
        check_deprecated_cmie_usage(acc); // F3
        check_module_member_refs(acc); // cross-module port reference
    }
}

// ============================================================================
// I4-ext: All interface pins bound to physical pins
// ============================================================================

/// When a component binds to an interface via e.g. `pins=[1=SPI.MOSI, 2=SPI.MISO]`,
/// every pin defined in the interface must be mapped to at least one physical pin.
/// Missing bindings mean the interface contract is not fulfilled.
fn check_iface_pin_completeness(acc: &mut CheckAccumulator) {
    let comps = crate::db::cmie::tables::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // For each interface binding in this component
        for (bind_name, port) in &comp.pins.names_to_id {
            let iface = match port {
                crate::semantic::component::mc_pins::McPinPort::Interface(iface) => iface,
                _ => continue,
            };

            let iface_name = &iface.name.to_string();
            let iface_pins = &iface.base.pins.names_to_id;

            if iface_pins.is_empty() {
                continue;
            }

            // Collect which interface pins are bound to physical pins
            let mut bound_iface_pins: HashSet<String> = HashSet::new();

            for (_pin_id, pin) in &comp.pins.pins {
                for name in &pin.names {
                    // Name format: "SPI.MOSI" or "bind_name.iface_pin"
                    if let Some((prefix, suffix)) = name.split_once('.') {
                        if prefix == bind_name.as_str() {
                            bound_iface_pins.insert(suffix.to_string());
                        }
                    }
                    // Also handle exact match: if the name IS the bind_name,
                    // this means all interface pins are aggregated under one name
                }
            }

            // Find missing interface pins
            let mut missing: Vec<String> = Vec::new();
            for iface_pin_name in iface_pins.keys() {
                if !bound_iface_pins.contains(iface_pin_name) {
                    missing.push(iface_pin_name.clone());
                }
            }

            // Only flag if some but not all pins are bound (completely unbound
            // is caught by other checks)
            if !missing.is_empty() && !bound_iface_pins.is_empty() {
                acc.push(CheckResult {
                    check_name: "interface",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}': interface '{}' requires {} pin(s), but '{}' \
                         only binds {} of them. Missing: {}",
                        comp.name,
                        iface_name,
                        iface_pins.len(),
                        bind_name,
                        bound_iface_pins.len(),
                        missing.join(", ")
                    ),
                    code: 3101,
                });
            }
        }
    }
}

// ============================================================================
// C4-ext: Interface role referenced exists in definition
// ============================================================================

/// When a component's param selects an interface role (e.g. `role=DCE`),
/// verify that the role actually exists in the interface definition.
fn check_iface_role_exists(acc: &mut CheckAccumulator) {
    let comps = crate::db::cmie::tables::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Check component params that reference interface roles
        for d in comp.params.iter() {
            use crate::semantic::basic::mc_param_type::McParamTypeKind;

            if let McParamTypeKind::InterfaceWithRole {
                ref class_name,
                ref role_val,
            } = d.param_type.kind
            {
                // Look up the interface in the workspace
                let ifaces = crate::db::cmie::tables::WORKSPACE.interfaces.borrow();
                let mut found_role = false;
                let mut found_iface = false;

                for ie in ifaces.iter() {
                    if ie.key().ident.to_string() == *class_name {
                        found_iface = true;
                        // Check if role exists
                        for role in &ie.value().roles {
                            if role.name.to_string() == *role_val {
                                found_role = true;
                                break;
                            }
                        }
                        break;
                    }
                }

                if found_iface && !found_role {
                    if let Some(pname) = d.get_primary_name() {
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}': param '{}' references role '{}' in interface \
                                 '{}', but that role is not defined in the interface. \
                                 Available roles: {}",
                                comp.name,
                                pname,
                                role_val,
                                class_name,
                                ifaces
                                    .iter()
                                    .find(|e| e.key().ident.to_string() == *class_name)
                                    .map(|e| {
                                        e.value()
                                            .roles
                                            .iter()
                                            .map(|r| r.name.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    })
                                    .unwrap_or_default()
                            ),
                            code: 3102,
                        });
                    }
                }

                if !found_iface {
                    if let Some(pname) = d.get_primary_name() {
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}': param '{}' references interface '{}' \
                                 which is not loaded.",
                                comp.name, pname, class_name
                            ),
                            code: 3103,
                        });
                    }
                }
            }
        }
    }
}

// ============================================================================
// F3: Deprecated CMIE usage
// ============================================================================

/// Detect when a component uses a deprecated interface or component.
/// Deprecation is indicated by a `deprecated` attribute on the definition.
fn check_deprecated_cmie_usage(acc: &mut CheckAccumulator) {
    // Collect deprecated CMIE names
    let deprecated_comps: HashSet<String> = {
        let mut s = HashSet::new();
        let comps = crate::db::cmie::tables::WORKSPACE.components.borrow();
        for e in comps.iter() {
            let c = e.value();
            if has_deprecated_attr(&c.attrs) {
                s.insert(e.key().ident.to_string());
            }
        }
        s
    };

    let deprecated_ifaces: HashSet<String> = {
        let mut s = HashSet::new();
        let ifaces = crate::db::cmie::tables::WORKSPACE.interfaces.borrow();
        for e in ifaces.iter() {
            let i = e.value();
            if has_deprecated_attr(&i.attrs) {
                s.insert(e.key().ident.to_string());
            }
        }
        s
    };

    // Check component interface bindings for deprecated interfaces
    {
        let comps = crate::db::cmie::tables::WORKSPACE.components.borrow();
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp = entry.value();

            for (_bind_name, port) in &comp.pins.names_to_id {
                if let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port {
                    let iface_name = iface.name.to_string();
                    if deprecated_ifaces.contains(&iface_name) {
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Info,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}' uses interface '{}' which is deprecated.",
                                comp.name, iface_name
                            ),
                            code: 3104,
                        });
                    }
                }
            }

            // Check component declaration params for deprecated classes
            for d in comp.params.iter() {
                if let Some(class_name) = d.get_class_name() {
                    if deprecated_ifaces.contains(&class_name)
                        || deprecated_comps.contains(&class_name)
                    {
                        if let Some(pname) = d.get_primary_name() {
                            acc.push(CheckResult {
                                check_name: "interface",
                                severity: CheckSeverity::Info,
                                uri: Some(uri.clone()),
                                span: Some(comp.span.start..comp.span.end),
                                message: format!(
                                    "Component '{}': param '{}' references '{}' which is deprecated.",
                                    comp.name, pname, class_name
                                ),
                                code: 3104,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check module instances for deprecated components
    {
        let modules = crate::db::cmie::tables::WORKSPACE.modules.borrow();
        for entry in modules.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let m = entry.value();

            for (_inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
                let class_name = match instance {
                    crate::McInstance::Component(c2) => c2.name.to_string(),
                    crate::McInstance::Interface(i2) => i2.name.to_string(),
                    _ => continue,
                };

                if deprecated_comps.contains(&class_name) || deprecated_ifaces.contains(&class_name)
                {
                    acc.push(CheckResult {
                        check_name: "interface",
                        severity: CheckSeverity::Info,
                        uri: Some(uri.clone()),
                        span: Some(m.span.start..m.span.end),
                        message: format!(
                            "Module '{}' uses '{}' which is deprecated.",
                            entry.key().ident,
                            class_name
                        ),
                        code: 3104,
                    });
                }
            }
        }
    }
}

/// Check if an attribute set contains a `deprecated` marker.
fn has_deprecated_attr(attrs: &crate::semantic::component::mc_attr::McAttributes) -> bool {
    attrs.iter().any(|a| {
        let key = a.id.to_string();
        key == "deprecated" || key == "obsolete" || key == "status"
    })
}

// ============================================================================
// Cross-module: instance port member references
// ============================================================================

/// In module body lines (and function body lines), when `instance_name.port_name`
/// is used, verify that `port_name` actually exists as a port of `instance_name`'s
/// component/interface/module type.
fn check_module_member_refs(acc: &mut CheckAccumulator) {
    let modules = crate::db::cmie::tables::WORKSPACE.modules.borrow();

    // Pre-build: class name → port names set (for components and interfaces)
    let comp_ports: std::collections::HashMap<String, HashSet<String>> = {
        let mut m = std::collections::HashMap::new();
        let comps = crate::db::cmie::tables::WORKSPACE.components.borrow();
        for e in comps.iter() {
            let mut ports: HashSet<String> = e.value().pins.names_to_id.keys().cloned().collect();
            // Also include pin names from individual McPin entries
            for pin in e.value().pins.pins.values() {
                for n in &pin.names {
                    ports.insert(n.clone());
                }
            }
            m.insert(e.key().ident.to_string(), ports);
        }
        m
    };

    let iface_ports: std::collections::HashMap<String, HashSet<String>> = {
        let mut m = std::collections::HashMap::new();
        let ifaces = crate::db::cmie::tables::WORKSPACE.interfaces.borrow();
        for e in ifaces.iter() {
            let ports: HashSet<String> = e.value().pins.names_to_id.keys().cloned().collect();
            m.insert(e.key().ident.to_string(), ports);
        }
        m
    };

    // Pre-build: module name → port names (module boundary ports)
    let mod_ports: std::collections::HashMap<String, HashSet<String>> = {
        let mut m = std::collections::HashMap::new();
        for entry in modules.iter() {
            let ports: HashSet<String> =
                entry.value().insts.iter_instance_names().cloned().collect();
            m.insert(entry.key().ident.to_string(), ports);
        }
        m
    };

    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let mod_span = Some(m.span.start..m.span.end);

        // map: instance name → class name
        let inst_class: std::collections::HashMap<String, String> = {
            let mut map = std::collections::HashMap::new();
            for (iname, (_iotype, inst)) in m.insts.iter_with_iotype() {
                let class = match inst {
                    crate::McInstance::Component(c2) => Some(c2.name.to_string()),
                    crate::McInstance::Interface(i2) => Some(i2.name.to_string()),
                    crate::McInstance::Module(m2) => Some(m2.name.to_string()),
                    _ => None,
                };
                if let Some(c) = class {
                    map.insert(iname.to_string(), c);
                }
            }
            map
        };

        if inst_class.is_empty() {
            continue;
        }

        // Check module top-level body lines
        for phrase in &m.lines {
            check_phrase_member_refs(
                phrase,
                &inst_class,
                &comp_ports,
                &iface_ports,
                &mod_ports,
                &uri,
                &entry.key().ident.to_string(),
                &mod_span,
                acc,
            );
        }

        // Check function body lines within the module
        for func in m.funcs.iter() {
            for phrase in &func.lines {
                check_phrase_member_refs(
                    phrase,
                    &inst_class,
                    &comp_ports,
                    &iface_ports,
                    &mod_ports,
                    &uri,
                    &entry.key().ident.to_string(),
                    &mod_span,
                    acc,
                );
            }
        }
    }
}

/// Check a single McPhrase for `instance.port` patterns and verify port exists.
fn check_phrase_member_refs(
    phrase: &crate::semantic::basic::mc_phrase::McPhrase,
    inst_class: &std::collections::HashMap<String, String>,
    comp_ports: &std::collections::HashMap<String, HashSet<String>>,
    iface_ports: &std::collections::HashMap<String, HashSet<String>>,
    mod_ports: &std::collections::HashMap<String, HashSet<String>>,
    uri: &str,
    mod_name: &str,
    module_span: &Option<std::ops::Range<usize>>,
    acc: &mut CheckAccumulator,
) {
    let text = format!("{}", phrase);

    // Split by `->` first (the connection arrow), then by `,` for multiple endpoints
    for side in text.split("->") {
        for endpoint in side.split(',') {
            let endpoint = endpoint.trim();
            if endpoint.is_empty() || endpoint == "_" {
                continue;
            }

            // Extract `instance.port` pattern: take first dot-separated pair
            if let Some((first, rest)) = endpoint.split_once('.') {
                let first = first.trim();
                // rest may contain further dots (e.g. "VDD.something")
                let port_name = rest.split('.').next().unwrap_or(rest).trim();

                if first.is_empty() || port_name.is_empty() {
                    continue;
                }

                // Try to match `first` against known instance names
                let class_name = inst_class.get(first).cloned();

                if class_name.is_none() {
                    continue;
                }
                let class_name = class_name.unwrap();

                // Check component ports
                if let Some(ports) = comp_ports.get(&class_name) {
                    if !ports.contains(port_name) && !port_name.contains('.') {
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.to_string()),
                            span: module_span.clone(),
                            message: format!(
                                "Module '{}': '{}.{}' — '{}' is not a defined port of \
                                 component '{}'. Available: {}",
                                mod_name,
                                first,
                                port_name,
                                port_name,
                                class_name,
                                summarize_names(ports)
                            ),
                            code: 3105,
                        });
                    }
                    continue; // Found in component ports — done
                }

                // Check interface ports
                if let Some(ports) = iface_ports.get(&class_name) {
                    if !ports.contains(port_name) && !port_name.contains('.') {
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.to_string()),
                            span: module_span.clone(),
                            message: format!(
                                "Module '{}': '{}.{}' — '{}' is not a defined port of \
                                 interface '{}'.",
                                mod_name, first, port_name, port_name, class_name
                            ),
                            code: 3105,
                        });
                    }
                    continue;
                }

                // Check module ports
                if let Some(ports) = mod_ports.get(&class_name) {
                    if !ports.contains(port_name) && !port_name.contains('.') {
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.to_string()),
                            span: module_span.clone(),
                            message: format!(
                                "Module '{}': '{}.{}' — '{}' is not a defined port of \
                                 module '{}'.",
                                mod_name, first, port_name, port_name, class_name
                            ),
                            code: 3105,
                        });
                    }
                }
            }
        }
    }
}

/// Format a set of names for diagnostic display (max 10).
fn summarize_names(names: &HashSet<String>) -> String {
    if names.len() <= 10 {
        let mut v: Vec<_> = names.iter().collect();
        v.sort();
        v.into_iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        format!("{} ports", names.len())
    }
}
