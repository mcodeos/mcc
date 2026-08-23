// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 instantiation — Module instance
//!
//! McModuleInst is the core data structure of the instantiation phase, representing a complete module instance.
//!
//! ## Module split (after refactoring)
//! - `mod.rs`         —— Type definitions, construction, `instantiate()` top-level flow, diagnostics, Display, ID counter
//! - `phases.rs`      —— Phase 1/3 entry (interfaces, declarations, connection stmts)
//! - `stmt.rs`        —— Single stmt expansion/dispatch (process_stmt / process_member_internal)
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
mod phases;
mod points;
mod stmt;
mod subst;

use super::mc_bus::McBusInst;
use super::mc_comp::McComponentInst;
use super::mc_net::{
    ConnectionInst, InstDiagLevel, InstDiagnostic, InstError, NetPoint, NetTable, PortInst,
};
use crate::instant::provenance::ExpansionKind;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::common::IOType;
use crate::semantic::mc_func::McFunction;
use crate::semantic::module::McModule;
use crate::vector::model::trunk::TrunkKind;
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
    /// Ground nets stay merged per module scope: a bare `GND` label follows the
    /// module's input ground source instead of being re-partitioned into local
    /// groups. Duplicate net names are still legal (e.g. distinct rail-member
    /// grounds such as `va.GND` vs `dc.GND`), hence a `Vec` not a `HashMap`.
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

    /// Current connection stmt's source span (for diagnostic position reporting).
    /// Updated when processing each top-level connection stmt in `instantiate_stmts_resilient`.
    /// Used as fallback when NetPoint.src_pos is unavailable.
    /// Unified [`SourcePos`] — uri + byte offset (§7.11(3)).
    pub(super) current_stmt_span: Option<crate::semantic::common::SourcePos>,

    /// Func-body expansion provenance. Set by the func-body
    /// expansion sites (user funcs / component methods / constructors / module
    /// closures) before processing each body line. Takes precedence over
    /// `current_stmt_span` when attributing anonymous instance names and
    /// connection source stmts, because the func may live in another file.
    /// Unified [`SourcePos`] — uri + byte offset (§7.11(3)).
    pub(super) current_func_span: Option<crate::semantic::common::SourcePos>,

    /// ★ P9-A2: Current trunk group name for provenance tracking.
    /// Set when processing a connection that involves a port group (e.g., flash.SPI, mic.MIC).
    /// Used by `make_conn_with_provenance` to tag connections with their trunk group.
    /// Cleared when the connection line is fully processed.
    pub(super) current_trunk: Option<String>,

    /// ★ §8.9.4: coarse kind of `current_trunk` (`Bus`/`Interface`/`List`/`Plain`),
    /// recorded at the source so `Trunk.kind` does not have to be re-derived.
    /// RAII-managed together with `current_trunk` by `with_trunk`.
    pub(super) current_trunk_kind: Option<TrunkKind>,

    /// ★ §8.9.4: standardized interface class of `current_trunk` (e.g.
    /// `UART.TTL`) when the port is an interface binding; carried into
    /// `TrunkCtx.iface_class` → `TrunkEnd.iface_class`. RAII-managed with
    /// `current_trunk` / `current_trunk_kind` by `with_trunk`.
    pub(super) current_trunk_iface: Option<String>,

    /// Component class names whose instantiation failed (any instance of this class).
    /// Used to skip stmts that reference failed components.
    pub(super) failed_classes: HashSet<String>,

    /// Structured failure records for known_missing.md (G4 baseline).
    pub(super) failed_records: Vec<FailedRecord>,

    /// Set of module-level function names that have been auto-invoked.
    /// Prevents double execution when a function is both auto-invoked and
    /// explicitly called from a parent module (e.g. `mcu.i2c()`).
    pub(super) auto_invoked_funcs: HashSet<String>,

    /// §11.6 chain-shunt context: when set, `wire_builtin_twopin` defers a
    /// `.Cap(_, _)` all-placeholder call (both endpoints provided by the outer
    /// chain) instead of reporting E4176. Set for the whole series by
    /// `process_series` when it detects a chain shunt; RAII save/restore via
    /// `with_twopin_defer` so recursive `process_stmt` calls (P2-5 expansion)
    /// re-enter `process_series` without leaking the flag into the next
    /// standalone `.Cap(_, _)`.
    pub(super) defer_twopin_placeholders: bool,

    /// Expansion provenance: this module's expansion log (module-local id space).
    /// Products tagged with `expansion_id` index into `expansion.records`.
    pub expansion: crate::instant::provenance::ExpansionLog,

    /// Expansion provenance: index into the **parent** module's `ExpansionLog`
    /// when this sub-module was created by an expansion. None for the top-level
    /// module / module-level creations.
    pub expansion_id: Option<usize>,
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

