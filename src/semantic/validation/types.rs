// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Type and expression validation checks.
//!
//! Checks:
//!   Q7 — Closure with undeclared free variable
//!   E1 — Type mismatch in param binding (arg value vs declared param type)
//!   E3 — Unit dimension mismatch (wrong physical unit in argument)

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashSet;

pub struct TypesCheck;

impl ValidationCheck for TypesCheck {
    fn name(&self) -> &'static str {
        "types"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        check_closure_free_vars(acc); // Q7
        check_param_type_mismatch(acc); // E1 + E3
    }
}

// ============================================================================
// Q7: Closure with undeclared free variable
// ============================================================================

/// Scan module body text for closure expressions (`|x, y| { ... }`) and
/// verify that all identifiers used inside the closure body are either
/// declared as closure parameters or are known module-level names.
fn check_closure_free_vars(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let mod_span = Some(m.span.start..m.span.end);

        // Collect known module-level names (instances, params)
        let known_names: HashSet<String> = {
            let mut s: HashSet<String> = m.insts.iter_instance_names().cloned().collect();
            for d in m.params.iter() {
                if let Some(n) = d.get_primary_name() {
                    s.insert(n);
                }
            }
            s
        };

        for phrase in &m.lines {
            let text = format!("{}", phrase);
            check_closure_in_text(
                &text,
                &uri,
                &entry.key().ident.to_string(),
                &known_names,
                &mod_span,
                acc,
            );
        }

        // Also check function body lines
        for func in m.funcs.iter() {
            for phrase in &func.lines {
                let text = format!("{}", phrase);
                let mut func_known = known_names.clone();
                for d in func.params.iter() {
                    if let Some(n) = d.get_primary_name() {
                        func_known.insert(n);
                    }
                }
                check_closure_in_text(
                    &text,
                    &uri,
                    &format!("{}::{}", entry.key().ident, func.name),
                    &func_known,
                    &mod_span,
                    acc,
                );
            }
        }
    }
}

/// Scan a single text line for `|params| {body}` closure patterns.
fn check_closure_in_text(
    text: &str,
    uri: &str,
    context: &str,
    known_names: &HashSet<String>,
    module_span: &Option<std::ops::Range<usize>>,
    acc: &mut CheckAccumulator,
) {
    // Find closure patterns: |param1, param2| { ... } or |param1, param2| -> ...
    let mut search_from = 0usize;
    while let Some(pipe_pos) = text[search_from..].find('|') {
        let abs_pipe = search_from + pipe_pos;
        let after_first_pipe = &text[abs_pipe + 1..];

        // Find matching closing pipe
        if let Some(close_pipe) = after_first_pipe.find('|') {
            let params_str = &after_first_pipe[..close_pipe].trim();
            let after_close = &after_first_pipe[close_pipe + 1..];

            // Parse closure params
            let closure_params: HashSet<String> = params_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && *s != "_")
                .collect();

            if closure_params.is_empty() {
                search_from = abs_pipe + 1;
                continue;
            }

            // Find closure body: either `{ ... }` or until end of phrase
            let body_start = after_close.find('{').map(|p| p + 1);
            let body_end =
                body_start.and_then(|start| after_close[start..].find('}').map(|e| start + e));

            if let (Some(start), Some(end)) = (body_start, body_end) {
                let body_text = &after_close[start..end];

                // Extract identifiers from body text
                let body_idents: HashSet<String> = body_text
                    .split_whitespace()
                    .map(|w| {
                        w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                            .to_string()
                    })
                    .filter(|s| {
                        !s.is_empty()
                            && s.chars()
                                .next()
                                .map_or(false, |c| c.is_alphabetic() || c == '_')
                            && *s != "this"
                            && *s != "pins"
                            && *s != "return"
                            && *s != "_"
                    })
                    .collect();

                // Find free variables: used in body but not in closure params
                let free_vars: Vec<String> = body_idents
                    .iter()
                    .filter(|id| {
                        !closure_params.contains(*id) && !known_names.contains(*id) && id.len() > 1
                    })
                    .cloned()
                    .collect();

                if !free_vars.is_empty() {
                    acc.push(CheckResult {
                        check_name: "types",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.to_string()),
                        span: module_span.clone(),
                        message: format!(
                            "In {}: closure |{}| uses undeclared free variable(s): {}. \
                             These names are not closure params or known module identifiers.",
                            context,
                            params_str,
                            free_vars.join(", ")
                        ),
                        code: 3401,
                    });
                }
            }

            search_from = abs_pipe + close_pipe + 2;
        } else {
            break;
        }
    }
}

