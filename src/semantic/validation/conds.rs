// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Conditional block validation (component-level `if`/`else`).
//!
//! Checks:
//!   T3 — empty conditional body (if-block with no pins/attrs)
//!   T4 — conditional without else coverage (missing else branch)
//!   O3 — IO type on component pin (context-dependent warning)
//!   O4 — `|` pin alternatives producing potentially conflicting net roles

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};

pub struct CondsCheck;

impl ValidationCheck for CondsCheck {
    fn name(&self) -> &'static str {
        "conds"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        check_empty_cond_body(acc); // T3
        check_missing_else(acc); // T4
        check_pin_io_context(acc); // O3
        check_pin_alt_roles(acc); // O4
        check_param_pin_name_collision(acc); // cross-CMIE
        check_empty_module(acc); // M6-extended
    }
}

// ============================================================================
// T3: Empty conditional body
// ============================================================================

/// An `if` block whose body contains no pins and no attributes is likely
/// an oversight — the condition selects nothing.
fn check_empty_cond_body(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // ── Conditional pins ──
        for (idx, cp) in comp.cond_pins.iter().enumerate() {
            for (bidx, (cond, pins)) in cp.if_blocks.iter().enumerate() {
                if pins.names_to_id.is_empty() {
                    acc.push(CheckResult {
                        check_name: "conds",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
                        message: format!(
                            "Component '{}': cond_pins[{}] if-block[{}] (cond={:?}) has an empty body. \
                             The condition selects no pins.",
                            comp.name, idx, bidx, cond
                        ),
                        code: 3001,
                    });
                }
            }
            if let Some(ref else_pins) = cp.else_pins {
                if else_pins.names_to_id.is_empty() && !cp.if_blocks.is_empty() {
                    acc.push(CheckResult {
                        check_name: "conds",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
                        message: format!(
                            "Component '{}': cond_pins[{}] else-block has an empty body. \
                             No pins selected for the default case.",
                            comp.name, idx
                        ),
                        code: 3001,
                    });
                }
            }
        }

        // ── Conditional attributes ──
        for (idx, ca) in comp.cond_attrs.iter().enumerate() {
            for (bidx, (cond, attrs)) in ca.if_blocks.iter().enumerate() {
                if attrs.is_empty() {
                    acc.push(CheckResult {
                        check_name: "conds",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
                        message: format!(
                            "Component '{}': cond_attrs[{}] if-block[{}] (cond={:?}) has an empty body. \
                             The condition selects no attributes.",
                            comp.name, idx, bidx, cond
                        ),
                        code: 3001,
                    });
                }
            }
            if let Some(ref else_attrs) = ca.else_attrs {
                if else_attrs.is_empty() && !ca.if_blocks.is_empty() {
                    acc.push(CheckResult {
                        check_name: "conds",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
                        message: format!(
                            "Component '{}': cond_attrs[{}] else-block has an empty body. \
                             No attributes selected for the default case.",
                            comp.name, idx
                        ),
                        code: 3001,
                    });
                }
            }
        }
    }
}

// ============================================================================
// T4: Conditional without else coverage
// ============================================================================

/// A conditional with `if` branches but no `else` may leave pins/attrs
/// undefined for some parameter value combinations.
fn check_missing_else(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        for (idx, cp) in comp.cond_pins.iter().enumerate() {
            if !cp.if_blocks.is_empty() && cp.else_pins.is_none() {
                acc.push(CheckResult {
                    check_name: "conds",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}': cond_pins[{}] has {} if-block(s) but no else block. \
                         Pins may be undefined for uncovered parameter values.",
                        comp.name,
                        idx,
                        cp.if_blocks.len()
                    ),
                    code: 3002,
                });
            }
        }

        for (idx, ca) in comp.cond_attrs.iter().enumerate() {
            if !ca.if_blocks.is_empty() && ca.else_attrs.is_none() {
                acc.push(CheckResult {
                    check_name: "conds",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}': cond_attrs[{}] has {} if-block(s) but no else block. \
                         Attributes may be undefined for uncovered parameter values.",
                        comp.name,
                        idx,
                        ca.if_blocks.len()
                    ),
                    code: 3002,
                });
            }
        }
    }
}

