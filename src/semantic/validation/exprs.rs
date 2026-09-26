// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Expression and operand-level validation checks.
//!
//! Checks:
//!   Q1 — `this` used outside instance context
//!   Q3 — `_` as sole net endpoint
//!   U283 — open lead: an anchored end meets a free `_`
//!   E4 — constant expression overflow
//!   V3 — reversed curly brace range (5:2)
//!   V4 — single-element range (3:3)
//!   C5 — IDX key collision in module instances

use super::body::collect_referenced_names;
use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::basic::mc_phrase::McPhrase;
use std::collections::{HashMap, HashSet};

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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_this_outside_instance(acc); // Q1
        check_uscore_sole_endpoint(acc); // Q3
        check_open_lead(acc); // U283
        check_constant_overflow(acc); // E4
        check_reversed_range(acc); // V3 + V4
        check_idx_key_collision(acc); // C5
    }
}

/// Q1: `this` used outside instance context.
///
/// `this` refers to the current instance and is only valid inside function
/// bodies, not in top-level net connections. Detected structurally: `this`
/// parses to an endpoint label `Label("this")`, collected by walking the
/// phrase AST rather than by re-scanning the statement's display text.
fn check_this_outside_instance(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, module) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        // Only check top-level net stmts (not func body stmts)
        for phrase in &module.stmts {
            let mut names = HashSet::new();
            collect_referenced_names(phrase, &mut names);
            if names.contains("this") {
                let text = format!("{}", phrase);
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(module.span.start..module.span.end),
                    message: format!(
                        "'this' used in top-level net line: '{}'. \
                         'this' is only valid inside instance/function contexts.",
                        text.trim()
                    ),
                    code: crate::errcodes::EXPR_THIS_TOP_LEVEL,
                });
            }
        }
    }
}

/// Q3: `_` as the sole net endpoint.
///
/// A net that connects only to `_` (underscore/NC placeholder) is meaningless.
/// `_` parses to `McPhrase::Lead`, so an all-`Lead` series (e.g. `_ -> _`)
/// is detected structurally without splitting the statement's display text.
fn check_uscore_sole_endpoint(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, module) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for phrase in &module.stmts {
            if let McPhrase::Series(items, _) = phrase {
                if !items.is_empty() && items.iter().all(|p| matches!(p, McPhrase::Lead(_))) {
                    let text = format!("{}", phrase);
                    acc.push(CheckResult {
                        check_name: "exprs",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(module.span.start..module.span.end),
                        message: format!(
                            "Net '{}' connects only to '_' (placeholder). \
                             This connection has no effect.",
                            text.trim()
                        ),
                        code: crate::errcodes::EXPR_PLACEHOLDER_ONLY,
                    });
                }
            }
        }
    }
}

/// U283: open lead — an anchored end meets a free `_`
/// (open-lead-design.md §2).
///
/// Per-statement census next to Q3: a series holding both anchored operands
/// and a `_` placeholder leaves the anonymous point with exactly one anchor
/// — a free end nothing can reach. Placeholders are statement-local and can
/// never be connected by a parent layer, so the statement face may report.
/// Two or more anchors make the anonymous point an interior splice (the
/// star form `(a, b) -> _`) and stay silent; zero anchors is the floating
/// wire, Q3's face. Anonymous wires never merge across statements, so the
/// census counts per statement and never dedups.
fn check_open_lead(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, module) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for phrase in &module.stmts {
            match phrase {
                // §10.6: a bare `(,)` group statement is a statement list —
                // each branch stands alone for the census. A group nested as
                // a series operand is instead a fan SHAPE (the point
                // collectors flatten it onto one copper), so it is not
                // expanded here; anchor_leaves sums its branches.
                McPhrase::Group(_) => {
                    if let Some(stmts) = phrase.expand_group_statements() {
                        for stmt in &stmts {
                            judge_open_lead(stmt, &uri, acc);
                        }
                    }
                }
                other => judge_open_lead(other, &uri, acc),
            }
        }
    }
}

