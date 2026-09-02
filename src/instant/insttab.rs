// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Flattened Instance Table
//!
//! Flattens the McModuleInst tree structure into a one-dimensional table,
//! where every instance (module, component, pin, port, bus, label) has a
//! unique ID and a complete hierarchical path.
//!
//! ## Usage
//! ```ignore
//! let table = InstTable::from_module_inst(&module_inst, 1000);
//! table.dump();
//! ```

use super::arena::NodeArena;
use super::inststore::{InstanceStore, TreeView};
use super::mc_bus::McBusInst;
use super::mc_mod::McModuleInst;
use super::mc_net::NetPoint;
use crate::instant::nettab::NetTableStore;
use crate::semantic::common::IOType;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

// ============================================================================
// InstKind - Instance entry type
// ============================================================================

/// Instance entry type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstKind {
    /// Module instance (including top-level)
    Module,
    /// Component instance (resistor, capacitor, IC, etc.)
    Component,
    /// Component pin
    Pin,
    /// Module port (in/out/inout)
    Port,
    /// Bus (e.g. power{VCC, GND})
    Bus,
    /// Label (standalone label / bus member)
    Label,
}

impl std::fmt::Display for InstKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstKind::Module => write!(f, "Module"),
            InstKind::Component => write!(f, "Component"),
            InstKind::Pin => write!(f, "Pin"),
            InstKind::Port => write!(f, "Port"),
            InstKind::Bus => write!(f, "Bus"),
            InstKind::Label => write!(f, "Label"),
        }
    }
}

impl InstKind {
    /// Registration priority — used to arbitrate when two different kinds
    /// compete for the same path.
    ///
    /// Structural entities (`Module` / `Component` / `Pin`) are real physical
    /// hierarchy nodes in the circuit, with priority over "net-side projections"
    /// (`Port` / `Bus` / `Label`). The latter are often just aliases/endpoints
    /// of some structural entity in the net namespace; when they collide with
    /// a structural entity on path, the structural entity should win.
    ///
    /// See the dedup arbitration logic in `InstTable::register`.
    fn registration_priority(&self) -> u8 {
        match self {
            InstKind::Module | InstKind::Component | InstKind::Pin => 2,
            InstKind::Port | InstKind::Bus | InstKind::Label => 1,
        }
    }
}

// ============================================================================
// MemberRole / MemberInfo — pin role for net merging and rail checks
// ============================================================================

/// Electrical role of a pin / interface member
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberRole {
    Power,
    Ground,
    Signal,
}

impl std::fmt::Display for MemberRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemberRole::Power => write!(f, "Power"),
            MemberRole::Ground => write!(f, "Ground"),
            MemberRole::Signal => write!(f, "Signal"),
        }
    }
}

/// Voltage value extracted from interface params (e.g. `DC(3.3V)` → 3.3 V)
#[derive(Debug, Clone, PartialEq)]
pub struct Volt {
    pub value: f64,
}

impl std::fmt::Display for Volt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}V", self.value)
    }
}

/// Role + optional voltage for a pin / interface member
#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub role: MemberRole,
    pub voltage: Option<Volt>,
}

impl MemberInfo {
    pub fn new(role: MemberRole, voltage: Option<Volt>) -> Self {
        Self { role, voltage }
    }
}

// ============================================================================
// VectorMemberInfo — vector group projection (design §11.1)
// ============================================================================

/// Vector member projection (design §11.1): which declared vector group
/// (`c[1:2]`) a flattened component entry belongs to, and its position within
/// the ordered member set.
///
/// Projection-only — the flat path stays `main.c1` (invariant B); consumers
/// (LSP, export, GAP1) use this field to reverse-query the vector structure.
/// Distinct from [`MemberInfo`] (interface member role/voltage), which carries
/// unrelated semantics and is consumed by interface / power-pin checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorMemberInfo {
    /// Vector base name — `"c"` for `c[1:2]` (declaration scope).
    pub vector_base: String,
    /// Member name within the group, e.g. `"c2"` (member_set product).
    pub member: String,
    /// Zero-based index in the ordered member set (c1 → 0, c2 → 1).
    pub index: usize,
}

impl VectorMemberInfo {
    pub fn new(vector_base: String, member: String, index: usize) -> Self {
        Self {
            vector_base,
            member,
            index,
        }
    }
}

/// Infer MemberRole from IOType and leaf name.
///
/// Returns `(role, inferred)` where `inferred == true` means the role was
/// determined by name heuristic rather than explicit qualifier.
pub fn infer_member_role(
    leaf_name: &str,
    io_type: &IOType,
    is_ground: fn(&str) -> bool,
    is_supply: fn(&str) -> bool,
) -> (MemberRole, bool) {
    // (a) explicit qualifier: ps → Power
    if matches!(io_type, IOType::Power) {
        return (MemberRole::Power, false);
    }
    // (b) fallback heuristic: name-based
    if is_ground(leaf_name) {
        return (MemberRole::Ground, true);
    }
    if is_supply(leaf_name) {
        return (MemberRole::Power, true);
    }
    // (c) default
    (MemberRole::Signal, false)
}

/// Check if a name looks like Ground.
pub(crate) fn is_ground_name(s: &str) -> bool {
    let u = s.to_uppercase();
    matches!(
        u.as_str(),
        "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GROUND" | "EARTH"
    )
}

/// Check if a name looks like Power (not Ground).
pub(crate) fn is_supply_name(s: &str) -> bool {
    let u = s.to_uppercase();
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
    let bytes = u.as_bytes();
    let digits = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    if u.contains('V') && digits >= 1 && u.len() <= 8 {
        if !u.starts_with("VO") {
            return true;
        }
    }
    false
}

// ============================================================================
// InstOrigin
// ============================================================================

/// ★ M0-B-E: instance origin —— whether the device came from a declaration or a funcall
#[derive(Debug, Clone)]
pub enum InstOrigin {
    /// Instance produced by a declaration (`RES R1(10kΩ)` etc.)
    Declared,
    /// Instance generated by a function call (`.Cap()` / `.Pullup()` / `.ESD()` etc.)
    FuncCall {
        fn_name: String,
        /// Byte offset of the construction site in the owning file
        /// (decision A, §7.1; 0 = unknown). Convert to a line for display.
        line: u32,
        /// Back-link to the expansion record that produced this instance
        /// (§7.4). Not part of `PartialEq` — provenance only, does not
        /// change semantic comparison (verify / golden depend on that).
        expansion_id: Option<usize>,
    },
}

impl PartialEq for InstOrigin {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (InstOrigin::Declared, InstOrigin::Declared) => true,
            (
                InstOrigin::FuncCall {
                    fn_name: a,
                    line: la,
                    ..
                },
                InstOrigin::FuncCall {
                    fn_name: b,
                    line: lb,
                    ..
                },
            ) => a == b && la == lb,
            _ => false,
        }
    }
}

impl Eq for InstOrigin {}

impl Default for InstOrigin {
    fn default() -> Self {
        InstOrigin::Declared
    }
}

// ============================================================================
// InstEntry - Single instance record
// ============================================================================

/// Single instance record
#[derive(Debug, Clone)]
pub struct InstEntry {
    /// Globally unique ID
    pub id: u32,
    /// Full hierarchical path: "main.submod1.res102.1"
    pub path: String,
    /// Instance type
    pub kind: InstKind,
    /// Parent instance ID (None for top-level module)
    pub parent_id: Option<u32>,
    /// Definition class name: "Res", "comp.sub", "power_domain" (empty string for pin/port/label)
    pub class_name: String,
    /// IO type (only meaningful for Port/Pin, otherwise IOType::None)
    pub io_type: IOType,
    /// Unified source position in the definition file (from NetPoint / AST)
    pub src_pos: Option<crate::semantic::common::SourcePos>,
    /// Coarse fallback position for diagnostics: where the entity was *declared*
    /// (e.g. a component pin's pin-id in the component body). Used only when
    /// `src_pos` is None (unconnected pins/ports have no wiring site, so the
    /// declaration site is the best anchor). `src_pos` (wiring site) always wins.
    pub fallback_pos: Option<crate::semantic::common::SourcePos>,
    /// URI of the file where this instance was defined
    pub def_uri: String,
    /// ★ Member role and voltage (for interface members / power pins)
    pub member_info: Option<MemberInfo>,
    /// ★ §11.1: vector member projection — the declared vector group this
    /// flattened component entry belongs to (None for scalar / non-vector).
    /// Populated during `flatten_module` from the modeling-layer `vectors`
    /// groups; consumers reverse-query the vector structure via
    /// [`InstTable::vector_member_paths`].
    pub vector_info: Option<VectorMemberInfo>,
    /// ★ M0-B-D: not-fitted marker (from McComponentInst.nc)
    pub not_fitted: bool,
    /// ★ M0-B-E: instance origin (declaration vs funcall)
    pub origin: InstOrigin,
    /// ★ virtual: true when this entry belongs to a synthetic wrapper module
    /// generated by virtual instantiation (`module VIRT_<T> { T u_1 }`).
    /// Set from the generation site (build/vinst), never inferred from the
    /// `VIRT_`/`u_1` names, so build / diagnostic layers can distinguish
    /// synthetic wrappers from real user modules and instances.
    pub synthetic: bool,
}

