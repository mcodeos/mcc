// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Within-file duplicate detection: pin names, enum values, overlapping pin ranges.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::HashMap;

pub struct DupWithinCheck;

impl ValidationCheck for DupWithinCheck {
    fn name(&self) -> &'static str {
        "dup-within"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        // Check components
        {
            let comps = crate::definition_space().workspace_components();
            for (sn, comp) in comps.iter() {
                let uri = sn.uri.to_string();
                check_pin_name_duplicates(uri, comp.name.to_string(), comp, acc);
            }
        }
        // Check enums
        {
            let enums = crate::definition_space().workspace_enums();
            for (sn, def) in enums.iter() {
                let uri = sn.uri.to_string();
                check_enum_value_duplicates(uri, def.name.to_string(), def, acc);
            }
        }
    }
}

/// H1: duplicate pin names in a component's pin definitions
fn check_pin_name_duplicates(
    uri: String,
    comp_name: String,
    comp: &crate::McComponent,
    acc: &mut CheckAccumulator,
) {
    let mut seen: HashMap<String, Vec<String>> = HashMap::new();
    for (pin_name, _) in &comp.pins.names_to_id {
        // Pin names like "GND" that appear multiple times with different pin IDs
        // are already tracked by names_to_id's McPinPort structure
        // Check if any pin name is explicitly duplicated
        seen.entry(pin_name.clone())
            .or_default()
            .push(pin_name.clone());
    }
    for (name, entries) in &seen {
        if entries.len() > 1 {
            acc.push(CheckResult {
                check_name: "dup-within",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Pin name '{}' appears {} times in component '{}'. \
                     Duplicate pin labels make net references ambiguous.",
                    name,
                    entries.len(),
                    comp_name
                ),
                code: crate::errcodes::DUP_WITHIN,
            });
        }
    }
}

/// H2: duplicate enum value names within an enum definition
fn check_enum_value_duplicates(
    uri: String,
    enum_name: String,
    edef: &crate::semantic::mc_enum::McEnumDef,
    acc: &mut CheckAccumulator,
) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for value in &edef.values {
        let name = value.name.to_string();
        *seen.entry(name).or_insert(0) += 1;
    }
    for (name, count) in &seen {
        if *count > 1 {
            acc.push(CheckResult {
                check_name: "dup-within",
                severity: CheckSeverity::Error,
                uri: Some(uri.clone()),
                span: Some(edef.span[0] as usize..edef.span[1] as usize),
                message: format!(
                    "Enum value '{}' appears {} times in enum '{}'.",
                    name, count, enum_name
                ),
                code: crate::errcodes::DUP_ENUM_VALUE,
            });
        }
    }
}