fn judge_open_lead(phrase: &McPhrase, uri: &str, acc: &mut CheckAccumulator) {
    match phrase {
        McPhrase::Series(items, _) => {
            // A `(,)` group riding inside a chain is judged branch by branch
            // — the split is all the group contributes, so each branch's own
            // free `_` is judged on its own chain exactly as if it were a
            // standalone statement. The nested-series arm covers the
            // preserved sub-chain shape the expansion itself now produces
            // (R0, b4034).
            for nested in items.iter().filter(|p| matches!(p, McPhrase::Group(_))) {
                if let Some(stmts) = nested.expand_group_statements() {
                    for stmt in &stmts {
                        judge_open_lead(stmt, uri, acc);
                    }
                }
            }
            for sub in items.iter().filter(|p| matches!(p, McPhrase::Series(_, _))) {
                judge_open_lead(sub, uri, acc);
            }
            let Some(McPhrase::Lead(off)) = items.iter().find(|p| matches!(p, McPhrase::Lead(_)))
            else {
                return;
            };
            let off = *off as usize;
            let anchors: usize = items
                .iter()
                .filter(|p| !matches!(p, McPhrase::Lead(_)))
                .map(anchor_leaves)
                .sum();
            if anchors != 1 {
                // 0 = floating wire (Q3's face); >=2 = interior splice, silent.
                return;
            }
            let text = format!("{}", phrase);
            acc.push(CheckResult {
                check_name: "exprs",
                severity: CheckSeverity::Warning,
                uri: Some(uri.to_string()),
                // A6: anchor the report where the free `_` was written.
                span: Some(off..off + 1),
                message: format!(
                    "Open lead: '{}' leaves a free '_' placeholder — the \
                     anonymous point gathers a single anchor and nothing can \
                     reach it.",
                    text.trim()
                ),
                code: crate::errcodes::OPEN_LEAD,
            });
        }
        McPhrase::Parallel(items) => {
            for branch in items {
                judge_open_lead(branch, uri, acc);
            }
        }
        _ => {}
    }
}

/// Count the anchored endpoints an operand contributes to the statement's
/// copper — an AST walk, no text inspection. An endpoint is one leaf per
/// declared member; groups fan out into their branches; `_` contributes
/// none.
///
/// The count is a **reference** question (U308): it asks how many endpoints
/// are *written*, not how wide any of them evaluates. `McRef::leaves()` walks
/// every branch of a list / node junction in source order, so `[A, B]` counts
/// two — the width-shaped view answers an empty list for a declared port whose
/// shape the Pass1 opcheck deliberately blanks (`mc_phrase.rs` §8.9.6.3 shape
/// by use), and reading that empty list as a count of zero silently drops the
/// free-end warning (measured in
/// `lock_pp_exprs__open_lead_4065_fires_on_a_declared_port_anchor`).
fn anchor_leaves(phrase: &McPhrase) -> usize {
    match phrase {
        McPhrase::Lead(_) => 0,
        McPhrase::Endpoint(ep) => ep
            .leaves()
            .iter()
            .map(|r| r.to_bus().member.len().max(1))
            .sum(),
        McPhrase::Multiple(v) | McPhrase::Parallel(v) | McPhrase::Series(v, _) => {
            v.iter().map(anchor_leaves).sum()
        }
        McPhrase::Reversed(inner) | McPhrase::Transposed(inner) | McPhrase::Member(inner, _) => {
            anchor_leaves(inner)
        }
        McPhrase::Group(_) => phrase
            .expand_group_statements()
            .map(|stmts| stmts.iter().map(anchor_leaves).sum())
            .unwrap_or(0),
        McPhrase::Closure(_) | McPhrase::FuncCall(_) => 1,
    }
}

/// E4: Constant expression overflow.
///
/// Checks integer and float literal expressions for overflow.
fn check_constant_overflow(acc: &mut CheckAccumulator) {
    // Check component attribute values for overflowing literal expressions
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp_span = comp.span.start..comp.span.end;
        for attr in comp.attrs.iter() {
            for val in &attr.values {
                check_val_for_overflow(
                    val,
                    &uri,
                    sn.ident.to_string(),
                    &attr.id.to_string(),
                    comp_span.clone(),
                    acc,
                );
            }
        }
    }
}

fn check_val_for_overflow(
    val: &crate::semantic::component::mc_attr::McAttrVal,
    uri: &str,
    comp_name: String,
    attr_id: &str,
    comp_span: std::ops::Range<usize>,
    acc: &mut CheckAccumulator,
) {
    match val {
        crate::semantic::component::mc_attr::McAttrVal::AttrExpr(expr) => {
            check_expr_overflow(expr, uri, &comp_name, attr_id, comp_span, acc);
        }
        crate::semantic::component::mc_attr::McAttrVal::Attributes(attrs) => {
            for child in attrs.iter() {
                for child_val in &child.values {
                    check_val_for_overflow(
                        child_val,
                        uri,
                        comp_name.clone(),
                        &child.id.to_string(),
                        comp_span.clone(),
                        acc,
                    );
                }
            }
        }
        _ => {}
    }
}

