// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Usage-Based Parameter Type Inference
//!
//! Infers the semantic type of untyped (bare identifier) parameters by
//! analyzing how they are used in the definition body and at call sites.
//!
//! Core principle: NO name-based heuristics. Only usage analysis.

use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::core::basic::mc_param_type::{McParamType, McParamTypeKind};
use crate::core::basic::mc_paramd::McParamDeclare;
use crate::core::basic::mc_uval::McUnit;

// ============================================================================
// Usage Site
// ============================================================================

/// A single usage site of a parameter in a definition body.
#[derive(Debug, Clone)]
pub struct UsageSite {
    /// What kind of usage this is
    pub kind: UsageKind,
    /// Span location for diagnostics
    pub pos: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageKind {
    /// `P -> net`, `net -> P`, `P - net`, `P + net`, `net - P`, `net + P`
    NetConnection,
    /// `pins = [ N = P ]`
    PinBinding,
    /// `spec.X = P` or `X = P` in attribute context
    AttrValue(String), // the attribute name (e.g., "resistance", "voltage")
    /// `P` appears in an arithmetic expression: `P * 2`, `P + 1`, etc.
    ArithmeticExpr,
    /// `P` passed as argument to a function call: `fcall(P)`
    FcallArg,
    /// `P` passed as argument to a constructor: `ClassName(P)`
    CtorArg,
    /// `P` appears in a return statement
    ReturnValue,
    /// `P` used in conditional: `if (P) { ... }`
    Conditional,
    /// `P = literal` assignment
    Assignment(String), // the RHS literal
    /// `P` referenced as a member: `P.member`
    MemberAccess,
}

// ============================================================================
// Inference Engine
// ============================================================================

/// Result of usage-based type inference for a single parameter.
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub param_name: String,
    pub param_type: McParamType,
    /// 0.0 = no confidence (mixed/unused), 1.0 = certain
    pub confidence: f32,
    /// Number of usage sites found
    pub usage_count: usize,
}

/// Collect all usage sites for a parameter name within a body AST subtree.
pub fn collect_usages(param_name: &str, body: &AstNode) -> Vec<UsageSite> {
    let mut usages = Vec::new();
    collect_usages_recursive(param_name, body, &mut usages);
    usages
}

fn collect_usages_recursive(param_name: &str, node: &AstNode, usages: &mut Vec<UsageSite>) {
    // Walk children
    if let Some(child) = node.get_sub_node() {
        for n in child.iter() {
            let ntype = n.get_type();
            let pos = n.get_pos() as usize;

            match ntype {
                // Net expressions
                MCAST_OPD_MINUS | MCAST_OPD_PLUS | MCAST_OPD_RIGHTARROW | MCAST_OPD_LEFTARROW => {
                    if node_contains_name(&n, param_name) {
                        usages.push(UsageSite {
                            kind: UsageKind::NetConnection,
                            pos,
                        });
                    }
                }
                // Attribute: key = value
                MCAST_ATTRIBUTE => {
                    if node_contains_name(&n, param_name) {
                        let attr_name = extract_attr_name(&n);
                        if is_spec_attr(&attr_name) {
                            usages.push(UsageSite {
                                kind: UsageKind::AttrValue(extract_spec_key(&attr_name)),
                                pos,
                            });
                        } else {
                            usages.push(UsageSite {
                                kind: UsageKind::AttrValue(attr_name),
                                pos,
                            });
                        }
                    }
                }
                MCAST_ATTRIBUTE_PIN => {
                    if node_contains_name(&n, param_name) {
                        usages.push(UsageSite {
                            kind: UsageKind::PinBinding,
                            pos,
                        });
                    }
                }
                MCAST_OPD_FCALL => {
                    if node_contains_name(&n, param_name) {
                        usages.push(UsageSite {
                            kind: UsageKind::FcallArg,
                            pos,
                        });
                    }
                }
                // Arithmetic
                MCAST_OPD_MULTI | MCAST_OPD_DIVID => {
                    if node_contains_name(&n, param_name) {
                        usages.push(UsageSite {
                            kind: UsageKind::ArithmeticExpr,
                            pos,
                        });
                    }
                }
                _ => {
                    // Recurse into sub-nodes for other types
                    collect_usages_recursive(param_name, &n, usages);
                }
            }
        }
    }
}

/// Check if an AST subtree contains a reference to the given parameter name.
fn node_contains_name(node: &AstNode, name: &str) -> bool {
    // Check this node itself via text extraction (works for MCAST_ID, MCAST_IDS, etc.)
    if let Some(text) = node.to_string() {
        if text == name {
            return true;
        }
    }
    // Recurse into children for composite nodes (MCAST_ATTRIBUTE, MCAST_NET, etc.)
    if let Some(child) = node.get_sub_node() {
        for n in child.iter() {
            if node_contains_name(&n, name) {
                return true;
            }
        }
    }
    false
}

