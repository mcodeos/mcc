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

use super::mc_mod::McModuleInst;
use crate::core::common::IOType;
use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// Definition class name: "Res", "MCU_ABC", "power_domain" (empty string for pin/port/label)
    pub class_name: String,
    /// IO type (only meaningful for Port/Pin, otherwise IOType::None)
    pub io_type: IOType,
    /// Source byte offset in the definition file (from NetPoint / AST)
    pub src_pos: Option<i32>,
    /// URI of the file where this instance was defined
    pub def_uri: String,
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
    /// Network name (port name > label name > auto number __net_N)
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
        }
    }

    /// Recursively generate flattened instance table from McModuleInst tree
    pub fn from_module_inst(inst: &McModuleInst, start_id: u32) -> Self {
        let mut table = InstTable::new(start_id);
        table.flatten_module(inst, "", None);
        table
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
    ///   Both cases print a diagnostic line, making it easier to locate kind
    ///   seizure issues like "Component registered as Port" (see bug ①).
    pub fn register(
        &mut self,
        path: String,
        kind: InstKind,
        parent_id: Option<u32>,
        class_name: String,
        io_type: IOType,
        src_pos: Option<i32>,
        def_uri: String,
    ) -> u32 {
        // Prevent duplicate registration
        if let Some(&existing_id) = self.path_index.get(&path) {
            let existing_kind = self.entries.get(&existing_id).map(|e| e.kind.clone());

            if let Some(existing_kind) = existing_kind {
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
            def_uri,
        };

        self.entries.insert(id, entry);
        self.path_index.insert(path, id);
        id
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
    // flatten traversal (Step 5)
    // ====================================================================

    /// Recursively flatten a module instance
    ///
    /// Traversal order: module itself → ports → components + pins →
    /// bus + members → standalone labels → sub-modules (recursive)
    fn flatten_module(&mut self, inst: &McModuleInst, prefix: &str, parent_id: Option<u32>) {
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
            self.register(
                port_path,
                InstKind::Port,
                Some(my_id),
                String::new(),
                port.iotype.clone(),
                None,
                inst.def_uri.to_string(),
            );

            // ── Phase-D support: register a bracketed path for ports with bus_members ──
            // Only create bracketed path for List ports (e.g., [A,B] or GPIO[1:2]),
            // NOT for Bus ports (e.g., rs485{A,B}) because Bus ports can be accessed
            // via the dot syntax (rs485.A, rs485.B).
            // Check if the port name contains '[' to identify List-style ports.
            if port.is_bus_port() && port.name.contains('[') {
                let bracket_name = format!("[{}]", port.bus_members.join(", "));
                let bracket_path = format!("{my_path}.{bracket_name}");
                self.register(
                    bracket_path,
                    InstKind::Port,
                    Some(my_id),
                    String::new(),
                    port.iotype.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
            }
        }

        // 3. Register components + pins
        for comp in &inst.components {
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
                    // ★ Pin function name (right side of mc `=`: Base/TX/RXD/1...)
                    //   is carried to the drawing side via the class_name field
                    //   (from_block::build_box_pins → BoxPin.description → render_pin
                    //   drawn inside the box). The Pin entry's class_name is only
                    //   read as a "description" by build_box_pins, not used for class
                    //   recognition, so reuse is safe.
                    //   The function name of a pure numeric pin like `1=1` is also
                    //   "1" — carry it together — both outer pin number and inner
                    //   function name are drawn.
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
                    self.register(
                        pin_path,
                        InstKind::Pin,
                        Some(comp_id),
                        pin_func_name,
                        net_point.iotype.clone(),
                        net_point.src_pos,
                        inst.def_uri.to_string(),
                    );
                }
            }
        }

        // 4. Register bus + bus members (Step 6: bus member expansion)
        //    Bus paths use `.` separator: main.power
        //    Bus member paths use `/` separator: main.power/VCC
        // [P0-DET] iterate buses in sorted name order: `register` allocates ids by
        // call order, so HashMap iteration order would leak into entry/pin ids.
        let mut bus_names: Vec<&String> = inst.get_buses().keys().collect();
        bus_names.sort();
        for bus_name in bus_names {
            let bus_inst = &inst.get_buses()[bus_name];
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
            for member in &bus_inst.members {
                let member_path = format!("{bus_path}/{member}");
                self.register(
                    member_path,
                    InstKind::Label,
                    Some(bus_id),
                    String::new(),
                    bus_io.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
            }
        }

        // 5. Register standalone labels (avoid duplication with ports/buses)
        // [P0-DET] sorted name order: `register` allocates ids by call order.
        let mut label_names: Vec<&String> = inst.get_labels().keys().collect();
        label_names.sort();
        for label_name in label_names {
            let net_point = &inst.get_labels()[label_name];
            let label_path = format!("{my_path}.{label_name}");
            if self.get_id_by_path(&label_path).is_none() {
                self.register(
                    label_path,
                    InstKind::Label,
                    Some(my_id),
                    String::new(),
                    net_point.iotype.clone(),
                    net_point.src_pos,
                    inst.def_uri.to_string(),
                );
            }
        }

        // 6. Recursively process sub-modules
        for sub in &inst.sub_modules {
            self.flatten_module(sub, &my_path, Some(my_id));
        }

        // 7. Register network information (map McModuleInst.nets to InstEntry IDs)
        self.flatten_nets(inst, &my_path);
    }

    /// Flatten the module instance's net table into NetEntry records
    ///
    /// Traverse `McModuleInst.nets` (union-find merged nets),
    /// add the module prefix to each `NetPoint.path` and map to the registered
    /// `InstEntry.id`.
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
    fn flatten_nets(&mut self, inst: &McModuleInst, module_path: &str) {
        // [P0-DET] sorted net-name order: `net_id_counter` is allocated by iteration
        // order, so HashMap order would leak into net ids (and downstream pin ids).
        let mut net_names: Vec<&String> = inst.nets.keys().collect();
        net_names.sort();
        for net_name in net_names {
            let net_points = &inst.nets[net_name];
            let mut point_ids: Vec<u32> = Vec::new();

            for np in net_points {
                // NetPoint.path is the module's internal relative path (e.g. "R1.1",
                // "VCC", "power.VCC", or bracket form "sub.[A, B]"). A single path
                // may resolve to 0 / 1 / N IDs after three-level fallback (N > 1
                // only when bracket expansion).
                for id in self.resolve_netpoint_path(&np.path, module_path) {
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
    /// - Component pin / module port / sub-module — joined by `.` (e.g. `main.mcu513.uC.XTAL`)
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
        eprintln!(
            "  {:<6} {:<40} {:<12} {:<16} IO",
            "ID", "Path", "Kind", "Class"
        );
        eprintln!(
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
            eprintln!(
                "  {:<6} {:<40} {:<12} {:<16} {}",
                entry.id, entry.path, entry.kind, class_display, io_str
            );
        }
        eprintln!("  ── Total: {} entries ──", self.entries.len());

        // Output network information
        if !self.nets.is_empty() {
            eprintln!();
            eprintln!("  {:<8} {:<24} Points", "NetID", "Name");
            eprintln!(
                "  {:<8} {:<24} ──────────────────────────────",
                "────────", "────────────────────────"
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
                eprintln!(
                    "  {:<8} {:<24} [{}]",
                    net.id,
                    net.name,
                    point_strs.join(", ")
                );
            }
            eprintln!("  ── Total: {} nets ──", self.nets.len());
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
