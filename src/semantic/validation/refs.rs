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
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp_name = sn.ident.to_string();
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
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let comp_name = sn.ident.to_string();
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
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

/// I1: references in spec blocks to undeclared variables.
///
/// The two spellings of a spec key are one fact (G2): the dotted key
/// `spec.X = v` and the table row `spec = [ X = v ]`. Both must be checked by
/// the same rule, so the table form recurses into its rows instead of passing in
/// silence.
///
/// Not every bare word is a param reference: `_` (unassigned), a boolean, and a
/// declared enum value (`safety_class = SC_NONE`) are values in their own right,
/// and the spec tables in the system library are written with them.
fn check_spec_refs(acc: &mut CheckAccumulator) {
    let ds = crate::definition_space();
    let enum_values: std::collections::HashSet<String> = ds
        .all_enums()
        .iter()
        .flat_map(|(_, e)| e.values.iter().map(|v| v.name.to_string()))
        .collect();
    let comps = ds.workspace_components();
    for (sn, comp) in comps.iter() {
        let comp_name = sn.ident.to_string();
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let param_names: std::collections::HashSet<String> = comp
            .params
            .iter()
            .filter_map(|d| d.get_primary_name())
            .collect();
        for attr in comp.attrs.iter() {
            // Structured segments decide whether the attr sits on the spec face;
            // the name is compared exactly.
            let segs = &attr.id.segments;
            let spec_key = crate::semantic::basic::attr_keys::SPEC_TABLE_KEY;
            let is_table_spec = segs.len() == 1 && attr.id.to_string() == spec_key;
            let is_dotted_spec = segs.len() > 1 && segs[0].to_string() == spec_key;
            if is_table_spec {
                for val in &attr.values {
                    if let crate::semantic::component::mc_attr::McAttrVal::Attributes(rows) = val {
                        check_spec_rows(
                            rows,
                            "spec",
                            &param_names,
                            &enum_values,
                            &comp_name,
                            &uri,
                            acc,
                        );
                    }
                }
            } else if is_dotted_spec {
                let key = attr.id.to_string();
                check_spec_value_refs(
                    &key,
                    attr,
                    &param_names,
                    &enum_values,
                    &comp_name,
                    &uri,
                    acc,
                );
                for val in &attr.values {
                    if let crate::semantic::component::mc_attr::McAttrVal::Attributes(rows) = val {
                        check_spec_rows(
                            rows,
                            &key,
                            &param_names,
                            &enum_values,
                            &comp_name,
                            &uri,
                            acc,
                        );
                    }
                }
            }
        }
    }
}

/// I1 helper: check the rows of one spec table. `path` is the dotted key of the
/// table itself, so a row's own key is `path.<row id>` — the same name the
/// dotted spelling of that row would carry.
fn check_spec_rows(
    rows: &[crate::semantic::component::mc_attr::McAttribute],
    path: &str,
    param_names: &std::collections::HashSet<String>,
    enum_values: &std::collections::HashSet<String>,
    comp_name: &str,
    uri: &str,
    acc: &mut CheckAccumulator,
) {
    for row in rows {
        let key = format!("{path}.{}", row.id);
        check_spec_value_refs(&key, row, param_names, enum_values, comp_name, uri, acc);
        for val in &row.values {
            if let crate::semantic::component::mc_attr::McAttrVal::Attributes(inner) = val {
                check_spec_rows(inner, &key, param_names, enum_values, comp_name, uri, acc);
            }
        }
    }
}

/// I1 helper: flag bare-identifier values of one spec key that name no declared
/// parameter. Uses the parsed `McAttrVal` type rather than a string heuristic.
///
/// A word that stands on its own as a value is not a dangling reference: the
/// placeholder `_`, the boolean literals (lexed as bare variables), and any
/// declared enum value. Only a word that is neither those nor a declared
/// parameter is reported.
fn check_spec_value_refs(
    key: &str,
    attr: &crate::semantic::component::mc_attr::McAttribute,
    param_names: &std::collections::HashSet<String>,
    enum_values: &std::collections::HashSet<String>,
    comp_name: &str,
    uri: &str,
    acc: &mut CheckAccumulator,
) {
    for val in &attr.values {
        if let crate::semantic::component::mc_attr::McAttrVal::AttrVariable(opd, _) = val {
            let word = opd.to_string();
            if param_names.contains(&word) || is_standalone_spec_value(&word, enum_values) {
                continue;
            }
            acc.push(CheckResult {
                check_name: "ref-integrity",
                severity: CheckSeverity::Error,
                uri: Some(uri.to_string()),
                span: attr.key_span.clone(),
                message: format!(
                    "Spec key '{}' in component '{}' references '{}' which is not a declared parameter.",
                    key, comp_name, word
                ),
                code: crate::errcodes::SPEC_KEY_UNDECLARED_PARAM,
            });
        }
    }
}

/// A bare word that is a spec value in its own right, not a parameter reference:
/// the placeholder `_` (unassigned), the boolean literals, or a declared enum
/// value (`safety_class = SC_NONE`).
fn is_standalone_spec_value(word: &str, enum_values: &std::collections::HashSet<String>) -> bool {
    matches!(word, "_" | "true" | "false") || enum_values.contains(word)
}
