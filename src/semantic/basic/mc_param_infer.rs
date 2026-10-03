// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Usage-Based Parameter Type Inference
//!
//! Infers the semantic type of untyped (bare identifier) parameters by
//! analyzing how they are used in the definition body and at call sites.
//!
//! Core principle: NO name-based heuristics. Only usage analysis.

use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::semantic::basic::attr_keys::{self, AttrValueKind};
use crate::semantic::basic::mc_ids::McIds;
use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};
use crate::semantic::basic::mc_paramd::{McParamDeclare, McParamDeclareKind};
use crate::semantic::basic::mc_uval::McUnit;

// Usage Site

/// A single usage site of a parameter in a definition body.
#[derive(Debug, Clone)]
pub struct UsageSite {
    /// What kind of usage this is
    pub kind: UsageKind,
    /// Span location for diagnostics
    pub pos: usize,
}

/// Usage taxonomy for parameter-type inference. Several kinds are classified
/// by the engine but not yet emitted by the current parsers (CtorArg /
/// ReturnValue / MemberAccess), and `Assignment` is constructed only in tests —
/// kept as the complete taxonomy rather than pruning per-parser.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageKind {
    /// `P -> net`, `net -> P`, `P - net`, `P + net`, `net - P`, `net + P`
    NetConnection,
    /// `pins = [ N = P ]`
    PinBinding,
    /// `spec.X = P` or `X = P` in attribute context
    AttrValue(String), // the whole dotted key as written ("spec.capacitance", "name")
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
    /// `P` as a bare identifier leaf under a node no classified arm covers
    /// (e.g. the `&` of `if (P & 0x01)` rides JUDGE_BITAND). Proves the name
    /// is used, but carries no type signal — aggregate counts it as nothing.
    BareIdent,
}

// Inference Engine

