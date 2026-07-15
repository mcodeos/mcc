/// Extra checks: H3, I1, I3, J2, J3, N5, N6, U1, U4, U5
use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
use std::collections::HashSet;

pub struct ExtraCheck;

impl ValidationCheck for ExtraCheck {
    fn name(&self) -> &'static str {
        "extra"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        // Collect library names for J3 shadow detection
        let lib_names: HashSet<String> = {
            let mut s = HashSet::new();
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for e in comps.iter() {
                s.insert(e.key().ident.to_string());
            }
            let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
            for e in ifaces.iter() {
                s.insert(e.key().ident.to_string());
            }
            let enums = crate::builder::workspace::WORKSPACE.enums.borrow();
            for e in enums.iter() {
                s.insert(e.key().ident.to_string());
            }
            s
        };

        // J3: user port/instance names that shadow library CMIE names
        {
            let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
            for entry in modules.iter() {
                let uri = entry.key().uri.to_string();
                if uri.contains("/unitest/") || uri.contains("/cases") {
                    continue;
                }
                let m = entry.value();
                for port_name in m.insts.iter_instance_names() {
                    if lib_names.contains(port_name) {
                        acc.push(CheckResult {
                            check_name: "extra",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: None,
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
                if uri.contains("/unitest/") || uri.contains("/cases") {
                    continue;
                }
                let m = entry.value();
                for name in m.insts.iter_instance_names() {
                    if name.len() <= 2 {
                        continue;
                    } // skip short names like "A", "X6"
                    if name
                        .chars()
                        .all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
                    {
                        if name.chars().any(|c| c.is_uppercase()) && !name.contains('_') {
                            acc.push(CheckResult {
                                check_name: "extra",
                                severity: CheckSeverity::Info,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Instance '{}' is all-uppercase (convention: lower_snake).",
                                    name
                                ),
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
                        check_name: "extra",
                        severity: CheckSeverity::Info,
                        uri: Some(entry.key().uri.to_string()),
                        span: None,
                        message: format!("Enum '{}' has only one value.", e.name),
                        code: 2501,
                    });
                }
            }
        }

        // N5 + R8: default value type/unit mismatch
        check_default_type_mismatch(acc);

        // H3: overlapping pin ranges
        check_overlapping_pins(acc);
        // I4: interface pin count mismatch
        check_interface_pin_counts(acc);
        // M1/M3: completely empty or pinless components
        check_component_structure(acc);
        // M4: pinless interfaces
        check_interface_structure(acc);
        // R4: empty function bodies
        check_empty_functions(acc);
        // U5: empty defines
        check_empty_defines(acc);
        // D2: instance class not found
        check_instance_class_found(acc);
        // D3: bus member collision
        check_bus_member_collision(acc);
        // J5: copy-pasted function bodies (DRY)
        check_dry_functions(acc);
        // F2: naming convention enforcement
        check_naming_convention(acc);
        check_func_name_conflict(acc); // R5
        check_reserved_names(acc, &lib_names); // F1
    }
}

/// R4: functions with empty bodies (module + component funcs).
fn check_empty_functions(acc: &mut CheckAccumulator) {
    // Module funcs
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        for func in entry.value().funcs.iter() {
            if func.lines.is_empty() && func.insts.is_empty() {
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: None,
                    message: format!("Function '{}' has an empty body.", func.name),
                    code: 2602,
                });
            }
        }
    }
    // Component funcs
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        for func in entry.value().funcs.iter() {
            if func.lines.is_empty() && func.insts.is_empty() {
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: None,
                    message: format!(
                        "Function '{}' in component '{}' has an empty body.",
                        func.name,
                        entry.key().ident
                    ),
                    code: 2602,
                });
            }
        }
    }
}

/// I4: interface pin count mismatch (physical pins vs interface definition).
fn check_interface_pin_counts(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let comp = entry.value();
        for (pin_name, port) in &comp.pins.names_to_id {
            if let crate::core::component::mc_pins::McPinPort::Interface(iface) = port {
                let iface_name = iface.name.to_string();
                let iface_pin_count = iface.base.pins.names_to_id.len();
                // Check each physical pin binding
                let phys_pins: Vec<&String> =
                    comp.pins.pin_id_to_names.values().flatten().collect();
                // Count how many physical pins are bound to this interface name
                let bound_count = phys_pins.iter().filter(|n| n.as_str() == pin_name).count();
                if bound_count < iface_pin_count {
                    acc.push(CheckResult {
                        check_name: "extra", severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()), span: None,
                        message: format!(
                            "Interface '{}' expects {} pins but only {} physical pins bound as '{}'.",
                            iface_name, iface_pin_count, bound_count, pin_name
                        ),
                        code: 2613,
                    });
                }
            }
        }
    }
}

