// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 Electrical Net Checks — driver conflict, floating inputs, voltage mismatch, etc.
//!
//! Runs after `mcb_pass2()` when the full flattened netlist (`InstTable`) is available.

use crate::core::common::IOType;
use crate::instant::inst_table::{InstEntry, InstTable, NetEntry};
use std::collections::HashSet;
/// Run all electrical net checks and return diagnostics.
pub fn run_net_checks(table: &InstTable) -> Vec<NetCheckResult> {
    let mut results = Vec::new();
    check_driver_conflict(table, &mut results); // P1
    check_undriven_nets(table, &mut results); // P2
    check_floating_inputs(table, &mut results); // P5
    check_nc_connected(table, &mut results); // P6
    check_unconnected_outputs(table, &mut results); // P7
    check_backfeed(table, &mut results); // P8
    check_unwired_instances(table, &mut results); // P9
    check_voltage_mismatch(table, &mut results); // P3+P4
    check_port_io_mismatch(table, &mut results); // V1
    check_power_nets(table, &mut results); // net count summary
    check_unused_module_ports(table, &mut results); // C4
    check_single_point_nets(table, &mut results); // self-loop
    check_pin_count_mismatch(table, &mut results); // pin count vs definition
    check_floating_outputs(table, &mut results); // output variant of P5
    results
}

#[derive(Debug, Clone)]
pub struct NetCheckResult {
    pub check: &'static str,
    pub severity: &'static str, // "error" | "warning"
    pub message: String,
    pub net_name: String,
    pub code: u32,
}

// ── P1: Multiple outputs driving the same net ──
fn check_driver_conflict(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let outputs: Vec<&InstEntry> = net
            .points
            .iter()
            .filter_map(|id| table.get_entry(*id))
            .filter(|e| matches!(e.io_type, IOType::Out | IOType::Power))
            .collect();
        if outputs.len() > 1 {
            let names: Vec<_> = outputs.iter().map(|e| e.path.as_str()).collect();
            results.push(NetCheckResult {
                check: "driver-conflict",
                severity: "error",
                message: format!(
                    "Net '{}' has {} drivers: {}. Possible short circuit.",
                    net.name,
                    outputs.len(),
                    names.join(", ")
                ),
                net_name: net.name.clone(),
                code: 3101,
            });
        }
    }
}

// ── P2: Nets with only input endpoints (no driver) ──
fn check_undriven_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let points: Vec<&InstEntry> = net
            .points
            .iter()
            .filter_map(|id| table.get_entry(*id))
            .collect();
        let has_driver = points
            .iter()
            .any(|e| matches!(e.io_type, IOType::Out | IOType::Power));
        let has_input = points
            .iter()
            .any(|e| matches!(e.io_type, IOType::In | IOType::InOut));
        if !has_driver && has_input && !points.is_empty() {
            results.push(NetCheckResult {
                check: "undriven-net",
                severity: "warning",
                message: format!("Net '{}' has inputs but no output/power driver.", net.name),
                net_name: net.name.clone(),
                code: 3102,
            });
        }
    }
}

// ── P5: Input ports with no net connection ──
fn check_floating_inputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        if matches!(entry.io_type, IOType::In) && !connected.contains(&entry.id) {
            results.push(NetCheckResult {
                check: "floating-input",
                severity: "warning",
                message: format!("Input '{}' is not connected to any net.", entry.path),
                net_name: entry.path.clone(),
                code: 3105,
            });
        }
    }
}

// ── P6: NC port connected to a net ──
fn check_nc_connected(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        for id in &net.points {
            if let Some(entry) = table.get_entry(*id) {
                if matches!(entry.io_type, IOType::NonCon) {
                    results.push(NetCheckResult {
                        check: "nc-connected",
                        severity: "warning",
                        message: format!(
                            "NC port '{}' is connected to net '{}'.",
                            entry.path, net.name
                        ),
                        net_name: net.name.clone(),
                        code: 3106,
                    });
                }
            }
        }
    }
}

// ── P7: Output ports with no net connection ──
fn check_unconnected_outputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        if matches!(entry.io_type, IOType::Out) && !connected.contains(&entry.id) {
            results.push(NetCheckResult {
                check: "unconnected-output",
                severity: "warning",
                message: format!("Output '{}' drives nothing.", entry.path),
                net_name: entry.path.clone(),
                code: 3107,
            });
        }
    }
}

