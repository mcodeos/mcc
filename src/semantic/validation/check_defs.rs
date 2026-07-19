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

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
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

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
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
                code: 2701,
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
                code: 2701,
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
                code: 2701,
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
    // Build the known set of all CMIE names
    let mut known: HashSet<String> = HashSet::new();
    {
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
        for e in comps.iter() {
            known.insert(e.key().ident.to_string());
        }
    }
    {
        let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
        for e in ifaces.iter() {
            known.insert(e.key().ident.to_string());
        }
    }
    {
        let enums = &crate::db::cmie::tables::WORKSPACE.enums;
        for e in enums.iter() {
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
                    let iface_name = iface.name.to_string();
                    if !known.contains(&iface_name) {
                        acc.push(CheckResult {
                            check_name: "defs",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}' binds to interface '{}' which is not loaded.",
                                entry.key().ident,
                                iface_name
                            ),
                            code: 2702,
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
                            code: 2702,
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
                    code: 2703,
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
                    code: 2704,
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
                    code: 2704,
                });
            }
        }
    }
}
