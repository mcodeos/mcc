// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Netlist health check (Tier 0 · NETLIST CORRECTNESS)
//!
//! **This module is read-only; it does not modify any data.** It runs once
//! after pass2 ends and before entering viz, answering one question:
//! *is the current netlist electrically correct?*
//!
//! The existing Tier 1 CORRECTNESS check verifies "render completeness" (no
//! NaN / stays on canvas / every net is drawn) — it is all-green for an
//! **electrically wrong** netlist. That is why short circuits can survive
//! for a long time. This module adds that layer.
//!
//! ## Usage
//!
//! ```ignore
//! let report = netcheck::run(&inst_table);
//! report.print();                  // prints the table
//! if !report.is_clean() {
//!     // fail here in CI
//! }
//! ```
//!
//! To hook up the pass1 symbol counts for the conservation check (R10),
//! pass a table of `module_path -> pass1 component count`:
//!
//! ```ignore
//! let report = netcheck::run_with_expectation(&inst_table, &expect);
//! ```
//!
//! ## Rule overview
//!
//! | rule | level | meaning |
//! |---|---|---|
//! | R01 LITERAL_POINT      | ERROR | endpoint path contains `{` `[` `,` — a vector reference was not expanded |
//! | R02 SHORT_PASSIVE      | ERROR | both pins of a two-terminal device land on the same net |
//! | R03 SHORT_RAIL         | ERROR | a net contains two different power-domain names (including VDD and GND on the same net) |
//! | R04 SHORT_LANE         | ERROR | two different members of the same bus land on the same net |
//! | R05 UNRESOLVED_UNIT    | ERROR | a unit-typed argument cannot claim any formal parameter slot |
//! | R06 MEGANET            | WARN  | non-power net has too many points and spans too many devices |
//! | R07 GHOST_INSTANCE     | ERROR | a device referenced in a net is missing from the instance table |
//! | R09 FLOATING_POWER_PIN | WARN  | a device's power / ground pin is not connected |
//! | R10 SYMBOL_CONSERVATION| ERROR | pass2 device count < pass1 symbol table device count (expectation must be passed in) |
//! | R11 SPLIT_RAIL         | ERROR | same-name power net inside one module is split into multiple mutually unconnected nets |
//! | R12 DANGLING_PORT      | INFO  | a port net has only itself as a point |
//! | R14 ORPHAN_INSTANCE     | WARN  | instance registered but not in any net |
//! | R15 SYNTHETIC_PIN       | WARN  | synthetic terminal (pin_id not belonging to any real pin, from port scalar/member handling) |

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;

use super::insttab::{InstKind, InstTable};

// ============================================================================
// Configuration constants
// ============================================================================

/// R06: a non-power net is suspicious once it exceeds this many points
const MEGANET_POINTS: usize = 8;
/// R06: and only once it spans this many different devices (pure fan-out signal nets don't count)
const MEGANET_OWNERS: usize = 3;

// ============================================================================
// Result types
// ============================================================================

/// Rule level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// report only; does not affect the gate
    Info,
    /// suspicious; does not affect the gate (but the trend should go down)
    Warn,
    /// the netlist is wrong; the gate must turn red
    Error,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Info => "INFO ",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
        }
    }
}

/// A single rule violation record
#[derive(Debug, Clone)]
pub struct Finding {
    /// Rule number, e.g. "R01"
    pub rule: &'static str,
    pub level: Level,
    /// Path of the module this finding belongs to (best effort; empty when unavailable)
    pub module: String,
    /// Human-readable one-line description
    pub detail: String,
}

/// The health-check report
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// Hit count per rule (includes rules with 0 hits, so the table is stable)
    pub counts: BTreeMap<&'static str, usize>,
    /// Number of objects scanned by each rule this round (0 means the rule did not actually run)
    pub scanned: BTreeMap<&'static str, usize>,
    /// Aggregate statistics
    pub total_nets: usize,
    pub total_components: usize,
    pub total_modules: usize,
}

impl Report {
    /// No ERROR-level violations
    pub fn is_clean(&self) -> bool {
        !self.findings.iter().any(|f| f.level == Level::Error)
    }

    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.level == Level::Error)
            .count()
    }

    /// Render as a table string
    pub fn render(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "┌ netcheck ─────────────────────────────────────────────────────────"
        );
        let _ = writeln!(
            s,
            "│ {} modules / {} components / {} nets",
            self.total_modules, self.total_components, self.total_nets
        );
        let _ = writeln!(
            s,
            "├───────────────────────────────────────────────────────────────────"
        );
        let _ = writeln!(s, "│ rule                          hits (unique/total)");

        for (rule, n) in &self.counts {
            let lvl = rule_level(rule);
            let scanned = self.scanned.get(rule).copied().unwrap_or(0);
            let mark = if scanned == 0 {
                "·"
            } else if *n == 0 {
                "✓"
            } else if lvl == Level::Error {
                "✗"
            } else {
                "·"
            };
            let status = if scanned == 0 { "SKIP" } else { "" };
            let _ = writeln!(
                s,
                "│ {} {} {:<22} {:>4}  {}",
                mark,
                lvl.tag(),
                rule_name(rule),
                n,
                status
            );
        }

        if !self.findings.is_empty() {
            let _ = writeln!(
                s,
                "├─ Details ────────────────────────────────────────────────────────────"
            );
            // Sort by (module, rule) so the output is stable
            let mut sorted: Vec<&Finding> = self.findings.iter().collect();
            sorted.sort_by(|a, b| {
                (a.module.as_str(), a.rule, a.detail.as_str()).cmp(&(
                    b.module.as_str(),
                    b.rule,
                    b.detail.as_str(),
                ))
            });
            let mut cur_mod = String::from("\u{0}");
            for f in sorted {
                if f.module != cur_mod {
                    cur_mod = f.module.clone();
                    let name = if cur_mod.is_empty() {
                        "<top-level/unattributed>"
                    } else {
                        cur_mod.as_str()
                    };
                    let _ = writeln!(s, "│ ── {name}");
                }
                let _ = writeln!(s, "│   [{}] {}", f.rule, f.detail);
            }
        }

        let total_errors: usize = self
            .counts
            .iter()
            .filter(|(rule, _)| rule_level(rule) == Level::Error)
            .map(|(_, &n)| n)
            .sum();
        let total_warns: usize = self
            .counts
            .iter()
            .filter(|(rule, _)| rule_level(rule) == Level::Warn)
            .map(|(_, &n)| n)
            .sum();
        let _ = writeln!(
            s,
            "└─ {} error(s) (total hits), {} warn(s) (total hits) ─────────────────",
            total_errors, total_warns
        );
        s
    }

    pub fn print(&self) {
        // Use eprintln rather than velog so the report is visible under any logging configuration
        mcc_dbg!("inst::mod", "{}", self.render());
    }
}

