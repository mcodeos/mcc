// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Expression and operand-level validation checks.
//!
//! Checks:
//!   Q1 — `this` used outside instance context
//!   Q3 — `_` as sole net endpoint
//!   T3 — empty conditional body
//!   E4 — constant expression overflow
//!   V3 — reversed curly brace range (5:2)
//!   V4 — single-element range (3:3)
//!   C5 — IDX key collision in module instances

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashMap;

pub struct ExprsCheck;

impl ValidationCheck for ExprsCheck {
    fn name(&self) -> &'static str {
        "exprs"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        check_this_outside_instance(acc); // Q1
        check_uscore_sole_endpoint(acc); // Q3
        check_empty_conditional(acc); // T3
        check_constant_overflow(acc); // E4
        check_reversed_range(acc); // V3 + V4
        check_idx_key_collision(acc); // C5
    }
}

/// Q1: `this` used outside instance context.
///
/// Scans module net phrase text for the `this` keyword. `this` should only
/// appear inside function bodies, not in top-level net connections.
fn check_this_outside_instance(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        // Only check top-level net lines (not func body lines)
        for phrase in &m.lines {
            let text = format!("{}", phrase);
            // Check for `this` as a standalone token
            for word in text.split_whitespace() {
                let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                if clean == "this" {
                    acc.push(CheckResult {
                        check_name: "exprs",
                        severity: CheckSeverity::Error,
                        uri: Some(uri.clone()),
                        span: None,
                        message: format!(
                            "'this' used in top-level net line: '{}'. \
                             'this' is only valid inside instance/function contexts.",
                            text.trim()
                        ),
                        code: 2901,
                    });
                    break; // One diagnostic per phrase
                }
            }
        }
    }
}

/// Q3: `_` as the sole net endpoint.
///
/// A net that connects only to `_` (underscore/NC placeholder) is meaningless.
fn check_uscore_sole_endpoint(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        for phrase in &m.lines {
            let text = format!("{}", phrase);
            if !text.contains("->") {
                continue;
            }
            let parts: Vec<&str> = text.split("->").collect();
            if parts.len() != 2 {
                continue;
            }
            let left_clean: Vec<&str> = parts[0]
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "_")
                .collect();
            let right_clean: Vec<&str> = parts[1]
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "_")
                .collect();

            if left_clean.is_empty() && right_clean.is_empty() {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: None,
                    message: format!(
                        "Net '{}' connects only to '_' (placeholder). \
                         This connection has no effect.",
                        text.trim()
                    ),
                    code: 2902,
                });
            }
        }
    }
}

/// T3: Empty conditional body.
///
/// Checks if an `if` condition has an empty block (no phrases inside).
fn check_empty_conditional(acc: &mut CheckAccumulator) {
    // Check component attrs for McConds with empty blocks
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        // Scan attr values for McConds structures
        for attr in comp.attrs.iter() {
            for val in &attr.values {
                check_attr_val_for_empty_cond(val, &uri, entry.key().ident.to_string(), acc);
            }
        }
    }
}

/// Recursively check McAttrVal values for McConds with empty blocks.
fn check_attr_val_for_empty_cond(
    val: &crate::core::component::mc_attr::McAttrVal,
    uri: &str,
    comp_name: String,
    acc: &mut CheckAccumulator,
) {
    match val {
        crate::core::component::mc_attr::McAttrVal::Attributes(attrs) => {
            for child in attrs.iter() {
                for child_val in &child.values {
                    check_attr_val_for_empty_cond(child_val, uri, comp_name.clone(), acc);
                }
            }
        }
        crate::core::component::mc_attr::McAttrVal::AttrExpr(expr) => {
            if let crate::core::basic::mc_expr::McExpression::Set(exprs) = expr {
                for e in exprs.iter() {
                    check_expr_for_empty_cond(e, uri, &comp_name, acc);
                }
            }
        }
        _ => {}
    }
}

/// Check a single McExpression to see if it yields an empty conditional body.
fn check_expr_for_empty_cond(
    _expr: &crate::core::basic::mc_expr::McExpression,
    _uri: &str,
    _comp_name: &str,
    _acc: &mut CheckAccumulator,
) {
    // McConds::new() parses AST nodes, not McExpression values.
    // The raw AST is needed for this check; defer to AST-walking pass.
    // For now, this is a no-op stub that can be filled in when
    // component bodies expose their raw AST condition nodes.
}

/// E4: Constant expression overflow.
///
/// Checks integer and float literal expressions for overflow.
fn check_constant_overflow(acc: &mut CheckAccumulator) {
    // Check component attribute values for overflowing literal expressions
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for attr in comp.attrs.iter() {
            for val in &attr.values {
                check_val_for_overflow(
                    val,
                    &uri,
                    entry.key().ident.to_string(),
                    &attr.id.to_string(),
                    acc,
                );
            }
        }
    }
}