// ============================================================================
// NetEntry - Network record
// ============================================================================

/// Network record, representing an electrical network after flattening
///
/// Each network connects several `InstEntry`s (pins, ports, labels, etc.),
/// referenced by their IDs in `points`.
///
/// ## Example
/// ```text
/// net "VCC" (#5001): [#1003(main.VCC), #1007(main.R1.1), #1012(main.R2.1)]
/// net "GND" (#5002): [#1004(main.GND), #1008(main.R1.2)]
/// ```
#[derive(Debug, Clone)]
pub struct NetEntry {
    /// Network unique ID
    pub id: u32,
    /// Network name (port name > label name > anonymous `_net{N}`)
    pub name: String,
    /// InstEntry IDs of all endpoints belonging to this network
    pub points: Vec<u32>,
}

// ============================================================================
// InstTable - Flattened instance table
// ============================================================================

/// Flattened instance table
///
/// Flattens the nested McModuleInst tree into a one-dimensional ID → entry
/// mapping, while maintaining a path → ID index for fast lookup.
/// Contains network information and can be directly consumed by the drawing side.
#[derive(Debug)]
pub struct InstTable {
    /// Next available ID
    next_id: u32,
    /// id -> entry (ordered by ID)
    entries: BTreeMap<u32, InstEntry>,
    /// path -> id (fast lookup)
    path_index: HashMap<String, u32>,

    /// Network ID counter
    net_id_counter: u32,
    /// net_id -> NetEntry (ordered by ID)
    nets: BTreeMap<u32, NetEntry>,
    /// point_id -> net_id (reverse index from endpoint to network)
    point_to_net: HashMap<u32, u32>,

    /// ★ M11.3: full paths of bridge passive components (Transposed 2-pin devices)
    bridge_passive_paths: HashSet<String>,

    /// Phase D: the circuit-wide frozen string net-table store, keyed by
    /// canonical module path. `flatten_nets` sources each module's table from
    /// here (`McModuleInst` never carries `NetPoint`); consumers that need the
    /// tree-level string nets (export netlist, viz ground override) read
    /// through [`Self::net_table`].
    net_table: Rc<RefCell<NetTableStore>>,
}

impl InstTable {
    /// Create a new instance table, specifying the starting ID
    pub fn new(start_id: u32) -> Self {
        Self {
            next_id: start_id,
            entries: BTreeMap::new(),
            path_index: HashMap::new(),
            net_id_counter: start_id + 100_000, // Network ID and instance ID use separate number spaces
            nets: BTreeMap::new(),
            point_to_net: HashMap::new(),
            bridge_passive_paths: HashSet::new(),
            net_table: Rc::new(RefCell::new(NetTableStore::new())),
        }
    }

    /// The circuit-wide frozen string net-table store (Phase D). Tree-level
    /// string-net consumers that hold a flat table read per-module tables
    /// here, keyed by canonical module path.
    pub fn net_table(&self) -> Rc<RefCell<NetTableStore>> {
        self.net_table.clone()
    }

    /// Recursively generate flattened instance table from McModuleInst tree.
    ///
    /// `view` (Phase C S3: arena edges + instance store) drives the traversal:
    /// sub-module order follows the arena `children` edges and the content
    /// resolves from the store (design §4 — the tree is a view over arena
    /// edges) instead of the tree's (now-removed) recursive `sub_modules` Vec.
    /// Callers hold the owning build's arena + store and construct the view
    /// (`mcc build` net-check path, `DianLu` flatten projection via
    /// [`Self::from_module_inst_with_arena`]).
    ///
    /// `net_store` (Phase D) carries the circuit-wide frozen string net tables
    /// produced during construction — the tree no longer stores `NetPoint`, so
    /// the projection sources each module's table from here.
    pub fn from_module_inst(
        inst: &McModuleInst,
        start_id: u32,
        net_store: Rc<RefCell<NetTableStore>>,
        view: &TreeView,
    ) -> Self {
        let mut table = InstTable::new(start_id);
        table.net_table = net_store;
        table.flatten_module(inst, "", None, view);
        // NOTE: no global ground merge here (strict DC rail identity). Ground
        // nets stay exactly as wired: each DC rail keeps its own ground
        // (`V5V.GND` != `V3V3.GND`) and grounds merge only through real wiring
        // ties (shared component ground pins, explicit `X.GND -> GND`).
        table
    }

    /// Recursively generate flattened instance table, with the Phase C arena
    /// + instance store driving the traversal. Thin wrapper over
    /// [`Self::from_module_inst`] that builds the view from the owning build's
    /// arena + store.
    pub(crate) fn from_module_inst_with_arena(
        inst: &McModuleInst,
        start_id: u32,
        arena: &NodeArena,
        store: &InstanceStore,
        net_store: Rc<RefCell<NetTableStore>>,
    ) -> Self {
        let view = TreeView::new(arena, store);
        Self::from_module_inst(inst, start_id, net_store, &view)
    }