// ── P3+P4: Voltage mismatch between connected power nets ──
fn check_voltage_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Extract voltage hints from net names (e.g., "VCC_5V" → 5.0, "VCC_3V3" → 3.3)
    fn parse_voltage(name: &str) -> Option<f32> {
        let name = name.to_uppercase();
        if let Some(v) = name
            .strip_prefix("VCC_")
            .or_else(|| name.strip_prefix("VDD_"))
        {
            let v = v.replace('V', ".").replace("_", ".");
            if let Ok(val) = v.parse::<f32>() {
                return Some(val);
            }
        }
        // Direct voltage suffix: "5V", "3V3", "1V8"
        if let Some(idx) = name.find('V') {
            let prefix = &name[..idx];
            let clean = prefix
                .replace('_', "")
                .replace("VCC", "")
                .replace("VDD", "");
            if !clean.is_empty() {
                let clean = clean.replace("V", ".").trim_matches('.').to_string();
                if let Ok(val) = clean.parse::<f32>() {
                    return Some(val);
                }
            }
        }
        None
    }

    let mut net_voltages: Vec<(&NetEntry, f32)> = Vec::new();
    for net in table.get_nets() {
        if let Some(v) = parse_voltage(&net.name) {
            net_voltages.push((net, v));
        }
    }
    // Check for voltage differences on connected paths (approximate — just compare names)
    for i in 0..net_voltages.len() {
        for j in i + 1..net_voltages.len() {
            let (n1, v1) = net_voltages[i];
            let (n2, v2) = net_voltages[j];
            if (v1 - v2).abs() > 0.5 && has_shared_point(table, n1, n2) {
                results.push(NetCheckResult {
                    check: "voltage-mismatch",
                    severity: "error",
                    message: format!(
                        "Power nets '{}' ({}V) and '{}' ({}V) may be shorted.",
                        n1.name, v1, n2.name, v2
                    ),
                    net_name: format!("{}+{}", n1.name, n2.name),
                    code: 3103,
                });
            }
        }
    }
}

fn has_shared_point(table: &InstTable, n1: &NetEntry, n2: &NetEntry) -> bool {
    let p1: HashSet<u32> = n1.points.iter().cloned().collect();
    n2.points.iter().any(|id| p1.contains(id))
}

// ── P9: Component instances with no pins connected to any net ──
fn check_unwired_instances(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        if matches!(entry.kind, crate::instant::inst_table::InstKind::Component)
            && !entry.class_name.is_empty()
        {
            let pins = table.get_pins_of(entry.id);
            if !pins.is_empty() && pins.iter().all(|p| !connected.contains(&p.id)) {
                results.push(NetCheckResult {
                    check: "unwired-instance",
                    severity: "warning",
                    message: format!(
                        "Instance '{}' has no pins connected to any net.",
                        entry.path
                    ),
                    net_name: entry.path.clone(),
                    code: 3109,
                });
            }
        }
    }
}

// ── P8: Output connected to PowerSupply (backfeed risk) ──
fn check_backfeed(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let has_out = net.points.iter().any(|id| {
            table
                .get_entry(*id)
                .map_or(false, |e| matches!(e.io_type, IOType::Out))
        });
        let has_ps = net.points.iter().any(|id| {
            table
                .get_entry(*id)
                .map_or(false, |e| matches!(e.io_type, IOType::Power))
        });
        if has_out && has_ps {
            results.push(NetCheckResult {
                check: "backfeed-risk",
                severity: "warning",
                message: format!(
                    "Net '{}' has both output and power supply. Backfeed risk.",
                    net.name
                ),
                net_name: net.name.clone(),
                code: 3108,
            });
        }
    }
}

