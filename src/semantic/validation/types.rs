// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Type and expression validation checks.
//!
//! Checks:
//!   E1 — Type mismatch in param binding (arg value vs declared param type)
//!   E3 — Unit dimension mismatch (wrong physical unit in argument)

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_param_type_mismatch(acc); // E1 + E3
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
            let (class_name, args): (String, &[crate::McParamValue]) = match instance {
                crate::McInstance::Component(c2) => {
                    (c2.base.name.to_string(), c2.params.as_slice())
                }
                _ => continue,
            };

            if let Some(param_types) = comp_param_types.get(&class_name) {
                for (orig_idx, _pname, unit_type) in param_types.iter() {
                    if unit_type.is_empty() {
                        continue;
                    }
                    if let Some(arg) = args.get(*orig_idx) {
                        let arg_display = arg.to_string();
                        let arg_clean = arg_display.trim();
                        if arg_clean.is_empty() || arg_clean == "_" {
                            continue; // placeholder — not an error
                        }

                        let mismatch = check_param_arg_compat(unit_type, arg);
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
                                code: crate::errcodes::TYPE_INCOMPATIBLE,
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
        McParamTypeKind::BasicInt { .. } => "Int".to_string(),
        McParamTypeKind::BasicHex { .. } => "Hex".to_string(),
        McParamTypeKind::BasicFloat { .. } => "Float".to_string(),
        McParamTypeKind::BasicString { .. } => "String".to_string(),
        _ => String::new(),
    }
}

/// Validate against the parsed argument variant instead of its display text.
fn check_param_arg_compat(unit_type: &str, arg: &crate::McParamValue) -> Option<String> {
    use crate::McParamValue;

    let display = arg.to_string();
    match unit_type {
        "String" => match arg {
            McParamValue::String(_) => None,
            McParamValue::Int(_) | McParamValue::Hex(_) | McParamValue::Float(_) => Some(format!(
                "'{}' is numeric, but the parameter expects a String.",
                display
            )),
            _ => None,
        },
        "Int" => match arg {
            McParamValue::String(_) => Some(format!(
                "'{}' is a string, but the parameter expects an Int.",
                display
            )),
            _ => None,
        },
        "Float" => match arg {
            McParamValue::String(_) => Some(format!(
                "'{}' is a string, but the parameter expects a Float.",
                display
            )),
            _ => None,
        },
        "Hex" => match arg {
            McParamValue::String(_) => Some(format!(
                "'{}' is a string, but the parameter expects a Hex value.",
                display
            )),
            _ => None,
        },
        expected_unit => match arg {
            McParamValue::UValue(value) => {
                let actual_unit = format!("{:?}", value.unit());
                (actual_unit != expected_unit).then(|| {
                    format!(
                        "'{}' has unit {}, but the parameter expects {}.",
                        display, actual_unit, expected_unit
                    )
                })
            }
            McParamValue::String(_) => Some(format!(
                "'{}' is a string, but the parameter expects {}.",
                display, expected_unit
            )),
            _ => None,
        },
    }
}