// ============================================================================
// O3: IO type on component pin (context-dependent)
// ============================================================================

/// Component pin definitions with IO types deserve scrutiny:
///   - `nc` (not-connected) on a component pin is unusual (typically on instances)
///   - `ps` (power supply) without associated voltage attribute
/// Locate the source span of a pin's name within its definition line.
///
/// First finds the pin definition line by searching for `keyword [...pin_id...]`,
/// then narrows to the specific `pin_name` within the names bracket `[...,name,...]`.
/// Falls back to the line's keyword span, then to the component span.
fn pin_definition_span(comp: &crate::semantic::component::McComponent, pin_id: &str, pin_name: Option<&str>) -> std::ops::Range<usize> {
    if let Ok(content) = std::fs::read_to_string(comp.uri.as_str()) {
        for keyword in &["ps ", "in ", "io ", "out ", "anl ", "nc "] {
            let mut search_from = 0;
            while let Some(kw_pos) = content[search_from..].find(keyword) {
                let line_start = search_from + kw_pos;
                let line_end_pos = content[line_start..].find('\n').unwrap_or(content.len() - line_start);
                let line = &content[line_start..line_start + line_end_pos];
                // Find the first bracket pair (pin IDs like [5,21]) and check for our pin_id
                if let (Some(bs), Some(be)) = (line.find('['), line.find(']')) {
                    if bs < be {
                        let id_tokens: Vec<&str> = line[bs + 1..be]
                            .split(&[',', ' ', ':'][..])
                            .filter(|s| !s.is_empty())
                            .collect();
                        if id_tokens.contains(&pin_id) {
                            // Try to narrow to the specific pin name
                            if let Some(name) = pin_name {
                                // Find the names bracket (second [...] in the line)
                                if let Some(rest) = line.get(be + 1..) {
                                    if let (Some(nbs), Some(nbe)) = (rest.find('['), rest.find(']')) {
                                        let names_bracket = &rest[nbs + 1..nbe];
                                        // Find the exact position of this name within the names bracket
                                        let name_tokens: Vec<&str> = names_bracket
                                            .split(&[',', ' '][..])
                                            .filter(|s| !s.is_empty())
                                            .collect();
                                        if name_tokens.contains(&name) {
                                            // Compute absolute position of the name within the file
                                            let name_pos_in_rest = names_bracket.find(name)
                                                .unwrap_or(0);
                                            let abs_name_pos = line_start + be + 1 + nbs + 1 + name_pos_in_rest;
                                            return abs_name_pos..abs_name_pos + name.len();
                                        }
                                    }
                                }
                            }
                            // Fallback: span of the keyword
                            return line_start..line_start + keyword.trim().len();
                        }
                    }
                }
                search_from = line_start + 1;
            }
        }
    }
    // Ultimate fallback
    comp.span.start..comp.span.end
}

