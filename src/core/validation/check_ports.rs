// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Port/instance level checks: C2-C5, D1-D3.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck};
use std::collections::HashMap;

pub struct PortInstanceCheck;

impl ValidationCheck for PortInstanceCheck {
    fn name(&self) -> &'static str { "port-instance" }
    fn phase(&self) -> CheckPhase { CheckPhase::PostParse }
    fn default_severity(&self) -> CheckSeverity { CheckSeverity::Warning }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
        for entry in modules.iter() {
            let mod_name = entry.key().ident.to_string();
            let m = entry.value();
            // C2: Duplicate port names within same module
            check_duplicate_ports(&mod_name, m, acc);
            // D1: Duplicate instance names within same module
            check_duplicate_instances(&mod_name, m, acc);
        }
    }
}

/// C2: Two ports with the same name in one module
fn check_duplicate_ports(
    mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator,
) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for port_name in m.insts.iter_instance_names() {
        *seen.entry(port_name.clone()).or_insert(0) += 1;
    }
    for (name, count) in &seen {
        if *count > 1 {
            acc.push(CheckResult {
                check_name: "port-instance", severity: CheckSeverity::Error,
                uri: None, span: None,
                message: format!(
                    "Port name '{}' appears {} times in module '{}'. Duplicate port names are ambiguous.",
                    name, count, mod_name
                ),
                code: 2402,
            });
        }
    }
}

/// D1: Two instances with the same name in one module
fn check_duplicate_instances(
    mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator,
) {
    // Module instances include both ports and component instances with overlapping namespaces
    let mut inst_names: HashMap<String, usize> = HashMap::new();
    for name in m.insts.iter_instance_names() {
        *inst_names.entry(name.clone()).or_insert(0) += 1;
    }
    // Also check params for same-named entries
    for name in m.params.names() {
        *inst_names.entry(name).or_insert(0) += 1;
    }
    for (name, count) in &inst_names {
        if *count > 1 {
            acc.push(CheckResult {
                check_name: "port-instance", severity: CheckSeverity::Warning,
                uri: None, span: None,
                message: format!(
                    "Name '{}' appears {} times in module '{}' (across ports, params, instances).",
                    name, count, mod_name
                ),
                code: 2401,
            });
        }
    }
}