fn rule_level(rule: &str) -> Level {
    match rule {
        "R01-e" | "R03a" | "R12" => Level::Info,
        "R06" | "R09" | "R14" | "R15" => Level::Warn,
        _ => Level::Error,
    }
}

fn rule_name(rule: &str) -> &'static str {
    match rule {
        "R01" => "R01 LITERAL_POINT",
        "R01-e" => "R01-e WAIVED",
        "R02" => "R02 SHORT_PASSIVE",
        "R03" => "R03 SHORT_RAIL",
        "R03a" => "R03a RAIL_ALIAS",
        "R04" => "R04 SHORT_LANE",
        "R05" => "R05 UNRESOLVED_UNIT",
        "R06" => "R06 MEGANET",
        "R07" => "R07 GHOST_INSTANCE",
        "R08" => "R08 PHANTOM_PATH",
        "R09" => "R09 FLOATING_PWR_PIN",
        "R10" => "R10 SYMBOL_CONSERV",
        "R11" => "R11 SPLIT_RAIL",
        "R12" => "R12 DANGLING_PORT",
        "R14" => "R14 ORPHAN_INSTANCE",
        "R15" => "R15 SYNTHETIC_PIN",
        _ => "?",
    }
}

// ============================================================================
// Entry point
// ============================================================================

/// Run all rules (excluding R10, which needs the pass1 expectation)
pub fn run(table: &InstTable) -> Report {
    run_with_expectation(table, &BTreeMap::new())
}

/// Run all rules.
///
/// `pass1_expect`: `module full path -> number of Component entries for that module in the pass1 symbol table`.
/// Pass an empty table to skip R10.
pub fn run_with_expectation(table: &InstTable, pass1_expect: &BTreeMap<String, usize>) -> Report {
    let mut rep = Report::default();

    // Register every rule once so that rules with 0 hits also appear in the table
    for r in [
        "R01", "R02", "R03", "R03a", "R04", "R05", "R06", "R07", "R08", "R09", "R10", "R11", "R12",
        "R14", "R15",
    ] {
        rep.counts.insert(r, 0);
    }

    let idx = Index::build(table);

    rep.total_nets = table.net_count();
    rep.total_components = table.get_components().len();
    rep.total_modules = table.get_modules().len();

    check_r01_literal_point(table, &idx, &mut rep);
    check_r02_short_passive(table, &idx, &mut rep);
    check_r03_r04_r06(table, &idx, &mut rep);
    check_r05_unresolved_unit(&mut rep);
    check_r07_ghost(table, &idx, &mut rep);
    check_r08_phantom_path(table, &idx, &mut rep);
    check_r09_floating_power(table, &idx, &mut rep);
    check_r10_conservation(table, &idx, pass1_expect, &mut rep);
    check_r11_split_rail(table, &idx, &mut rep);
    check_r12_dangling_port(table, &idx, &mut rep);
    check_r14_orphan_instance(table, &idx, &mut rep);
    check_r15_synthetic_pin(&mut rep);

    rep
}

// ============================================================================
// Index: precompute mappings that are needed repeatedly, such as "point -> owning module"
// ============================================================================

struct Index {
    /// entry id -> nearest Module ancestor id
    nearest_module: BTreeMap<u32, u32>,
    /// module id -> path
    module_path: BTreeMap<u32, String>,
    /// net id -> owning module path (best effort)
    net_module: BTreeMap<u32, String>,
    /// entry id -> the Component id that owns it (itself when the entry is a Component)
    owner_comp: BTreeMap<u32, u32>,
}

impl Index {
    fn build(table: &InstTable) -> Self {
        let mut nearest_module = BTreeMap::new();
        let mut module_path = BTreeMap::new();
        let mut owner_comp = BTreeMap::new();

        for (id, e) in table.iter() {
            if e.kind == InstKind::Module {
                module_path.insert(*id, e.path.clone());
            }
        }

        for (id, _) in table.iter() {
            // Walk up to find the nearest Module
            let mut cur = table.get_entry(*id).and_then(|e| e.parent_id);
            let mut guard = 0usize;
            while let Some(p) = cur {
                guard += 1;
                if guard > 256 {
                    break; // guard against cycles
                }
                match table.get_entry(p) {
                    Some(pe) => {
                        if pe.kind == InstKind::Module {
                            nearest_module.insert(*id, p);
                            break;
                        }
                        cur = pe.parent_id;
                    }
                    None => break,
                }
            }

            // Walk up to find the nearest Component
            let mut cur = Some(*id);
            let mut guard = 0usize;
            while let Some(c) = cur {
                guard += 1;
                if guard > 256 {
                    break;
                }
                match table.get_entry(c) {
                    Some(ce) => {
                        if ce.kind == InstKind::Component {
                            owner_comp.insert(*id, c);
                            break;
                        }
                        cur = ce.parent_id;
                    }
                    None => break,
                }
            }
        }

        // Net's owning module = the longest common ancestor among the nearest modules of all its points
        let mut net_module = BTreeMap::new();
        for net in table.get_nets() {
            let mut cands: Vec<&str> = Vec::new();
            for p in &net.points {
                if let Some(m) = nearest_module.get(p) {
                    if let Some(path) = module_path.get(m) {
                        cands.push(path.as_str());
                    }
                }
            }
            let m = common_module_prefix(&cands);
            net_module.insert(net.id, m);
        }

        Index {
            nearest_module,
            module_path,
            net_module,
            owner_comp,
        }
    }

