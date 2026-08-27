/// Extra checks: H3, I1, I3, J3, N5, N6, U1, U4, U5
use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
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

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        // Collect library names for J3 shadow detection
        let lib_names: HashSet<String> = {
            let mut s = HashSet::new();
            let comps = &crate::db::cmie::tables::WORKSPACE.components;
            for e in comps.iter() {
                s.insert(e.key().ident.to_string());
            }
            let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
            for e in ifaces.iter() {
                s.insert(e.key().ident.to_string());
            }
            let enums = &crate::db::cmie::tables::WORKSPACE.enums;
            for e in enums.iter() {
                s.insert(e.key().ident.to_string());
            }
            s
        };

        // J3: user port/instance names that shadow library CMIE names
        {
            let modules = &crate::db::cmie::tables::WORKSPACE.modules;
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
                            code: crate::errcodes::NAME_PORT_SHADOWS_CMIE,
                        });
                    }
                }
            }
        }

        // U1: enums with only one value
        {
            let enums = &crate::db::cmie::tables::WORKSPACE.enums;
            for entry in enums.iter() {
                let e = entry.value();
                if e.values.len() == 1 {
                    acc.push(CheckResult {
                        check_name: "extra",
                        severity: CheckSeverity::Info,
                        uri: Some(entry.key().uri.to_string()),
                        span: Some(e.span[0] as usize..e.span[1] as usize),
                        message: format!("Enum '{}' has only one value.", e.name),
                        code: crate::errcodes::ENUM_SINGLE_VALUE,
                    });
                }
            }
        }

        // N5 + R8: default value type/unit mismatch
        check_default_type_mismatch(acc);

        // H3: overlapping pin ranges (deferred — needs better cross-line vs
        //     same-line distinction; no implementation yet)
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
        check_default_value_range(acc); // B7
        check_duplicate_spec_keys(acc); // spec sub-key uniqueness
    }
}

/// R4: functions with empty bodies (module + component funcs).
fn check_empty_functions(acc: &mut CheckAccumulator) {
    // Module funcs
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        for func in entry.value().funcs.iter() {
            if func.stmts.is_empty() && func.insts.is_empty() {
                let func_span = func.span.clone().unwrap_or(m.span.start..m.span.end);
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(func_span),
                    message: format!("Function '{}' has an empty body.", func.name),
                    code: crate::errcodes::FUNC_EMPTY_BODY,
                });
            }
        }
    }
    // Component funcs
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for func in entry.value().funcs.iter() {
            if func.stmts.is_empty() && func.insts.is_empty() {
                let func_span = func.span.clone().unwrap_or(comp.span.start..comp.span.end);
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(func_span),
                    message: format!(
                        "Function '{}' in component '{}' has an empty body.",
                        func.name,
                        entry.key().ident
                    ),
                    code: crate::errcodes::FUNC_EMPTY_BODY,
                });
            }
        }
    }
}

/// I4: interface pin count mismatch (physical pins vs interface definition).
fn check_interface_pin_counts(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        for (pin_name, port) in &comp.pins.names_to_id {
            if let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port {
                let iface_name = iface.name.to_string();
                // Real member pins in source declaration order; raw names_to_id
                // keys additionally contain list-form binding names (e.g. the
                // `[VBUS, GND]` key from `[1,5] = [VBUS, GND]::DC(5V)`) that a
                // component can never bind, inflating the required count.
                let iface_pin_count = iface.base.pins.member_names().len();
                // Check each physical pin binding
                let phys_pins: Vec<&String> =
                    comp.pins.pin_id_to_names.values().flatten().collect();
                // Count how many physical pins are bound to this interface name.
                // Interface members are stored as "I2C0.SCL", "I2C0.SDA" in
                // pin_id_to_names, so we match both exact and dot-prefixed forms.
                // For list-form names like [VDD, GND], match the list members.
                let bound_count = if iface.name.is_list() {
                    if let Some(members) = iface.name.list_members() {
                        phys_pins
                            .iter()
                            .filter(|n| members.contains(&n.to_string()))
                            .count()
                    } else {
                        0
                    }
                } else {
                    let dot_prefix = format!("{}.", pin_name);
                    phys_pins
                        .iter()
                        .filter(|n| n.as_str() == pin_name || n.as_str().starts_with(&dot_prefix))
                        .count()
                };
                if bound_count < iface_pin_count {
                    // Use the specific pin name span when available; fall back to comp span.
                    let span = comp
                        .pins
                        .pin_name_spans
                        .get(pin_name)
                        .cloned()
                        .unwrap_or_else(|| comp.span.start..comp.span.end);
                    acc.push(CheckResult {
                        check_name: "extra", severity: CheckSeverity::Warning,
                        uri: Some(uri.clone()), span: Some(span),
                        message: format!(
                            "Interface '{}' expects {} pins but only {} physical pins bound as '{}'.",
                            iface_name, iface_pin_count, bound_count, pin_name
                        ),
                        code: crate::errcodes::IFACE_PIN_COUNT_MISMATCH,
                    });
                }
            }
        }
    }
}

