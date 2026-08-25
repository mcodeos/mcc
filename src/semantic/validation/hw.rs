// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Hardware-specific validation checks.
//!
//! Checks:
//!   HW1 — Power pin (VCC/VDD/GND/VSS) without voltage/power attributes
//!   HW2 — Pin ID gaps in component pin definitions
//!   HW3 — Pin count extremes (too many or too few)
//!   HW5 — Interface role with dangling peer reference
//!   HW6 — Component with only single-type IO pins (all inputs, all outputs)

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::HashSet;

pub struct HwCheck;

impl ValidationCheck for HwCheck {
    fn name(&self) -> &'static str {
        "hw"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_power_pin_no_voltage(acc); // HW1
        check_pin_id_gaps(acc); // HW2
        check_pin_count_extremes(acc); // HW3
        check_role_peer_dangling(acc); // HW5
        check_single_ioc_type_component(acc); // HW6
        check_component_metadata(acc); // HW7
        check_func_param_pin_shadow(acc); // HW8
        check_unused_interface(acc); // HW9
    }
}

// ============================================================================
// HW1: Power pin without voltage/power attributes
// ============================================================================

/// Components with VCC, VDD, VSS, GND, or similar power pin names should have
/// voltage-related attributes (e.g., `voltage`, `vcc`, `vdd`, `power`) or a
/// voltage-typed parameter to document the expected operating voltage.
pub(crate) const POWER_PIN_NAMES: &[&str] = &[
    "VCC", "VDD", "VSS", "GND", "VEE", "VPP", "VBAT", "VIN", "VOUT", "VREF", "VCORE", "VAA",
    "VDDA", "VSSA", "VBUS", "VSYS",
];

/// Ground-only power pins. A component whose only power-related pins are
/// ground pins has no supply rail to document, so HW1 does not flag it.
const GROUND_PIN_NAMES: &[&str] = &["GND", "GNDA", "VSS", "VSSA"];

const VOLTAGE_ATTR_KEYS: &[&str] = &[
    "voltage",
    "volt",
    "vcc",
    "vdd",
    "vss",
    "power",
    "supply",
    "operating_voltage",
    "input_voltage",
    "output_voltage",
    "vrange",
];

