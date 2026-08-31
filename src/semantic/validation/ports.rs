// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Port/instance level checks: C2-C5, D1-D3.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::{HashMap, HashSet};

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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        let modules = crate::definition_space().workspace_modules();
        for (sn, module) in modules.iter() {
            let mod_name = sn.ident.to_string();
            check_duplicate_ports(&mod_name, module, acc); // C2
            check_duplicate_instances(&mod_name, module, acc); // D1
            check_param_inst_overlap(&mod_name, module, acc); // value-param + instance overlap
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
                code: crate::errcodes::PORT_DUPLICATE_NAME,
            });
        }
    }
}

fn check_param_inst_overlap(mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator) {
    let pn: HashSet<String> = m
        .params
        .iter()
        .filter(|d| !d.is_port())
        .filter_map(|d| d.get_primary_name())
        .collect();
    for (n, (_, inst)) in m.insts.iter_with_iotype() {
        if !matches!(
            inst,
            crate::McInstance::Component(_)
                | crate::McInstance::Module(_)
                | crate::McInstance::Interface(_)
                | crate::McInstance::Unresolved { .. }
        ) {
            continue;
        }
        if pn.contains(n) {
            let span = span_for(m, n);
            acc.push(CheckResult {
                check_name: "port-instance",
                severity: CheckSeverity::Warning,
                uri: Some(m.uri.to_string()),
                span,
                message: format!(
                    "Name '{}' in '{}' is both a value parameter and an instance.",
                    n, mod_name
                ),
                code: crate::errcodes::NAME_PARAM_AND_INSTANCE,
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

/// D1: Two instances with the same name in one module
fn check_duplicate_instances(mod_name: &str, m: &crate::McModule, acc: &mut CheckAccumulator) {
    for (name, (_, inst)) in m.insts.iter_with_iotype() {
        if !matches!(
            inst,
            crate::McInstance::Component(_)
                | crate::McInstance::Module(_)
                | crate::McInstance::Interface(_)
                | crate::McInstance::Unresolved { .. }
        ) {
            continue;
        }
        let count = m.insts.port_spans().get(name).map_or(0, std::vec::Vec::len);
        if count > 1 {
            let span = span_for(m, name);
            acc.push(CheckResult {
                check_name: "port-instance",
                severity: CheckSeverity::Warning,
                uri: Some(m.uri.to_string()),
                span,
                message: format!(
                    "Instance '{}' is declared {} times in module '{}'.",
                    name, count, mod_name
                ),
                code: crate::errcodes::INST_DECLARED_MULTIPLE,
            });
        }
    }
}