fn check_expr_overflow(
    expr: &crate::semantic::basic::mc_expr::McExpression,
    uri: &str,
    comp_name: &str,
    attr_id: &str,
    comp_span: std::ops::Range<usize>,
    acc: &mut CheckAccumulator,
) {
    match expr {
        crate::semantic::basic::mc_expr::McExpression::Int(int_val) => {
            // Flag unusually large integer literals (>1 billion for hw params)
            if int_val.value > 1_000_000_000 || int_val.value < -1_000_000_000 {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: Some(comp_span.clone()),
                    message: format!(
                        "Attribute '{}' in '{}' has large integer value {} which may indicate overflow or mistaken input.",
                        attr_id, comp_name, int_val.value
                    ),
                    code: crate::errcodes::ATTR_LARGE_INT,
                });
            }
        }
        crate::semantic::basic::mc_expr::McExpression::Float(float_val) => {
            if float_val.value.is_infinite() {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: Some(comp_span.clone()),
                    message: format!(
                        "Attribute '{}' in '{}' has infinite float value.",
                        attr_id, comp_name
                    ),
                    code: crate::errcodes::ATTR_INFINITE_FLOAT,
                });
            }
        }
        _ => {}
    }
}

/// V3: Reversed curly brace range (e.g., `{5:2}` instead of `{2:5}`).
fn check_reversed_range(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp_span = comp.span.start..comp.span.end;
        for attr in comp.attrs.iter() {
            for val in &attr.values {
                check_val_for_reversed_range(
                    val,
                    &uri,
                    sn.ident.to_string(),
                    comp_span.clone(),
                    acc,
                );
            }
        }
    }
}

fn check_val_for_reversed_range(
    val: &crate::semantic::component::mc_attr::McAttrVal,
    uri: &str,
    comp_name: String,
    comp_span: std::ops::Range<usize>,
    acc: &mut CheckAccumulator,
) {
    match val {
        crate::semantic::component::mc_attr::McAttrVal::AttrExpr(expr) => {
            check_expr_range(expr, uri, &comp_name, comp_span, acc);
        }
        crate::semantic::component::mc_attr::McAttrVal::Attributes(attrs) => {
            for child in attrs.iter() {
                for child_val in &child.values {
                    check_val_for_reversed_range(
                        child_val,
                        uri,
                        comp_name.clone(),
                        comp_span.clone(),
                        acc,
                    );
                }
            }
        }
        _ => {}
    }
}

fn check_expr_range(
    expr: &crate::semantic::basic::mc_expr::McExpression,
    uri: &str,
    comp_name: &str,
    comp_span: std::ops::Range<usize>,
    acc: &mut CheckAccumulator,
) {
    if let crate::semantic::basic::mc_expr::McExpression::Slice(left, right) = expr {
        if let (
            crate::semantic::basic::mc_expr::McExpression::Int(l),
            crate::semantic::basic::mc_expr::McExpression::Int(r),
        ) = (left.as_ref(), right.as_ref())
        {
            if l.value > r.value {
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.to_string()),
                    span: Some(comp_span.clone()),
                    message: format!(
                        "Reversed range {{{}:{}}} in '{}'. Did you mean {{{}:{}}}?",
                        l.value, r.value, comp_name, r.value, l.value
                    ),
                    code: crate::errcodes::RANGE_REVERSED,
                });
            } else if l.value == r.value {
                // V4: single-element range
                acc.push(CheckResult {
                    check_name: "exprs",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.to_string()),
                    span: Some(comp_span.clone()),
                    message: format!(
                        "Single-element range {{{}:{}}} in '{}'. This expands to one element.",
                        l.value, r.value, comp_name
                    ),
                    code: crate::errcodes::RANGE_SINGLE_ELEMENT,
                });
            }
        }
    }
}

/// C5: IDX key collision — two inst names share the same base key before `[`
/// with different slice specifications.
fn check_idx_key_collision(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, module) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let mut base_keys: HashMap<String, Vec<String>> = HashMap::new();
        for name in module.insts.iter_instance_names() {
            if let Some(bracket_pos) = name.find('[') {
                // Skip anonymous-bus / label-list instances.
                // `[VDD_3V3,GND]` is a label list whose members are
                // module port labels — no base key to collide on.
                if bracket_pos == 0 {
                    continue;
                }
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
                    span: Some(module.span.start..module.span.end),
                    message: format!(
                        "IDX key '{}' has multiple slice specs: {}. \
                         These share the same base key which may cause ambiguity.",
                        base,
                        full_names.join(", ")
                    ),
                    code: crate::errcodes::IDX_MULTIPLE_SLICE_SPEC,
                });
            }
        }
    }
}
