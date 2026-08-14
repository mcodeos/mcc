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
use crate::query::lookup::mcb_find_module_uri;
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
    pub nets: HashMap<String, Vec<NetPoint>>,

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

    /// Component class names whose instantiation failed (any instance of this class).
    /// Used to skip lines that reference failed components.
    pub(super) failed_classes: HashSet<String>,

    /// Structured failure records for known_missing.md (G4 baseline).
    pub(super) failed_records: Vec<FailedRecord>,

    /// Set of module-level function names that have been auto-invoked.
    /// Prevents double execution when a function is both auto-invoked and
    /// explicitly called from a parent module (e.g. `mcu513.i2c()`).
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
    /// 1. Look up the registered URI of the module definition by name from the global table
    /// 2. Use the current current_uri (caller context)
    /// 3. Empty string (should not be reached in theory)
    fn resolve_def_uri(def: &McModule) -> McURI {
        mcb_find_module_uri(&def.name)
            .or_else(current_uri::try_get)
            .unwrap_or_default()
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
            nets: HashMap::new(),
            conn_id_counter: 0,
            labels: HashMap::new(),
            buses: HashMap::new(),
            auto_inst_counter: HashMap::new(),
            next_phrase_id: 0,
            auto_inst_map: HashMap::new(),
            diagnostics: Vec::new(),
            bridge_passive_names: HashSet::new(),
            current_line_span: None,
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
            nets: HashMap::new(),
            conn_id_counter: 0,
            labels: HashMap::new(),
            buses: HashMap::new(),
            auto_inst_counter: HashMap::new(),
            next_phrase_id: 0,
            auto_inst_map: HashMap::new(),
            diagnostics: Vec::new(),
            bridge_passive_names: HashSet::new(),
            current_line_span: None,
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
        // (e.g. `func loadFlash(spi)`) must be explicitly called.
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
    /// Functions with parameters (e.g. `func loadFlash(spi)`) must be
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
            // explicitly called from a parent module (e.g. `mcu513.i2c()`).
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
            table.register_port(&port.name, port.iotype.clone());
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

        self.nets = table.into_nets();

        // P2-4-US513-DEBUG
        if self.name == "mcu513" {
            mcc_dbg!("inst::mod", "[P2-4-US513] build_net_table for mcu513:");
            let mut net_names: Vec<&String> = self.nets.keys().collect();
            net_names.sort();
            for net_name in net_names {
                let points = &self.nets[net_name];
                mcc_dbg!(
                    "inst::mod",
                    "[P2-4-US513]   net '{}': {:?}",
                    net_name,
                    points.iter().map(|p| p.path.clone()).collect::<Vec<_>>()
                );
            }
        }
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