/// Result of usage-based type inference for a single parameter.
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub param_type: McParamType,
    /// 0.0 = no confidence (mixed/unused), 1.0 = certain
    pub confidence: f32,
    /// Number of usage sites found; read by the `infer_*` tests.
    #[allow(dead_code)]
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
                    if let Some(key) = attr_key(&n) {
                        collect_attr_usages(&n, &key, param_name, pos, usages);
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
                MCAST_JUDGE_EQEQ
                | MCAST_JUDGE_NOTEQ
                | MCAST_JUDGE_LESSTHAN
                | MCAST_JUDGE_GREATERTHAN
                | MCAST_JUDGE_LESSEQTHAN
                | MCAST_JUDGE_GREATEREQTHAN
                | MCAST_JUDGE_IN
                | MCAST_JUDGE_AND
                | MCAST_JUDGE_OR => {
                    if node_contains_name(&n, param_name) {
                        usages.push(UsageSite {
                            kind: UsageKind::Conditional,
                            pos,
                        });
                    }
                }
                // U300 M9b: `return <expr>` — the returned expression uses
                // every param it names. The expression rides as the return
                // marker's next sibling (NET → IOTYPE_RETURN → expr), so the
                // marker arm reads its next link (connection/attribute shapes
                // alone leave a func whose body is a bare `return x` looking
                // empty and `x` unused).
                MCAST_IOTYPE_RETURN => {
                    if let Some(expr) = n.get_next() {
                        if node_contains_name(&expr, param_name) {
                            usages.push(UsageSite {
                                kind: UsageKind::ReturnValue,
                                pos,
                            });
                        }
                    }
                }
                // Role keyword: `role SOURCE { ... }` in body marks the `role`
                // parameter as used.
                MCAST_ROLE => {
                    if param_name == "role" {
                        usages.push(UsageSite {
                            kind: UsageKind::AttrValue("role".to_string()),
                            pos,
                        });
                    }
                    // Also recurse into children (role name, attrs, pins, etc.)
                    collect_usages_recursive(param_name, &n, usages);
                }
                _ => {
                    // A leaf identifier whose text is the param name is a
                    // usage even when no classified wrapper arm saw it: the
                    // `&` of `if (P & 0x01)` rides JUDGE_BITAND, which no arm
                    // classifies, and plain recursion would reach the leaf
                    // and record nothing (U361). A leaf gives no type signal,
                    // so the site only proves use.
                    if n.get_sub_node().is_none() && n.to_string().as_deref() == Some(param_name) {
                        usages.push(UsageSite {
                            kind: UsageKind::BareIdent,
                            pos,
                        });
                    }
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

/// The attribute key as written (`name`, `spec.capacitance`), read from the
/// line's `MCAST_ATT_ID` child.
fn attr_key(node: &AstNode) -> Option<String> {
    let att_id = node.get_sub_node()?;
    let ids = att_id.get_sub_node()?;
    McIds::new(&ids).map(|m| m.to_string())
}

/// Record the keys of one line that carry `name`: the line's own key, then each
/// nested line under its path — `spec = [ resistance = rs ]` writes the same key
/// as `spec.resistance = rs` (07-attrs.md §3 rule 7).
fn collect_attr_usages(
    node: &AstNode,
    path: &str,
    name: &str,
    pos: usize,
    usages: &mut Vec<UsageSite>,
) {
    let mentions = node
        .get_sub_node()
        .is_some_and(|child| child.iter().any(|n| attr_value_mentions(&n, name)));
    if mentions {
        usages.push(UsageSite {
            kind: UsageKind::AttrValue(path.to_string()),
            pos,
        });
    }
    for inner in nested_attributes(node) {
        let Some(inner_key) = attr_key(&inner) else {
            continue;
        };
        collect_attr_usages(
            &inner,
            &format!("{path}.{inner_key}"),
            name,
            inner.get_pos() as usize,
            usages,
        );
    }
}

/// Does this line's own value carry `name`? A key and a nested line do not — the
/// nested line is read on its own path — and neither does a container, whose
/// text is its first descendant's.
fn attr_value_mentions(node: &AstNode, name: &str) -> bool {
    match node.get_type() {
        MCAST_ATTRIBUTE | MCAST_ATT_ID => false,
        MCAST_ID | MCAST_IDS | MCAST_IDA => node.to_string().as_deref() == Some(name),
        _ => node
            .get_sub_node()
            .is_some_and(|child| child.iter().any(|n| attr_value_mentions(&n, name))),
    }
}

/// The nested attribute lines one level down, under a bracket value.
fn nested_attributes(node: &AstNode) -> Vec<AstNode> {
    let mut found = Vec::new();
    if let Some(child) = node.get_sub_node() {
        for n in child.iter() {
            if n.get_type() == MCAST_ATTRIBUTE {
                found.push(n);
            } else {
                found.extend(nested_attributes(&n));
            }
        }
    }
    found
}

// Aggregation: usages → McParamType

/// Attribute key → parameter type. The key, not the shape of the value written
/// under it, decides what the value means (D5); the key ledger holds that
/// mapping (`semantic::basic::attr_keys`), so the answer is a table read and
/// never a name table kept here.
fn key_to_param_type(key: &str) -> Option<McParamTypeKind> {
    match attr_keys::value_kind(key)? {
        AttrValueKind::Quantity(unit) => Some(McParamTypeKind::UnitValue { unit }),
        AttrValueKind::Text => Some(McParamTypeKind::BasicString { default_val: None }),
        AttrValueKind::Count => Some(McParamTypeKind::BasicInt { default_val: None }),
    }
}

/// Aggregate usage sites into a parameter type with confidence.
pub fn aggregate_usages(usages: &[UsageSite]) -> InferenceResult {
    if usages.is_empty() {
        return InferenceResult {
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
                match key_to_param_type(key) {
                    Some(McParamTypeKind::UnitValue { unit }) => {
                        *unit_counts.entry(unit).or_insert(0) += 1;
                    }
                    Some(McParamTypeKind::BasicString { .. }) => string_count += 1,
                    Some(McParamTypeKind::BasicInt { .. }) => int_count += 1,
                    _ => {}
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
                // Can't determine type from argument/return position alone —
                // need callee signature info. Don't count as numeric to avoid
                // incorrectly inferring params that only appear in function calls.
            }
            UsageKind::BareIdent => {
                // Proves use, no type signal (see UsageKind::BareIdent).
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
        param_type: McParamType::unknown(),
        confidence: 0.0,
        usage_count: usages.len(),
    }
}

/// Full inference pipeline for a single parameter.
pub fn infer_param(param_name: &str, body: &AstNode) -> InferenceResult {
    let usages = collect_usages(param_name, body);
    aggregate_usages(&usages)
}

/// Check for unused parameters — uses all_name_forms() for IDX-aware matching.
pub fn find_unused_params(declares: &[McParamDeclare], body: &AstNode) -> Vec<String> {
    // U385 leg E3: a vector formal's written body spellings never
    // string-equal its canonical form (`kin[4][l,r]` as written vs
    // `kin[4][l, r]` canonical) nor its expanded members, so the per-form
    // scan below misses them. Collect the leaves once; each declare gets a
    // spelling-form fallback ([`declare_used_in_leaves`],
    // layer-expansion-law.md §6.1).
    let mut leaves = Vec::new();
    leaf_texts(body, &mut leaves);
    let mut unused = Vec::new();
    for declare in declares {
        let name_forms = declare.all_name_forms();
        if name_forms.is_empty() {
            continue;
        }
        let has_usage = name_forms
            .iter()
            .any(|name| !collect_usages(name, body).is_empty())
            || declare_used_in_leaves(declare, &leaves);
        if !has_usage {
            let name = declare.display_name();
            if !name.is_empty() {
                unused.push(name);
            }
        }
    }
    unused
}

/// Collect the text of every node (leaves and composites) in the subtree.
fn leaf_texts(node: &AstNode, out: &mut Vec<String>) {
    if let Some(text) = node.to_string() {
        out.push(text);
    }
    if let Some(child) = node.get_sub_node() {
        for n in child.iter() {
            leaf_texts(&n, out);
        }
    }
}

/// The U385 leg E3 spelling-form fallback: a vector/ida formal (square
/// subscript) counts as used when a leaf is its bare base name, its
/// space-stripped canonical form, or its member-index face `<base>[<n>]`.
/// Plain formals return false — the ordinary per-form scan covers them.
fn declare_used_in_leaves(declare: &McParamDeclare, leaves: &[String]) -> bool {
    let McParamDeclareKind::Single(ids) = &declare.kind else {
        return false;
    };
    let Some(base) = ids.get_base_name() else {
        return false;
    };
    let canonical = ids.to_string().replace(' ', "");
    leaves.iter().any(|t| {
        t.as_str() == base
            || t.replace(' ', "") == canonical
            || (t.starts_with(base.as_str())
                && t[base.len()..]
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .is_some_and(|s| s.parse::<usize>().is_ok()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_paraminfer__empty_usages() {
        let result = aggregate_usages(&[]);
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.usage_count, 0);
    }

    #[test]
    fn sem_paraminfer__label_dominant() {
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
        let result = aggregate_usages(&usages);
        assert!(result.confidence >= 0.9);
        assert_eq!(result.param_type.kind, McParamTypeKind::Label);
    }

    #[test]
    fn sem_paraminfer__string_dominant() {
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
        let result = aggregate_usages(&usages);
        assert!(result.confidence >= 0.9);
        assert!(matches!(
            result.param_type.kind,
            McParamTypeKind::BasicString { .. }
        ));
    }

    #[test]
    fn sem_paraminfer__unused_finder() {
        // Placeholder: needs actual AST
        // Test that unused detection works with empty body
    }

    #[test]
    fn sem_paraminfer__bare_ident_proves_use_without_signal() {
        // U361: a bare-ident leaf site proves the param is used (so
        // find_unused_params stays silent) but carries no type signal (so
        // aggregate stays at unknown/0.0 — never feeds a count).
        let usages = vec![
            UsageSite {
                kind: UsageKind::BareIdent,
                pos: 0,
            },
            UsageSite {
                kind: UsageKind::BareIdent,
                pos: 10,
            },
        ];
        let result = aggregate_usages(&usages);
        assert_eq!(result.usage_count, 2);
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.param_type.kind, McParamTypeKind::Unknown);
    }

    #[test]
    fn sem_paraminfer__bare_ident_never_wins_a_vote() {
        // Bare-ident sites carry no vote: with no classified majority the
        // aggregate stays unknown even though usages exist (used-but-untyped),
        // and the sites can never outvote a classified kind.
        let usages = vec![
            UsageSite {
                kind: UsageKind::BareIdent,
                pos: 0,
            },
            UsageSite {
                kind: UsageKind::BareIdent,
                pos: 1,
            },
            UsageSite {
                kind: UsageKind::Conditional,
                pos: 2,
            },
        ];
        let result = aggregate_usages(&usages);
        assert_eq!(result.usage_count, 3);
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.param_type.kind, McParamTypeKind::Unknown);
    }

    #[test]
    fn sem_paraminfer__registered_key_reads_its_unit_from_the_ledger() {
        let usages = vec![UsageSite {
            kind: UsageKind::AttrValue("spec.capacitance".into()),
            pos: 0,
        }];
        let result = aggregate_usages(&usages);
        assert!(matches!(
            result.param_type.kind,
            McParamTypeKind::UnitValue { unit: McUnit::Cap }
        ));
    }

    #[test]
    fn sem_paraminfer__unregistered_key_infers_no_unit() {
        // `spec.Capacitance` is not the registered spelling, and a `spec` key
        // answers to its path rather than to its last segment.
        for key in ["spec.Capacitance", "capacitance", "spec.hbm"] {
            let usages = vec![UsageSite {
                kind: UsageKind::AttrValue(key.into()),
                pos: 0,
            }];
            let result = aggregate_usages(&usages);
            assert!(matches!(
                result.param_type.kind,
                McParamTypeKind::BareNumeric
            ));
        }
    }
}
