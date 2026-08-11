// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pin usage checks — runs after Pass2 flattening.
//!
//! Design doc §4:
//! - `check_unused_pins` (§4.2 检查 1): report Pin entries in the flattened
//!   InstTable that are not connected to any net.
//! - `check_conflicting_pins` (§4.2 检查 2): report pinids that have multiple
//!   option names and where 2+ different option names are actually in use.
//!
//! Both checks work directly from the InstTable's Pin entries, which are
//! created after dynamic pin resolution (§2.20). This makes them work
//! correctly for both static and dynamic (parameterized) component pins,
//! without needing the template component definition.

use crate::instant::insttab::{InstEntry, InstKind, InstTable};
use crate::semantic::common::IOType;
use std::collections::{HashMap, HashSet};

/// Pin usage result (same shape as `NetCheckResult` in `nets/mod.rs`).
#[derive(Debug, Clone)]
pub struct PinCheckResult {
    pub check: &'static str,
    pub severity: &'static str, // "error" | "warning" | "info"
    pub message: String,
    pub instance_path: String,
    pub code: u32,
    pub pos: u32,
    pub uri: String,
}

/// Run all pin usage checks and return diagnostics.
pub fn run_pin_checks(table: &InstTable) -> Vec<PinCheckResult> {
    let mut results = Vec::new();
    check_unused_pins(table, &mut results);
    check_conflicting_pins(table, &mut results);
    results
}

/// Extract the best available source position from an InstEntry.
fn entry_pos(entry: &InstEntry) -> (u32, String) {
    let pos = entry.src_pos.unwrap_or(0) as u32;
    (pos, entry.def_uri.clone())
}

/// Extract the pinid (last path segment) from a Pin entry's path.
fn pinid_from_path(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// Check if a pin name indicates NC (Not Connected).
fn is_nc_pin(class_name: &str) -> bool {
    class_name == "NC" || class_name == "nc"
}

/// Pick the best display name for a pin. If the stored name is just the pinid
/// itself (which happens for some interface-derived components), return None so
/// the caller can omit it from the message instead of showing a redundant value.
fn pin_display_name<'a>(pinid: &'a str, class_name: &'a str) -> Option<&'a str> {
    if class_name.is_empty() || class_name == pinid {
        None
    } else {
        Some(class_name)
    }
}

// ── §4.2 检查 1: Unused pins ──
///
/// For each Component instance, iterate over its Pin entries in the InstTable.
/// A Pin entry is "unused" if it is not connected to any net (`get_net_of`
/// returns None). For unused pins:
/// - Skip if the pin name is "NC" (NC pins are intentionally unconnected, §2.19).
/// - Downgrade to Info if `iotype == IOType::Power` (power pins).
/// - Otherwise report Warning.
///
/// This approach works for both static and dynamic (§2.20) pins because Pin
/// entries are created after dynamic pin resolution during flattening.
fn check_unused_pins(table: &InstTable, results: &mut Vec<PinCheckResult>) {
    for (_, entry) in table.iter() {
        if !matches!(entry.kind, InstKind::Component) || entry.class_name.is_empty() {
            continue;
        }

        let pins = table.get_pins_of(entry.id);
        for pin in &pins {
            // Connected to a net → not unused
            if table.get_net_of(pin.id).is_some() {
                continue;
            }

            let pinid = pinid_from_path(&pin.path);
            let pin_name = &pin.class_name;

            // NC pins are intentionally unconnected (§2.19)
            if is_nc_pin(pin_name) {
                continue;
            }

            let (severity, suffix) = if matches!(pin.io_type, IOType::Power) {
                ("info", " (power pin)")
            } else {
                ("warning", "")
            };
            let (pos, uri) = entry_pos(entry);
            let name_part = pin_display_name(pinid, pin_name)
                .map(|n| format!(" ({})", n))
                .unwrap_or_default();
            results.push(PinCheckResult {
                check: "unused-pin",
                severity,
                message: format!(
                    "Pin '{}'{} on '{}' is not connected{}",
                    pinid, name_part, entry.path, suffix
                ),
                instance_path: entry.path.clone(),
                code: 3201,
                pos,
                uri,
            });
        }
    }
}

// ── §4.2 检查 2: Conflicting pin option names ──
///
/// For each Component instance, group its Pin entries by pinid. For pinids
/// that have 2+ Pin entries with different `class_name` values (option names)
/// that are all connected to nets, report Warning.
///
/// Note: In the current InstTable, each pinid typically has only one Pin entry
/// (with the first option name), so this check rarely fires. Full multi-option
/// conflict detection requires connection-level option name tracking
/// (design doc §4.3 `PinUsageTracker::used_options`).
fn check_conflicting_pins(table: &InstTable, results: &mut Vec<PinCheckResult>) {
    for (_, entry) in table.iter() {
        if !matches!(entry.kind, InstKind::Component) || entry.class_name.is_empty() {
            continue;
        }

        let pins = table.get_pins_of(entry.id);

        // Group connected Pin entries by pinid, collecting distinct option names.
        let mut pinid_to_names: HashMap<&str, HashSet<&str>> = HashMap::new();
        for pin in &pins {
            if table.get_net_of(pin.id).is_none() {
                continue; // Only consider connected pins
            }
            if pin.class_name.is_empty() {
                continue;
            }
            let pinid = pinid_from_path(&pin.path);
            pinid_to_names
                .entry(pinid)
                .or_default()
                .insert(pin.class_name.as_str());
        }

        for (pinid, names) in &pinid_to_names {
            if names.len() >= 2 {
                let (pos, uri) = entry_pos(entry);
                let mut used_list: Vec<String> = names.iter().map(|s| s.to_string()).collect();
                used_list.sort();
                results.push(PinCheckResult {
                    check: "conflicting-pin-options",
                    severity: "warning",
                    message: format!(
                        "Pin '{}' on '{}' uses conflicting option names: {}",
                        pinid,
                        entry.path,
                        used_list.join(", ")
                    ),
                    instance_path: entry.path.clone(),
                    code: 3202,
                    pos,
                    uri,
                });
            }
        }
    }
}
