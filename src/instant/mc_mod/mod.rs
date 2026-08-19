// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 instantiation — Module instance
//!
//! McModuleInst is the core data structure of the instantiation phase, representing a complete module instance.
//!
//! ## Module split (after refactoring)
//! - `mod.rs`         —— Type definitions, construction, `instantiate()` top-level flow, diagnostics, Display, ID counter
//! - `phases.rs`      —— Phase 1/3 entry (interfaces, declarations, connection lines)
//! - `line.rs`        —— Single line expansion/dispatch (process_line / process_member_internal)
//! - `points.rs`      —— Endpoint extraction (get_left/right_points, node_to_netpoint)
//! - `bus.rs`         —— Bus handling (ensure_bus / curly-mn parsing)
//! - `group.rs`       —— Group / Transposed handling + create_connection
//! - `funccall.rs`    —— FuncCall dispatch entry + built-in twopin + endpoint resolve
//! - `funccall_inst.rs` —— Component / Module / UserFunc / InstanceMethod instantiation + prefix_instance
//! - `iterated.rs`    —— Iterated call expansion
//! - `subst.rs`       —— Parameter substitution helpers
//! - `debug_dump.rs`  —— Pass1→Pass2 info completeness debug output (MC_INST_DUMP=1 enabled)

mod bus;
mod dump;
mod expand;
mod fcallinst;
mod funccall;
pub(crate) mod group;
mod iterated;
mod line;
mod phases;
mod points;
mod subst;

use super::mc_bus::McBusInst;
use super::mc_comp::McComponentInst;
use super::mc_net::{
    ConnectionInst, InstDiagLevel, InstDiagnostic, InstError, NetPoint, NetTable, PortInst,
};
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::common::IOType;
use crate::semantic::module::McModule;
use crate::{current_uri, McURI};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ============================================================================
// McModuleInst - Module instance
// ============================================================================

/// Module instance
#[derive(Debug, Clone)]
pub struct McModuleInst {
    /// Instance name
    pub name: String,

    /// Base definition
    pub def: Arc<McModule>,

    /// URI of the file containing the definition (used to correctly set current_uri context during recursive instantiation)
    pub def_uri: McURI,

    /// Parameter bindings
    pub params: McParamBindings,

    /// Port instances (all port types)
    pub ports: Vec<PortInst>,

    /// Sub-component instances
    pub components: Vec<McComponentInst>,

    /// Sub-module instances
    pub sub_modules: Vec<McModuleInst>,

    /// Internal connections
    pub connections: Vec<ConnectionInst>,

    /// Net table (label -> set of connection points)
    ///
    /// ★ GND re-partition: a single module may now produce MULTIPLE nets all
    /// named "GND" (local ground groups per writing line / reference form), so
    /// this is a `Vec` (not a `HashMap`) — duplicate names are legal.
    pub nets: Vec<(String, Vec<NetPoint>)>,

    /// Connection ID counter
    pub(super) conn_id_counter: u32,

    /// Internal label registry (for implicit labels)
    pub(super) labels: HashMap<String, NetPoint>,

    /// Bus instance table (bus_name -> McBusInst)
    pub(super) buses: HashMap<String, McBusInst>,

    /// Auto-instantiation counter (component type name → used count), used to generate unique instance names
    pub(super) auto_inst_counter: HashMap<String, u32>,

    /// Stable phrase ID counter for auto_inst_map (replaces pointer-based key).
    pub(super) next_phrase_id: u32,

    /// Mapping from FuncCall member to auto-created component instance name.
    /// Key: stable u32 ID assigned via `assign_phrase_ids()` before processing.
    /// Clone-safe: the ID is stored in McFuncCall.id and survives cloning.
    pub(super) auto_inst_map: HashMap<u32, String>,

    /// Instantiation diagnostic collector (non-fatal errors/warnings)
    ///
    /// Records issues encountered during instantiation without interrupting the flow.
    /// The caller can inspect results via `has_errors()` / `all_diagnostics()`.
    pub diagnostics: Vec<InstDiagnostic>,

    /// ★ M11.3: set of component instance names that are Transposed (bridge passive)
    pub(super) bridge_passive_names: HashSet<String>,

