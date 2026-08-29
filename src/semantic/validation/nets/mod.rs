// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 Electrical Net Checks — driver conflict, floating inputs, voltage mismatch, etc.
//!
//! Runs after `mcb_pass2()` when the full flattened netlist (`InstTable`) is available.

use crate::instant::insttab::{
    is_ground_name, is_supply_name, InstEntry, InstKind, InstOrigin, InstTable, MemberRole,
    NetEntry,
};
use crate::semantic::basic::mc_kvs::KVSValue;
use crate::semantic::basic::mc_literal::McLiteral;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::common::IOType;
use crate::semantic::component::mc_attr::McAttrVal;
use crate::semantic::component::mc_pins::McPinPort;
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
    check_pullup_degenerate(table, &mut results); // D7 (network-level)
    results
}

#[derive(Debug, Clone)]
pub struct NetCheckResult {
    pub check: &'static str,
    pub severity: &'static str, // "error" | "warning" | "info"
    pub message: String,
    pub net_name: String,
    pub code: u32,
    /// Source byte offset of the relevant point (0 if not available)
    pub pos: u32,
    /// Source file URI (empty if not available)
    pub uri: String,
}

/// Extract the best available source position from an InstEntry.
/// `src_pos` is the wiring site (preferred); `fallback_pos` is the declaration
/// site used for unconnected pins/ports; `(0, uri)` is the last resort.
fn entry_pos(entry: &InstEntry) -> (u32, String) {
    if let Some(p) = &entry.src_pos {
        return (p.offset, p.uri.clone());
    }
    if let Some(p) = &entry.fallback_pos {
        return (p.offset, p.uri.clone());
    }
    (0, entry.def_uri.clone())
}

/// §2.19 OR semantics: an entry is NC if its iotype is `NonCon` (the `nc`
/// prefix) or its class name is "NC"/"nc" (case-insensitive) — whichever
/// declaration is used, the pin is intentionally unconnected.
fn is_nc_entry(entry: &InstEntry) -> bool {
    matches!(entry.io_type, IOType::NonCon) || entry.class_name.eq_ignore_ascii_case("nc")
}

/// Find the first InstEntry that has a source position among a set of point IDs.
fn best_pos(table: &InstTable, ids: &[u32]) -> (u32, String) {
    for id in ids {
        if let Some(entry) = table.get_entry(*id) {
            if let Some(p) = &entry.src_pos {
                return (p.offset, p.uri.clone());
            }
        }
    }
    // Fallback: any entry — prefer a declaration (fallback) position over (0, uri)
    for id in ids {
        if let Some(entry) = table.get_entry(*id) {
            if let Some(p) = &entry.fallback_pos {
                return (p.offset, p.uri.clone());
            }
        }
    }
    for id in ids {
        if let Some(entry) = table.get_entry(*id) {
            if !entry.def_uri.is_empty() {
                return (0, entry.def_uri.clone());
            }
        }
    }
    (0, String::new())
}

