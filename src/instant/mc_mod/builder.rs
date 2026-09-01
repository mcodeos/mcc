// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase B of the dianlu-tree refactor (implementation plan §9 B / design §5
//! D4): [`InstantiationBuilder`] — the construction-phase carrier.
//!
//! Every construction-time scratch field is moved out of `McModuleInst` (the
//! frozen model, D4) into this builder:
//! - counters: `conn_id_counter` / `auto_inst_counter` / `next_phrase_id`
//! - span context: `current_stmt_span` / `current_func_span`
//! - trunk group context: `current_trunk` / `current_trunk_kind` / `current_trunk_iface`
//! - func scope stack: `func_scope`
//!
//! `McModuleInst` keeps the model data only — ports / components / sub_modules /
//! connections / nets / labels / buses / diagnostics / vectors — plus the
//! post-construction ledgers that later phases consult on the finished tree
//! (`auto_inst_map` / `auto_invoked_funcs` / `failed_classes` /
//! `failed_records` / `bridge_passive_names`).
//!
//! # Deref
//!
//! [`Deref`]/[`DerefMut`] target `McModuleInst` keep the ~10 construction impl
//! modules working unchanged: a builder method reads/writes model fields
//! through the deref, and reads/writes its own scratch fields directly, so the
//! Phase B migration is a pure impl-header move (`impl McModuleInst` →
//! `impl InstantiationBuilder`) with zero body changes.
//!
//! # Counter resume on re-entry
//!
//! A sub-module is finished (frozen) and pushed into `sub_modules`, then
//! re-entered when the parent calls an instance method (`mcu.i2c()`). The
//! re-entry lifts the frozen tree back into a builder
//! ([`InstantiationBuilder::new`]); the counters are resumed from the tree's
//! observable construction state (connection ids, `auto_inst_map` keys, and
//! the locked auto-name pattern `_C1` / `@_phantom_*_1` / `@?*_1`), so the
//! re-entered body never collides with the sub-module's own products (zero
//! behavior change). A fresh tree resumes to 0.

use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};

use super::{AutoNameKind, CurrentUriGuard, McModuleInst};
use crate::instant::identity::{CircuitKey, IdentityRegistry};
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{
    is_ground_name, ConnectionInst, InstDiagnostic, InstError, NetPoint, NetTable,
};
use crate::instant::provenance::ExpansionKind;
use crate::semantic::basic::mc_param::McParamBindings;
use crate::semantic::common::{IOType, McCMIE, SourcePos};
use crate::semantic::mc_func::McFunction;
use crate::semantic::validation::ledger::{self, LedgerAction, LedgerEntry, LedgerKind};
use crate::vector::model::trunk::TrunkKind;
use crate::{current_uri, McIds};

// ============================================================================
// InstantiationBuilder
// ============================================================================

/// Construction-phase carrier (design §5 D4): the tree under construction plus
/// every scratch ledger that must not survive into the frozen model. Built,
/// driven through the construction flow, then dropped by [`Self::finish`] —
/// the model itself stays shareable / cacheable / snapshot-able.
pub(crate) struct InstantiationBuilder {
    /// The frozen module model being constructed (D4). Construction methods
    /// write through this field via [`DerefMut`]; [`Self::finish`] returns it.
    tree: McModuleInst,

    /// Next connection ID.
    ///
    /// Private to the builder: consumed only through [`Self::next_conn_id`].
    conn_id_counter: u32,

    /// Auto-instantiation counter (component type name → used count), used to
    /// generate unique instance names.
    ///
    /// Private to the builder: consumed only through [`Self::auto_name`].
    auto_inst_counter: HashMap<String, u32>,

    /// Stable phrase ID counter for `auto_inst_map` (replaces pointer-based
    /// key). Written by `assign_phrase_ids` (stmt.rs) while processing a
    /// statement.
    pub(super) next_phrase_id: u32,

    /// Current connection stmt's source span (for diagnostic position
    /// reporting). Read by the construction impl modules (bus / group /
    /// fcallinst) and written by `instantiate_stmts_resilient` (phases.rs).
    pub(super) current_stmt_span: Option<SourcePos>,

