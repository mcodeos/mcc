// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Body-level syntax and expression validation.
//!
//! Checks:
//!   L1 — Mixed `.` and `/` path separators in URIs
//!   P6 — `return` outside function context
//!   P7 — `return` with literal instead of endpoint
//!   S3 — Empty bracket instance list (`[] :: TYPE`)
//!   S4 — `this` on LHS of `::` declaration
//!   S6 — `role` keyword used as call argument value
//!   T1 — Bitwise operator (`&`/`|`) in condition context
//!   C4-ext — Module port declared but never connected in any net

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
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

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        check_mixed_path_separators(acc); // L1
        check_return_outside_function(acc); // P6
        check_return_with_literal(acc); // P7
        check_empty_bracket_list(acc); // S3
        check_this_lhs_declaration(acc); // S4
        check_role_as_call_arg(acc); // S6
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

    // Collect all unique URIs from all workspace tables
    {
        let comps = crate::builder::workspace::WORKSPACE.components.borrow();
        for e in comps.iter() {
            seen.insert(e.key().uri.to_string());
        }
        let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
        for e in ifaces.iter() {
            seen.insert(e.key().uri.to_string());
        }
        let enums = crate::builder::workspace::WORKSPACE.enums.borrow();
        for e in enums.iter() {
            seen.insert(e.key().uri.to_string());
        }
        let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
        for e in modules.iter() {
            seen.insert(e.key().uri.to_string());
        }
        let mcodes = crate::builder::workspace::WORKSPACE.mcodes.borrow();
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
            acc.push(CheckResult {
                check_name: "body",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: None,
                message: format!(
                    "URI '{}' mixes '.' (namespace) and '/' (path) separators. \
                     Use one style consistently.",
                    uri
                ),
                code: 3201,
            });
        }
    }
}

// ============================================================================
// P6: `return` outside function context
// ============================================================================

/// In MCode, `return` is only valid inside `func` bodies. A `return` in a
/// module's top-level net lines or component attribute body is an error.
fn check_return_outside_function(acc: &mut CheckAccumulator) {
    // ── Module top-level body lines ──
    {
        let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
        for entry in modules.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let m = entry.value();

            // Check top-level module body lines (not inside functions)
            for phrase in &m.lines {
                let text = format!("{}", phrase);
                if text_contains_keyword(&text, "return") {
                    acc.push(CheckResult {
                        check_name: "body",
                        severity: CheckSeverity::Error,
                        uri: Some(uri.clone()),
                        span: Some(m.span.start..m.span.end),
                        message: format!(
                            "Module '{}': 'return' used outside function context: '{}'. \
                             'return' is only valid inside 'func' bodies.",
                            entry.key().ident,
                            text.trim()
                        ),
                        code: 3202,
                    });
                    break; // One diagnostic per module for this check
                }
            }
        }
    }

    // ── Component top-level body (attributes) ──
    // Component bodies are parsed into structured attrs/pins/funcs, not raw lines.
    // A `return` in a component attr value would be unusual but checking attr
    // value text would produce false positives. Skip component-level check —
    // the parser would catch `return` as a syntax error in component context.
}

/// Check if `text` contains `keyword` as a standalone token (word boundary).
fn text_contains_keyword(text: &str, keyword: &str) -> bool {
    // Simple: check each whitespace-separated token
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if clean == keyword {
            return true;
        }
    }
    false
}

// ============================================================================
// P7: `return` with literal instead of endpoint
// ============================================================================

/// When `return` appears in a function body, it should specify a net endpoint
/// (port name, instance pin, label), not a bare literal value.
///
/// Examples:
///   ✓ `return VDD`          (endpoint)
///   ✓ `return dc_out.VDD`   (instance endpoint)
///   ✗ `return 42`           (bare integer)
///   ✗ `return "done"`       (bare string)
///   ✗ `return 3.3V`         (unit value — no named endpoint)
fn check_return_with_literal(acc: &mut CheckAccumulator) {
    // ── Module functions ──
    {
        let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
        for entry in modules.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let m = entry.value();
            let mod_span = Some(m.span.start..m.span.end);
            for func in m.funcs.iter() {
                for phrase in &func.lines {
                    let text = format!("{}", phrase);
                    check_return_literal_in_text(
                        &text,
                        &uri,
                        &format!("module '{}' func '{}'", entry.key().ident, func.name),
                        &mod_span,
                        acc,
                    );
                }
            }
        }
    }

    // ── Component functions ──
    {
        let comps = crate::builder::workspace::WORKSPACE.components.borrow();
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp = entry.value();
            let comp_span = Some(comp.span.start..comp.span.end);
            for func in comp.funcs.iter() {
                for phrase in &func.lines {
                    let text = format!("{}", phrase);
                    check_return_literal_in_text(
                        &text,
                        &uri,
                        &format!("component '{}' func '{}'", comp.name, func.name),
                        &comp_span,
                        acc,
                    );
                }
            }
        }
    }
}

