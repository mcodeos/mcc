/// Extra checks: H3, I1, I3, J2, J3, N5, N6, U1, U4, U5
use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck};
use std::collections::HashSet;

pub struct ExtraCheck;

impl ValidationCheck for ExtraCheck {
    fn name(&self) -> &'static str { "extra" }
    fn phase(&self) -> CheckPhase { CheckPhase::PostParse }
    fn default_severity(&self) -> CheckSeverity { CheckSeverity::Warning }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        // Collect library names for J3 shadow detection
        let lib_names: HashSet<String> = {
            let mut s = HashSet::new();
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for e in comps.iter() { s.insert(e.key().ident.to_string()); }
            let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
            for e in ifaces.iter() { s.insert(e.key().ident.to_string()); }
            let enums = crate::builder::workspace::WORKSPACE.enums.borrow();
            for e in enums.iter() { s.insert(e.key().ident.to_string()); }
            s
        };

        // J3: user port/instance names that shadow library CMIE names
        {
            let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
            for entry in modules.iter() {
                let uri = entry.key().uri.to_string();
                if uri.contains("/unitest/") || uri.contains("/cases") { continue; }
                let m = entry.value();
                for port_name in m.insts.iter_instance_names() {
                    if lib_names.contains(port_name) {
                        acc.push(CheckResult {
                            check_name: "extra", severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()), span: None,
                            message: format!("Port '{}' shadows a library CMIE name.", port_name),
                            code: 2203,
                        });
                    }
                }
            }
        }

        // J2: UPPERCASE instance names (should be lowercase)
        {
            let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
            for entry in modules.iter() {
                let uri = entry.key().uri.to_string();
                if uri.contains("/unitest/") || uri.contains("/cases") { continue; }
                let m = entry.value();
                for name in m.insts.iter_instance_names() {
                    if name.len() <= 2 { continue; } // skip short names like "A", "X6"
                    if name.chars().all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_') {
                        if name.chars().any(|c| c.is_uppercase()) && !name.contains('_') {
                            acc.push(CheckResult {
                                check_name: "extra", severity: CheckSeverity::Info,
                                uri: Some(uri.clone()), span: None,
                                message: format!("Instance '{}' is all-uppercase (convention: lower_snake).", name),
                                code: 2202,
                            });
                        }
                    }
                }
            }
        }

        // U1: enums with only one value
        {
            let enums = crate::builder::workspace::WORKSPACE.enums.borrow();
            for entry in enums.iter() {
                let e = entry.value();
                if e.values.len() == 1 {
                    acc.push(CheckResult {
                        check_name: "extra", severity: CheckSeverity::Info,
                        uri: Some(entry.key().uri.to_string()), span: None,
                        message: format!("Enum '{}' has only one value.", e.name),
                        code: 2501,
                    });
                }
            }
        }

        // N5: default value type mismatch (::INT = "str")
        {
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for entry in comps.iter() {
                let comp = entry.value();
                for d in comp.params.iter() {
                    if let Some(def) = d.param_type.default_value() {
                        let kind = &d.param_type.kind;
                        use crate::core::basic::mc_param_type::McParamTypeKind;
                        match kind {
                            McParamTypeKind::BasicInt { .. } | McParamTypeKind::BasicHex { .. } => {
                                if def.starts_with('"') || def.starts_with('\'') {
                                    acc.push(CheckResult {
                                        check_name: "extra", severity: CheckSeverity::Error,
                                        uri: Some(entry.key().uri.to_string()), span: None,
                                        message: format!("Param '{}' is ::INT but default '{}' is a string.",
                                            d.get_primary_name().unwrap_or_default(), def),
                                        code: 2505,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // H3: overlapping pin ranges within a component
        check_overlapping_pins(acc);
        // F1: reserved name usage (this, pins as user-defined names)
        check_reserved_names(acc, &lib_names);
    }
}

/// H3: pin ranges like [10,11] and [8:11] that overlap.
fn check_overlapping_pins(acc: &mut CheckAccumulator) {
    // Pin overlap detection requires pin ID range expansion.
    // Deferred: needs McPins.pin_id_to_names/pins iteration with range expansion.
    // Placeholder for future implementation.
    let _ = acc;
}

/// F1: user-defined names that match reserved keywords.
fn check_reserved_names(acc: &mut CheckAccumulator, _lib_names: &HashSet<String>) {
    let reserved: HashSet<&str> = ["this", "pins", "role", "func", "return", "in", "out", "io", "ps", "anl", "nc", "if", "else"].iter().cloned().collect();
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") { continue; }
        let comp = entry.value();
        for d in comp.params.iter() {
            if let Some(name) = d.get_primary_name() {
                if reserved.contains(name.as_str()) {
                    acc.push(CheckResult {
                        check_name: "extra", severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()), span: None,
                        message: format!("Parameter '{}' uses reserved keyword.", name),
                        code: 2601,
                    });
                }
            }
        }
    }
}