    /// Register an instance, return the allocated ID
    ///
    /// If the path is already registered:
    /// - New and old kinds are the same → directly reuse the existing ID
    ///   (normal dedup, silent).
    /// - New and old kinds differ → arbitrate per [`InstKind::registration_priority`]:
    ///   * New kind priority is **higher** (structural entity seizes a path
    ///     previously occupied by the net side)
    ///     → **in-place upgrade** the entry (replace kind / parent_id /
    ///     class_name / io_type; the ID remains unchanged, and the established
    ///     path_index and parent references remain valid).
    ///   * Otherwise keep the old entry and discard this registration.
    ///
    /// When BOTH the existing and the new registration are structural
    /// (Module/Component/Pin) with different declaration classes, the collision
    /// is reported as GAP3 (E4062 PIN_OCCUPIED_BY_DECLARATION) — two different
    /// declarations materialized to the same physical pin id (see the check
    /// body below for the domain split vs E5151 / 4051 / 4053).
    pub fn register(
        &mut self,
        path: String,
        kind: InstKind,
        parent_id: Option<u32>,
        class_name: String,
        io_type: IOType,
        src_pos: Option<crate::semantic::common::SourcePos>,
        def_uri: String,
    ) -> u32 {
        // Prevent duplicate registration
        if let Some(&existing_id) = self.path_index.get(&path) {
            let existing_kind = self.entries.get(&existing_id).map(|e| e.kind.clone());

            if let Some(existing_kind) = existing_kind {
                // ── GAP3 (E4062 PIN_OCCUPIED_BY_DECLARATION) ─────────────────
                // "Two different declarations materialize to the same physical
                // pin id" (design §9.3.3 / vector-pipeline §2.3). Fires only when
                // BOTH registrations are structural entities (Module/Component/
                // Pin) AND their declaration classes differ — a flat-layer
                // physical-position preemption the declaration layer cannot see.
                // Every valid-syntax trigger is absorbed by the pass1
                // declaration layer (E5151 same-scope instance names, `insts`
                // name-keyed dedup), so this is dormant-by-construction for
                // well-formed MCode — it converts the silent merge into an
                // error should a collision ever reach flatten. The domain split:
                // GAP3 = pin DECLARATION occupancy (here), E5151 = same-scope
                // instance names (pass1), 4051 = per-connection net merge
                // (build side, visit.rs), 4053 = bus pin-group monotonicity
                // (pass1, instref.rs) — mutually exclusive, no double-report.
                let existing_class = self
                    .entries
                    .get(&existing_id)
                    .map(|e| e.class_name.clone())
                    .unwrap_or_default();
                if kind.registration_priority() == 2
                    && existing_kind.registration_priority() == 2
                    && !existing_class.is_empty()
                    && existing_class != class_name
                {
                    crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::PIN_OCCUPIED_BY_DECLARATION,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        src_pos.as_ref().map(|s| s.offset).unwrap_or(0),
                        1,
                        &crate::errcodes::format_msg(
                            crate::errcodes::PIN_OCCUPIED_BY_DECLARATION,
                            &[
                                &path as &dyn std::fmt::Display,
                                &existing_class as &dyn std::fmt::Display,
                                &class_name as &dyn std::fmt::Display,
                            ],
                        ),
                        &[],
                    );
                }
                if existing_kind != kind {
                    let new_pri = kind.registration_priority();
                    let old_pri = existing_kind.registration_priority();

                    if new_pri > old_pri {
                        // Structural entity (Component/Module/Pin) reclaims a
                        // path previously occupied by net side (Port/Bus/Label)
                        // —— in-place upgrade.
                        if let Some(entry) = self.entries.get_mut(&existing_id) {
                            entry.kind = kind;
                            entry.parent_id = parent_id;
                            entry.class_name = class_name;
                            entry.io_type = io_type;
                            if src_pos.is_some() {
                                entry.src_pos = src_pos;
                            }
                            if !def_uri.is_empty() {
                                entry.def_uri = def_uri;
                            }
                        }
                    } else {
                        // Old kind priority >= new kind —— keep the old entry.
                    }
                } else {
                    // Same kind: update io_type if the new one is more specific.
                    // This handles cases like: first registered as Bus with io_type=None,
                    // later registered as Bus with io_type=InOut (from port declaration).
                    if let Some(entry) = self.entries.get_mut(&existing_id) {
                        let needs_update = match (&entry.io_type, &io_type) {
                            // Update if current is None/Unknown and new is more specific
                            (IOType::None, _) if !matches!(io_type, IOType::None) => true,
                            // Update parent_id if current is None
                            (IOType::None, _)
                                if entry.parent_id.is_none() && parent_id.is_some() =>
                            {
                                true
                            }
                            _ => false,
                        };
                        if needs_update {
                            entry.io_type = io_type;
                            if entry.parent_id.is_none() {
                                entry.parent_id = parent_id;
                            }
                        }
                        // Always update src_pos/def_uri if the new ones are more specific
                        if src_pos.is_some() && entry.src_pos.is_none() {
                            entry.src_pos = src_pos;
                        }
                        if !def_uri.is_empty() && entry.def_uri.is_empty() {
                            entry.def_uri = def_uri;
                        }
                    }
                }
            }
            return existing_id;
        }

        let id = self.next_id;
        self.next_id += 1;

        let entry = InstEntry {
            id,
            path: path.clone(),
            kind,
            parent_id,
            class_name,
            io_type,
            src_pos,
            fallback_pos: None,
            def_uri,
            member_info: None,
            vector_info: None,
            not_fitted: false,
            origin: InstOrigin::Declared,
            synthetic: false,
        };

