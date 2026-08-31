// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Instance, role, function-param, and default-value validation.
//!
//! Checks:
//!   S1 — instance param count/type mismatch vs definition
//!   R1 — role with empty body
//!   R2 — role name conflicts with interface port/param
//!   R6 — IO type direction on function parameter declaration
//!   R7 — `role` keyword as param in component/module (non-interface)
//!   R9 — non-constant / expression-like default value

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::HashSet;

pub struct InstsCheck;

impl ValidationCheck for InstsCheck {
    fn name(&self) -> &'static str {
        "insts"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_instance_param_mismatch(acc); // S1
        check_role_empty_body(acc); // R1
        check_role_name_conflict(acc); // R2
        check_func_param_iotype(acc); // R6
        check_role_param_outside_interface(acc); // R7
        check_non_constant_default(acc); // R9
    }
}

// ============================================================================
// S1: Instance param count/type mismatch vs definition
// ============================================================================

/// For each module, check that Component/Module/Interface instance constructor
/// args match the definition's parameter arity.
fn check_instance_param_mismatch(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, m) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // Walk each instance in the module's symbol table
        for (inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
            let span = instance_span(m, inst_name);
            match instance {
                crate::McInstance::Component(c2) => {
                    let class_name = c2.base.name.to_string();
                    // ★ §P1 C6: a same-name constructor func declares the actual
                    // constructor arity. `comp.sub flash(V3V3)` binds its arg
                    // to `func sub([V3V3, GND]::DC(3.3V))` inside the component,
                    // not to the (empty) header param list. Fall back to the header
                    // params when no such func exists.
                    let ctor = class_name
                        .rsplit('.')
                        .next()
                        .and_then(|last| c2.base.funcs.find(last));
                    let declared = match ctor {
                        Some(f) => &f.params,
                        None => &c2.base.params,
                    };
                    let def_param_count = declared.len();
                    // Strip NC modifiers from the call arg count
                    let call_arg_count = c2
                        .params
                        .iter()
                        .filter(|p| {
                            !matches!(p, crate::semantic::basic::mc_param::McParamValue::NC(_))
                        })
                        .count();

                    if call_arg_count > def_param_count {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span,
                            message: format!(
                                "Instance '{}' of component '{}' passes {} args, but '{}' declares {} param(s).",
                                inst_name, class_name, call_arg_count, class_name, def_param_count
                            ),
                            code: crate::errcodes::INST_ARG_COUNT_MISMATCH,
                        });
                    } else if call_arg_count < def_param_count {
                        // Count required: only params that have NO unit type AND NO default value.
                        let required = declared
                            .iter()
                            .filter(|d| !d.has_unit_type() && !d.has_default_value())
                            .count();
                        // Missing required args never block instance creation
                        // (Component-Spec Separation): silent in dev mode,
                        // reported as a warning (E5352) in strict mode.
                        if call_arg_count < required && crate::cli::strict_mode() {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span,
                                message: format!(
                                    "Instance '{}' of component '{}' passes {} args, but '{}' requires at least {} ({} total, {} optional).",
                                    inst_name, class_name, call_arg_count, class_name, required,
                                    def_param_count, def_param_count - required
                                ),
                                code: crate::errcodes::INST_ARG_COUNT_MISMATCH,
                            });
                        }
                    }
                }
                crate::McInstance::Module(m2) => {
                    let class_name = m2.base.name.to_string();
                    // ★ DC interface params ([VDD_3V3,GND]::DC(3.3V)) ARE the
                    // constructor args bound by position (`mod.sub mcu(V3V3, V1V2)`
                    // per §P1 C4), so they must count toward the declared arity.
                    // Only pure ports (Label / Idx / ComponentInstance — e.g. the
                    // `in signal, ps ground` kind of header entry) are excluded.
                    let module_params: Vec<_> = m2
                        .base
                        .params
                        .iter()
                        .filter(|d| {
                            !matches!(
                                d.param_type.kind,
                                crate::semantic::basic::mc_param_type::McParamTypeKind::Label
                                    | crate::semantic::basic::mc_param_type::McParamTypeKind::Idx
                                    | crate::semantic::basic::mc_param_type::McParamTypeKind::ComponentInstance { .. }
                            )
                        })
                        .collect();
                    let def_param_count = module_params.len();
                    let call_arg_count = m2.args.len();

                    if call_arg_count > def_param_count {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span,
                            message: format!(
                                "Instance '{}' of module '{}' passes {} args, but '{}' declares {} param(s).",
                                inst_name, class_name, call_arg_count, class_name, def_param_count
                            ),
                            code: crate::errcodes::INST_ARG_COUNT_MISMATCH,
                        });
                    } else if call_arg_count < def_param_count {
                        // Required = non-port data params without a default.
                        // Interface/InterfaceWithRole DC params are ports: they may
                        // be left unbound here and supplied later via a constructor
                        // funcall (`mic(V3V3)`) or a net line (`V3V3 -> mic.dc`),
                        // so they never make the arg count "required".
                        let required = module_params
                            .iter()
                            .filter(|d| !d.is_port() && !d.has_default_value())
                            .count();
                        // Missing required args never block instance creation
                        // (Component-Spec Separation): silent in dev mode,
                        // reported as a warning (E5352) in strict mode.
                        if call_arg_count < required && crate::cli::strict_mode() {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span,
                                message: format!(
                                    "Instance '{}' of module '{}' passes {} args, but '{}' requires at least {} ({} total, {} optional).",
                                    inst_name, class_name, call_arg_count, class_name, required,
                                    def_param_count, def_param_count - required
                                ),
                                code: crate::errcodes::INST_ARG_COUNT_MISMATCH,
                            });
                        }
                    }
                }
                crate::McInstance::Interface(i2) => {
                    // Skip anonymous-bus / label-list instances.
                    //   [VDD_3V3,GND]::DC(3.3V)
                    // is a declaration-site port binding where the bracket
                    // members (VDD_3V3, GND) are module port labels, not
                    // constructor args.  The value (3.3V) lives inside the
                    // ::DC(…) type annotation — param count doesn't apply.
                    if inst_name.starts_with('[') {
                        continue;
                    }
                    let class_name = i2.base.name.to_string();
                    let def_param_count = i2.base.params.len();
                    let call_arg_count = i2.params.len();

                    if call_arg_count > def_param_count {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span,
                            message: format!(
                                "Instance '{}' of interface '{}' passes {} args, but '{}' declares {} param(s).",
                                inst_name, class_name, call_arg_count, class_name, def_param_count
                            ),
                            code: crate::errcodes::INST_ARG_COUNT_MISMATCH,
                        });
                    } else if call_arg_count < def_param_count {
                        let required = i2
                            .base
                            .params
                            .iter()
                            .filter(|d| !d.has_default_value())
                            .count();
                        // Missing required args never block instance creation
                        // (Component-Spec Separation): silent in dev mode,
                        // reported as a warning (E5352) in strict mode.
                        if call_arg_count < required && crate::cli::strict_mode() {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span,
                                message: format!(
                                    "Instance '{}' of interface '{}' passes {} args, but '{}' requires at least {} ({} total, {} optional).",
                                    inst_name, class_name, call_arg_count, class_name, required,
                                    def_param_count, def_param_count - required
                                ),
                                code: crate::errcodes::INST_ARG_COUNT_MISMATCH,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Look up the declaration span for an instance name within a module.
/// Falls back to the module's start span when no specific span is recorded
/// (e.g. for anonymous or synthesized instances).
fn instance_span(m: &crate::McModule, inst_name: &str) -> Option<std::ops::Range<usize>> {
    if let Some(spans) = m.insts.port_spans().get(inst_name) {
        if let Some(s) = spans.first() {
            return Some(s.clone());
        }
    }
    Some(m.span.start..m.span.end)
}

// ============================================================================
// R1: Role with empty body
// ============================================================================

/// Interface roles that have no pins, no attrs, and no body content.
fn check_role_empty_body(acc: &mut CheckAccumulator) {
    let ifaces = crate::definition_space().workspace_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for role in &iface.roles {
            let has_pins = !role.pins.names_to_id.is_empty();
            let has_attrs = !role.attrs.is_empty();
            // Check if the body AST has any children beyond the default node
            let has_body = role
                .body
                .get_sub_node()
                .map_or(false, |sub| sub.iter().next().is_some());

            if !has_pins && !has_attrs && !has_body {
                acc.push(CheckResult {
                    check_name: "insts",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(iface.span.start..iface.span.end),
                    message: format!(
                        "Role '{}' in interface '{}' has an empty body (no pins, attrs, or clauses).",
                        role.name, iface.name
                    ),
                    code: crate::errcodes::ROLE_EMPTY_BODY,
                });
            }
        }
    }
}

// ============================================================================
// R2: Role name conflict with interface port/param
// ============================================================================

/// Role name should not collide with a port name or parameter name
/// in the same interface.
fn check_role_name_conflict(acc: &mut CheckAccumulator) {
    let ifaces = crate::definition_space().workspace_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // Collect all pin/port names in the interface
        let pin_names: HashSet<String> = iface.pins.names_to_id.keys().cloned().collect();

        // Collect param names
        let param_names: HashSet<String> = iface
            .params
            .iter()
            .filter_map(|d| d.get_primary_name())
            .collect();

        for role in &iface.roles {
            let role_name = role.name.to_string();
            if pin_names.contains(&role_name) {
                acc.push(CheckResult {
                    check_name: "insts",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(iface.span.start..iface.span.end),
                    message: format!(
                        "Role '{}' in interface '{}' shares a name with a pin/port.",
                        role_name, iface.name
                    ),
                    code: crate::errcodes::ROLE_NAME_SHADOWS,
                });
            }
            if param_names.contains(&role_name) {
                acc.push(CheckResult {
                    check_name: "insts",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(iface.span.start..iface.span.end),
                    message: format!(
                        "Role '{}' in interface '{}' shares a name with a parameter.",
                        role_name, iface.name
                    ),
                    code: crate::errcodes::ROLE_NAME_SHADOWS,
                });
            }
        }
    }
}