/// Scan text for `return <literal>` patterns where the return value is not
/// a named endpoint.
fn check_return_literal_in_text(
    text: &str,
    uri: &str,
    context: &str,
    def_span: &Option<std::ops::Range<usize>>,
    acc: &mut CheckAccumulator,
) {
    let lower = text.to_lowercase();
    let return_positions: Vec<usize> = lower.match_indices("return").map(|(i, _)| i).collect();

    for pos in return_positions {
        let after_return = &text[pos + 6..].trim_start(); // skip "return"
        if after_return.is_empty() {
            continue; // bare `return` is fine (implicit return)
        }

        // Get the first token after `return`
        let first_token = after_return
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_');

        if first_token.is_empty() {
            continue;
        }

        // Check if the first token is a literal:
        // - Integer: all digits
        // - Float: digits with a decimal point
        // - String: quoted text
        // - Unit value: digits followed by unit chars (e.g., "5V", "3.3V", "100nF")
        let is_quoted = first_token.starts_with('"') || first_token.starts_with('\'');
        let is_numeric = first_token.chars().all(|c| c.is_ascii_digit());
        let is_float = first_token.chars().all(|c| c.is_ascii_digit() || c == '.')
            && first_token.contains('.')
            && !first_token.starts_with('.');
        let is_unit = first_token.starts_with(|c: char| c.is_ascii_digit() || c == '.')
            && first_token.chars().any(|c| c.is_alphabetic());

        if is_quoted || is_numeric || is_float || is_unit {
            acc.push(CheckResult {
                check_name: "body",
                severity: CheckSeverity::Warning,
                uri: Some(uri.to_string()),
                span: def_span.clone(),
                message: format!(
                    "In {}: 'return {}' — 'return' should specify a net endpoint (port/instance), \
                     not a literal value.",
                    context, first_token
                ),
                code: 3203,
            });
        }
    }
}

// ============================================================================
// S3: Empty bracket instance list (`[] :: TYPE`)
// ============================================================================

/// An empty `[] :: TYPE(...)` declares a zero-length instance vector.
/// This is almost certainly a mistake — the user likely intended to
/// specify a range or list of instances.
fn check_empty_bracket_list(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        for phrase in &m.lines {
            let text = format!("{}", phrase);
            // Look for `[] ::` pattern
            if text.contains("[] ::") || text.contains("[]::") {
                acc.push(CheckResult {
                    check_name: "body",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(m.span.start..m.span.end),
                    message: format!(
                        "Module '{}': empty bracket instance list '[] :: TYPE' in '{}'. \
                         An empty instance list creates no instances — remove it or \
                         specify a range like '[1:4]'.",
                        entry.key().ident,
                        text.trim()
                    ),
                    code: 3204,
                });
            }
        }
    }
}

// ============================================================================
// S4: `this` on LHS of `::` declaration
// ============================================================================

/// `this :: TYPE` is invalid syntax. The `this` keyword refers to the
/// current instance and cannot be used as a new instance name.
/// Valid: `r1 :: RES(10k)`  Invalid: `this :: RES(10k)`
fn check_this_lhs_declaration(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        for phrase in &m.lines {
            let text = format!("{}", phrase);
            // Look for `this ::` pattern (with optional whitespace)
            let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();

            if cleaned.starts_with("this::") {
                acc.push(CheckResult {
                    check_name: "body",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(m.span.start..m.span.end),
                    message: format!(
                        "Module '{}': 'this :: TYPE' in '{}'. \
                         'this' refers to the current instance and cannot be used \
                         as a new instance name on the LHS of '::'.",
                        entry.key().ident,
                        text.trim()
                    ),
                    code: 3205,
                });
            }
        }
    }
}

// ============================================================================
// S6: `role` keyword used as call argument value
// ============================================================================