    /// Current connection line's source span (for diagnostic position reporting).
    /// Updated when processing each top-level connection line in `instantiate_lines_resilient`.
    /// Used as fallback when NetPoint.src_pos is unavailable.
    pub(super) current_line_span: Option<crate::ast::ast_semantic::Span>,

    /// ★ P9-A2: Current port group name for provenance tracking.
    /// Set when processing a connection that involves a port group (e.g., flash.SPI, mic.MIC).
    /// Used by `make_conn_with_provenance` to tag connections with their port group.
    /// Cleared when the connection line is fully processed.
    pub(super) current_port_group: Option<String>,

    /// Component class names whose instantiation failed (any instance of this class).
    /// Used to skip lines that reference failed components.
    pub(super) failed_classes: HashSet<String>,

    /// Structured failure records for known_missing.md (G4 baseline).
    pub(super) failed_records: Vec<FailedRecord>,

    /// Set of module-level function names that have been auto-invoked.
    /// Prevents double execution when a function is both auto-invoked and
    /// explicitly called from a parent module (e.g. `mcu.i2c()`).
    pub(super) auto_invoked_funcs: HashSet<String>,
}

/// Structured record of a failed component instantiation.
#[derive(Debug, Clone)]
pub(crate) struct FailedRecord {
    pub module: String,
    pub src_line: Option<usize>,
    pub component_name: String,
    pub class_name: String,
    pub reason: String,
}

impl McModuleInst {
    /// Resolve the URI of the file containing the module definition
    ///
    /// Priority:
    /// 1. The module definition itself carries its source URI (`def.uri`)
    /// 2. Use the current current_uri (caller context)
    /// 3. Empty string (should not be reached in theory)
    fn resolve_def_uri(def: &McModule) -> McURI {
        if !def.uri.as_str().is_empty() {
            return def.uri.clone();
        }
        current_uri::try_get().unwrap_or_default()
    }

    /// Create a new module instance
    pub fn new(name: &str, def: Arc<McModule>) -> Self {
        let def_uri = Self::resolve_def_uri(&def);
        Self {
            name: name.to_string(),
            def,
            def_uri,
            params: McParamBindings::new(),
            ports: Vec::new(),
            components: Vec::new(),
            sub_modules: Vec::new(),
            connections: Vec::new(),
            nets: Vec::new(),
            conn_id_counter: 0,
            labels: HashMap::new(),
            buses: HashMap::new(),
            auto_inst_counter: HashMap::new(),
            next_phrase_id: 0,
            auto_inst_map: HashMap::new(),
            diagnostics: Vec::new(),
            bridge_passive_names: HashSet::new(),
            current_line_span: None,
            current_port_group: None,
            failed_classes: HashSet::new(),
            failed_records: Vec::new(),
            auto_invoked_funcs: HashSet::new(),
        }
    }

    /// Create a module instance with parameters
    pub fn with_params(
        name: &str,
        def: Arc<McModule>,
        param_values: &[McParamValue],
    ) -> Result<Self, InstError> {
        let params = McParamBindings::bind(&def.params, param_values)
            .map_err(|e| InstError::Other(format!("Parameter binding failed: {e:?}")))?;
        let def_uri = Self::resolve_def_uri(&def);

        Ok(Self {
            name: name.to_string(),
            def,
            def_uri,
            params,
            ports: Vec::new(),
            components: Vec::new(),
            sub_modules: Vec::new(),
            connections: Vec::new(),
            nets: Vec::new(),
            conn_id_counter: 0,
            labels: HashMap::new(),
            buses: HashMap::new(),
            auto_inst_counter: HashMap::new(),
            next_phrase_id: 0,
            auto_inst_map: HashMap::new(),
            diagnostics: Vec::new(),
            bridge_passive_names: HashSet::new(),
            current_line_span: None,
            current_port_group: None,
            failed_classes: HashSet::new(),
            failed_records: Vec::new(),
            auto_invoked_funcs: HashSet::new(),
        })
    }

