// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Reference integrity checks: I1-I4.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::basic::mc_param_type::McParamTypeKind;

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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_bare_params(acc); // I2
        check_spec_refs(acc); // I1
        check_comp_func_unused_params(acc); // B1 for component funcs
    }
}

/// B1: component functions that declare parameters but have an empty body
/// (no stmts, no instances). The function signature exists but no implementation
/// is provided — likely incomplete or stub code.
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
            if !func.params.is_empty() && func.stmts.is_empty() && func.insts.is_empty() {
                let param_names = func.params.names().join(", ");
                let func_span = func.span.clone().unwrap_or(comp.span.start..comp.span.end);
                acc.push(CheckResult {
                    check_name: "ref-integrity",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(func_span),
                    message: format!(
                        "Function '{}' in component '{}' has params [{}] but no body (empty implementation).",
                        func.name, comp_name, param_names
                    ),
                    code: crate::errcodes::FUNC_PARAMS_NO_BODY,
                });
            }
        }
    }
}

/// I2: flag component parameters whose type could not be determined.
/// Smart Param inference may resolve bare identifiers to Label/Idx/etc.;
/// only warn when the kind remains Unknown after inference.
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
            if declare.param_type.kind == McParamTypeKind::Unknown
                && declare.get_primary_name().is_some()
            {
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
                            "Parameter '{}' in component '{}' has no type annotation and its type could not be inferred. \
                             Consider adding ::INT, ::STRING, ::UV.VOLT, etc.",
                            name, comp_name
                        ),
                        code: crate::errcodes::REF_INTEGRITY,
                    });
                }
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
            // Check if attr.id starts with "spec." using structured segments
            let is_spec = attr.id.segments.len() > 1 && attr.id.segments[0].to_string() == "spec";
            if is_spec {
                for val in &attr.values {
                    // Use the parsed McAttrVal type instead of string heuristic.
                    // AttrVariable is a bare identifier — check if it matches a known param.
                    if let crate::semantic::component::mc_attr::McAttrVal::AttrVariable(opd, _) =
                        val
                    {
                        let word = opd.to_string();
                        if !param_names.contains(&word) {
                            acc.push(CheckResult {
                                check_name: "ref-integrity", severity: CheckSeverity::Error,
                                uri: Some(uri.clone()), span: attr.key_span.clone(),
                                message: format!(
                                    "Spec key '{}' in component '{}' references '{}' which is not a declared parameter.",
                                    attr.id, comp_name, word
                                ),
                                code: crate::errcodes::SPEC_KEY_UNDECLARED_PARAM,
                            });
                        }
                    }
                }
            }
        }
    }
}