    fn module_of_net(&self, net_id: u32) -> &str {
        self.net_module
            .get(&net_id)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    fn module_of_entry(&self, id: u32) -> &str {
        self.nearest_module
            .get(&id)
            .and_then(|m| self.module_path.get(m))
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

/// Longest common prefix of a set of module paths (split on `.`)
fn common_module_prefix(paths: &[&str]) -> String {
    if paths.is_empty() {
        return String::new();
    }
    let first: Vec<&str> = paths[0].split('.').collect();
    let mut n = first.len();
    for p in &paths[1..] {
        let segs: Vec<&str> = p.split('.').collect();
        let mut k = 0;
        while k < n && k < segs.len() && first[k] == segs[k] {
            k += 1;
        }
        n = k;
    }
    first[..n].join(".")
}

// ============================================================================
// String helpers (self-contained; does not depend on the viz layer to avoid cross-layer coupling)
// ============================================================================

/// Take the last segment of a path: `"main.mic.MIC/P"` -> `"P"`
fn leaf(path: &str) -> &str {
    let a = path.rsplit('.').next().unwrap_or(path);
    a.rsplit('/').next().unwrap_or(a)
}

/// Drop the last segment: `"main.ldo.ldo.1"` -> `Some("main.ldo.ldo")`
fn owner_path(path: &str) -> Option<&str> {
    // Split on '/' first, then '.', and take whichever separator comes later
    let dot = path.rfind('.');
    let slash = path.rfind('/');
    let cut = match (dot, slash) {
        (Some(d), Some(s)) => Some(d.max(s)),
        (Some(d), None) => Some(d),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    }?;
    if cut == 0 {
        None
    } else {
        Some(&path[..cut])
    }
}

/// Whether the name looks like ground
fn is_ground_name(s: &str) -> bool {
    let u = leaf(s).to_uppercase();
    matches!(
        u.as_str(),
        "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GROUND" | "EARTH"
    )
}

/// Whether the name looks like a supply (ground excluded)
fn is_supply_name(s: &str) -> bool {
    let u = leaf(s).to_uppercase();
    if is_ground_name(&u) {
        return false;
    }
    const EXACT: &[&str] = &[
        "VCC",
        "VDD",
        "VBUS",
        "VPP",
        "AVDD",
        "DVDD",
        "POWER_SYS",
        "VBAT",
        "VIN",
        "VOUT",
    ];
    if EXACT.contains(&u.as_str()) {
        return true;
    }
    if ["VCC", "VDD", "AVDD", "DVDD", "VBUS", "VBAT"]
        .iter()
        .any(|p| u.starts_with(p))
    {
        return true;
    }
    // Names like 3V3 / 5V0 / 1V2 / V3V3 / V5V
    let bytes = u.as_bytes();
    let digits = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    if u.contains('V') && digits >= 1 && u.len() <= 8 {
        // Exclude plain pin names (VO1 / VO2 style amplifier outputs)
        if !u.starts_with("VO") {
            return true;
        }
    }
    false
}

/// Normalized identity of a power net, used by R11 (a same-named power rail should not have two nets)
fn rail_identity(s: &str) -> Option<String> {
    let l = leaf(s);
    if is_ground_name(l) {
        return Some("GND".to_string());
    }
    if is_supply_name(l) {
        return Some(l.to_uppercase());
    }
    None
}

// ============================================================================
// R01 · unexpanded vector reference
// ============================================================================

/// ★ R01-e: check whether a literal path is a pure boundary port declaration.
///
/// A literal path like `dc{VDD_3V3, GND}` represents a port declaration (dc)
/// with its members. If the base name (dc) is a Port or Label whose parent is a
/// Module, the literal point is exempt from R01.
///
/// For anonymous port groups like `[VCC_1V2, GND]` (no base name before `[`),
/// the members themselves are the boundary ports.
fn is_boundary_port_decl(path: &str, boundary_leaves: &HashSet<String>) -> bool {
    // Extract the base name: everything before the first {, [, or ,
    let brace = path.find('{');
    let bracket = path.find('[');
    let comma = path.find(',');
    let first = [brace, bracket, comma].iter().filter_map(|&x| x).min();
    match first {
        Some(0) => {
            // No base name (starts with bracket/brace). Extract members and check them.
            // e.g., [VCC_1V2, GND] -> members are VCC_1V2, GND
            let inner = path
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim_start_matches('{')
                .trim_end_matches('}');
            inner.split(',').any(|m| {
                let member = m.trim();
                !member.is_empty() && boundary_leaves.contains(member)
            })
        }
        Some(pos) => {
            // Has a base name, e.g., dc{VDD_3V3, GND} -> base is "dc"
            let base = &path[..pos];
            !base.is_empty() && boundary_leaves.contains(base)
        }
        None => {
            // No brackets/braces/commas at all, check the whole path
            !path.is_empty() && boundary_leaves.contains(path)
        }
    }
}

fn check_r01_literal_point(table: &InstTable, idx: &Index, rep: &mut Report) {
    // ★ Patch 2-1: isolated literal points are no longer in the InstTable,
    // so read the full list directly from LITERAL_POINT_DETAILS.
    let details = crate::instant::mc_net::LITERAL_POINT_DETAILS
        .lock()
        .unwrap();
    if !details.is_empty() {
        // ★ R01-e: build a set of boundary port/label leaf names.
        // A literal point is a "pure boundary port declaration" when its base name
        // (or its members, for anonymous port groups) is a Port or Label whose
        // parent is a Module. These are port-declaration views, not electrical
        // connection points, and are exempt from R01.
        let mut boundary_leaves: HashSet<String> = HashSet::new();
        for module in table.get_modules() {
            for child in table.children_of(module.id) {
                if matches!(child.kind, InstKind::Port | InstKind::Label) {
                    if let Some(leaf) = child.path.rsplit('.').next() {
                        boundary_leaves.insert(leaf.to_string());
                    }
                }
            }
        }

        // ★ Deduplicate: bucket by path, keeping the occurrence count.
        // R01-e exempted paths are counted separately.
        let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
        let mut waived = 0usize;
        let mut waived_paths: Vec<String> = Vec::new();
        for (path, _) in details.iter() {
            if is_boundary_port_decl(path, &boundary_leaves) {
                waived += 1;
                if !waived_paths.contains(&path.to_string()) {
                    waived_paths.push(path.to_string());
                }
                continue;
            }
            *buckets.entry(path.as_str()).or_insert(0) += 1;
        }
        let unique = buckets.len();
        let total: usize = buckets.values().sum();
        set_scanned(rep, "R01", total + waived);

        // Report R01-e waived count (Info level, separate line)
        if waived > 0 {
            waived_paths.sort();
            let waived_items: Vec<String> = waived_paths
                .iter()
                .map(|p| format!("`{p}`"))
                .collect();
            note(
                rep,
                "R01-e",
                String::new(),
                format!(
                    "R01-e waived: {} (pure boundary port declaration: {})",
                    waived,
                    waived_items.join(", ")
                ),
            );
        }

        if !buckets.is_empty() {
            // Sort by descending occurrence count
            let mut sorted: Vec<(&str, usize)> = buckets.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

            let items: Vec<String> = sorted
                .iter()
                .map(|(path, count)| {
                    if *count > 1 {
                        format!("`{path}` ×{count}")
                    } else {
                        format!("`{path}`")
                    }
                })
                .collect();
            *rep.counts.entry("R01").or_insert(0) = unique;
            rep.findings.push(Finding {
                rule: "R01",
                level: rule_level("R01"),
                module: String::new(),
                detail: format!(
                    "{} unexpanded vector reference(s) ({} unique, {} occurrences): {}",
                    total,
                    unique,
                    total,
                    items.join("  ")
                ),
            });
        }
        return; // no need to scan the InstTable after isolation
    }

    // Fallback: if isolation did not take effect (e.g. optimized away in release builds), use the old path
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for net in table.get_nets() {
        for p in &net.points {
            seen.insert(*p);
        }
    }

    let mut scanned = 0usize;
    for id in seen {
        let Some(e) = table.get_entry(id) else {
            continue;
        };
        scanned += 1;
        if e.path.contains('{') || e.path.contains('[') || e.path.contains(',') {
            push(
                rep,
                "R01",
                idx.module_of_entry(id).to_string(),
                format!(
                    "unexpanded vector reference entered the netlist: `{}` (id={}, kind={})",
                    e.path, e.id, e.kind
                ),
            );
        }
    }

    // Net names must not contain brackets either
    for net in table.get_nets() {
        if net.name.contains('{') || net.name.contains('[') || net.name.contains(',') {
            push(
                rep,
                "R01",
                idx.module_of_net(net.id).to_string(),
                format!(
                    "net name contains literal brackets: `{}` (net#{})",
                    net.name, net.id
                ),
            );
        }
    }
    set_scanned(rep, "R01", scanned);
}

// ============================================================================
// R02 · two-terminal device with both pins on the same net
// ============================================================================

fn check_r02_short_passive(table: &InstTable, idx: &Index, rep: &mut Report) {
    let mut scanned = 0usize;
    for comp in table.get_components() {
        let pins = table.get_pins_of(comp.id);
        if pins.len() != 2 {
            continue;
        }
        scanned += 1;
        let n0 = table.get_net_of(pins[0].id).map(|n| n.id);
        let n1 = table.get_net_of(pins[1].id).map(|n| n.id);
        if let (Some(a), Some(b)) = (n0, n1) {
            if a == b {
                let net_name = table.get_net(a).map(|n| n.name.clone()).unwrap_or_default();
                push(
                    rep,
                    "R02",
                    idx.module_of_entry(comp.id).to_string(),
                    format!(
                        "two-terminal device `{}` ({}) has both pins on net `{}` (net#{}) —— short circuit",
                        comp.path, comp.class_name, net_name, a
                    ),
                );
            }
        }
    }
    set_scanned(rep, "R02", scanned);
}

// ============================================================================
// R03 / R04 / R06 · semantic conflicts inside a net
// ============================================================================

fn check_r03_r04_r06(table: &InstTable, idx: &Index, rep: &mut Report) {
    set_scanned(rep, "R03", table.net_count());
    set_scanned(rep, "R03a", table.net_count());
    set_scanned(rep, "R04", table.net_count());
    set_scanned(rep, "R06", table.net_count());

    for net in table.get_nets() {
        let module = idx.module_of_net(net.id).to_string();

        // ── Collect the information on this net ──
        let mut supplies: BTreeSet<String> = BTreeSet::new();
        let mut grounds: BTreeSet<String> = BTreeSet::new();
        // bus prefix -> set of member names seen on this net
        let mut bus_members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut owners: BTreeSet<u32> = BTreeSet::new();
        let mut has_rail = false;

        for p in &net.points {
            let Some(e) = table.get_entry(*p) else {
                continue;
            };
            let l = leaf(&e.path);

            if is_ground_name(l) {
                grounds.insert(l.to_uppercase());
                has_rail = true;
            } else if is_supply_name(l) {
                supplies.insert(l.to_uppercase());
                has_rail = true;
            }

            // Bus member: for `X.MIC.P`, the prefix is `X.MIC` and the member is `P`.
            if let Some(op) = owner_path(&e.path) {
                // Only count when the path itself is a bus member; a Component
                // pin must not be treated as a same-net bus member.
                let owner_is_bus = table
                    .get_id_by_path(op)
                    .and_then(|oid| table.get_entry(oid))
                    .map(|oe| matches!(oe.kind, InstKind::Bus | InstKind::Port))
                    .unwrap_or(false);
                if owner_is_bus {
                    bus_members
                        .entry(op.to_string())
                        .or_default()
                        .insert(l.to_string());
                }
            }

            if let Some(c) = idx.owner_comp.get(p) {
                owners.insert(*c);
            }
        }

        // ── R03: supply-ground short circuit (ERROR) ──
        if !supplies.is_empty() && !grounds.is_empty() {
            push(
                rep,
                "R03",
                module.clone(),
                format!(
                    "net `{}` (net#{}) contains both supply and ground: {:?} + {:?} —— short circuit",
                    net.name, net.id, supplies, grounds
                ),
            );
        }

        // ── R03a: power domain aliases coexist (INFO) ──
        let distinct_supplies = supplies.len();
        if distinct_supplies >= 2 {
            push(
                rep,
                "R03a",
                module.clone(),
                format!(
                    "net `{}` (net#{}) contains multiple power-domain aliases: {:?} — a short circuit if these names represent different voltages",
                    net.name, net.id, supplies
                ),
            );
        }

        // ── R04: multiple members of the same bus on the same net ──
        for (bus, members) in &bus_members {
            if members.len() >= 2 {
                push(
                    rep,
                    "R04",
                    module.clone(),
                    format!(
                        "bus `{}`: {} members land on the same net `{}` (net#{}): {:?}",
                        bus,
                        members.len(),
                        net.name,
                        net.id,
                        members
                    ),
                );
            }
        }

        // ── R06: meganet ──
        if !has_rail && net.points.len() > MEGANET_POINTS && owners.len() > MEGANET_OWNERS {
            push(
                rep,
                "R06",
                module.clone(),
                format!(
                    "net `{}` (net#{}) has {} points spanning {} devices; a non-power net should not be this large",
                    net.name,
                    net.id,
                    net.points.len(),
                    owners.len()
                ),
            );
        }
    }
}

// ============================================================================
// R07 · ghost instance — the endpoint owner must resolve to a legitimate registered entry in the InstTable
// ============================================================================
//
// Whitelist: owner ∈ {Component, Module, Bus, Port} is legitimate
// Resolving to no entry, or to a bare class-name fragment, is reported
//
// Test specimen: speaker's `DIO` (a class-name fragment, not found in the InstTable)

fn check_r07_ghost(table: &InstTable, idx: &Index, rep: &mut Report) {
    // ★ P0.5-3: precompute the set of direct Component child paths for each module.
    // The old criterion "owner exists in entries" was self-referential — ghosts
    // are born inside entries themselves.
    // New criterion: for an endpoint whose owner is a Component, the owner must
    // appear in that module's children (kind==Component), not merely "the string
    // exists in entries".
    // Non-Component owners (Module/Bus/Port/Label) are skipped and handled by R08 etc.
    let mut module_components: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for m in table.get_modules() {
        let comps: BTreeSet<String> = table
            .children_of(m.id)
            .iter()
            .filter(|e| e.kind == InstKind::Component)
            .map(|e| e.path.clone())
            .collect();
        module_components.insert(m.id, comps);
    }

    // (module_id, owner_path) → set of component names referenced by that owner
    let mut ghosts: BTreeMap<(u32, String), BTreeSet<String>> = BTreeMap::new();
    let mut scanned = 0usize;

    for net in table.get_nets() {
        for p in &net.points {
            let Some(e) = table.get_entry(*p) else {
                continue;
            };

            // Step 1 · determine the owner: the part of the path before the last dot, or the path itself when there is no dot
            let owner = owner_path(&e.path)
                .map(|op| op.to_string())
                .unwrap_or_else(|| leaf(&e.path).to_string());

            // Step 2 · find the nearest module of the endpoint
            let module_id = match idx.nearest_module.get(p) {
                Some(m) => *m,
                None => continue,
            };

            // Step 3 · look up the owner's registered kind
            // Only Component-kind owners need to be checked against the module's children
            // Module/Bus/Port/Label owners are legitimate non-Component references, skip them
            let owner_kind = table
                .get_id_by_path(&owner)
                .and_then(|oid| table.get_entry(oid))
                .map(|oe| oe.kind.clone());

            match owner_kind {
                Some(InstKind::Component) => {
                    scanned += 1;
                    // Component-kind owner: must appear in the module's children
                    let is_valid = module_components
                        .get(&module_id)
                        .map(|comps| comps.contains(&owner))
                        .unwrap_or(false);
                    if !is_valid {
                        let comp_name = leaf(&owner).to_string();
                        ghosts
                            .entry((module_id, owner))
                            .or_default()
                            .insert(comp_name);
                    }
                }
                Some(InstKind::Module | InstKind::Bus | InstKind::Port) => {
                    // legitimate reference, not a ghost
                }
                Some(_) | None => {
                    // Label/Pin kind, or no entry resolved → class-name fragment (e.g. DIO)
                    scanned += 1;
                    let comp_name = leaf(&owner).to_string();
                    ghosts
                        .entry((module_id, owner))
                        .or_default()
                        .insert(comp_name);
                }
            }
        }
    }

    set_scanned(rep, "R07", scanned);

    // Aggregate the report by module
    let mut module_ghosts: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for ((module_id, _owner), comps) in &ghosts {
        module_ghosts
            .entry(*module_id)
            .or_default()
            .extend(comps.iter().cloned());
    }

    for (module_id, comps) in &module_ghosts {
        let module_name = idx
            .module_path
            .get(module_id)
            .map(|s| s.as_str())
            .unwrap_or("?");
        let mut names: Vec<&str> = comps.iter().map(|s| s.as_str()).collect();
        names.sort();
        push(
            rep,
            "R07",
            module_name.to_string(),
            format!(
                "{} references {} unregistered device(s) — {}",
                leaf(module_name),
                comps.len(),
                names.join(" ")
            ),
        );
    }
}

// ============================================================================
// R08 · phantom path —— an intermediate segment must be a registered instance, not just a string
// ============================================================================

fn check_r08_phantom_path(table: &InstTable, idx: &Index, rep: &mut Report) {
    /// Whether the leaf is a purely numeric pin number
    fn is_numeric_pin_leaf(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    }

    // ★ P0.5-3: precompute the set of direct instance child paths (Component + Module) for each module.
    // Same reasoning as R07: the old criterion "the middle segment exists in entries" was self-referential.
    // New criterion: the middle segment must be an entry with kind∈{Component,Module} in the module's children.
    let mut module_children: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for m in table.get_modules() {
        let children: BTreeSet<String> = table
            .children_of(m.id)
            .iter()
            .filter(|e| matches!(e.kind, InstKind::Component | InstKind::Module))
            .map(|e| e.path.clone())
            .collect();
        module_children.insert(m.id, children);
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut scanned = 0usize;

    for net in table.get_nets() {
        for p in &net.points {
            let Some(e) = table.get_entry(*p) else {
                continue;
            };
            let leaf_name = leaf(&e.path);

            // Step 1 · filter endpoints: only handle endpoints whose leaf is a purely numeric pin number
            if !is_numeric_pin_leaf(leaf_name) {
                continue;
            }
            scanned += 1;

            // Step 2 · look up the middle segment
            let owner = match owner_path(&e.path) {
                Some(op) => op,
                None => continue,
            };

            // Step 3 · find the nearest module of the endpoint
            let module_id = match idx.nearest_module.get(p) {
                Some(m) => *m,
                None => continue,
            };

            // The middle segment must be a direct Component/Module child of the module
            let owner_is_proper = module_children
                .get(&module_id)
                .map(|children| children.contains(owner))
                .unwrap_or(false);

            if owner_is_proper {
                continue;
            }

            // Middle segment not registered → check whether the upper level (grandparent) is in the module's children
            if let Some(grandparent) = owner_path(owner) {
                let gp_is_proper = module_children
                    .get(&module_id)
                    .map(|children| children.contains(grandparent))
                    .unwrap_or(false);
                if gp_is_proper {
                    let key = format!("R08:{owner}");
                    if seen.insert(key) {
                        push(
                            rep,
                            "R08",
                            idx.module_of_entry(*p).to_string(),
                            format!(
                                "phantom path: `{}` has an unregistered middle segment `{}` (upper level `{}` exists)",
                                e.path, owner, grandparent
                            ),
                        );
                    }
                }
            }
        }
    }

    set_scanned(rep, "R08", scanned);
}

// ============================================================================
// R09 · floating power / ground pins
// ============================================================================

fn check_r09_floating_power(table: &InstTable, idx: &Index, rep: &mut Report) {
    let mut scanned = 0usize;
    for comp in table.get_components() {
        for pin in table.get_pins_of(comp.id) {
            let name = leaf(&pin.path);
            // Pin-number forms ("1"/"2") carry no semantics, so fall back to the functional name in class_name
            let fname = pin.class_name.trim();
            let is_pwr = is_ground_name(name)
                || is_supply_name(name)
                || is_ground_name(fname)
                || is_supply_name(fname);
            if !is_pwr {
                continue;
            }
            scanned += 1;
            if table.get_net_of(pin.id).is_none() {
                push(
                    rep,
                    "R09",
                    idx.module_of_entry(comp.id).to_string(),
                    format!(
                        "device `{}` has an unconnected power/ground pin `{}`",
                        comp.path,
                        leaf(&pin.path)
                    ),
                );
            }
        }
    }
    set_scanned(rep, "R09", scanned);
}

// ============================================================================
// R10 · symbol conservation (what pass1 has, pass2 must also have)
// ============================================================================

fn check_r10_conservation(
    table: &InstTable,
    _idx: &Index,
    expect: &BTreeMap<String, usize>,
    rep: &mut Report,
) {
    // ★ Foolproofing: emit SKIP before returning early
    if expect.is_empty() {
        note(
            rep,
            "R10",
            "-".to_string(),
            "R10 is not wired to the pass1 symbol table, so this rule is void this round"
                .to_string(),
        );
        set_scanned(rep, "R10", 0);
        return;
    }

    // ★ Foolproofing: if some module's pass1 expectation set size < 2, emit WARN
    for (path, want) in expect {
        if *want < 2 {
            push(
                rep,
                "R10",
                path.clone(),
                format!("R10 expectation set seems collapsed: {path} has only {want} Component(s), so this rule is void this round",),
            );
        }
    }

    // Count the direct Component children of each module
    let mut actual: BTreeMap<String, usize> = BTreeMap::new();
    for m in table.get_modules() {
        let n = table
            .children_of(m.id)
            .iter()
            .filter(|e| e.kind == InstKind::Component)
            .count();
        actual.insert(m.path.clone(), n);
    }

    set_scanned(rep, "R10", expect.len());

    for (path, want) in expect {
        let got = actual.get(path).copied().unwrap_or(0);
        if got < *want {
            push(
                rep,
                "R10",
                path.clone(),
                format!(
                    "pass1 symbol table has {want} device(s), but pass2 only registered {got} —— missing {}",
                    want - got
                ),
            );
        }
    }
}

// ============================================================================
// R11 · same-name power net split into multiple nets (bucketed by rail_identity)
// ============================================================================

fn check_r11_split_rail(table: &InstTable, idx: &Index, rep: &mut Report) {
    // rail_identity → the nets where this identity appears
    let mut buckets: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    let mut scanned = 0usize;

    for net in table.get_nets() {
        let mut ids: BTreeSet<String> = BTreeSet::new();
        for p in &net.points {
            if let Some(e) = table.get_entry(*p) {
                if let Some(rid) = rail_identity(&e.path) {
                    ids.insert(rid);
                }
            }
        }
        if !ids.is_empty() {
            scanned += 1;
        }
        for rid in ids {
            buckets.entry(rid).or_default().insert(net.id);
        }
    }

    // ★ P0.5-4: cross-level port union —— first merge same-rail nets of parent and child modules through port connection relations
    //
    // Problem: `main.dcdc::GND` and `main::GND` are connected through a port,
    // but R11 cannot see this connection when bucketing by net, so it would
    // wrongly report SPLIT_RAIL.
    //
    // Solution: for each module, collect the rail_identity exported by its ports.
    // For each parent-child module pair, if the child module's port exports a
    // rail_identity, and both parent and child have nets with that rail_identity,
    // union those nets.
    let mut uf: BTreeMap<u32, u32> = BTreeMap::new();
    fn uf_find(uf: &mut BTreeMap<u32, u32>, x: u32) -> u32 {
        let p = *uf.entry(x).or_insert(x);
        if p == x {
            x
        } else {
            let root = uf_find(uf, p);
            uf.insert(x, root);
            root
        }
    }
    fn uf_union(uf: &mut BTreeMap<u32, u32>, a: u32, b: u32) {
        let ra = uf_find(uf, a);
        let rb = uf_find(uf, b);
        if ra != rb {
            uf.insert(ra, rb);
        }
    }

    // Collect the rail_identity each module exports through its ports
    // ★ Not only Port, but also Bus sub-members and direct Labels — a module's
    // power parameters may be registered as a Label (e.g. main.GND), a Bus
    // sub-Label (e.g. dc.GND), or a Port.
    //
    // ★ P0.5-5: tighten the union condition —— also record the port entry id for
    // each rail_identity, so we can later verify that the parent layer actually
    // connected the port through a port binding.
    // module_id → (rail_identity → Vec<port_entry_id>)
    let mut module_port_rails: BTreeMap<u32, BTreeMap<String, Vec<u32>>> = BTreeMap::new();
    for m in table.get_modules() {
        // 1) explicit ports (Port)
        for port in table.get_ports_of(m.id) {
            if let Some(rid) = rail_identity(&port.path) {
                module_port_rails
                    .entry(m.id)
                    .or_default()
                    .entry(rid)
                    .or_default()
                    .push(port.id);
            }
        }
        // 2) Bus children of the module → the rail_identity of their sub-Labels
        //    e.g. speaker's dc{VDD_3V3, GND} → Bus "dc" → Label "GND" / "VDD_3V3"
        for child in table.children_of(m.id) {
            match child.kind {
                InstKind::Bus => {
                    for grandchild in table.children_of(child.id) {
                        if let Some(rid) = rail_identity(&grandchild.path) {
                            module_port_rails
                                .entry(m.id)
                                .or_default()
                                .entry(rid)
                                .or_default()
                                .push(grandchild.id);
                        }
                    }
                }
                InstKind::Label => {
                    // 3) direct Label children (e.g. main.GND, main.mcu.GND)
                    if let Some(rid) = rail_identity(&child.path) {
                        module_port_rails
                            .entry(m.id)
                            .or_default()
                            .entry(rid)
                            .or_default()
                            .push(child.id);
                    }
                }
                _ => {}
            }
        }
    }

    // Collect the set of modules each net lives in (via the nearest_module of the net's points)
    let mut net_modules: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for net in table.get_nets() {
        let mut mods = BTreeSet::new();
        for p in &net.points {
            if let Some(m) = idx.nearest_module.get(p) {
                mods.insert(*m);
            }
        }
        if !mods.is_empty() {
            net_modules.insert(net.id, mods);
        }
    }

    // For each parent-child module pair, union same-rail nets connected through ports
    //
    // ★ P0.5-5: tighten the union condition —— only union when the parent layer
    // actually connected the child module's port through a port binding. A mere
    // "some parent-side connection mentions this port name" is not a union basis.
    // Test: whether the net that contains the port's entry also contains a point
    // of the parent module (i.e. the port is connected on the parent side).
    for m in table.get_modules() {
        let parent_entry_id = match table.get_entry(m.id).and_then(|e| e.parent_id) {
            Some(pid) => pid,
            None => continue,
        };

        // ★ parent_entry_id is the id of the parent entry (may be a Component/Module/Label…),
        // not the parent Module's id. If the parent entry is itself a Module, use its
        // id directly; otherwise walk up through nearest_module to find the nearest Module.
        let parent_module_id = {
            let parent_entry = table.get_entry(parent_entry_id);
            match parent_entry {
                Some(pe) if pe.kind == InstKind::Module => parent_entry_id,
                Some(_other) => match idx.nearest_module.get(&parent_entry_id) {
                    Some(pm) => *pm,
                    None => continue,
                },
                None => continue,
            }
        };

        if let Some(port_rails) = module_port_rails.get(&m.id) {
            for (rid, port_eids) in port_rails {
                // ★ Check: did the parent layer actually connect this port through a port binding?
                // At least one net containing a port entry also contains a point of the parent module → connected
                let port_connected = port_eids.iter().any(|&eid| {
                    if let Some(net) = table.get_net_of(eid) {
                        net.points
                            .iter()
                            .any(|p| idx.nearest_module.get(p) == Some(&parent_module_id))
                    } else {
                        false
                    }
                });
                if !port_connected {
                    // The parent layer did not bind this port, so no union
                    continue;
                }

                // Collect the parent module's nets with this rail_identity
                let mut parent_nets: Vec<u32> = Vec::new();
                // Collect the child module's nets with this rail_identity
                let mut child_nets: Vec<u32> = Vec::new();

                for (nid, mods) in &net_modules {
                    if let Some(net_set) = buckets.get(rid) {
                        if !net_set.contains(nid) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if mods.contains(&parent_module_id) {
                        parent_nets.push(*nid);
                    }
                    if mods.contains(&m.id) {
                        child_nets.push(*nid);
                    }
                }

                // Union the same-rail nets of the parent and child modules
                for &pn in &parent_nets {
                    for &cn in &child_nets {
                        uf_union(&mut uf, pn, cn);
                    }
                }
            }
        }
    }

    set_scanned(rep, "R11", scanned);

    // ★ P0.5-6: scope by module —— only report rails split into multiple nets within the same module.
    // Re-bucket by module first, then check the number of union groups inside each module.
    // Use idx.net_module as each net's "primary module" (deepest common ancestor),
    // to avoid a net shared across modules being counted in multiple modules.
    let mut module_buckets: BTreeMap<String, BTreeMap<String, BTreeSet<u32>>> = BTreeMap::new();
    for (rid, nets) in &buckets {
        for nid in nets {
            let primary = idx.module_of_net(*nid);
            if primary.is_empty() {
                continue;
            }
            module_buckets
                .entry(primary.to_string())
                .or_default()
                .entry(rid.clone())
                .or_default()
                .insert(*nid);
        }
    }

    for (mod_name, rid_buckets) in &module_buckets {
        for (rid, nets) in rid_buckets {
            let mut groups: BTreeSet<u32> = BTreeSet::new();
            for nid in nets {
                groups.insert(uf_find(&mut uf, *nid));
            }
            if groups.len() >= 2 {
                let mut group_repr: BTreeMap<u32, u32> = BTreeMap::new();
                for nid in nets {
                    let root = uf_find(&mut uf, *nid);
                    group_repr.entry(root).or_insert(*nid);
                }
                let names: Vec<String> = group_repr
                    .values()
                    .filter_map(|n| {
                        table
                            .get_net(*n)
                            .map(|e| format!("{}::{}#{}", mod_name, e.name, e.id))
                    })
                    .collect();
                push(
                    rep,
                    "R11",
                    mod_name.clone(),
                    format!(
                        "power rail `{}` inside the module is split into {} mutually unconnected nets: {}",
                        rid,
                        groups.len(),
                        names.join(", ")
                    ),
                );
            }
        }
    }
}

// ============================================================================
// R12 · port net with only itself as a point
// ============================================================================

fn check_r12_dangling_port(table: &InstTable, idx: &Index, rep: &mut Report) {
    set_scanned(rep, "R12", table.net_count());
    for net in table.get_nets() {
        if net.points.len() != 1 {
            continue;
        }
        let Some(e) = table.get_entry(net.points[0]) else {
            continue;
        };
        if !matches!(e.kind, InstKind::Port | InstKind::Bus) {
            continue;
        }
        push(
            rep,
            "R12",
            idx.module_of_net(net.id).to_string(),
            format!(
                "the net of port `{}` has only itself as a point (declared but not connected)",
                e.path
            ),
        );
    }
}

// ============================================================================
// R14 · orphan instance —— registered Component that is not in any net
// ============================================================================

fn check_r14_orphan_instance(table: &InstTable, idx: &Index, rep: &mut Report) {
    // Collect every Component owner that appears in a net (via the owner_comp of the net's points)
    let mut wired_owners: BTreeSet<u32> = BTreeSet::new();
    for net in table.get_nets() {
        for p in &net.points {
            if let Some(c) = idx.owner_comp.get(p) {
                wired_owners.insert(*c);
            }
        }
    }

    let mut scanned = 0usize;
    let mut orphans: BTreeMap<String, Vec<String>> = BTreeMap::new(); // module -> [comp_names]

    for comp in table.get_components() {
        scanned += 1;
        if wired_owners.contains(&comp.id) {
            continue;
        }
        let module = idx.module_of_entry(comp.id).to_string();
        let mod_key = if module.is_empty() {
            "<top-level>".to_string()
        } else {
            module
        };
        orphans
            .entry(mod_key)
            .or_default()
            .push(leaf(&comp.path).to_string());
    }

    set_scanned(rep, "R14", scanned);

    for (module, names) in &orphans {
        let mut sorted = names.clone();
        sorted.sort();
        push(
            rep,
            "R14",
            module.clone(),
            format!(
                "{} instance(s) registered but not in any net: {}",
                sorted.len(),
                sorted.join(", ")
            ),
        );
    }
}

// ============================================================================
// R15 · synthetic terminal —— a pin_id detected by the viz layer that does not belong to any real pin
// ============================================================================

fn check_r15_synthetic_pin(rep: &mut Report) {
    let count = crate::viz::SYNTHETIC_PIN_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    set_scanned(rep, "R15", 1); // R15 runs once per viz render
    if count > 0 {
        rep.counts.insert("R15", count);
        rep.findings.push(Finding {
            rule: "R15",
            level: Level::Warn,
            module: String::new(),
            detail: format!(
                "{} synthetic terminal(s) (pin_id not belonging to any real pin, possibly from port scalar/member handling or an unresolved endpoint reference)",
                count
            ),
        });
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

fn push(rep: &mut Report, rule: &'static str, module: String, detail: String) {
    *rep.counts.entry(rule).or_insert(0) += 1;
    rep.findings.push(Finding {
        rule,
        level: rule_level(rule),
        module,
        detail,
    });
}

/// Add a note that does not increment any counter (used for SKIP-style status notes); always INFO level
fn note(rep: &mut Report, rule: &'static str, module: String, detail: String) {
    rep.findings.push(Finding {
        rule,
        level: Level::Info,
        module,
        detail,
    });
}

fn set_scanned(rep: &mut Report, rule: &'static str, n: usize) {
    rep.scanned.entry(rule).or_insert(n);
}

// ============================================================================
// Unit tests
// ============================================================================

// R05 · UNRESOLVED_UNIT — a unit-typed argument cannot claim any formal parameter slot
// Counter is incremented during parameter binding in mc_param::bind_with_opts.
fn check_r05_unresolved_unit(rep: &mut Report) {
    let count = crate::semantic::basic::mc_param::R05_UNRESOLVED_UNIT
        .load(std::sync::atomic::Ordering::Relaxed);
    set_scanned(rep, "R05", 1); // R05 is a global counter, always "running"
    if count > 0 {
        rep.counts.insert("R05", count);
        rep.findings.push(Finding {
            rule: "R05",
            level: Level::Error,
            module: String::new(),
            detail: format!(
                "{} unit-typed argument(s) could not claim any formal parameter slot",
                count
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_works() {
        assert_eq!(leaf("main.mic.MIC/P"), "P");
        assert_eq!(leaf("main.modldo.ldo.1"), "1");
        assert_eq!(leaf("GND"), "GND");
        assert_eq!(leaf(""), "");
    }

    #[test]
    fn owner_path_works() {
        assert_eq!(owner_path("main.modldo.ldo.1"), Some("main.modldo.ldo"));
        assert_eq!(owner_path("main.mic.MIC/P"), Some("main.mic.MIC"));
        assert_eq!(owner_path("GND"), None);
    }

    #[test]
    fn rail_names() {
        assert!(is_ground_name("GND"));
        assert!(is_ground_name("main.x.VSS"));
        assert!(!is_ground_name("VDD"));

        assert!(is_supply_name("VDD_3V3"));
        assert!(is_supply_name("V3V3"));
        assert!(is_supply_name("VCC_1V2"));
        assert!(is_supply_name("POWER_SYS"));
        // Amplifier outputs do not count as supplies
        assert!(!is_supply_name("VO1"));
        assert!(!is_supply_name("VO2"));
        // Signal names do not count
        assert!(!is_supply_name("DAC_OUT"));
        assert!(!is_supply_name("SCLK"));
    }

    #[test]
    fn rail_identity_merges_grounds() {
        assert_eq!(rail_identity("main.x.GND").as_deref(), Some("GND"));
        assert_eq!(rail_identity("main.x.VSS").as_deref(), Some("GND"));
        assert_eq!(rail_identity("V3V3").as_deref(), Some("V3V3"));
        assert_eq!(rail_identity("DAC_OUT"), None);
    }

    #[test]
    fn common_prefix() {
        assert_eq!(
            common_module_prefix(&["main.modldo", "main.moddcdc"]),
            "main"
        );
        assert_eq!(common_module_prefix(&["main.mic", "main.mic"]), "main.mic");
        assert_eq!(common_module_prefix(&[]), "");
        assert_eq!(common_module_prefix(&["main"]), "main");
    }
}
