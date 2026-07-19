// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Attribute validation checks.
//!
//! Checks:
//!   N1 — attribute name uses reserved keyword
//!   N2 — dotted attribute name with unresolvable segments
//!   N4 — excessive nested attribute set depth (>16)
//!   N7 — `pins.X` where X is not a recognized pin group
//!   N8 — overlapping `pins =` and `pins.N =` assignments

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashSet;

pub struct AttrsCheck;

impl ValidationCheck for AttrsCheck {
    fn name(&self) -> &'static str {
        "attrs"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        let comps = crate::db::cmie::tables::WORKSPACE.components.borrow();
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp = entry.value();
            check_reserved_attr_name(comp, &uri, acc); // N1
            check_unresolvable_dotted_name(comp, &uri, acc); // N2
            check_nesting_depth(comp, &uri, acc); // N4
            check_pins_group(comp, &uri, acc); // N7
            check_pins_overlap(comp, &uri, acc); // N8
        }
    }
}

/// Reserved keywords that should not be used as attribute names.
const RESERVED_KEYWORDS: &[&str] = &[
    "this", "pins", "role", "func", "return", "in", "out", "io", "ps", "anl", "nc", "if", "else",
];

/// N1: Attribute id or dot-segment uses a reserved keyword.
fn check_reserved_attr_name(comp: &crate::McComponent, uri: &str, acc: &mut CheckAccumulator) {
    for attr in comp.attrs.iter() {
        let attr_id = attr.id.to_string();
        // Check the full id
        for kw in RESERVED_KEYWORDS {
            if attr_id == *kw {
                acc.push(CheckResult {
                    check_name: "attrs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: attr.key_span.clone(),
                    message: format!(
                        "Attribute '{}' in component '{}' uses reserved keyword '{}'.",
                        attr_id,
                        entry_key_ident(comp),
                        kw
                    ),
                    code: 2801,
                });
                continue;
            }
        }
        // Check each dot-segment
        for seg in attr_id.split('.') {
            for kw in RESERVED_KEYWORDS {
                if seg == *kw {
                    acc.push(CheckResult {
                        check_name: "attrs",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.to_string()),
                        span: attr.key_span.clone(),
                        message: format!(
                            "Attribute '{}' in component '{}' has segment '{}' which is a reserved keyword.",
                            attr_id,
                            entry_key_ident(comp),
                            seg
                        ),
                        code: 2801,
                    });
                }
            }
        }
    }
}

/// N2: Dotted attribute name where first segment is not the component name
/// nor a known first-level attribute key.
fn check_unresolvable_dotted_name(
    comp: &crate::McComponent,
    uri: &str,
    acc: &mut CheckAccumulator,
) {
    let comp_name = entry_key_ident(comp);

    // Collect known first-level attribute keys from this component
    let known_keys: HashSet<String> = comp
        .attrs
        .iter()
        .map(|a| {
            // First segment of a dotted name, or the whole name if no dots
            a.id.to_string().split('.').next().unwrap_or("").to_string()
        })
        .collect();

    for attr in comp.attrs.iter() {
        let attr_id = attr.id.to_string();
        if !attr_id.contains('.') {
            continue;
        }
        let first_seg = attr_id.split('.').next().unwrap_or("");
        if first_seg != comp_name && !known_keys.contains(first_seg) && !first_seg.is_empty() {
            acc.push(CheckResult {
                check_name: "attrs",
                severity: CheckSeverity::Error,
                uri: Some(uri.to_string()),
                span: attr.key_span.clone(),
                message: format!(
                    "Attribute '{}' starts with '{}' which is not the component name \
                     or a recognized attribute group.",
                    attr_id, first_seg
                ),
                code: 2802,
            });
        }
    }
}

/// N4: Recursively walk attribute value tree and warn if depth exceeds 16.
fn check_nesting_depth(comp: &crate::McComponent, uri: &str, acc: &mut CheckAccumulator) {
    for attr in comp.attrs.iter() {
        for val in &attr.values {
            let depth = attr_val_depth(val, 0);
            if depth > 16 {
                acc.push(CheckResult {
                    check_name: "attrs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: attr.key_span.clone(),
                    message: format!(
                        "Attribute '{}' has nested depth {} (>16). Consider flattening.",
                        attr.id, depth
                    ),
                    code: 2804,
                });
            }
        }
    }
}

/// Recursively compute the nesting depth of an McAttrVal.
fn attr_val_depth(val: &crate::semantic::component::mc_attr::McAttrVal, current: u32) -> u32 {
    match val {
        crate::semantic::component::mc_attr::McAttrVal::Attributes(attrs) => {
            let mut max_child = current + 1;
            for child in attrs.iter() {
                for child_val in &child.values {
                    let d = attr_val_depth(child_val, current + 1);
                    if d > max_child {
                        max_child = d;
                    }
                }
            }
            max_child
        }
        _ => current,
    }
}

/// N7: `pins.X` where X is not a recognized pin group name.
fn check_pins_group(comp: &crate::McComponent, uri: &str, acc: &mut CheckAccumulator) {
    let pin_names: HashSet<&str> = comp.pins.names_to_id.keys().map(|s| s.as_str()).collect();

    for attr in comp.attrs.iter() {
        let attr_id = attr.id.to_string();
        if attr_id.starts_with("pins.") {
            let suffix = &attr_id[5..]; // strip "pins."
            if suffix.is_empty() {
                continue; // bare "pins" key without dot, handled by N8
            }
            // Take the first segment (e.g., "GPIO" from "GPIO.voltage")
            let group = suffix.split('.').next().unwrap_or(suffix);
            if !pin_names.contains(group) && !group.is_empty() {
                acc.push(CheckResult {
                    check_name: "attrs",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.to_string()),
                    span: attr.key_span.clone(),
                    message: format!(
                        "Attribute '{}' references pin group '{}' which is not defined \
                         in component '{}'.",
                        attr_id,
                        group,
                        entry_key_ident(comp)
                    ),
                    code: 2805,
                });
            }
        }
    }
}

/// N8: Component has both bare `pins =` and `pins.N =` attributes — potential overlap.
fn check_pins_overlap(comp: &crate::McComponent, uri: &str, acc: &mut CheckAccumulator) {
    let has_bare_pins = comp.attrs.iter().any(|a| a.id.to_string() == "pins");
    let has_dotted_pins = comp
        .attrs
        .iter()
        .any(|a| a.id.to_string().starts_with("pins."));

    if has_bare_pins && has_dotted_pins {
        acc.push(CheckResult {
            check_name: "attrs",
            severity: CheckSeverity::Warning,
            uri: Some(uri.to_string()),
            span: Some(comp.span.start..comp.span.end),
            message: format!(
                "Component '{}' has both 'pins =' and 'pins.X =' attributes. \
                 These may conflict.",
                entry_key_ident(comp)
            ),
            code: 2806,
        });
    }
}

/// Helper: get the ident string from a component reference.
/// We don't have direct access to the key from the value, so we use the name field.
fn entry_key_ident(comp: &crate::McComponent) -> String {
    comp.name.to_string()
}
