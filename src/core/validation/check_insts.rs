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

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
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

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
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
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        // Walk each instance in the module's symbol table
        for (inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
            match instance {
                crate::McInstance::Component(c2) => {
                    let class_name = c2.name.to_string();
                    let def_param_count = c2.base.params.len();
                    let call_arg_count = c2.params.len();

                    if call_arg_count > def_param_count {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: None,
                            message: format!(
                                "Instance '{}' of component '{}' passes {} args, but '{}' declares {} param(s).",
                                inst_name, class_name, call_arg_count, class_name, def_param_count
                            ),
                            code: 2801,
                        });
                    } else if call_arg_count < def_param_count {
                        // Count required (non-default) params
                        let required = c2
                            .base
                            .params
                            .iter()
                            .filter(|d| !d.has_default_value())
                            .count();
                        if call_arg_count < required {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Instance '{}' of component '{}' passes {} args, but '{}' requires at least {} ({} total, {} optional).",
                                    inst_name, class_name, call_arg_count, class_name, required,
                                    def_param_count, def_param_count - required
                                ),
                                code: 2801,
                            });
                        }
                    }
                }
                crate::McInstance::Module(m2) => {
                    let class_name = m2.name.to_string();
                    let def_param_count = m2.base.params.len();
                    let call_arg_count = m2.args.len();

                    if call_arg_count > def_param_count {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: None,
                            message: format!(
                                "Instance '{}' of module '{}' passes {} args, but '{}' declares {} param(s).",
                                inst_name, class_name, call_arg_count, class_name, def_param_count
                            ),
                            code: 2801,
                        });
                    } else if call_arg_count < def_param_count {
                        let required = m2
                            .base
                            .params
                            .iter()
                            .filter(|d| !d.has_default_value())
                            .count();
                        if call_arg_count < required {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Instance '{}' of module '{}' passes {} args, but '{}' requires at least {} ({} total, {} optional).",
                                    inst_name, class_name, call_arg_count, class_name, required,
                                    def_param_count, def_param_count - required
                                ),
                                code: 2801,
                            });
                        }
                    }
                }
                crate::McInstance::Interface(i2) => {
                    let class_name = i2.name.to_string();
                    let def_param_count = i2.base.params.len();
                    let call_arg_count = i2.params.len();

                    if call_arg_count > def_param_count {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: None,
                            message: format!(
                                "Instance '{}' of interface '{}' passes {} args, but '{}' declares {} param(s).",
                                inst_name, class_name, call_arg_count, class_name, def_param_count
                            ),
                            code: 2801,
                        });
                    } else if call_arg_count < def_param_count {
                        let required = i2
                            .base
                            .params
                            .iter()
                            .filter(|d| !d.has_default_value())
                            .count();
                        if call_arg_count < required {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Instance '{}' of interface '{}' passes {} args, but '{}' requires at least {} ({} total, {} optional).",
                                    inst_name, class_name, call_arg_count, class_name, required,
                                    def_param_count, def_param_count - required
                                ),
                                code: 2801,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

// ============================================================================
// R1: Role with empty body
// ============================================================================

/// Interface roles that have no pins, no attrs, and no body content.
fn check_role_empty_body(acc: &mut CheckAccumulator) {
    let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
    for entry in ifaces.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let iface = entry.value();
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
                    span: None,
                    message: format!(
                        "Role '{}' in interface '{}' has an empty body (no pins, attrs, or clauses).",
                        role.name, iface.name
                    ),
                    code: 2802,
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
    let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
    for entry in ifaces.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let iface = entry.value();

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
                    span: None,
                    message: format!(
                        "Role '{}' in interface '{}' shares a name with a pin/port.",
                        role_name, iface.name
                    ),
                    code: 2803,
                });
            }
            if param_names.contains(&role_name) {
                acc.push(CheckResult {
                    check_name: "insts",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: None,
                    message: format!(
                        "Role '{}' in interface '{}' shares a name with a parameter.",
                        role_name, iface.name
                    ),
                    code: 2803,
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
        let comps = crate::builder::workspace::WORKSPACE.components.borrow();
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp = entry.value();
            for func in comp.funcs.iter() {
                for d in func.params.iter() {
                    if d.param_type.direction.is_some() {
                        if let Some(pname) = d.get_primary_name() {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Function '{}' in component '{}': param '{}' has IO direction ({:?}), \
                                     which is only valid for ports.",
                                    func.name, entry.key().ident, pname,
                                    d.param_type.direction.unwrap().as_str()
                                ),
                                code: 2804,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check module functions
    {
        let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
        for entry in modules.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let m = entry.value();
            for func in m.funcs.iter() {
                for d in func.params.iter() {
                    if d.param_type.direction.is_some() {
                        if let Some(pname) = d.get_primary_name() {
                            acc.push(CheckResult {
                                check_name: "insts",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Function '{}' in module '{}': param '{}' has IO direction ({:?}), \
                                     which is only valid for ports.",
                                    func.name, entry.key().ident, pname,
                                    d.param_type.direction.unwrap().as_str()
                                ),
                                code: 2804,
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
    use crate::core::basic::mc_param_type::McParamTypeKind;

    // Check components
    {
        let comps = crate::builder::workspace::WORKSPACE.components.borrow();
        for entry in comps.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp = entry.value();
            for d in comp.params.iter() {
                if matches!(d.param_type.kind, McParamTypeKind::Role) {
                    if let Some(pname) = d.get_primary_name() {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Error,
                            uri: Some(uri.clone()),
                            span: None,
                            message: format!(
                                "Component '{}' uses 'role' keyword for param '{}'. \
                                 'role' is only valid in interface definitions.",
                                entry.key().ident,
                                pname
                            ),
                            code: 2805,
                        });
                    }
                }
            }
        }
    }

    // Check modules
    {
        let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
        for entry in modules.iter() {
            let uri = entry.key().uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let m = entry.value();
            for d in m.params.iter() {
                if matches!(d.param_type.kind, McParamTypeKind::Role) {
                    if let Some(pname) = d.get_primary_name() {
                        acc.push(CheckResult {
                            check_name: "insts",
                            severity: CheckSeverity::Error,
                            uri: Some(uri.clone()),
                            span: None,
                            message: format!(
                                "Module '{}' uses 'role' keyword for param '{}'. \
                                 'role' is only valid in interface definitions.",
                                entry.key().ident,
                                pname
                            ),
                            code: 2805,
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
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for d in comp.params.iter() {
            let pname = d.get_primary_name().unwrap_or_default();
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
                        span: None,
                        message: format!(
                            "Param '{}' in component '{}' has a non-constant default value '{}'. \
                             Use a simple literal or unit-value.",
                            pname,
                            entry.key().ident,
                            def_val
                        ),
                        code: 2806,
                    });
                }
            }
        }
    }
}