    /// Func-body expansion provenance. Read by the construction impl modules
    /// (bus / group / fcallinst); set by [`Self::with_func_stmt`].
    pub(super) current_func_span: Option<SourcePos>,

    /// ★ P9-A2: Current trunk group name for provenance tracking.
    /// Set when processing a connection that involves a port group (e.g.,
    /// `flash.SPI`, `mic.MIC`); read by `make_conn_with_provenance` (group.rs).
    pub(super) current_trunk: Option<String>,

    /// ★ §8.9.4: coarse kind of `current_trunk`
    /// (`Bus`/`Interface`/`List`/`Plain`), recorded at the source so
    /// `Trunk.kind` does not have to be re-derived.
    pub(super) current_trunk_kind: Option<TrunkKind>,

    /// ★ §8.9.4: standardized interface class of `current_trunk` (e.g.
    /// `UART.TTL`) when the port is an interface binding.
    pub(super) current_trunk_iface: Option<String>,

    /// P6 passthrough scope: stack of enclosing function formal-name sets.
    /// Private to the builder: consumed only through [`Self::with_func_scope`]
    /// / [`Self::is_passthrough_formal`].
    func_scope: Vec<HashSet<String>>,

    /// Phase C1: per-build identity registry (canonical path ↔ [`NodeId`])
    /// shared by this module and every sub-module built under it. Every
    /// product pushed by [`Self::add_component`] / [`Self::add_submodule`]
    /// is interned here, so the frozen tree carries stable node ids. The
    /// registry itself is a per-build artifact: the top-level entry returns
    /// it via [`Self::into_parts`], and `DianLu` rebuilds it from the frozen
    /// tree for its consumers.
    identity: IdentityRegistry,

    /// Phase C1: canonical path of the module currently being built
    /// (`main`, `main.ldo`, ...). Products intern under
    /// `{current_path}.{name}` so ids are circuit-global.
    current_path: String,
}

impl Deref for InstantiationBuilder {
    type Target = McModuleInst;

    fn deref(&self) -> &Self::Target {
        &self.tree
    }
}

impl DerefMut for InstantiationBuilder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tree
    }
}

impl InstantiationBuilder {
    // ========================================================================
    // Construction / consumption
    // ========================================================================

    /// Wrap a module tree into a construction builder.
    ///
    /// A fresh tree (from [`McModuleInst::new`] / `with_params`) resumes every
    /// counter to 0. A re-entered tree (a finished sub-module lifted back into
    /// a builder by `run_submodule_method`) resumes the counters from the
    /// tree's observable construction state — connection ids, `auto_inst_map`
    /// keys, and the locked auto-name pattern — so the re-entered body never
    /// collides with the sub-module's own products (zero behavior change).
    pub(crate) fn new(tree: McModuleInst) -> Self {
        // Top-level entry: derive the circuit key from the tree, resume any
        // node ids the tree already carries (defensive — a plain frozen tree
        // may be lifted directly), and build from the circuit root path.
        let current_path = tree.name.clone();
        let mut identity =
            IdentityRegistry::new(CircuitKey::new(&tree.def_uri.to_string(), &tree.name));
        let mut tree = tree;
        // The circuit root is not anyone's sub-module, so it never passes
        // `add_submodule`; intern it here so the root node carries an id too
        // (Phase C1: same path → same id, root included).
        if tree.node_id.is_none() {
            tree.node_id = Some(identity.intern(&current_path));
        }
        resume_tree(&mut identity, &current_path, &tree);
        Self::assemble(tree, identity, current_path)
    }

    /// Build a sub-module re-entered from its parent (Phase B re-entry): the
    /// registry carries the parent's circuit-global ids and the current path
    /// is the sub-module's full canonical path (`main.ldo`), so products
    /// intern into the same circuit namespace.
    pub(crate) fn with_identity(
        tree: McModuleInst,
        identity: IdentityRegistry,
        current_path: String,
    ) -> Self {
        Self::assemble(tree, identity, current_path)
    }

