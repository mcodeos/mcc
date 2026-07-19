// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Port/instance level checks: C2-C5, D1-D3.

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashMap;

pub struct PortInstanceCheck;

impl ValidationCheck for PortInstanceCheck {
    fn name(&self) -> &'static str {
        "port-instance"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        let modules = crate::db::cmie::tables::WORKSPACE.modules.borrow();
        for entry in modules.iter() {
            let mod_name = entry.key().ident.to_string();
            let m = entry.value();
            check_duplicate_ports(&mod_name, m, acc); // C2
            check_duplicate_instances(&mod_name, m, acc); // D1
            check_undefined_net_refs(&mod_name, m, acc); // E2
            check_param_inst_overlap(&mod_name, m, acc); // param+port naming overlap
        }
    }
}

/// C2: Two ports with the same name in one module
fn check_duplicate_ports(mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for port_name in m.insts.iter_instance_names() {
        *seen.entry(port_name.clone()).or_insert(0) += 1;
    }
    for (name, count) in &seen {
        if *count > 1 {
            let span = span_for(m, name);
            acc.push(CheckResult {
                check_name: "port-instance", severity: CheckSeverity::Error,
                uri: Some(m.uri.to_string()), span,
                message: format!(
                    "Port name '{}' appears {} times in module '{}'. Duplicate port names are ambiguous.",
                    name, count, mod_name
                ),
                code: 2402,
            });
        }
    }
}

fn check_param_inst_overlap(mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator) {
    use std::collections::HashSet;
    let pn: HashSet<String> = m
        .params
        .iter()
        .filter_map(|d| d.get_primary_name())
        .collect();
    for n in m.insts.iter_instance_names() {
        if pn.contains(n) {
            let span = span_for(m, n);
            acc.push(CheckResult {
                check_name: "port-instance",
                severity: CheckSeverity::Warning,
                uri: Some(m.uri.to_string()),
                span,
                message: format!("Name '{}' in '{}' is both a param and a port.", n, mod_name),
                code: 2410,
            });
        }
    }
}

/// Try to get a span for a name in a module (port_spans first, then def_spans).
fn span_for(m: &crate::McModule, name: &str) -> Option<std::ops::Range<usize>> {
    // Try port_spans
    if let Some(spans) = m.insts.port_spans().get(name) {
        if let Some(s) = spans.first() {
            return Some(s.clone());
        }
    }
    // Try def_spans (for params)
    for (k, s) in m.params.iter_defs_with_span() {
        if k == name || k.contains(name) || name.contains(k) {
            return Some(s);
        }
    }
    None
}

/// E2: net connections referencing identifiers not declared as ports/instances.
fn check_undefined_net_refs(mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator) {
    let uri = m.uri.to_string();
    if super::is_test_file(&uri) {
        return;
    }
    for phrase in &m.lines {
        let text = format!("{}", phrase);
        for word in text.split_whitespace() {
            let clean = word.trim_matches(|c: char| {
                !c.is_alphanumeric() && c != '_' && c != '.' && c != '[' && c != ']'
            });
            if clean.is_empty() || clean.len() == 1 {
                continue;
            }
            if clean.starts_with('"') || clean.starts_with('\'') {
                continue;
            }
            // Check if it's a known port/instance
            if !m.insts.contains(clean)
                && !m
                    .insts
                    .iter_instance_names()
                    .any(|k| m.insts.all_name_forms_for(k).contains(&clean.to_string()))
                && !m.params.is_defined(clean)
            {
                acc.push(CheckResult {
                    check_name: "port-instance",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: span_for(m, clean),
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
fn check_duplicate_instances(mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator) {
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
            let span = span_for(m, name);
            acc.push(CheckResult {
                check_name: "port-instance",
                severity: CheckSeverity::Warning,
                uri: Some(m.uri.to_string()),
                span,
                message: format!(
                    "Name '{}' appears {} times in module '{}' (across ports, params, instances).",
                    name, count, mod_name
                ),
                code: 2401,
            });
        }
    }
}
