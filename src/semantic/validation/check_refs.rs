// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Reference integrity checks: I1-I4.

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};

pub struct RefIntegrityCheck;

impl ValidationCheck for RefIntegrityCheck {
    fn name(&self) -> &'static str {
        "ref-integrity"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        check_bare_params(acc); // I2
        check_spec_refs(acc); // I1
        check_comp_func_unused_params(acc); // B1 for component funcs
        check_label_refs(acc); // I3: label refs not found
    }
}

/// B1 extension: unused parameters in component functions.
fn check_comp_func_unused_params(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        let comp_name = entry.key().ident.to_string();
        for func in comp.funcs.iter() {
            if !func.params.is_empty() && func.lines.is_empty() && func.insts.is_empty() {
                let param_names = func.params.names().join(", ");
                acc.push(CheckResult {
                    check_name: "ref-integrity",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Function '{}' in component '{}' has unused params: [{}].",
                        func.name, comp_name, param_names
                    ),
                    code: 2303,
                });
            }
        }
    }
}

/// I2: flag component parameters declared without `::TYPE` annotation.
fn check_bare_params(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let comp_name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for declare in comp.params.iter() {
            if !declare.has_type_constraint() && declare.get_primary_name().is_some() {
                if let Some(name) = declare.get_primary_name() {
                    // Skip role params — they're intentionally untyped keywords
                    if name == "role" {
                        continue;
                    }
                    acc.push(CheckResult {
                        check_name: "ref-integrity",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
                        message: format!(
                            "Parameter '{}' in component '{}' has no type annotation. \
                             Consider adding ::INT, ::STRING, ::UV.VOLT, etc.",
                            name, comp_name
                        ),
                        code: 2302,
                    });
                }
            }
        }
    }
}

/// I3: label references whose target definition cannot be found in any scope.
///
/// Iterates module net phrases and flags label/port names that appear in
/// connection expressions but don't match any known instance or parameter.
fn check_label_refs(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let mod_name = entry.key().ident.to_string();

        // Collect all known names in this module's scope
        let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
        for name in m.insts.iter_instance_names() {
            known.insert(name.to_string());
        }
        for name in m.params.names() {
            known.insert(name);
        }

        // Check port refs: each ref should point to a known name
        for (_span, port_name, _scope) in m.insts.iter_port_refs() {
            if port_name.starts_with('@') {
                continue; // Anonymous instances are self-defining
            }
            let ids = crate::semantic::basic::mc_ids::McIds::from(port_name.as_str());
            let candidates = ids.expand();
            let found = if candidates.is_empty() {
                known.contains(port_name)
            } else {
                candidates.iter().any(|c| known.contains(c))
            };
            if !found {
                acc.push(CheckResult {
                    check_name: "ref-integrity",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(m.span.start..m.span.end),
                    message: format!(
                        "Reference '{}' in module '{}' may not be defined in any visible scope.",
                        port_name, mod_name
                    ),
                    code: 2310,
                });
            }
        }
    }
}

/// I1: references in spec/attr blocks to undeclared variables.
fn check_spec_refs(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let comp_name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        let param_names: std::collections::HashSet<String> = comp
            .params
            .iter()
            .filter_map(|d| d.get_primary_name())
            .collect();
        for attr in comp.attrs.iter() {
            let key = attr.id.to_string();
            if key.starts_with("spec.") {
                for val in &attr.values {
                    let vs = format!("{}", val);
                    // Check if the value is a bare identifier matching a param name
                    let word = vs.trim();
                    if !word.is_empty()
                        && !word.starts_with('"')
                        && !word.starts_with('\'')
                        && !word.chars().any(|c| c.is_ascii_digit() || c == '(')
                        && !param_names.contains(word)
                    {
                        acc.push(CheckResult {
                            check_name: "ref-integrity", severity: CheckSeverity::Error,
                            uri: Some(uri.clone()), span: attr.key_span.clone(),
                            message: format!(
                                "Spec key '{}' in component '{}' references '{}' which is not a declared parameter.",
                                key, comp_name, word
                            ),
                            code: 2301,
                        });
                    }
                }
            }
        }
    }
}
