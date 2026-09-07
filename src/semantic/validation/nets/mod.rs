// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 Electrical Net Checks — driver conflict, floating inputs, voltage mismatch, etc.
//!
//! Runs after `mcb_pass2()` when the full flattened netlist (`InstTable`) is available.

use crate::db::diagnostic::diagnostic::Diagnostic;
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
use crate::semantic::component::mc_pins::{McPinPort, McPwrPin, PwrDir};
use crate::semantic::component::McComponent;
use crate::semantic::module::pi::{decode_pwr_pin, McPowerDecls};
use crate::semantic::validation::finding::CheckFinding;
use std::collections::HashSet;

/// Run all electrical net checks and return diagnostics.
///
/// FlatErc rules are declared — and ordered — in `crate::rules`
/// (`FLAT_ERC_RULES`); the runner executes each declared rule in declaration
/// order (§5-5 of the rule-registry design), replacing the former hand-written
/// call table. Output is byte-identical to the old sequence.
pub fn run_net_checks(table: &InstTable) -> Vec<NetCheckResult> {
    let mut results = Vec::new();
    for rule in crate::rules::flat_erc_rules() {
        (rule.run)(table, &mut results);
    }
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

/// Convert net-check results into ready-to-log `Diagnostic`s (dianlu-tree
/// Phase A). Each carrier is first projected onto the unified
/// [`CheckFinding`] line (`finding.rs`), then flattened by the finding's
/// `to_diagnostic`; the mapping reproduces the former severity-string match
/// and uri/pos passthrough byte-for-byte. A result without a uri keeps an
/// empty uri and [`log_net_check_diagnostics`] falls back to the caller's
/// `current_uri` at log time.
///
/// This is the FlatErc emission choke of the rule-registry normalization
/// point (§8-5): each finding is adjudicated against the process-wide
/// override store before it becomes a diagnostic. Under the empty (or fully
/// non-overridable) store the output is byte-identical to the legacy mapping.
pub fn net_results_to_diagnostics(results: &[NetCheckResult]) -> Vec<Diagnostic> {
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        let finding = CheckFinding::from(r.clone());
        let Some(effective) =
            crate::db::diagnostic::override_store::with_store(|s| s.apply_to_finding(&finding))
        else {
            continue; // allow hit suppresses the finding at the display layers
        };
        out.push(effective.to_diagnostic());
    }
    out
}

