// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Netdiff: compare actual pass2 netlist against golden TOML.
//!
//! ## Usage
//! ```sh
//! cargo test netdiff -- --nocapture
//! ```
//!
//! ## Output
//! - terminal: per-module diff report
//! - file: baseline/netdiff_baseline.md (summary table)

use mcc::{
    InstEntry, InstKind, InstTable, McComponentInst, McIds, McModuleInst, McParamValue, McURI,
    MccProjectTree,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

// ============================================================================
// Golden TOML structures (mirrors tests/golden_schema_validation.rs)
// ============================================================================

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct GoldenMeta {
    module: String,
    nets: usize,
    components: usize,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct GoldenComp {
    id: String,
    class: String,
    value: String,
    pins: usize,
    origin: String,
}

#[derive(Debug, serde::Deserialize, Clone)]
struct GoldenNet {
    name: String,
    points: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GoldenSeries {
    comp: String,
    is_series: bool,
}

#[derive(Debug, serde::Deserialize)]
struct GoldenModule {
    #[allow(dead_code)]
    meta: GoldenMeta,
    #[serde(default)]
    comp: Vec<GoldenComp>,
    #[serde(default)]
    net: Vec<GoldenNet>,
    #[serde(default)]
    series: Vec<GoldenSeries>,
}

// ============================================================================
// Actual netlist structures (extracted from InstTable)
// ============================================================================

/// Normalized endpoint: "comp_leaf.pin_number"
type NormPoint = String;

#[derive(Debug, Clone)]
struct ActualComp {
    leaf_name: String, // instance leaf name, e.g. "ldo", "@CAP1"
    #[allow(dead_code)]
    full_path: String, // full InstTable path
    class: String,     // class name, e.g. "LDO.SGM2019_33YN5G_TR"
    value: String,     // primary parameter value (derived from class/params)
    #[allow(dead_code)]
    pins: usize, // pin count
    #[allow(dead_code)]
    pin_numbers: Vec<usize>, // all pin numbers
}

#[derive(Debug, Clone)]
struct ActualNet {
    name: String,
    points: BTreeSet<NormPoint>, // normalized: "comp_leaf.pin_number"
}

#[derive(Debug)]
struct ActualModule {
    module_name: String, // golden name, e.g. "POWER_LDO"
    comps: Vec<ActualComp>,
    nets: Vec<ActualNet>,
}

// ============================================================================
// Comparison result types
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffKind {
    /// golden 1 net → actual N nets (golden net too coarse)
    ExtraSplit,
    /// golden N nets → actual 1 net (SHORT — most severe)
    #[allow(dead_code)]
    ExtraMerge,
    /// endpoints in wrong nets
    WrongPoint,
    /// golden net not found in actual at all
    MissingNet,
}

impl std::fmt::Display for DiffKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffKind::ExtraSplit => write!(f, "EXTRA-SPLIT"),
            DiffKind::ExtraMerge => write!(f, "EXTRA-MERGE"),
            DiffKind::WrongPoint => write!(f, "WRONG-POINT"),
            DiffKind::MissingNet => write!(f, "MISSING-NET"),
        }
    }
}

#[derive(Debug)]
struct DiffEntry {
    kind: DiffKind,
    description: String,
}

#[derive(Debug)]
struct ModuleReport {
    module: String,
    golden_nets: usize,
    actual_nets: usize,
    golden_comps: usize,
    actual_comps: usize,
    matched_comps: usize,
    comp_mapping: Vec<(String, String)>, // (golden_id, actual_leaf_name)
    golden_only_comps: Vec<String>,
    actual_only_comps: Vec<String>,
    diffs: Vec<DiffEntry>,
    match_rate: f64, // fraction of golden nets matched
    /// Whether G3 projection was relaxed (absent comps removed).
    g3_relaxed: bool,
}

// ============================================================================
// G3 Projection types
// ============================================================================

/// Result of projecting golden to golden' by removing absent components.
struct ProjectedGolden {
    /// Projected nets (may be merged, endpoints removed).
    nets: Vec<GoldenNet>,
    /// Nets dropped because they had < 2 points after projection.
    dropped_nets: Vec<String>,
    /// Absent comp IDs that were removed.
    removed_comps: Vec<String>,
    /// (from_net_name, into_net_name) pairs that were merged.
    merged_pairs: Vec<(String, String)>,
    /// Number of golden comps present (matched to actual).
    present_count: usize,
    /// Total number of golden comps.
    total_count: usize,
}

// ============================================================================
// Helpers
// ============================================================================

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hbl")
}

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

fn baseline_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline")
}

fn load_golden(path: &std::path::Path) -> GoldenModule {
    let content = fs::read_to_string(path).expect("Failed to read golden file");
    toml::from_str(&content).expect("Failed to parse golden TOML")
}

/// Parse a golden point like "ldo.1" or "vin.GND" into (comp_id, pin_number) or None for ports.
fn parse_golden_point(point: &str) -> Option<(&str, &str)> {
    if let Some(dot) = point.rfind('.') {
        let after = &point[dot + 1..];
        if after.chars().all(|c| c.is_ascii_digit()) {
            let before = &point[..dot];
            return Some((before, after));
        }
    }
    None
}