        self.entries.insert(id, entry);
        self.path_index.insert(path, id);
        id
    }

    /// Set member_info for an entry by ID.
    pub fn set_member_info(&mut self, id: u32, member_info: MemberInfo) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.member_info = Some(member_info);
        }
    }

    /// §11.1: attach the vector-group projection to a flattened component
    /// entry. Called from `flatten_module` when the entry is a member of a
    /// declared vector group.
    pub fn set_vector_info(&mut self, id: u32, vector_info: VectorMemberInfo) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.vector_info = Some(vector_info);
        }
    }

    /// §11.1: reverse-query — all flattened paths whose entries belong to the
    /// declared vector group `base`, in member order. The forward projection
    /// (vector_info) is built at flatten time; the reverse index is a
    /// low-frequency O(n) scan here (vector member counts are small, and
    /// consumers such as LSP query `c[1:2]` only on demand).
    pub fn vector_member_paths(&self, base: &str) -> Vec<String> {
        let mut items: Vec<(usize, &str)> = self
            .entries
            .values()
            .filter_map(|e| {
                e.vector_info
                    .as_ref()
                    .filter(|vi| vi.vector_base == base)
                    .map(|vi| (vi.index, e.path.as_str()))
            })
            .collect();
        items.sort_by_key(|(idx, _)| *idx);
        items.into_iter().map(|(_, p)| p.to_string()).collect()
    }

    /// Mark every entry whose path is exactly `prefix` or starts with
    /// `prefix + "."` as a synthetic virtual-instantiation wrapper.
    ///
    /// Called by `virtual_build_flat` with the generated wrapper module name,
    /// so the synthetic marker is attached at the generation site and carried
    /// into the build (and from there into the viz graph), never inferred by
    /// matching the `VIRT_` / `u_1` names downstream.
    pub fn mark_synthetic_by_path_prefix(&mut self, prefix: &str) {
        let scope = format!("{prefix}.");
        for (_id, entry) in self.entries.iter_mut() {
            if entry.path == prefix || entry.path.starts_with(&scope) {
                entry.synthetic = true;
            }
        }
    }

    /// Convenience wrapper for tests — calls `register` with empty source info.
    #[cfg(test)]
    pub fn register_simple(
        &mut self,
        path: String,
        kind: InstKind,
        parent_id: Option<u32>,
        class_name: String,
        io_type: IOType,
    ) -> u32 {
        self.register(
            path,
            kind,
            parent_id,
            class_name,
            io_type,
            None,
            String::new(),
        )
    }

    // ====================================================================
    // Query methods
    // ====================================================================

    /// Find ID by path
    pub fn get_id_by_path(&self, path: &str) -> Option<u32> {
        self.path_index.get(path).copied()
    }

    /// Find entry by ID
    pub fn get_entry(&self, id: u32) -> Option<&InstEntry> {
        self.entries.get(&id)
    }

    /// Get all direct child instances under a given parent node
    pub fn children_of(&self, parent_id: u32) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.parent_id == Some(parent_id))
            .collect()
    }

    /// Iterate all entries (ordered by ID)
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &InstEntry)> {
        self.entries.iter()
    }

    /// Return total entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// ★ M11.3: check whether a component path is a bridge passive (Transposed 2-pin device)
    pub fn is_bridge_passive(&self, path: &str) -> bool {
        self.bridge_passive_paths.contains(path)
    }

    // ====================================================================
    // Network query methods
    // ====================================================================

    /// Get all networks
    pub fn get_nets(&self) -> Vec<&NetEntry> {
        self.nets.values().collect()
    }

    /// Find network by ID
    pub fn get_net(&self, net_id: u32) -> Option<&NetEntry> {
        self.nets.get(&net_id)
    }

    /// Find the network a given endpoint belongs to
    pub fn get_net_of(&self, point_id: u32) -> Option<&NetEntry> {
        self.point_to_net
            .get(&point_id)
            .and_then(|net_id| self.nets.get(net_id))
    }

    /// Get all component entries
    pub fn get_components(&self) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.kind == InstKind::Component)
            .collect()
    }

    /// Get all module entries
    pub fn get_modules(&self) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.kind == InstKind::Module)
            .collect()
    }

    /// Get all pins of a component
    pub fn get_pins_of(&self, comp_id: u32) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.parent_id == Some(comp_id) && e.kind == InstKind::Pin)
            .collect()
    }

    /// Get all ports of a module
    pub fn get_ports_of(&self, mod_id: u32) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.parent_id == Some(mod_id) && e.kind == InstKind::Port)
            .collect()
    }

    /// Return total network count
    pub fn net_count(&self) -> usize {
        self.nets.len()
    }

    // ====================================================================
    // Ground net merging — REMOVED (strict DC rail identity)
    // ====================================================================
    //
    // `merge_ground_nets` (global merge of every `MemberRole::Ground` net into a
    // single "GND" net) has been deleted. Under strict DC rail identity, a
    // module's different DC rails may carry different grounds: `va.GND` and
    // `vb.GND` are distinct nets, and each stays traceable to its rail. Grounds
    // merge only through real wiring ties (shared component ground pins,
    // explicit `X.GND -> GND` connections), which the endpoint union-find in
    // mc_net / visit handles naturally.
    //
    // `MemberInfo`/`MemberRole` are still set on ports/pins below: the viz
    // projection layer (project.rs) and the graph builder (fromblock.rs) use
    // the role to classify rail vs signal endpoints and to extract voltages.
    // Only the global ground merge is gone.

    // ====================================================================
    // flatten traversal (Step 5)
    // ====================================================================

    /// Declaration-site fallback for unconnected ports (AGENTS.md: "the module
    /// span for ports"). A port that is never wired (e.g. E4117 floating
    /// bidirectional) has no net point to back-fill a wiring site into
    /// `src_pos`, so net checks would anchor at offset 0 → file:1:1; anchor
    /// them at the port's declaration span in the module body instead.
    /// `src_pos` (a real wiring site) always wins.
    fn backfill_port_decl_pos(&mut self, id: u32, def_uri: &str, span: Option<Range<usize>>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if entry.src_pos.is_none() && entry.fallback_pos.is_none() {
                if let Some(span) = span {
                    entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                        def_uri.to_string(),
                        span.start as u32,
                    ));
                }
            }
        }
    }

    /// Port declaration span for a flattened port/bus name. Body instances
    /// (`insts.port_spans`) first, then signature interface params
    /// (`def.params.def_spans`) — the latter covers bracket-form params such
    /// as `[VDD_3V3, GND]::DC(3.3V)`, whose whole-name span lives in
    /// `def.params` and is dropped from `port_spans` by `filter_port_spans`.
    fn port_decl_span_of(inst: &McModuleInst, name: &str) -> Option<Range<usize>> {
        inst.def
            .insts
            .get_port_span(name)
            .or_else(|| inst.def.params.get_def_span(name))
    }

    /// Recursively flatten a module instance
    ///
    /// Traversal order: module itself → ports → components + pins →
    /// bus + members → standalone labels → sub-modules (recursive)
    fn flatten_module(
        &mut self,
        inst: &McModuleInst,
        prefix: &str,
        parent_id: Option<u32>,
        view: &TreeView,
    ) {
        // 1. Register the module itself
        let my_path = if prefix.is_empty() {
            inst.name.clone()
        } else {
            format!("{}.{}", prefix, inst.name)
        };
        let my_id = self.register(
            my_path.clone(),
            InstKind::Module,
            parent_id,
            inst.def.name.to_string(),
            IOType::None,
            None,
            inst.def_uri.to_string(),
        );

        // 2. Register ports
        for port in &inst.ports {
            let port_path = format!("{}.{}", my_path, port.name);
            let port_id = self.register(
                port_path,
                InstKind::Port,
                Some(my_id),
                String::new(),
                port.iotype.clone(),
                None,
                inst.def_uri.to_string(),
            );
            // Signature interface params (e.g. `[VDD_3V3, GND]::DC(3.3V)`)
            // are declared in `def.params`, not `def.insts` — when the body
            // instance lookup misses, fall back to the param declaration span
            // so unconnected-port diagnostics anchor at the declaration
            // instead of file:1:1.
            let port_decl_span = Self::port_decl_span_of(inst, &port.name);
            self.backfill_port_decl_pos(port_id, &inst.def_uri, port_decl_span.clone());

            // ── Phase-D support: register a bracketed path for ports with bus_members ──
            // Only create bracketed path for List ports (e.g., [A,B] or GPIO[1:2]),
            // NOT for Bus ports (e.g., rs485{A,B}) because Bus ports can be accessed
            // via the dot syntax (rs485.A, rs485.B).
            // Check if the port name contains '[' to identify List-style ports.
            if port.is_bus_port() && port.name.contains('[') {
                let bracket_name = format!("[{}]", port.bus_members.join(", "));
                let bracket_path = format!("{my_path}.{bracket_name}");
                let bracket_id = self.register(
                    bracket_path,
                    InstKind::Port,
                    Some(my_id),
                    String::new(),
                    port.iotype.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
                self.backfill_port_decl_pos(bracket_id, &inst.def_uri, port_decl_span.clone());
            }

            // ── P2-4: register individual bus member ports ──
            // Net points use plain member names (e.g. "VDD_3V3"), not bracket form.
            // Without individual registration, resolve_netpoint_path can't find them
            // and the port points are silently dropped from the InstTable nets.
            // This causes port-only nets (e.g. V5V.VCC, MIC.P, DAC) to be invisible
            // to netdiff and downstream consumers.
            //
            // ── P2-4: include port name prefix for non-bracket ports ──
            // Bracket ports (e.g. [VDD_3V3, GND]) use flat member paths:
            //   main.dcdc.VDD_3V3
            // Named ports (e.g. vin, vout, USB_VBUS_1) include port name:
            //   main.ldo.vin.VCC
            // This preserves the port→member relationship so netdiff can match
            // golden references like "vin.VCC" and "USB_VBUS_1.VDD_3V".
            for member in &port.bus_members {
                let member_path = if port.name.contains('[') {
                    // Bracket port: flat member path (e.g. [VDD_3V3, GND] → main.dcdc.VDD_3V3)
                    format!("{}.{}", my_path, member)
                } else if port.name.contains('{') {
                    // Curly port: extract base name prefix
                    // e.g. vin{VCC, GND} → main.ldo.vin.VCC
                    // e.g. {VCC, GND} → main.xxx.VCC (no base name, flat)
                    let base = port.name.split('{').next().unwrap_or("");
                    if base.is_empty() {
                        format!("{}.{}", my_path, member)
                    } else {
                        format!("{}.{}.{}", my_path, base, member)
                    }
                } else {
                    // Named port without brackets: include port name prefix
                    format!("{}.{}.{}", my_path, port.name, member)
                };
                let member_id = self.register(
                    member_path,
                    InstKind::Port,
                    Some(my_id),
                    String::new(),
                    port.iotype.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
                self.backfill_port_decl_pos(member_id, &inst.def_uri, port_decl_span.clone());

                // Set member_info role (Ground/Power) — consumed by the viz
                // projection layer for rail classification, not for net merging.
                let (role, _inferred) =
                    infer_member_role(member, &port.iotype, is_ground_name, is_supply_name);
                if !matches!(role, MemberRole::Signal) {
                    self.set_member_info(member_id, MemberInfo::new(role, None));
                }
            }
        }

        // ── ★ P8-1: build func-to-owner map for correcting parent_id of
        // func-created instances. When a component defines a func (e.g. FLASH.GD25Q32E
        // defines func GD25Q32E), instances created by that func belong to the
        // component instance, not the calling module.
        //
        // ★ P9-A1 fix: only include Declared components. Func-created instances
        // (CAP, RES etc.) are themselves being reparented and cannot be owners.
        let mut func_to_owner: HashMap<String, String> = HashMap::new();
        let comps: Vec<_> = view.components(inst).collect();
        for comp in comps {
            if !matches!(comp.origin, InstOrigin::Declared) {
                continue;
            }
            for func in comp.def.funcs.iter() {
                func_to_owner.insert(func.name.to_string(), comp.name.clone());
            }
        }

        // ★ §11.1: build the vector-member projection map (member name →
        // group info) from the modeling-layer `vectors` groups before the
        // component loop. `member_ids` are the physical instance coordinates
        // within this module (`c1`, `c2`, …), which equal the flat `comp.name`.
        // The map is looked up once per component — vector groups are sparse,
        // so a HashMap build is cheaper than scanning `inst.vectors` per comp.
        let mut vector_member_map: HashMap<String, VectorMemberInfo> = HashMap::new();
        for v in &inst.vectors {
            for (idx, mid) in v.member_ids.iter().enumerate() {
                let member = v
                    .member_names
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| mid.clone());
                vector_member_map
                    .entry(mid.clone())
                    .or_insert_with(|| VectorMemberInfo::new(v.base.clone(), member, idx));
            }
        }

        // 3. Register components + pins (two-pass: non-func-created first,
        //    then func-created with corrected parent_id)
        let comps: Vec<_> = view.components(inst).collect();
        for comp in comps {
            if matches!(comp.origin, InstOrigin::FuncCall { .. }) {
                continue; // handled in second pass
            }
            let comp_path = format!("{}.{}", my_path, comp.name);
            let comp_id = self.register(
                comp_path.clone(),
                InstKind::Component,
                Some(my_id),
                comp.def.name.to_string(),
                IOType::None,
                None,
                inst.def_uri.to_string(),
            );

            // ★ Declaration-site fallback for unwired instances
            // A fully-unconnected component never appears in a net, so no net
            // point back-fills a wiring site into `src_pos` — net diagnostics
            // (E4116 pin-count, E4112, …) would anchor at offset 0 → file:1:1.
            // The module's instance table records the declaration span of
            // `RES r1` (parse_declare → store_port_span); use it as the
            // fallback so the report points at the declaration instead.
            if let Some(span) = inst.def.insts.get_port_span(&comp.name) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    if entry.src_pos.is_none() && entry.fallback_pos.is_none() {
                        entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                            inst.def_uri.clone(),
                            span.start as u32,
                        ));
                    }
                }
            }

            // ★ M0-B-D: pass through the nc marker
            if comp.nc {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.not_fitted = true;
                }
            }
            // ★ M0-B-E: pass through origin
            if let Some(entry) = self.entries.get_mut(&comp_id) {
                entry.origin = comp.origin.clone();
            }
            // ★ §11.1: attach the vector-group projection for vector members
            if let Some(info) = vector_member_map.get(&comp.name) {
                self.set_vector_info(comp_id, info.clone());
            }

            // ★ M11.3: record bridge passive full paths
            if inst.bridge_passive_names.contains(&comp.name) {
                self.bridge_passive_paths.insert(comp_path.clone());
            }

            // Each pin as an independent entry
            // ★ Use sorted keys to ensure stable pin order
            let mut pin_names: Vec<&String> = comp.pins.keys().collect();
            pin_names.sort();
            for pin_name in pin_names {
                if let Some(net_point) = comp.pins.get(pin_name) {
                    let pin_path = format!("{comp_path}.{pin_name}");
                    let pin_func_name = comp
                        .cond_pin_names
                        .get(pin_name)
                        .and_then(|names| names.first())
                        .or_else(|| {
                            comp.def
                                .pins
                                .pin_id_to_names
                                .get(pin_name)
                                .and_then(|names| names.first())
                        })
                        .cloned()
                        .unwrap_or_default();
                    let pin_id = self.register(
                        pin_path,
                        InstKind::Pin,
                        Some(comp_id),
                        pin_func_name.clone(),
                        net_point.iotype.clone(),
                        net_point.src_pos.clone(),
                        inst.def_uri.to_string(),
                    );

                    // ── Fallback position for unconnected pins ──
                    // An unconnected pin never appears in a net, so `flatten_nets`
                    // can't back-fill a wiring site into `src_pos`. Anchor the
                    // pin's diagnostics at its declaration instead: the pin-id
                    // span in the component body (`io [12,13] = UART1...`).
                    // Only set when the span exists AND the entry has no position
                    // of its own (declaration is strictly weaker than a wiring site).
                    if let Some(entry) = self.entries.get_mut(&pin_id) {
                        if entry.src_pos.is_none() && entry.fallback_pos.is_none() {
                            if let Some(r) = comp.def.pins.pin_id_spans.get(pin_name) {
                                entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                                    comp.def.uri.clone(),
                                    r.start as u32,
                                ));
                            }
                        }
                    }

                    // Set member_info role (Ground/Power) — consumed by the viz
                    // projection layer for rail classification, not for net merging.
                    let (role, _inferred) = infer_member_role(
                        &pin_func_name,
                        &net_point.iotype,
                        is_ground_name,
                        is_supply_name,
                    );
                    if !matches!(role, MemberRole::Signal) {
                        self.set_member_info(pin_id, MemberInfo::new(role, None));
                    }
                }
            }
        }

        // ★ P8-1 pass 2: func-created components — re-parent to the component
        // that defines the func, not the calling module.
        let comps: Vec<_> = view.components(inst).collect();
        for comp in comps {
            if !matches!(comp.origin, InstOrigin::FuncCall { .. }) {
                continue;
            }
            let fn_name = match &comp.origin {
                InstOrigin::FuncCall { fn_name, .. } => fn_name.clone(),
                _ => continue,
            };

            let (comp_path, comp_parent_id) = match func_to_owner.get(&fn_name) {
                Some(owner_name) => {
                    let owner_path = format!("{my_path}.{owner_name}");
                    if let Some(owner_id) = self.get_id_by_path(&owner_path) {
                        let path = format!("{owner_path}.{}", comp.name);
                        (path, Some(owner_id))
                    } else {
                        // Owner not found (should not happen), fall back to module parent
                        (format!("{my_path}.{}", comp.name), Some(my_id))
                    }
                }
                None => {
                    // Func owner not in this module (e.g., builtin func), fall back
                    (format!("{my_path}.{}", comp.name), Some(my_id))
                }
            };

            let comp_id = self.register(
                comp_path.clone(),
                InstKind::Component,
                comp_parent_id,
                comp.def.name.to_string(),
                IOType::None,
                None,
                inst.def_uri.to_string(),
            );

            // ★ Declaration-site fallback (same rationale as pass-1): anchor
            // net diagnostics for a never-wired func-created instance at the
            // caller's declaration rather than offset 0 → file:1:1.
            if let Some(span) = inst.def.insts.get_port_span(&comp.name) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    if entry.src_pos.is_none() && entry.fallback_pos.is_none() {
                        entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                            inst.def_uri.clone(),
                            span.start as u32,
                        ));
                    }
                }
            }

            if comp.nc {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.not_fitted = true;
                }
            }
            if let Some(entry) = self.entries.get_mut(&comp_id) {
                entry.origin = comp.origin.clone();
            }

            if inst.bridge_passive_names.contains(&comp.name) {
                self.bridge_passive_paths.insert(comp_path.clone());
            }

            let mut pin_names: Vec<&String> = comp.pins.keys().collect();
            pin_names.sort();
            for pin_name in pin_names {
                if let Some(net_point) = comp.pins.get(pin_name) {
                    let pin_path = format!("{comp_path}.{pin_name}");
                    let pin_func_name = comp
                        .cond_pin_names
                        .get(pin_name)
                        .and_then(|names| names.first())
                        .or_else(|| {
                            comp.def
                                .pins
                                .pin_id_to_names
                                .get(pin_name)
                                .and_then(|names| names.first())
                        })
                        .cloned()
                        .unwrap_or_default();
                    let pin_id = self.register(
                        pin_path,
                        InstKind::Pin,
                        Some(comp_id),
                        pin_func_name.clone(),
                        net_point.iotype.clone(),
                        net_point.src_pos.clone(),
                        inst.def_uri.to_string(),
                    );

                    let (role, _inferred) = infer_member_role(
                        &pin_func_name,
                        &net_point.iotype,
                        is_ground_name,
                        is_supply_name,
                    );
                    if !matches!(role, MemberRole::Signal) {
                        self.set_member_info(pin_id, MemberInfo::new(role, None));
                    }
                }
            }
        }

        // 4. Register bus + bus members (Step 6: bus member expansion)
        //    Bus paths use `.` separator: main.power
        //    Bus member paths use `/` separator: main.power/VCC
        // [P0-DET] iterate buses in sorted name order: `register` allocates ids by
        // call order, so HashMap iteration order would leak into entry/pin ids.
        //
        // Phase E: labels/buses come from the module's frozen overlay fragment
        // in the store (keyed by `my_path`) — `McModuleInst` no longer carries
        // them. The fragment is cloned out first so the store borrow ends
        // before the `&mut self` `register` calls below.
        let (labels, buses) = {
            let store = self.net_table.borrow();
            let labels: HashMap<String, NetPoint> = store.labels_of(&my_path).clone();
            let buses: HashMap<String, McBusInst> = store.buses_of(&my_path).clone();
            (labels, buses)
        };
        let mut bus_names: Vec<&String> = buses.keys().collect();
        bus_names.sort();
        for bus_name in bus_names {
            let bus_inst = &buses[bus_name];
            let bus_path = format!("{my_path}.{bus_name}");

            // ── Bug ② defense ───────────────────────────────────────────
            // `inst.buses` theoretically only contains real buses, but if the
            // upstream (points.rs's ensure_bus) mistakenly collects some
            // component/sub-module instance name as a bus, here it would expand
            // component pins into `<comp>/<pid>` form Labels. Step 3 has already
            // registered components/sub-modules with `.` as Component/Module;
            // if bus_path hits either of these two kinds, skip the whole bus.
            if let Some(existing_id) = self.get_id_by_path(&bus_path) {
                if let Some(e) = self.get_entry(existing_id) {
                    if matches!(e.kind, InstKind::Component | InstKind::Module) {
                        continue;
                    }
                }
            }

            // ── Fix: inherit IO type from Port if this bus is a port declaration ──
            // Bus ports like `rs485{A,B}` have IO type InOut, but their members
            // were registered with IOType::None, causing them to be misidentified
            // as power labels in viz rendering.
            let bus_io = self
                .get_id_by_path(&bus_path)
                .and_then(|id| self.get_entry(id))
                .map(|e| e.io_type.clone())
                .unwrap_or(IOType::None);

            let bus_id = self.register(
                bus_path.clone(),
                InstKind::Bus,
                Some(my_id),
                String::new(),
                bus_io.clone(),
                None,
                inst.def_uri.to_string(),
            );

            // Expand bus members with the inherited IO type
            let member_decl_span = Self::port_decl_span_of(inst, bus_name);
            for member in &bus_inst.members {
                let member_path = format!("{bus_path}/{member}");
                let member_id = self.register(
                    member_path,
                    InstKind::Label,
                    Some(bus_id),
                    String::new(),
                    bus_io.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
                // Unconnected bus members (E4117) have no wiring site; anchor
                // them at the owning port/bus declaration span instead of
                // file:1:1 (same fallback as the port loop above).
                self.backfill_port_decl_pos(member_id, &inst.def_uri, member_decl_span.clone());
            }
        }

        // 5. Register standalone labels (avoid duplication with ports/buses)
        // [P0-DET] sorted name order: `register` allocates ids by call order.
        // Port buses also inject bare member labels (e.g. `io MIC{P,N}` yields
        // plain `P` / `N` labels) that carry no wiring site; map each member
        // back to its owning port's declaration span so E4117 anchors there.
        let mut member_to_port_span: HashMap<&str, Option<Range<usize>>> = HashMap::new();
        for port in &inst.ports {
            let span = Self::port_decl_span_of(inst, &port.name);
            for member in &port.bus_members {
                member_to_port_span
                    .entry(member.as_str())
                    .or_insert_with(|| span.clone());
            }
        }
        let mut label_names: Vec<&String> = labels.keys().collect();
        label_names.sort();
        for label_name in label_names {
            let net_point = &labels[label_name];
            let label_path = format!("{my_path}.{label_name}");
            if self.get_id_by_path(&label_path).is_none() {
                let label_id = self.register(
                    label_path,
                    InstKind::Label,
                    Some(my_id),
                    String::new(),
                    net_point.iotype.clone(),
                    net_point.src_pos.clone(),
                    inst.def_uri.to_string(),
                );
                self.backfill_port_decl_pos(
                    label_id,
                    &inst.def_uri,
                    member_to_port_span
                        .get(label_name.as_str())
                        .cloned()
                        .flatten(),
                );
            }
        }

        // 6. Recursively process sub-modules. Phase C S3: the view's arena
        //    `children` edges drive the traversal order (design §4 — the tree
        //    is a view over arena edges); the aligned tree node supplies the
        //    sub-module data from the store.
        for sub in view.sub_modules(inst) {
            self.flatten_module(sub, &my_path, Some(my_id), view);
        }

        // 7. Register network information (module's frozen string net table)
        self.flatten_nets(inst, &my_path);
    }

    /// Flatten the module instance's net table into NetEntry records
    ///
    /// Traverse the module's frozen string net table (Phase D — sourced from
    /// the circuit-wide store, never from the tree), add the module prefix to
    /// each `NetPoint.path` and map to the registered `InstEntry.id`.
    ///
    /// ## Path resolution — three-level fallback + bracket expansion
    ///
    /// Maintains **exactly the same** behavior as
    /// `crate::vector::mc_vec_builder::McVecBuilder::resolve_netpoint`,
    /// avoiding the two pipelines (flatten_nets and mc_vec_builder) giving
    /// different resolution results for the same `NetPoint.path`.
    ///
    /// In the past, `flatten_nets` only tried two candidates: `module_path.path`
    /// and `path`, causing all points in the "bus member" form (e.g. `mic.MIC.P`
    /// needing to hit `main.mic.MIC/P`) to be silently lost here, making
    /// `InstTable.nets` have far fewer points than `McVecBlock.nets`, which in
    /// turn caused the layer to see fewer top-level edges (root cause 2).
    ///
    /// See `resolve_netpoint_path` comment for details.
    fn flatten_nets(&mut self, _inst: &McModuleInst, module_path: &str) {
        // [P0-DET] sorted net-name order: `net_id_counter` is allocated by iteration
        // order, so HashMap order would leak into net ids (and downstream pin ids).
        // Each module's table is a Vec (a module may hold multiple nets all named
        // "GND"); it is pre-sorted deterministically by build_net_table, so just
        // iterate in order.
        let net_entries = self
            .net_table
            .borrow()
            .get(module_path)
            .map(|t| t.to_vec())
            .unwrap_or_default();

        for (net_name, net_points) in net_entries {
            let mut point_ids: Vec<u32> = Vec::new();

            for np in &net_points {
                let mut ids = self.resolve_netpoint_path(&np.path, module_path);
                // ── P2-2: register boundary connection pins on the fly ──
                // When a boundary connection creates a pin like mcu.10, it's not
                // registered as a port or component pin in the InstTable. Register it
                // as a Pin entry under the owner submodule so flatten_nets can resolve it.
                if ids.is_empty() && np.owner.is_some() {
                    if let Some(owner_name) = &np.owner {
                        let full_path = format!("{module_path}.{}", np.path);
                        let owner_full = format!("{module_path}.{owner_name}");
                        if let Some(parent_id) = self.get_id_by_path(&owner_full) {
                            let pin_id = self.register(
                                full_path,
                                InstKind::Pin,
                                Some(parent_id),
                                String::new(),
                                np.iotype.clone(),
                                np.src_pos.clone(),
                                String::new(),
                            );
                            ids.push(pin_id);
                        }
                    }
                }
                for id in ids {
                    // ★ Back-fill entry src_pos from the net point, so net-level
                    // diagnostics (E4103 undriven-net, driver-conflict,
                    // voltage-mismatch, …) resolve to the wiring site instead of
                    // offset 0 → file:1:1. Entries registered earlier (bus
                    // members, ports, pins) carry no position; the net point's
                    // src_pos is the first real position available. Only
                    // back-fill when the position lives in the entry's own file
                    // — a position from a library func body (e.g. res.mc) would
                    // be interpreted in the wrong file otherwise.
                    if let Some(sp) = &np.src_pos {
                        if let Some(entry) = self.entries.get_mut(&id) {
                            if entry.src_pos.is_none() && sp.uri == entry.def_uri {
                                entry.src_pos = Some(sp.clone());
                            }
                        }
                    }
                    point_ids.push(id);
                }
            }

            // At least 2 endpoints are needed to constitute a meaningful net
            if point_ids.len() >= 2 {
                let net_id = self.net_id_counter;
                self.net_id_counter += 1;

                // Build reverse index
                for &pid in &point_ids {
                    self.point_to_net.insert(pid, net_id);
                }

                self.nets.insert(
                    net_id,
                    NetEntry {
                        id: net_id,
                        name: net_name.clone(),
                        points: point_ids,
                    },
                );
            } else if point_ids.is_empty() && !net_points.is_empty() {
                // ── §11.4 GAP2: net statement materialized 0 physical pins ─────
                // Every NetPoint of this module-level net failed to resolve to a
                // registered physical entry (component pin / port). The whole
                // statement produced no connection at all — its endpoints are
                // unresolved structured ghosts (bases declared nowhere) or paths
                // no entity registered. This is the global half of E4057
                // (NET_DROPPED_STATEMENT): the local NAME[k] alias site in
                // mc_phrase.rs catches the indexed-alias shape at pass1; here the
                // materialization fact — 0 pins from a live net — is checked on
                // the flattened table, once per dropped net, at the first
                // point's wiring site. A net that kept ≥1 physical point is a
                // stub, not 0-pin, and stays quiet here (the ghost reference is
                // E3137's pass1 domain; the orphaned pin is the 41xx unconnected
                // checks' domain) — the domains do not double-report.
                let src_pos = net_points.iter().find_map(|np| np.src_pos.clone());
                let paths: Vec<String> = net_points.iter().map(|np| np.path.clone()).collect();
                let msg = crate::errcodes::format_msg(
                    crate::errcodes::NET_DROPPED_STATEMENT,
                    &[
                        &net_name.clone(),
                        &format!(
                            "materialized no physical pins (endpoints [{}] all failed to resolve)",
                            paths.join(", ")
                        ),
                    ],
                );
                match &src_pos {
                    Some(sp) => crate::db::diagnostic::diagnostic::diagnostic_log_at(
                        crate::errcodes::NET_DROPPED_STATEMENT,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        sp.uri.clone(),
                        sp.offset,
                        0,
                        &msg,
                        &[],
                    ),
                    None => crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::NET_DROPPED_STATEMENT,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        0,
                        0,
                        &msg,
                        &[],
                    ),
                }
            }
        }
    }

    /// Resolve a single NetPoint path to zero or more registered InstEntry IDs
    ///
    /// This method is the "single source of truth" shared by `flatten_nets` and
    /// `mc_vec_builder::resolve_netpoint` — both must produce the same ID set
    /// for the same `NetPoint.path`, otherwise the downstream (drawing layer)
    /// will see edges inconsistent with McVecBlock.
    ///
    /// ## Input forms
    /// - Plain single path: `R1.1`, `VCC`, `power.VCC`, `sub1.clk` → 0 or 1 ID
    /// - List form (bracket): `sub.[A, B, C]` → up to 3 IDs (resolve each after expansion)
    ///
    /// ## Resolution failure
    /// Single paths for which all fallbacks fail are silently discarded
    /// (return empty Vec). No warning is printed and no counter is incremented
    /// here; diagnosis is handled on the `mc_vec_builder` side;
    /// the flatten_nets side only cares about "connect what can be connected,
    /// skip what cannot".
    pub(crate) fn resolve_netpoint_path(&self, path: &str, module_path: &str) -> Vec<u32> {
        // ── (A) Bracket list expansion: `sub.[A, B]` → ["sub.A", "sub.B"] ──
        if let Some(expanded) = expand_bracket_list(path) {
            return expanded
                .iter()
                .filter_map(|p| self.resolve_single_path(p, module_path))
                .collect();
        }

        // ── (B) Plain single path: three-level fallback ──
        self.resolve_single_path(path, module_path)
            .into_iter()
            .collect()
    }

    /// Single path three-level fallback resolution (internal helper)
    ///
    /// Lookup order:
    /// 1. `module_path.path`        (most common: sub-module internal component pin/port)
    /// 2. `path`                    (top-level port direct reference)
    /// 3. Replace the trailing `.` with `/` and try (★ key: bus member, e.g. `power.VCC` → `power/VCC`)
    ///
    /// ## Why (3) is needed: heterogeneous path separators
    /// InstTable registration rules:
    /// - Component pin / module port / sub-module — joined by `.` (e.g. `main.mcu.uC.XTAL`)
    /// - Bus member — joined by `/` (e.g. `main.power/VCC`, see flatten_module step 4)
    ///
    /// And `NetPoint.path` is **always assembled with `.`** in the phrase parsing stage,
    /// so all points accessed via "bus member" (`bus.member` syntax) will miss
    /// in steps (1) and (2). Step (3) replaces the trailing separator with `/`
    /// and tries once more to hit them.
    ///
    /// Only replacing the **last** `.` is intentional — to avoid multiple
    /// ambiguous interpretations of `a.b.c` (`a.b/c` vs `a/b.c`), and consistent
    /// with the current single-level bus expansion semantic boundary.
    fn resolve_single_path(&self, path: &str, module_path: &str) -> Option<u32> {
        // Handle the edge case where `module_path` is the empty string
        // (current callers guarantee non-empty, defensive handling here)
        let full_path = if module_path.is_empty() {
            path.to_string()
        } else {
            format!("{module_path}.{path}")
        };

        // (1) Module prefix + path, most common
        if let Some(&id) = self.path_index.get(&full_path) {
            return Some(id);
        }
        // (2) Direct path lookup (top-level port/label)
        if let Some(&id) = self.path_index.get(path) {
            return Some(id);
        }
        // (3) ★ Replace last `.` with `/` — bus member fallback
        //     Example: main.power.VCC → main.power/VCC
        //              power.VCC      → power/VCC (if top-level is a bus)
        for candidate in [full_path.as_str(), path] {
            if let Some(pos) = candidate.rfind('.') {
                let bus_style = format!("{}/{}", &candidate[..pos], &candidate[pos + 1..]);
                if let Some(&id) = self.path_index.get(&bus_style) {
                    return Some(id);
                }
            }
        }
        None
    }

    // ====================================================================
    // dump output (Step 8)
    // ====================================================================

    /// Print the table (for debugging)
    pub fn dump(&self) {
        mcc_dbg!(
            "inst::table",
            "  {:<6} {:<40} {:<12} {:<16} IO",
            "ID",
            "Path",
            "Kind",
            "Class"
        );
        mcc_dbg!(
            "inst::table",
            "  {:<6} {:<40} {:<12} {:<16} ────",
            "──────",
            "────────────────────────────────────────",
            "────────────",
            "────────────────"
        );
        for entry in self.entries.values() {
            let io_str = match &entry.io_type {
                IOType::In => "in",
                IOType::Out => "out",
                IOType::InOut => "io",
                IOType::Power => "power",
                IOType::Analog => "analog",
                IOType::Return => "return",
                IOType::NonCon => "nc",
                IOType::Label => "label",
                IOType::None => "-",
            };
            let class_display = if entry.class_name.is_empty() {
                "-"
            } else {
                &entry.class_name
            };
            mcc_dbg!(
                "inst::table",
                "  {:<6} {:<40} {:<12} {:<16} {}",
                entry.id,
                entry.path,
                entry.kind,
                class_display,
                io_str
            );
        }
        mcc_dbg!(
            "inst::table",
            "  ── Total: {} entries ──",
            self.entries.len()
        );

        // Output network information
        if !self.nets.is_empty() {
            mcc_dbg!("inst::table", "");
            mcc_dbg!("inst::table", "  {:<8} {:<24} Points", "NetID", "Name");
            mcc_dbg!(
                "inst::table",
                "  {:<8} {:<24} ──────────────────────────────",
                "────────",
                "────────────────────────"
            );
            for net in self.nets.values() {
                let point_strs: Vec<String> = net
                    .points
                    .iter()
                    .map(|pid| {
                        self.entries
                            .get(pid)
                            .map(|e| format!("#{} ({})", pid, e.path))
                            .unwrap_or_else(|| format!("#{pid}"))
                    })
                    .collect();
                mcc_dbg!(
                    "inst::table",
                    "  {:<8} {:<24} [{}]",
                    net.id,
                    net.name,
                    point_strs.join(", ")
                );
            }
            mcc_dbg!("inst::table", "  ── Total: {} nets ──", self.nets.len());
        }
    }

    /// Collect all failed component records from the module tree and write to known_missing.md.
    ///
    /// Phase C S3-D: the tree walk resolves sub-modules through `view` (arena
    /// edges + store — the tree's Vec fields are gone).
    pub fn write_known_missing(inst: &McModuleInst, output_path: &str, view: &TreeView) {
        let mut all_records: Vec<&crate::instant::mc_mod::FailedRecord> = Vec::new();
        Self::collect_failed_records(inst, view, &mut all_records);

        if all_records.is_empty() {
            return;
        }

        let mut content = String::from("# Known Missing Components (G4 Baseline)\n\n");
        content.push_str(
            "Components that failed instantiation and were excluded from the netlist.\n\n",
        );
        content.push_str("| Module | Component | Class | Src Line | Reason |\n");
        content.push_str("|--------|-----------|-------|----------|--------|\n");

        for r in &all_records {
            let line = r
                .src_line
                .map(|l| l.to_string())
                .unwrap_or_else(|| "?".to_string());
            content.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                r.module, r.component_name, r.class_name, line, r.reason
            ));
        }

        content.push_str(&format!(
            "\nTotal: {} failed instantiation(s)\n",
            all_records.len()
        ));

        if let Some(parent) = std::path::Path::new(output_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(output_path, &content) {
            mcc_dbg!(
                "inst::table",
                "[G4] Failed to write known_missing.md: {}",
                e
            );
        } else {
            mcc_dbg!(
                "inst::table",
                "[G4] known_missing.md written with {} entries",
                all_records.len()
            );
        }
    }

    fn collect_failed_records<'a>(
        inst: &'a McModuleInst,
        view: &'a TreeView<'a>,
        out: &mut Vec<&'a crate::instant::mc_mod::FailedRecord>,
    ) {
        for r in &inst.failed_records {
            out.push(r);
        }
        for sub in view.sub_modules(inst) {
            Self::collect_failed_records(sub, view, out);
        }
    }
}