/// Extract attribute name from MCAST_ATTRIBUTE node (e.g., "spec.resistance")
fn extract_attr_name(node: &AstNode) -> String {
    if let Some(child) = node.get_sub_node() {
        for n in child.iter() {
            if n.get_type() == MCAST_ID || n.get_type() == MCAST_IDS || n.get_type() == MCAST_IDA {
                return format!("{:?}", n);
            }
        }
    }
    String::new()
}

/// Check if this is a spec.X = Y style attribute
fn is_spec_attr(attr_name: &str) -> bool {
    attr_name.starts_with("spec.") || attr_name.contains('.')
}

/// Extract the key name from spec.X
fn extract_spec_key(attr_name: &str) -> String {
    if let Some(pos) = attr_name.find('.') {
        attr_name[pos + 1..].to_string()
    } else {
        attr_name.to_string()
    }
}

// ============================================================================
// Aggregation: usages → McParamType
// ============================================================================

/// Attribute name → physical unit type mapping.
/// Based on the attribute KEY in the definition body, NOT the parameter name.
fn spec_key_to_unit(key: &str) -> Option<McParamTypeKind> {
    match key.to_lowercase().as_str() {
        "resistance" | "impedance" | "r" => Some(McParamTypeKind::UnitValue { unit: McUnit::Ohm }),
        "voltage" | "volt" | "v" => Some(McParamTypeKind::UnitValue { unit: McUnit::Volt }),
        "capacitance" | "cap" | "c" => Some(McParamTypeKind::UnitValue { unit: McUnit::Cap }),
        "inductance" | "l" => Some(McParamTypeKind::UnitValue { unit: McUnit::Ind }),
        "current" | "i" => Some(McParamTypeKind::UnitValue { unit: McUnit::Amp }),
        "frequency" | "freq" | "f" => Some(McParamTypeKind::UnitValue { unit: McUnit::Hz }),
        "temperature" | "temp" | "t" => Some(McParamTypeKind::UnitValue { unit: McUnit::Temp }),
        "power" | "p" => Some(McParamTypeKind::UnitValue { unit: McUnit::Wat }),
        "tolerance" | "accuracy" => Some(McParamTypeKind::UnitValue {
            unit: McUnit::Percent,
        }),
        "time" | "delay" => Some(McParamTypeKind::UnitValue { unit: McUnit::Time }),
        "length" | "width" | "height" | "len" => {
            Some(McParamTypeKind::UnitValue { unit: McUnit::Len })
        }
        "partno" | "part" | "name" | "label" | "polarity" | "package" => {
            Some(McParamTypeKind::BasicString { default_val: None })
        }
        "quantity" | "count" | "pins" | "rows" | "cols" => {
            Some(McParamTypeKind::BasicInt { default_val: None })
        }
        _ => None,
    }
}