// ============================================================================
// R6: IO type direction on function parameter declaration
// ============================================================================

/// Function parameters should not carry IO direction (in/out/io/ps/anl/nc).
/// IO types are for ports, not function arguments.
fn check_func_param_iotype(acc: &mut CheckAccumulator) {
    // Check component functions
    {
        let comps = crate::definition_space().workspace_components();
        for (sn, comp) in comps.iter() {
            let uri = sn.uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            for func in comp.funcs.iter() {
                for d in func.params.iter() {
                    if d.param_type.direction.is_some() {
                        if let Some(pname) = d.get_primary_name() {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: Some(comp.span.start..comp.span.end),
                                message: format!(
                                    "Function '{}' in component '{}': param '{}' has IO direction ({:?}), \
                                     which is only valid for ports.",
                                    func.name, sn.ident, pname,
                                    d.param_type.direction.unwrap().as_str()
                                ),
                                code: crate::errcodes::ATTR_NESTING_TOO_DEEP,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check module functions
    {
        let modules = crate::definition_space().workspace_modules();
        for (sn, m) in modules.iter() {
            let uri = sn.uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            for func in m.funcs.iter() {
                for d in func.params.iter() {
                    if d.param_type.direction.is_some() {
                        if let Some(pname) = d.get_primary_name() {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: Some(m.span.start..m.span.end),
                                message: format!(
                                    "Function '{}' in module '{}': param '{}' has IO direction ({:?}), \
                                     which is only valid for ports.",
                                    func.name, sn.ident, pname,
                                    d.param_type.direction.unwrap().as_str()
                                ),
                                code: crate::errcodes::ATTR_NESTING_TOO_DEEP,
                            });
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// R7: `role` keyword as param in component/module (non-interface)
// ============================================================================

/// The `role` keyword parameter is only valid inside interface definitions.
/// Flag it when used in components or modules.
fn check_role_param_outside_interface(acc: &mut CheckAccumulator) {
    use crate::semantic::basic::mc_param_type::McParamTypeKind;

    // Check components
    {
        let comps = crate::definition_space().workspace_components();
        for (sn, comp) in comps.iter() {
            let uri = sn.uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            for d in comp.params.iter() {
                if matches!(d.param_type.kind, McParamTypeKind::Role) {
                    if let Some(pname) = d.get_primary_name() {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Error,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}' uses 'role' keyword for param '{}'. \
                                 'role' is only valid in interface definitions.",
                                sn.ident, pname
                            ),
                            code: crate::errcodes::ATTR_PIN_GROUP_UNDEFINED,
                        });
                    }
                }
            }
        }
    }

    // Check modules
    {
        let modules = crate::definition_space().workspace_modules();
        for (sn, m) in modules.iter() {
            let uri = sn.uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            for d in m.params.iter() {
                if matches!(d.param_type.kind, McParamTypeKind::Role) {
                    if let Some(pname) = d.get_primary_name() {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Error,
                            uri: Some(uri.clone()),
                            span: Some(m.span.start..m.span.end),
                            message: format!(
                                "Module '{}' uses 'role' keyword for param '{}'. \
                                 'role' is only valid in interface definitions.",
                                sn.ident, pname
                            ),
                            code: crate::errcodes::ATTR_PIN_GROUP_UNDEFINED,
                        });
                    }
                }
            }
        }
    }
}

// ============================================================================
// R9: Non-constant / expression-like default value
// ============================================================================

/// Default values should be simple constants, not expressions with operators
/// or variable references.
fn check_non_constant_default(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for d in comp.params.iter() {
            let pname = d.get_primary_name().unwrap_or_default();
            // Skip enum-class params — their defaults are enum value references
            if d.has_enum_class() {
                continue;
            }
            if let Some(def_val) = d.param_type.default_value() {
                // Heuristic: if default contains arithmetic/logic operators,
                // it's likely a non-constant expression.
                let is_expression = def_val.contains('+')
                    || def_val.contains('-')
                    || def_val.contains('*')
                    || def_val.contains('/')
                    || def_val.contains("<<")
                    || def_val.contains(">>")
                    || def_val.contains('&')
                    || def_val.contains('|')
                    || def_val.contains('^')
                    || def_val.contains("&&")
                    || def_val.contains("||")
                    || def_val.contains("==")
                    || def_val.contains("!=")
                    || def_val.contains(">=")
                    || def_val.contains("<=")
                    // References to other params or variables
                    || def_val.contains("this.")
                    || def_val.contains("pins.")
                    || (def_val.chars().next().map_or(false, |c| c.is_alphabetic())
                        && !def_val.starts_with("UV.")
                        && !def_val.starts_with("true")
                        && !def_val.starts_with("false")
                        && !def_val.contains('"')
                        && !def_val.contains('\''));

                if is_expression {
                    acc.push(CheckResult {
                        check_name: "insts",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
                        message: format!(
                            "Param '{}' in component '{}' has a non-constant default value '{}'. \
                             Use a simple literal or unit-value.",
                            pname, sn.ident, def_val
                        ),
                        code: crate::errcodes::PINS_PLUS_AND_PINS_CONFLICT,
                    });
                }
            }
        }
    }
}
