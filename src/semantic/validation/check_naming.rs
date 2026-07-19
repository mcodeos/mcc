// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Naming convention checks: component case, pin name conventions, instance name length.
//!
//! Checks:
//!   J1 — lowercase component name (should be UPPER_SNAKE)
//!   N9 — mixed pin naming conventions within a single component
//!   N10 — single-character or overly short instance names
//!   N11 — pin names that are purely numeric (confusing with pin IDs)

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashSet;

pub struct NamingCheck;

impl ValidationCheck for NamingCheck {
    fn name(&self) -> &'static str {
        "naming"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        // Collect library CMIE names for shadow detection context
        let lib_names: HashSet<String> = {
            let mut s = HashSet::new();
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for entry in comps.iter() {
                s.insert(entry.key().ident.to_string());
            }
            let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
            for entry in ifaces.iter() {
                s.insert(entry.key().ident.to_string());
            }
            s
        };

        check_lowercase_components(acc); // J1
        check_mixed_pin_naming(acc); // N9
        check_short_instance_names(acc); // N10
        check_numeric_pin_names(acc); // N11
        check_lib_name_shadow(acc, &lib_names); // extended J3
    }
}

// ============================================================================
// J1: Lowercase component names (should be UPPER_SNAKE)
// ============================================================================

fn check_lowercase_components(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let comp = entry.value();
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        // Component names should start with uppercase or digit (like 74HC)
        if let Some(first) = name.chars().next() {
            if first.is_lowercase() && !name.contains('.') {
                // Names with dots like "AMP.BUFFER" are fine
                acc.push(CheckResult {
                    check_name: "naming",
                    severity: CheckSeverity::Info,
                    uri: Some(uri),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}' starts with lowercase. Convention is UPPER_SNAKE.",
                        name
                    ),
                    code: 2201,
                });
            }
        }
    }
}

// ============================================================================
// N9: Mixed pin naming conventions within a single component
// ============================================================================

/// Pin names within a component should follow a consistent convention.
/// Mixing UPPER_SNAKE (e.g., `CHIP_SELECT`) with lower_snake (e.g., `chip_select`)
/// or PascalCase (e.g., `ChipSelect`) is confusing.
fn check_mixed_pin_naming(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        let pin_names: Vec<&str> = comp
            .pins
            .names_to_id
            .keys()
            .map(|s| s.as_str())
            .filter(|n| n.len() >= 2 && !n.contains('.') && *n != "NC" && *n != "nc")
            .collect();

        if pin_names.len() < 3 {
            continue;
        }

        let has_upper_snake = pin_names.iter().any(|n| {
            n.chars()
                .all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
                && n.contains('_')
        });
        let has_lower_snake = pin_names.iter().any(|n| {
            n.chars()
                .all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
                && n.contains('_')
        });
        let has_upper_flat = pin_names
            .iter()
            .any(|n| n.chars().all(|c| c.is_uppercase() || c.is_ascii_digit()) && !n.contains('_'));
        let has_lower_flat = pin_names
            .iter()
            .any(|n| n.chars().all(|c| c.is_lowercase() || c.is_ascii_digit()) && !n.contains('_'));

        let conventions = [
            has_upper_snake,
            has_lower_snake,
            has_upper_flat,
            has_lower_flat,
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        if conventions >= 3 {
            // Detect specific mixed pairs
            let mut styles = Vec::new();
            if has_upper_snake {
                styles.push("UPPER_SNAKE");
            }
            if has_lower_snake {
                styles.push("lower_snake");
            }
            if has_upper_flat {
                styles.push("UPPERFLAT");
            }
            if has_lower_flat {
                styles.push("lowerflat");
            }

            acc.push(CheckResult {
                check_name: "naming",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has mixed pin naming conventions: {}. \
                     Consider using a single consistent style (e.g., UPPER_SNAKE).",
                    comp.name,
                    styles.join(", ")
                ),
                code: 2205,
            });
        }
    }
}

// ============================================================================
// N10: Single-character or overly short instance names
// ============================================================================

/// Instance names that are single characters (e.g., `R1 r1` → `r1` is fine,
/// but `RES r` is too short) make schematics harder to read. Flag instance
/// names that are single characters and not obviously a numbered reference.
fn check_short_instance_names(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        for name in m.insts.iter_instance_names() {
            // Skip special names
            if name.starts_with('@') || name.starts_with('[') {
                continue;
            }

            // Single character names like "A", "B", "X" are too cryptic
            if name.len() == 1 {
                acc.push(CheckResult {
                    check_name: "naming",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(m.span.start..m.span.end),
                    message: format!(
                        "Module '{}': instance '{}' is a single character. \
                         Use descriptive names like 'r1', 'led1', 'usb_socket'.",
                        entry.key().ident,
                        name
                    ),
                    code: 2206,
                });
            }
        }
    }
}

// ============================================================================
// N11: Pin names that are purely numeric
// ============================================================================

/// Pin names like "1", "2", "3" (instead of functional names like "VCC",
/// "GND", "A1") lose semantic meaning. While common for simple passive
/// components (resistors, capacitors), for active components with >3 pins,
/// purely numeric names suggest incomplete documentation.
fn check_numeric_pin_names(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        let pin_count = comp.pins.names_to_id.len();
        if pin_count <= 3 {
            continue; // Passives often have numeric pins (1, 2, 3)
        }

        let numeric_count = comp
            .pins
            .names_to_id
            .keys()
            .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
            .count();

        if numeric_count as f64 / pin_count as f64 > 0.8 && pin_count > 4 {
            acc.push(CheckResult {
                check_name: "naming",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}': {}/{} pin names are purely numeric. \
                     Consider adding functional names for clarity.",
                    comp.name, numeric_count, pin_count
                ),
                code: 2207,
            });
        }
    }
}

// ============================================================================
// Extended J3: Entity names that shadow library CMIE names
// ============================================================================

/// User-defined module port/instance names that happen to match a known
/// library component, interface, or enum name create ambiguity.
fn check_lib_name_shadow(acc: &mut CheckAccumulator, lib_names: &HashSet<String>) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        for port_name in m.insts.iter_instance_names() {
            if lib_names.contains(port_name) {
                acc.push(CheckResult {
                    check_name: "naming",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(m.span.start..m.span.end),
                    message: format!(
                        "Module '{}': port/instance '{}' shadows a library CMIE name. \
                         This may cause confusion when resolving type references.",
                        entry.key().ident,
                        port_name
                    ),
                    code: 2208,
                });
            }
        }

        // Also check module param names
        for d in m.params.iter() {
            if let Some(pname) = d.get_primary_name() {
                if lib_names.contains(&pname) {
                    acc.push(CheckResult {
                        check_name: "naming",
                        severity: CheckSeverity::Info,
                        uri: Some(uri.clone()),
                        span: Some(m.span.start..m.span.end),
                        message: format!(
                            "Module '{}': param '{}' shares a name with a library CMIE. \
                             Consider a different name to avoid ambiguity.",
                            entry.key().ident,
                            pname
                        ),
                        code: 2209,
                    });
                }
            }
        }
    }
}