/// Log net-check diagnostics at their own uris (dianlu-tree Phase A
/// caller-side helper). A diagnostic with an empty uri falls back to the
/// caller's `current_uri` — the entry module's file. Replaces the logging
/// that used to live inside `DianLu::flatten`.
pub fn log_net_check_diagnostics(diags: &[Diagnostic]) {
    for d in diags {
        let uri = if d.loc.uri.is_empty() {
            crate::current_uri::try_get().unwrap_or_default()
        } else {
            d.loc.uri.clone()
        };
        crate::db::diagnostic::diagnostic::diagnostic_log_at(
            d.code,
            d.level,
            uri,
            d.loc.pos,
            d.loc.len,
            &d.msg,
            &[],
        );
    }
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
pub(crate) fn check_driver_conflict(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        // Only true signal `Out` pins are "drivers" for short-circuit purposes.
        // Power pins are rails by design — same-name multi-pin groups broadcast
        // every member onto one net (`[[19,32,48,64],[18]]=[VDD,VSS]` → four VDD
        // pins share the VDD rail), so multiple Power entries on a net are normal,
        // not a short. Power-rail conflicts (e.g. two different supplies tied)
        // are handled by NET_VOLTAGE_MISMATCH (E4104) instead.
        let out_ids: Vec<u32> = net
            .points
            .iter()
            .filter_map(|id| table.get_entry(*id))
            .filter(|e| matches!(e.io_type, IOType::Out))
            .map(|e| e.id)
            .collect();
        // Vantage dedupe (rule-registry design §4-a): in the A' flat model a
        // module-boundary `out` port is a junction — the exit of the net segment
        // inside its own module instance. When that exit's net already carries an
        // `Out` point rooted inside the port's module (the internal driver it
        // forwards), the port is not a second physical driver; counting both
        // double-counts one signal (e.g. `SUB { out Y; BUF b; b.Y -> Y }`
        // instanced as `s1` puts `s1.Y` and `s1.b.2` on one internal net). The
        // port still counts when no interior driver shares the net — two out
        // ports on a parent net (`s1.Y -> s2.Y`) are two different modules' real
        // drivers shorted, exactly what E4101 exists to catch.
        let drivers: Vec<&InstEntry> = out_ids
            .iter()
            .filter(|&&id| !is_module_out_port_exit(table, id, &out_ids))
            .filter_map(|id| table.get_entry(*id))
            .collect();
        if drivers.len() > 1 {
            let names: Vec<_> = drivers.iter().map(|e| e.path.as_str()).collect();
            let (pos, uri) = entry_pos(drivers[0]);
            results.push(NetCheckResult {
                check: "driver-conflict",
                severity: "error",
                message: format!(
                    "Net '{}' has {} drivers: {}. Possible short circuit.",
                    net.name,
                    drivers.len(),
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

/// §4-a vantage: is `id` a module-boundary `out` port that is merely the exit
/// of an interior driver already present on the same net?
fn is_module_out_port_exit(table: &InstTable, id: u32, net_out_ids: &[u32]) -> bool {
    let Some(port) = table.get_entry(id) else {
        return false;
    };
    if !matches!(port.kind, InstKind::Port) {
        return false;
    }
    let Some(owner) = port.parent_id else {
        return false;
    };
    net_out_ids.iter().any(|&oid| {
        if oid == id {
            return false;
        }
        let Some(other) = table.get_entry(oid) else {
            return false;
        };
        // A sibling out port of the same module instance is a distinct signal
        // of that module, not this port's interior driver.
        if other.kind == InstKind::Port && other.parent_id == Some(owner) {
            return false;
        }
        // Walk the other point's parent chain: the port's own module instance
        // must appear strictly between it and the root.
        let mut cur = other.parent_id;
        while let Some(pid) = cur {
            if pid == owner {
                return true;
            }
            cur = table.get_entry(pid).and_then(|e| e.parent_id);
        }
        false
    })
}

// ── P2: Nets with only input endpoints (no driver) ──
pub(crate) fn check_undriven_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
        // A net named after an implicit power rail (`VCC`, `GND`, `V3V3`, …)
        // is a source by convention — a bare `VCC -> x.signal` at module
        // scope supplies the net; it does not hang undriven. Mirrors the
        // floating-label carve-out (floating.rs) and infer_member_role's rail
        // classification.
        if is_supply_name(&net.name) || is_ground_name(&net.name) {
            continue;
        }
        // A net containing a module-boundary port (`x.signal -> D_STATUS.1`)
        // receives its drive from the enclosing scope through that port; the
        // flat table keeps the parent-side connection as a separate net sharing
        // the port entry, so drive cannot be judged here. Port connectivity is
        // checked at the boundary instead (check_floating_inputs / C4
        // unused-module-port / check_floating_outputs).
        let has_boundary_port = points
            .iter()
            .any(|e| matches!(e.kind, crate::instant::insttab::InstKind::Port));
        if has_boundary_port {
            continue;
        }
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
// §A′-scope: directional float checks (E4108 / E-output-undriven / E4117)
// own *component pads* (kind Pin) only. Module-boundary ports (kind Port) are
// owned by C4/E4114 (`check_unused_module_ports`); A′ bus-slash / bare-member
// alias entries (kind Label) and other non-physical spellings never carry a
// directional float — the physical member Port or pad reports for them. This
// split is what stops the same unconnected pad from being double-reported by
// a directional check and by C4.
pub(crate) fn check_floating_inputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
            && matches!(entry.kind, InstKind::Pin)
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
pub(crate) fn check_nc_connected(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
pub(crate) fn check_unconnected_outputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated component/interface is never wired by definition, so
        // its unwired output pins are the normal shape of the view.
        // §A′-scope: component pads only (kind Pin); module-boundary ports go
        // to C4/E4114, A′ alias spellings never carry a directional float.
        if matches!(entry.io_type, IOType::Out)
            && matches!(entry.kind, InstKind::Pin)
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
pub(crate) fn check_voltage_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
    let comps = crate::definition_space().workspace_components();
    let def = comps
        .iter()
        .find(|(sn, _)| sn.ident.to_string() == comp_entry.class_name)
        .map(|(_, c)| c)?;

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
pub(crate) fn check_unwired_instances(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
pub(crate) fn check_backfeed(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
pub(crate) fn check_pullup_degenerate(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
pub(crate) fn check_port_io_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
pub(crate) fn check_power_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
pub(crate) fn check_unused_module_ports(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
        // §A′-scope: this check owns *module-boundary ports* — entries whose
        // kind is Port (the aggregate or dotted member a parent design wires).
        // Component pads (kind Pin) are owned by the directional float checks
        // (E4108 / E-output-undriven / E4117); A′ bus-slash/bare alias spellings
        // (kind Label) are folded onto the Port and never surface here. The
        // former `class_name` guard (which only selected class-carrying pads)
        // is what made C4 re-report Pin pads that the directional checks had
        // already reported — the ldo.4 (E4108+E4114) and UC.8 (E4117+E4114)
        // duplicates on hbl.
        if entry.parent_id == top_id || entry.parent_id.is_none() {
            continue;
        }
        if !matches!(entry.kind, crate::instant::insttab::InstKind::Port) {
            continue;
        }
        if matches!(
            entry.io_type,
            IOType::In | IOType::Out | IOType::InOut | IOType::Power
        ) && !connected.contains(&entry.id)
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
pub(crate) fn check_single_point_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
pub(crate) fn check_pin_count_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    let comps = crate::definition_space().workspace_components();
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
        if let Some(def) = comps
            .iter()
            .find(|(sn, _)| sn.ident.to_string() == entry.class_name)
            .map(|(_, c)| c)
        {
            // Count distinct non-NC physical pads, not name entries. A single
            // pad can carry several alternate signal names (`2 = SO | IO1`) or
            // receive an interface binding (`[1,2,5,6] = SPI::SPI("Slave")`),
            // each registering its own key in `names_to_id`; the connected-pin
            // numerator below counts pads, so the denominator must too —
            // otherwise every multi-aliased part reports "N of M pins
            // connected" even when fully wired (E4116). NC pads are
            // intentionally unconnected and never counted (OR semantics §2.19).
            let def_pin_count = def.pins.pins.values().filter(|p| !p.is_nc).count();
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

// ── Abstract placed unselected (abstract-variant plan §6.1) ────────────────
/// An instance whose def is an `abstract component` carries the `unselected`
/// marker set at flatten time (never inferred here from a `partno` sentinel —
/// abstract defs may legally hold a reference partno). Placement is legal and
/// the netlist is produced, but a BOM tool must still pick a variant: ERC W.
pub(crate) fn check_unselected_abstract(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (_, entry) in table.iter() {
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated component (VIRT view) is never BOM-selected, so the
        // unselected warning is not the normal shape of that view.
        if !matches!(entry.kind, crate::instant::insttab::InstKind::Component)
            || entry.class_name.is_empty()
            || entry.synthetic
            || !entry.unselected
        {
            continue;
        }
        let (pos, uri) = entry_pos(entry);
        results.push(NetCheckResult {
            check: "abstract-unselected",
            severity: "warning",
            message: format!(
                "abstract component instance '{}' is unselected (no partno); \
                 BOM must pick a variant",
                entry.path
            ),
            net_name: entry.path.clone(),
            code: crate::errcodes::ABSTRACT_PART_UNSELECTED,
            pos,
            uri,
        });
    }
}

// ── Floating outputs (output variant of floating input check) ──
pub(crate) fn check_floating_outputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
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
        // §A′-scope: component pads only (kind Pin); an unconnected
        // module-boundary InOut port is owned by C4/E4114, and the A′ bus-slash
        // lane / bare-member alias spellings of that port (kind Label) are
        // folded onto it — reporting the lane and its aggregate twice is the
        // `UART1` + `UART1/TX` + `UART1/RX` phantom triple this guard removes.
        if matches!(entry.io_type, IOType::InOut)
            && matches!(entry.kind, InstKind::Pin)
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

// ── Power-intent L1 (design §3 / §13 landing 1): declared relation edges ──
//
// A declared `@bridge`/`@couple`/`@clamp` edge *never* merges L0 copper —
// net-identity already unions real wiring. It only declares a relation between
// two L1 potential classes. These checks are the declaration-local slice of the
// §3.2 role table, emitted as FlatErc rules (PWR-2 loop/@star, PWR-7 clamp
// target). Each verdict is *owning-def local* (an instance's nets are the
// def's nets under the instance path, and relation edges only name same-scope
// nets/refs, iron rule 1 §6), so a def instantiated N times reports once, at
// its own source span.

/// Module instances carrying power-intent declarations, deduped by owning def
/// (`def_uri` + `def.name`). The map is keyed by module entry id; the entry
/// supplies the def's file for span anchoring.
fn power_intent_defs(table: &InstTable) -> Vec<(&McPowerDecls, String)> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut out = Vec::new();
    for (id, pi) in table.power_decls() {
        let Some(entry) = table.get_entry(*id) else {
            continue;
        };
        if !matches!(entry.kind, InstKind::Module) {
            continue;
        }
        if !seen.insert((entry.def_uri.clone(), entry.class_name.clone())) {
            continue;
        }
        out.push((pi, entry.def_uri.clone()));
    }
    out
}

/// PWR-2 (design §3.2/§3.4): the DC `@bridge` subgraph must be acyclic. A
/// second/cyclic leg between endpoints already DC-bridged (parallel legs, or a
/// triangle) is a loop — unless a hub conduit on either endpoint carries
/// `@star`, which discharges the intentional loop to the simulation layer.
/// Union-find over declared bridge endpoint nets; an edge whose endpoints
/// already share a root is the redundant path.
pub(crate) fn check_power_bridge_loop(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        let edges: Vec<crate::semantic::module::pi::L1Edge> = pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Bridge)
            .collect();
        if edges.is_empty() {
            continue;
        }
        let refs = pi.l1_refs();
        let star: HashSet<&str> = refs
            .iter()
            .filter(|r| r.star)
            .map(|r| r.name.as_str())
            .collect();

        // Distinct endpoint nets → index, then union-find.
        let mut names: Vec<String> = Vec::new();
        let mut idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for e in &edges {
            for ep in e.endpoints.iter() {
                if !idx.contains_key(ep) {
                    idx.insert(ep.clone(), names.len());
                    names.push(ep.clone());
                }
            }
        }
        let mut parent: Vec<usize> = (0..names.len()).collect();
        let find = |parent: &mut Vec<usize>, mut x: usize| -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        };

        for e in &edges {
            let (Some(a), Some(b)) = (idx.get(&e.endpoints[0]), idx.get(&e.endpoints[1])) else {
                continue;
            };
            let (a, b) = (*a, *b);
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
                continue;
            }
            // Redundant DC path. Discharged when either endpoint is a @star hub.
            if star.contains(names[a].as_str()) || star.contains(names[b].as_str()) {
                continue;
            }
            results.push(NetCheckResult {
                check: "power-bridge-loop",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_BRIDGE_LOOP,
                    &[&names[a], &names[b]],
                ),
                net_name: names[a].clone(),
                code: crate::errcodes::POWER_BRIDGE_LOOP,
                pos: e.span.start as u32,
                uri: uri.clone(),
            });
        }
    }
}

/// PWR-7 (design §11 / §3.2): `@clamp(ref)` must reference an
/// `@role(protective)`/`@role(earth)` ref — clamping to a main/quiet/isolated
/// ref would dump transient current into the wrong reference. A clamp target
/// that is not a same-scope ref is not adjudicated here (its role is supplied
/// by an ancestor world / port contract, iron rule 1).
pub(crate) fn check_clamp_ref_role(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        let refs = pi.l1_refs();
        let role_of: std::collections::HashMap<&str, &str> = refs
            .iter()
            .filter_map(|r| r.role.as_deref().map(|role| (r.name.as_str(), role)))
            .collect();
        for e in pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Clamp)
        {
            let Some(target) = e.endpoints.first() else {
                continue;
            };
            let Some(role) = role_of.get(target.as_str()).copied() else {
                continue;
            };
            if role == "protective" || role == "earth" {
                continue;
            }
            results.push(NetCheckResult {
                check: "clamp-ref-role",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::CLAMP_REF_NOT_PROTECTIVE,
                    &[&target, &role],
                ),
                net_name: target.clone(),
                code: crate::errcodes::CLAMP_REF_NOT_PROTECTIVE,
                pos: e.span.start as u32,
                uri: uri.clone(),
            });
        }
    }
}

// ── Power-intent DC rail contract (§4.1 / §13.2): Volt-arg decode ──────────
// A domain rail declares the *guarantee* half of a DC contract:
// `rail [hot, ret]::DC(v, tol, capacity, eff)`. Two declaration-local
// verdicts, owning-def local exactly like the relation-edge rules above:
//   * decode (6009): every rail ctor arg decodes to the contract it names
//     (nominal is a signed DC volts, tol is ±%, capacity a current, eff a
//     factor). The Volt-arg self-check — no fake window, no silent pass.
//   * two-roots (6010): a net is the hot member of at most one rail. Writing
//     an intermediate net into a domain rail gives S two handwritten roots and
//     every downstream window ERC a fake conflict (§4.1 — an intermediate net never enters a domain).
// The full sink-window E-PWR-001 (`S(net) ⊆ input_req` / sink req window)
// needs the psnk + spec semantic layer (design §13 axis ③) and is not
// adjudicated here — it cannot be golden-verified until that layer lands.
pub(crate) fn check_power_rail_contract(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        for r in pi.l1_rails() {
            if let Some(bad) = &r.bad {
                results.push(NetCheckResult {
                    check: "rail-contract",
                    severity: "error",
                    message: crate::errcodes::format_msg(
                        crate::errcodes::POWER_RAIL_DECODE,
                        &[&r.hot, &r.ret, bad],
                    ),
                    net_name: r.hot.clone(),
                    code: crate::errcodes::POWER_RAIL_DECODE,
                    pos: r.span.start as u32,
                    uri: uri.clone(),
                });
            }
        }
    }
}

pub(crate) fn check_power_rail_two_roots(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        let rails = pi.l1_rails();
        let mut first: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (i, r) in rails.iter().enumerate() {
            if let Some(&j) = first.get(r.hot.as_str()) {
                // A second rail guarantees the same hot net: two handwritten
                // roots on one S. Report once per extra root, at its span.
                let prev = &rails[j];
                results.push(NetCheckResult {
                    check: "rail-two-roots",
                    severity: "error",
                    message: crate::errcodes::format_msg(
                        crate::errcodes::POWER_RAIL_TWO_ROOTS,
                        &[&r.hot, &prev.domain, &r.domain],
                    ),
                    net_name: r.hot.clone(),
                    code: crate::errcodes::POWER_RAIL_TWO_ROOTS,
                    pos: r.span.start as u32,
                    uri: uri.clone(),
                });
            } else {
                first.insert(r.hot.as_str(), i);
            }
        }
    }
}

/// E-PWR-001 (design §4.4 mandatory-nominal check / §11): a sink (`psnk`) on
/// a net must require that net's derived supply nominal S. The canonical §4.4
/// case: a `::DC(3.3V)`
/// sink sitting on a 5V-supplied net is a wrong hookup (P3/E-PWR-001) — nominal
/// vs nominal is the comparison, no window needed.
///
/// S is derived per net from the net's handwritten supply roots — the design's
/// §4.3 roots: `S(root)` = a handwritten guarantee window (a psrc or a
/// domain-rail block). Both are consumed here, joined to the flat net the
/// root's hot member lands on:
///   * a domain rail whose `hot` resolves to the net name (net name == rail name — identity,
///     not a voltage-from-name heuristic);
///   * a `psrc`/`psbi` source pin whose hot terminal is directly on the net
///     (e.g. ORing `OUT` psrc feeding VMAIN_5V in the golden).
/// If the roots on one net disagree in nominal, the net is left un-adjudicated
/// (source contention is PWR-3 OR-merge territory, deferred) rather than judged
/// against an arbitrary pick. Copper pass-through propagation (S crossing a fuse
/// / inductor / ferrite §4.3) and converter re-anchoring are the later S-set
/// step; this rule compares only sinks on nets that carry a direct root.
/// A root whose own nominal failed to decode is reported by the decl-local
/// decode ERC (rail: 6009; pin: 6012), never adjudicated here; likewise a
/// sink whose nominal does not decode is skipped rather than compared blind.
pub(crate) fn check_sink_nominal_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Guarantee net name → (domain, nominal, verbatim nominal text). Two rails
    // claiming the same hot with *different* nominals leave the net un-
    // adjudicated (6010 reports the two-root conflict within a scope; across
    // scopes the flat net name cannot be pinned to one guarantee).
    let mut guarantee: std::collections::HashMap<String, (String, f64, String)> =
        std::collections::HashMap::new();
    for (pi, _uri) in power_intent_defs(table) {
        for r in pi.l1_rails() {
            let Some(v) = r.v else {
                continue; // 6009's job
            };
            match guarantee.get(&r.hot) {
                Some((_, prev, _)) if (*prev - v).abs() > 1e-9 => {
                    guarantee.remove(&r.hot); // ambiguous scope — skip
                }
                None => {
                    guarantee.insert(r.hot.clone(), (r.domain.clone(), v, r.v_text.clone()));
                }
                _ => {} // same nominal redeclared: keep the first
            }
        }
    }

    // Component class → def, then component-instance id → def, so every net
    // point recovers its def contract without re-scanning the definition space.
    // (`workspace` lives for the whole check so the borrowed defs stay valid.)
    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    let comp_def: std::collections::HashMap<u32, &McComponent> = table
        .get_components()
        .iter()
        .filter_map(|e| defs.get(&e.class_name).map(|d| (e.id, *d)))
        .collect();

    for net in table.get_nets() {
        // ── Derive S(net) from its handwritten supply roots (design §4.3). ──
        // Rail face first: exact net name, then the last dotted segment
        // (module-qualified / power-rail tier-3 spellings).
        let rail_root = guarantee
            .get(&net.name)
            .or_else(|| net.name.rsplit('.').next().and_then(|l| guarantee.get(l)))
            .map(|(_domain, v, text)| (*v, text.clone()));
        // Source pins: a psrc/psbi hot terminal directly on the net.
        let mut src_nominal: Vec<(f64, String)> = Vec::new();
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = comp_def.get(&comp_id).copied() else {
                continue;
            };
            let Some(contract) = source_contract_for(def, entry) else {
                continue;
            };
            let dec = decode_pwr_pin(contract);
            if let Some(v) = dec.v {
                src_nominal.push((v, dec.v_text));
            }
        }
        // All roots on one net must agree on one S, else leave it un-adjudicated
        // (source contention / cross-scope ambiguity → 6010 or PWR-3, not here).
        let (v_supply, supply_text) = match rail_root {
            Some((v, text)) => {
                if src_nominal.iter().any(|(sv, _)| (*sv - v).abs() > 1e-9) {
                    continue; // a source pin on the net disagrees with the rail
                }
                (v, text)
            }
            None => {
                let mut it = src_nominal.iter();
                let Some((first, first_text)) = it.next() else {
                    continue; // no supply root on this net — intermediate (S-set later)
                };
                if it.any(|(sv, _)| (*sv - *first).abs() > 1e-9) {
                    continue; // two sources, different nominals — defer
                }
                (*first, first_text.clone())
            }
        };

        // ── mandatory-nominal: every decodable psnk sink on this net must need S. ──
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = comp_def.get(&comp_id).copied() else {
                continue;
            };
            let Some(contract) = sink_contract_for(def, entry) else {
                continue;
            };
            let dec = decode_pwr_pin(contract);
            let Some(v_sink) = dec.v else { continue };
            if (v_sink - v_supply).abs() <= 1e-9 {
                continue; // sink nominal == S(net) — the healthy hookup
            }
            // Instance-side sink terminal for the message (`main.s.VDD`, not
            // the positional pin id `main.s.1`).
            let base = entry
                .path
                .rsplit_once('.')
                .map(|(p, _)| p)
                .unwrap_or(&entry.path);
            let sink_path = format!("{base}.{}", contract.hot);
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "sink-nominal-mismatch",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_SINK_NOMINAL_MISMATCH,
                    &[&sink_path, &net.name, &dec.v_text, &supply_text],
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::POWER_SINK_NOMINAL_MISMATCH,
                pos,
                uri,
            });
        }
    }
}

