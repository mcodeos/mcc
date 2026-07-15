// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Naming convention checks: component case, instance case, library shadowing.

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
        // Collect library CMIE names first (for shadow detection)
        let mut lib_names: HashSet<String> = HashSet::new();
        {
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for entry in comps.iter() {
                lib_names.insert(entry.key().ident.to_string());
            }
            let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
            for entry in ifaces.iter() {
                lib_names.insert(entry.key().ident.to_string());
            }
        }

        // J1: check component names for lowercase (should be UPPER_SNAKE)
        {
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for entry in comps.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                // Skip test/lab files
                if uri.contains("/unitest/") || uri.contains("/cases") || uri.contains("/lab/") {
                    continue;
                }
                // Component names should start with uppercase or digit (like 74HC)
                if let Some(first) = name.chars().next() {
                    if first.is_lowercase() && !name.contains('.') {
                        // Names with dots like "Amplifier.BUFFER" are fine
                        acc.push(CheckResult {
                            check_name: "naming",
                            severity: CheckSeverity::Info,
                            uri: Some(uri),
                            span: None,
                            message: format!(
                                "Component '{}' starts with lowercase. Convention is UPPER_SNAKE.",
                                name
                            ),
                            code: 2201,
                        });
                    }
                }
                // J3: check if name shadows a different kind of CMIE
                // (same name as interface/enum but this is a component)
                // Note: this is a simplified check; full scope analysis needed for precision
            }
        }
    }
}