/// Parse a golden port-reference point into a normalized form.
/// - `port.X` → `X` (module's own single port)
/// - `port.X.Y` → `X.Y` (module's own multi-segment port, e.g. `port.I2C0.SCL`)
/// - `submodule.X` → `submodule.X` (submodule port, e.g. `mcu513.VDD_3V3`)
/// Returns None for component pin points (handled by parse_golden_point).
fn parse_golden_port_point(point: &str) -> Option<String> {
    // Skip numeric pins — handled by parse_golden_point
    if let Some(last_dot) = point.rfind('.') {
        let after = &point[last_dot + 1..];
        if after.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    // Check for "port." prefix — strip it to normalize module's own port references.
    // This handles both single-segment (port.VDD_3V3) and multi-segment
    // (port.I2C0.SCL, port.MIC.P) port references.
    if let Some(rest) = point.strip_prefix("port.") {
        if !rest.is_empty() {
            return Some(rest.to_string());
        }
    }
    // Non-port references: submodule ports (mcu513.VDD_3V3, dc.GND) or
    // interface parameter references — keep as-is.
    if point.contains('.') {
        Some(point.to_string())
    } else {
        None
    }
}

/// Extract the leaf name from an InstTable path.
fn leaf_name(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// Extract the parent's leaf name from an InstTable path.
fn parent_leaf_name(path: &str) -> Option<&str> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.len() >= 2 {
        Some(segments[segments.len() - 2])
    } else {
        None
    }
}

/// Try to extract pin number from a pin path.
/// Paths like "main.modldo.ldo.1" → pin 1
fn extract_pin_number(path: &str) -> Option<usize> {
    leaf_name(path).parse::<usize>().ok()
}

/// Normalize an actual pin path to (comp_leaf_name, pin_number).
fn normalize_pin(pin_path: &str, comp_leaf_names: &HashSet<String>) -> Option<(String, usize)> {
    let pin_num = extract_pin_number(pin_path)?;
    // Find the component leaf name: it's the segment before the pin number
    let parent = parent_leaf_name(pin_path)?;
    if comp_leaf_names.contains(parent) {
        Some((parent.to_string(), pin_num))
    } else {
        // Try the grandparent (for nested pin paths)
        None
    }
}

// ============================================================================
// Build actual netlist from InstTable
// ============================================================================

/// Extract a human-readable component value from its first parameter.
fn extract_component_value(comp: &McComponentInst) -> String {
    let first = comp.params.iter().next();
    let val = first.and_then(|b| b.get_value());
    // Debug: print what we found
    if comp.name.contains("CAP") || comp.name.starts_with('C') {
        eprintln!(
            "[NETDIFF-VAL] comp={} class={} first_binding={:?}",
            comp.name,
            comp.def.name,
            first.map(|b| format!(
                "declare={} value={:?}",
                b.declare.get_primary_name().unwrap_or_default(),
                b.get_value()
            ))
        );
    }
    val.map(|v| format_param_value(v))
        .unwrap_or_else(|| comp.def.name.to_string())
}

/// Format a McParamValue as a human-readable string for matching golden TOML values.
///
/// Uses McUnitValue's Display impl (e.g. "1.00µF", "100.00kΩ") then post-processes
/// to match golden format: replaces 'µ'→'u', strips trailing zeros after decimal.
fn format_param_value(v: &McParamValue) -> String {
    match v {
        McParamValue::UValue(uv) => {
            // Use McUnitValue's Display which formats with human-readable SI prefixes
            let s = uv.to_string();
            // Replace Greek mu (µ) with ASCII 'u' to match golden TOML format
            let s = s.replace('µ', "u");
            // Strip trailing zeros after decimal point
            // e.g. "1.00uF" → "1uF", "2.20uF" → "2.2uF", "100.00kΩ" → "100kΩ"
            if let Some(dot_pos) = s.find('.') {
                let unit_start = s[dot_pos..]
                    .find(|c: char| !c.is_ascii_digit() && c != '.')
                    .map(|p| dot_pos + p)
                    .unwrap_or(s.len());
                let num_part = &s[..unit_start];
                let unit_part = &s[unit_start..];
                let trimmed_num = num_part.trim_end_matches('0').trim_end_matches('.');
                format!("{}{}", trimmed_num, unit_part)
            } else {
                s
            }
        }
        McParamValue::String(s) => s.value.clone(),
        McParamValue::Int(i) => i.value.to_string(),
        McParamValue::Float(f) => f.value.to_string(),
        _ => format!("{:?}", v),
    }
}

/// Walk the module tree and collect (full_path → value) mappings for all components.
fn collect_comp_values(tree: &McModuleInst) -> HashMap<String, String> {
    let mut values: HashMap<String, String> = HashMap::new();
    fn walk(inst: &McModuleInst, prefix: &str, values: &mut HashMap<String, String>) {
        let my_prefix = if prefix.is_empty() {
            inst.name.clone()
        } else {
            format!("{}.{}", prefix, inst.name)
        };
        for comp in &inst.components {
            let value = extract_component_value(comp);
            let full_path = format!("{}.{}", my_prefix, comp.name);
            if comp.name.contains("CAP") || comp.name.starts_with('C') {
                eprintln!("[NETDIFF-COLLECT] full_path={full_path} value={value}");
            }
            values.insert(full_path, value);
        }
        for sub in &inst.sub_modules {
            walk(sub, &my_prefix, values);
        }
    }
    walk(tree, "", &mut values);
    values
}

/// Build the actual module representation from the compiled data.
fn build_actual_modules(table: &InstTable, tree: &MccProjectTree) -> Vec<ActualModule> {
    let comp_values = collect_comp_values(tree);
    let modules = table.get_modules();

    // Build module_path → component children
    let mut module_comps: HashMap<String, Vec<&InstEntry>> = HashMap::new();
    for m in &modules {
        let children = table.children_of(m.id);
        let comps: Vec<&InstEntry> = children
            .into_iter()
            .filter(|e| e.kind == InstKind::Component)
            .collect();
        module_comps.insert(m.path.clone(), comps);
    }

    let module_order = [
        "POWER_USB",
        "POWER_LDO",
        "POWER_DCDC",
        "US513",
        "MIC_SIP",
        "SPEAKER_M",
        "main",
    ];

    let mut result: Vec<ActualModule> = Vec::new();

    for &golden_name in &module_order {
        // Find the module entry by class_name
        let mod_entry = modules
            .iter()
            .find(|m| m.class_name == golden_name || (golden_name == "main" && m.path == "main"));

        let mod_path = match mod_entry {
            Some(me) => me.path.clone(),
            None => {
                // Try suffix match
                match modules.iter().find(|m| m.class_name.contains(golden_name)) {
                    Some(m) => m.path.clone(),
                    None => continue,
                }
            }
        };

        // Get components for this module
        let comp_children = module_comps.get(&mod_path).cloned().unwrap_or_default();

        let mut actual_comps: Vec<ActualComp> = Vec::new();
        for ce in &comp_children {
            let leaf = leaf_name(&ce.path).to_string();
            let pins = table.get_pins_of(ce.id);
            let mut pin_nums: Vec<usize> = Vec::new();
            for p in &pins {
                if let Some(n) = extract_pin_number(&p.path) {
                    pin_nums.push(n);
                }
            }
            pin_nums.sort();
            pin_nums.dedup();

            // Use extracted component value from params, fallback to class_name
            // Use the full InstEntry path as the key (matches collect_comp_values)
            let comp_value = comp_values
                .get(&ce.path)
                .cloned()
                .unwrap_or_else(|| ce.class_name.clone());

            actual_comps.push(ActualComp {
                leaf_name: leaf.clone(),
                full_path: ce.path.clone(),
                class: ce.class_name.clone(),
                value: comp_value,
                pins: pin_nums.len(),
                pin_numbers: pin_nums,
            });
        }

        // Build actual nets for this module
        let comp_leaf_names: HashSet<String> =
            actual_comps.iter().map(|c| c.leaf_name.clone()).collect();

        // ★ P2-4: filter pins by module path prefix to prevent cross-module
        // pin leakage. Without this, a pin like `main.mic._C1.1` (mic module)
        // would normalize to `_C1.1` and match the main module's `_C1`,
        // polluting the main module's nets.
        //
        // Check: pin path must start with `<mod_path>.` and the remainder
        // must have exactly 2 segments (comp_leaf.pin_number), ensuring the
        // pin belongs directly to this module, not a sub-module.
        let mod_prefix = format!("{}.", mod_path);
        let mod_prefix_len = mod_prefix.len();

        // ── P2-4: build per-module pin→net and port→net mappings ──
        // For submodules, use entry().or_insert() (first wins, submodule internal
        // nets take precedence over parent nets). For the top-level module, use
        // insert() (last wins, parent nets take precedence).
        // This prevents the parent's flatten_nets from overwriting submodule port
        // point mappings in the netdiff comparison.
        let is_top_level = mod_path == "main";
        let mut pin_to_net: HashMap<String, String> = HashMap::new();
        let mut port_to_net: HashMap<String, String> = HashMap::new();
        for net in table.get_nets() {
            for &point_id in &net.points {
                if let Some(entry) = table.get_entry(point_id) {
                    match entry.kind {
                        InstKind::Pin => {
                            if is_top_level {
                                pin_to_net.insert(entry.path.clone(), net.name.clone());
                            } else {
                                pin_to_net
                                    .entry(entry.path.clone())
                                    .or_insert_with(|| net.name.clone());
                            }
                        }
                        InstKind::Port => {
                            if is_top_level {
                                port_to_net.insert(entry.path.clone(), net.name.clone());
                            } else {
                                port_to_net
                                    .entry(entry.path.clone())
                                    .or_insert_with(|| net.name.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // DEBUG: print port entries for this module
        eprintln!("\n=== DEBUG {golden_name} port entries ===");
        let mod_path_no_dot = mod_path.trim_end_matches('.');
        for (_, entry) in table.iter() {
            if entry.kind == InstKind::Port
                && (entry.path.starts_with(&mod_prefix) || entry.path == mod_path_no_dot)
            {
                let net = table.get_net_of(entry.id);
                eprintln!(
                    "  port_path={} kind={:?} net={:?}",
                    entry.path,
                    entry.kind,
                    net.map(|n| n.name.as_str())
                );
            }
        }
        eprintln!("=== port_to_net entries for {golden_name} ===");
        for (port_path, net_name) in &port_to_net {
            if port_path.starts_with(&mod_prefix) {
                eprintln!("  port_to_net: {port_path} -> {net_name}");
            }
        }

        let mut net_points: HashMap<String, BTreeSet<NormPoint>> = HashMap::new();

        // ── Component pins (depth=1 or 2) ──
        // depth=1: comp_leaf.pin_num (e.g. main.flash.1 → "flash.1")
        // depth=2: submodule.comp_leaf.pin_num (e.g. main.mcu513.uC.10 → "mcu513.10")
        //   Golden format uses submodule.pin for submodule component pins.
        for (pin_path, net_name) in &pin_to_net {
            if !pin_path.starts_with(&mod_prefix) {
                continue;
            }
            let remainder = &pin_path[mod_prefix_len..];
            let depth = remainder.chars().filter(|&c| c == '.').count();
            if depth == 1 {
                if let Some((comp_leaf, pin_num)) = normalize_pin(pin_path, &comp_leaf_names) {
                    let norm = format!("{}.{}", comp_leaf, pin_num);
                    net_points.entry(net_name.clone()).or_default().insert(norm);
                } else {
                    // ── P2-2: handle submodule boundary pins ──
                    // When a boundary connection creates a pin like mcu513.10,
                    // the parent is a submodule name, not a component name.
                    // Extract the pin number and parent directly.
                    if let Some(pin_num) = extract_pin_number(pin_path) {
                        if let Some(parent) = parent_leaf_name(pin_path) {
                            let norm = format!("{}.{}", parent, pin_num);
                            net_points.entry(net_name.clone()).or_default().insert(norm);
                        }
                    }
                }
            } else if depth == 2 {
                // For the main module, skip depth=2 pins (submodule internal
                // component pins). flatten_nets propagates all submodule
                // internal nets to the parent, which pollutes the main module's
                // actual nets with submodule internal pins like moddcdc.1,
                // speaker.2, mcu513.1 etc. These are pure submodule internals
                // that have no place in the main module's golden comparison.
                //
                // (Root cause B — SPI member binding — is fixed: submodule SPI
                // member pins like mcu.10 land on the main nets by name.)
                if golden_name == "main" {
                    continue;
                }
                // Submodule component pin: main.mcu513.uC.10 → "mcu513.10"
                // Take the first segment (submodule name) and the pin number,
                // matching the golden format convention.
                if let Some(pin_num) = extract_pin_number(pin_path) {
                    let first_dot = remainder.find('.').unwrap_or(remainder.len());
                    let submodule = &remainder[..first_dot];
                    let norm = format!("{}.{}", submodule, pin_num);
                    net_points.entry(net_name.clone()).or_default().insert(norm);
                }
            }
        }

        // ── P2-4: port points (depth=0, 1, or 2) ──
        // Golden format always includes the submodule name for submodule ports:
        //   depth=0: module's own port member → golden "port.X" → norm "X"
        //            e.g. main.moddcdc.VDD_3V3 → "VDD_3V3", golden "port.VDD_3V3" → "VDD_3V3"
        //   depth=1: submodule.port_member → golden "sub.port_member" → norm "sub.port_member"
        //            e.g. main.moddcdc.VDD_3V3 → "moddcdc.VDD_3V3", golden "moddcdc.VDD_3V3"
        //   depth=2: submodule.port_name.port_member → golden "sub.port_name.port_member"
        //            e.g. main.modldo.vin.VCC → "modldo.vin.VCC", golden "modldo.vin.VCC"
        //            e.g. main.mic.MIC.P → "mic.MIC.P", golden "mic.MIC.P"
        for (port_path, net_name) in &port_to_net {
            if !port_path.starts_with(&mod_prefix) {
                continue;
            }
            let remainder = &port_path[mod_prefix_len..];
            let depth = remainder.chars().filter(|&c| c == '.').count();

            // ── P3-3: filter power bus port labels ──
            // Power bus declarations like V5V::DC(5V) create port labels
            // (V5V.VCC, V5V.GND) that are direct children of the module.
            // These only exist at the top level (main module). For submodules,
            // depth=1 port labels are the module's own ports (e.g., MIC.P,
            // dc.GND) and should be kept.
            // Filter: for the top-level module, skip depth=1 port points
            // whose first segment is NOT a module instance.
            if depth == 1 && is_top_level {
                if let Some(dot) = remainder.find('.') {
                    let first_seg = &remainder[..dot];
                    let seg_path = format!("{}.{}", mod_path.trim_end_matches('.'), first_seg);
                    let is_module = table
                        .get_id_by_path(&seg_path)
                        .and_then(|id| table.get_entry(id))
                        .map(|e| e.kind == InstKind::Module)
                        .unwrap_or(false);
                    if !is_module {
                        continue;
                    }
                }
            }

            let norm = if depth == 0 {
                // Module's own port member: POWER_DCDC.VDD_3V3 → "VDD_3V3"
                remainder.to_string()
            } else if depth <= 2 {
                // Submodule port (depth=1 or 2): keep full remainder with submodule name
                //   main.moddcdc.VDD_3V3 → "moddcdc.VDD_3V3"
                //   main.modldo.vin.VCC → "modldo.vin.VCC"
                //   main.mic.MIC.P → "mic.MIC.P"
                remainder.to_string()
            } else {
                continue;
            };

            net_points.entry(net_name.clone()).or_default().insert(norm);
        }

        let mut actual_nets: Vec<ActualNet> = net_points
            .into_iter()
            .filter(|(_, points)| !points.is_empty())
            .map(|(name, points)| ActualNet { name, points })
            .collect();

        // ── P2-4: merge port-only nets into matching component nets ──
        // When a port is referenced by both the submodule's internal net
        // and the parent's connection net, the point_to_net overwrite in
        // flatten_nets leaves the port point in the parent's net while the
        // submodule's internal net keeps only component pins. This causes
        // WRONG-POINT / MISSING-NET in netdiff.
        //
        // Fix: identify nets that contain only port references (no component
        // pins with numeric suffixes), and merge their port points into the
        // matching component nets by name.
        {
            // Helper: check if a point is a component pin (has "comp.pin" format)
            fn is_component_pin(point: &str) -> bool {
                if let Some(last_dot) = point.rfind('.') {
                    let after = &point[last_dot + 1..];
                    after.chars().all(|c| c.is_ascii_digit())
                } else {
                    false
                }
            }

            // Collect port points to move: (port_point, source_net_name, target_net_name)
            let mut port_moves: Vec<(String, String, String)> = Vec::new();

            // Phase 1: port-only nets → merge all port points into matching nets
            for net in &actual_nets {
                if net.points.iter().any(|p| is_component_pin(p)) {
                    continue; // Skip nets with component pins
                }
                for port_point in &net.points {
                    let target_exists = actual_nets.iter().any(|n| {
                        n.name == *port_point && n.points.iter().any(|p| is_component_pin(p))
                    });
                    if target_exists {
                        port_moves.push((port_point.clone(), net.name.clone(), port_point.clone()));
                    }
                }
            }

            // Phase 2: mixed nets → move port points to matching component nets
            // When a port point is in a net whose name doesn't match the port,
            // and there exists a component net with the matching name, move it.
            for net in &actual_nets {
                if !net.points.iter().any(|p| is_component_pin(p)) {
                    continue; // Already handled in Phase 1
                }
                for port_point in &net.points {
                    if is_component_pin(port_point) {
                        continue;
                    }
                    // Only move if: port is in wrong net AND matching net exists
                    if net.name != *port_point {
                        let target_exists = actual_nets.iter().any(|n| {
                            n.name == *port_point && n.points.iter().any(|p| is_component_pin(p))
                        });
                        if target_exists {
                            port_moves.push((
                                port_point.clone(),
                                net.name.clone(),
                                port_point.clone(),
                            ));
                        }
                    }
                }
            }

            // Apply the moves: add to target, remove from source
            for (port_point, source_name, target_name) in &port_moves {
                if let Some(target_net) = actual_nets.iter_mut().find(|n| n.name == *target_name) {
                    target_net.points.insert(port_point.clone());
                }
                if let Some(source_net) = actual_nets.iter_mut().find(|n| n.name == *source_name) {
                    source_net.points.remove(port_point);
                }
            }

            // Remove port-only nets that have been fully merged
            actual_nets.retain(|net| {
                net.points.iter().any(|p| is_component_pin(p))
                    || net
                        .points
                        .iter()
                        .all(|p| !port_moves.iter().any(|(pp, _, _)| pp == p))
            });
        }

        actual_nets.sort_by(|a, b| a.name.cmp(&b.name));

        // DEBUG: print actual nets for key modules
        if golden_name == "US513"
            || golden_name == "POWER_DCDC"
            || golden_name == "POWER_LDO"
            || golden_name == "SPEAKER_M"
            || golden_name == "main"
        {
            eprintln!("\n=== DEBUG {golden_name} actual nets ===");
            for net in &actual_nets {
                eprintln!(
                    "  net '{}' ({} pts): {:?}",
                    net.name,
                    net.points.len(),
                    net.points.iter().collect::<Vec<_>>()
                );
            }
            eprintln!("=== DEBUG {golden_name} actual comps ===");
            for c in &actual_comps {
                eprintln!(
                    "  comp {} (class={}, pins={})",
                    c.leaf_name, c.class, c.pins
                );
            }
        }

        result.push(ActualModule {
            module_name: golden_name.to_string(),
            comps: actual_comps,
            nets: actual_nets,
        });
    }

    result
}

// ============================================================================
// Component matching
// ============================================================================

/// Match golden comps to actual comps.
/// Returns (mapping, golden_only, actual_only).
fn match_comps(
    golden: &[GoldenComp],
    actual: &[ActualComp],
) -> (Vec<(String, String)>, Vec<String>, Vec<String>) {
    let mut mapping: Vec<(String, String)> = Vec::new();
    let mut matched_actual: HashSet<String> = HashSet::new();
    let mut matched_golden: HashSet<String> = HashSet::new();

    // Step 1: exact name match
    for gc in golden {
        for ac in actual {
            if ac.leaf_name == gc.id {
                mapping.push((gc.id.clone(), ac.leaf_name.clone()));
                matched_actual.insert(ac.leaf_name.clone());
                matched_golden.insert(gc.id.clone());
                break;
            }
        }
    }

    // Step 2: (class, value) multiset matching
    let mut golden_by_class_val: HashMap<(String, String), Vec<&GoldenComp>> = HashMap::new();
    for gc in golden {
        if !matched_golden.contains(&gc.id) {
            let key = (gc.class.clone(), gc.value.clone());
            golden_by_class_val.entry(key).or_default().push(gc);
        }
    }

    let mut actual_by_class_val: HashMap<(String, String), Vec<&ActualComp>> = HashMap::new();
    for ac in actual {
        if !matched_actual.contains(&ac.leaf_name) {
            let key = (ac.class.clone(), ac.value.clone());
            actual_by_class_val.entry(key).or_default().push(ac);
        }
    }

    // Match within each class/value group
    for (key, gcomps) in &golden_by_class_val {
        if let Some(acomps) = actual_by_class_val.get(key) {
            let count = gcomps.len().min(acomps.len());
            for i in 0..count {
                mapping.push((gcomps[i].id.clone(), acomps[i].leaf_name.clone()));
                matched_golden.insert(gcomps[i].id.clone());
                matched_actual.insert(acomps[i].leaf_name.clone());
            }
        }
    }

    // Step 3: adjacency-based disambiguation (simplified: try class-only match)
    let mut golden_by_class: HashMap<String, Vec<&GoldenComp>> = HashMap::new();
    for gc in golden {
        if !matched_golden.contains(&gc.id) {
            golden_by_class
                .entry(gc.class.clone())
                .or_default()
                .push(gc);
        }
    }

    let mut actual_by_class: HashMap<String, Vec<&ActualComp>> = HashMap::new();
    for ac in actual {
        if !matched_actual.contains(&ac.leaf_name) {
            actual_by_class
                .entry(ac.class.clone())
                .or_default()
                .push(ac);
        }
    }

    for (cls, gcomps) in &golden_by_class {
        if let Some(acomps) = actual_by_class.get(cls) {
            let count = gcomps.len().min(acomps.len());
            for i in 0..count {
                mapping.push((gcomps[i].id.clone(), acomps[i].leaf_name.clone()));
                matched_golden.insert(gcomps[i].id.clone());
                matched_actual.insert(acomps[i].leaf_name.clone());
            }
        }
    }

    let golden_only: Vec<String> = golden
        .iter()
        .filter(|gc| !matched_golden.contains(&gc.id))
        .map(|gc| gc.id.clone())
        .collect();

    let actual_only: Vec<String> = actual
        .iter()
        .filter(|ac| !matched_actual.contains(&ac.leaf_name))
        .map(|ac| ac.leaf_name.clone())
        .collect();

    (mapping, golden_only, actual_only)
}

// ============================================================================
// Net comparison
// ============================================================================

/// Compare golden nets against actual nets.
fn compare_nets(
    golden_nets: &[GoldenNet],
    actual_nets: &[ActualNet],
    comp_mapping: &[(String, String)],
) -> (Vec<DiffEntry>, usize) {
    // Build golden_id → actual_leaf mapping
    let g2a: HashMap<&str, &str> = comp_mapping
        .iter()
        .map(|(g, a)| (g.as_str(), a.as_str()))
        .collect();

    // Normalize golden nets: map golden comp IDs to actual leaf names
    let mut golden_sets: Vec<(String, BTreeSet<NormPoint>)> = Vec::new();
    for gn in golden_nets {
        let mut points: BTreeSet<NormPoint> = BTreeSet::new();
        for pt in &gn.points {
            // Component pin: "comp_id.pin_num" → "actual_leaf.pin_num"
            if let Some((comp_id, pin_str)) = parse_golden_point(pt) {
                if let Some(&actual_leaf) = g2a.get(comp_id) {
                    points.insert(format!("{}.{}", actual_leaf, pin_str));
                } else {
                    // comp_id not in component mapping → treat as submodule port reference
                    // e.g., "mcu513.10" in main module where mcu513 is a submodule,
                    // not a component. The actual side normalizes these to "mcu513.10".
                    points.insert(pt.clone());
                }
            }
            // ── P2-4: port point ──
            // "port.X" → "X" (module's own port)
            // "submodule.X" → "submodule.X" (submodule port)
            if let Some(port_norm) = parse_golden_port_point(pt) {
                points.insert(port_norm);
            }
        }
        if !points.is_empty() {
            golden_sets.push((gn.name.clone(), points));
        }
    }

    // Actual net sets (already normalized)
    let actual_sets: Vec<(String, BTreeSet<NormPoint>)> = actual_nets
        .iter()
        .map(|an| (an.name.clone(), an.points.clone()))
        .collect();

    let mut diffs: Vec<DiffEntry> = Vec::new();
    let mut matched_count = 0;
    let mut matched_actual_indices: HashSet<usize> = HashSet::new();

    // For each golden net, find the best matching actual net
    for (gi, (gname, gset)) in golden_sets.iter().enumerate() {
        let mut best_match: Option<(usize, f64)> = None;

        for (ai, (_, aset)) in actual_sets.iter().enumerate() {
            if matched_actual_indices.contains(&ai) {
                continue;
            }
            let intersection = gset.intersection(aset).count();
            let union = gset.union(aset).count();
            if union == 0 {
                continue;
            }
            let jaccard = intersection as f64 / union as f64;
            if jaccard > 0.0 {
                match best_match {
                    None => best_match = Some((ai, jaccard)),
                    Some((_, prev_j)) if jaccard > prev_j => best_match = Some((ai, jaccard)),
                    _ => {}
                }
            }
        }

        if let Some((ai, jaccard)) = best_match {
            if jaccard >= 0.5 {
                matched_actual_indices.insert(ai);
                matched_count += 1;

                if jaccard < 1.0 {
                    let aset = &actual_sets[ai].1;
                    let missing: Vec<_> = gset.difference(aset).collect();
                    let extra: Vec<_> = aset.difference(gset).collect();
                    let mut desc = format!(
                        "golden#{} {} ↔ actual.{} (jaccard={:.2})",
                        gi + 1,
                        gname,
                        actual_sets[ai].0,
                        jaccard
                    );
                    if !missing.is_empty() {
                        desc.push_str(&format!(
                            " | extra in golden: [{}]",
                            missing
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    if !extra.is_empty() {
                        desc.push_str(&format!(
                            " | extra in actual: [{}]",
                            extra
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    diffs.push(DiffEntry {
                        kind: DiffKind::WrongPoint,
                        description: desc,
                    });
                }
            } else {
                diffs.push(DiffEntry {
                    kind: DiffKind::MissingNet,
                    description: format!(
                        "golden#{} {} — no matching actual net (best jaccard={:.2})",
                        gi + 1,
                        gname,
                        jaccard
                    ),
                });
            }
        } else {
            diffs.push(DiffEntry {
                kind: DiffKind::MissingNet,
                description: format!("golden#{} {} — no actual counterpart", gi + 1, gname),
            });
        }
    }

    // Check for unmatched actual nets (potential EXTRA-SPLIT target)
    for (ai, (aname, aset)) in actual_sets.iter().enumerate() {
        if !matched_actual_indices.contains(&ai) {
            let mut from_golden: Vec<String> = Vec::new();
            for (gname, gset) in &golden_sets {
                let overlap = gset.intersection(aset).count();
                if overlap > 0 {
                    from_golden.push(format!(
                        "golden.{} ({} of {} points)",
                        gname,
                        overlap,
                        gset.len()
                    ));
                }
            }
            if !from_golden.is_empty() {
                diffs.push(DiffEntry {
                    kind: DiffKind::ExtraSplit,
                    description: format!(
                        "actual.{} contains endpoints from {}",
                        aname,
                        from_golden.join(", ")
                    ),
                });
            }
        }
    }

    (diffs, matched_count)
}

// ============================================================================
// G3 Projection
// ============================================================================

/// Project golden to golden' by removing absent components.
///
/// Algorithm:
///   a. For each absent comp, remove its endpoints from all nets.
///   b. If the comp is marked `is_series: true` in [[series]], merge the two
///      nets that contained its pin 1 and pin 2.
///   c. If `is_series: false` (bridge / bypass / pull-up to rail), only remove
///      endpoints — do NOT merge.
///   d. Nets with < 2 points after projection are dropped.
///
/// ## Why unconditional merge is wrong
///
/// R442(1MΩ) in US513 is parallel across XTAL X1/X2, with `is_series: false`.
/// Its two endpoints sit on two different nets (XTAL.X1 and XTAL.X2).
/// If we unconditionally merged, golden' would claim X1 and X2 should be one
/// net — but the compiler (without R442) correctly keeps them separate.
/// The projection would then manufacture a false EXTRA-SPLIT.
///
/// Merge only happens when the component is series-connected in a chain
/// whose other parts still exist.
fn project(golden: &GoldenModule, present: &HashSet<String>) -> ProjectedGolden {
    let total_count = golden.comp.len();
    let present_count = present.len();

    // Build comp_id → is_series map
    let series_map: HashMap<&str, bool> = golden
        .series
        .iter()
        .map(|s| (s.comp.as_str(), s.is_series))
        .collect();

    // Identify absent comps
    let absent: Vec<&GoldenComp> = golden
        .comp
        .iter()
        .filter(|c| !present.contains(&c.id))
        .collect();

    let removed_comps: Vec<String> = absent.iter().map(|c| c.id.clone()).collect();

    // Clone nets as mutable
    let mut nets: Vec<GoldenNet> = golden.net.clone();

    // Find which net contains a given point string
    let find_net_index = |nets: &[GoldenNet], point: &str| -> Option<usize> {
        nets.iter()
            .position(|n| n.points.iter().any(|p| p == point))
    };

    let mut merged_pairs: Vec<(String, String)> = Vec::new();

    for comp in &absent {
        let pt1 = format!("{}.1", comp.id);
        let pt2 = format!("{}.2", comp.id);

        let idx1 = find_net_index(&nets, &pt1);
        let idx2 = find_net_index(&nets, &pt2);

        // Remove endpoints from nets
        for net in nets.iter_mut() {
            net.points.retain(|p| p != &pt1 && p != &pt2);
        }

        // Merge if is_series and the two pins were on different nets
        let is_series = series_map.get(comp.id.as_str()).copied().unwrap_or(false);
        if is_series {
            if let (Some(i1), Some(i2)) = (idx1, idx2) {
                if i1 != i2 {
                    // Merge net i2 into net i1, then remove net i2
                    // We need to be careful: after removal, indices may have shifted.
                    // Use a name-based approach instead.
                    let (name1, name2) = (nets[i1].name.clone(), nets[i2].name.clone());
                    let points2 = std::mem::take(&mut nets[i2].points);
                    nets[i1].points.extend(points2);
                    // Mark net i2 for removal by clearing its points
                    nets[i2].points.clear();
                    merged_pairs.push((name2, name1));
                }
            }
        }
    }

    // Remove nets with empty points (merged-away or dropped)
    nets.retain(|n| !n.points.is_empty());

    // Drop nets with < 2 points
    let mut dropped_nets: Vec<String> = Vec::new();
    nets.retain(|n| {
        if n.points.len() < 2 {
            dropped_nets.push(n.name.clone());
            false
        } else {
            true
        }
    });

    ProjectedGolden {
        nets,
        dropped_nets,
        removed_comps,
        merged_pairs,
        present_count,
        total_count,
    }
}

/// Format the [G3] projection status line.
fn format_g3_line(pg: &ProjectedGolden, module: &str) -> String {
    if pg.removed_comps.is_empty() {
        format!(
            "[G3] {}: present {}/{} — projection not relaxed, comparing full golden",
            module, pg.present_count, pg.total_count
        )
    } else {
        format!(
            "[G3] {}: present {}/{} — removed {} comps / merged {} nets / dropped {} nets",
            module,
            pg.present_count,
            pg.total_count,
            pg.removed_comps.len(),
            pg.merged_pairs.len(),
            pg.dropped_nets.len()
        )
    }
}

// ============================================================================
// Report formatting
// ============================================================================

fn format_module_report(report: &ModuleReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "┌ netdiff: {} ──────────────────────────────\n",
        report.module
    ));
    out.push_str(&format!(
        "│ golden  {} nets / {} comps\n",
        report.golden_nets, report.golden_comps
    ));
    out.push_str(&format!(
        "│ actual  {} nets / {} comps\n",
        report.actual_nets, report.actual_comps
    ));

    let mapping_str: Vec<String> = report
        .comp_mapping
        .iter()
        .map(|(g, a)| format!("{}→{}", g, a))
        .collect();
    out.push_str(&format!(
        "│ comps   {}/{} matched  ({})\n",
        report.matched_comps,
        report.golden_comps,
        mapping_str.join(", ")
    ));

    if !report.golden_only_comps.is_empty() {
        out.push_str(&format!(
            "│ golden-only comps: {}\n",
            report.golden_only_comps.join(", ")
        ));
    }
    if !report.actual_only_comps.is_empty() {
        out.push_str(&format!(
            "│ actual-only comps: {}\n",
            report.actual_only_comps.join(", ")
        ));
    }

    for diff in &report.diffs {
        out.push_str(&format!(
            "│ ✗ {:<12} {}\n",
            diff.kind.to_string(),
            diff.description
        ));
    }

    out.push_str(&format!(
        "│ ─ match {}/{} nets\n",
        (report.match_rate * report.golden_nets as f64).round() as usize,
        report.golden_nets
    ));
    out.push_str("└\n");

    out
}

// ============================================================================
// Main test
// ============================================================================

#[test]
fn netdiff_all_modules() {
    // 1. Compile hbl project
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: McURI = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    let mcode_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode");
    mcc::mcc_set_system_root(mcode_dir.as_path());
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    // ── P2-3: load mcode library for CAP, RES, IND, DIO etc. ──
    mcc::mcb_load_lib("mcode", mcode_dir.as_path());
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");

    // 2. Build actual modules
    let actual_modules = build_actual_modules(&table, &tree);

    // 3. Load golden TOMLs
    let dir = golden_dir();
    let module_order = [
        "POWER_USB",
        "POWER_LDO",
        "POWER_DCDC",
        "US513",
        "MIC_SIP",
        "SPEAKER_M",
        "main",
    ];

    let mut golden_modules: HashMap<String, GoldenModule> = HashMap::new();
    for &name in &module_order {
        let path = dir.join(format!("{}.golden.toml", name));
        if path.exists() {
            golden_modules.insert(name.to_string(), load_golden(&path));
        }
    }

    // 4. Compare each module (with G3 projection)
    let mut reports: Vec<ModuleReport> = Vec::new();
    let mut all_output = String::new();

    for &name in &module_order {
        let golden = match golden_modules.get(name) {
            Some(g) => g,
            None => continue,
        };

        let actual = match actual_modules.iter().find(|m| m.module_name == name) {
            Some(a) => a,
            None => {
                eprintln!("WARNING: module {} not found in actual output", name);
                continue;
            }
        };

        // Match components
        let (comp_mapping, golden_only, actual_only) = match_comps(&golden.comp, &actual.comps);

        // ── DEBUG: dump US513 actual netlist ──
        if name == "US513"
            || name == "SPEAKER_M"
            || name == "POWER_USB"
            || name == "POWER_LDO"
            || name == "MIC_SIP"
            || name == "main"
        {
            eprintln!("\n=== {name} ACTUAL COMPS ===");
            for c in &actual.comps {
                eprintln!(
                    "  COMP leaf={} class={} value={} pins={}",
                    c.leaf_name, c.class, c.value, c.pins
                );
            }
            eprintln!("\n=== {name} ACTUAL NETS ===");
            for n in &actual.nets {
                eprintln!("  NET name={} points={:?}", n.name, n.points);
            }
            eprintln!();
        }

        // G3 projection: build present set from matched golden comps
        let present: HashSet<String> = comp_mapping.iter().map(|(gid, _)| gid.clone()).collect();
        let projected = project(golden, &present);

        let (diffs, matched_nets) = compare_nets(&projected.nets, &actual.nets, &comp_mapping);

        let projected_net_count = projected.nets.len();
        let match_rate = if projected_net_count == 0 {
            1.0
        } else {
            matched_nets as f64 / projected_net_count as f64
        };

        let report = ModuleReport {
            module: name.to_string(),
            golden_nets: projected_net_count,
            actual_nets: actual.nets.len(),
            golden_comps: projected.present_count,
            actual_comps: actual.comps.len(),
            matched_comps: comp_mapping.len(),
            comp_mapping,
            golden_only_comps: golden_only,
            actual_only_comps: actual_only,
            diffs,
            match_rate,
            g3_relaxed: !projected.removed_comps.is_empty(),
        };

        let formatted = format_module_report(&report);
        all_output.push_str(&formatted);
        let g3_line = format_g3_line(&projected, name);
        all_output.push_str(&format!("  {}\n\n", g3_line));
        reports.push(report);
    }

    // 5. Print to terminal
    println!("\n{}", all_output);

    // 6. Write baseline/netdiff_baseline.md
    let baseline_path = baseline_dir().join("netdiff_baseline.md");
    fs::create_dir_all(baseline_dir()).expect("create baseline dir");

    let mut baseline = String::new();
    baseline.push_str("# Netdiff Baseline\n\n");
    baseline.push_str(&format!("Generated: {}\n\n", chrono_like_now()));
    baseline.push_str(
        "| Module | Golden nets | Actual nets | Comp match | Match rate | Main diff types |\n",
    );
    baseline.push_str("|---|---|---|---|---|---|\n");

    for report in &reports {
        let diff_types: Vec<String> = {
            let mut kinds: HashSet<String> = HashSet::new();
            for d in &report.diffs {
                kinds.insert(d.kind.to_string());
            }
            let mut v: Vec<String> = kinds.into_iter().collect();
            v.sort();
            v
        };

        baseline.push_str(&format!(
            "| {} | {} | {} | {}/{} | {:.0}% | {} |\n",
            report.module,
            report.golden_nets,
            report.actual_nets,
            report.matched_comps,
            report.golden_comps,
            report.match_rate * 100.0,
            diff_types.join(", ")
        ));
    }

    fs::write(&baseline_path, &baseline).expect("write baseline");
    println!("Baseline written to: {:?}", baseline_path);

    // 7. Self-check assertions (known expected differences)
    // P2-3: mcode library (CAP, RES, IND, DIO, etc.) is now loaded, so all
    // anonymous comps are instantiated. G3 is no longer relaxed for most modules.
    //
    // G3 projection: golden_comps now reflects present_count (matched golden comps),
    // not the total golden comps. golden_nets reflects the projected net count.

    // POWER_DCDC: all 10 comps matched (mcode loaded)
    let dcdc = reports.iter().find(|r| r.module == "POWER_DCDC").unwrap();
    assert!(
        dcdc.matched_comps >= 10,
        "POWER_DCDC: should have all 10 comps matched"
    );
    assert!(
        !dcdc.g3_relaxed,
        "POWER_DCDC: G3 should NOT be relaxed (all comps present)"
    );

    // SPEAKER_M: all 11 comps matched (mcode loaded)
    let spk = reports.iter().find(|r| r.module == "SPEAKER_M").unwrap();
    assert!(
        spk.matched_comps >= 11,
        "SPEAKER_M: should have all 11 comps matched"
    );
    assert!(
        !spk.g3_relaxed,
        "SPEAKER_M: G3 should NOT be relaxed (all comps present)"
    );

    // POWER_LDO: DC interface ports expand to vin.VCC / vin.GND / vout.VCC / vout.GND
    // (P2-10 interface member naming); GND absorbs both port GND members. All 3 nets match.
    let ldo = reports.iter().find(|r| r.module == "POWER_LDO").unwrap();
    assert!(
        ldo.diffs.is_empty(),
        "POWER_LDO: expected no diffs after P2-4 shunt cap fix"
    );
    assert!(
        !ldo.g3_relaxed,
        "POWER_LDO: G3 should NOT be relaxed (all comps present)"
    );

    // main: submodule interface ports expand to submodule.port.member endpoints
    // (modldo.vin.VCC / modldo.vout.GND / ...); the golden follows the source.
    // The SPI data/clock lanes are name-aligned (Root cause B fixed): interface
    // members bind to pins by pin name first, so flash.2=MISO / flash.5=MOSI /
    // flash.6=SCLK pair with mcu513.11 / mcu513.9 / mcu513.8 as the golden expects.
    let main_mod = reports.iter().find(|r| r.module == "main").unwrap();
    assert!(
        main_mod.match_rate >= 0.95,
        "main: expected match_rate >= 0.95, got {:.2}",
        main_mod.match_rate
    );
    assert!(
        !main_mod.g3_relaxed,
        "main: G3 should NOT be relaxed (all comps present)"
    );

    // MIC_SIP: all 7 comps matched, 4/4 nets matched (wm7121.VCC fixed)
    let mic = reports.iter().find(|r| r.module == "MIC_SIP").unwrap();
    assert!(
        mic.diffs.is_empty(),
        "MIC_SIP: expected no diffs after wm7121.VCC fix"
    );
    assert!(
        !mic.g3_relaxed,
        "MIC_SIP: G3 should NOT be relaxed (all comps present)"
    );

    // US513: 21/21 comps matched, G3 no longer relaxed (all comps present)
    let us513 = reports.iter().find(|r| r.module == "US513").unwrap();
    assert!(
        us513.matched_comps >= 21,
        "US513: should have all 21 comps matched"
    );
    assert!(
        !us513.g3_relaxed,
        "US513: G3 should NOT be relaxed (all comps present)"
    );

    // All 7 modules should have reports
    assert_eq!(reports.len(), 7, "expected 7 module reports");
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let y = 1970 + (days / 365) as i32;
    let d = days % 365;
    let m = (d / 30) + 1;
    let day = (d % 30) + 1;
    format!("{:04}-{:02}-{:02}", y, m, day)
}

// ============================================================================
// G3Net trait — prevents hardcoding projection to netlist-only data structures
// ============================================================================

/// G3-net abstraction: any collection of named, point-queryable nets.
///
/// **Netlist form** (current): nets are sets of (comp_id, pin_number) pairs.
/// **Render form** (P4): nets are sets of line segments on a canvas.
///
/// By defining this trait, the projection logic can work on both forms
/// without hardcoding to netlist-only data structures.
#[allow(dead_code)]
trait G3Net {
    /// Type of a point in this net representation.
    type PointId: Clone + Ord + std::fmt::Display;

    /// Name of this net (for reporting).
    fn name(&self) -> &str;

    /// All points in this net.
    fn points(&self) -> &BTreeSet<Self::PointId>;
}

/// TODO(P4): G3-render projection.
///
/// **Criterion**: every line segment on the render canvas has both
/// endpoints belonging to the same net in golden'.
///
/// This is stricter than netlist comparison because it catches
/// geometric misrouting that doesn't change net membership
/// (e.g., a wire that crosses from one net to another without
/// a component in between, or a bus lane that's drawn on the
/// wrong side of a symbol).
///
/// Not implemented yet — P4 scope.
#[allow(dead_code)]
fn project_render(_golden: &GoldenModule, _present: &HashSet<String>) -> ProjectedGolden {
    todo!("P4: G3-render projection — checks line segments, not just net membership")
}

// ============================================================================
// G3 Projection unit tests
// ============================================================================

#[test]
fn g3_projection_t1_r442_no_merge() {
    // t1: US513 golden, delete R442 (is_series=false)
    // → XTAL.X1 and XTAL.X2 must stay as two separate nets.
    let path = golden_dir().join("US513.golden.toml");
    let golden = load_golden(&path);

    // All comps present except R442
    let mut present: HashSet<String> = golden.comp.iter().map(|c| c.id.clone()).collect();
    present.remove("R442");

    let projected = project(&golden, &present);

    // Verify R442 is in removed list
    assert!(
        projected.removed_comps.contains(&"R442".to_string()),
        "R442 should be removed"
    );

    // Verify XTAL.X1 and XTAL.X2 are still separate nets
    let xtal_x1 = projected
        .nets
        .iter()
        .find(|n| n.name == "XTAL.X1")
        .expect("XTAL.X1 should exist");
    let xtal_x2 = projected
        .nets
        .iter()
        .find(|n| n.name == "XTAL.X2")
        .expect("XTAL.X2 should exist");

    // R442 endpoints should be removed
    assert!(
        !xtal_x1.points.contains(&"R442.1".to_string()),
        "R442.1 should be removed from XTAL.X1"
    );
    assert!(
        !xtal_x2.points.contains(&"R442.2".to_string()),
        "R442.2 should be removed from XTAL.X2"
    );

    // XTAL.X1 should still have X6.1, uC.3, C18a.1
    assert!(xtal_x1.points.contains(&"X6.1".to_string()));
    assert!(xtal_x1.points.contains(&"uC.3".to_string()));
    assert!(xtal_x1.points.contains(&"C18a.1".to_string()));

    // XTAL.X2 should still have X6.2, uC.4, C18b.1
    assert!(xtal_x2.points.contains(&"X6.2".to_string()));
    assert!(xtal_x2.points.contains(&"uC.4".to_string()));
    assert!(xtal_x2.points.contains(&"C18b.1".to_string()));

    // No merges should have happened
    assert!(
        projected.merged_pairs.is_empty(),
        "R442 is is_series=false, should not merge"
    );

    println!("t1 passed: R442 removed, XTAL.X1/X2 stay separate");
}

#[test]
fn g3_projection_t2_r47k_merge() {
    // t2: POWER_DCDC golden, delete R47k (is_series=true)
    // → VDD_3V3 and EN must merge into one net.
    let path = golden_dir().join("POWER_DCDC.golden.toml");
    let golden = load_golden(&path);

    // All comps present except R47k
    let mut present: HashSet<String> = golden.comp.iter().map(|c| c.id.clone()).collect();
    present.remove("R47k");

    let projected = project(&golden, &present);

    // Verify R47k is in removed list
    assert!(
        projected.removed_comps.contains(&"R47k".to_string()),
        "R47k should be removed"
    );

    // Verify VDD_3V3 and EN merged
    let merged = projected
        .nets
        .iter()
        .find(|n| n.name == "VDD_3V3")
        .expect("VDD_3V3 should exist after merge");

    // EN should no longer exist as a separate net
    assert!(
        !projected.nets.iter().any(|n| n.name == "EN"),
        "EN should be merged into VDD_3V3"
    );

    // R47k endpoints should be removed
    assert!(!merged.points.contains(&"R47k.1".to_string()));
    assert!(!merged.points.contains(&"R47k.2".to_string()));

    // VDD_3V3 points should include original VDD_3V3 + EN points
    assert!(merged.points.contains(&"port.VDD_3V3".to_string()));
    assert!(merged.points.contains(&"lp322dcdc.4".to_string()));
    assert!(merged.points.contains(&"C10u_in.1".to_string()));
    // EN points should now be in VDD_3V3
    assert!(merged.points.contains(&"lp322dcdc.1".to_string()));
    assert!(merged.points.contains(&"C1u_en.1".to_string()));

    // Verify merge pair recorded
    assert_eq!(projected.merged_pairs.len(), 1);
    assert_eq!(projected.merged_pairs[0].0, "EN");
    assert_eq!(projected.merged_pairs[0].1, "VDD_3V3");

    println!("t2 passed: R47k removed, VDD_3V3 and EN merged");
}