/// Pin-contract Volt-arg decode (the pin-side of [`POWER_RAIL_DECODE`] 6009;
/// power-intent-design.md §5.2 closed word-list discipline): every `psrc`/`psnk`/`psbi`
/// `::DC(…)` ctor arg must decode to the contract it names. `decode_pwr_pin`
/// keeps the first failure as `L1PwrPin::bad` — a non-DC nominal (e.g.
/// `::DC(5A)` on a sink), a source-exclusive budget key (`tol`/`capacity`/`eff`)
/// on a sink, a `spec`-belonging window key (`req`/`abs`) on the pin, or a
/// missing mandatory nominal. This rule reports it decl-locally — once per used
/// component class's power contract, at its own source span — rather than the
/// hookup layer silently skipping an undecodable sink.
pub(crate) fn check_pin_contract_decode(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    // One report per used component class's offending contract (a def
    // instantiated N times is checked once, at its own decl — like 6009).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in table.get_components() {
        if !seen.insert(entry.class_name.clone()) {
            continue;
        }
        let Some(def) = defs.get(&entry.class_name).copied() else {
            continue;
        };
        for contract in &def.pins.pwr {
            let dec = decode_pwr_pin(contract);
            let Some(bad) = dec.bad else {
                continue;
            };
            results.push(NetCheckResult {
                check: "pin-contract-decode",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_PIN_DECODE,
                    &[&entry.class_name, &dec.hot, &bad],
                ),
                net_name: dec.hot.clone(),
                code: crate::errcodes::POWER_PIN_DECODE,
                pos: dec.span.start as u32,
                uri: entry.def_uri.clone(),
            });
        }
    }
}