// ── P1: Multiple outputs driving the same net ──
fn check_driver_conflict(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        // Only true signal `Out` pins are "drivers" for short-circuit purposes.
        // Power pins are rails by design — same-name multi-pin groups broadcast
        // every member onto one net (`[[19,32,48,64],[18]]=[VDD,VSS]` → four VDD
        // pins share the VDD rail), so multiple Power entries on a net are normal,
        // not a short. Power-rail conflicts (e.g. two different supplies tied)
        // are handled by NET_VOLTAGE_MISMATCH (E4104) instead.
        let outputs: Vec<&InstEntry> = net
            .points
            .iter()
            .filter_map(|id| table.get_entry(*id))
            .filter(|e| matches!(e.io_type, IOType::Out))
            .collect();
        if outputs.len() > 1 {
            let names: Vec<_> = outputs.iter().map(|e| e.path.as_str()).collect();
            let (pos, uri) = entry_pos(outputs[0]);
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
                code: crate::errcodes::NET_MULTI_DRIVE,
                pos,
                uri,
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
            let (pos, uri) = best_pos(table, &net.points);
            results.push(NetCheckResult {
                check: "undriven-net",
                severity: "warning",
                message: format!("Net '{}' has inputs but no output/power driver.", net.name),
                net_name: net.name.clone(),
                code: crate::errcodes::NET_NO_DRIVER,
                pos,
                uri,
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
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated component/interface is never wired by definition, so
        // its unwired pins are the normal shape of the view, not a defect.
        if matches!(entry.io_type, IOType::In)
            && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "floating-input",
                severity: "warning",
                message: format!("Input '{}' is not connected to any net.", entry.path),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_INPUT_UNCONNECTED,
                pos,
                uri,
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
                    let (pos, uri) = entry_pos(entry);
                    results.push(NetCheckResult {
                        check: "nc-connected",
                        severity: "warning",
                        message: format!(
                            "NC port '{}' is connected to net '{}'.",
                            entry.path, net.name
                        ),
                        net_name: net.name.clone(),
                        code: crate::errcodes::NET_NC_CONNECTED,
                        pos,
                        uri,
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
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated component/interface is never wired by definition, so
        // its unwired output pins are the normal shape of the view.
        if matches!(entry.io_type, IOType::Out)
            && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "unconnected-output",
                severity: "warning",
                message: format!("Output '{}' drives nothing.", entry.path),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_OUTPUT_UNDRIVEN,
                pos,
                uri,
            });
        }
    }
}

// ── P3+P4: Voltage mismatch between power pins on the same net ──
//
// The declared operating voltage of a power pin is READ from the pin's
// declaration — its attribute KVS (`voltage` / `volt` key) or, for pins bound
// to a power interface, the interface binding's volt parameter (`::DC(3.3V)`).
// No voltage is ever guessed from net names: the old net-name heuristic
// (`VCC_5V` → 5.0, `3V3` → 3.3) is gone. A pin that declares a *range*
// (`2.5V~5.5V`, `±`) is a tolerance statement, not a fixed rail, and is
// skipped. A net carrying two supply pins whose declared voltage sets share
// no common value (within tolerance) is reported as a short.
fn check_voltage_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    const TOL: f64 = 0.5;
    for net in table.get_nets() {
        // (pin path, declared alternative voltages) for supply pins on this net
        let mut declared: Vec<(String, Vec<f64>)> = Vec::new();
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) {
                continue;
            }
            let Some(voltages) = pin_declared_voltages(table, entry) else {
                continue;
            };
            if voltages.is_empty() {
                continue;
            }
            declared.push((entry.path.clone(), voltages));
        }
        if declared.len() < 2 {
            continue;
        }
        // Conflict: two pins whose declared sets share no value within TOL.
        for i in 0..declared.len() {
            let (p1, v1) = &declared[i];
            let mut conflicted = false;
            for j in i + 1..declared.len() {
                let (p2, v2) = &declared[j];
                let compatible = v1.iter().any(|a| v2.iter().any(|b| (a - b).abs() <= TOL));
                if compatible {
                    continue;
                }
                let (pos, uri) = best_pos(table, &net.points);
                results.push(NetCheckResult {
                    check: "voltage-mismatch",
                    severity: "error",
                    message: format!(
                        "Net '{}': power pins '{}' ({}V) and '{}' ({}V) declare \
                         incompatible voltages; they may be shorted.",
                        net.name,
                        p1,
                        fmt_voltages(v1),
                        p2,
                        fmt_voltages(v2)
                    ),
                    net_name: net.name.clone(),
                    code: crate::errcodes::NET_VOLTAGE_MISMATCH,
                    pos,
                    uri,
                });
                conflicted = true;
                break;
            }
            if conflicted {
                break;
            }
        }
    }
}