    /// Shared constructor: resume the construction counters from the tree and
    /// assemble the builder around `identity` / `current_path`.
    fn assemble(tree: McModuleInst, identity: IdentityRegistry, current_path: String) -> Self {
        let conn_id_counter = tree
            .connections
            .iter()
            .map(|c| c.id)
            .max()
            .map_or(0, |max| max + 1);
        let next_phrase_id = tree.auto_inst_map.keys().max().map_or(0, |max| max + 1);
        let auto_inst_counter = resume_auto_inst_counter(&tree);
        Self {
            tree,
            conn_id_counter,
            auto_inst_counter,
            next_phrase_id,
            current_stmt_span: None,
            current_func_span: None,
            current_trunk: None,
            current_trunk_kind: None,
            current_trunk_iface: None,
            func_scope: Vec::new(),
            identity,
            current_path,
        }
    }

    /// Consume the builder and return the finished module tree (D4 frozen
    /// model). All scratch state is dropped here — nothing construction-phase
    /// survives into the model.
    pub(crate) fn finish(self) -> McModuleInst {
        self.tree
    }

    /// Consume the builder into (tree, identity registry): the frozen model
    /// plus the per-build identity ledger. The top-level entry hands the
    /// registry to the caller; `run_submodule_method` hands it back to the
    /// parent builder on re-entry.
    pub(crate) fn into_parts(self) -> (McModuleInst, IdentityRegistry) {
        (self.tree, self.identity)
    }

    // ========================================================================
    // Unified product factories (expansion provenance tagging, §7.11)
    // ========================================================================

    /// Push a component instance, tagging it with the current expansion id
    /// and interning its canonical path (Phase C1). Centralizes
    /// `expansion_id` tagging so products never bypass provenance. Products
    /// already tagged explicitly (e.g. by a construction entry that returns
    /// them to be pushed later) keep their tag.
    pub(super) fn add_component(&mut self, inst: McComponentInst) {
        let mut inst = inst;
        if inst.expansion_id.is_none() {
            inst.expansion_id = self.expansion.current_id();
        }
        if inst.node_id.is_none() {
            let path = format!("{}.{}", self.current_path, inst.name);
            inst.node_id = Some(self.identity.intern(&path));
        }
        self.components.push(inst);
    }

    /// Push a sub-module instance, tagging it with the current expansion id
    /// and interning its canonical path (Phase C1).
    pub(super) fn add_submodule(&mut self, inst: McModuleInst) {
        let mut inst = inst;
        if inst.expansion_id.is_none() {
            inst.expansion_id = self.expansion.current_id();
        }
        if inst.node_id.is_none() {
            let path = format!("{}.{}", self.current_path, inst.name);
            inst.node_id = Some(self.identity.intern(&path));
        }
        self.sub_modules.push(inst);
    }

    /// Full canonical path of a child under the module being built
    /// (Phase C1): `{current_path}.{name}`. Sub-module instantiation passes
    /// this down so its products intern circuit-globally.
    pub(super) fn child_path(&self, name: &str) -> String {
        format!("{}.{}", self.current_path, name)
    }

    /// Mutable access to the per-build identity registry (Phase C1) — handed
    /// to sub-module instantiation so every node id stays circuit-global.
    pub(super) fn identity_mut(&mut self) -> &mut IdentityRegistry {
        &mut self.identity
    }

    /// Take the registry out of the builder (Phase C1 re-entry): handed into
    /// a lifted sub-builder and restored via [`Self::restore_identity`].
    pub(super) fn take_identity(&mut self) -> IdentityRegistry {
        std::mem::take(&mut self.identity)
    }

    /// Put the registry back after a lifted sub-builder freezes (Phase C1).
    pub(super) fn restore_identity(&mut self, registry: IdentityRegistry) {
        self.identity = registry;
    }