// ============================================================================
// Helper: bracket list expansion (Iter 1 extension)
// ============================================================================

/// Try to split a path of the form `<prefix>.[<m1>, <m2>, ...]` into a list of
/// independent paths
///
/// Maintains **consistent behavior** with
/// `crate::vector::mc_vec_builder::McVecBuilder::expand_bracket_list`
/// (both sides share the same set of rules, avoiding drift).
///
/// - Returns `Some(vec!["<prefix>.<m1>", "<prefix>.<m2>", ...])`
/// - Non-match, malformed form (empty prefix / empty list / `]` not at the end),
///   or empty members all return `None`, and the caller treats it as a normal
///   single path
///
/// ## Design decisions
/// - Use `.[` as the only trigger identifier
/// - `]` must be at the end of the string; otherwise degrade to normal single
///   path processing (safer than accidental splitting)
/// - Members split by `,` and `trim`; empty members are filtered (tolerate `a.[X, ,Y]`)
/// - Nesting is not supported (`a.[X.[Y, Z], W]`)
fn expand_bracket_list(path: &str) -> Option<Vec<String>> {
    let open = path.find(".[")?;
    if !path.ends_with(']') {
        return None;
    }
    let close = path.len() - 1;
    // Defend against zero-length body like `prefix.[]` (close - (open + 2) < 1)
    if close <= open + 2 {
        return None;
    }
    let prefix = &path[..open];
    if prefix.is_empty() {
        return None;
    }
    let body = &path[open + 2..close];
    let members: Vec<String> = body
        .split(',')
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(|m| format!("{prefix}.{m}"))
        .collect();
    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup() {
        let mut table = InstTable::new(1000);
        let id = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        assert_eq!(id, 1000);
        assert_eq!(table.get_id_by_path("main"), Some(1000));
        assert!(table.get_entry(1000).is_some());
    }

    #[test]
    fn test_no_duplicate_registration() {
        let mut table = InstTable::new(1000);
        let id1 = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        let id2 = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        assert_eq!(id1, id2);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_children_of() {
        let mut table = InstTable::new(1000);
        let parent = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        table.register_simple(
            "main.VCC".into(),
            InstKind::Port,
            Some(parent),
            String::new(),
            IOType::In,
        );
        table.register_simple(
            "main.GND".into(),
            InstKind::Port,
            Some(parent),
            String::new(),
            IOType::In,
        );
        table.register_simple(
            "main.R1".into(),
            InstKind::Component,
            Some(parent),
            "Res".into(),
            IOType::None,
        );

        let children = table.children_of(parent);
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_id_uniqueness() {
        let mut table = InstTable::new(1000);
        table.register_simple("a".into(), InstKind::Module, None, "A".into(), IOType::None);
        table.register_simple("b".into(), InstKind::Module, None, "B".into(), IOType::None);
        table.register_simple("c".into(), InstKind::Module, None, "C".into(), IOType::None);

        let ids: Vec<u32> = table.iter().map(|(id, _)| *id).collect();
        let unique: std::collections::HashSet<u32> = ids.iter().cloned().collect();
        assert_eq!(ids.len(), unique.len());
    }

    // ========================================================================
    // Iter 1: Path resolution's three-level fallback + bracket expansion
    // ========================================================================

    /// Bus member fallback: `power.VCC` should hit `main.power/VCC` already
    /// registered with `/`
    ///
    /// This is the most direct reproduction of root cause 2 — the original
    /// `flatten_nets` only tried `main.power.VCC` and `power.VCC`, both miss,
    /// causing the point to be silently lost in the flat netlist.
    #[test]
    fn test_resolve_bus_member_path_fallback() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let bus = table.register_simple(
            "main.power".into(),
            InstKind::Bus,
            Some(m),
            String::new(),
            IOType::None,
        );
        let vcc = table.register_simple(
            "main.power/VCC".into(),
            InstKind::Label,
            Some(bus),
            String::new(),
            IOType::None,
        );

        let ids = table.resolve_netpoint_path("power.VCC", "main");
        assert_eq!(
            ids,
            vec![vcc],
            "bus-member path should resolve via `/` fallback"
        );
    }

    /// Plain component pin path still hits from step (1), fallback does not
    /// change existing behavior
    #[test]
    fn test_resolve_plain_dot_path_still_works() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let comp = table.register_simple(
            "main.R1".into(),
            InstKind::Component,
            Some(m),
            String::new(),
            IOType::None,
        );
        let pin = table.register_simple(
            "main.R1.1".into(),
            InstKind::Pin,
            Some(comp),
            String::new(),
            IOType::None,
        );

        let ids = table.resolve_netpoint_path("R1.1", "main");
        assert_eq!(ids, vec![pin]);
    }

    /// Top-level port `VCC` (without prefix) should be hit by step (2)
    #[test]
    fn test_resolve_top_level_port_no_prefix() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let vcc = table.register_simple(
            "main.VCC".into(),
            InstKind::Port,
            Some(m),
            String::new(),
            IOType::None,
        );
        // Use "main.VCC" directly, go through step (1)
        let ids = table.resolve_netpoint_path("VCC", "main");
        assert_eq!(ids, vec![vcc]);
    }

    /// Bracket expansion: `sub.[A, B]` should be resolved into two independent IDs
    #[test]
    fn test_resolve_bracket_list_expands() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let sub = table.register_simple(
            "main.moddcdc".into(),
            InstKind::Module,
            Some(m),
            String::new(),
            IOType::None,
        );
        let a = table.register_simple(
            "main.moddcdc.VDD_3V3".into(),
            InstKind::Port,
            Some(sub),
            String::new(),
            IOType::None,
        );
        let b = table.register_simple(
            "main.moddcdc.GND".into(),
            InstKind::Port,
            Some(sub),
            String::new(),
            IOType::None,
        );

        let ids = table.resolve_netpoint_path("moddcdc.[VDD_3V3, GND]", "main");
        assert_eq!(ids, vec![a, b]);
    }

    /// Bracket partial hit: missed members are silently skipped, hit members
    /// retain original order
    #[test]
    fn test_resolve_bracket_partial_miss() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let sub = table.register_simple(
            "main.moddcdc".into(),
            InstKind::Module,
            Some(m),
            String::new(),
            IOType::None,
        );
        let a = table.register_simple(
            "main.moddcdc.VDD_3V3".into(),
            InstKind::Port,
            Some(sub),
            String::new(),
            IOType::None,
        );
        // GHOST deliberately not registered

        let ids = table.resolve_netpoint_path("moddcdc.[VDD_3V3, GHOST]", "main");
        assert_eq!(ids, vec![a]);
    }

    /// Unregistered path returns empty Vec (no panic, no polluting reverse index)
    #[test]
    fn test_resolve_missing_path_returns_empty() {
        let mut table = InstTable::new(1000);
        table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let ids = table.resolve_netpoint_path("ghost.signal", "main");
        assert!(ids.is_empty());
    }

    /// Syntax test cases for expand_bracket_list (kept in sync with mc_vec_builder side)
    #[test]
    fn test_expand_bracket_list_syntax() {
        assert_eq!(
            expand_bracket_list("moddcdc.[VDD_3V3, GND]"),
            Some(vec!["moddcdc.VDD_3V3".into(), "moddcdc.GND".into()])
        );
        assert_eq!(
            expand_bracket_list("sub.[ A , B ]"),
            Some(vec!["sub.A".into(), "sub.B".into()])
        );
        assert_eq!(expand_bracket_list("sub.[X]"), Some(vec!["sub.X".into()]));
        // No match: no `.[`
        assert_eq!(expand_bracket_list("foo.bar"), None);
        // No match: `]` not at the end
        assert_eq!(expand_bracket_list("foo.[A, B].suffix"), None);
        // Malformed: empty body
        assert_eq!(expand_bracket_list("foo.[]"), None);
        // Malformed: empty prefix
        assert_eq!(expand_bracket_list(".[A, B]"), None);
        // Tolerate: extra commas between members
        assert_eq!(
            expand_bracket_list("foo.[A, , B]"),
            Some(vec!["foo.A".into(), "foo.B".into()])
        );
    }
}