fn check_pin_io_context(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Iterate all pins (keyed by pin ID) to check IO types
        for (pin_id, pin) in &comp.pins.pins {
            use crate::IOType;
            let pin_span = pin_definition_span(comp, pin_id, pin.names.first().map(|s| s.as_str()));
            match pin.iotype {
                IOType::NonCon => {
                    let names = if pin.names.is_empty() {
                        pin_id.clone()
                    } else {
                        pin.names.join(", ")
                    };
                    acc.push(CheckResult {
                        check_name: "conds",
                        severity: CheckSeverity::Info,
                        uri: Some(uri.clone()),
                        span: Some(pin_span.clone()),
                        message: format!(
                            "Component '{}': pin '{}' ({}) is declared NC (not-connected) at \
                             the component level. NC is typically used at instantiation.",
                            comp.name, names, pin_id
                        ),
                        code: 3003,
                    });
                }
                IOType::Power => {
                    let names = if pin.names.is_empty() {
                        pin_id.clone()
                    } else {
                        pin.names.join(", ")
                    };
                    // Check for voltage-related attribute
                    let has_voltage_attr = comp.attrs.iter().any(|a| {
                        let key = a.id.to_string();
                        key.contains("voltage") || key.contains("volt") || key == "vcc"
                    });
                    if !has_voltage_attr {
                        acc.push(CheckResult {
                            check_name: "conds",
                            severity: CheckSeverity::Info,
                            uri: Some(uri.clone()),
                            span: Some(pin_span.clone()),
                            message: format!(
                                "Component '{}': power pin '{}' ({}) has no associated \
                                 voltage attribute. Consider adding e.g. `voltage = \"5V\"`.",
                                comp.name, names, pin_id
                            ),
                            code: 3004,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

// ============================================================================
// O4: `|` pin alternatives producing conflicting net roles
// ============================================================================

/// When multiple pin IDs share the same name (via `McPinPort::Multi`),
/// check whether their IO types are in conflict.
fn check_pin_alt_roles(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // For each named port, check if it maps to multiple pin IDs with conflicting IO types
        for (pin_name, port) in &comp.pins.names_to_id {
            let pin_ids: Vec<&String> = match port {
                crate::semantic::component::mc_pins::McPinPort::Single(id) => vec![id],
                crate::semantic::component::mc_pins::McPinPort::Multi(ids) => ids.iter().collect(),
                _ => continue,
            };

            if pin_ids.len() < 2 {
                continue;
            }

            // Collect IO types for these pin IDs
            use crate::IOType;
            let mut io_types: Vec<&IOType> = Vec::new();
            for pid in &pin_ids {
                if let Some(pin) = comp.pins.pins.get(*pid) {
                    io_types.push(&pin.iotype);
                }
            }

            let has_in = io_types.iter().any(|t| matches!(t, IOType::In));
            let has_out = io_types.iter().any(|t| matches!(t, IOType::Out));
            let has_ps = io_types.iter().any(|t| matches!(t, IOType::Power));
            let has_anl = io_types.iter().any(|t| matches!(t, IOType::Analog));

            // in + out → consider using InOut
            if has_in && has_out {
                acc.push(CheckResult {
                    check_name: "conds",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}': pin name '{}' maps to pins with both In and Out \
                         IO types. Consider using 'io' (InOut) for bidirectional pins.",
                        comp.name, pin_name
                    ),
                    code: 3005,
                });
            }

            // out + ps → potential backfeed risk
            if has_out && has_ps {
                acc.push(CheckResult {
                    check_name: "conds",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}': pin name '{}' maps to pins with both Output and Power \
                         IO types. This may create backfeed risk on the connected net.",
                        comp.name, pin_name
                    ),
                    code: 3006,
                });
            }

            // anl + ps → unusual combination
            if has_anl && has_ps {
                acc.push(CheckResult {
                    check_name: "conds",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}': pin name '{}' maps to pins with both Analog and Power \
                         IO types. Verify this is the intended behavior.",
                        comp.name, pin_name
                    ),
                    code: 3007,
                });
            }
        }
    }
}

// ============================================================================
// Cross-CMIE: Param-pin name collision in components
// ============================================================================

/// A component parameter sharing a name with a pin is confusing —
/// the same identifier means two different things in different contexts.
fn check_param_pin_name_collision(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Build set of pin names
        let pin_names: std::collections::HashSet<String> =
            comp.pins.names_to_id.keys().cloned().collect();

        for d in comp.params.iter() {
            if let Some(pname) = d.get_primary_name() {
                if pin_names.contains(&pname) {
                    acc.push(CheckResult {
                        check_name: "conds",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
                        message: format!(
                            "Component '{}': param '{}' shares a name with a pin. \
                             This may cause confusion in net expressions.",
                            comp.name, pname
                        ),
                        code: 3008,
                    });
                }
            }
        }
    }
}

// ============================================================================
// M6-extended: Completely empty module (no params, insts, lines, funcs)
// ============================================================================

/// A module with no content at all is almost certainly a stub or mistake.
fn check_empty_module(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let has_params = !m.params.is_empty();
        let has_insts = !m.insts.is_empty();
        let has_lines = !m.lines.is_empty();
        let has_funcs = !m.funcs.is_empty();

        if !has_params && !has_insts && !has_lines && !has_funcs {
            acc.push(CheckResult {
                check_name: "conds",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: Some(m.span.start..m.span.end),
                message: format!(
                    "Module '{}' has no params, instances, net lines, or functions. \
                     Is this a stub?",
                    entry.key().ident
                ),
                code: 3009,
            });
        }
    }
}
