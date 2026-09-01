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

mod builder;
mod bus;
mod dump;
mod expand;
mod fcallinst;
mod funccall;
pub(crate) mod group;
mod iterated;
mod matching;
mod phases;
mod points;
mod stmt;
mod subst;

pub(crate) use builder::InstantiationBuilder;

use super::mc_bus::McBusInst;
use super::mc_comp::McComponentInst;
use super::mc_net::{ConnectionInst, InstDiagLevel, InstDiagnostic, InstError, NetPoint, PortInst};
use crate::instant::identity::{IdentityRegistry, NodeId};
use crate::instant::net_store::NetTableStore;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::common::IOType;
use crate::semantic::module::McModule;
use crate::{current_uri, McURI};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
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

    /// Internal label registry (for implicit labels)
    pub(super) labels: HashMap<String, NetPoint>,

    /// Bus instance table (bus_name -> McBusInst)
    pub(super) buses: HashMap<String, McBusInst>,

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

    /// Component class names whose instantiation failed (any instance of this class).
    /// Used to skip stmts that reference failed components.
    pub(super) failed_classes: HashSet<String>,

    /// Structured failure records for known_missing.md (G4 baseline).
    pub(super) failed_records: Vec<FailedRecord>,

    /// Set of module-level function names that have been auto-invoked.
    /// Prevents double execution when a function is both auto-invoked and
    /// explicitly called from a parent module (e.g. `mcu.i2c()`).
    pub(super) auto_invoked_funcs: HashSet<String>,

    /// Expansion provenance: this module's expansion log (module-local id space).
    /// Products tagged with `expansion_id` index into `expansion.records`.
    pub expansion: crate::instant::provenance::ExpansionLog,

    /// Expansion provenance: index into the **parent** module's `ExpansionLog`
    /// when this sub-module was created by an expansion. None for the top-level
    /// module / module-level creations.
    pub expansion_id: Option<usize>,

    /// ★ §11.2: vector grouping nodes (declared vector instances).
    ///
    /// Grouping overlay over `components` — the physical member instances still
    /// live in `components` (existing consumption paths unchanged); each
    /// `McVectorInst` is the modeling-layer coordinate for an ordered member
    /// set (`c[1:2]` → base `"c"`, members `["c1","c2"]`). This is the
    /// instance-space counterpart of `McInstances.vectors` (pass1): the
    /// declaration no longer erases vector information.
    pub vectors: Vec<McVectorInst>,

    /// Phase C1: per-build node identity (canonical path interned in the
    /// circuit's `IdentityRegistry`). `None` until the module instance is
    /// added to its parent (`add_submodule`) or, for the top module, until
    /// it is interned as the circuit root; the frozen tree always carries it.
    pub node_id: Option<NodeId>,
}

/// §11.2: a declared vector instance — modeling-layer grouping node.
///
/// Physical member instances are ordinary `McComponentInst` in `components`;
/// this node groups them under the vector base name so lane broadcast /
/// member-set alignment (Phase 2 GAP1) and flatten projection (Phase 1.7
/// `vector_info`) can operate on the ordered member set. Contract E: only
/// multi-member ranges (`expanded.len() >= 2`) produce a node; single-member
/// ranges are scalars and stay out of `vectors`.
#[derive(Debug, Clone)]
pub struct McVectorInst {
    /// Vector base name — `"c"` for `c[1:2]` (declaration scope, no dotted prefix).
    pub base: String,
    /// Ordered member names from the declaration's member set — `["c1","c2"]`,
    /// member_set product (strict written order, never sorted). Nested
    /// combinations expand cartesian, row-major (§11.2 ordering contract).
    pub member_names: Vec<String>,
    /// Physical member instance coordinates — full instance names resolving
    /// against `self.components` (empty prefix → module-level `"c1"`; nested
    /// under a func invocation → `"U1.c1"`). Instance-tree nodes carry no ID
    /// today (§11.1: name + Rust reference); a per-build node ID is a Phase 3+
    /// option.
    pub member_ids: Vec<String>,
    /// Optional true 2D+ vector shape (rows × cols) for genuinely 2D declared
    /// vectors (`M[1:2][3:4]` = 8 same-type sub-instances). Real corpus has 0
    /// occurrences (§7.1) — spec-completeness item, always `None` today.
    pub shape: Option<Vec<usize>>,

