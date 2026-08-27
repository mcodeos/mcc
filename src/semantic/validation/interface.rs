// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Interface binding validation.
//!
//! Checks:
//!   I4-ext — all interface pins are bound to physical pins in the component
//!   C4-ext — interface roles referenced in component must exist in the interface definition
//!   F3 — deprecated CMIE usage (component extends deprecated interface/component)

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_iface_pin_completeness(acc); // I4-ext
        check_iface_role_exists(acc); // C4-ext
        check_deprecated_cmie_usage(acc); // F3
    }
}

// ============================================================================
// I4-ext: All interface pins bound to physical pins
// ============================================================================

/// When a component binds to an interface via e.g. `pins=[1=SPI.MOSI, 2=SPI.MISO]`,
/// every pin defined in the interface must be mapped to at least one physical pin.
/// Missing bindings mean the interface contract is not fulfilled.
fn check_iface_pin_completeness(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
            // Use the interface's real member pin names (source declaration
            // order) instead of raw `names_to_id` keys: the key table also
            // holds container keys for list-form bindings (e.g. the `[VBUS,
            // GND]` binding name inside `[1,5] = [VBUS, GND]::DC(5V)`), which
            // a component can never bind and would spuriously show as missing.
            let iface_pins: HashSet<String> = iface.base.pins.member_names().into_iter().collect();

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
            for iface_pin_name in &iface_pins {
                if !bound_iface_pins.contains(iface_pin_name) {
                    missing.push(iface_pin_name.clone());
                }
            }

            // Only flag if some but not all pins are bound (completely unbound
            // is caught by other checks)
            if bound_iface_pins.len() < iface_pins.len()
                && !missing.is_empty()
                && !bound_iface_pins.is_empty()
            {
                // Point at the interface binding itself (`ADC` in
                // `io [16, 17] = ADC::ADC.DIFF(...)`), not the component class
                // name. `pin_name_spans` records the leading-identifier span for
                // every `names_to_id` key; fall back to the component name span.
                let span = comp
                    .pins
                    .pin_name_spans
                    .get(bind_name)
                    .cloned()
                    .unwrap_or_else(|| comp.span.start..comp.span.end);
                acc.push(CheckResult {
                    check_name: "interface",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(span),
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
                    code: crate::errcodes::IFACE_PINS_NOT_ALL_BOUND,
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
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
                let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
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
                            code: crate::errcodes::IFACE_ROLE_NOT_FOUND,
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
                            code: crate::errcodes::IFACE_NOT_LOADED,
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
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
        let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
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
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
                            code: crate::errcodes::IFACE_DEPRECATED_CMIE,
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
                                code: crate::errcodes::IFACE_DEPRECATED_CMIE,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check module instances for deprecated components
    {
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let m = entry.value();

            for (_inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
                let class_name = match instance {
                    crate::McInstance::Component(c2) => c2.base.name.to_string(),
                    crate::McInstance::Interface(i2) => i2.base.name.to_string(),
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
                        code: crate::errcodes::IFACE_DEPRECATED_CMIE,
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