/// RAII guard for the thread-local `current_uri` (context state machine
/// migration, §7.2 / §7.11(2)): saves the previous value on construction,
/// installs `uri`, and restores the previous value on drop — an early
/// `return` / error can no longer leave the wrong file URI installed for
/// later symbol / line-index lookups.
pub(crate) struct CurrentUriGuard {
    saved: Option<McURI>,
}

impl CurrentUriGuard {
    pub(crate) fn new(uri: &McURI) -> Self {
        let saved = current_uri::try_get();
        if !uri.as_str().is_empty() {
            current_uri::set(uri);
        }
        CurrentUriGuard { saved }
    }
}

impl Drop for CurrentUriGuard {
    fn drop(&mut self) {
        match &self.saved {
            Some(uri) => current_uri::set(uri),
            None => current_uri::reset(),
        }
    }
}

/// Anonymous-instance naming category (§7.11(4)). The special prefixes are
/// generated inside `auto_name`, so call sites never concatenate `@_phantom_`
/// or `@?` strings themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutoNameKind {
    /// Real anonymous device: reference-designator style (`_C1`, `_R2`).
    Normal,
    /// Internal isolation node: `@_phantom_<name>_<n>` (never a device).
    Phantom,
    /// Stub for an unrecognized class name: `@?<name>_<n>`.
    Stub,
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
            current_stmt_span: None,
            current_func_span: None,
            current_trunk: None,
            current_trunk_kind: None,
            current_trunk_iface: None,
            failed_classes: HashSet::new(),
            failed_records: Vec::new(),
            auto_invoked_funcs: HashSet::new(),
            defer_twopin_placeholders: false,
            expansion: crate::instant::provenance::ExpansionLog::default(),
            expansion_id: None,
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
            current_stmt_span: None,
            current_func_span: None,
            current_trunk: None,
            current_trunk_kind: None,
            current_trunk_iface: None,
            failed_classes: HashSet::new(),
            failed_records: Vec::new(),
            auto_invoked_funcs: HashSet::new(),
            defer_twopin_placeholders: false,
            expansion: crate::instant::provenance::ExpansionLog::default(),
            expansion_id: None,
        })
    }

    // ========================================================================
    // Unified product factories (expansion provenance tagging, §7.11)
    // ========================================================================

    /// Push a component instance, tagging it with the current expansion id.
    /// Centralizes `expansion_id` tagging so products never bypass provenance.
    /// Products already tagged explicitly (e.g. by a construction entry that
    /// returns them to be pushed later) keep their tag.
    pub(super) fn add_component(&mut self, inst: McComponentInst) {
        let mut inst = inst;
        if inst.expansion_id.is_none() {
            inst.expansion_id = self.expansion.current_id();
        }
        self.components.push(inst);
    }

    /// Push a sub-module instance, tagging it with the current expansion id.
    pub(super) fn add_submodule(&mut self, inst: McModuleInst) {
        let mut inst = inst;
        if inst.expansion_id.is_none() {
            inst.expansion_id = self.expansion.current_id();
        }
        self.sub_modules.push(inst);
    }

    /// Push a connection, tagging it with the current expansion id.
    pub(super) fn add_connection(&mut self, conn: ConnectionInst) {
        let mut conn = conn;
        if conn.expansion_id.is_none() {
            conn.expansion_id = self.expansion.current_id();
        }
        self.connections.push(conn);
    }

    /// Current statement offset for provenance (top-level statement span start).
    /// Unified [`SourcePos`] (uri + byte offset, §7.11(3)); None when no
    /// statement is being processed.
    pub(super) fn current_call_site(&self) -> Option<crate::semantic::common::SourcePos> {
        self.current_stmt_span
            .as_ref()
            .map(|s| crate::semantic::common::SourcePos::new(self.def_uri.clone(), s.offset))
    }

    /// Function definition site (unified [`SourcePos`], §7.11(3)).
    pub(super) fn func_def_site(
        func_def: &McFunction,
    ) -> Option<crate::semantic::common::SourcePos> {
        let uri = func_def.source_uri().cloned()?;
        let off = func_def.span.as_ref().map(|sp| sp.start as u32)?;
        Some(crate::semantic::common::SourcePos::new(uri, off))
    }

    /// Execute instantiation
    ///
    /// Uses a fault-tolerant strategy: errors in each phase are recorded into `diagnostics` instead of interrupting the flow.
    /// Even if some sub-modules/connection stmts fail, still try to complete the net table construction.
    /// The caller checks results via `has_errors()` / `all_diagnostics()`.
    ///
    /// ## Flow
    /// 1. Switch `current_uri` to the file containing this module definition
    ///    (RAII guard, §7.2 — restored on every exit path)
    /// 2. (Optional) When `MC_INST_DUMP=1` is enabled, print pass1 input snapshot
    /// 3. Phase 1: interface instantiation (ports)
    /// 4. Phase 3: declared instantiation (components / sub-modules / labels)
    /// 5. Phase 4: connection line processing
    /// 6. Net table construction
    /// 7. (Optional) When `MC_INST_DUMP=1` is enabled, print pass2 output + pass1↔pass2 diff
    pub fn instantiate(&mut self) -> Result<(), InstError> {
        // ★ Switch current_uri to the file containing this module definition to ensure correct internal symbol resolution
        //   Sub-modules may be defined in different files; mcb_get_cmie() depends on current_uri for context lookup.
        //   RAII (§7.2): the guard restores the caller's URI on every exit path.
        let _uri_guard = CurrentUriGuard::new(&self.def_uri);

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

        // 3. Process connection stmts — per-stmt fault tolerance
        self.instantiate_stmts_resilient();

        // 3.5 Auto-invoke module-level parameterless functions (closures)
        // Module-level functions like `func i2c() { ... }` with no parameters
        // are auto-invoked during instantiation. Functions with parameters
        // (e.g. `func do_flash(spi)`) must be explicitly called.
        self.auto_invoke_module_funcs();

        // 3.6 Post-processing (moved from instantiate_stmts_resilient to cover auto-invoked closures)
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
            if func.stmts.is_empty() {
                continue;
            }
            mcc_dbg!(
                "inst::mod",
                "[P2-4-AUTO] auto-invoking module func '{}' with {} body stmts",
                func.name,
                func.stmts.len()
            );
            // ── Expansion provenance: AutoInvoke (call_site = None, no user
            //    call statement; products attach to the module node) ──
            let eidx = self.expansion.begin(
                ExpansionKind::AutoInvoke,
                None,
                func.name.to_string(),
                None,
                Self::func_def_site(&func),
            );
            for (li, stmt) in func.stmts.iter().enumerate() {
                mcc_dbg!(
                    "inst::mod",
                    "[P2-4-AUTO-DBG] module '{}' func '{}' processing stmt: {:?}",
                    self.name,
                    func.name,
                    std::mem::discriminant(stmt)
                );
                // Attribute anonymous instances/connections of this body stmt
                // to its exact source stmt in the func's own file (RAII:
                // `with_func_stmt` restores `current_func_span` on every exit).
                self.with_func_stmt(&func, Some(li), |this| {
                    if let Err(e) = this.process_stmt(stmt) {
                        mcc_dbg!(
                            "inst::mod",
                            "[P2-4-AUTO-DBG] module '{}' func '{}' stmt FAILED: {e}",
                            this.name,
                            func.name
                        );
                        this.record_warning(
                            crate::errcodes::INST_FUNC_BODY_STMT_FAILED,
                            crate::errcodes::format_msg(
                                crate::errcodes::INST_FUNC_BODY_STMT_FAILED,
                                &[&func.name, &e],
                            ),
                        );
                    }
                });
            }
            self.expansion.end(eidx);
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

    /// Reference designator prefix for common inline-constructed types.
    /// Unknown or multi-segment types (dots already replaced with `_`) fall
    /// back to the full type name, e.g. `DIO.ESD` -> `DIO_ESD`.
    fn ref_designator_prefix(type_name: &str) -> &str {
        match type_name {
            "CAP" => "C",
            "RES" => "R",
            "DIO" => "D",
            "IND" => "L",
            "XTAL" => "Y",
            "HDR" => "J",
            _ => type_name,
        }
    }

    /// Automatically generate a unique instance name for an anonymous inline
    /// construction (`CAP(0.1uF)` etc.).
    ///
    /// Real devices get a reference-designator style name: `_C1`, `_R2`,
    /// `_DIO_ESD_1`. The leading `_` keeps the engine namespace separate from
    /// user-written names. The second return value is the byte offset of the
    /// construction site (decision A, §7.1; used by `mcc verify` to annotate
    /// generated instances with an `L<n>` column); it is 0 when the site is
    /// unknown or for internal phantom / stub types.
    ///
    /// Internal phantom / stub types keep their special prefix and plain
    /// numbering — they are not real devices and their names participate in
    /// normalize/reuse logic that must not see a `@line` suffix. The prefix
    /// is generated here per `AutoNameKind` (§7.11(4)); call sites never
    /// concatenate `@_phantom_` / `@?` themselves.
    pub(super) fn auto_name(&mut self, kind: AutoNameKind, type_name: &str) -> (String, u32) {
        match kind {
            AutoNameKind::Normal => {
                let prefix = Self::ref_designator_prefix(type_name);
                let counter = {
                    let c = self
                        .auto_inst_counter
                        .entry(prefix.to_string())
                        .or_insert(0);
                    *c += 1;
                    *c
                };
                let line = self.current_offset();
                let name = format!("_{prefix}{counter}");
                if type_name.contains("CAP") || type_name.contains("RES") {
                    mcc_dbg!(
                        "inst::mod",
                        "[AUTO-NAME] module={} type={type_name} counter={counter} name={name}",
                        self.name
                    );
                }
                (name, line)
            }
            AutoNameKind::Phantom => {
                let key = format!("@_phantom_{type_name}");
                let counter = self.auto_inst_counter.entry(key.clone()).or_insert(0);
                *counter += 1;
                (format!("{key}_{counter}"), 0)
            }
            AutoNameKind::Stub => {
                let key = format!("@?{type_name}");
                let counter = self.auto_inst_counter.entry(key.clone()).or_insert(0);
                *counter += 1;
                (format!("{key}_{counter}"), 0)
            }
        }
    }

    /// Enter a func-body stmt context: attribute anonymous instances and
    /// connection provenance to the exact source stmt of the construction in
    /// the func's own file (per-body-stmt offset when available, else the
    /// func's definition offset). Sets `current_func_span` to the **byte
    /// offset** (decision A, §7.1); consumers convert offset → line for
    /// display. Returns the previous context so the caller can restore it
    /// after processing the stmt — prefer [`Self::with_func_stmt`] instead,
    /// which restores on all exits (including early errors).
    pub(super) fn enter_func_stmt(
        &mut self,
        func: &McFunction,
        stmt_idx: Option<usize>,
    ) -> Option<crate::semantic::common::SourcePos> {
        let prev = self.current_func_span.clone();
        let uri = func.source_uri().cloned();
        self.current_func_span = match uri {
            Some(u) => {
                if let Some(off) = stmt_idx.and_then(|i| func.stmt_offsets.get(i)) {
                    Some(crate::semantic::common::SourcePos::new(
                        u.clone(),
                        *off as u32,
                    ))
                } else {
                    func.span.as_ref().map(|sp| {
                        crate::semantic::common::SourcePos::new(u.clone(), sp.start as u32)
                    })
                }
            }
            None => None,
        };
        prev
    }

    /// Run `f` with the func-body stmt context active and restore the
    /// previous context on every exit (RAII §7.11(2)): early `return` /
    /// `Err` inside `f` can no longer leak a stale `current_func_span` into
    /// subsequent connections.
    pub(super) fn with_func_stmt<R>(
        &mut self,
        func: &McFunction,
        stmt_idx: Option<usize>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = self.enter_func_stmt(func, stmt_idx);
        let r = f(self);
        self.current_func_span = saved;
        r
    }

    /// Run `f` with the given `current_trunk` / `current_trunk_kind` /
    /// `current_trunk_iface` active and restore the previous values on every
    /// exit (RAII §7.11(2)). The group is a connection-time hint read by
    /// `make_conn_with_provenance`; a leaked group would mis-attribute the
    /// *next* connection's port group, so the save/restore must survive early
    /// returns inside `f`.
    pub(super) fn with_trunk<R>(
        &mut self,
        group: Option<String>,
        kind: Option<TrunkKind>,
        iface_class: Option<String>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved_group = self.current_trunk.take();
        let saved_kind = self.current_trunk_kind.take();
        let saved_iface = self.current_trunk_iface.take();
        self.current_trunk = group;
        self.current_trunk_kind = kind;
        self.current_trunk_iface = iface_class;
        let r = f(self);
        self.current_trunk = saved_group;
        self.current_trunk_kind = saved_kind;
        self.current_trunk_iface = saved_iface;
        r
    }

    /// RAII save/restore for `defer_twopin_placeholders`. The chain-shunt
    /// series (`[V3V3,GND] -> CAP(..).Cap(_, _) -> [VCC,VSS]`) defers both pin
    /// bindings to the outer chain (§11.6); recursive `process_stmt` calls
    /// (P2-5 expansion) re-enter `process_series` and install their own value,
    /// so the save/restore must survive early returns inside `f`.
    pub(super) fn with_twopin_defer<R>(
        &mut self,
        defer: bool,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = self.defer_twopin_placeholders;
        self.defer_twopin_placeholders = defer;
        let r = f(self);
        self.defer_twopin_placeholders = saved;
        r
    }

    /// Byte offset of the current construction site for provenance (func-body
    /// line offset first, then the top-level statement span start, else 0).
    /// Consecutive constructions on the same source line are still
    /// distinguishable by offset (decision A, §7.1).
    fn current_offset(&self) -> u32 {
        match (&self.current_func_span, &self.current_stmt_span) {
            (Some(spos), _) => spos.offset,
            (None, Some(s)) => s.offset,
            (None, None) => 0,
        }
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

        // ── A net must have at least 2 points ──
        // A single node is not a net; drop any 1-point residue (e.g. a lone
        // label with no pins).
        self.nets.retain(|(_, pts)| pts.len() >= 2);
    }

    // ========================================================================
    // Ground identity helpers
    // ========================================================================

    /// Exact ground-name leaf matcher (netcheck's `is_ground_name`, NOT the
    /// `starts_with` variant — `GND_OUT` / `VIN` etc. must not be treated as
    /// ground).
    fn is_ground_leaf(s: &str) -> bool {
        let leaf = s.rsplit('.').next().unwrap_or(s);
        matches!(
            leaf.to_uppercase().as_str(),
            "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GROUND" | "EARTH"
        )
    }

    /// Strict DC rail identity: the ground member point owned by a rail scalar.
    ///
    /// A rail scalar (`V5V`, `usbsocket.vin`) owns its ground member
    /// `{rail}.GND`. Binding a DC port's ground member to this point keeps
    /// every rail's ground distinct until real wiring ties them together
    /// (shared component ground pins, explicit `X.GND -> GND` connections).
    /// A scalar that is itself a ground reference (bare `GND` or `s.GND`)
    /// is returned unchanged — the author is explicitly naming that net.
    pub(super) fn rail_ground_point(&self, rail: &NetPoint, gnd_member: &str) -> NetPoint {
        let leaf = rail
            .member_name
            .as_deref()
            .unwrap_or_else(|| rail.path.rsplit('.').next().unwrap_or(&rail.path));
        if Self::is_ground_leaf(leaf) {
            return rail.clone();
        }
        NetPoint::new(&format!("{}.{}", rail.path, gnd_member), IOType::None)
            .with_member_name(gnd_member)
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
