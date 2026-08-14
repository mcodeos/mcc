// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Definition structure validation.
//!
//! Checks:
//!   A4 — interface/component name collision (same name used for both)
//!   A5 — missing required CMIE (instance class not found in any table)
//!   M2 — `.int` suffix on class name in wrong context (component)
//!   M5 — `.int` suffix on enum/interface

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::HashSet;

pub struct DefsCheck;

impl ValidationCheck for DefsCheck {
    fn name(&self) -> &'static str {
        "defs"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_name_collision(acc); // A4
        check_missing_cmie(acc); // A5
        check_int_suffix(acc); // M2, M5
    }
}

/// A4: Same ident used for both a component and an interface.
fn check_name_collision(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
    let enums = &crate::db::cmie::tables::WORKSPACE.enums;
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;

    // Collect names by CMIE kind
    let comp_names: HashSet<String> = comps.iter().map(|e| e.key().ident.to_string()).collect();
    let iface_names: HashSet<String> = ifaces.iter().map(|e| e.key().ident.to_string()).collect();
    let enum_names: HashSet<String> = enums.iter().map(|e| e.key().ident.to_string()).collect();
    let mod_names: HashSet<String> = modules.iter().map(|e| e.key().ident.to_string()).collect();

    // Component ↔ Interface collisions
    for name in comp_names.intersection(&iface_names) {
        let comp_spans: Vec<_> = comps
            .iter()
            .filter(|e| e.key().ident.to_string() == *name && !super::is_test_file(&e.key().uri))
            .map(|e| {
                (
                    e.key().uri.clone(),
                    e.value().span.start..e.value().span.end,
                )
            })
            .collect();
        for (uri, span) in &comp_spans {
            acc.push(CheckResult {
                check_name: "defs",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: Some(span.clone()),
                message: format!(
                    "'{}' is defined as both a component and an interface. \
                     This creates ambiguity for name resolution.",
                    name
                ),
                code: crate::errcodes::DEF_AMBIGUOUS_NAME,
            });
        }
    }

    // Interface ↔ Enum collisions
    for name in iface_names.intersection(&enum_names) {
        let iface_spans: Vec<_> = ifaces
            .iter()
            .filter(|e| e.key().ident.to_string() == *name && !super::is_test_file(&e.key().uri))
            .map(|e| {
                (
                    e.key().uri.clone(),
                    e.value().span.start..e.value().span.end,
                )
            })
            .collect();
        for (uri, span) in &iface_spans {
            acc.push(CheckResult {
                check_name: "defs",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: Some(span.clone()),
                message: format!(
                    "'{}' is defined as both an interface and an enum. \
                     This creates ambiguity for name resolution.",
                    name
                ),
                code: crate::errcodes::DEF_AMBIGUOUS_NAME,
            });
        }
    }

    // Component ↔ Module collisions
    for name in comp_names.intersection(&mod_names) {
        let comp_spans: Vec<_> = comps
            .iter()
            .filter(|e| e.key().ident.to_string() == *name && !super::is_test_file(&e.key().uri))
            .map(|e| {
                (
                    e.key().uri.clone(),
                    e.value().span.start..e.value().span.end,
                )
            })
            .collect();
        for (uri, span) in &comp_spans {
            acc.push(CheckResult {
                check_name: "defs",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(span.clone()),
                message: format!(
                    "'{}' is defined as both a component and a module. \
                     This creates ambiguity for name resolution.",
                    name
                ),
                code: crate::errcodes::DEF_AMBIGUOUS_NAME,
            });
        }
    }
}

