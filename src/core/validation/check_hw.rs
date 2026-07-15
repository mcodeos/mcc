// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Hardware-specific validation checks.
//!
//! Checks:
//!   HW1 — Power pin (VCC/VDD/GND/VSS) without voltage/power attributes
//!   HW2 — Pin ID gaps in component pin definitions
//!   HW3 — Pin count extremes (too many or too few)
//!   HW4 — Suspect NC pin pattern (multiple consecutive NC pins)
//!   HW5 — Interface role with dangling peer reference
//!   HW6 — Component with only single-type IO pins (all inputs, all outputs)

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};
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

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        check_power_pin_no_voltage(acc); // HW1
        check_pin_id_gaps(acc); // HW2
        check_pin_count_extremes(acc); // HW3
        check_consecutive_nc_pins(acc); // HW4
        check_role_peer_dangling(acc); // HW5
        check_single_ioc_type_component(acc); // HW6
    }
}

// ============================================================================
// HW1: Power pin without voltage/power attributes
// ============================================================================

/// Components with VCC, VDD, VSS, GND, or similar power pin names should have
/// voltage-related attributes (e.g., `voltage`, `vcc`, `vdd`, `power`) or a
/// voltage-typed parameter to document the expected operating voltage.
const POWER_PIN_NAMES: &[&str] = &[
    "VCC", "VDD", "VSS", "GND", "VEE", "VPP", "VBAT", "VIN", "VOUT", "VREF", "VCORE", "VAA",
    "VDDA", "VSSA", "VBUS", "VSYS",
];

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
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Check if component has power-named pins
        let has_power_pin = comp.pins.names_to_id.keys().any(|name| {
            POWER_PIN_NAMES
                .iter()
                .any(|pn| name.eq_ignore_ascii_case(pn))
        });

        if !has_power_pin {
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

        if !has_voltage_attr && !has_voltage_param {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: None,
                message: format!(
                    "Component '{}' has power-related pins ({}) but no voltage attribute \
                     or voltage-typed parameter. Consider adding e.g. `voltage = \"5V\"` \
                     or a `volt::UV.VOLT` parameter.",
                    comp.name,
                    comp.pins
                        .names_to_id
                        .keys()
                        .filter(|n| POWER_PIN_NAMES.iter().any(|pn| n.eq_ignore_ascii_case(pn)))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                code: 3301,
            });
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
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
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
                span: None,
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
                code: 3302,
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
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
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
                span: None,
                message: format!(
                    "Component '{}' has {} pins. Verify this is correct — \
                     high pin counts may indicate a data entry error.",
                    comp.name, pin_count
                ),
                code: 3303,
            });
        }

        // HW3b: Zero pins but not abstract (has params or attrs suggesting it should have pins)
        if pin_count == 0
            && !comp.params.is_empty()
            && !comp.attrs.is_empty()
            && comp.funcs.is_empty()
        {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: None,
                message: format!(
                    "Component '{}' has 0 pins but has params and attributes. \
                     Is this an abstract component? Consider adding a pin definition \
                     or marking it as abstract.",
                    comp.name
                ),
                code: 3304,
            });
        }
    }
}

// ============================================================================
// HW4: Suspect NC pin pattern (multiple consecutive NC pins)
// ============================================================================

/// Three or more consecutive NC (not-connected) pins in a component may
/// indicate an incorrectly copied pin table or missing assignments.
/// NC pins are normal (e.g., thermal pads, reserved pins) but clusters
/// deserve review.
fn check_consecutive_nc_pins(acc: &mut CheckAccumulator) {
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();

        // Collect pins sorted by numeric ID, tracking NC status
        let mut sorted_pins: Vec<(u32, bool)> = Vec::new(); // (pin_id, is_nc)
        for (pin_id, pin) in &comp.pins.pins {
            if let Ok(num) = pin_id.parse::<u32>() {
                let is_nc =
                    pin.names.is_empty() || pin.names.iter().all(|n| n == "NC" || n == "nc");
                sorted_pins.push((num, is_nc));
            }
        }
        sorted_pins.sort_by_key(|(id, _)| *id);

        // Find runs of 3+ consecutive NC pins
        let mut run_start: Option<u32> = None;
        let mut run_count = 0u32;

        for (id, is_nc) in &sorted_pins {
            if *is_nc {
                if run_start.is_none() {
                    run_start = Some(*id);
                }
                run_count += 1;
            } else {
                if run_count >= 3 {
                    if let Some(start) = run_start {
                        acc.push(CheckResult {
                            check_name: "hw",
                            severity: CheckSeverity::Info,
                            uri: Some(uri.clone()),
                            span: None,
                            message: format!(
                                "Component '{}' has {} consecutive NC pins starting at pin {}. \
                                 Verify these are intentional (e.g., reserved/test points).",
                                comp.name, run_count, start
                            ),
                            code: 3305,
                        });
                    }
                }
                run_start = None;
                run_count = 0;
            }
        }
        // Check trailing run
        if run_count >= 3 {
            if let Some(start) = run_start {
                acc.push(CheckResult {
                    check_name: "hw",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: None,
                    message: format!(
                        "Component '{}' has {} consecutive NC pins starting at pin {}. \
                         Verify these are intentional.",
                        comp.name, run_count, start
                    ),
                    code: 3305,
                });
            }
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
    let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
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
                                code: 3306,
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
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
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
                IOType::Return | IOType::None => {} // these don't indicate direction
            }
        }

        let active_types = [has_in, has_out, has_ps, has_anl, has_nc, has_io]
            .iter()
            .filter(|&&x| x)
            .count();

        // If all pins are the same active type (excluding passive), that's unusual
        if active_types == 1 && pin_count >= 4 {
            let io_desc = if has_in {
                "Input"
            } else if has_out {
                "Output"
            } else if has_ps {
                "Power"
            } else if has_anl {
                "Analog"
            } else {
                return; // NC-only or passive-only, skip
            };

            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: None,
                message: format!(
                    "Component '{}': all {} pins are type '{}'. \
                     Most components have mixed IO types (input, output, power). \
                     Verify the pin definitions are complete.",
                    comp.name, pin_count, io_desc
                ),
                code: 3307,
            });
        }
    }
}