fn check_power_pin_no_voltage(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Unified power-pin collection: a pin is a supply pin when it is
        // power-*typed* (`ps`/Power) or power-*named* (VCC/VREF/GND/…). Both
        // axes describe the same supply-rail concept, so they are handled by a
        // single rule (POWER_PIN_NO_VOLTAGE) instead of two overlapping checks.
        // Display name prefers a power keyword, else the pin's first name.
        let power_pins: Vec<(String, String)> = comp
            .pins
            .pins
            .iter()
            .filter(|(_, pin)| {
                let named = pin
                    .names
                    .iter()
                    .any(|n| POWER_PIN_NAMES.iter().any(|pn| n.eq_ignore_ascii_case(pn)));
                named || matches!(pin.iotype, crate::IOType::Power)
            })
            .map(|(pin_id, pin)| {
                let name = pin
                    .names
                    .iter()
                    .find(|n| POWER_PIN_NAMES.iter().any(|pn| n.eq_ignore_ascii_case(pn)))
                    .or_else(|| pin.names.first())
                    .cloned()
                    .unwrap_or_else(|| pin_id.clone());
                (pin_id.clone(), name)
            })
            .collect();

        if power_pins.is_empty() {
            continue;
        }

        // GND-only passives: a component whose only power-related pins are
        // ground pins (GND/VSS) has no supply rail to document, so the
        // voltage-attribute hint does not apply (e.g. passive mics/speakers).
        let has_supply_pin = power_pins.iter().any(|(_, name)| {
            !GROUND_PIN_NAMES
                .iter()
                .any(|g| name.eq_ignore_ascii_case(g))
        });
        if !has_supply_pin {
            continue;
        }

        // Check if component has voltage-related attributes
        let has_voltage_attr = comp.attrs.iter().any(|a| {
            let key = a.id.to_string().to_lowercase();
            VOLTAGE_ATTR_KEYS.iter().any(|vk| key.contains(vk))
        });

        // Check if component has voltage-related params (e.g., volt::UV.VOLT)
        let has_voltage_param = comp.params.iter().any(|d| {
            let pname = d.get_primary_name().unwrap_or_default().to_lowercase();
            pname.contains("volt") || pname.contains("vcc") || pname.contains("vdd")
        });

        // Check if any interface binding provides voltage info (e.g. ::DC(3.3V)).
        // The member ids (`iface.name`, e.g. `[VDD, GND]` or `vin{POWER_SYS, GND}`)
        // may not carry the class name, so also test the interface class
        // (`iface.base.name`, e.g. `DC` for `[VDD, GND]::DC()`).
        let has_voltage_iface = comp.pins.names_to_id.values().any(|port| {
            if let crate::semantic::component::mc_pins::McPinPort::Interface(ref iface) = port {
                let iname = iface.name.to_string().to_lowercase();
                let cname = iface.base.name.to_string().to_lowercase();
                if iname.contains("dc")
                    || iname.contains("power")
                    || iname.contains("supply")
                    || cname.contains("dc")
                    || cname.contains("power")
                    || cname.contains("supply")
                {
                    return true;
                }
                return iface.params.iter().any(|p| {
                    if let crate::semantic::basic::mc_param::McParamValue::UValue(uv) = p {
                        matches!(uv.unit(), crate::semantic::basic::mc_uval::McUnit::Volt)
                    } else {
                        false
                    }
                });
            }
            false
        });

        if !has_voltage_attr && !has_voltage_param && !has_voltage_iface {
            // One Info per supply pin, anchored on the pin's own name span: the
            // suggested fix (a `voltage` attribute) belongs on the supply pin,
            // so the marker lives there, not on the component name. A pin that
            // carries its own inline voltage (e.g. `volt:1.2V`) is already
            // documented and is skipped individually.
            for (pin_id, name) in &power_pins {
                let pin_has_voltage = comp.pins.pins.get(pin_id).is_some_and(|pin| {
                    pin.values.iter().any(|v| {
                        if let crate::semantic::component::mc_attr::McAttrVal::KVS(kvs) = v {
                            let key = kvs.key.to_string().to_lowercase();
                            key.contains("volt") || key.contains("vcc") || key.contains("vdd")
                        } else {
                            false
                        }
                    })
                });
                if pin_has_voltage {
                    continue;
                }
                // Anchor on the individual pin ID first: alias-style pin lists
                // (`A4 = VBUS, "VBUS"`) share one name span per name, but the
                // pin-id span (`pin_id_spans` keyed by "A4") is unique per pin,
                // so four VBUS pins each land on their own declaration line.
                let span = comp
                    .pins
                    .pin_id_spans
                    .get(pin_id)
                    .or_else(|| comp.pins.pin_name_spans.get(pin_id))
                    .or_else(|| comp.pins.pin_name_spans.get(name))
                    .cloned()
                    .filter(|s| s.end > s.start)
                    .unwrap_or_else(|| comp.span.start..comp.span.end);
                acc.push(CheckResult {
                    check_name: "hw",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(span),
                    message: format!(
                        "Component '{}': power pin '{}' ({}) has no associated \
                         voltage attribute. Consider adding e.g. `voltage = \"5V\"`.",
                        comp.name, name, pin_id
                    ),
                    code: crate::errcodes::POWER_PIN_NO_VOLTAGE,
                });
            }
        }
    }
}

// ============================================================================
// HW2: Pin ID gaps in component pin definitions
// ============================================================================

/// Components with non-sequential pin IDs (e.g., pins 1,2,3,5,6 — missing 4)
/// may indicate accidentally skipped pins or copy-paste errors. This is common
/// for NC (not-connected) pins but worth flagging for review.
fn check_pin_id_gaps(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Collect all numeric pin IDs
        let mut pin_ids: Vec<u32> = Vec::new();
        for pin_id in comp.pins.pins.keys() {
            if let Ok(num) = pin_id.parse::<u32>() {
                pin_ids.push(num);
            }
        }

        if pin_ids.len() < 3 {
            continue; // Too few pins for meaningful gap analysis
        }

        pin_ids.sort_unstable();

        // Find gaps
        let mut gaps: Vec<u32> = Vec::new();
        for window in pin_ids.windows(2) {
            let curr = window[0];
            let next = window[1];
            if next > curr + 1 {
                for missing in (curr + 1)..next {
                    gaps.push(missing);
                }
            }
        }

        // Only report if there are a reasonable number of gaps
        // (1-2 gaps in a large component is normal for NC pins)
        let total_pins = pin_ids.len();
        let gap_count = gaps.len();

        if gap_count > 0 && (gap_count as f64 / total_pins as f64) > 0.05 {
            let gap_list: Vec<String> = gaps.iter().take(10).map(|g| g.to_string()).collect();
            let suffix = if gaps.len() > 10 {
                format!(" ... and {} more", gaps.len() - 10)
            } else {
                String::new()
            };

            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has {} pin ID gap(s) ({} of {} pins): {}{}. \
                     These may be intentional NC pins or could indicate missing definitions.",
                    comp.name,
                    gap_count,
                    gap_count,
                    total_pins,
                    gap_list.join(", "),
                    suffix
                ),
                code: crate::errcodes::HW_PIN_NUMBER_GAP,
            });
        }
    }
}