/// Aggregate usage sites into a parameter type with confidence.
pub fn aggregate_usages(param_name: &str, usages: &[UsageSite]) -> InferenceResult {
    if usages.is_empty() {
        return InferenceResult {
            param_name: param_name.to_string(),
            param_type: McParamType::unknown(),
            confidence: 0.0,
            usage_count: 0,
        };
    }

    // Count occurrences by category
    let mut label_count = 0;
    let mut numeric_count = 0;
    let mut string_count = 0;
    let mut int_count = 0;
    let mut unit_counts: std::collections::HashMap<McUnit, usize> =
        std::collections::HashMap::new();

    for usage in usages {
        match &usage.kind {
            UsageKind::NetConnection | UsageKind::PinBinding | UsageKind::Conditional => {
                label_count += 1;
            }
            UsageKind::AttrValue(key) => {
                numeric_count += 1;
                if let Some(kind) = spec_key_to_unit(key) {
                    match kind {
                        McParamTypeKind::UnitValue { unit } => {
                            *unit_counts.entry(unit).or_insert(0) += 1;
                        }
                        McParamTypeKind::BasicString { .. } => string_count += 1,
                        McParamTypeKind::BasicInt { .. } => int_count += 1,
                        _ => {}
                    }
                }
            }
            UsageKind::ArithmeticExpr => {
                numeric_count += 1;
            }
            UsageKind::Assignment(val) => {
                if val.starts_with('"') || val.starts_with('\'') {
                    string_count += 1;
                } else {
                    numeric_count += 1;
                }
            }
            UsageKind::FcallArg | UsageKind::CtorArg | UsageKind::ReturnValue => {
                // Can't determine type from argument position alone — need callee info
                // For now, keep as weak numeric signal
                numeric_count += 1;
            }
            UsageKind::MemberAccess => {
                label_count += 1;
            }
        }
    }

    let total = usages.len() as f32;

    // Strong signals: all usages agree
    if label_count as f32 == total && label_count > 0 {
        return InferenceResult {
            param_name: param_name.to_string(),
            param_type: McParamType {
                kind: McParamTypeKind::Label,
                direction: None,
            },
            confidence: 0.95,
            usage_count: usages.len(),
        };
    }

    if string_count as f32 == total && string_count > 0 {
        return InferenceResult {
            param_name: param_name.to_string(),
            param_type: McParamType {
                kind: McParamTypeKind::BasicString { default_val: None },
                direction: None,
            },
            confidence: 0.95,
            usage_count: usages.len(),
        };
    }

    // Check for dominant unit type from attr values
    if let Some((unit, count)) = unit_counts.iter().max_by_key(|(_, c)| *c) {
        let ratio = *count as f32 / total;
        if ratio >= 0.8 {
            return InferenceResult {
                param_name: param_name.to_string(),
                param_type: McParamType {
                    kind: McParamTypeKind::UnitValue { unit: unit.clone() },
                    direction: None,
                },
                confidence: ratio,
                usage_count: usages.len(),
            };
        }
    }

    // Mixed signals with majority
    let max_count = label_count
        .max(numeric_count)
        .max(string_count)
        .max(int_count);
    let ratio = max_count as f32 / total;

    if ratio >= 0.8 {
        if label_count == max_count {
            return InferenceResult {
                param_name: param_name.to_string(),
                param_type: McParamType {
                    kind: McParamTypeKind::Label,
                    direction: None,
                },
                confidence: 0.7,
                usage_count: usages.len(),
            };
        }
        if numeric_count == max_count {
            return InferenceResult {
                param_name: param_name.to_string(),
                param_type: McParamType {
                    kind: McParamTypeKind::BareNumeric,
                    direction: None,
                },
                confidence: 0.7,
                usage_count: usages.len(),
            };
        }
        if string_count == max_count {
            return InferenceResult {
                param_name: param_name.to_string(),
                param_type: McParamType {
                    kind: McParamTypeKind::BasicString { default_val: None },
                    direction: None,
                },
                confidence: 0.7,
                usage_count: usages.len(),
            };
        }
    }

    // Mixed signals, cannot determine
    InferenceResult {
        param_name: param_name.to_string(),
        param_type: McParamType::unknown(),
        confidence: 0.0,
        usage_count: usages.len(),
    }
}

/// Full inference pipeline for a single parameter.
pub fn infer_param(param_name: &str, body: &AstNode) -> InferenceResult {
    let usages = collect_usages(param_name, body);
    aggregate_usages(param_name, &usages)
}

/// Check for unused parameters — uses all_name_forms() for IDX-aware matching.
pub fn find_unused_params(declares: &[McParamDeclare], body: &AstNode) -> Vec<String> {
    let mut unused = Vec::new();
    for declare in declares {
        if declare.has_type_constraint() {
            continue;
        }
        let name_forms = declare.all_name_forms();
        if name_forms.is_empty() {
            continue;
        }
        let has_usage = name_forms
            .iter()
            .any(|name| !collect_usages(name, body).is_empty());
        if !has_usage {
            if let Some(primary) = declare.get_primary_name() {
                unused.push(primary);
            }
        }
    }
    unused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_usages() {
        let result = aggregate_usages("test", &[]);
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.usage_count, 0);
    }

    #[test]
    fn test_label_dominant() {
        let usages = vec![
            UsageSite {
                kind: UsageKind::NetConnection,
                pos: 0,
            },
            UsageSite {
                kind: UsageKind::NetConnection,
                pos: 1,
            },
            UsageSite {
                kind: UsageKind::PinBinding,
                pos: 2,
            },
        ];
        let result = aggregate_usages("dc24v", &usages);
        assert!(result.confidence >= 0.9);
        assert_eq!(result.param_type.kind, McParamTypeKind::Label);
    }

    #[test]
    fn test_string_dominant() {
        let usages = vec![
            UsageSite {
                kind: UsageKind::Assignment("\"BASE\"".into()),
                pos: 0,
            },
            UsageSite {
                kind: UsageKind::Assignment("\"WIDE\"".into()),
                pos: 1,
            },
        ];
        let result = aggregate_usages("partno", &usages);
        assert!(result.confidence >= 0.9);
        assert!(matches!(
            result.param_type.kind,
            McParamTypeKind::BasicString { .. }
        ));
    }

    #[test]
    fn test_unused_finder() {
        // Placeholder: needs actual AST
        // Test that unused detection works with empty body
    }
}