    /// Phase C1: per-build node identity of the grouping node (canonical path
    /// `{module}.{base}` interned in the circuit's `IdentityRegistry`).
    /// `None` until `materialize_vector_groups` pushes the node; the frozen
    /// tree always carries it.
    pub node_id: Option<NodeId>,
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
            labels: HashMap::new(),
            buses: HashMap::new(),
            auto_inst_map: HashMap::new(),
            diagnostics: Vec::new(),
            bridge_passive_names: HashSet::new(),
            failed_classes: HashSet::new(),
            failed_records: Vec::new(),
            auto_invoked_funcs: HashSet::new(),
            expansion: crate::instant::provenance::ExpansionLog::default(),
            expansion_id: None,
            vectors: Vec::new(),
            node_id: None,
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
            labels: HashMap::new(),
            buses: HashMap::new(),
            auto_inst_map: HashMap::new(),
            diagnostics: Vec::new(),
            bridge_passive_names: HashSet::new(),
            failed_classes: HashSet::new(),
            failed_records: Vec::new(),
            auto_invoked_funcs: HashSet::new(),
            expansion: crate::instant::provenance::ExpansionLog::default(),
            expansion_id: None,
            vectors: Vec::new(),
            node_id: None,
        })
    }

    /// Execute instantiation (thin wrapper).
    ///
    /// Phase B (dianlu-tree refactor): the full construction flow lives on
    /// [`InstantiationBuilder`]; this entry lifts a plain tree into a builder,
    /// runs the flow, and freezes the result back into the model — the public
    /// instantiation entry keeps its signature and every call site stays
    /// unchanged.
    pub fn instantiate(&mut self) -> Result<(), InstError> {
        self.instantiate_with_store().map(|_| ())
    }

    /// Like [`Self::instantiate`], but returns the circuit-wide frozen string
    /// net-table store (Phase D) the flow produced. The store is the only
    /// carrier of the per-module string net tables after the tree freezes —
    /// `McModuleInst` no longer stores `NetPoint`, so the caller (the
    /// `DianLu` / the projection) takes the store out of the build.
    pub(crate) fn instantiate_with_store(
        &mut self,
    ) -> Result<Rc<RefCell<NetTableStore>>, InstError> {
        // Clone the identity fields first: `replace` needs its new value while
        // `self` is still mutably borrowed, so the placeholder tree cannot read
        // through `self`. The placeholder is discarded — `tree` (the actual
        // current state, `with_params` bindings included) drives the flow.
        let name = self.name.clone();
        let def = self.def.clone();
        let tree = std::mem::replace(self, Self::new(&name, def));
        let mut builder = InstantiationBuilder::new(tree);
        let result = builder.instantiate();
        let store = builder.net_store();
        *self = builder.finish();
        result.map(|()| store)
    }

    /// Like [`Self::instantiate`], but interns every product into the
    /// caller's per-build [`IdentityRegistry`] under `current_path` (Phase
    /// C1). Sub-module instantiation goes through here so node ids stay
    /// circuit-global; the registry is handed back through `identity` after
    /// the flow freezes the tree. The caller's shared circuit-wide net-table
    /// store is handed down (Phase D) so this module's frozen table lands in
    /// the same store the parent reads for ground-tie propagation.
    pub(crate) fn instantiate_in_scope(
        &mut self,
        identity: &mut IdentityRegistry,
        current_path: &str,
        net_store: Rc<RefCell<NetTableStore>>,
    ) -> Result<(), InstError> {
        let name = self.name.clone();
        let def = self.def.clone();
        let tree = std::mem::replace(self, Self::new(&name, def));
        let mut builder = InstantiationBuilder::with_identity(
            tree,
            std::mem::take(identity),
            current_path.to_string(),
            net_store,
        );
        let result = builder.instantiate();
        let (frozen, reg) = builder.into_parts();
        *identity = reg;
        *self = frozen;
        result
    }

    // ========================================================================
    // Diagnostic queries (finished-tree inspection)
    // ========================================================================

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

    // ========================================================================
    // Ground identity helpers
    // ========================================================================

    /// Strict DC rail identity: the ground member point owned by a rail scalar.
    /// Delegates to the centralized [`matching::rail_ground_point`]
    /// (dc-rail-identity-design.md / matching-rules-design.md §5).
    pub(super) fn rail_ground_point(&self, rail: &NetPoint, gnd_member: &str) -> NetPoint {
        matching::rail_ground_point(rail, gnd_member)
    }

    /// Is `name` a structurally valid reference to one of this module's ports
    /// (exact port name, bare member of a bus port, or `port.member` against
    /// the member group)? Pure read on the finished tree — consulted by
    /// `check_unconnected_module_ports` on frozen sub-modules (phases.rs).
    pub(super) fn is_valid_port_ref(&self, name: &str) -> bool {
        // Pin-id artifact (func expansion): numeric-only paths never name a port.
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }

        // Curly-named ports are registered with the brace suffix in the name
        // (e.g. "USB_VBUS_1{VDD_3V, GND}"); the base matches member paths.
        let base_matches = |candidate: &str, target: &str| -> bool {
            let base = brace_suffix_strip(candidate);
            candidate == target || (!base.is_empty() && base == target)
        };

        // 1. Exact port name (covers square forms like "[VDD_3V3, GND]").
        if self.ports.iter().any(|p| base_matches(&p.name, name)) {
            return true;
        }
        // 2. Bare member of a bus port.
        if self
            .ports
            .iter()
            .any(|p| p.bus_members.iter().any(|m| m == name))
        {
            return true;
        }
        // 3. Dotted member: port.member.
        if let Some((port, member)) = name.split_once('.') {
            return self
                .ports
                .iter()
                .any(|p| base_matches(&p.name, port) && p.bus_members.iter().any(|m| m == member));
        }
        false
    }
}