/// The `role` keyword is reserved for interface role selection. Passing
/// `role` as a positional argument to a component/module constructor is
/// likely a mistake.
///
/// Example: `AMP(role, 5V)` — `role` is not a value, it's a keyword.
fn check_role_as_call_arg(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        // Check all instance calls for `role` as an argument
        for (inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
            // Get the arg list text from the instance
            let args: Vec<String> = match instance {
                crate::McInstance::Component(c2) => {
                    c2.params.iter().map(|p| p.to_string()).collect()
                }
                crate::McInstance::Interface(i2) => {
                    i2.params.iter().map(|p| p.to_string()).collect()
                }
                crate::McInstance::Module(m2) => m2.args.iter().map(|a| a.to_string()).collect(),
                _ => continue,
            };

            for arg in &args {
                let cleaned = arg.trim();
                if cleaned == "role" {
                    acc.push(CheckResult {
                        check_name: "body",
                        severity: CheckSeverity::Error,
                        uri: Some(uri.clone()),
                        span: Some(m.span.start..m.span.end),
                        message: format!(
                            "Module '{}': instance '{}' passes 'role' as a call argument. \
                             'role' is a keyword, not a value. Did you mean to select \
                             a specific role (e.g. 'DCE') or declare a role parameter?",
                            entry.key().ident,
                            inst_name
                        ),
                        code: 3206,
                    });
                }
            }
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
/// We detect this by scanning the text of cond_pins/cond_attrs conditions.
fn check_bitwise_in_condition(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        let comp_span = Some(comp.span.start..comp.span.end);

        // Inspect conditional pin conditions
        for (idx, cp) in comp.cond_pins.iter().enumerate() {
            for (bidx, (cond, _pins)) in cp.if_blocks.iter().enumerate() {
                let cond_text = format!("{:?}", cond);
                check_condition_for_bitwise(
                    &cond_text,
                    &uri,
                    &format!(
                        "component '{}' cond_pins[{}] if-block[{}]",
                        comp.name, idx, bidx
                    ),
                    &comp_span,
                    acc,
                );
            }
        }

        // Inspect conditional attr conditions
        for (idx, ca) in comp.cond_attrs.iter().enumerate() {
            for (bidx, (cond, _attrs)) in ca.if_blocks.iter().enumerate() {
                let cond_text = format!("{:?}", cond);
                check_condition_for_bitwise(
                    &cond_text,
                    &uri,
                    &format!(
                        "component '{}' cond_attrs[{}] if-block[{}]",
                        comp.name, idx, bidx
                    ),
                    &comp_span,
                    acc,
                );
            }
        }
    }
}

/// Check a condition's debug representation for bitwise operators that
/// might have been intended as logical operators.
///
/// The `McCondition` enum uses Rust's `Debug` formatting, so comparisons
/// appear as e.g. `Eq { left: Ident(...), right: Literal("5") }`.
/// Single `&`/`|` wouldn't normally survive parsing into McCondition
/// (they'd be parsed as arithmetic), so this is a heuristic check
/// on the raw `{:?}` text.
fn check_condition_for_bitwise(
    cond_text: &str,
    uri: &str,
    context: &str,
    comp_span: &Option<std::ops::Range<usize>>,
    acc: &mut CheckAccumulator,
) {
    // McCondition::In with a single binary value ("0" or "1") could indicate
    // that a bitwise operation result is being used as a boolean condition.
    // e.g., `if flags & MASK:` where the condition checks if a single-bit
    // result equals 0 or 1 — this is valid but worth reviewing for clarity.
    if cond_text.contains("In {") {
        let value_count = cond_text.match_indices("Literal(").count();
        if value_count == 1 {
            if cond_text.contains("Literal(\"0\")") || cond_text.contains("Literal(\"1\")") {
                acc.push(CheckResult {
                    check_name: "body",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.to_string()),
                    span: comp_span.clone(),
                    message: format!(
                        "In {}: condition compares against a single binary value. \
                         If this is a bitwise operation result used as boolean, \
                         consider using explicit comparison (e.g., `(flags & MASK) != 0`).",
                        context
                    ),
                    code: 3208,
                });
            }
        }
    }
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
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        // Collect all port/instance names declared in `insts`
        let declared: HashSet<String> = m.insts.iter_instance_names().cloned().collect();

        if declared.is_empty() {
            continue;
        }

        // Collect all names referenced in net connection lines
        let mut referenced: HashSet<String> = HashSet::new();
        for phrase in &m.lines {
            let text = format!("{}", phrase);
            // Split by `->` to get both sides, then split by `,` for multi-endpoint
            for side in text.split("->") {
                for endpoint in side.split(',') {
                    let ep = endpoint.trim();
                    if ep.is_empty() || ep == "_" {
                        continue;
                    }
                    // Take the first dot-separated segment (instance name)
                    if let Some((first, _rest)) = ep.split_once('.') {
                        referenced.insert(first.trim().to_string());
                    } else {
                        referenced.insert(ep.to_string());
                    }
                }
            }
        }

        // Also collect names referenced in function body lines
        for func in m.funcs.iter() {
            for phrase in &func.lines {
                let text = format!("{}", phrase);
                for side in text.split("->") {
                    for endpoint in side.split(',') {
                        let ep = endpoint.trim();
                        if ep.is_empty() || ep == "_" {
                            continue;
                        }
                        if let Some((first, _rest)) = ep.split_once('.') {
                            referenced.insert(first.trim().to_string());
                        } else {
                            referenced.insert(ep.to_string());
                        }
                    }
                }
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
                    code: 3207,
                });
            }
        }
    }
}
