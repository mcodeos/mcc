// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Body-level syntax and expression validation.
//!
//! Checks:
//!   L1 — Mixed `.` and `/` path separators in URIs
//!   S4 — `this` on LHS of `::` declaration
//!   T1 — Bitwise operator (`&`/`|`) in condition context
//!   C4-ext — Module port declared but never connected in any net

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::basic::mc_conds::McCondition;
use crate::semantic::basic::mc_endpoint::McEndpoint;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use std::collections::HashSet;

pub struct BodyCheck;

impl ValidationCheck for BodyCheck {
    fn name(&self) -> &'static str {
        "body"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_mixed_path_separators(acc); // L1
        check_this_lhs_declaration(acc); // S4
        check_bitwise_in_condition(acc); // T1
        check_unconnected_module_ports(acc); // C4-ext
    }
}

// ============================================================================
// L1: Mixed `.` and `/` path separators in URIs
// ============================================================================

/// URIs should consistently use either `.` (dot-notation namespace, like
/// `mcode.SPI`) or `/` (filesystem path notation, like `mcode/SPI`), but
/// not both styles in the same URI. Mixed separators indicate a typo or
/// inconsistent path construction.
///
/// A `.mc` file extension is excluded from consideration — only dots that
/// appear as namespace separators (not followed by `mc` or other common
/// extensions) count toward the "has dot" test.
fn check_mixed_path_separators(acc: &mut CheckAccumulator) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut uri_spans: std::collections::HashMap<String, std::ops::Range<usize>> =
        std::collections::HashMap::new();

    // Collect all unique URIs from all workspace tables
    {
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
        for e in comps.iter() {
            seen.insert(e.key().uri.to_string());
        }
        let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
        for e in ifaces.iter() {
            seen.insert(e.key().uri.to_string());
        }
        let enums = &crate::db::cmie::tables::WORKSPACE.enums;
        for e in enums.iter() {
            seen.insert(e.key().uri.to_string());
        }
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for e in modules.iter() {
            let uri = e.key().uri.to_string();
            let span = e.value().span.clone();
            uri_spans.insert(uri.clone(), span.start..span.end);
            seen.insert(uri);
        }
        let mcodes = &crate::db::cmie::tables::WORKSPACE.mcodes;
        for e in mcodes.iter() {
            seen.insert(e.key().clone());
        }
    }

    for uri in &seen {
        if super::is_test_file(uri) {
            continue;
        }

        let has_slash = uri.contains('/');

        // Check for dot-as-namespace-separator (not file extension).
        // A dot that is followed by a known extension is excluded.
        let has_namespace_dot = {
            let dots: Vec<usize> = uri.match_indices('.').map(|(i, _)| i).collect();
            dots.iter().any(|&pos| {
                let after_dot = &uri[pos + 1..];
                // Exclude common file extensions
                !after_dot.starts_with("mc/")
                    && !after_dot.starts_with("mc")
                    && !after_dot.starts_with("json/")
                    && !after_dot.starts_with("json")
                    && !after_dot.starts_with("yaml/")
                    && !after_dot.starts_with("yaml")
                    && !after_dot.starts_with("toml/")
                    && !after_dot.starts_with("toml")
                    && after_dot.contains('.')
            })
        };

        if has_slash && has_namespace_dot {
            let span = uri_spans.get(uri).cloned();
            acc.push(CheckResult {
                check_name: "body",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span,
                message: format!(
                    "URI '{}' mixes '.' (namespace) and '/' (path) separators. \
                     Use one style consistently.",
                    uri
                ),
                code: crate::errcodes::USE_MIXED_PATH_SEPARATORS,
            });
        }
    }
}

// ============================================================================
// P7: `return` with literal instead of endpoint
// ============================================================================

// ============================================================================
// S4: `this` on LHS of `::` declaration
// ============================================================================

/// `this :: TYPE` is invalid syntax. The `this` keyword refers to the
/// current instance and cannot be used as a new instance name.
/// Valid: `r1 :: RES(10k)`  Invalid: `this :: RES(10k)`
///
/// `this :: RES(...)` parses as an ordinary instance declaration whose name is
/// the reserved keyword — detect it structurally by instance kind, not by
/// re-scanning the declaration's display text. A bare `this` used as a net
/// endpoint parses to `McInstance::Label("this")` and is NOT a declaration.
fn check_this_lhs_declaration(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let span = m.span.start..m.span.end;
        let declared_this = m.insts.iter_with_iotype().any(|(name, (_io, inst))| {
            name == "this" && !matches!(inst, crate::McInstance::Label(_))
        });
        if declared_this {
            acc.push(CheckResult {
                check_name: "body",
                severity: CheckSeverity::Error,
                uri: Some(uri.clone()),
                span: Some(span),
                message: format!(
                    "Module '{}': 'this :: TYPE' is invalid — 'this' refers to the \
                     current instance and cannot be used as a new instance name \
                     on the LHS of '::'.",
                    entry.key().ident
                ),
                code: crate::errcodes::INST_THIS_TYPE,
            });
        }
    }
}