/// A5: Instance references to classes that don't exist in any loaded table.
///
/// Extends the D2 check in check_extra.rs (which only covers module instances
/// with uppercase-starting names) by also checking:
///   - Pin interface bindings in components
///   - Declare class expressions in component params
fn check_missing_cmie(acc: &mut CheckAccumulator) {
    // Build the known set of all CMIE names.
    //
    // ★ System library definitions (mcode etc., loaded with is_system_lib=true)
    //   are stored in the GLOBAL tables (mcc_components / mcc_interfaces / etc.),
    //   while user-project definitions live in WORKSPACE.*. Both must be
    //   included, otherwise the validation would emit false "not loaded"
    //   warnings for every system lib name referenced from user code.
    let mut known: HashSet<String> = HashSet::new();
    {
        // Components: workspace + global
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
        for e in comps.iter() {
            known.insert(e.key().ident.to_string());
        }
        let global_comps = &crate::db::infra::global::mcc_components;
        for e in global_comps.iter() {
            known.insert(e.key().ident.to_string());
        }
    }
    {
        // Modules: workspace + global
        let mods = &crate::db::cmie::tables::WORKSPACE.modules;
        for e in mods.iter() {
            known.insert(e.key().ident.to_string());
        }
        let global_mods = &crate::db::infra::global::mcc_modules;
        for e in global_mods.iter() {
            known.insert(e.key().ident.to_string());
        }
    }
    {
        // Interfaces: workspace + global
        let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
        for e in ifaces.iter() {
            known.insert(e.key().ident.to_string());
        }
        let global_ifaces = &crate::db::infra::global::mcc_interfaces;
        for e in global_ifaces.iter() {
            known.insert(e.key().ident.to_string());
        }
    }
    {
        // Enums: workspace + global
        let enums = &crate::db::cmie::tables::WORKSPACE.enums;
        for e in enums.iter() {
            known.insert(e.key().ident.to_string());
        }
        let global_enums = &crate::db::infra::global::mcc_enums;
        for e in global_enums.iter() {
            known.insert(e.key().ident.to_string());
        }
    }
    {
        // Defines: workspace only (defines are not stored in global tables)
        let defs = &crate::db::cmie::tables::WORKSPACE.defines;
        for e in defs.iter() {
            known.insert(e.key().ident.to_string());
        }
    }

    // Check component pin interface bindings
    {
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp = entry.value();
            for (_pin_name, port) in &comp.pins.names_to_id {
                if let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port {
                    // ★ Use base_name() (the actual interface class name, e.g. "GPIO"),
                    //   not iface.name (which is the instance name and may contain
                    //   bus/list brackets like "GPIO[3, 4]").
                    let iface_name = iface.base_name();
                    if !known.contains(&iface_name) {
                        // Use pin_name_spans for accurate position
                        let span = comp
                            .pins
                            .pin_name_spans
                            .get(_pin_name)
                            .cloned()
                            .unwrap_or_else(|| comp.span.start..comp.span.end);
                        acc.push(CheckResult {
                            check_name: "defs",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(span),
                            message: format!(
                                "Component '{}' binds to interface '{}' which is not loaded.",
                                entry.key().ident,
                                iface_name
                            ),
                            code: crate::errcodes::DEF_REF_NOT_LOADED,
                        });
                    }
                }
            }
        }
    }

    // Check component param declare class expressions
    {
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp = entry.value();
            for d in comp.params.iter() {
                if let Some(class_name) = d.get_class_name() {
                    if !known.contains(&class_name) && !class_name.is_empty() {
                        acc.push(CheckResult {
                            check_name: "defs",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}' param references class '{}' which is not loaded.",
                                entry.key().ident,
                                class_name
                            ),
                            code: crate::errcodes::DEF_REF_NOT_LOADED,
                        });
                    }
                }
            }
        }
    }

    // Check module instances reference valid classes
    {
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let m = entry.value();
            for (_inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
                let class_name: Option<String> = match instance {
                    crate::McInstance::Component(c2) => Some(c2.base.name.to_string()),
                    crate::McInstance::Module(m2) => Some(m2.base.name.to_string()),
                    crate::McInstance::Interface(i2) => Some(i2.base.name.to_string()),
                    _ => None,
                };
                if let Some(cn) = class_name {
                    if !known.contains(&cn) && !cn.is_empty() {
                        acc.push(CheckResult {
                            check_name: "defs",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(m.span.start..m.span.end),
                            message: format!(
                                "Module '{}' references class '{}' which is not loaded.",
                                entry.key().ident,
                                cn
                            ),
                            code: crate::errcodes::DEF_REF_NOT_LOADED,
                        });
                    }
                }
            }
        }
    }
}

/// M2 + M5: `.int` suffix on component (M2) or enum/interface (M5).
fn check_int_suffix(acc: &mut CheckAccumulator) {
    // M2: .int suffix on component names
    {
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let name = entry.key().ident.to_string();
            if name.ends_with(".int") {
                let c = entry.value();
                acc.push(CheckResult {
                    check_name: "defs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(c.span.start..c.span.end),
                    message: format!(
                        "Component '{}' has '.int' suffix. '.int' is conventionally \
                         reserved for interface names.",
                        name
                    ),
                    code: crate::errcodes::COMPONENT_INT_SUFFIX,
                });
            }
        }
    }

    // M5: .int suffix on enum or interface names
    {
        let enums = &crate::db::cmie::tables::WORKSPACE.enums;
        for entry in enums.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let name = entry.key().ident.to_string();
            if name.ends_with(".int") {
                acc.push(CheckResult {
                    check_name: "defs",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(entry.value().span[0] as usize..entry.value().span[1] as usize),
                    message: format!(
                        "Enum '{}' has '.int' suffix, which is unconventional for enums.",
                        name
                    ),
                    code: crate::errcodes::ENUM_INT_SUFFIX,
                });
            }
        }
    }

    {
        let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
        for entry in ifaces.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let name = entry.key().ident.to_string();
            if name.ends_with(".int") {
                let iface = entry.value();
                acc.push(CheckResult {
                    check_name: "defs",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(iface.span.start..iface.span.end),
                    message: format!(
                        "Interface '{}' has '.int' suffix, which is unconventional \
                         for interfaces.",
                        name
                    ),
                    code: crate::errcodes::ENUM_INT_SUFFIX,
                });
            }
        }
    }
}