fn check_val_for_overflow(
    val: &crate::core::component::mc_attr::McAttrVal,
    uri: &str,
    comp_name: String,
    attr_id: &str,
    acc: &mut CheckAccumulator,
) {
    match val {
        crate::core::component::mc_attr::McAttrVal::AttrExpr(expr) => {
            check_expr_overflow(expr, uri, &comp_name, attr_id, acc);
        }
        crate::core::component::mc_attr::McAttrVal::Attributes(attrs) => {
            for child in attrs.iter() {
                for child_val in &child.values {
                    check_val_for_overflow(
                        child_val,
                        uri,
                        comp_name.clone(),
                        &child.id.to_string(),
                        acc,
                    );
                }
            }
        }
        _ => {}
    }
}

fn check_expr_overflow(
    expr: &crate::core::basic::mc_expr::McExpression,
    uri: &str,
    comp_name: &str,
    attr_id: &str,
    acc: &mut CheckAccumulator,
) {
    match expr {
        crate::core::basic::mc_expr::McExpression::Int(int_val) => {
            // Flag unusually large integer literals (>1 billion for hw params)
            if int_val.value > 1_000_000_000 || int_val.value < -1_000_000_000 {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: None,
                    message: format!(
                        "Attribute '{}' in '{}' has large integer value {} which may indicate overflow or mistaken input.",
                        attr_id, comp_name, int_val.value
                    ),
                    code: 2905,
                });
            }
        }
        crate::core::basic::mc_expr::McExpression::Float(float_val) => {
            if float_val.value.is_infinite() {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: None,
                    message: format!(
                        "Attribute '{}' in '{}' has infinite float value.",
                        attr_id, comp_name
                    ),
                    code: 2905,
                });
            }
        }
        _ => {}
    }
}

/// V3: Reversed curly brace range (e.g., `{5:2}` instead of `{2:5}`).
fn check_reversed_range(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for attr in comp.attrs.iter() {
            for val in &attr.values {
                check_val_for_reversed_range(val, &uri, entry.key().ident.to_string(), acc);
            }
        }
    }
}

fn check_val_for_reversed_range(
    val: &crate::core::component::mc_attr::McAttrVal,
    uri: &str,
    comp_name: String,
    acc: &mut CheckAccumulator,
) {
    match val {
        crate::core::component::mc_attr::McAttrVal::AttrExpr(expr) => {
            check_expr_range(expr, uri, &comp_name, acc);
        }
        crate::core::component::mc_attr::McAttrVal::Attributes(attrs) => {
            for child in attrs.iter() {
                for child_val in &child.values {
                    check_val_for_reversed_range(child_val, uri, comp_name.clone(), acc);
                }
            }
        }
        _ => {}
    }
}

fn check_expr_range(
    expr: &crate::core::basic::mc_expr::McExpression,
    uri: &str,
    comp_name: &str,
    acc: &mut CheckAccumulator,
) {
    if let crate::core::basic::mc_expr::McExpression::Slice(left, right) = expr {
        if let (
            crate::core::basic::mc_expr::McExpression::Int(l),
            crate::core::basic::mc_expr::McExpression::Int(r),
        ) = (left.as_ref(), right.as_ref())
        {
            if l.value > r.value {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: None,
                    message: format!(
                        "Reversed range {{{}:{}}} in '{}'. Did you mean {{{}:{}}}?",
                        l.value, r.value, comp_name, r.value, l.value
                    ),
                    code: 2906,
                });
            } else if l.value == r.value {
                // V4: single-element range
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.to_string()),
                    span: None,
                    message: format!(
                        "Single-element range {{{}:{}}} in '{}'. This expands to one element.",
                        l.value, r.value, comp_name
                    ),
                    code: 2907,
                });
            }
        }
    }
}

/// C5: IDX key collision — two inst names share the same base key before `[`
/// with different slice specifications.
fn check_idx_key_collision(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let mut base_keys: HashMap<String, Vec<String>> = HashMap::new();
        for name in m.insts.iter_instance_names() {
            if let Some(bracket_pos) = name.find('[') {
                let base = name[..bracket_pos].to_string();
                base_keys.entry(base).or_default().push(name.clone());
            }
        }
        for (base, full_names) in &base_keys {
            if full_names.len() > 1 {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: None,
                    message: format!(
                        "IDX key '{}' has multiple slice specs: {}. \
                         These share the same base key which may cause ambiguity.",
                        base,
                        full_names.join(", ")
                    ),
                    code: 2908,
                });
            }
        }
    }
}