/// The `psnk` contract whose *hot* terminal this flat pin is, if any. A flat
/// pin is the hot member of a sink contract when the def-side member names of
/// its pin id include the contract's `hot` (pin ids are positional: `main.s.1`
/// → def pin "1" whose registered names are the terminals); the return member
/// (`ret`, e.g. GND) never matches its own contract's `hot`, so the return pin
/// is naturally skipped.
fn sink_contract_for<'a>(def: &'a McComponent, entry: &InstEntry) -> Option<&'a McPwrPin> {
    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    let names: Vec<&str> = def
        .pins
        .pins
        .get(pin_id)
        .map(|p| p.names.iter().map(|n| n.as_str()).collect())
        .unwrap_or_default();
    def.pins.pwr.iter().find(|c| {
        c.dir == PwrDir::Snk && (names.iter().any(|n| *n == c.hot) || entry.class_name == c.hot)
    })
}

/// The `psrc`/`psbi` contract whose *hot* terminal this flat pin is, if any —
/// the source-side mirror of [`sink_contract_for`]. A `psbi` counts as a source
/// root because its `::DC(v)` is the *discharge* supply guarantee (§4.1), which
/// is the S its hot net carries while it sources. Shared-return pins never match
/// the contract's own `hot`, so return nets get no S from source pins.
fn source_contract_for<'a>(def: &'a McComponent, entry: &InstEntry) -> Option<&'a McPwrPin> {
    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    let names: Vec<&str> = def
        .pins
        .pins
        .get(pin_id)
        .map(|p| p.names.iter().map(|n| n.as_str()).collect())
        .unwrap_or_default();
    def.pins.pwr.iter().find(|c| {
        matches!(c.dir, PwrDir::Src | PwrDir::Bi)
            && (names.iter().any(|n| *n == c.hot) || entry.class_name == c.hot)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::diagnostic::override_store::{
        install_store, AllowEntry, OverrideStore, PathScope,
    };
    use crate::semantic::validation::CheckSeverity;

    fn net_result(code: u32, uri: &str) -> NetCheckResult {
        NetCheckResult {
            check: "probe",
            severity: "error",
            message: "probe message".to_string(),
            net_name: "n".to_string(),
            code,
            pos: 7,
            uri: uri.to_string(),
        }
    }

    fn diag_key(d: &Diagnostic) -> (u32, String) {
        (d.code, d.msg.clone())
    }

    #[test]
    fn net_results_to_diagnostics_is_identity_under_any_store_today() {
        // §8-5 identity anchor: while every catalog rule is non-overridable,
        // even a store that would otherwise hit must leave the FlatErc
        // diagnostic output byte-identical. This is what lets the lock tests
        // run unmodified.
        let results = [
            net_result(crate::errcodes::NET_MULTI_DRIVE, ""),
            net_result(1, "a.mc"), // unregistered code
        ];
        let empty = net_results_to_diagnostics(&results);

        // A populated store (severity override + global allow for E4101)
        // refuses both under the current non-overridable catalog.
        let mut store = OverrideStore::default();
        store
            .project
            .severities
            .insert(crate::errcodes::NET_MULTI_DRIVE, CheckSeverity::Info);
        store.project.allows.push(AllowEntry {
            code: crate::errcodes::NET_MULTI_DRIVE,
            path: PathScope::Project,
            reason: None,
        });
        install_store(store);
        let populated = net_results_to_diagnostics(&results);
        install_store(OverrideStore::default());
        assert_eq!(
            populated.iter().map(diag_key).collect::<Vec<_>>(),
            empty.iter().map(diag_key).collect::<Vec<_>>()
        );
        assert_eq!(populated.len(), 2);
    }
}