// ── V1: Module ports with mismatched IO directions on same net ──
fn check_port_io_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let mut has_in = false;
        let mut has_out = false;
        let mut has_ps = false;
        let mut out_count = 0u32;
        for id in &net.points {
            if let Some(e) = table.get_entry(*id) {
                has_in |= matches!(e.io_type, IOType::In);
                has_out |= matches!(e.io_type, IOType::Out);
                has_ps |= matches!(e.io_type, IOType::Power);
                if matches!(e.io_type, IOType::Out) {
                    out_count += 1;
                }
            }
        }
        if out_count > 1 && !has_in && has_ps {
            results.push(NetCheckResult {
                check: "port-io-mismatch",
                severity: "warning",
                message: format!(
                    "Net '{}' has {} outputs and power but no input.",
                    net.name, out_count
                ),
                net_name: net.name.clone(),
                code: 3110,
            });
        }
    }
}

// ── Power net summary ──
fn check_power_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let mut count = 0u32;
    for net in table.get_nets() {
        for id in &net.points {
            if let Some(e) = table.get_entry(*id) {
                if matches!(e.io_type, IOType::Power) {
                    count += 1;
                    break;
                }
            }
        }
    }
    if count > 10 {
        results.push(NetCheckResult {
            check: "power-net-count",
            severity: "info",
            message: format!("Design has {} power nets. Review for consolidation.", count),
            net_name: String::new(),
            code: 3199,
        });
    }
}

// ── C4: Module boundary ports not connected to any net ──
fn check_unused_module_ports(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    let top_id = table
        .iter()
        .find(|(_, e)| {
            matches!(e.kind, crate::instant::inst_table::InstKind::Module) && e.parent_id.is_none()
        })
        .map(|(id, _)| *id);
    for (_, entry) in table.iter() {
        // Check module boundary ports (not internal pins)
        if entry.parent_id == top_id || entry.parent_id.is_none() {
            continue;
        }
        if matches!(
            entry.io_type,
            IOType::In | IOType::Out | IOType::InOut | IOType::Power
        ) && !connected.contains(&entry.id)
            && !entry.class_name.is_empty()
        {
            results.push(NetCheckResult {
                check: "unused-module-port",
                severity: "warning",
                message: format!(
                    "Module port '{}' ({:?}) is not connected to any net.",
                    entry.path, entry.io_type
                ),
                net_name: entry.path.clone(),
                code: 3111,
            });
        }
    }
}

// ── Single-point nets (self-loop or isolated point) ──
fn check_single_point_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        if net.points.len() == 1 {
            if let Some(entry) = table.get_entry(net.points[0]) {
                results.push(NetCheckResult {
                    check: "single-point-net",
                    severity: "warning",
                    message: format!(
                        "Net '{}' has only one endpoint: '{}'. Possible dangling connection.",
                        net.name, entry.path
                    ),
                    net_name: net.name.clone(),
                    code: 3112,
                });
            }
        }
    }
}

// ── Pin count mismatch: instance has fewer connected pins than component defines ──
fn check_pin_count_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    let comps = crate::builder::workspace::WORKSPACE.components.borrow();
    for (_, entry) in table.iter() {
        if !matches!(entry.kind, crate::instant::inst_table::InstKind::Component)
            || entry.class_name.is_empty()
        {
            continue;
        }
        if let Some(def_entry) = comps
            .iter()
            .find(|e| e.key().ident.to_string() == entry.class_name)
        {
            let def_pin_count = def_entry.value().pins.names_to_id.len();
            if def_pin_count == 0 {
                continue;
            }
            let pins = table.get_pins_of(entry.id);
            let connected_pins = pins.iter().filter(|p| connected.contains(&p.id)).count();
            if connected_pins < def_pin_count {
                results.push(NetCheckResult {
                    check: "pin-count-mismatch",
                    severity: "warning",
                    message: format!(
                        "'{}' has {} of {} pins connected.",
                        entry.path, connected_pins, def_pin_count
                    ),
                    net_name: entry.path.clone(),
                    code: 3113,
                });
            }
        }
    }
}

// ── Floating outputs (output variant of floating input check) ──
fn check_floating_outputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        if matches!(entry.io_type, IOType::InOut) && !connected.contains(&entry.id) {
            results.push(NetCheckResult {
                check: "floating-bidirectional",
                severity: "warning",
                message: format!(
                    "Bidirectional port '{}' is not connected to any net.",
                    entry.path
                ),
                net_name: entry.path.clone(),
                code: 3114,
            });
        }
    }
}