// ============================================================================
// HW3: Pin count extremes
// ============================================================================

/// Components with unusually many pins (>300) or zero pins (not abstract)
/// deserve a second look. Extremely high pin counts may indicate a data error;
/// zero-pin components should probably be abstract or use an interface instead.
fn check_pin_count_extremes(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        let pin_count = comp.pins.pins.len();

        // HW3a: Too many pins (likely a large BGA or data error)
        if pin_count > 300 {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has {} pins. Verify this is correct — \
                     high pin counts may indicate a data entry error.",
                    comp.name, pin_count
                ),
                code: crate::errcodes::HW_PIN_COUNT_HIGH,
            });
        }

        // HW3b: Zero pins but not abstract (has params or attrs suggesting it should have pins)
        // Skip components with dynamic pin definitions (§2.20) — their pins
        // are resolved at instantiation time, so the template has 0 static pins.
        if pin_count == 0
            && !comp.pins.has_dynamic_pins()
            && !comp.params.is_empty()
            && !comp.attrs.is_empty()
            && comp.funcs.is_empty()
        {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has 0 pins but has params and attributes. \
                     Is this an abstract component? Consider adding a pin definition \
                     or marking it as abstract.",
                    comp.name
                ),
                code: crate::errcodes::HW_ZERO_PINS_WITH_PARAMS,
            });
        }
    }
}

// ============================================================================
// HW5: Interface role with dangling peer reference
// ============================================================================

/// An interface role that specifies a `peer` relationship should have a
/// corresponding peer role defined in the same interface. A dangling peer
/// reference indicates an incomplete interface definition.
fn check_role_peer_dangling(acc: &mut CheckAccumulator) {
    let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
    for entry in ifaces.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let iface = entry.value();

        // Collect all role names in this interface
        let role_names: HashSet<String> = iface.roles.iter().map(|r| r.name.to_string()).collect();

        for role in &iface.roles {
            // Check if role has a peer attr referencing another role
            for attr in &role.attrs {
                let key = attr.id.to_string().to_lowercase();
                if key == "peer" {
                    for val in &attr.values {
                        let peer_name = format!("{}", val).trim().to_string();
                        if !peer_name.is_empty() && !role_names.contains(&peer_name) {
                            acc.push(CheckResult {
                                check_name: "hw",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: attr.key_span.clone(),
                                message: format!(
                                    "Interface '{}': role '{}' references peer '{}' \
                                     which is not defined in this interface. \
                                     Available roles: {}",
                                    iface.name,
                                    role.name,
                                    peer_name,
                                    if role_names.is_empty() {
                                        "(none)".to_string()
                                    } else {
                                        role_names
                                            .iter()
                                            .map(|s| s.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    }
                                ),
                                code: crate::errcodes::HW_IFACE_ROLE_UNBOUND,
                            });
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// HW6: Component with only single-type IO pins
// ============================================================================

/// A component where ALL pins share the same IO type (all Input, all Output,
/// or all Power) is unusual. Most real components have a mix of input,
/// output, and power pins. A single-type component may indicate incomplete
/// pin definitions or a misclassified component.
fn check_single_ioc_type_component(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        let pin_count = comp.pins.pins.len();
        if pin_count < 3 {
            continue; // Too few pins to make this meaningful
        }

        use crate::IOType;
        let mut has_in = false;
        let mut has_out = false;
        let mut has_ps = false;
        let mut has_anl = false;
        let mut has_nc = false;
        let mut has_io = false;

        for pin in comp.pins.pins.values() {
            match pin.iotype {
                IOType::In => has_in = true,
                IOType::Out => has_out = true,
                IOType::Power => has_ps = true,
                IOType::Analog => has_anl = true,
                IOType::NonCon => has_nc = true,
                IOType::InOut => has_io = true,
                IOType::Return | IOType::None | IOType::Label => {} // these don't indicate direction
            }
        }

        let active_types = [has_in, has_out, has_ps, has_anl, has_nc, has_io]
            .iter()
            .filter(|&&x| x)
            .count();

        // If all pins are the same active type (excluding passive), that's unusual
        if active_types == 1 && pin_count >= 4 {
            // A chip whose every pin is a power pin (e.g. an LDO like AMS1117
            // with IN/OUT/ADJ/GND) is the normal shape of a power component,
            // not an incomplete definition — don't flag it.
            if has_ps {
                return;
            }
            let io_desc = if has_in {
                "Input"
            } else if has_out {
                "Output"
            } else if has_anl {
                "Analog"
            } else {
                return; // NC-only or passive-only, skip
            };

            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}': all {} pins are type '{}'. \
                     Most components have mixed IO types (input, output, power). \
                     Verify the pin definitions are complete.",
                    comp.name, pin_count, io_desc
                ),
                code: crate::errcodes::HW_ALL_SAME_IO_TYPE,
            });
        }
    }
}

// ============================================================================
// HW7: Component metadata completeness
// ============================================================================

/// Every component should ideally have a `description` attribute.
/// Missing metadata makes library browsing and BOM generation harder.
fn check_component_metadata(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        let has_name = comp.attrs.iter().any(|a| a.id.to_string() == "name");
        let has_desc = comp.attrs.iter().any(|a| a.id.to_string() == "description");

        if has_name && !has_desc && comp.pins.pins.len() > 2 {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Hint,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has a name but no 'description' attribute. \
                     Adding a description helps library maintainability.",
                    comp.name
                ),
                code: crate::errcodes::HW_NAME_WITHOUT_DESC,
            });
        }
    }
}