    /// Push a connection, tagging it with the current expansion id.
    pub(super) fn add_connection(&mut self, conn: ConnectionInst) {
        let mut conn = conn;
        // §5 same-name group fan-in (same-name-pin-group.md §6.3): a logical
        // slot point carries its physical pads (`spk{GND}` → [spk.3, spk.4]).
        // Expanding here — the single choke point every connection passes
        // through — puts every physical pad of the logical net into the same
        // connection net as its peers, so the pads are never left dangling and
        // merge naturally with direct `spk.3` references via union-find. Shape
        // checks and the §5 uniqueness warnings upstream already ran on the
        // logical points, so this only affects the physical net.
        if conn.points.iter().any(|p| !p.same_name_pads.is_empty()) {
            let mut pts = Vec::with_capacity(conn.points.len());
            for p in conn.points {
                if p.same_name_pads.is_empty() {
                    pts.push(p);
                } else {
                    pts.extend(p.same_name_pads.iter().cloned());
                }
            }
            // ConnectionInst::new already folded duplicate canonical paths
            // before the expansion; re-apply the same dedup so the pad fan-in
            // cannot re-introduce a repeated physical pad in one connection.
            conn.points = ConnectionInst::dedup_canonical(pts);
        }
        if conn.expansion_id.is_none() {
            conn.expansion_id = self.expansion.current_id();
        }
        self.connections.push(conn);
    }

    /// Current statement offset for provenance (top-level statement span start).
    /// Unified [`SourcePos`] (uri + byte offset, §7.11(3)); None when no
    /// statement is being processed.
    pub(super) fn current_call_site(&self) -> Option<SourcePos> {
        self.current_stmt_span
            .as_ref()
            .map(|s| SourcePos::new(self.def_uri.clone(), s.offset))
    }

    /// Function definition site (unified [`SourcePos`], §7.11(3)).
    pub(super) fn func_def_site(func_def: &McFunction) -> Option<SourcePos> {
        let uri = func_def.source_uri().cloned()?;
        let off = func_def.span.as_ref().map(|sp| sp.start as u32)?;
        Some(SourcePos::new(uri, off))
    }