/// Strip a trailing `{...}` / `[...]` member suffix from a port name to get
/// its base identifier (e.g. "USB_VBUS_1{VDD_3V, GND}" → "USB_VBUS_1").
/// Square-only names like "[VDD_3V3, GND]" strip to "" and keep the exact
/// form for matching.
fn brace_suffix_strip(s: &str) -> &str {
    let cut = match (s.find('{'), s.find('[')) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    match cut {
        Some(i) => &s[..i],
        None => s,
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

        if !self.diagnostics.is_empty() {
            writeln!(f, "  Diagnostics ({}):", self.diagnostics.len())?;
            for diag in &self.diagnostics {
                writeln!(f, "    - {diag}")?;
            }
        }

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// P0.5 golden lock (dianlu-tree refactor Phase B): the auto-naming
    /// sequences produced by `auto_name` from the position counters, locked
    /// before the counters move out of `McModuleInst` into
    /// `InstantiationBuilder`. The move must be a pure relocation with zero
    /// observable naming change.
    ///
    /// Locks, per `AutoNameKind`:
    /// - Normal: `_<ref-designator-prefix><counter>` — per-prefix counter
    ///   (`_C1`, `_C2`, then `_R1` for a new prefix).
    /// - Phantom: `@_phantom_<type>_<n>` — per-type counter (isolation for
    ///   leaked `<inst>.in`/`<inst>.out` placeholders).
    /// - Stub: `@?<type>_<n>` — per-type counter (unrecognized-class fallback).
    ///
    /// The three kinds share one counter map keyed differently, so each kind's
    /// counter must be isolated from the others (cross-kind independence).
    #[test]
    fn auto_name_sequence_lock() {
        // P0.5 lock: drive `auto_name` through the builder (the counters moved
        // out of `McModuleInst` into `InstantiationBuilder` with the tree still
        // reachable via Deref) — the sequences below must stay byte-identical.
        let mut inst = InstantiationBuilder::new(McModuleInst::new(
            "main",
            Arc::new(McModule::test_stub("main")),
        ));

        // Normal: sequential per-ref-designator-prefix counters.
        assert_eq!(inst.auto_name(AutoNameKind::Normal, "CAP").0, "_C1");
        assert_eq!(inst.auto_name(AutoNameKind::Normal, "CAP").0, "_C2");
        assert_eq!(inst.auto_name(AutoNameKind::Normal, "RES").0, "_R1");

        // Phantom: per-type counters with the raw type name in the key.
        assert_eq!(
            inst.auto_name(AutoNameKind::Phantom, "CAP").0,
            "@_phantom_CAP_1"
        );
        assert_eq!(
            inst.auto_name(AutoNameKind::Phantom, "CAP").0,
            "@_phantom_CAP_2"
        );

        // Stub: per-type counters with the raw type name in the key.
        assert_eq!(inst.auto_name(AutoNameKind::Stub, "CAP").0, "@?CAP_1");
        assert_eq!(inst.auto_name(AutoNameKind::Stub, "CAP").0, "@?CAP_2");

        // Cross-kind independence: phantom/stub calls never advance the Normal
        // counter, and different types never share a Normal counter.
        assert_eq!(inst.auto_name(AutoNameKind::Normal, "CAP").0, "_C3");
        assert_eq!(inst.auto_name(AutoNameKind::Normal, "RES").0, "_R2");

        // Dotted class names keep their dots inside phantom/stub keys (the
        // prefix / counter split happens at the key, not on the type name).
        assert_eq!(
            inst.auto_name(AutoNameKind::Phantom, "DIO.ESD").0,
            "@_phantom_DIO.ESD_1"
        );
        assert_eq!(
            inst.auto_name(AutoNameKind::Stub, "DIO.ESD").0,
            "@?DIO.ESD_1"
        );

        // Returned byte offsets: Normal carries the construction-site offset
        // (0 here, no span context set), phantom/stub always 0.
        assert_eq!(inst.auto_name(AutoNameKind::Normal, "CAP").1, 0);
        assert_eq!(inst.auto_name(AutoNameKind::Phantom, "CAP").1, 0);
        assert_eq!(inst.auto_name(AutoNameKind::Stub, "CAP").1, 0);
    }
}