/// M1: components with no params, no pins, no attrs, no funcs.
/// M3: components without pins.
fn check_component_structure(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        let name = entry.key().ident.to_string();
        let has_params = !comp.params.is_empty();
        let has_pins = !comp.pins.names_to_id.is_empty() || comp.pins.has_dynamic_pins();
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
                code: crate::errcodes::COMPONENT_EMPTY,
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
                code: crate::errcodes::COMPONENT_NO_PINS,
            });
        }
    }
}

/// M4: interfaces without pins.
fn check_interface_structure(acc: &mut CheckAccumulator) {
    let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
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
                code: crate::errcodes::INTERFACE_EMPTY,
            });
        }
    }
}

/// N5 + R8: default value type mismatch for typed parameters.
fn check_default_type_mismatch(acc: &mut CheckAccumulator) {
    use crate::semantic::basic::mc_param_type::McParamTypeKind;
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
                                code: crate::errcodes::PARAM_INT_DEFAULT_STRING,
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
                                code: crate::errcodes::PARAM_STRING_DEFAULT_NUMERIC,
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
                                code: crate::errcodes::PARAM_UV_DEFAULT_NO_UNIT,
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
    let defines = &crate::db::cmie::tables::WORKSPACE.defines;
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
                code: crate::errcodes::DEFINE_NO_ATTRS,
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
                        code: crate::errcodes::DEFINE_NON_ATTR_CLAUSE,
                    });
                    break;
                }
            }
        }
    }
}

/// D2: unresolved instance class name.
fn check_instance_class_found(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let m = entry.value();
        for (name, (_, inst)) in m.insts.insts() {
            if let crate::McInstance::Unresolved { class_name } = inst {
                // Anchor on the instance declaration itself (the name span
                // registered at parse), not the whole module: `BUTTON SW1`
                // must light up on `SW1`, not on the module name.
                let span = m
                    .insts
                    .get_port_span(name)
                    .unwrap_or_else(|| m.span.clone());
                acc.push(CheckResult {
                    check_name: "extra",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: Some(span),
                    message: format!(
                        "Instance '{}' references class '{}' that is not loaded.",
                        name, class_name
                    ),
                    code: crate::errcodes::INST_CLASS_NOT_LOADED,
                });
            }
        }
    }
}

/// D3: bus member collision — two instances/buses with same base name,
/// conflicting or duplicate member names.
fn check_bus_member_collision(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
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
                                    code: crate::errcodes::BUS_DUPLICATE_MEMBER,
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
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
            func.stmts.len().hash(&mut h);
            // Hash the McPhrase Display output as a body fingerprint
            for stmt in &func.stmts {
                format!("{}", stmt).hash(&mut h);
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
                    code: crate::errcodes::COMPONENT_DUPLICATE_FUNC_BODY,
                });
            }
        }
    }
}

/// F2: naming convention — UPPER_SNAKE for components/interfaces/enums.
fn check_naming_convention(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
                        code: crate::errcodes::COMPONENT_MIXED_CASE,
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
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
                        code: crate::errcodes::PARAM_RESERVED_KEYWORD,
                    });
                }
            }
        }
    }
}

/// R5: function name conflicts with a port/instance name in the same module.
fn check_func_name_conflict(acc: &mut CheckAccumulator) {
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
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
                    code: crate::errcodes::FUNC_SHARES_NAME_WITH_PORT,
                });
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
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
                                code: crate::errcodes::PARAM_NEGATIVE_DEFAULT,
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
                                    code: crate::errcodes::PARAM_FLOAT_DEFAULT_INVALID,
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

/// Check for duplicate sub-keys within `spec = [...]` attribute blocks.
///
/// A `spec` attribute with duplicate sub-keys (e.g., `spec = [voltage = 5V, voltage = 12V]`)
/// will silently keep only the last value, which may not be intended.
fn check_duplicate_spec_keys(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
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
                                code: crate::errcodes::SPEC_KEY_DUPLICATE,
                            });
                        }
                    }
                }
            }
        }
    }
}
