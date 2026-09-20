// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Role-position constructor args must be bare identifiers (U144 slices 1+3).
//!
//! A role-bearing interface's constructor argument position IS the role
//! position: `SPI::SPI(Master)` selects the role by name, so the name must
//! resolve in the definition space. A quoted or numeric literal there
//! (`SPI::SPI("Slave")`, `PJ(123)`) classifies as a plain string/number
//! parameter instead — the binding parses, but the role is never recorded:
//! E4104 role validation, E4184 port role gate and peer matching are all
//! silently bypassed, and a misspelling passes clean (the ident-vs-literal
//! ruling; evidence matrix in mcd log/9.20.u143-ref-position-literal-audit.md).
//! This check turns that silent degrade into an explicit error.
//!
//! Faces covered: component formal params (A3, first slice), module port head
//! formals (A3, first slice; the bare twin is E4184's), and component
//! `pins`-block rows (third slice — those bind through McPins and never reach
//! `classify_declare`; there the check also enforces E4104 on bare names,
//! which the pins face never had).

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::basic::mc_param::McParamValue;
use std::collections::HashSet;

pub struct IfaceRoleArgCheck;

impl ValidationCheck for IfaceRoleArgCheck {
    fn name(&self) -> &'static str {
        "interface"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Error
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_iface_role_arg_literal(acc);
    }
}

/// Every interface whose name resolves to a role-bearing definition — keyed by
/// the same `ident` comparison `check_iface_role_exists` (E4104) uses, so the
/// two checks see the same name set.
fn role_bearing_ifaces() -> HashSet<String> {
    crate::definition_space()
        .workspace_interfaces()
        .iter()
        .filter(|(_, iface)| !iface.roles.is_empty())
        .map(|(sn, _)| sn.ident.to_string())
        .collect()
}

/// A binding that carries constructor args but classified as plain-parameter
/// (`McParamTypeKind::Interface`) means NONE of its args was a bare
/// identifier (any identifier arg would have made it `InterfaceWithRole`).
/// Against a role-bearing interface that is a literal in the role position.
fn check_iface_role_arg_literal(acc: &mut CheckAccumulator) {
    use crate::semantic::basic::mc_param_type::McParamTypeKind;

    let role_ifaces = role_bearing_ifaces();
    if role_ifaces.is_empty() {
        return;
    }

    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for d in comp.params.iter() {
            if let McParamTypeKind::Interface {
                ref class_name,
                ref params,
            } = d.param_type.kind
            {
                if params.is_empty() || !role_ifaces.contains(class_name) {
                    continue;
                }
                let pname = d.get_primary_name().unwrap_or_default();
                let span = comp
                    .params
                    .get_def_span(&pname)
                    .unwrap_or_else(|| comp.span.start..comp.span.end);
                acc.push(CheckResult {
                    check_name: "interface",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(span),
                    message: format!(
                        "Component '{}': param '{}' binds interface '{}' with literal \
                         argument(s) [{}] — the role position takes a bare identifier; a \
                         quoted or numeric literal is swallowed as a plain parameter and \
                         silently bypasses role validation. Write the role name bare: \
                         '{}::{}(Role)'",
                        comp.name,
                        pname,
                        class_name,
                        params.join(", "),
                        pname,
                        class_name
                    ),
                    code: crate::errcodes::IFACE_ROLE_ARG_LITERAL,
                });
            }
        }
    }

    // Module ports are role-less conduits (replicated-binding-design R3): even
    // a bare role there is E4184's error, so a literal one is the same R3 miss
    // in disguise — it dodged the gate only by not looking like a role.
    let modules = crate::definition_space().workspace_modules();
    for (sn, module) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for d in module.params.iter() {
            if let McParamTypeKind::Interface {
                ref class_name,
                ref params,
            } = d.param_type.kind
            {
                if params.is_empty() || !role_ifaces.contains(class_name) {
                    continue;
                }
                let pname = d.get_primary_name().unwrap_or_default();
                let span = module
                    .params
                    .get_def_span(&pname)
                    .unwrap_or_else(|| module.span.start..module.span.end);
                acc.push(CheckResult {
                    check_name: "interface",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: Some(span),
                    message: format!(
                        "Module '{}': port '{}' binds interface '{}' with literal \
                         argument(s) [{}] — module ports are role-less conduits; drop the \
                         arguments entirely: '{}::{}()'",
                        module.name,
                        pname,
                        class_name,
                        params.join(", "),
                        pname,
                        class_name
                    ),
                    code: crate::errcodes::IFACE_ROLE_ARG_LITERAL,
                });
            }
        }
    }

    // Pins-row face (U144 third slice): `io [1,2] = I2C0::I2C(Master)` inside a
    // component `pins` block binds through McPins, never through
    // `classify_declare`, so the two loops above cannot see it (the U143 audit
    // noted exactly this bypass). The `McPinPort::Interface` port retains the
    // raw constructor args, so judge them here. Against a role-bearing
    // interface every constructor argument is a role reference: a quoted or
    // numeric literal is the E4185 degrade, and a bare name that matches no
    // role is E4104's miss — the same rule the param face enforces, which the
    // pins face never had (probe: a misspelled bare role checked clean). An
    // empty argument list (`GPIO()`) stays unjudged: whether a role is
    // required there is E4104/E4184's business, already settled on their own
    // faces.
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for (member, port) in comp.pins.names_to_id.iter() {
            let crate::McPinPort::Interface(mc2) = port else {
                continue;
            };
            if mc2.params.is_empty() || mc2.base.roles.is_empty() {
                continue;
            }
            let role_names: HashSet<String> =
                mc2.base.roles.iter().map(|r| r.name.to_string()).collect();
            let class_name = mc2.base.name.to_string();
            let span = comp
                .pins
                .pin_name_spans
                .get(member)
                .cloned()
                .unwrap_or_else(|| comp.span.start..comp.span.end);
            for p in mc2.params.iter() {
                match p {
                    McParamValue::Ids(ids) => {
                        let role_val = ids.to_string();
                        if role_names.contains(&role_val) {
                            continue;
                        }
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(span.clone()),
                            message: format!(
                                "Component '{}': pins member '{}' references role '{}' in \
                                 interface '{}', but that role is not defined in the \
                                 interface. Available roles: {}",
                                comp.name,
                                member,
                                role_val,
                                class_name,
                                role_names
                                    .iter()
                                    .map(|r| r.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ),
                            code: crate::errcodes::IFACE_ROLE_NOT_FOUND,
                        });
                    }
                    other => {
                        acc.push(CheckResult {
                            check_name: "interface",
                            severity: CheckSeverity::Error,
                            uri: Some(uri.clone()),
                            span: Some(span.clone()),
                            message: format!(
                                "Component '{}': pins member '{}' binds interface '{}' with \
                                 literal argument '{}' — the role position takes a bare \
                                 identifier; a quoted or numeric literal is swallowed and \
                                 silently bypasses role validation. Write the role name \
                                 bare: '{}::{}(Role)'",
                                comp.name, member, class_name, other, member, class_name
                            ),
                            code: crate::errcodes::IFACE_ROLE_ARG_LITERAL,
                        });
                    }
                }
            }
        }
    }
}