/// Read the operating voltages a pin declares, from its definition:
///
/// 1. **Attribute KVS** — the pin's `voltage` / `volt` key (e.g.
///    `voltage:3.3V`, `voltage:[1.2V, 1.3V]`). Read from `McPin.values`.
/// 2. **Interface binding** — for pins bound to a power interface
///    (`[VDD, GND]::DC(3.3V)`, `VIN{Vin, GND}::DC(5V)`), the interface
///    binding's volt parameter. Found by locating the `McPinPort::Interface`
///    whose `registered_pins` / member names include this pin.
///
/// Only SUPPLY pins (power-typed or power-named, ground excluded) are
/// voltage sources: a signal pin's `voltage` attribute describes signal
/// levels, not the rail. Ground pins (GND/VSS) are the reference and never
/// participate. Range values (`2.5V~5.5V`) are skipped — they declare
/// tolerance, not a fixed rail. Returns `None` when the pin is not a supply
/// pin or declares no concrete voltage.
fn pin_declared_voltages(table: &InstTable, entry: &InstEntry) -> Option<Vec<f64>> {
    let comp_entry = entry.parent_id.and_then(|pid| table.get_entry(pid))?;
    if comp_entry.class_name.is_empty() {
        return None;
    }
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    let def_entry = comps
        .iter()
        .find(|e| e.key().ident.to_string() == comp_entry.class_name)?;
    let def = def_entry.value();

    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    let pin = def.pins.pins.get(pin_id).or_else(|| {
        def.pins
            .pins
            .values()
            .find(|p| p.names.iter().any(|n| n == &entry.class_name))
    })?;

    // Supply pin only. Ground / reference pins (GND, VSS, VSSA, EPAD — the
    // exposed pad) are the return path and never declare a rail, even though
    // they are typically `Power`-typed; a shared ground pin legitimately
    // belongs to several rails (e.g. the EPAD of a multi-rail MCU), so it
    // must not seed a voltage comparison. A pin is a supply candidate when
    // it is power-named (leaf of `VIN.Vin` → `Vin`) or `Power`-typed and it
    // is not a ground reference.
    let leaf = entry.class_name.rsplit('.').next().unwrap_or("");
    let is_ground = is_ground_name(leaf)
        || is_ground_name(pin_id)
        || pin.names.iter().any(|n| is_ground_name(n))
        || leaf.eq_ignore_ascii_case("EPAD")
        || pin_id.eq_ignore_ascii_case("EPAD");
    if is_ground {
        return None;
    }
    let is_supply = is_supply_name(leaf) || matches!(pin.iotype, IOType::Power);
    if !is_supply {
        return None;
    }

    let mut out: Vec<f64> = Vec::new();
    // 1) Attribute KVS voltage.
    for val in pin.values.iter() {
        if let McAttrVal::KVS(kvs) = val {
            let key = kvs.key.to_string().to_lowercase();
            if key.contains("volt") {
                collect_kvs_voltage(&kvs.value, &mut out);
            }
        }
    }
    // 2) Interface binding volt parameter.
    for port in def.pins.names_to_id.values() {
        let McPinPort::Interface(iface) = port else {
            continue;
        };
        let owns_pin = iface.registered_pins.iter().any(|r| r == &pin_id)
            || iface
                .pin_name_mapping
                .iter()
                .any(|n| n == &entry.class_name);
        if !owns_pin {
            continue;
        }
        for p in &iface.params {
            if let McParamValue::UValue(uv) = p {
                if matches!(uv.unit(), McUnit::Volt) && !uv.is_range_or_plusminus() {
                    out.push(uv.value());
                }
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Extract scalar voltages from a KVS value:
/// `Const(Keyword("3.3V"))` → 3.3; `Square([Uval(1.2V), Uval(1.3V)])` →
/// [1.2, 1.3]; nested `low:`/`high:` sub-keys are recursed. Ranges
/// (`0V ~ 0.7V`) are skipped — only concrete scalar volts count.
fn collect_kvs_voltage(value: &KVSValue, out: &mut Vec<f64>) {
    match value {
        KVSValue::Const(c) => {
            let crate::semantic::basic::mc_literal::McConst::Keyword(s) = c;
            if let Some(v) = parse_voltage_str(s) {
                out.push(v);
            }
        }
        KVSValue::Square(vals) => {
            for a in vals {
                match a {
                    McAttrVal::AttrLiteral(McLiteral::Uval(uv)) => {
                        if matches!(uv.unit(), McUnit::Volt) && !uv.is_range_or_plusminus() {
                            out.push(uv.value());
                        }
                    }
                    McAttrVal::KVS(nested) => collect_kvs_voltage(&nested.value, out),
                    _ => {}
                }
            }
        }
        KVSValue::Nested(list) => {
            for k in list {
                collect_kvs_voltage(&k.value, out);
            }
        }
    }
}

/// Parse a scalar voltage text to volts: `"3.3V"` → 3.3, `"3V3"` → 3.3,
/// `"5"` → 5.0. Non-numeric / symbolic values (`0.7*VDD`, ranges) return None.
fn parse_voltage_str(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s.contains('~') || s.contains('±') || s.contains('*') {
        return None;
    }
    let digits = s.strip_suffix(['V', 'v']).unwrap_or(s);
    if digits.contains('V') || digits.contains('v') {
        digits
            .replace(['V', 'v'], ".")
            .trim_matches('.')
            .parse()
            .ok()
    } else {
        digits.parse().ok()
    }
}

/// Format a declared-voltage set for display: `[3.3]` → `3.3`, `[1.2, 3.3]` → `1.2/3.3`.
fn fmt_voltages(vs: &[f64]) -> String {
    vs.iter()
        .map(|v| format!("{v}"))
        .collect::<Vec<_>>()
        .join("/")
}

// ── P9: Component instances with no pins connected to any net ──
fn check_unwired_instances(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        // Skip synthetic virtual-instantiation wrappers: a component/interface
        // viewed standalone is by definition unwired — the E4112 unwired check
        // is meaningless for the fabricated VIRT_* unit (its whole point is a
        // box with no nets), and it fires on every such view.
        if matches!(entry.kind, crate::instant::insttab::InstKind::Component)
            && !entry.class_name.is_empty()
            && !entry.synthetic
        {
            let pins = table.get_pins_of(entry.id);
            if !pins.is_empty() && pins.iter().all(|p| !connected.contains(&p.id)) {
                let (pos, uri) = entry_pos(entry);
                results.push(NetCheckResult {
                    check: "unwired-instance",
                    severity: "warning",
                    message: format!(
                        "Instance '{}' has no pins connected to any net.",
                        entry.path
                    ),
                    net_name: entry.path.clone(),
                    code: crate::errcodes::NET_INSTANCE_UNCONNECTED,
                    pos,
                    uri,
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
            let (pos, uri) = best_pos(table, &net.points);
            results.push(NetCheckResult {
                check: "backfeed-risk",
                severity: "warning",
                message: format!(
                    "Net '{}' has both output and power supply. Backfeed risk.",
                    net.name
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::NET_BACKFEED_RISK,
                pos,
                uri,
            });
        }
    }
}

// ── D7: PULLUP_DEGENERATE — pullup/pulldown degraded into a signal bridge ──
// unified-twopin-no-builtin §2.6: after wiring, scan Pullup/Pulldown resistor
// `this{1}`/`this{2}` nets. A pullup is a component instance produced by a
// `func Pullup(...)` / `func Pulldown(...)` method dispatch — tagged at
// instantiation time by the method-name origin marker (M0-B-E.1). A plain
// series resistor has no method provenance and is not scanned.
//
// Rail detection uses network identity (IOType::Power / inferred Ground/Power
// member role) instead of the old name-prefix heuristic. Both ends non-rail →
// the pullup degenerated into a signal-signal bridge (E4056), e.g.
// `Pullup(SCL, SDA)` shorting two signals instead of pulling one up to a rail.
fn check_pullup_degenerate(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let nets: Vec<&NetEntry> = table.get_nets();
    let net_of = |pin_id: u32| -> Option<&NetEntry> {
        nets.iter().find(|n| n.points.contains(&pin_id)).copied()
    };
    let net_is_rail = |net: &NetEntry| -> bool {
        net.points.iter().any(|id| {
            table.get_entry(*id).map_or(false, |e| {
                matches!(e.io_type, IOType::Power)
                    || e.member_info.as_ref().map_or(false, |m| {
                        matches!(m.role, MemberRole::Power | MemberRole::Ground)
                    })
            })
        })
    };
    for (_, entry) in table.iter() {
        let fn_name = match &entry.origin {
            InstOrigin::FuncCall { fn_name, .. } => fn_name.as_str(),
            _ => continue,
        };
        let is_pull =
            fn_name.eq_ignore_ascii_case("pullup") || fn_name.eq_ignore_ascii_case("pulldown");
        if !is_pull || !matches!(entry.kind, InstKind::Component) {
            continue;
        }
        let pins = table.get_pins_of(entry.id);
        if pins.len() < 2 {
            continue;
        }
        let (Some(n1), Some(n2)) = (net_of(pins[0].id), net_of(pins[1].id)) else {
            continue;
        };
        if net_is_rail(n1) || net_is_rail(n2) {
            continue;
        }
        let (pos, uri) = entry_pos(entry);
        results.push(NetCheckResult {
            check: "pullup-degenerate",
            severity: "warning",
            message: crate::errcodes::format_msg(
                crate::errcodes::PULLUP_DEGENERATE,
                &[&fn_name, &n1.name, &n2.name],
            ),
            net_name: n1.name.clone(),
            code: crate::errcodes::PULLUP_DEGENERATE,
            pos,
            uri,
        });
    }
}

// ── V1: Module ports with mismatched IO directions on same net ──
fn check_port_io_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let mut has_in = false;
        let mut _has_out = false;
        let mut has_ps = false;
        let mut out_count = 0u32;
        for id in &net.points {
            if let Some(e) = table.get_entry(*id) {
                has_in |= matches!(e.io_type, IOType::In);
                _has_out |= matches!(e.io_type, IOType::Out);
                has_ps |= matches!(e.io_type, IOType::Power);
                if matches!(e.io_type, IOType::Out) {
                    out_count += 1;
                }
            }
        }
        if out_count > 1 && !has_in && has_ps {
            let (pos, uri) = best_pos(table, &net.points);
            results.push(NetCheckResult {
                check: "port-io-mismatch",
                severity: "warning",
                message: format!(
                    "Net '{}' has {} outputs and power but no input.",
                    net.name, out_count
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::NET_OUTPUTS_NO_INPUT,
                pos,
                uri,
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
            code: crate::errcodes::NET_POWER_NET_COUNT,
            pos: 0,
            uri: String::new(),
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
            matches!(e.kind, crate::instant::insttab::InstKind::Module) && e.parent_id.is_none()
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
            && !is_nc_entry(entry)
            // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
            // instantiated component/interface is never wired by definition,
            // so its boundary ports are the normal shape of the view.
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "unused-module-port",
                severity: "warning",
                message: format!(
                    "Module port '{}' ({:?}) is not connected to any net.",
                    entry.path, entry.io_type
                ),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_MODULE_PORT_UNCONNECTED,
                pos,
                uri,
            });
        }
    }
}

// ── Single-point nets (self-loop or isolated point) ──
fn check_single_point_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        if net.points.len() == 1 {
            if let Some(entry) = table.get_entry(net.points[0]) {
                let (pos, uri) = entry_pos(entry);
                results.push(NetCheckResult {
                    check: "single-point-net",
                    severity: "warning",
                    message: format!(
                        "Net '{}' has only one endpoint: '{}'. Possible dangling connection.",
                        net.name, entry.path
                    ),
                    net_name: net.name.clone(),
                    code: crate::errcodes::NET_DANGLING_ENDPOINT,
                    pos,
                    uri,
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
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for (_, entry) in table.iter() {
        // Same synthetic-wraper carve-out as E4112: a virtually-instantiated
        // component is never wired, so "N of M pins connected" (E4116) is a
        // guaranteed false positive on every component/interface file view.
        if !matches!(entry.kind, crate::instant::insttab::InstKind::Component)
            || entry.class_name.is_empty()
            || entry.synthetic
        {
            continue;
        }
        if let Some(def_entry) = comps
            .iter()
            .find(|e| e.key().ident.to_string() == entry.class_name)
        {
            // Count non-NC pin names only — NC pins are intentionally
            // unconnected and never counted (OR semantics, §2.19).
            let nc_names = def_entry
                .value()
                .pins
                .pins
                .values()
                .filter(|p| p.is_nc)
                .map(|p| p.names.len())
                .sum::<usize>();
            let def_pin_count = def_entry
                .value()
                .pins
                .names_to_id
                .len()
                .saturating_sub(nc_names);
            if def_pin_count == 0 {
                continue;
            }
            let pins = table.get_pins_of(entry.id);
            let connected_pins = pins.iter().filter(|p| connected.contains(&p.id)).count();
            if connected_pins < def_pin_count {
                let (pos, uri) = entry_pos(entry);
                results.push(NetCheckResult {
                    check: "pin-count-mismatch",
                    severity: "warning",
                    message: format!(
                        "'{}' has {} of {} pins connected.",
                        entry.path, connected_pins, def_pin_count
                    ),
                    net_name: entry.path.clone(),
                    code: crate::errcodes::NET_PARTIAL_CONNECTION,
                    pos,
                    uri,
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
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated interface's io ports render boundary pins and are never
        // wired by definition, so "bidirectional port not connected" is the
        // normal shape of the view, not a defect.
        if matches!(entry.io_type, IOType::InOut)
            && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "floating-bidirectional",
                severity: "warning",
                message: format!(
                    "Bidirectional port '{}' is not connected to any net.",
                    entry.path
                ),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_BIDIR_UNCONNECTED,
                pos,
                uri,
            });
        }
    }
}