    /// Execute instantiation
    ///
    /// Uses a fault-tolerant strategy: errors in each phase are recorded into `diagnostics` instead of interrupting the flow.
    /// Even if some sub-modules/connection lines fail, still try to complete the net table construction.
    /// The caller checks results via `has_errors()` / `all_diagnostics()`.
    ///
    /// ## Flow
    /// 1. Switch `current_uri` to the file containing this module definition
    /// 2. (Optional) When `MC_INST_DUMP=1` is enabled, print pass1 input snapshot
    /// 3. Phase 1: interface instantiation (ports)
    /// 4. Phase 3: declared instantiation (components / sub-modules / labels)
    /// 5. Phase 4: connection line processing
    /// 6. Net table construction
    /// 7. (Optional) When `MC_INST_DUMP=1` is enabled, print pass2 output + pass1↔pass2 diff
    /// 8. Restore `current_uri`
    pub fn instantiate(&mut self) -> Result<(), InstError> {
        // ★ Switch current_uri to the file containing this module definition to ensure correct internal symbol resolution
        //   Sub-modules may be defined in different files; mcb_get_cmie() depends on current_uri for context lookup
        let saved_uri = current_uri::try_get();
        if !self.def_uri.is_empty() {
            current_uri::set(&self.def_uri);
        }

        // ── DEBUG: pass1 input snapshot (optional) ────────────────────────────
        if dump::dump_enabled() {
            self.dump_pass1_input();
        }

        // 1. Instantiate interface (ports) — rarely fails
        if let Err(e) = self.instantiate_interface() {
            self.record_error(
                crate::errcodes::INST_IFACE_INSTANTIATE_FAILED,
                crate::errcodes::format_msg(crate::errcodes::INST_IFACE_INSTANTIATE_FAILED, &[&e]),
            );
        }

        // 2. Process instances declared in the symbol table (components and sub-modules) — per-instance fault tolerance
        self.instantiate_declarations_resilient();

        // 3. Process connection lines — per-line fault tolerance
        self.instantiate_lines_resilient();

        // 3.5 Auto-invoke module-level parameterless functions (closures)
        // Module-level functions like `func i2c() { ... }` with no parameters
        // are auto-invoked during instantiation. Functions with parameters
        // (e.g. `func do_flash(spi)`) must be explicitly called.
        self.auto_invoke_module_funcs();

        // 3.6 Post-processing (moved from instantiate_lines_resilient to cover auto-invoked closures)
        self.infer_bare_port_members_from_buses();
        self.validate_expanded_net_points();
        self.dedup_connections();
        self.check_unbound_param_ports();

        // 4. Build the final net table (based on successful connections)
        self.build_net_table();

        // ── DEBUG: pass2 output + pass1↔pass2 diff (optional) ─────────────
        if dump::dump_enabled() {
            self.dump_pass2_output();
            self.dump_pass_diff();
        }

        // ★ Restore the caller's current_uri context
        match saved_uri {
            Some(ref uri) => current_uri::set(uri),
            None => current_uri::reset(),
        }

        Ok(()) // Always return Ok — errors have been recorded to diagnostics
    }

    /// Auto-invoke module-level parameterless functions (closures).
    ///
    /// Module-level functions like `func i2c() { ... }` with no parameters
    /// are treated as closures and auto-invoked during instantiation.
    /// Functions with parameters (e.g. `func do_flash(spi)`) must be
    /// explicitly called and are skipped here.
    pub(super) fn auto_invoke_module_funcs(&mut self) {
        let funcs: Vec<_> = self.def.funcs.iter().cloned().collect();
        mcc_dbg!(
            "inst::mod",
            "[P2-4-AUTO] module '{}' has {} funcs: {:?}",
            self.name,
            funcs.len(),
            funcs
                .iter()
                .map(|f| format!("{} (arity={})", f.name, f.params.iter().count()))
                .collect::<Vec<_>>()
        );
        for func in funcs {
            let arity = func.params.iter().count();
            if arity > 0 {
                continue; // skip parameterized functions
            }
            if func.lines.is_empty() {
                continue;
            }
            mcc_dbg!(
                "inst::mod",
                "[P2-4-AUTO] auto-invoking module func '{}' with {} body lines",
                func.name,
                func.lines.len()
            );
            for line in &func.lines {
                mcc_dbg!(
                    "inst::mod",
                    "[P2-4-AUTO-DBG] module '{}' func '{}' processing line: {:?}",
                    self.name,
                    func.name,
                    std::mem::discriminant(line)
                );
                if let Err(e) = self.process_line(line) {
                    mcc_dbg!(
                        "inst::mod",
                        "[P2-4-AUTO-DBG] module '{}' func '{}' line FAILED: {e}",
                        self.name,
                        func.name
                    );
                    self.record_warning(
                        crate::errcodes::INST_FUNC_BODY_LINE_FAILED,
                        crate::errcodes::format_msg(
                            crate::errcodes::INST_FUNC_BODY_LINE_FAILED,
                            &[&func.name, &e],
                        ),
                    );
                }
            }
            // Mark as auto-invoked to prevent double execution when
            // explicitly called from a parent module (e.g. `mcu.i2c()`).
            self.auto_invoked_funcs.insert(func.name.to_string());
        }
    }

