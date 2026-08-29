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

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
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
                if !pins.has_any_pins() {
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
                        code: crate::errcodes::COND_EMPTY_BODY,
                    });
                }
            }
            if let Some(ref else_pins) = cp.else_pins {
                if !else_pins.has_any_pins() && !cp.if_blocks.is_empty() {
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
                        code: crate::errcodes::COND_EMPTY_BODY,
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
                        code: crate::errcodes::COND_EMPTY_BODY,
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
                        code: crate::errcodes::COND_EMPTY_BODY,
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
                    code: crate::errcodes::COND_IF_WITHOUT_ELSE,
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
                    code: crate::errcodes::COND_IF_WITHOUT_ELSE,
                });
            }
        }
    }
}

// ============================================================================
// O3: IO type on component pin (context-dependent)
// ============================================================================

/// Find the `[`/`]` span of the first bracket group in `s`, honoring nested
/// brackets (e.g. `[1,[2,8]]`): the returned close is the matching bracket
/// for the first `[`, so the pair always satisfies `open < close`.
fn first_bracket_span(s: &str) -> Option<(usize, usize)> {
    let bs = s.find('[')?;
    let mut depth = 0usize;
    for (i, c) in s[bs..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((bs, bs + i));
                }
            }
            _ => {}
        }
    }
    None
}

/// Component pin definitions with IO types deserve scrutiny:
///   - `nc` (not-connected) on a component pin is unusual (typically on instances)
///   - `ps` (power supply) without associated voltage attribute
/// Locate the source span of a pin's name within its definition line.
///
/// First finds the pin definition line by searching for `keyword [...pin_id...]`,
/// then narrows to the specific `pin_name` within the names bracket `[...,name,...]`.
/// Falls back to the line's keyword span, then to the component span.
fn pin_definition_span(
    comp: &crate::semantic::component::McComponent,
    pin_id: &str,
    pin_name: Option<&str>,
) -> std::ops::Range<usize> {
    // Prefer the exact pin-name span recorded at parse time. This covers
    // single pins like `ps 0 = EPAD, "..."`, which the bracket-based text
    // search below cannot narrow (no `[...]` group on the line) and would
    // otherwise fall back to the component name span.
    if let Some(name) = pin_name {
        if let Some(span) = comp.pins.pin_name_spans.get(name) {
            if span.end > span.start {
                return span.clone();
            }
        }
    }
    if let Ok(content) = std::fs::read_to_string(comp.uri.as_str()) {
        for keyword in &["ps ", "in ", "io ", "out ", "anl ", "nc "] {
            let mut search_from = 0;
            while let Some(kw_pos) = content[search_from..].find(keyword) {
                let line_start = search_from + kw_pos;
                let line_end_pos = content[line_start..]
                    .find('\n')
                    .unwrap_or(content.len() - line_start);
                let line = &content[line_start..line_start + line_end_pos];
                // Find the pin-id bracket group (e.g. [5,21] or nested
                // [1,[2,8]]) and check for our pin_id.
                if let Some((bs, be)) = first_bracket_span(line) {
                    if bs < be {
                        // Strip bracket chars so grouped ids like "[2" match.
                        let id_tokens: Vec<&str> = line[bs + 1..be]
                            .split(&[',', ' ', ':'][..])
                            .filter(|s| !s.is_empty())
                            .map(|s| s.trim_matches(|c| c == '[' || c == ']'))
                            .collect();
                        if id_tokens.contains(&pin_id) {
                            // Try to narrow to the specific pin name
                            if let Some(name) = pin_name {
                                // Find the names bracket (second [...] in the line)
                                if let Some(rest) = line.get(be + 1..) {
                                    if let Some((nbs, nbe)) = first_bracket_span(rest) {
                                        let names_bracket = &rest[nbs + 1..nbe];
                                        // Find the exact position of this name within the names bracket
                                        let name_tokens: Vec<&str> = names_bracket
                                            .split(&[',', ' '][..])
                                            .filter(|s| !s.is_empty())
                                            .collect();
                                        if name_tokens.contains(&name) {
                                            // Compute absolute position of the name within the file
                                            let name_pos_in_rest =
                                                names_bracket.find(name).unwrap_or(0);
                                            let abs_name_pos =
                                                line_start + be + 1 + nbs + 1 + name_pos_in_rest;
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
            let pin_span = pin_definition_span(comp, pin_id, pin.names.first().map(|s| s.as_str()));
            // §2.19 OR semantics: a pin is NC if its iotype is NonCon (`nc`
            // prefix) or any name is "NC"/"nc" — either declaration marks it.
            if pin.is_nc {
                // A pin whose *name* is literally "NC"/"nc"
                // (`io [1, 3, ...] = NC, "No connect"`) is the idiomatic
                // no-connect declaration at the component level — deliberate,
                // not a mistake, so no warning. The pin is already registered
                // as NC (`is_nc` set at parse) and excluded from net/voltage
                // checks downstream. Only NC coming from the explicit `nc`
                // iotype keyword deserves scrutiny here.
                let named_nc = pin.names.iter().any(|n| n.eq_ignore_ascii_case("nc"));
                if !named_nc {
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
                        code: crate::errcodes::PIN_NC_COMPONENT_LEVEL,
                    });
                }
                continue;
            }
            // Power-pin voltage checking lives in the hw pass
            // (check_power_pin_no_voltage, E5454): one rule covers both
            // power-typed and power-named pins, so there is no per-pin branch here.
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
                    code: crate::errcodes::PIN_IO_MIX_IN_OUT,
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
                    code: crate::errcodes::PIN_IO_MIX_OUTPUT_POWER,
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
                    code: crate::errcodes::PIN_IO_MIX_ANALOG_POWER,
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
                        code: crate::errcodes::PARAM_PIN_NAME_SHADOW,
                    });
                }
            }
        }
    }
}

// ============================================================================
// M6-extended: Completely empty module (no params, insts, stmts, funcs)
// ============================================================================

/// A module with no content at all is almost certainly a stub or mistake.
fn check_empty_module(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let name = entry.key().ident.to_string();
        // `module VIRT_<T>` wrappers fabricated by virtual instantiation are
        // allowed to be empty — an interface wrapper carries no instance (its
        // pins render as boundary ports, or are absent entirely when dynamic),
        // and it is never user code. Skip them so building an interface-only
        // library file does not report the fabricated module as a stub.
        if crate::build::virtual_inst::is_synthetic_module(&name) {
            continue;
        }
        let m = entry.value();
        let has_params = !m.params.is_empty();
        let has_insts = !m.insts.is_empty();
        let has_stmts = !m.stmts.is_empty();
        let has_funcs = !m.funcs.is_empty();

        if !has_params && !has_insts && !has_stmts && !has_funcs {
            acc.push(CheckResult {
                check_name: "conds",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: Some(m.span.start..m.span.end),
                message: format!(
                    "Module '{}' has no params, instances, net statements, or functions. \
                     Is this a stub?",
                    name
                ),
                code: crate::errcodes::MODULE_STUB,
            });
        }
    }
}