// ============================================================================
// HW8: Function parameter shadows a component pin name
// ============================================================================

/// When a component function declares a parameter with the same name as a
/// component pin, it creates ambiguity in net expressions. The function
/// parameter may unintentionally shadow the pin reference.
fn check_func_param_pin_shadow(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Collect all pin names
        let pin_names: HashSet<String> = comp.pins.names_to_id.keys().cloned().collect();

        if pin_names.is_empty() || comp.funcs.is_empty() {
            continue;
        }

        for func in comp.funcs.iter() {
            for d in func.params.iter() {
                if let Some(pname) = d.get_primary_name() {
                    if pin_names.contains(&pname) {
                        acc.push(CheckResult {
                            check_name: "hw",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}': function '{}' param '{}' shadows a pin name. \
                                 This may cause ambiguity in net expressions within the function body.",
                                comp.name,
                                func.name,
                                pname
                            ),
                            code: crate::errcodes::HW_FUNC_PARAM_SHADOWS_PIN,
                        });
                    }
                }
            }
        }
    }
}

// ============================================================================
// HW9: Unused interface — defined but never bound by any component
// ============================================================================

/// An interface that is defined in the workspace but never referenced by
/// any component's pin bindings is dead code. It may indicate an incomplete
/// component definition or an obsolete interface.
fn check_unused_interface(acc: &mut CheckAccumulator) {
    let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
    let comps = &crate::db::cmie::tables::WORKSPACE.components;

    // Collect all interface names that are bound by at least one component
    let mut used_ifaces: HashSet<String> = HashSet::new();
    for entry in comps.iter() {
        let comp = entry.value();
        for (_pin_name, port) in &comp.pins.names_to_id {
            if let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port {
                used_ifaces.insert(iface.name.to_string());
            }
        }
        // Also check param type declarations
        for d in comp.params.iter() {
            if let Some(class_name) = d.get_class_name() {
                used_ifaces.insert(class_name);
            }
        }
    }

    for entry in ifaces.iter() {
        let iface = entry.value();
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        if !used_ifaces.contains(&name) {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(iface.span.start..iface.span.end),
                message: format!(
                    "Interface '{}' is defined but never bound by any component. \
                     Consider using it in a component definition or removing it.",
                    name
                ),
                code: crate::errcodes::HW_IFACE_NEVER_BOUND,
            });
        }
    }
}