/// H3: overlapping pin range assignments.
fn check_overlapping_pins(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let comp = entry.value();
        if comp.pins.pin_ranges.len() > 1 {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Info,
                uri: Some(uri),
                span: None,
                message: format!(
                    "Component '{}' has {} pin range definitions. Check for overlaps.",
                    entry.key().ident,
                    comp.pins.pin_ranges.len()
                ),
                code: 2608,
            });
        }
    }
}

/// M1: components with no params, no pins, no attrs, no funcs.
/// M3: components without pins.
fn check_component_structure(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let comp = entry.value();
        let name = entry.key().ident.to_string();
        let has_params = !comp.params.is_empty();
        let has_pins = !comp.pins.names_to_id.is_empty();
        let has_attrs = comp.attrs.len() > 0;
        let has_funcs = !comp.funcs.is_empty();
        // M1: completely empty
        if !has_params && !has_pins && !has_attrs && !has_funcs {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: None,
                message: format!(
                    "Component '{}' has no params, pins, attributes, or functions.",
                    name
                ),
                code: 2603,
            });
        }
        // M3: has content but no pins
        if (has_params || has_attrs || has_funcs) && !has_pins {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Warning,
                uri: Some(uri),
                span: None,
                message: format!("Component '{}' has no pin definitions.", name),
                code: 2604,
            });
        }
    }
}

/// M4: interfaces without pins.
fn check_interface_structure(acc: &mut CheckAccumulator) {
    let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
    for entry in ifaces.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let iface = entry.value();
        if iface.pins.names_to_id.is_empty() && iface.roles.is_empty() {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Warning,
                uri: Some(uri),
                span: None,
                message: format!("Interface '{}' has no pins or roles.", entry.key().ident),
                code: 2605,
            });
        }
    }
}