    // ========================================================================
    // Instantiation top-level flow
    // ========================================================================

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
        if super::dump::dump_enabled() {
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
        if super::dump::dump_enabled() {
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
            // ── §3.3/§3.5: materialize the module func's standalone
            //    declarations (func.insts) BEFORE its body stmts, mirroring
            //    run_component_method (fcallinst.rs). Otherwise a module func
            //    `res[1:2]::RES(0)` + `res[1:2].Pullup([net,vcc])` dispatches
            //    the method call before res1/res2 exist → literal `res[1:2]`
            //    phantom + E3179 (§2.6 Table A). Empty prefix = module-level
            //    instances live directly in `self.components`. ──
            if let Err(e) = self.materialize_declared_subinstances(&func, "") {
                mcc_dbg!(
                    "inst::mod",
                    "[P2-4-AUTO] module '{}' func '{}' declared-subinstance materialization FAILED: {e}",
                    self.name,
                    func.name
                );
            }
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
        // Surface the error as a file:line diagnostic so it reaches the build
        // report and `summary.errors` — `InstDiagnostic` alone is only consumed
        // by mcviz metrics and module dumps, so otherwise the error is silent
        // (the enabler behind the periph.mc E4007 chain loss). Mirror the
        // `diagnostic_log_at` pattern from bus.rs:143.
        let (uri, pos) = match (&self.current_func_span, &self.current_stmt_span) {
            (Some(sp), _) => (sp.uri.clone(), sp.offset),
            (None, Some(s)) => (s.uri.clone(), s.offset),
            (None, None) => (self.def_uri.clone(), 0),
        };
        crate::db::diagnostic::diagnostic::diagnostic_log_at(
            code,
            crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
            uri,
            pos,
            1,
            &message,
            &[],
        );
        let name = self.name.clone();
        self.diagnostics
            .push(InstDiagnostic::error(code, &name, message));
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
        let name = self.name.clone();
        self.diagnostics
            .push(InstDiagnostic::warning(code, &name, message));
    }

    /// Merge diagnostics from a sub-module into the current module
    pub(super) fn merge_diagnostics_from(&mut self, child: &McModuleInst) {
        self.diagnostics.extend(child.diagnostics.iter().cloned());
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
                let module_name = self.name.clone();
                let counter = self.auto_inst_counter.entry(key.clone()).or_insert(0);
                *counter += 1;
                // Failure ledger (observation-only): an `.in`/`.out` access to a
                // component whose type declares no such pin is isolated into a
                // phantom instance (points.rs P7/P2 fix) — silently broken.
                ledger::record(
                    LedgerEntry::new(LedgerKind::Phantom, type_name.to_string(), module_name)
                        .with_action(LedgerAction::Silent),
                );
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
    ) -> Option<SourcePos> {
        let prev = self.current_func_span.clone();
        let uri = func.source_uri().cloned();
        self.current_func_span = match uri {
            Some(u) => {
                if let Some(off) = stmt_idx.and_then(|i| func.stmt_offsets.get(i)) {
                    Some(SourcePos::new(u.clone(), *off as u32))
                } else {
                    func.span
                        .as_ref()
                        .map(|sp| SourcePos::new(u.clone(), sp.start as u32))
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

    /// RAII: push the formal names of a function body being expanded and
    /// restore them on every exit path. The scope chain lets a nested call's
    /// vector-width check classify a bare actual as a passthrough variable
    /// (matching-rules-design.md §2.1 / P6) by scope alone — never by name
    /// content.
    pub(super) fn with_func_scope<R>(
        &mut self,
        bindings: &McParamBindings,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let names: HashSet<String> = bindings
            .iter()
            .filter_map(|b| b.declare.get_primary_name())
            .collect();
        self.func_scope.push(names);
        let r = f(self);
        self.func_scope.pop();
        r
    }

    /// P6 passthrough test: a bare actual is a passthrough variable when its
    /// name is a formal of the current function scope chain or of this module.
    /// Any other bare name is a definite scalar whose lane count rules.
    pub(super) fn is_passthrough_formal(&self, name: &str) -> bool {
        self.func_scope.iter().any(|s| s.contains(name))
            || self.params.iter().any(|b| {
                b.declare
                    .get_primary_name()
                    .map(|n| n == name)
                    .unwrap_or(false)
            })
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

    /// Byte offset of the current construction site for provenance (func-body
    /// line offset first, then the top-level statement span start, else 0).
    /// Consecutive constructions on the same source line are still
    /// distinguishable by offset (decision A, §7.1).
    pub(super) fn current_offset(&self) -> u32 {
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

        // ── Sub-module internal ground tie propagation ────────────────────
        // A raw per-module net only unions this module's own connections; it
        // cannot see a sub-module's internal port-to-port short. Mirror the
        // projection layer's mechanism (3) (viz/project.rs): if a sub-module
        // net carries >= 2 distinct boundary ground points, those port members
        // are one net inside the sub-module (e.g. modldo's SGM2019 pin 2 ties
        // `vin.GND ~ ldo.2 ~ vout.GND`), so their parent-scope paths
        // (`modldo.vin.GND`, `modldo.vout.GND`) must share a parent net too.
        // Without this, a shared-ground LDO would split V5V.GND / V3V3.GND at
        // this layer (only the projection layer used to re-merge them).
        // Only points already registered in the parent table are tied
        // (tie_paths skips unknown paths), matching the projection behavior.
        for sub in &self.sub_modules {
            let prefix = format!("{}.", sub.name);
            for (_, pts) in &sub.nets {
                let mut grounds: Vec<&str> = Vec::new();
                for p in pts {
                    // Boundary ground point = the sub-module's own port
                    // member / label (owner None), leaf classified as ground.
                    if p.owner.is_none() {
                        let leaf = p.path.rsplit('.').next().unwrap_or(&p.path);
                        if is_ground_name(leaf) && !grounds.contains(&p.path.as_str()) {
                            grounds.push(&p.path);
                        }
                    }
                }
                if grounds.len() >= 2 {
                    let parent_paths: Vec<String> =
                        grounds.iter().map(|g| format!("{prefix}{g}")).collect();
                    let refs: Vec<&str> = parent_paths.iter().map(|s| s.as_str()).collect();
                    table.tie_paths(&refs);
                }
            }
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

        // ── Ground net re-partition: GND by writing line + reference form ──
        self.split_ground_nets();

        // ── A net must have at least 2 points ──
        // A single node is not a net; drop any 1-point residue (e.g. a lone
        // ground label whose pins were all pulled into local GND groups).
        self.nets.retain(|(_, pts)| pts.len() >= 2);
    }

    // ========================================================================
    // Ground net re-partition (GND by writing line + reference form)
    // ========================================================================

    /// Exact ground-name leaf matcher (NOT the `starts_with` variant of
    /// [`super::mc_net::is_ground_name`] — `GND_OUT` / `VIN` etc. must not be
    /// treated as ground, otherwise the modldo split would be re-merged).
    fn is_ground_leaf(s: &str) -> bool {
        let leaf = s.rsplit('.').next().unwrap_or(s);
        matches!(
            leaf.to_uppercase().as_str(),
            "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GROUND" | "EARTH"
        )
    }

    /// Re-partition the module's ground nets into local ground groups.
    ///
    /// Applies ONLY to nets whose ground points are bare single-segment labels
    /// (`GND`, `AGND`, ...). Rule (user-confirmed, GENERAL — applies to every
    /// module):
    ///   - each statement line's bare GND defaults to an independent local ground net;
    ///   - a `component{...GND}` reference (the ground is drawn from the same
    ///     physical pin of the same component) merges the hanging 2-pin
    ///     passives' ground endpoints under a pin key `(component, gnd_pin)`;
    ///   - a bare label / bus-member ground (`GND`, `[X, GND]`, `(X, GND)`) is a
    ///     name reference only: each source statement forms its own group.
    ///
    /// Strict DC rail identity: a net carrying a rail-member ground (`va.GND`,
    /// `vin.GND`, `dc.GND` — multi-segment path) is a SINGLE rail identity and
    /// is never re-partitioned — different rails keep distinct grounds and each
    /// stays traceable to the module's DC rail, so re-partitioning would
    /// fragment real wiring.
    ///
    /// Every produced group is a net named `{base}@<component>` / `{base}@<line>`
    /// carrying its own local ground symbol, so all local grounds stay distinct
    /// (schematic convention). Port / label / sub-module-port points are never
    /// split — only local component pins hanging on a ground hang off the local
    /// ground symbols.
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
        // points are NOT hangings and stay in the central ground group. Owned
        // `String`s so the set does not borrow `self.components` across the
        // net re-partition loop below.
        let component_owners: std::collections::BTreeSet<String> =
            self.components.iter().map(|c| c.name.clone()).collect();

        // Indices of nets that contain a ground-name point. A net carrying a
        // rail-member ground (`va.GND`, `vin.GND`, `dc.GND` — multi-segment
        // path) is a single rail identity and is left untouched: strict DC
        // rail identity keeps each rail's ground distinct and traceable, so
        // re-partitioning it into per-line `@N` groups would fragment real
        // wiring (e.g. `vin.GND ~ ldo.2 ~ cap.2`). Only nets whose ground
        // points are bare single-segment labels (`GND`, `AGND`, ...) get the
        // per-line local-ground re-partition.
        let ground_net_idx: Vec<usize> = self
            .nets
            .iter()
            .enumerate()
            .filter(|(_, (_, pts))| {
                let has_ground = pts.iter().any(|p| Self::is_ground_leaf(&p.path));
                let has_rail_ground = pts
                    .iter()
                    .any(|p| p.path.contains('.') && Self::is_ground_leaf(&p.path));
                has_ground && !has_rail_ground
            })
            .map(|(i, _)| i)
            .collect();

        // Process in reverse so earlier indices stay valid while we remove/insert.
        for &idx in ground_net_idx.iter().rev() {
            let (name, points) = self.nets.remove(idx);
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
                    // Real source line number (byte offset -> line via the
                    // line index; guards are pushed during Pass2 by mcb_pass2).
                    let line = c.source_span.as_ref().map(|p| {
                        crate::db::infra::context::lookup_line_col(&p.uri, p.offset)
                            .map(|(l, _)| l)
                            .unwrap_or(0)
                    });
                    Some(Touch {
                        id: c.id,
                        line: line.unwrap_or(0),
                        label_grounds,
                        others,
                    })
                })
                .collect();

            if touches.is_empty() {
                self.nets.push((name, points));
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

            // src -> the source line where it is directly wired to a label
            // ground (e.g. `lp322dcdc.2` wired to `GND` on line 112). Passives
            // hung on the SAME line as that direct wiring belong to the central
            // group (one statement = one local ground, e.g. the USB socket
            // `(usbsock.5 + ... + usbsock.9) + TP3 -> vin.GND` is a single GND);
            // only passives hung from a DIFFERENT statement form a pin-key group.
            let mut src_line: HashMap<String, u32> = HashMap::new();
            for t in &touches {
                if t.label_grounds.is_empty() {
                    continue;
                }
                for o in &t.others {
                    if comp_sources.contains(o) && !src_line.contains_key(o) {
                        src_line.insert(o.clone(), t.line);
                    }
                }
            }

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
                            // Same statement as the source's own ground wiring:
                            // stays in the central group (no pin-key split).
                            let same_line =
                                src_line.get(src).copied() == Some(t.line) && t.line != 0;
                            if !same_line {
                                pin_groups.entry(src.clone()).or_default().push(np);
                            }
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

            // Rebuild the split nets. Every group carries its own local ground
            // symbol with a DISTINCT identity and name, so the InstTable can
            // resolve each to its own label (no shared "GND" node). The base
            // keeps the ORIGINAL net name so distinct system grounds stay
            // distinguishable and every local group is traceable to the system
            // ground it belongs to (`GND` / `AGND` / `PGND` / `vin.GND` ...):
            //   - central   : "<base>"            — the real ground sources
            //   - pin group : "<base>@<component>" — passives on one component
            //                                      ground pin (pin key)
            //   - line group: "<base>@<line>"      — passives on one label/bus
            //                                      ground statement
            let mut out: Vec<(String, Vec<NetPoint>)> = Vec::new();
            // Central group: only emit when it has >= 2 points. A lone ground
            // label (e.g. `GND` / `dc.GND` with all its pins pulled into local
            // groups) is a single node, not a net — it must not surface as a
            // `GND (1 pts) (stub)`.
            if central.len() >= 2 {
                central.sort_by(|a, b| a.path.cmp(&b.path));
                out.push((name.clone(), central));
            }
            for (src, mut np) in pin_groups {
                np.sort_by(|a, b| a.path.cmp(&b.path));
                if !np.is_empty() {
                    let gname = format!("{}@{}", name, owner_of(&src));
                    let gnd = NetPoint::new(&gname, IOType::None);
                    self.labels.insert(gname.clone(), gnd.clone());
                    let mut grp = vec![gnd];
                    grp.append(&mut np);
                    out.push((gname, grp));
                }
            }
            for ((line, id), mut np) in line_groups {
                np.sort_by(|a, b| a.path.cmp(&b.path));
                if !np.is_empty() {
                    let gname = if line >= 2 {
                        format!("{}@{}", name, line)
                    } else {
                        format!("{}@c{}", name, id)
                    };
                    let gnd = NetPoint::new(&gname, IOType::None);
                    self.labels.insert(gname.clone(), gnd.clone());
                    let mut grp = vec![gnd];
                    grp.append(&mut np);
                    out.push((gname, grp));
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

    // ========================================================================
    // Class-name resolution
    // ========================================================================

    /// P2-7-XTAL: strict full-name, case-sensitive class check.
    ///
    /// Replaces the old first-letter-uppercase heuristic. A component may
    /// define uppercase methods (`func Cap(...)`, `func Reset()`) that are
    /// NOT classes, so the only reliable "is this a class name?" test is an
    /// exact, case-sensitive match against the registered CMIE class table:
    ///   - `"Cap"`   → false (a method name; the class is "CAP")
    ///   - `"CAP"`   → true  (component class)
    ///   - `"DIO.ESD"` → true (dotted component class)
    /// No `to_uppercase()` normalization is applied, so a func named `Cap`
    /// can never be mistaken for the `CAP` class.
    pub(super) fn is_registered_class_name(name: &str) -> bool {
        let ids = McIds::from(name);
        let cur = current_uri::get();
        // Primary: resolve in the current file context. A name resolving to a
        // Component/Module/Interface is a class name.
        if matches!(
            crate::db::cmie::cmie::mcb_get_cmie(&ids, &cur),
            Some(McCMIE::Component(_)) | Some(McCMIE::Module(_)) | Some(McCMIE::Interface(_))
        ) {
            return true;
        }
        // Fallback: the current-context resolve may have hit an *enum* that
        // shares the name (e.g. `enum CAP` for capacitor dielectrics vs
        // `component CAP`). A class-construction inside a system-library
        // component method body (e.g. `CAP(cload)` in xtal.mc's `Setup`)
        // resolves against the *caller module's* context, where the enum can
        // shadow the component class. The global mcode class tables are
        // authoritative for "is this a registered component/module/interface
        // class name" (strict full-name, case-sensitive — A4).
        matches!(
            crate::db::resolve::policy::Resolver::resolve_system(&ids),
            Some(McCMIE::Component(_)) | Some(McCMIE::Module(_)) | Some(McCMIE::Interface(_))
        )
    }
}

// ============================================================================
// Counter resume (dianlu-tree refactor Phase B)
// ============================================================================

/// Rebuild the per-prefix auto-name counters from a (frozen) tree's component
/// names.
///
/// Phase B moved the counters into the builder, so a re-entered sub-module
/// resumes them from the locked auto-name pattern (`_C1` / `@_phantom_*_1` /
/// `@?*_1`) instead of carrying counter state in the model. Names that do not
/// match the pattern (user-written instance names, prefix-bearing nested
/// products) are skipped; a max-per-prefix merge keeps the counter exact even
/// when components were consumed by a failed construction and never pushed.
fn resume_auto_inst_counter(tree: &McModuleInst) -> HashMap<String, u32> {
    let mut counters: HashMap<String, u32> = HashMap::new();
    for comp in &tree.components {
        let name = comp.name.as_str();
        if !(name.starts_with('_') || name.starts_with('@')) {
            continue;
        }
        // Split the trailing counter: `_C1` -> key `_C`, value 1;
        // `@_phantom_CAP_1` / `@?CAP_1` -> key up to the last `_`, value 1.
        let Some((key, num)) = name.rsplit_once('_') else {
            continue;
        };
        let Ok(n) = num.parse::<u32>() else {
            continue;
        };
        let entry = counters.entry(key.to_string()).or_insert(0);
        if n > *entry {
            *entry = n;
        }
    }
    counters
}

/// Reload every node id a frozen tree already carries into `registry`
/// (Phase C1 re-entry / defensive lift): module root, its ports, its vectors,
/// its components, and its sub-modules recursively. Idempotent under
/// [`IdentityRegistry::resume`] — re-lifting the same tree keeps the same ids.
fn resume_tree(registry: &mut IdentityRegistry, path: &str, module: &McModuleInst) {
    if let Some(id) = module.node_id {
        registry.resume(path, id);
    }
    for port in &module.ports {
        if let Some(id) = port.node_id {
            registry.resume(&format!("{path}.{}", port.name), id);
        }
    }
    for vec in &module.vectors {
        if let Some(id) = vec.node_id {
            registry.resume(&format!("{path}.{}", vec.base), id);
        }
    }
    for comp in &module.components {
        if let Some(id) = comp.node_id {
            registry.resume(&format!("{path}.{}", comp.name), id);
        }
    }
    for sub in &module.sub_modules {
        let sub_path = format!("{path}.{}", sub.name);
        if let Some(id) = sub.node_id {
            registry.resume(&sub_path, id);
        }
        resume_tree(registry, &sub_path, sub);
    }
}