// ============================================================================
// T1: Bitwise operator (`&`/`|`) in condition context
// ============================================================================

/// In component conditional blocks (`if ...`), using `&` (bitwise AND) or
/// `|` (bitwise OR) where `&&` (logical AND) or `||` (logical OR) is
/// intended is a common mistake.
///
/// A condition of the form `In { Literal(0|1) In ... }` — comparing against a
/// single binary value — suggests a bitwise-operation result is being used as
/// a boolean; worth a review for clarity. Detected structurally on
/// `McCondition`, not by scanning its `Debug` text.
fn check_bitwise_in_condition(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Inspect conditional pin conditions
        for (idx, cp) in comp.cond_pins.iter().enumerate() {
            for (bidx, (cond, _pins)) in cp.if_blocks.iter().enumerate() {
                if is_single_binary_in(cond) {
                    push_single_binary_diag(
                        acc,
                        &uri,
                        &Some(cp.span.clone()),
                        &comp.name.to_string(),
                        &format!("cond_pins[{}] if-block[{}]", idx, bidx),
                    );
                }
            }
        }

        // Inspect conditional attr conditions
        for (idx, ca) in comp.cond_attrs.iter().enumerate() {
            for (bidx, (cond, _attrs)) in ca.if_blocks.iter().enumerate() {
                if is_single_binary_in(cond) {
                    push_single_binary_diag(
                        acc,
                        &uri,
                        &Some(ca.span.clone()),
                        &comp.name.to_string(),
                        &format!("cond_attrs[{}] if-block[{}]", idx, bidx),
                    );
                }
            }
        }
    }
}

/// `In { Literal("0"|"1"), .. }` — comparing against a single binary value
/// on the left side of the `In` condition.
fn is_single_binary_in(cond: &McCondition) -> bool {
    use crate::semantic::basic::mc_conds::McCondOperand;
    match cond {
        McCondition::In { left, .. } => {
            matches!(left, McCondOperand::Literal(v) if v == "0" || v == "1")
        }
        _ => false,
    }
}

fn push_single_binary_diag(
    acc: &mut CheckAccumulator,
    uri: &str,
    comp_span: &Option<std::ops::Range<usize>>,
    comp_name: &str,
    where_: &str,
) {
    acc.push(CheckResult {
        check_name: "body",
        severity: CheckSeverity::Info,
        uri: Some(uri.to_string()),
        span: comp_span.clone(),
        message: format!(
            "In component '{}' {}: condition compares against a single binary value. \
             If this is a bitwise operation result used as boolean, consider using \
             explicit comparison (e.g., `(flags & MASK) != 0`).",
            comp_name, where_
        ),
        code: crate::errcodes::COND_SINGLE_BINARY,
    });
}

// ============================================================================
// C4-ext: Module port declared but never connected in any net
// ============================================================================

/// A module port (declared as a parameter) that appears in `insts` but is
/// never referenced in any `->` connection line is a floating/unused port.
///
/// This is the module-level complement to P4 (unconnected output port in
/// pass2) — it catches unused formal parameters at the definition level.
fn check_unconnected_module_ports(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        // Synthetic `module VIRT_<T>` wrappers fabricated for standalone
        // component/interface views are never user module code: their `io`
        // ports exist only to render boundary pins, so "declared but never
        // connected" is expected. Skip them (mirrors the MODULE_STUB carve-out
        // in conds.rs).
        if crate::build::vinst::is_synthetic_module(&entry.key().ident.to_string()) {
            continue;
        }

        // Collect all port/instance names declared in `insts`
        let declared: HashSet<String> = m.insts.iter_instance_names().cloned().collect();

        if declared.is_empty() {
            continue;
        }

        // Collect all names referenced in net connection stmts and function bodies.
        // Walk the McPhrase AST directly instead of formatting to text and splitting,
        // because text-based splitting corrupts names when parentheses, brackets, or
        // function-call commas are present (e.g. `GND)` instead of `GND`).
        let mut referenced: HashSet<String> = HashSet::new();
        for phrase in &m.stmts {
            collect_referenced_names(phrase, &mut referenced);
        }
        for func in m.funcs.iter() {
            for phrase in &func.stmts {
                collect_referenced_names(phrase, &mut referenced);
            }
        }

        // Report ports that are declared but never referenced
        for port_name in &declared {
            if !referenced.contains(port_name)
                && !port_name.starts_with('@')   // internal labels
                && !port_name.starts_with('[')
            // bus brackets
            {
                // Check if it's a module formal parameter port
                let is_param = m.params.is_defined(port_name);
                if !is_param {
                    continue; // Skip instances — they might have internal connections
                }

                acc.push(CheckResult {
                    check_name: "body",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(m.span.start..m.span.end),
                    message: format!(
                        "Module '{}': port '{}' is declared but never connected in any net. \
                         Consider removing it or wiring it up.",
                        entry.key().ident,
                        port_name
                    ),
                    code: crate::errcodes::MODULE_PORT_UNUSED,
                });
            }
        }
    }
}