/// N5 + R8: default value type mismatch for typed parameters.
fn check_default_type_mismatch(acc: &mut CheckAccumulator) {
    use crate::core::basic::mc_param_type::McParamTypeKind;
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let comp = entry.value();
        let uri = entry.key().uri.to_string();
        for d in comp.params.iter() {
            if let Some(def) = d.param_type.default_value() {
                let pname = d.get_primary_name().unwrap_or_default();
                match &d.param_type.kind {
                    // INT/HEX with string default
                    McParamTypeKind::BasicInt { .. } | McParamTypeKind::BasicHex { .. } => {
                        if def.starts_with('"') || def.starts_with('\'') {
                            acc.push(CheckResult {
                                check_name: "extra",
                                severity: CheckSeverity::Error,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Param '{}' is ::INT/HEX but default '{}' is a string.",
                                    pname, def
                                ),
                                code: 2505,
                            });
                        }
                    }
                    // STRING with numeric default
                    McParamTypeKind::BasicString { .. } => {
                        if !def.starts_with('"')
                            && !def.starts_with('\'')
                            && def.chars().next().map_or(false, |c| c.is_ascii_digit())
                        {
                            acc.push(CheckResult {
                                check_name: "extra",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: None,
                                message: format!(
                                    "Param '{}' is ::STRING but default '{}' looks numeric.",
                                    pname, def
                                ),
                                code: 2506,
                            });
                        }
                    }
                    // Unit-typed with plain number (no unit suffix)
                    McParamTypeKind::UnitValue { unit }
                    | McParamTypeKind::UnitValueDefault { unit, .. } => {
                        if def
                            .chars()
                            .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
                        {
                            let unit_name = format!("{:?}", unit);
                            acc.push(CheckResult {
                                check_name: "extra", severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()), span: None,
                                message: format!(
                                    "Param '{}' is ::UV.{} but default '{}' has no unit suffix. Add e.g. '5V'.",
                                    pname, unit_name, def
                                ),
                                code: 2507,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// U4/U5: defines with non-attribute clauses or empty body.
fn check_empty_defines(acc: &mut CheckAccumulator) {
    let defines = crate::builder::workspace::WORKSPACE.defines.borrow();
    for entry in defines.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let def = entry.value();
        // U5: empty define (no attrs and empty body)
        if def.attrs.is_empty() {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: None,
                message: format!("Define '{}' has no attributes.", def.name),
                code: 2611,
            });
        }
        // U4: define with non-attribute body clauses — scan body AST
        if let Some(sub) = def.body.get_sub_node() {
            for child in sub.iter() {
                let ct = child.get_type();
                if ct != crate::MCAST_ATTRIBUTE {
                    acc.push(CheckResult {
                        check_name: "extra", severity: CheckSeverity::Warning,
                        uri: Some(uri), span: None,
                        message: format!(
                            "Define '{}' contains non-attribute clause (type={}). Defines should only contain attributes.",
                            def.name, ct
                        ),
                        code: 2612,
                    });
                    break;
                }
            }
        }
    }
}

/// D2: instance class name not found in workspace component/interfaces tables.
fn check_instance_class_found(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
    let mut known: HashSet<String> = HashSet::new();
    for e in comps.iter() {
        known.insert(e.key().ident.to_string());
    }
    for e in ifaces.iter() {
        known.insert(e.key().ident.to_string());
    }

    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let m = entry.value();
        for name in m.insts.iter_instance_names() {
            // Skip labels, buses, anon ports
            if name.starts_with('@') || name.starts_with('[') || name.len() <= 1 {
                continue;
            }
            if !known.contains(name) {
                // Check if it looks like a class name (uppercase start)
                if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    acc.push(CheckResult {
                        check_name: "extra",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: None,
                        message: format!(
                            "Instance '{}' references class that is not loaded.",
                            name
                        ),
                        code: 2606,
                    });
                }
            }
        }
    }
}

/// D3: bus member collision — two instances/buses with same base name,
/// conflicting or duplicate member names.
fn check_bus_member_collision(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let m = entry.value();
        let mut bus_members: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for inst_name in m.insts.iter_instance_names() {
            if let Some((_, inst)) = m.insts.insts().get(inst_name) {
                match inst {
                    crate::McInstance::Bus(bus) => {
                        let entry = bus_members.entry(bus.name.clone()).or_default();
                        for m in &bus.member {
                            if entry.contains(m) {
                                acc.push(CheckResult {
                                    check_name: "extra",
                                    severity: CheckSeverity::Warning,
                                    uri: Some(uri.clone()),
                                    span: None,
                                    message: format!(
                                        "Bus '{}' has duplicate member '{}' in module.",
                                        bus.name, m
                                    ),
                                    code: 2609,
                                });
                            } else {
                                entry.push(m.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// J5: copy-pasted function bodies (DRY violation).
fn check_dry_functions(acc: &mut CheckAccumulator) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let comp = entry.value();
        if comp.funcs.len() < 2 {
            continue;
        }
        let mut seen: std::collections::HashMap<u64, Vec<&str>> = std::collections::HashMap::new();
        for func in comp.funcs.iter() {
            let mut h = DefaultHasher::new();
            func.lines.len().hash(&mut h);
            // Hash the McPhrase Display output as a body fingerprint
            for line in &func.lines {
                format!("{}", line).hash(&mut h);
            }
            let hash = h.finish();
            let name = func.name.to_string();
            seen.entry(hash)
                .or_default()
                .push(Box::leak(name.into_boxed_str()));
        }
        for (_, names) in &seen {
            if names.len() > 1 {
                acc.push(CheckResult {
                    check_name: "extra", severity: CheckSeverity::Info,
                    uri: Some(uri.clone()), span: None,
                    message: format!(
                        "Component '{}' has {} identical function bodies: {}. Consider refactoring.",
                        entry.key().ident, names.len(),
                        names.iter().map(|s| s as &str).collect::<Vec<_>>().join(", ")
                    ),
                    code: 2610,
                });
            }
        }
    }
}

/// F2: naming convention — UPPER_SNAKE for components/interfaces/enums.
fn check_naming_convention(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") || uri.contains("/lab/") {
            continue;
        }
        // Skip dot-notation names like "Amplifier.BUFFER" (check each segment)
        for seg in name.split('.') {
            if let Some(first) = seg.chars().next() {
                if first.is_lowercase() && seg.chars().any(|c| c.is_uppercase()) {
                    // Mixed case like "camelCase" — should be all upper
                    acc.push(CheckResult {
                        check_name: "extra",
                        severity: CheckSeverity::Info,
                        uri: Some(uri.clone()),
                        span: None,
                        message: format!(
                            "Component '{}' uses mixed case. Convention is UPPER_SNAKE.",
                            name
                        ),
                        code: 2607,
                    });
                    break;
                }
            }
        }
    }
}

/// F1: user-defined names that match reserved keywords.
fn check_reserved_names(acc: &mut CheckAccumulator, _lib_names: &HashSet<String>) {
    let reserved: HashSet<&str> = [
        "this", "pins", "role", "func", "return", "in", "out", "io", "ps", "anl", "nc", "if",
        "else",
    ]
    .iter()
    .cloned()
    .collect();
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let comp = entry.value();
        for d in comp.params.iter() {
            if let Some(name) = d.get_primary_name() {
                if reserved.contains(name.as_str()) {
                    acc.push(CheckResult {
                        check_name: "extra",
                        severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()),
                        span: None,
                        message: format!("Parameter '{}' uses reserved keyword.", name),
                        code: 2601,
                    });
                }
            }
        }
    }
}

/// R5: function name conflicts with a port/instance name in the same module.
fn check_func_name_conflict(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if uri.contains("/unitest/") || uri.contains("/cases") {
            continue;
        }
        let m = entry.value();
        let inst_names: HashSet<String> = m.insts.iter_instance_names().cloned().collect();
        let param_names: HashSet<String> = m
            .params
            .iter()
            .filter_map(|d| d.get_primary_name())
            .collect();
        for func in m.funcs.iter() {
            let fname = func.name.to_string();
            if inst_names.contains(&fname) || param_names.contains(&fname) {
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: None,
                    message: format!(
                        "Function '{}' shares name with a port/param in the same module.",
                        fname
                    ),
                    code: 2614,
                });
            }
        }
    }
}