// ============================================================================
// E1 + E3: Type mismatch / Unit dimension mismatch in param binding
// ============================================================================

/// For each module instance of a typed component, check whether the
/// positional arguments are compatible with the declared parameter types.
///
/// Heuristic approach:
///   - If param is `::UV.CAP` (capacitance), arg should look like `10uF`, `100nF`, etc.
///   - If param is `::UV.VOLT` (voltage), arg should look like `5V`, `3.3V`, etc.
///   - If param is `::UV.OHM` (resistance), arg should look like `10kΩ`, `100Ω`, etc.
///   - If param is `::INT`, arg should be a number, not a string
///   - If param is `::STRING`, arg should be quoted, not a bare number
fn check_param_type_mismatch(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    let comps = &crate::db::cmie::tables::WORKSPACE.components;

    // Build: component name → Vec<(param_index, param_name, unit_type)>
    let comp_param_types: std::collections::HashMap<String, Vec<(usize, String, String)>> = {
        let mut m = std::collections::HashMap::new();
        for entry in comps.iter() {
            let name = entry.key().ident.to_string();
            let comp = entry.value();
            let types: Vec<(usize, String, String)> = comp
                .params
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let pname = d.get_primary_name().unwrap_or_default();
                    let unit_str = param_type_to_unit_str(&d.param_type.kind);
                    (i, pname, unit_str)
                })
                .collect();
            if !types.is_empty() {
                m.insert(name, types);
            }
        }
        m
    };

    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        for (_inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
            let (class_name, args): (String, Vec<String>) = match instance {
                crate::McInstance::Component(c2) => (
                    c2.name.to_string(),
                    c2.params.iter().map(|p| p.to_string()).collect(),
                ),
                _ => continue,
            };

            if let Some(param_types) = comp_param_types.get(&class_name) {
                for (orig_idx, _pname, unit_type) in param_types.iter() {
                    if unit_type.is_empty() {
                        continue;
                    }
                    if let Some(arg) = args.get(*orig_idx) {
                        let arg_clean = arg.trim();
                        if arg_clean.is_empty() || arg_clean == "_" {
                            continue; // placeholder — not an error
                        }

                        // Check compatibility
                        let mismatch = check_unit_arg_compat(unit_type, arg_clean);
                        if let Some(detail) = mismatch {
                            acc.push(CheckResult {
                                check_name: "types",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: Some(m.span.start..m.span.end),
                                message: format!(
                                    "Module '{}': instance of '{}' passes '{}' for param #{} \
                                     (expected {}). {}",
                                    entry.key().ident,
                                    class_name,
                                    arg_clean,
                                    orig_idx + 1,
                                    unit_type,
                                    detail
                                ),
                                code: 3402,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Convert a McParamTypeKind to a human-readable unit type string.
/// Returns empty string for untyped params.
fn param_type_to_unit_str(kind: &crate::semantic::basic::mc_param_type::McParamTypeKind) -> String {
    use crate::semantic::basic::mc_param_type::McParamTypeKind;
    match kind {
        McParamTypeKind::UnitValue { unit } | McParamTypeKind::UnitValueDefault { unit, .. } => {
            format!("{:?}", unit)
        }
        McParamTypeKind::BasicInt { .. } | McParamTypeKind::BasicHex { .. } => "Int".to_string(),
        McParamTypeKind::BasicFloat { .. } => "Float".to_string(),
        McParamTypeKind::BasicString { .. } => "String".to_string(),
        _ => String::new(),
    }
}

/// Heuristic check: does the argument value look compatible with the expected unit type?
/// Returns Some(detail) if incompatible, None if OK or uncertain.
fn check_unit_arg_compat(unit_type: &str, arg: &str) -> Option<String> {
    // If arg is quoted, it's a string
    let is_quoted = arg.starts_with('"') || arg.starts_with('\'');
    // If arg looks numeric (with optional unit suffix)
    let has_unit_suffix = arg
        .chars()
        .any(|c| c.is_alphabetic() && c != 'e' && c != 'E');
    let is_numeric = arg.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == '-');

    match unit_type {
        "String" => {
            if !is_quoted && is_numeric {
                return Some(format!(
                    "'{}' looks like a number, but the parameter expects a String. \
                     Consider quoting: \"{}\"",
                    arg, arg
                ));
            }
        }
        "Int" | "Float" | "Hex" => {
            if is_quoted {
                return Some(format!(
                    "'{}' is a quoted string, but the parameter expects {}.",
                    arg, unit_type
                ));
            }
            if !is_numeric && !arg.starts_with("UV.") {
                return Some(format!(
                    "'{}' does not look like a numeric value for an {} parameter.",
                    arg, unit_type
                ));
            }
        }
        "Cap" => {
            if is_quoted {
                return Some(format!(
                    "'{}' is a string, but the parameter expects a capacitance (e.g., 10uF, 100nF).",
                    arg
                ));
            }
            if is_numeric && has_unit_suffix {
                let suffix = arg
                    .chars()
                    .filter(|c| c.is_alphabetic() && *c != 'e' && *c != 'E')
                    .collect::<String>()
                    .to_lowercase();
                // Common capacitance suffixes
                if !suffix.ends_with('f') && !suffix.starts_with('f') {
                    return Some(format!(
                        "'{}' has suffix '{}' which doesn't look like a capacitance unit. \
                         Expected e.g., 10uF, 100nF, 1pF.",
                        arg, suffix
                    ));
                }
            }
        }
        "Volt" => {
            if is_quoted {
                return Some(format!(
                    "'{}' is a string, but the parameter expects a voltage (e.g., 5V, 3.3V).",
                    arg
                ));
            }
            if is_numeric && has_unit_suffix {
                let suffix = arg
                    .chars()
                    .filter(|c| c.is_alphabetic() && *c != 'e' && *c != 'E')
                    .collect::<String>()
                    .to_lowercase();
                if suffix == "uf" || suffix == "nf" || suffix == "pf" || suffix.ends_with("f") {
                    return Some(format!(
                        "'{}' has capacitance suffix but parameter expects voltage (Volt).",
                        arg
                    ));
                }
            }
        }
        "Ohm" | "Res" => {
            if is_quoted {
                return Some(format!(
                    "'{}' is a string, but the parameter expects a resistance (e.g., 10k, 100R).",
                    arg
                ));
            }
            if is_numeric && has_unit_suffix {
                let suffix = arg
                    .chars()
                    .filter(|c| c.is_alphabetic() && *c != 'e' && *c != 'E')
                    .collect::<String>()
                    .to_lowercase();
                if suffix == "v" || suffix == "a" || suffix == "uf" || suffix == "nf" {
                    return Some(format!(
                        "'{}' has non-resistance suffix. Parameter expects resistance (Ohm).",
                        arg
                    ));
                }
            }
        }
        "Amp" => {
            if is_quoted {
                return Some(format!(
                    "'{}' is a string, but the parameter expects a current (e.g., 1A, 500mA).",
                    arg
                ));
            }
            if is_numeric && has_unit_suffix {
                let suffix = arg
                    .chars()
                    .filter(|c| c.is_alphabetic() && *c != 'e' && *c != 'E')
                    .collect::<String>()
                    .to_lowercase();
                if suffix == "v" || suffix == "uf" || suffix == "nf" {
                    return Some(format!(
                        "'{}' looks like a voltage/capacitance but parameter expects current (Amp).",
                        arg
                    ));
                }
            }
        }
        "Wat" => {
            if is_quoted {
                return Some(format!(
                    "'{}' is a string, but the parameter expects power (e.g., 1W, 500mW).",
                    arg
                ));
            }
            if is_numeric && has_unit_suffix {
                let suffix = arg
                    .chars()
                    .filter(|c| c.is_alphabetic() && *c != 'e' && *c != 'E')
                    .collect::<String>()
                    .to_lowercase();
                if suffix == "v" || suffix == "a" {
                    return Some(format!(
                        "'{}' looks like voltage/current but parameter expects power (Wat).",
                        arg
                    ));
                }
            }
        }
        "Hz" => {
            if is_quoted {
                return Some(format!(
                    "'{}' is a string, but the parameter expects frequency (e.g., 10MHz, 1kHz).",
                    arg
                ));
            }
            if is_numeric && has_unit_suffix {
                let suffix = arg
                    .chars()
                    .filter(|c| c.is_alphabetic() && *c != 'e' && *c != 'E')
                    .collect::<String>()
                    .to_lowercase();
                if suffix == "v" || suffix == "uf" || suffix == "a" {
                    return Some(format!(
                        "'{}' looks like a non-frequency value but parameter expects Hz.",
                        arg
                    ));
                }
            }
        }
        _ => {} // Unknown or untyped — can't check
    }

    None
}
