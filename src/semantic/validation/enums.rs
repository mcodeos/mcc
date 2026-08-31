// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Enum definition validation.
//!
//! Checks:
//!   U2 — duplicate enum value names
//!   U3 — invalid enum member names (dotted, keyword-like, digit-start)
//!   N3 — self-referential attribute value (key = same-name value)

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::HashSet;

pub struct EnumsCheck;

impl ValidationCheck for EnumsCheck {
    fn name(&self) -> &'static str {
        "enums"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_duplicate_enum_values(acc); // U2
        check_invalid_enum_member_names(acc); // U3
        check_self_ref_attr(acc); // N3
        check_duplicate_attr_keys(acc); // N6-extra
    }
}

// ============================================================================
// U2: Duplicate enum value names
// ============================================================================

/// Within a single enum definition, all value names must be unique.
fn check_duplicate_enum_values(acc: &mut CheckAccumulator) {
    let enums = crate::definition_space().workspace_enums();
    for (sn, def) in enums.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        for val in &def.values {
            let name = val.name.to_string();
            if !seen.insert(name.clone()) {
                acc.push(CheckResult {
                    check_name: "enums",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(val.span[0] as usize..val.span[1] as usize),
                    message: format!(
                        "Enum '{}' has duplicate value '{}'. Enum values must be unique.",
                        def.name, name
                    ),
                    code: crate::errcodes::ENUM_DUPLICATE_VALUE,
                });
            }
        }
    }
}

// ============================================================================
// U3: Invalid enum member names
// ============================================================================

/// Enum member names should be simple identifiers, not:
///   - dotted names (like `UV.CAP`)
///   - value-like (starting with digit)
///   - empty
///   - reserved keywords
fn check_invalid_enum_member_names(acc: &mut CheckAccumulator) {
    let reserved: HashSet<&str> = [
        "this",
        "pins",
        "role",
        "func",
        "return",
        "in",
        "out",
        "io",
        "ps",
        "anl",
        "nc",
        "if",
        "else",
        "enum",
        "component",
        "module",
        "interface",
        "define",
        "use",
        "pub",
    ]
    .iter()
    .cloned()
    .collect();

    let enums = crate::definition_space().workspace_enums();
    for (sn, def) in enums.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for val in &def.values {
            let name = val.name.to_string();
            let span = val.span[0] as usize..val.span[1] as usize;

            // U3a: dotted names like `UV.CAP` or `AMP.BUFFER`
            if name.contains('.') {
                acc.push(CheckResult {
                    check_name: "enums",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(span.clone()),
                    message: format!(
                        "Enum '{}' member '{}' contains a dot. Enum values must be simple identifiers.",
                        def.name, name
                    ),
                    code: crate::errcodes::ENUM_MEMBER_DOT,
                });
            }

            // U3b: names that start with a digit (look like literal values)
            if name.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                acc.push(CheckResult {
                    check_name: "enums",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(span.clone()),
                    message: format!(
                        "Enum '{}' member '{}' starts with a digit. Enum values must be identifiers.",
                        def.name, name
                    ),
                    code: crate::errcodes::ENUM_MEMBER_LEADING_DIGIT,
                });
            }

            // U3c: reserved keyword used as enum value name
            if reserved.contains(name.as_str()) {
                acc.push(CheckResult {
                    check_name: "enums",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(span),
                    message: format!(
                        "Enum '{}' member '{}' is a reserved keyword. Consider a different name.",
                        def.name, name
                    ),
                    code: crate::errcodes::ENUM_MEMBER_RESERVED,
                });
            }
        }
    }
}

// ============================================================================
// N3: Self-referential attribute value
// ============================================================================

/// Detect attributes where the value is the same as the key name,
/// e.g. `manufacturer = manufacturer` which is likely a copy-paste mistake.
fn check_self_ref_attr(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        for attr in comp.attrs.iter() {
            let key_str = attr.id.to_string();
            if key_str.is_empty() {
                continue;
            }

            // Check each value — if any matches the key name, it's self-referential
            for val in &attr.values {
                let val_str = format!("{}", val).trim().to_string();
                if val_str == key_str {
                    acc.push(CheckResult {
                        check_name: "enums",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: attr.key_span.clone(),
                        message: format!(
                            "Component '{}': attribute '{}' has a self-referential value. \
                             '{} = {}' looks like a copy-paste mistake.",
                            comp.name, key_str, key_str, val_str
                        ),
                        code: crate::errcodes::ATTR_SELF_REFERENTIAL,
                    });
                    break;
                }
            }
        }
    }

    // Also check interfaces
    let ifaces = crate::definition_space().workspace_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        for attr in iface.attrs.iter() {
            let key_str = attr.id.to_string();
            if key_str.is_empty() {
                continue;
            }
            for val in &attr.values {
                let val_str = format!("{}", val).trim().to_string();
                if val_str == key_str {
                    acc.push(CheckResult {
                        check_name: "enums",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: attr.key_span.clone(),
                        message: format!(
                            "Interface '{}': attribute '{}' has a self-referential value. \
                             '{} = {}' looks like a copy-paste mistake.",
                            iface.name, key_str, key_str, val_str
                        ),
                        code: crate::errcodes::ATTR_SELF_REFERENTIAL,
                    });
                    break;
                }
            }
        }
    }
}

// ============================================================================
// Duplicate attribute keys (N6-extra)
// ============================================================================

/// Detect duplicate attribute keys within a single component or interface.
/// e.g. `manufacturer = "TI"` followed by `manufacturer = "ST"` silently
/// overwrites — the second value wins, which may not be intended.
fn check_duplicate_attr_keys(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        for attr in comp.attrs.iter() {
            let key_str = attr.id.to_string();
            if key_str.is_empty() {
                continue;
            }
            if !seen.insert(key_str.clone()) {
                acc.push(CheckResult {
                    check_name: "enums",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: attr.key_span.clone(),
                    message: format!(
                        "Component '{}': attribute '{}' is defined multiple times. \
                         Only the last value takes effect.",
                        comp.name, key_str
                    ),
                    code: crate::errcodes::RANGE_REVERSED,
                });
            }
        }
    }

    let ifaces = crate::definition_space().workspace_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        for attr in iface.attrs.iter() {
            let key_str = attr.id.to_string();
            if key_str.is_empty() {
                continue;
            }
            if !seen.insert(key_str.clone()) {
                acc.push(CheckResult {
                    check_name: "enums",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: attr.key_span.clone(),
                    message: format!(
                        "Interface '{}': attribute '{}' is defined multiple times. \
                         Only the last value takes effect.",
                        iface.name, key_str
                    ),
                    code: crate::errcodes::RANGE_REVERSED,
                });
            }
        }
    }
}