    // ========================================================================
    // Diagnostic helper methods
    // ========================================================================

    /// Record a non-fatal error to the diagnostic collector
    pub(super) fn record_error(&mut self, code: u32, message: String) {
        mcc_dbg!(
            "inst::mod",
            "[inst:{}] ERROR #{}: {}",
            self.name,
            code,
            message
        );
        self.diagnostics
            .push(InstDiagnostic::error(code, &self.name, message));
    }

    /// Record a warning to the diagnostic collector
    pub(super) fn record_warning(&mut self, code: u32, message: String) {
        mcc_dbg!(
            "inst::mod",
            "[inst:{}] WARN #{}: {}",
            self.name,
            code,
            message
        );
        self.diagnostics
            .push(InstDiagnostic::warning(code, &self.name, message));
    }

    /// Merge diagnostics from a sub-module into the current module
    pub(super) fn merge_diagnostics_from(&mut self, child: &McModuleInst) {
        self.diagnostics.extend(child.diagnostics.iter().cloned());
    }

    /// Whether there is any error-level diagnostic
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.level == InstDiagLevel::Error)
    }

    /// Recursively collect all diagnostics (including sub-modules)
    pub fn all_diagnostics(&self) -> Vec<&InstDiagnostic> {
        let mut all: Vec<&InstDiagnostic> = self.diagnostics.iter().collect();
        for sub in &self.sub_modules {
            all.extend(sub.all_diagnostics());
        }
        all
    }

    // ========================================================================
    // Public accessors — used by InstTable flatten (Step 0)
    // ========================================================================

    /// Get a read-only reference to all internal labels
    pub fn get_labels(&self) -> &HashMap<String, NetPoint> {
        &self.labels
    }

    /// Get a read-only reference to all bus instances
    pub fn get_buses(&self) -> &HashMap<String, McBusInst> {
        &self.buses
    }

    /// Return this module's union-find merged nets in deterministic name order.
    pub fn sorted_nets(&self) -> Vec<(&str, &[NetPoint])> {
        let mut nets: Vec<(&str, &[NetPoint])> = self
            .nets
            .iter()
            .map(|(name, points)| (name.as_str(), points.as_slice()))
            .collect();
        nets.sort_by(|a, b| a.0.cmp(b.0));
        nets
    }

    // ========================================================================
    // ID counter / naming (small utilities reused across multiple module files)
    // ========================================================================

    /// Automatically generate a unique instance name
    ///
    /// Each type maintains an independent counter, generating names in `{type}_{n}` format:
    /// - First CAP → `CAP_1`
    /// - Second CAP → `CAP_2`
    /// - First RES → `RES_1`
    pub(super) fn auto_name(&mut self, type_name: &str) -> String {
        let counter = self
            .auto_inst_counter
            .entry(type_name.to_string())
            .or_insert(0);
        *counter += 1;
        let name = format!("{type_name}_{counter}");
        if type_name.contains("CAP") || type_name.contains("RES") || type_name.starts_with("@") {
            mcc_dbg!(
                "inst::mod",
                "[AUTO-NAME] module={} type={type_name} counter={counter} name={name}",
                self.name
            );
        }
        name
    }

    /// Take the next connection ID
    pub(super) fn next_conn_id(&mut self) -> u32 {
        let id = self.conn_id_counter;
        self.conn_id_counter += 1;
        id
    }

    // ========================================================================
    // Net table construction
    // ========================================================================

    pub(super) fn build_net_table(&mut self) {
        let mut table = NetTable::new();

        for port in &self.ports {
            // ★ Multi-member ports (bracket `[A, B]` / curly `name{A, B}` /
            // interface ports) resolve their connections via member paths
            // (`vin.POWER_SYS`, `vin.GND`), never via the whole-port literal.
            // Registering the whole-port name creates an orphan 1-point stub
            // net (e.g. `[POWER_SYS, GND]`, `vin`, `dc{VDD_3V3, GND}`).
            // Only scalar ports (no bus_members) get a whole-port point.
            if port.bus_members.is_empty() {
                table.register_port(&port.name, port.iotype.clone());
            }
        }

        // ★ P7-4 diagnostic: print the connection table in order (id + point paths), for cross-build diff of connection order
        crate::vlog!(
            "[det-conn] module '{}' {} connection(s) in order:",
            self.name,
            self.connections.len()
        );
        for (i, c) in self.connections.iter().enumerate() {
            crate::vlog!(
                "[det-conn]  [{:>3}] id={:<4} {:?}",
                i,
                c.id,
                c.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>()
            );
        }

        for conn in &self.connections {
            table.add_connection(conn);
        }

        self.nets = table
            .into_nets()
            .into_iter()
            .map(|(name, pts)| (name, pts))
            .collect();
        // [P0-DET] deterministic net order (feeds net ids downstream): sort by
        // name, then by the joined point paths (stable for duplicate "GND").
        self.nets.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| {
                let ap: String = a.1.iter().map(|p| p.path.as_str()).collect();
                let bp: String = b.1.iter().map(|p| p.path.as_str()).collect();
                ap.cmp(&bp)
            })
        });

        // P2-4-US513-DEBUG
        if self.name == "mcu513" {
            mcc_dbg!("inst::mod", "[P2-4-US513] build_net_table for mcu513:");
            for (net_name, points) in &self.nets {
                mcc_dbg!(
                    "inst::mod",
                    "[P2-4-US513]   net '{}': {:?}",
                    net_name,
                    points.iter().map(|p| p.path.clone()).collect::<Vec<_>>()
                );
            }
        }

        // ── Ground net re-partition: GND by writing line + reference form ──
        self.split_ground_nets();
    }

    // ========================================================================
    // Ground net re-partition (GND by writing line + reference form)
    // ========================================================================

    /// Exact ground-name leaf matcher (netcheck's `is_ground_name`, NOT the
    /// `starts_with` variant — `GND_OUT` / `VIN` etc. must not be treated as
    /// ground, otherwise the modldo split would be re-merged).
    fn is_ground_leaf(s: &str) -> bool {
        let leaf = s.rsplit('.').next().unwrap_or(s);
        matches!(
            leaf.to_uppercase().as_str(),
            "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GROUND" | "EARTH"
        )
    }

    /// Re-partition the module's ground nets into local ground groups.
    ///
    /// Replaces the old "one global GND net per module" with per-line local
    /// ground groups. Rule (user-confirmed, GENERAL — applies to every module):
    ///   - each statement line's GND defaults to an independent local ground net;
    ///   - a `component{...GND}` reference (the ground is drawn from the same
    ///     physical pin of the same component) merges the hanging 2-pin
    ///     passives' ground endpoints under a pin key `(component, gnd_pin)`;
    ///   - a bare label / bus-member ground (`GND`, `[X, GND]`, `(X, GND)`) is a
    ///     name reference only: each source statement forms its own group.
    ///
    /// Every produced group is a net named **`GND`** carrying its own local
    /// ground symbol, so all local grounds read "GND" (schematic convention).
    /// Port / label / sub-module-port points are never split — only local
    /// component pins hanging on a ground hang off the local GND symbols.
    fn split_ground_nets(&mut self) {
        if self.nets.len() < 2 || self.connections.is_empty() {
            return;
        }

        fn owner_of(p: &str) -> &str {
            match p.rfind('.') {
                Some(i) => &p[..i],
                None => p,
            }
        }

        // Local component instance names: a point whose owner is one of these is
        // a "hanging passive" (e.g. `CAP_1.2`). Port / label / sub-module-port
        // points are NOT hangings and stay in the central ground group.
        let component_owners: std::collections::BTreeSet<&str> =
            self.components.iter().map(|c| c.name.as_str()).collect();

        // Indices of nets that contain a ground-name point.
        let ground_net_idx: Vec<usize> = self
            .nets
            .iter()
            .enumerate()
            .filter(|(_, (_, pts))| pts.iter().any(|p| Self::is_ground_leaf(&p.path)))
            .map(|(i, _)| i)
            .collect();

        // Process in reverse so earlier indices stay valid while we remove/insert.
        for &idx in ground_net_idx.iter().rev() {
            let (_name, points) = self.nets.remove(idx);
            let point_set: HashSet<&str> = points.iter().map(|p| p.path.as_str()).collect();

            // Connections touching this net, with line + classification.
            struct Touch {
                id: u32,
                line: u32,
                label_grounds: Vec<String>,
                others: Vec<String>,
            }
            let touches: Vec<Touch> = self
                .connections
                .iter()
                .filter_map(|c| {
                    let mut label_grounds = Vec::new();
                    let mut others = Vec::new();
                    for p in &c.points {
                        if !point_set.contains(p.path.as_str()) {
                            continue;
                        }
                        if Self::is_ground_leaf(&p.path) {
                            label_grounds.push(p.path.clone());
                        } else {
                            others.push(p.path.clone());
                        }
                    }
                    if label_grounds.is_empty() && others.is_empty() {
                        return None;
                    }
                    let line = c.source_span.as_ref().map(|(_, l)| *l).unwrap_or(0);
                    Some(Touch {
                        id: c.id,
                        line,
                        label_grounds,
                        others,
                    })
                })
                .collect();

            if touches.is_empty() {
                self.nets.push(("GND".to_string(), points));
                continue;
            }

            // Ground sources:
            //  - label grounds (leaf is a ground name)
            //  - component ground pins: a local-component pin appearing in >= 2
            //    touching connections AND paired with a label ground in at least
            //    one (e.g. `lp322dcdc.2` in `GND ~ lp322dcdc.2` + cap mountings).
            let mut freq: HashMap<&str, usize> = HashMap::new();
            for t in &touches {
                for o in &t.others {
                    *freq.entry(o.as_str()).or_insert(0) += 1;
                }
            }
            let mut comp_sources: HashSet<String> = HashSet::new();
            for t in &touches {
                if t.label_grounds.is_empty() {
                    continue;
                }
                for o in &t.others {
                    if component_owners.contains(owner_of(o))
                        && freq.get(o.as_str()).copied().unwrap_or(0) >= 2
                    {
                        comp_sources.insert(o.clone());
                    }
                }
            }

            // A "hanging" point = a local-component pin that is NOT a ground
            // source. Port / label / sub-module-port points never hang.
            let is_hanging = |o: &str| -> bool {
                if comp_sources.contains(o) {
                    return false;
                }
                component_owners.contains(owner_of(o))
            };

            // Central group: all ground sources (the real ground nodes).
            let mut pin_groups: std::collections::BTreeMap<String, Vec<NetPoint>> =
                std::collections::BTreeMap::new();
            // Line groups: per-statement hanging pins on a label/bus ground.
            // Keyed by (line, conn id) — connection id is a stable per-statement
            // key while the real line number lookup is unreliable.
            let mut line_groups: std::collections::BTreeMap<(u32, u32), Vec<NetPoint>> =
                std::collections::BTreeMap::new();

            for t in &touches {
                for o in &t.others {
                    if !is_hanging(o) {
                        continue;
                    }
                    let Some(np) = points.iter().find(|p| &p.path == o).cloned() else {
                        continue;
                    };
                    if t.label_grounds.is_empty() {
                        if let Some(src) = t.others.iter().find(|o2| comp_sources.contains(*o2)) {
                            pin_groups.entry(src.clone()).or_default().push(np);
                        }
                    } else {
                        line_groups.entry((t.line, t.id)).or_default().push(np);
                    }
                }
            }

            // Central group = every point NOT consumed by a pin/line group
            // (label grounds, component ground sources, sub-module ports, labels,
            // and any hanging pin that could not be grouped). This guarantees no
            // point is ever dropped by the re-partition.
            let mut consumed: HashSet<&str> = HashSet::new();
            for (_, np) in pin_groups.iter() {
                for p in np {
                    consumed.insert(p.path.as_str());
                }
            }
            for (_, np) in line_groups.iter() {
                for p in np {
                    consumed.insert(p.path.as_str());
                }
            }
            let mut central: Vec<NetPoint> = points
                .iter()
                .filter(|p| !consumed.contains(p.path.as_str()))
                .cloned()
                .collect();

            // Rebuild the split nets. Every group carries its own local GND
            // symbol (a synthetic NetPoint whose path is "GND"), so each is a
            // complete >=2-point net. All groups are named "GND".
            let mut out: Vec<(String, Vec<NetPoint>)> = Vec::new();
            if !central.is_empty() {
                central.sort_by(|a, b| a.path.cmp(&b.path));
                out.push(("GND".to_string(), central));
            }
            for (_src, mut np) in pin_groups {
                np.sort_by(|a, b| a.path.cmp(&b.path));
                if !np.is_empty() {
                    let mut grp = vec![NetPoint::new("GND", IOType::None)];
                    grp.append(&mut np);
                    out.push(("GND".to_string(), grp));
                }
            }
            for (_key, mut np) in line_groups {
                np.sort_by(|a, b| a.path.cmp(&b.path));
                if !np.is_empty() {
                    let mut grp = vec![NetPoint::new("GND", IOType::None)];
                    grp.append(&mut np);
                    out.push(("GND".to_string(), grp));
                }
            }

            let groups_str = out
                .iter()
                .map(|(n, p)| format!("{}({})", n, p.len()))
                .collect::<Vec<_>>()
                .join(", ");
            crate::vlog!(
                "[split-ground] module '{}': net '{}' ({:?}) -> {}",
                self.name,
                "GND",
                points.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
                groups_str
            );
            self.nets.extend(out);
        }

        // [P0-DET] deterministic net order (feeds net ids downstream): sort by
        // name, then by the joined point paths (stable for duplicate "GND").
        self.nets.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| {
                let ap: String = a.1.iter().map(|p| p.path.as_str()).collect();
                let bp: String = b.1.iter().map(|p| p.path.as_str()).collect();
                ap.cmp(&bp)
            })
        });
    }
}

