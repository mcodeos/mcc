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

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        let comps = crate::definition_space().workspace_components();
        for (sn, comp) in comps.iter() {
            let uri = sn.uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            check_reserved_attr_name(comp, &uri, acc); // N1
            check_unresolvable_dotted_name(comp, &uri, acc); // N2
            check_nesting_depth(comp, &uri, acc); // N4
            check_pins_group(comp, &uri, acc); // N7
            check_pins_overlap(comp, &uri, acc); // N8
        }
    }
}

/// N1: Attribute id or dot-segment uses a reserved keyword.
///
/// "Reserved" is a column of the key registry
/// (`semantic::basic::attr_keys`), not a list kept here: one dictionary, one
/// place to add a key.
fn check_reserved_attr_name(comp: &crate::McComponent, uri: &str, acc: &mut CheckAccumulator) {
    use crate::semantic::basic::attr_keys;
    for attr in comp.attrs.iter() {
        let attr_id = attr.id.to_string();
        // One row reports this code once. A key whose first segment glues a
        // subscript onto a word (`pins[1]`, `x[0]`) is reported where the key is
        // built, anchored on this same span, so a row that already carries the
        // code is left to that report — it says the stronger thing.
        let mut reported = crate::db::diagnostic::diagnostic::has_code_at(
            crate::errcodes::ATTR_RESERVED_KEYWORD,
            &uri.to_string(),
            attr.key_span.as_ref().map_or(0, |span| span.start as u32),
        );
        // Check the full id
        if !reported && attr_keys::is_reserved(&attr_id) {
            reported = true;
            acc.push(CheckResult {
                check_name: "attrs",
                severity: CheckSeverity::Warning,
                uri: Some(uri.to_string()),
                span: attr.key_span.clone(),
                message: format!(
                    "Attribute '{}' in component '{}' uses reserved keyword '{}'.",
                    attr_id,
                    entry_key_ident(comp),
                    attr_id
                ),
                code: crate::errcodes::ATTR_RESERVED_KEYWORD,
            });
        }
        // Check each dot-segment
        for seg in attr_id.split('.') {
            if !reported && attr_keys::is_reserved(seg) {
                reported = true;
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
                    code: crate::errcodes::ATTR_RESERVED_KEYWORD,
                });
            }
        }
    }
}

/// N2: Dotted attribute name whose leading segments name neither the component
/// nor a registered first-level attribute key.
///
/// Two name witnesses resolve the leading segments, each as a *segment
/// prefix* — the attribute id spells the name and keeps at least one key
/// segment after it (`TTL.D.partno` on `component TTL.D.SN74LVC1G175`):
/// the component's own full dotted name, and — on a materialized variant
/// copy — the declared base name, whose attributes ride the clone
/// (adoption.rs `materialize_variant`). Comparing the first segment against
/// the full name string can never hold for a dotted name (U194): `TTL` !=
/// `TTL.D.SN74LVC1G175` made every multi-point attribute on a named
/// component — and its re-report on every variant clone — an error.
///
/// Anything else is resolved against the key registry
/// (`semantic::basic::attr_keys`, contract-design.md §1.7) — a *closed* set, so
/// the check can actually fire. An earlier form collected the first segments
/// of the component's own attributes as the known set, which always contained
/// the segment under test and made the predicate a tautology.
fn check_unresolvable_dotted_name(
    comp: &crate::McComponent,
    uri: &str,
    acc: &mut CheckAccumulator,
) {
    let mut witnesses: Vec<&crate::semantic::basic::mc_ids::McIds> = vec![&comp.name];
    if let Some(base) = &comp.variant_base {
        witnesses.push(base);
    }

    for attr in comp.attrs.iter() {
        if attr.id.segments.len() <= 1 {
            continue;
        }
        if witnesses
            .iter()
            .any(|w| spells_name_prefix(&attr.id, w))
        {
            continue;
        }
        let first_seg = attr.id.segments[0].to_string();
        if first_seg.is_empty() || crate::semantic::basic::attr_keys::is_known_key(&first_seg) {
            continue;
        }
        acc.push(CheckResult {
            check_name: "attrs",
            severity: CheckSeverity::Error,
            uri: Some(uri.to_string()),
            span: attr.key_span.clone(),
            message: format!(
                "Attribute '{}' starts with '{}', which is neither the name of component '{}' \
                 nor a registered attribute key.",
                attr.id,
                first_seg,
                entry_key_ident(comp)
            ),
            code: crate::errcodes::ATTR_DOTTED_NAME_UNRESOLVED,
        });
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
                    code: crate::errcodes::ATTR_NESTING_TOO_DEEP,
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
                    code: crate::errcodes::ATTR_PIN_GROUP_UNDEFINED,
                });
            }
        }
    }
}

/// N8: Component has both bare `pins =` and `pins.N =` attributes — potential overlap.
fn check_pins_overlap(comp: &crate::McComponent, uri: &str, acc: &mut CheckAccumulator) {
    let has_bare_pins = comp
        .attrs
        .iter()
        .any(|a| a.id.segments.len() == 1 && a.id.segments[0].to_string() == "pins");
    let has_dotted_pins = comp
        .attrs
        .iter()
        .any(|a| a.id.segments.len() > 1 && a.id.segments[0].to_string() == "pins");

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
            code: crate::errcodes::PINS_PLUS_AND_PINS_CONFLICT,
        });
    }
}

/// Helper: get the ident string from a component reference.
/// We don't have direct access to the key from the value, so we use the name field.
fn entry_key_ident(comp: &crate::McComponent) -> String {
    comp.name.to_string()
}

/// `id` spells `name` as its leading segments and keeps at least one segment
/// beyond it (`TTL.D.partno` against `TTL.D`) — the segment-prefix form of
/// "this attribute belongs to that named definition" (N2's name witnesses).
fn spells_name_prefix(
    id: &crate::semantic::basic::mc_ids::McIds,
    name: &crate::semantic::basic::mc_ids::McIds,
) -> bool {
    let n = name.segments.len();
    id.segments.len() > n && id.segments[..n] == name.segments[..]
}