// ============================================================================
// AST-walking helpers for collecting referenced names from McPhrase trees.
// Replaces the former text-based splitting approach that corrupted names when
// parentheses, brackets, or function-call commas were present.
// ============================================================================

/// Recursively walk a `McPhrase` and collect all endpoint base names,
/// including names passed as function-call arguments.
///
/// `pub(crate)` — reused by `mcc show nets <OWNER.FUNC>` to build the
/// func-body connection-line nets.
pub(crate) fn collect_referenced_names(phrase: &McPhrase, names: &mut HashSet<String>) {
    match phrase {
        McPhrase::Lead => {}
        McPhrase::Endpoint(ep) => collect_endpoint_names(ep, names),
        McPhrase::Series(items, _) => {
            for item in items {
                collect_referenced_names(item, names);
            }
        }
        McPhrase::Parallel(items) | McPhrase::Multiple(items) => {
            for item in items {
                collect_referenced_names(item, names);
            }
        }
        McPhrase::Group(g) => {
            for item in &g.opds {
                collect_referenced_names(item, names);
            }
        }
        McPhrase::Transposed(p) => collect_referenced_names(p, names),
        McPhrase::Closure(c) => {
            for line in &c.body {
                collect_referenced_names(line, names);
            }
        }
        McPhrase::FuncCall(fc) => {
            if let Some(caller) = &fc.caller {
                collect_referenced_names(caller, names);
            }
            // Ports can be passed as function-call arguments (e.g.
            // `uC.power([VDD_3V3, GND], ...)`), so walk params too.
            for param in &fc.params {
                collect_param_names(param, names);
            }
        }
        McPhrase::Member(inner, ep) => {
            collect_referenced_names(inner, names);
            collect_endpoint_names(ep, names);
        }
    }
}

/// Collect bare endpoint names from a module net phrase that should be
/// treated as inline labels (for `show instances`).
///
/// Unlike [`collect_referenced_names`], this:
///   - skips `FuncCall` nodes entirely — their parameters are arguments to
///     component/module funcs (e.g. `X6.setup(GND, NC)`, `uC.power(...)`),
///     not module-scope labels, and
///   - ignores the member endpoint of `Member` phrases (`complex.pin`),
///     since those are pin accesses, not label definitions.
pub(crate) fn collect_net_label_names(phrase: &McPhrase, names: &mut HashSet<String>) {
    match phrase {
        McPhrase::Lead => {}
        McPhrase::Endpoint(ep) => collect_endpoint_names(ep, names),
        McPhrase::Series(items, _) | McPhrase::Parallel(items) | McPhrase::Multiple(items) => {
            for item in items {
                collect_net_label_names(item, names);
            }
        }
        McPhrase::Group(g) => {
            for item in &g.opds {
                collect_net_label_names(item, names);
            }
        }
        McPhrase::Transposed(p) => collect_net_label_names(p, names),
        McPhrase::Closure(c) => {
            for line in &c.body {
                collect_net_label_names(line, names);
            }
        }
        McPhrase::FuncCall(_) => {}
        McPhrase::Member(inner, _) => collect_net_label_names(inner, names),
    }
}

/// Collect base names from an `McEndpoint`.
fn collect_endpoint_names(ep: &McEndpoint, names: &mut HashSet<String>) {
    match ep {
        McEndpoint::Single(ref_) => {
            names.insert(ref_.base.get_name());
        }
        McEndpoint::List(nodes) => {
            for node in nodes {
                collect_endpoint_names(node, names);
            }
        }
        McEndpoint::Node { input, output } => {
            for node in input.iter().chain(output.iter()) {
                collect_endpoint_names(node, names);
            }
        }
    }
}

/// Collect identifier names from function-call parameter values.
fn collect_param_names(param: &McParamValue, names: &mut HashSet<String>) {
    match param {
        McParamValue::Ids(ids) => {
            names.insert(ids.to_string());
        }
        McParamValue::Opd(opd) => {
            names.insert(opd.to_string());
        }
        McParamValue::Phrase(p) => {
            collect_referenced_names(p, names);
        }
        McParamValue::Set(items) => {
            for item in items {
                collect_param_names(item, names);
            }
        }
        _ => {} // Constants, numbers, strings — not port references
    }
}