// ============================================================================
// Display
// ============================================================================

impl std::fmt::Display for McModuleInst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Module: {}", self.name)?;

        let inputs: Vec<_> = self
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::In))
            .collect();
        let outputs: Vec<_> = self
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::Out))
            .collect();
        let bidirs: Vec<_> = self
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::InOut))
            .collect();

        if !inputs.is_empty() {
            writeln!(f, "  Inputs:")?;
            for port in &inputs {
                writeln!(f, "    - {port}")?;
            }
        }

        if !outputs.is_empty() {
            writeln!(f, "  Outputs:")?;
            for port in &outputs {
                writeln!(f, "    - {port}")?;
            }
        }

        if !bidirs.is_empty() {
            writeln!(f, "  Bidirs:")?;
            for port in &bidirs {
                writeln!(f, "    - {port}")?;
            }
        }

        if !self.components.is_empty() {
            writeln!(f, "  Components:")?;
            for comp in &self.components {
                writeln!(f, "    - {comp}")?;
            }
        }

        if !self.sub_modules.is_empty() {
            writeln!(f, "  Sub-modules:")?;
            for sub in &self.sub_modules {
                write!(f, "    ")?;
                // Recursively indent sub-module content
                let sub_str = format!("{sub}");
                for (i, line) in sub_str.lines().enumerate() {
                    if i == 0 {
                        writeln!(f, "{line}")?;
                    } else {
                        writeln!(f, "    {line}")?;
                    }
                }
            }
        }

        if !self.buses.is_empty() {
            writeln!(f, "  Buses:")?;
            for (name, bus) in &self.buses {
                writeln!(f, "    {}{{{}}}", name, bus.members.join(", "),)?;
            }
        }

        if !self.connections.is_empty() {
            writeln!(f, "  Connections:")?;
            for conn in &self.connections {
                writeln!(f, "    - {conn}")?;
            }
        }

        if !self.nets.is_empty() {
            writeln!(f, "  Nets:")?;
            for (name, points) in &self.nets {
                let points_str: Vec<String> = points.iter().map(|p| p.to_string()).collect();
                writeln!(f, "    {}: [{}]", name, points_str.join(", "))?;
            }
        }

        if !self.diagnostics.is_empty() {
            writeln!(f, "  Diagnostics ({}):", self.diagnostics.len())?;
            for diag in &self.diagnostics {
                writeln!(f, "    - {diag}")?;
            }
        }

        Ok(())
    }
}
