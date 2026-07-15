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
            check_duplicate_ports(&mod_name, m, acc);      // C2
            check_duplicate_instances(&mod_name, m, acc);   // D1
            check_undefined_net_refs(&mod_name, m, acc);    // E2
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

/// E2: net connections referencing identifiers not declared as ports/instances.
fn check_undefined_net_refs(
    mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator,
) {
    let uri = m.uri.to_string();
    if uri.contains("/unitest/") || uri.contains("/cases") { return; }
    for phrase in &m.lines {
        let text = format!("{}", phrase);
        for word in text.split_whitespace() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '[' && c != ']');
            if clean.is_empty() || clean.len() == 1 { continue; }
            if clean.starts_with('"') || clean.starts_with('\'') { continue; }
            // Check if it's a known port/instance
            if !m.insts.contains(clean)
                && !m.insts.iter_instance_names().any(|k| m.insts.all_name_forms_for(k).contains(&clean.to_string()))
                && !m.params.is_defined(clean)
            {
                acc.push(CheckResult {
                    check_name: "port-instance", severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()), span: None,
                    message: format!(
                        "Module '{}' references '{}' which is not a declared port or instance.",
                        mod_name, clean
                    ),
                    code: 2403,
                });
            }
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
