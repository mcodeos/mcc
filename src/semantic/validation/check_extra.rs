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
                if super::is_test_file(&uri) {
                    continue;
                }
                let m = entry.value();
                let mod_span_j3 = Some(m.span.start..m.span.end);
                for port_name in m.insts.iter_instance_names() {
                    if lib_names.contains(port_name) {
                        acc.push(CheckResult {
                            check_name: "extra",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: mod_span_j3.clone(),
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
                if super::is_test_file(&uri) {
                    continue;
                }
                let m = entry.value();
                let mod_span_j2 = Some(m.span.start..m.span.end);
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
                                span: mod_span_j2.clone(),
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
                        span: Some(e.span[0] as usize..e.span[1] as usize),
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
        check_port_direction_mismatch(acc); // C3
        check_default_value_range(acc); // B7
        check_body_literal_as_arg(acc); // S5
        check_module_func_unused_params(acc); // B1-ext for module funcs
        check_duplicate_spec_keys(acc); // spec sub-key uniqueness
    }
}

/// R4: functions with empty bodies (module + component funcs).
fn check_empty_functions(acc: &mut CheckAccumulator) {
    // Module funcs
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let mod_span = Some(m.span.start..m.span.end);
        for func in entry.value().funcs.iter() {
            if func.lines.is_empty() && func.insts.is_empty() {
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: mod_span.clone(),
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
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        let comp_span = Some(comp.span.start..comp.span.end);
        for func in entry.value().funcs.iter() {
            if func.lines.is_empty() && func.insts.is_empty() {
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: comp_span.clone(),
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
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for (pin_name, port) in &comp.pins.names_to_id {
            if let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port {
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
                        uri: Some(uri.clone()), span: Some(comp.span.start..comp.span.end),
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
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        if comp.pins.pin_ranges.len() > 1 {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Info,
                uri: Some(uri),
                span: Some(comp.span.start..comp.span.end),
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
        if super::is_test_file(&uri) {
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
                span: Some(comp.span.start..comp.span.end),
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
                span: Some(comp.span.start..comp.span.end),
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
        if super::is_test_file(&uri) {
            continue;
        }
        let iface = entry.value();
        if iface.pins.names_to_id.is_empty() && iface.roles.is_empty() {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Warning,
                uri: Some(uri),
                span: Some(iface.span.start..iface.span.end),
                message: format!("Interface '{}' has no pins or roles.", entry.key().ident),
                code: 2605,
            });
        }
    }
}

/// N5 + R8: default value type mismatch for typed parameters.
fn check_default_type_mismatch(acc: &mut CheckAccumulator) {
    use crate::semantic::basic::mc_param_type::McParamTypeKind;
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
                                span: Some(comp.span.start..comp.span.end),
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
                                span: Some(comp.span.start..comp.span.end),
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
                                uri: Some(uri.clone()), span: Some(comp.span.start..comp.span.end),
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
        if super::is_test_file(&uri) {
            continue;
        }
        let def = entry.value();
        let def_span = Some(def.span.start..def.span.end);
        // U5: empty define (no attrs and empty body)
        if def.attrs.is_empty() {
            acc.push(CheckResult {
                check_name: "extra",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: def_span.clone(),
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
                        uri: Some(uri), span: def_span.clone(),
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
        if super::is_test_file(&uri) {
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
                        span: Some(m.span.start..m.span.end),
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
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let mod_span_bus = Some(m.span.start..m.span.end);
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
                                    span: mod_span_bus.clone(),
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
        if super::is_test_file(&uri) {
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
                    uri: Some(uri.clone()), span: Some(comp.span.start..comp.span.end),
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
        let comp = entry.value();
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) || uri.contains("/lab/") {
            continue;
        }
        // Skip dot-notation names like "AMP.BUFFER" (check each segment)
        for seg in name.split('.') {
            if let Some(first) = seg.chars().next() {
                if first.is_lowercase() && seg.chars().any(|c| c.is_uppercase()) {
                    // Mixed case like "camelCase" — should be all upper
                    acc.push(CheckResult {
                        check_name: "extra",
                        severity: CheckSeverity::Info,
                        uri: Some(uri.clone()),
                        span: Some(comp.span.start..comp.span.end),
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
        if super::is_test_file(&uri) {
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
                        span: Some(comp.span.start..comp.span.end),
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
        if super::is_test_file(&uri) {
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
                    span: Some(m.span.start..m.span.end),
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

/// C3: Port direction mismatch — check if a net connects two output-like ports.
///
/// Scans module net phrases to detect cases where both endpoints of a net
/// are outputs (Out or Power), which could cause driver conflicts.
fn check_port_direction_mismatch(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        for phrase in &m.lines {
            let text = format!("{}", phrase);
            let parts: Vec<&str> = text.split("->").collect();
            if parts.len() < 2 {
                continue;
            }
            let left_names: Vec<&str> = parts[0].split(',').map(|s| s.trim()).collect();
            let right_names: Vec<&str> = parts[1].split(',').map(|s| s.trim()).collect();
            for left in &left_names {
                if let Some(io_left) = m.insts.get_iotype(left) {
                    if matches!(io_left, crate::IOType::Out | crate::IOType::Power) {
                        for right in &right_names {
                            if let Some(io_right) = m.insts.get_iotype(right) {
                                if matches!(io_right, crate::IOType::Out | crate::IOType::Power) {
                                    acc.push(CheckResult {
                                        check_name: "extra",
                                        severity: CheckSeverity::Warning,
                                        uri: Some(uri.clone()),
                                        span: Some(m.span.start..m.span.end),
                                        message: format!(
                                            "Net '{}' connects '{}' ({:?}) to '{}' ({:?}). Both are outputs.",
                                            text.trim(), left, io_left, right, io_right
                                        ),
                                        code: 2615,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// B7: Default value out of range for typed parameters.
///
/// Heuristic: if a param is BasicInt and its default is negative, flag it
/// as potentially out of range (most integer params expect non-negative).
fn check_default_value_range(acc: &mut CheckAccumulator) {
    use crate::semantic::basic::mc_param_type::McParamTypeKind;
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for d in comp.params.iter() {
            if let Some(def) = d.param_type.default_value() {
                let pname = d.get_primary_name().unwrap_or_default();
                match &d.param_type.kind {
                    McParamTypeKind::BasicInt { .. } | McParamTypeKind::BasicHex { .. } => {
                        if def.starts_with('-') {
                            acc.push(CheckResult {
                                check_name: "extra",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: Some(comp.span.start..comp.span.end),
                                message: format!(
                                    "Param '{}' in '{}' has negative default '{}'. Most integer params expect non-negative values.",
                                    pname, entry.key().ident, def
                                ),
                                code: 2513,
                            });
                        }
                    }
                    McParamTypeKind::BasicFloat { .. } => {
                        if let Ok(val) = def.parse::<f64>() {
                            if val.is_infinite() || val.is_nan() {
                                acc.push(CheckResult {
                                    check_name: "extra",
                                    severity: CheckSeverity::Error,
                                    uri: Some(uri.clone()),
                                    span: Some(comp.span.start..comp.span.end),
                                    message: format!(
                                        "Param '{}' in '{}' has invalid float default '{}'.",
                                        pname,
                                        entry.key().ident,
                                        def
                                    ),
                                    code: 2513,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// S5: Body literal used as a call argument.
///
/// Passing an inline body block `{...}` as a positional argument to a
/// component/module constructor is unusual and likely a mistake.
/// Body literals are for component definition, not instantiation.
fn check_body_literal_as_arg(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();

        for phrase in &m.lines {
            let text = format!("{}", phrase);

            // Look for `({` patterns — a body literal block being passed as an
            // argument inside a constructor call like `COMP({...})` or `COMP(a, {...})`
            if text.contains("({") {
                // Check that this isn't a component definition `component NAME({...})`
                if text.starts_with("component") || text.starts_with("module") {
                    continue;
                }

                // Check that the `({` is inside a constructor call (has `(` before it)
                if let Some(paren_pos) = text.find('(') {
                    if let Some(body_pos) = text.find("({") {
                        if body_pos > paren_pos {
                            acc.push(CheckResult {
                                check_name: "extra",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: Some(m.span.start..m.span.end),
                                message: format!(
                                    "Module '{}': inline body literal used as call argument \
                                     in '{}'. Body blocks are for definitions, not \
                                     instantiation arguments.",
                                    entry.key().ident,
                                    text.trim()
                                ),
                                code: 2616,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// B1-ext: unused parameters in module-level functions.
///
/// Module functions should use all declared parameters. An unused parameter
/// may indicate dead code or an incomplete implementation.
fn check_module_func_unused_params(acc: &mut CheckAccumulator) {
    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        let mod_name = entry.key().ident.to_string();

        for func in m.funcs.iter() {
            if func.params.is_empty() {
                continue;
            }
            if func.lines.is_empty() && func.insts.is_empty() {
                // Already caught by R4 (empty function body) — skip
                continue;
            }

            // Collect param names
            let param_names: HashSet<String> = func
                .params
                .iter()
                .filter_map(|d| d.get_primary_name())
                .collect();

            // Collect all identifiers referenced in the function body
            let mut used_names: HashSet<String> = HashSet::new();
            for phrase in &func.lines {
                let text = format!("{}", phrase);
                for word in text.split_whitespace() {
                    let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                    if !clean.is_empty() {
                        used_names.insert(clean.to_string());
                    }
                }
            }

            // Find unused params
            let unused: Vec<String> = param_names.difference(&used_names).cloned().collect();

            if !unused.is_empty() {
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(m.span.start..m.span.end),
                    message: format!(
                        "Function '{}' in module '{}' declares but never uses params: [{}]. \
                         Consider removing unused parameters.",
                        func.name,
                        mod_name,
                        unused.join(", ")
                    ),
                    code: 2617,
                });
            }
        }
    }
}

/// Check for duplicate sub-keys within `spec = [...]` attribute blocks.
///
/// A `spec` attribute with duplicate sub-keys (e.g., `spec = [voltage = 5V, voltage = 12V]`)
/// will silently keep only the last value, which may not be intended.
fn check_duplicate_spec_keys(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        for attr in comp.attrs.iter() {
            let key = attr.id.to_string();
            if key != "spec" {
                continue;
            }

            // Spec values are structured as McAttrVal::Attributes containing sub-attributes
            for val in &attr.values {
                if let crate::semantic::component::mc_attr::McAttrVal::Attributes(sub_attrs) = val {
                    let mut seen_keys: HashSet<String> = HashSet::new();
                    for sub in sub_attrs.iter() {
                        let sub_key = sub.id.to_string();
                        if !seen_keys.insert(sub_key.clone()) {
                            acc.push(CheckResult {
                                check_name: "extra",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: sub.key_span.clone(),
                                message: format!(
                                    "Component '{}': spec key '{}' appears multiple times. \
                                     Only the last value takes effect; earlier values are overwritten.",
                                    comp.name,
                                    sub_key
                                ),
                                code: 2618,
                            });
                        }
                    }
                }
            }
        }
    }
}
