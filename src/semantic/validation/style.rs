// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Style/naming checks: J1-J5, F1-F3, plus the mcode style-guide gates
//! (spec/21-mcode-style.md §2/§7): net names, role/enum values, func names.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::HashSet;

pub struct StyleCheck;

impl ValidationCheck for StyleCheck {
    fn name(&self) -> &'static str {
        "style"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Info
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        let mut lib_names: HashSet<String> = HashSet::new();
        {
            let comps = crate::definition_space().workspace_components();
            for (sn, _) in comps.iter() {
                lib_names.insert(sn.ident.to_string());
            }
            let ifaces = crate::definition_space().workspace_interfaces();
            for (sn, _) in ifaces.iter() {
                lib_names.insert(sn.ident.to_string());
            }
        }

        // J1: Lowercase component names
        // J2: UPPERCASE instance names (deferred — needs inst scan in modules)
        // J3: Identifier shadows library name
        // J4: Empty () on parameterless components (deferred until source syntax is retained)
        // J5: Copy-pasted function bodies — dropped: component funcs describe
        //     net connections, not refactorable logic; identical bodies are a
        //     legitimate shared-connection pattern (e.g. RFReceiver/RFSender).
        // F1: Reserved name usage
        // F2: Naming convention — implemented in extra.rs as check_naming_convention
        // F3: Deprecated CMIE usage (deferred — needs deprecation metadata)

        check_lowercase_components(acc, &lib_names);
        check_net_names_upper_snake(acc);
        check_role_enum_values_upper_snake(acc);
        check_func_names_upper_initial(acc);
    }
}

// mcode style guide gates (spec/21-mcode-style.md §2/§7) — the three
// machine-judgeable classes. Instance names stay outside the gate on
// purpose: the refdes-form vs functional-block split needs design context
// no machine holds (§2 adjudication notes). The sweep reads workspace files only —
// the factory corpus is already conformant (b4075/b4079), and gating system
// root files would let a stale live copy red the gate.

/// A name violates the UPPER_SNAKE rule when it carries an ASCII lowercase
/// letter. The predicate is deliberately spelling-only: any other script
/// passes, and the voltage-run-on form (`VCC24`) is a §2 writing rule the
/// case gate does not judge.
fn has_ascii_lowercase(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_lowercase())
}

/// §2 #3: net labels and module port faces are UPPER_SNAKE. Two faces name a
/// net: the declared ports (header formals and `label`-bearing body rows —
/// the entries carrying a real IOType) and the body's net labels
/// (`McInstance::Label`, one per bare identifier a connection phrase names).
/// Component/module/interface instances are identity names, not net names,
/// and are never judged here; pins and datasheet names never reach this sweep.
fn check_net_names_upper_snake(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, m) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut flag = |name: &str, acc: &mut CheckAccumulator| {
            // Composite spellings (`a.b`) are member chains, not one net name.
            if name.is_empty() || name.contains('.') || !seen.insert(name.to_string()) {
                return;
            }
            if !has_ascii_lowercase(name) {
                return;
            }
            let span = m
                .insts
                .get_port_span(name)
                .unwrap_or_else(|| m.span.start..m.span.end);
            acc.push(CheckResult {
                check_name: "style",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(span),
                message: format!(
                    "Net/port name '{}' is not UPPER_SNAKE; the style guide spells net labels and port faces UPPER_SNAKE (mcode-style §2 #3).",
                    name
                ),
                code: crate::errcodes::NAME_NET_NOT_UPPER_SNAKE,
            });
        };

        for (name, (_, inst)) in m.insts.insts() {
            if name.starts_with('@') || name.starts_with('[') {
                continue;
            }
            let is_net = matches!(inst, crate::McInstance::Label(_)) || m.insts.is_port_io_type(name);
            if is_net {
                flag(name, acc);
            }
        }
    }
}

/// §2 #6: role values and enum values are UPPER_SNAKE. Role values are the
/// interface's role vocabulary (`role Osc` inside an `interface`); enum
/// values are the ids an enum declares. The `@role(...)` lowercase word
/// vocabulary is a different face (§2.1 exemption) and is not judged here.
fn check_role_enum_values_upper_snake(acc: &mut CheckAccumulator) {
    let ifaces = crate::definition_space().workspace_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        for role in &iface.roles {
            let name = role.name.to_string();
            if !has_ascii_lowercase(&name) {
                continue;
            }
            acc.push(CheckResult {
                check_name: "style",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(iface.span.start..iface.span.end),
                message: format!(
                    "Role value '{}' is not UPPER_SNAKE; the style guide spells role and enum values UPPER_SNAKE (mcode-style §2 #6).",
                    name
                ),
                code: crate::errcodes::NAME_ROLE_ENUM_NOT_UPPER_SNAKE,
            });
        }
    }

    let enums = crate::definition_space().workspace_enums();
    for (sn, def) in enums.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        for val in &def.values {
            let name = val.name.to_string();
            if !has_ascii_lowercase(&name) {
                continue;
            }
            acc.push(CheckResult {
                check_name: "style",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(val.span[0] as usize..val.span[1] as usize),
                message: format!(
                    "Enum value '{}' is not UPPER_SNAKE; the style guide spells role and enum values UPPER_SNAKE (mcode-style §2 #6).",
                    name
                ),
                code: crate::errcodes::NAME_ROLE_ENUM_NOT_UPPER_SNAKE,
            });
        }
    }
}

/// §2 #9: function names are uppercase-initial (library and user funcs one
/// rule) — functions are class-level behavior and take the class's form.
/// Only the first letter is judged: `LoadFlash` is conformant as written.
fn check_func_names_upper_initial(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        for f in comp.funcs.iter() {
            let name = f.name.to_string();
            let Some(first) = name.chars().next() else {
                continue;
            };
            if !first.is_ascii_lowercase() {
                continue;
            }
            let span = f
                .span
                .clone()
                .unwrap_or_else(|| comp.span.start..comp.span.end);
            acc.push(CheckResult {
                check_name: "style",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(span),
                message: format!(
                    "Function name '{}' does not start with an uppercase letter; functions are class-level behavior and take the class's uppercase-initial form (mcode-style §2 #9).",
                    name
                ),
                code: crate::errcodes::NAME_FUNC_NOT_UPPER_INITIAL,
            });
        }
    }
}

fn check_lowercase_components(acc: &mut CheckAccumulator, _lib_names: &HashSet<String>) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let name = sn.ident.to_string();
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        if let Some(first) = name.chars().next() {
            if first.is_lowercase() && !name.contains('.') {
                acc.push(CheckResult {
                    check_name: "style",
                    severity: CheckSeverity::Info,
                    uri: Some(uri),
                    span: Some(comp.span.start..comp.span.end),
                    message: format!(
                        "Component '{}' starts with lowercase (convention: UPPER_SNAKE).",
                        name
                    ),
                    code: crate::errcodes::NAME_COMPONENT_LOWERCASE,
                });
            }
        }
    }
}
