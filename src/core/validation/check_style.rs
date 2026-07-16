// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Style/naming checks: J1-J5, F1-F3.

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashSet;

pub struct StyleCheck;

impl ValidationCheck for StyleCheck {
    fn name(&self) -> &'static str {
        "style"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Info
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        let mut lib_names: HashSet<String> = HashSet::new();
        {
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for e in comps.iter() {
                lib_names.insert(e.key().ident.to_string());
            }
            let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
            for e in ifaces.iter() {
                lib_names.insert(e.key().ident.to_string());
            }
        }

        // J1: Lowercase component names
        // J2: UPPERCASE instance names (deferred — needs inst scan in modules)
        // J3: Identifier shadows library name
        // J4: Empty () on parameterless components
        // J5: Copy-pasted function bodies (deferred — AST comparison needed)
        // F1: Reserved name usage
        // F2: Naming convention (deferred — needs per-project config)
        // F3: Deprecated CMIE usage (deferred — needs deprecation metadata)

        check_lowercase_components(acc, &lib_names);
        check_empty_parens(acc);
    }
}

fn check_lowercase_components(acc: &mut CheckAccumulator, _lib_names: &HashSet<String>) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let comp = entry.value();
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        if let Some(first) = name.chars().next() {
            if first.is_lowercase() && !name.contains('.') {
                acc.push(CheckResult {
                    check_name: "style",
                    severity: CheckSeverity::Info,
                    uri: Some(uri),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}' starts with lowercase (convention: UPPER_SNAKE).",
                        name
                    ),
                    code: 2201,
                });
            }
        }
    }
}

fn check_empty_parens(acc: &mut CheckAccumulator) {
    // J4: components declared with () but no params
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let comp = entry.value();
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        if comp.params.is_empty() {
            acc.push(CheckResult {
                check_name: "style",
                severity: CheckSeverity::Info,
                uri: Some(uri),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has no parameters. Consider removing empty ().",
                    name
                ),
                code: 2204,
            });
        }
    }
}
