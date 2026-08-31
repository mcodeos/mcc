// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Style/naming checks: J1-J5, F1-F3.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        let mut lib_names: HashSet<String> = HashSet::new();
        {
            let comps = crate::definition_space().workspace_components();
            for (sn, _) in comps.iter() {
                lib_names.insert(sn.ident.to_string());
            }
            let ifaces = crate::definition_space().workspace_interfaces();
            for (sn, _) in ifaces.iter() {
                lib_names.insert(sn.ident.to_string());
            }
        }

        // J1: Lowercase component names
        // J2: UPPERCASE instance names (deferred — needs inst scan in modules)
        // J3: Identifier shadows library name
        // J4: Empty () on parameterless components (deferred until source syntax is retained)
        // J5: Copy-pasted function bodies — dropped: component funcs describe
        //     net connections, not refactorable logic; identical bodies are a
        //     legitimate shared-connection pattern (e.g. RFReceiver/RFSender).
        // F1: Reserved name usage
        // F2: Naming convention — implemented in extra.rs as check_naming_convention
        // F3: Deprecated CMIE usage (deferred — needs deprecation metadata)

        check_lowercase_components(acc, &lib_names);
    }
}

fn check_lowercase_components(acc: &mut CheckAccumulator, _lib_names: &HashSet<String>) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let name = sn.ident.to_string();
        let uri = sn.uri.to_string();
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
                    code: crate::errcodes::NAME_COMPONENT_LOWERCASE,
                });
            }
        }
    }
}
