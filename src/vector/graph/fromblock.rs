// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Main converter: `McVecBlock` -> `McVecGraph`
//!
//! ## ★ P03 (S1) Changes
//! Cut the dual-track net model, only produce `VizNet`:
//! - **Delete** Phase 3a (`generate_edges_from_net` + `edge_map`)
//! - **Delete** Phase 3.5's `synthesize_rail_edges`, replace with `synthesize_rail_nets`
//!   directly synthesizing `VizNet` (synthesized endpoint `pin_id = -1`)
//! Single net model: `VizNet` is the only net representation the graph carries.

use std::collections::HashMap;
use std::sync::Arc;

use crate::instant::insttab::{InstEntry, InstKind, InstTable, MemberRole};
use crate::semantic::module::McModule;

use super::super::model::netshape::{GroupRole, NetShape};
use super::super::model::{AttrRole, ConnectionType, McVecBlock, McVecNet, NetAttrMirror};
use super::boxdef::{
    BoundaryPort, BoxPin, CustomSymbol, IoSummary, McVecBox, PinConstraint, PinLayout, PortDir,
    VisualRole,
};
use super::detect::{
    compute_io, compute_scope_chain, detect_kind, detect_symbol, extract_designator,
    extract_last_segment, parse_pin_number, translate_io_type, warn_if_pin_mismatch, DetectedKind,
};
use super::graphdef::McVecGraph;
use super::kinds::{BoxKind, NetKind};
use super::naming;
use super::netdef::{EndpointRef, IoDirection, NetRole, VizNet};
use super::symbol::Symbol;

// ============================================================================
// §5③ Helper: declared rail identity (classification-retirement-design §4/§5)
// ============================================================================

/// Ground-side reading of an entry's declared supply role.
///
/// `member_info` is only set on declared Port/Pin members whose inferred role is
/// Power or Ground (flatten 1b); everything else — legacy Labels, signal members,
/// bus containers — returns `None` and must never be guessed from the name.
fn declared_rail_is_ground(entry: &InstEntry) -> Option<bool> {
    match entry.member_info.as_ref().map(|m| &m.role) {
        Some(&MemberRole::Ground) => Some(true),
        Some(&MemberRole::Power) => Some(false),
        _ => None,
    }
}

/// Ground-side reading of a net's declared supply mirror (`net.attr.role`).
///
/// Ret/Reference = the declared return (ground side), Hot = the supply side,
/// Signal = no declaration. `None` answers "not a declared rail" so the caller
/// must not draw a power/ground symbol (ruling ① — no declaration, no guessing).
fn attr_rail_is_ground(attr: &NetAttrMirror) -> Option<bool> {
    match attr.role {
        AttrRole::Ret | AttrRole::Reference => Some(true),
        AttrRole::Hot => Some(false),
        AttrRole::Signal => None,
    }
}

// ============================================================================
// Helper: IOType → PortDir
// ============================================================================

/// Translate `IOType` to `PortDir` for module ports
pub fn translate_io_to_port_dir(t: &crate::semantic::common::IOType) -> PortDir {
    use crate::semantic::common::IOType;
    match t {
        IOType::In => PortDir::In,
        IOType::Out => PortDir::Out,
        IOType::InOut => PortDir::Io,
        IOType::Power => PortDir::Ps,
        _ => PortDir::None,
    }
}

// ============================================================================
// Helper: build box from ID (shared by Phase 1 / Phase 1.5)
// ============================================================================

/// Build the physical pin list [`BoxPin`] from a group of pin/port `InstEntry`s
///
/// - `pin_id`      = mcode `=` left side's **common name / number** (path last segment: `1`/`B`/`A1`),
///                   used as-is, **no longer self-numbering 1/2/3**.
/// - `description` = mcode `=` right side's **function name / description** (`TX`/`Base`), taken from
///                   the Pin entry's `class_name`. Defense: if it equals the component's own class_name
///                   (inherited) or equals `pin_id`, treat as no valid description and empty it, to
///                   avoid treating component model as pin description.
/// - `io`          = translated pin direction.
///
/// `owner_class` is the class_name of the component this pin belongs to, used only for the above
/// dedup defense.
fn build_box_pins(entries: &[&InstEntry], owner_class: &str) -> Vec<BoxPin> {
    entries
        .iter()
        .map(|e| {
            let pin_id = extract_last_segment(&e.path);
            let raw = e.class_name.trim();
            // description = function name (mc `=` right). Pin entry's class_name is filled with the
            // function name by inst_table (port entry is always empty -> unaffected). When function
            // name == pin number (pure numeric pin `1=1`), **no longer discarded** -- outer pin number
            // + inner function name are both drawn (render_pin decides).
            // Still blocks owner_class, preventing component class name from accidentally leaking
            // into pin description.
            let description = if !raw.is_empty() && raw != owner_class {
                raw.to_string()
            } else {
                String::new()
            };
            BoxPin {
                id: e.id as i64,
                pin_id,
                description,
                io: translate_io_type(&e.io_type),
                port_dir: PortDir::None,
            }
        })
        .collect()
}

/// Typed chips (detect.rs Phase F.1) don't register pins as independent `Pin` children;
/// `pin_count` comes from the real recorded count (`InstEntry::pin_count`, see `detect_kind`).
/// When a chip still has a declared/resolved count but its Pin children failed to register,
/// we synthesize "placeholder pins" based on that count (common name uses the index, no
/// description), letting these components also display pins rather than an empty square.
/// Chips whose classes declare no pins have count 0 and get no fabricated pins.
///
/// Placeholder pins use high-base ids, not conflicting with real InstTable ids. These chips don't
/// have connections in this scenario, these ids won't be queried by router, even if duplicated
/// across boxes it's fine (`find_pin` only queries within its own box).
fn placeholder_pins(box_id: i64, pin_count: usize) -> Vec<BoxPin> {
    const PLACEHOLDER_BASE: i64 = 8_000_000_000;
    (0..pin_count)
        .map(|i| {
            let idx = (i + 1) as u32;
            BoxPin {
                id: PLACEHOLDER_BASE + box_id * 1000 + idx as i64,
                pin_id: idx.to_string(),
                description: String::new(),
                io: IoDirection::Unknown,
                port_dir: PortDir::None,
            }
        })
        .collect()
}

/// Build the module-port identities behind a module box's boundary.
///
/// A module's ports reach us as a **flat** list under the module's own id: a port
/// group (`main.LDO.vin`) and its members (`main.LDO.vin.V5V`,
/// `main.LDO.vin.GND`) are siblings with no parent link between them — the
/// grouping is carried by the dotted path alone. A port's identity is therefore
/// recovered structurally: the entry whose path is a *strict dotted prefix* of
/// another's is that one's group, and the group's leaf segment is the port name
/// the boundary is labeled with.
///
/// Every entry — group *and* each member — is emitted under the same port name,
/// so whichever of those ids a net endpoint happens to carry resolves to the same
/// port. Pure structure: ids and paths only, never a name heuristic.
fn boundary_ports_of(ports: &[&InstEntry]) -> Vec<BoundaryPort> {
    // An entry's group is the *longest* other entry whose path is a strict dotted
    // prefix of it, so `A.B` wins over `A` for `A.B.C`.
    let group_of = |e: &InstEntry| -> Option<&InstEntry> {
        ports
            .iter()
            .filter(|g| {
                g.id != e.id
                    && e.path.len() > g.path.len() + 1
                    && e.path.starts_with(&g.path)
                    && e.path.as_bytes()[g.path.len()] == b'.'
            })
            .max_by_key(|g| g.path.len())
            .copied()
    };

    ports
        .iter()
        .filter_map(|e| {
            let group = group_of(e).unwrap_or(e);
            let port_name = extract_last_segment(&group.path);
            if port_name.is_empty() {
                return None;
            }
            // The group's own io type is the port's declared direction. When the
            // declaration sits on the members instead (`psnk vin{V5V, GND}` — the
            // group entry carries no io type, the members do), read it from the
            // first member that declares one, so every id of the port agrees.
            let mut io = translate_io_type(&group.io_type);
            if io == IoDirection::Unknown {
                io = ports
                    .iter()
                    .filter(|m| group_of(m).map(|g| g.id) == Some(group.id))
                    .map(|m| translate_io_type(&m.io_type))
                    .find(|d| *d != IoDirection::Unknown)
                    .unwrap_or(IoDirection::Unknown);
            }
            Some(BoundaryPort {
                entry_pin_id: e.id as i64,
                port_name,
                io,
            })
        })
        .collect()
}

/// ★ Unified wiring point for component/module pin-layout and project SVG
/// overrides. SubModule boxes look up a *module* `layout = [ ... ]` (their pins
/// are the module's boundary ports); Component boxes a component pin layout.
fn apply_reserved_overrides(b: &mut McVecBox) {
    let is_module = b.kind == BoxKind::SubModule;
    let cls = b.class_name.clone();
    let layout = if is_module {
        module_pin_layout(&cls)
    } else {
        component_pin_layout(&cls)
    };
    if let Some(layout) = layout {
        b.set_layout_hint(layout);
        b.pin_constraint = PinConstraint::FixedOrder;
    }
    if !is_module {
        // Project custom symbols stay a component-only override (unchanged).
        if let Some(sym) = resolve_custom_symbol(&cls) {
            b.set_custom_symbol(sym);
        }
    }
}

/// ★ C1b: extract component value from class name and symbol.
///
/// When the component declaration doesn't provide a value parameter, there is
/// nothing real to print — earlier code invented "0R" for every default resistor,
/// which cluttered the schematic with a bogus value. Return `None` and let the
/// renderer draw only the designator.
fn extract_component_value(_class_name: &str, _symbol: &Symbol) -> Option<String> {
    None
}

/// ★ Reserved interface ①: query a component class's custom pin layout.
///
/// Looks up the component by class_name in workspace + global tables, reads
/// `comp.layout` (core `McLayout{left,right,top,bottom}`) and hands each edge's
/// member strings (pin numbers and/or function names) to drawing-side
/// [`PinLayout`] unchanged — matching downstream is plain string equality
/// against `pin_id` or the pin description.
///
/// Returns `None` when the component is not found or all four layout edges are
/// empty (the caller falls back to the default pin-arrangement heuristic).
pub(crate) fn component_pin_layout(class_name: &str) -> Option<PinLayout> {
    let comp = crate::db::cmie::tables::WORKSPACE.component_by_class(class_name)?;
    let layout = &comp.layout;
    if layout.is_empty() {
        return None;
    }
    Some(PinLayout {
        left: layout.left.clone(),
        right: layout.right.clone(),
        top: layout.top.clone(),
        bottom: layout.bottom.clone(),
    })
}

/// ★ Module variant of the reserved-interface ① lookup: query a module class's
/// boundary-port layout. SubModule boxes carry this hint so their ports land on
/// the requested edges when the module is instantiated. Mirrors
/// [`component_pin_layout`] over `workspace_modules()` + the defregistry system
/// module fallback.
fn module_pin_layout(class_name: &str) -> Option<PinLayout> {
    let module = module_by_class(class_name)?;
    let layout = &module.layout;
    if layout.is_empty() {
        return None;
    }
    Some(PinLayout {
        left: layout.left.clone(),
        right: layout.right.clone(),
        top: layout.top.clone(),
        bottom: layout.bottom.clone(),
    })
}

/// Look a module up by its class-name ident string — workspace view first,
/// then the defregistry system-module name index (mirror of
/// `component_by_class`).
fn module_by_class(class_name: &str) -> Option<Arc<McModule>> {
    for (sn, module) in crate::definition_space().workspace_modules() {
        if sn.ident.to_string() == class_name {
            return Some(module);
        }
    }
    for hit in crate::db::defregistry::system_name_hits(class_name) {
        if hit.kind != crate::db::defregistry::DefKind::Module {
            continue;
        }
        if let Some((_, def)) = crate::db::defregistry::live_entry_by_id(hit.id) {
            if let crate::db::defregistry::DefValue::Module(m) = def {
                return Some(m);
            }
        }
    }
    None
}

/// ★ Project SVG interface: query the validated project-local symbol registry by class name.
/// Missing, invalid, or undeclared symbols return `None` and keep the system renderer fallback.
fn resolve_custom_symbol(class_name: &str) -> Option<CustomSymbol> {
    super::psymbol::resolve_project_symbol(class_name)
}

/// Build a box from InstTable by id (shared by Phase 1 / Phase 1.5, avoids classification logic drift)
fn make_box_from_id(table: &InstTable, id: u32) -> Option<McVecBox> {
    let entry = table.get_entry(id)?;
    let name = extract_last_segment(&entry.path);
    match detect_kind(table, id) {
        DetectedKind::Component {
            pin_count,
            class_name,
        } => {
            let kind = if pin_count <= 2 {
                BoxKind::TwoPin
            } else {
                BoxKind::MultiPin
            };
            let pins = table.get_pins_of(id);
            let io = compute_io(&pins);
            let mut box_pins = build_box_pins(&pins, &class_name);
            // typed-chip (Phase F.1): no registered Pin children -> synthesize placeholder pins from the recorded count (if any)
            if box_pins.is_empty() && pin_count > 0 {
                box_pins = placeholder_pins(id as i64, pin_count);
            }
            let symbol = detect_symbol(table, id, &kind);
            let designator = extract_designator(&name);
            let inst_path = entry.path.clone();
            let scope_chain = compute_scope_chain(&inst_path);
            let mut b = McVecBox::new_v2(
                id as i64,
                name,
                class_name,
                kind,
                symbol,
                designator,
                None,
                pin_count,
                io,
                inst_path,
                scope_chain,
            );
            b.set_pins(box_pins);
            warn_if_pin_mismatch(&b);
            // ★ M11.3: propagate bridge passive intent from truth layer
            if table.is_bridge_passive(&entry.path) {
                b.visual_role = Some(VisualRole::BridgePassive);
            }
            apply_reserved_overrides(&mut b); // ★ Reserved: layout / custom symbol (default no-op)
                                              // ★ M0-B-D/E: pass through not_fitted / origin
            b.not_fitted = entry.not_fitted;
            b.origin = entry.origin.clone();
            b.synthetic = entry.synthetic;
            Some(b)
        }
        // ★ P7-6: DetectedKind::Label branch removed — Label entries are no longer
        // boxed anywhere. The only callers of make_box_from_id are backfill Component /
        // Module (which never produce DetectedKind::Label) and the net-endpoint path
        // (which only calls for Component/Module). If this branch is ever needed again,
        // the caller must be tracked in its comment.
        DetectedKind::SubModule {
            port_count,
            class_name,
        } => {
            let ports = table.get_ports_of(id);
            let io = compute_io(&ports);
            let box_pins = build_box_pins(&ports, &class_name);
            let inst_path = entry.path.clone();
            let scope_chain = compute_scope_chain(&inst_path);
            let mut b = McVecBox::new_v2(
                id as i64,
                name,
                class_name,
                BoxKind::SubModule,
                Symbol::Module,
                None,
                None,
                port_count,
                io,
                inst_path,
                scope_chain,
            );
            b.set_pins(box_pins);
            b.boundary_ports = boundary_ports_of(&ports);
            apply_reserved_overrides(&mut b); // ★ Reserved: module port layout
            b.synthetic = entry.synthetic;
            Some(b)
        }
        DetectedKind::PowerLabel => {
            // §5③: only synthesize a rail box for a DECLARED supply endpoint — the
            // is_ground bit comes from the member role, never the name; an entry
            // with no declared role yields no box. (Current callers pass only
            // Component/Module, which never classify as PowerLabel, so this arm is
            // effectively dead — kept declaration-driven for safety.)
            let is_ground = declared_rail_is_ground(entry)?;
            let inst_path = entry.path.clone();
            let scope_chain = compute_scope_chain(&inst_path);
            Some(McVecBox::new_v2(
                id as i64,
                name,
                String::new(),
                BoxKind::PowerLabel,
                Symbol::PowerRail { is_ground },
                None,
                None,
                0,
                IoSummary::new(),
                inst_path,
                scope_chain,
            ))
        }
        DetectedKind::Skip | DetectedKind::Label => None,
    }
}

// ============================================================================
// Main entry
// ============================================================================

/// Build `McVecGraph` from `McVecBlock` + `InstTable`
///
/// Top-level call (`is_top_level = true`) runs **P0-3**: synthesize undeclared power/ground
/// PowerLabels at the top level (typical scenario: the example project's main only declares V1V2/V3V3/V5V Ports,
/// no main.GND, but sub-modules all expose `GND` ports). Sub-graph recursion
/// (`is_top_level = false`) doesn't synthesize, avoiding adding a set of power symbols out of
/// thin air at every layer.
pub fn build_mc_vec_graph(block: &McVecBlock, table: &InstTable) -> McVecGraph {
    // ── ★ P7-2: pass2 → viz projection layer (viz/project.rs, the single mandatory gateway for all callers) ──
    // Cleanses three classes of netlist noise (scalar stub ∪ member nets /
    // duplicate endpoints on the same port / rail label pseudo-endpoints).
    // This is the only vector→viz reverse dependency: projection is a viz-side
    // policy and must take effect uniformly at the boundary.
    // Audit log: baseline/render_projection.md.
    let (projected, _projection_log) = crate::viz::project::project_block_tree(block, table);
    let graph = build_mc_vec_graph_inner(&projected, table, /*is_top_level=*/ true);
    graph
}

fn build_mc_vec_graph_inner(
    block: &McVecBlock,
    table: &InstTable,
    is_top_level: bool,
) -> McVecGraph {
    let root_name = if block.bid >= 0 {
        table
            .get_entry(block.bid as u32)
            .map(|e| extract_last_segment(&e.path))
            .unwrap_or_else(|| block.name.clone())
    } else {
        block.name.clone()
    };

    let mut graph = McVecGraph::new(block.bid, root_name.clone());
    graph.is_root = is_top_level;
    // ★ §8.9.4: coarse bus/interface trunks carried over from the semantic layer
    graph.port_trunks = block.port_trunks.clone();

    // ── ★ P7-9: build pin_parent map (member port id → parent port group id) ──
    // Used by the facade pass to collapse member ports (SCL/SDA → I2C0) to port groups.
    // Walk the block's port groups, then each port group's member ports.
    for pg in table.get_ports_of(block.bid as u32) {
        for member in table.get_ports_of(pg.id) {
            if member.kind == crate::instant::insttab::InstKind::Port {
                graph.pin_parent.insert(member.id as i64, pg.id as i64);
            }
        }
    }

    // ── Phase 1: block.insts -> boxes (duck typing recognition) ──
    let mut box_ids_set: std::collections::HashSet<u32> = std::collections::HashSet::new();

    // ── §5③: declared rail identity per net point ──
    // The projection layer has already filled `McVecNet.attr` (the declared supply
    // mirror) on every net. A PowerRail symbol is only synthesized for a point that
    // sits on a net with a declared identity (role Hot/Ret/Reference); name matching
    // never drives it. Index endpoint point id -> ground-side bit for the Phase 1
    // per-instance boxes that have no net in scope.
    let mut point_rail_is_ground: std::collections::HashMap<i64, bool> =
        std::collections::HashMap::new();
    for net in &block.nets {
        if let Some(g) = net.attr.as_ref().and_then(attr_rail_is_ground) {
            for pid in net.all_point_ids() {
                if pid >= 0 {
                    point_rail_is_ground.insert(pid, g);
                }
            }
        }
    }

    crate::velog!(
        "[graph] build_mc_vec_graph_inner: bid={}, block.insts has {} entries: {:?}",
        block.bid,
        block.insts.len(),
        &block.insts
    );

    for &iid in &block.insts {
        if iid < 0 {
            continue;
        }
        let id = iid as u32;
        // ★ M4-fix: the top-level module itself must not appear as a SubModule frame in the schematic
        // block.insts may contain the top-level module's own bid; detect_kind would classify it as SubModule
        if is_top_level && id == block.bid as u32 {
            continue;
        }
        if box_ids_set.contains(&id) {
            continue;
        }
        let entry = match table.get_entry(id) {
            Some(e) => e,
            None => {
                continue;
            }
        };
        let name = extract_last_segment(&entry.path);
        let detected = detect_kind(table, id);

        match detected {
            DetectedKind::Component {
                pin_count,
                class_name,
            } => {
                let kind = if pin_count <= 2 {
                    BoxKind::TwoPin
                } else {
                    BoxKind::MultiPin
                };
                let pins = table.get_pins_of(id);
                let io = compute_io(&pins);
                let mut box_pins = build_box_pins(&pins, &class_name);
                // typed-chip (Phase F.1): no registered Pin children -> synthesize placeholder pins from the recorded count (if any)
                if box_pins.is_empty() && pin_count > 0 {
                    box_pins = placeholder_pins(id as i64, pin_count);
                }
                // ★ P01: compute symbol / designator in one pass
                let symbol = detect_symbol(table, id, &kind);
                let designator = extract_designator(&name);
                let value: Option<String> = extract_component_value(&class_name, &symbol);
                crate::velog!(
                    "[graph] ✓ Component: {name} (class={class_name}, symbol={symbol}, pins={pin_count})"
                );
                let inst_path = entry.path.clone();
                let scope_chain = compute_scope_chain(&inst_path);
                let mut b = McVecBox::new_v2(
                    id as i64,
                    name,
                    class_name,
                    kind,
                    symbol,
                    designator,
                    value,
                    pin_count,
                    io,
                    inst_path,
                    scope_chain,
                );
                b.set_pins(box_pins);
                warn_if_pin_mismatch(&b);
                // ★ M11.3: propagate bridge passive intent from truth layer
                if table.is_bridge_passive(&entry.path) {
                    b.visual_role = Some(VisualRole::BridgePassive);
                }
                apply_reserved_overrides(&mut b); // ★ Reserved: layout / custom symbol
                                                  // ★ M0-B-D/E: pass through not_fitted / origin (the primary
                                                  // Phase 1 path used to drop them; only the backfill path via
                                                  // make_box_from_id copied them — S8 saw zero NC devices)
                b.not_fitted = entry.not_fitted;
                b.origin = entry.origin.clone();
                graph.boxes.push(b);
                box_ids_set.insert(id);
            }
            DetectedKind::SubModule {
                port_count,
                class_name,
            } => {
                let ports = table.get_ports_of(id);
                let io = compute_io(&ports);
                let box_pins = build_box_pins(&ports, &class_name);
                crate::velog!(
                    "[graph] ✓ SubModule: {name} (class={class_name}, ports={port_count})"
                );
                let inst_path = entry.path.clone();
                let scope_chain = compute_scope_chain(&inst_path);
                let mut b = McVecBox::new_v2(
                    id as i64,
                    name,
                    class_name,
                    BoxKind::SubModule,
                    Symbol::Module, // ★ P01
                    None,           // SubModule has no designator (it is a hierarchy name)
                    None,
                    port_count,
                    io,
                    inst_path,
                    scope_chain,
                );
                b.set_pins(box_pins);
                b.boundary_ports = boundary_ports_of(&ports);
                apply_reserved_overrides(&mut b); // ★ Reserved: module port layout
                graph.boxes.push(b);
                box_ids_set.insert(id);
            }
            DetectedKind::PowerLabel => {
                // §5③ (classification-retirement-design): a PowerRail symbol is only
                // synthesized for an endpoint with a DECLARED supply identity — the
                // member role (Ground/Power), else the declared rail net the point
                // belongs to (`point_rail_is_ground`). A name-power Label/Port with
                // no declaration is NOT drawn as a rail: never name-guess (①).
                let is_ground = declared_rail_is_ground(entry)
                    .or_else(|| point_rail_is_ground.get(&iid).copied());
                match is_ground {
                    Some(is_ground) => {
                        crate::velog!("[graph] ✓ PowerLabel: {name}");
                        let symbol = Symbol::PowerRail { is_ground };
                        let inst_path = entry.path.clone();
                        let scope_chain = compute_scope_chain(&inst_path);
                        graph.boxes.push(McVecBox::new_v2(
                            id as i64,
                            name,
                            String::new(),
                            BoxKind::PowerLabel,
                            symbol,
                            None,
                            None,
                            0,
                            IoSummary::new(),
                            inst_path,
                            scope_chain,
                        ));
                        box_ids_set.insert(id);
                    }
                    None => {
                        crate::velog!(
                            "[graph] skip undeclared power label '{name}' (id={id}): \
                             no declared supply role on entry or net"
                        );
                    }
                }
            }
            DetectedKind::Label => {
                // ★ P7-6: Label entries are literal views of port declarations, not
                // drawable boxes. Skip them — same as the backfill path.
                crate::velog!("[graph] Phase 1 skip Label: '{}' (id={})", name, id);
            }
            DetectedKind::Skip => {
                if entry.kind == InstKind::Bus {
                    for member in &table.children_of(id) {
                        // §5③: only declared-supply bus members (connection-point DC
                        // pair members carry a Ground/Power member role) become rail
                        // boxes; signal bus members (MIC{P,N} etc.) never do.
                        if !box_ids_set.contains(&member.id) {
                            if let Some(is_ground) = declared_rail_is_ground(member) {
                                let mname = extract_last_segment(&member.path);
                                crate::velog!("[graph] ✓ PowerLabel (bus member): {mname}");
                                let symbol = Symbol::PowerRail { is_ground };
                                let inst_path = member.path.clone();
                                let scope_chain = compute_scope_chain(&inst_path);
                                graph.boxes.push(McVecBox::new_v2(
                                    member.id as i64,
                                    mname,
                                    String::new(),
                                    BoxKind::PowerLabel,
                                    symbol,
                                    None,
                                    None,
                                    0,
                                    IoSummary::new(),
                                    inst_path,
                                    scope_chain,
                                ));
                                box_ids_set.insert(member.id);
                            }
                        }
                    }
                }
            }
        }
    }

    // ── ★ Phase 1.3: backfill all remaining children of the module that weren't in block.insts ─
    // This catches label entries (VCC/Vin) and fitted Components/Modules that are registered
    // in InstTable but weren't pushed into block.insts by the builder.
    // ★ M4-1B: recursively backfill Components/Modules at multiple levels (children,
    // grandchildren, great-grandchildren), mirroring the visit.rs backfill.
    fn backfill_children_recursive(
        graph: &mut McVecGraph,
        table: &InstTable,
        box_ids_set: &mut std::collections::HashSet<u32>,
        parent_id: u32,
        depth: u32,
    ) {
        const MAX_DEPTH: u32 = 3; // children, grandchildren, great-grandchildren
        if depth > MAX_DEPTH {
            return;
        }
        for child in table.children_of(parent_id) {
            if box_ids_set.contains(&child.id) {
                // Already has a box — still recurse into Components for nested fitted components.
                // ★ Do NOT recurse into Module (sub-module instances): their children are handled
                // by the sub-module's own graph construction.
                if matches!(child.kind, InstKind::Component) {
                    backfill_children_recursive(graph, table, box_ids_set, child.id, depth + 1);
                }
                continue;
            }
            match child.kind {
                InstKind::Label | InstKind::Bus | InstKind::Port | InstKind::Pin => {
                    // ★ P7-6: skip Label / Port / Pin / Bus — these are literal views of
                    // port declarations, not drawable boxes. Creating Dot boxes for them
                    // produces degree=0 pins=0 garbage that pollutes box-count accounting.
                    crate::velog!(
                        "[graph] Phase 1.3 skip: '{}' (id={}, kind={:?}) depth={}",
                        extract_last_segment(&child.path),
                        child.id,
                        child.kind,
                        depth
                    );
                }
                InstKind::Component => {
                    // ★ P8-6: skip func-created instances — they belong to component inner layers,
                    // not the parent layer. Their boxes are rendered in the sub-graph.
                    if matches!(
                        child.origin,
                        crate::instant::insttab::InstOrigin::FuncCall { .. }
                    ) {
                        crate::velog!(
                            "[graph] Phase 1.3 skip func-created: '{}' (id={}, depth={})",
                            extract_last_segment(&child.path),
                            child.id,
                            depth
                        );
                        continue;
                    }
                    // ★ M4-1B: backfill fitted components not in block.insts
                    if let Some(b) = make_box_from_id(table, child.id) {
                        crate::velog!(
                            "[graph] Phase 1.3 backfill: '{}' (id={}, kind={:?}) depth={}",
                            extract_last_segment(&child.path),
                            child.id,
                            child.kind,
                            depth
                        );
                        graph.boxes.push(b);
                        box_ids_set.insert(child.id);
                    }
                    // Recurse into children for nested fitted components (e.g. IC -> fitted CAP)
                    backfill_children_recursive(graph, table, box_ids_set, child.id, depth + 1);
                }
                InstKind::Module => {
                    // Module not in block.insts: create a box but do NOT recurse into children.
                    // The sub-module's children are handled by its own graph construction.
                    if let Some(b) = make_box_from_id(table, child.id) {
                        crate::velog!(
                            "[graph] Phase 1.3 backfill: '{}' (id={}, kind=Module) depth={}",
                            extract_last_segment(&child.path),
                            child.id,
                            depth
                        );
                        graph.boxes.push(b);
                        box_ids_set.insert(child.id);
                    }
                }
            }
        }
    }
    backfill_children_recursive(&mut graph, table, &mut box_ids_set, block.bid as u32, 0);

    // ── ★ P7-8: Phase 1.45 deleted (boundary terminalization replaces it with PortTerminal) ──

    // ── ★ P9-B: Phase 1.46 deleted for root layer.
    // Root layer only has declared boxes; no virtual border, no synthesized boxes. ──

    // ── Phase 1.5: supplement missing boxes from block.nets endpoints ──
    //
    // ★ Module-port drawing: the gate is `!is_block_diagram`, not `!is_top_level`.
    // A root layer is a block diagram only when it actually contains sub-module
    // boxes — the identical predicate api.rs computes before choosing a layouter.
    // A module opened on its own has a root that is *not* a block diagram; it is
    // the same schematic as the same module reached as a sub-layer, so it takes
    // the same boundary treatment. One strategy: the picture of a module never
    // depends on whether it was opened on its own or expanded inside its project.
    let is_block_diagram = is_top_level && graph.boxes.iter().any(|b| b.kind == BoxKind::SubModule);
    if !is_block_diagram {
        //
        // ## Key: 3 cases when endpoint doesn't belong to a known box
        //
        // **Case A**: endpoint's parent is a Component (@?Cap_1.2's parent = @?Cap_1), but this
        // Component isn't in box_ids_set -> visit.rs missed adding it to block.insts (pass2 registration
        // issue). **Synthesize a Component box** so it can be drawn, instead of treating the endpoint
        // itself as PowerLabel.
        //
        // **Case B**: the endpoint itself is a real power/ground label (VCC/GND/V3V3/...). Synthesize
        // a PowerLabel.
        //
        // **Case C**: the endpoint is a child of some Bus / Port (SPI.CSN, MIC{P,N}.P etc.) and is not a
        // power name. **Skip, don't forcibly create a PowerLabel** (previous bug -- drew CSN/MOSI/10/XTAL
        // all as power).
        //
        // ## Old logic before S3.5
        // The old check was `kind == Label || kind == Bus || is_power_rail(name)` -> too broad,
        // any Label/Bus kind endpoint became PowerLabel. pass2 registers SPI sub-ports as Label,
        // all were wrongly drawn as power.
        for net in &block.nets {
            for pid in net.all_point_ids() {
                if pid < 0 {
                    continue;
                }
                let u = pid as u32;
                if box_ids_set.contains(&u) {
                    continue;
                }
                let entry = match table.get_entry(u) {
                    Some(e) => e,
                    None => continue,
                };

                // Endpoint belongs to some existing box -> skip
                if let Some(parent_id) = entry.parent_id {
                    if box_ids_set.contains(&parent_id) {
                        continue;
                    }

                    // ★ S3.5 Fix C: parent is a Component but not in box_ids_set
                    // -> visit.rs didn't include it in insts. Synthesize Component box here.
                    if let Some(parent_entry) = table.get_entry(parent_id) {
                        if parent_entry.kind == InstKind::Component
                            && !box_ids_set.contains(&parent_id)
                        {
                            let parent_name = extract_last_segment(&parent_entry.path);
                            let pins = table.get_pins_of(parent_id);
                            let pin_count = pins.len();
                            let kind = if pin_count <= 2 {
                                BoxKind::TwoPin
                            } else {
                                BoxKind::MultiPin
                            };
                            let symbol = Symbol::from_class_name(&parent_entry.class_name)
                                .unwrap_or(Symbol::Unknown);
                            let designator = super::detect::extract_designator(&parent_name);
                            let io = compute_io(&pins);
                            let box_pins = build_box_pins(&pins, &parent_entry.class_name);
                            crate::velog!(
                                "[graph] ✓ Synthesized Component (from net endpoint): {} \
                             (class={}, symbol={}, pins={}) -- visit.rs missed this",
                                parent_name,
                                parent_entry.class_name,
                                symbol,
                                pin_count
                            );
                            let inst_path = parent_entry.path.clone();
                            let scope_chain = compute_scope_chain(&inst_path);
                            let mut b = McVecBox::new_v2(
                                parent_id as i64,
                                parent_name,
                                parent_entry.class_name.clone(),
                                kind,
                                symbol,
                                designator,
                                None,
                                pin_count,
                                io,
                                inst_path,
                                scope_chain,
                            );
                            b.set_pins(box_pins);
                            // ★ M11.3: propagate bridge passive intent from truth layer
                            if table.is_bridge_passive(&parent_entry.path) {
                                b.visual_role = Some(VisualRole::BridgePassive);
                            }
                            // ★ P7-1: Phase 1.5 Case A synthesized box, countable by G10
                            b.provenance = super::boxdef::BoxProvenance::SynthesizedFromEndpoint;
                            graph.boxes.push(b);
                            box_ids_set.insert(parent_id);
                            continue;
                        }
                    }
                }

                // ── ★ ITER-3: sub-module internal Port/Label walk-up lift ─────────────────────────
                //
                // Trigger scenario: top-level net references an external signal endpoint inside a
                // SubModule, e.g.
                //   - `main.mcu.SPI/SCLK`   (kind=Label, parent=mcu.SPI Port, 1012)
                //   - `main.mcu.UART0`     (kind=Port,  parent=mcu,           1007)
                //   - `main.mcu.DAC_OUT`   (kind=Port,  parent=mcu,           1007)
                //   - `main.mcu.SPK_MUTE`  (kind=Port,  parent=mcu,           1007)
                //
                // Old logic only checked if the **direct parent** (above line 247-250) was a known box
                // -- for `SPI/SCLK` type, the direct parent is `mcu.SPI` Port (id 1012) not in
                // box_ids_set, so it doesn't continue. Then Fix C only handles Component parent, not
                // Port parent. Finally falling into the "looks_like_power / looks_like_bus_label"
                // check, all false -> prints `✗ Skipping unresolved endpoint`, leaving a bunch of
                // misleading warnings.
                //
                // Actually Phase 2's `build_point_to_box` will BFS through all descendants of each
                // SubModule box, mapping `SPI` Port (1012), `SPI/SCLK` Label (1060) all back to the
                // SubModule box (1007), Phase 3 thus correctly builds VizNet. This means Phase 1.5's
                // "✗ Skipping" log **is functionally wrong** -- these endpoints aren't really lost,
                // they just don't have an independent box.
                //
                // This ITER-3 fix does two things:
                //   1. Walk up the ancestor chain, once hits an ancestor in box_ids_set (typically a
                //      SubModule), explicitly continue, printing `✓ Lifted to ancestor box` instead of
                //      `✗ Skipping`, making the log clear about "the endpoint actually has ownership".
                //   2. Prevent the power-label check below from wrongly drawing endpoints that should
                //      belong to a SubModule as floating PowerLabels (e.g. a sub-module exposes a Port
                //      named `VDD_ANALOG`, it **should** belong to that sub-module, not be drawn as
                //      a floating triangle).
                //
                // Note: this step doesn't change the actual graph topology -- Phase 2 BFS already
                // handles it. But the logs and subsequent box creation paths become correct, and it
                // sets up a hook for the future "label pin names (DAC_OUT/SPK_MUTE) on SubModule edges
                // instead of anonymous __net_N labels".
                if let Some(parent_id) = entry.parent_id {
                    // Walk up starting from parent (parent itself was already handled by the
                    // box_ids_set check at line 248, here we handle "grandparent or higher").
                    const MAX_HOPS: u32 = 16; // defensive upper limit, prevent InstTable circular references
                    let mut cursor: Option<u32> =
                        table.get_entry(parent_id).and_then(|p| p.parent_id);
                    let mut hit_ancestor: Option<(u32, u32)> = None; // (anc_id, hops)
                    let mut hops: u32 = 0;
                    while let Some(anc_id) = cursor {
                        hops += 1;
                        if hops > MAX_HOPS {
                            crate::velog!(
                                "[graph] ⚠ ITER-3 lift: ancestor walk exceeded {} hops for '{}', \
                             aborting (suspect cycle in InstTable parent chain)",
                                MAX_HOPS,
                                entry.path
                            );
                            break;
                        }
                        if box_ids_set.contains(&anc_id) {
                            hit_ancestor = Some((anc_id, hops));
                            break;
                        }
                        cursor = table.get_entry(anc_id).and_then(|e| e.parent_id);
                    }
                    if let Some((anc_id, h)) = hit_ancestor {
                        let anc_name = table
                            .get_entry(anc_id)
                            .map(|e| extract_last_segment(&e.path))
                            .unwrap_or_else(|| format!("id={anc_id}"));
                        crate::velog!(
                        "[graph] ✓ ITER-3 lifted endpoint '{}' (kind={:?}) -> ancestor box '{}' (id={}, hops={}) \
                         -- Phase 2 BFS will map this point to the ancestor",
                        entry.path, entry.kind, anc_name, anc_id, h
                    );
                        // Don't push box, don't insert box_ids_set -- Phase 2 BFS handles naturally.
                        continue;
                    }
                }

                let name = extract_last_segment(&entry.path);

                // ★ FIX: endpoint itself is a Component/Module (uC/X6/ldo/spk...) -> directly create a box,
                // not treat as "unresolvable" and discard (old logic only handled "endpoint's parent is Component")
                if matches!(entry.kind, InstKind::Component | InstKind::Module) {
                    if let Some(b) = make_box_from_id(table, u) {
                        crate::velog!(
                            "[graph] ✓ Box from net endpoint (self is {:?}): {}",
                            entry.kind,
                            name
                        );
                        graph.boxes.push(b);
                        box_ids_set.insert(u);
                    }
                    continue;
                }

                // ★ S3.5 Fix B + §5③ (classification-retirement-design): only create a
                // PowerRail in two cases:
                //   (1) the endpoint sits on a net with a DECLARED supply identity
                //       (`net.attr` role Hot/Ret/Reference) — the name is irrelevant;
                //   (2) Bus kind and name is signal-like (entire bus as label, like MIC{P,N})
                // An undeclared net (attr None → Signal) is never drawn as a rail,
                // however power-like its name (ruling ①). Pure Label kind (especially
                // SPI/UART sub-ports CSN/MOSI/10) is no longer misjudged.
                let rail_is_ground = net.attr.as_ref().and_then(attr_rail_is_ground);
                let looks_like_bus_label =
                    entry.kind == InstKind::Bus && naming::is_signal_like(&name);
                if rail_is_ground.is_none() && !looks_like_bus_label {
                    // ── ★ Phase E.1: sub-layer edge endpoints -> boundary label box ────────────
                    //
                    // Trigger scenario: **non-top-level** sub-layer (block.bid is some SubModule), the
                    // endpoint's ancestor chain can walk all the way up to `block.bid` itself (i.e.
                    // the endpoint is this layer's own external interface or internal named signal),
                    // but ITER-3 can't find any box in between (because the sub-layer's box_ids_set
                    // contains mcu's children: CAP/RES/uC etc., not including mcu itself).
                    //
                    // Old logic: such endpoints would fall to `✗ Skipping unresolved endpoint`, the
                    // sub-layer render loses mcu's own Port/Label edge labels, drill-down sees
                    // a bunch of dangling connections (user feedback "second level has issues").
                    //
                    // Examples (mcu inner layer, block.bid=1010):
                    //   - `main.mcu.UART0`        Port,  parent=1010 -> direct hit
                    //   - `main.mcu.DAC_OUT`      Port,  parent=1010 -> direct hit
                    //   - `main.mcu.[VCC_1V2, GND]` Port,  parent=1010 -> direct hit
                    //   - `main.mcu.SPI/SCLK`     Label, parent=1015 (SPI Port), \
                    //                                       grandparent=1010 -> two-hop hit
                    //   - `main.mcu.AVDD09_CAP`   Label, parent=1010 -> direct hit
                    //                                                    (internal signal label)
                    //
                    // Fix: after hit, create a PowerLabel (actually "boundary label" reusing the same
                    // BoxKind, visually an arrow + name, suitable for Port label semantics) so that
                    // Phase 2 BFS can map the corresponding connection endpoints to this box, drill-down
                    // no longer loses labels.
                    //
                    // ★ M4-fix: the top-level module also needs boundary labels. Previously
                    // !is_top_level kept top-level ports from creating boundary label boxes,
                    // so ports like DAC_OUT/MIC.N lost endpoints in Phase 3. The top-level
                    // module has no SubModule frame (Phase 1.45 skips it), so ports could
                    // not map to any frame.
                    if block.bid >= 0 {
                        const MAX_HOPS_E1: u32 = 16;
                        let layer_bid = block.bid as u32;
                        let mut cursor: Option<u32> = entry.parent_id;
                        let mut hops: u32 = 0;
                        let mut reaches_layer = false;
                        while let Some(c) = cursor {
                            hops += 1;
                            if hops > MAX_HOPS_E1 {
                                break;
                            }
                            if c == layer_bid {
                                reaches_layer = true;
                                break;
                            }
                            cursor = table.get_entry(c).and_then(|e| e.parent_id);
                        }
                        if reaches_layer {
                            // ★ C1b F3: PowerLabel boundary labels are no longer created.
                            // Equipotential tree rendering handles Port terminals as tree symbols.
                            crate::velog!(
                                "[graph] Phase-E1 skip boundary label: '{}' (kind={:?}) -> tree symbol",
                                entry.path,
                                entry.kind,
                            );
                            continue;
                        }
                    }

                    crate::velog!(
                        "[graph] ✗ Skipping unresolved endpoint '{}' (kind={:?}, parent_id={:?}) \
                     -- not a power rail / not a bus label / parent not a Component. \
                     This endpoint will not have a box drawn for it.",
                        entry.path,
                        entry.kind,
                        entry.parent_id
                    );
                    continue;
                }

                // ★ §5③ (classification-retirement-design): this endpoint is the net's
                // boundary port group. It is a **name on the module's boundary**, not a
                // supply this layer consumes, so it is never drawn as a rail symbol
                // inside the layer. The boundary draws it instead — the parent box's
                // lead, or the module frame of the layer's own drawing (module-port
                // drawing, `mcd/doc/viz/module-port-drawing-design.md`). P7-8 used to
                // mint a `PortTerminal` box for exactly this id; that producer is
                // retired, so skipping here is the whole answer rather than a deferral.
                if let Some(ref bi) = net.boundary {
                    if bi.port_group_id == u as i64 {
                        crate::velog!(
                            "[graph] ✓ PowerLabel skipped: boundary port group '{}' (id={}) drawn by the module boundary",
                            entry.path,
                            u
                        );
                        continue;
                    }
                }

                crate::velog!(
                    "[graph] ✓ PowerLabel (from net endpoint): {} (kind={:?})",
                    name,
                    entry.kind
                );
                let symbol = Symbol::PowerRail {
                    // §5③: is_ground comes from the declared net attr (Ret/Reference =
                    // ground side); the signal-bus-label branch above never is ground.
                    is_ground: rail_is_ground.unwrap_or(false),
                };
                let inst_path = entry.path.clone();
                let scope_chain = compute_scope_chain(&inst_path);
                let mut b = McVecBox::new_v2(
                    u as i64,
                    name,
                    String::new(),
                    BoxKind::PowerLabel,
                    symbol,
                    None,
                    None,
                    0,
                    IoSummary::new(),
                    inst_path,
                    scope_chain,
                );
                // ★ P7-1: Phase 1.5 generic PowerLabel synthesized box, countable by G10
                b.provenance = super::boxdef::BoxProvenance::SynthesizedFromEndpoint;
                graph.boxes.push(b);
                box_ids_set.insert(u);
            }
        }
    } // ★ Module-port drawing: end of !is_block_diagram guard for Phase 1.5

    let mut count_by_kind = [0usize; 6]; // TwoPin/MultiPin/SubModule/PowerLabel/Dot/PortTerminal
    for b in &graph.boxes {
        let i = match b.kind {
            BoxKind::TwoPin => 0,
            BoxKind::MultiPin => 1,
            BoxKind::SubModule => 2,
            BoxKind::PowerLabel => 3,
            BoxKind::Dot => 4,
            BoxKind::PortTerminal => 5,
        };
        count_by_kind[i] += 1;
    }
    crate::velog!(
        "[graph] '{}' box inventory: total={}, TwoPin={}, MultiPin={}, SubModule={}, PowerLabel={}, PortTerminal={}",
        root_name,
        graph.boxes.len(),
        count_by_kind[0],
        count_by_kind[1],
        count_by_kind[2],
        count_by_kind[3],
        count_by_kind[5],
    );
    if !graph.boxes.is_empty() && count_by_kind[0] + count_by_kind[1] + count_by_kind[2] == 0 {
        crate::velog!(
            "[graph] '{}' WARNING: all {} boxes are PowerLabel -- \
             likely visit.rs missed components or Phase 1.5 misclassified endpoints",
            root_name,
            graph.boxes.len()
        );
    }

    // ── ★ P7-8: PortTerminal creation — retired (module-port drawing) ────────────
    //
    // This step used to mint one `BoxKind::PortTerminal` per boundary port group of
    // every **non-root** layer. It was written when a sub-layer was itself drawn as
    // a block diagram; C1b then moved every sub-layer to the device (equipotential
    // tree) pipeline (`api.rs`), which paints tree symbols and never a box — so the
    // boxes it kept minting were invisible, while still taking a layout slot and
    // inflating the canvas bbox.
    //
    // Measured on hbl, LDO layer: 875px of canvas around 350px of content, and — the
    // defect the module-port strategy exists to kill — the *same* module drawn
    // standalone came out 366px. Same module, two drawings.
    //
    // The module's boundary is drawn now, by design, in the two places it is seen
    // (`mcd/doc/viz/module-port-drawing-design.md`):
    //   * the **parent's** block diagram — the sub-module box's leads, named by the
    //     port each wire crosses (`McVecBox::boundary_ports` + `render/sub_module.rs`);
    //   * the module's **own** layer — the dashed boundary frame with the ports on it
    //     (`viz::layout::module_frame`, from the same `BoundaryInfo` marker).
    // A root block diagram is not covered by either, and minting its own ports here
    // (main's ports have nothing above them to be the boundary of) put a dozen
    // terminals at the canvas origin with colliding labels — measured on pwrint main.
    // So no layer mints one today; `BoxKind::PortTerminal` survives as a rendering
    // kind with no producer, which is the honest state until a root boundary frame
    // is designed.

    // ── Phase 2: build point_to_box mapping ──
    let point_to_box = build_point_to_box(table, &graph.boxes);

    crate::velog!(
        "[graph] Phase 2 done: {} point->box mappings across {} boxes",
        point_to_box.len(),
        graph.boxes.len(),
    );

    // ── D4: GHOST_PORT detection (box-level) ────────────────────────────
    // Scan boxes for placeholder pins (id ≥ 8e9) that were synthesized
    // because the component declared only an estimated pin count (pins = N)
    // without actual pin definitions. These placeholder pins represent
    // unmapped ghost ports.
    for b in &graph.boxes {
        for p in &b.pins {
            if p.id >= 8_000_000_000 {
                crate::db::diagnostic::diagnostic::diagnostic_log(
                    crate::errcodes::GHOST_PORT_BOX,
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                    0,
                    1,
                    &crate::errcodes::format_msg(
                        crate::errcodes::GHOST_PORT_BOX,
                        &[
                            &b.name,
                            &b.id as &dyn std::fmt::Display,
                            &p.pin_id,
                            &p.id as &dyn std::fmt::Display,
                        ],
                    ),
                    &[],
                );
            }
        }
    }

    // ── ★ Phase 3: VizNet (only network model after P03) ──
    //
    // Keep multi-endpoint topology directly, no longer split into "pairwise" pairs.

    // ★ DEBUG: print block.nets structure
    graph.nets = generate_viznets_from_block(block, &point_to_box, table, &graph.boxes);

    // ★ Node conservation probe: building the graph must not change electrical facts.
    // Every net on the block side must have its endpoint set appear verbatim in some VizNet.
    probe_node_conservation(block, &graph.nets, &point_to_box);

    crate::velog!(
        "[graph] Phase 3 done: {} VizNet(s) generated (hyperedge model)",
        graph.nets.len()
    );

    // ── ★ P7-3: Phase 3.5 (same-name label synthesis of rail/signal nets) deleted ───────────
    // It was a pure name-matching machine (anti-pattern §2.3 "name as criterion"),
    // and after P7-2 projection it could only produce fake nets duplicating real
    // ones (measured on the main layer: MIC/[GND,VCC_1V2]/DAC_OUT/POWER_SYS all
    // duplicate __net_32/34/V5V.VCC). Cross-module connections are carried by the
    // projected real nets.

    // ── M0-2: populate module_ports from port declarations ──
    {
        let ports = table.get_ports_of(block.bid as u32);
        let mut module_ports = Vec::with_capacity(ports.len());
        for p in &ports {
            let port_name = extract_last_segment(&p.path);
            let port_dir = translate_io_to_port_dir(&p.io_type);
            let role = match &p.member_info {
                Some(mi) => match mi.role {
                    crate::instant::insttab::MemberRole::Power
                    | crate::instant::insttab::MemberRole::Ground => NetRole::Rail {
                        volt: mi.voltage.as_ref().map(|v| v.to_string()),
                    },
                    _ => NetRole::Signal,
                },
                None => NetRole::Signal,
            };
            module_ports.push((port_name, port_dir, role));
        }
        graph.module_ports = module_ports;
    }

    // ── M0-B-D/E: log summary of not_fitted / origin ──
    {
        let not_fitted_count = graph.boxes.iter().filter(|b| b.not_fitted).count();
        let not_fitted_names: Vec<&str> = graph
            .boxes
            .iter()
            .filter(|b| b.not_fitted)
            .map(|b| b.name.as_str())
            .collect();
        let declared = graph
            .boxes
            .iter()
            .filter(|b| matches!(b.origin, crate::instant::insttab::InstOrigin::Declared))
            .count();
        let funcall = graph.boxes.len() - declared;
        let mut fcall_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for b in &graph.boxes {
            if let crate::instant::insttab::InstOrigin::FuncCall { ref fn_name, .. } = b.origin {
                *fcall_counts.entry(fn_name.clone()).or_insert(0) += 1;
            }
        }
        let fcall_summary: Vec<String> = fcall_counts
            .iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        crate::velog!(
            "[graph] NOT-FITTED: {not_fitted_count} box(es) — {}",
            not_fitted_names.join(" ")
        );
        crate::velog!(
            "[graph] ORIGIN: declared={declared} funcall={funcall} ({})",
            fcall_summary.join(" ")
        );
    }

    // ── Phase 4: recursively process block.blocks ──
    for sub in &block.blocks {
        graph.sub_graphs.push(build_mc_vec_graph_inner(
            sub, table, /*is_top_level=*/ false,
        ));
    }

    graph
}

/// Smart build (equivalent to `build_mc_vec_graph`, keeps API compatibility)
pub fn build_graph_smart(block: &McVecBlock, table: &InstTable) -> McVecGraph {
    build_mc_vec_graph(block, table)
}

// ============================================================================
// ★ NEW: VizNet generation (multi-endpoint hyperedge)
// ============================================================================

/// Directly construct [`VizNet`] list from `McVecBlock.nets`
///
/// Differences from `generate_edges_from_net`:
/// - No pairwise splitting
/// - One VizNet per net, all endpoints preserved
/// - Auto-classify NetKind (Power / Ground / Signal)
///
/// ## ★ P01 (S2) Changes
/// Endpoints fetched from InstTable, IOType translated to `IoDirection`, numeric pin number
/// extracted from pin name, filled in one go with `EndpointRef::full(...)`. Previously before
/// P03 these two fields were both Unknown / None.
fn generate_viznets_from_block(
    block: &McVecBlock,
    point_to_box: &HashMap<u32, u32>,
    table: &InstTable,
    boxes: &[McVecBox],
) -> Vec<VizNet> {
    let mut out = Vec::with_capacity(block.nets.len());

    // ★ Set of discrete two-terminal passive boxes. A bus never passes through the middle of an R/C,
    //   so "the net touches a passive device" is a reliable signal that "this is not a bus".
    //   (Same criterion as the net-labeling guard at rails.rs:331.)
    //   ★ M0-C BLOCKED: after M0-A lands, this heuristic should read
    //   NetShape.series_chain instead: M0-A makes ConnPair carry a `via` field
    //   and merge_pairs_to_vecnet fills NetShape.series_chain, turning "which
    //   two-terminal devices a net passes through" into a source-code fact
    //   rather than an inference.
    let passive_boxes: std::collections::HashSet<i64> = boxes
        .iter()
        .filter(|b| b.is_two_pin_passive())
        .map(|b| b.id)
        .collect();
    let touches_passive = |ids: &[i64]| -> bool {
        ids.iter().any(|pid| {
            point_to_box
                .get(&(*pid as u32))
                .map(|&b| passive_boxes.contains(&(b as i64)))
                .unwrap_or(false)
        })
    };

    /// Extract N:N bus width from NetShape groups.
    /// Returns `Some(n)` if shape represents a true N:N bus (both sides same width > 1),
    /// `None` otherwise.
    fn bus_width_from_shape(shape: &NetShape) -> Option<usize> {
        if shape.groups.len() != 2 {
            return None;
        }
        let left_n = match &shape.groups[0] {
            GroupRole::Broadcast(n) => *n,
            GroupRole::Scalar => 1,
        };
        let right_n = match &shape.groups[1] {
            GroupRole::Broadcast(n) => *n,
            GroupRole::Scalar => 1,
        };
        if left_n == right_n && left_n > 1 {
            Some(left_n)
        } else {
            None
        }
    }

    /// Is this net really a bus?
    ///
    /// NetShape-first: when shape is present, use groups to determine, no longer
    /// rely on `connection_type()` shape inference. Only falls back to
    /// `connection_type()` when shape is absent (legacy behavior).
    /// ★ M0-C BLOCKED: this function will be deleted once M0-A completes.
    ///   At that point NetRole::Bus is filled directly by M0-A's NetShape (M0-B),
    ///   and the NtoN split and Bus upgrade branches read `net.role == NetRole::Bus` instead.
    ///   NetShape coverage is currently insufficient; keep this heuristic as a fallback for now.
    fn is_real_bus(
        net: &McVecNet,
        kind: &NetKind,
        touches_passive: &dyn Fn(&[i64]) -> bool,
    ) -> Option<usize> {
        if matches!(kind, NetKind::Power | NetKind::Ground) {
            return None;
        }

        // ★ P3.1: NetShape-first bus detection
        if let Some(shape) = &net.shape {
            if let Some(n) = bus_width_from_shape(shape) {
                if n > 1 && !touches_passive(&net.all_point_ids()) {
                    return Some(n);
                }
            }
            return None; // shape present but not N:N → not a bus
        }

        // Legacy fallback: no shape provenance (W2901 SHAPE_INCOMPLETE).
        // Stage 3 guarantees this is rarely triggered — only on paths that
        // have not yet been covered by `build_net_shape`.
        #[allow(deprecated)]
        if let ConnectionType::NtoN(n) = net.connection_type() {
            tracing::warn!(
                target: "mcc::vector",
                code = crate::errcodes::SHAPE_INCOMPLETE,
                net = %net.name,
                "W2901 SHAPE_INCOMPLETE: net '{}' has no NetShape provenance; fell back to connection_type() inference",
                net.name
            );
            if n > 1 && !touches_passive(&net.all_point_ids()) {
                return Some(n);
            }
        }
        None
    }

    // Endpoint construction helper (from point_id get box / pin name / io / pin number).
    let make_endpoint = |pid: i64| -> Option<EndpointRef> {
        if pid < 0 {
            return None;
        }
        let u = pid as u32;
        let box_id = point_to_box.get(&u).map(|&bid| bid as i64)?;
        let (pin_name, io_type, pin_number) = match table.get_entry(u) {
            Some(e) => {
                let n = extract_last_segment(&e.path);
                let io = translate_io_type(&e.io_type);
                let pn = parse_pin_number(&n);
                (n, io, pn)
            }
            None => (String::new(), IoDirection::Unknown, None),
        };
        Some(EndpointRef::full(
            box_id, pid, pin_name, io_type, pin_number,
        ))
    };

    // ★ SPI expansion: construct port's child members (SCLK/MOSI/...) as endpoints, box reuses parent port's box.
    //   (Child members usually aren't in point_to_box -- they're not top-level net endpoints, so separately mapped to parent box.)
    //
    // ★ M0-C BLOCKED: this branch will be deleted once M-1 completes.
    //   Its reason for existence is "top-level mcu.SPI collapsed into a single
    //   point" —— after M-1-1 fixes vector reference expansion, mcu.SPI will
    //   be 4 independent endpoints at the main layer, and this expansion branch
    //   is no longer needed.
    let make_child_endpoint = |child_id: i64, box_id: i64| -> EndpointRef {
        let (name, io, pn) = match table.get_entry(child_id as u32) {
            Some(e) => {
                let n = extract_last_segment(&e.path);
                let pn = parse_pin_number(&n);
                (n, translate_io_type(&e.io_type), pn)
            }
            None => (String::new(), IoDirection::Unknown, None),
        };
        EndpointRef::full(box_id, child_id, name, io, pn)
    };

    // Split-out member nets need unique nids -> increment from above all original nids, avoiding collisions.
    let mut synth_nid = block.nets.iter().map(|n| n.nid).max().unwrap_or(0) + 1;

    for net in &block.nets {
        // ── ★ SPI expansion: collapsed Port/Bus (1 point) <-> n peer pins -> n 2-point Signal nets ──
        //   Top-level mcu.SPI is a collapsed Port (single "spi" pin), flash side is n independent pins (Broadcast).
        //   Extract the Port's n signal members, pair them positionally with peer n pins into n point-to-point Signal nets
        //   -> visually n independent straight lines, not 1 pin fan-out / brown bus trunk.
        //   Defense: only expand when (collapsed side is indeed Port/Bus with >= n signal members, peer side exactly n pins, box mappable);
        //   otherwise do nothing, fall to the regular construction below (don't drop net).
        {
            let groups: Vec<Vec<i64>> = net.nets.iter().map(|v| v.ids().to_vec()).collect();
            if groups.len() == 2 {
                let (one_idx, many_idx) = if groups[0].len() == 1 && groups[1].len() >= 2 {
                    (0usize, 1usize)
                } else if groups[1].len() == 1 && groups[0].len() >= 2 {
                    (1usize, 0usize)
                } else {
                    (usize::MAX, usize::MAX)
                };
                if one_idx != usize::MAX {
                    let port_pid = groups[one_idx][0];
                    let many = &groups[many_idx];
                    let n = many.len();
                    let kind0 = naming::classify_net(&net.name);
                    let is_busport = table
                        .get_entry(port_pid as u32)
                        .map(|e| matches!(e.kind, InstKind::Port | InstKind::Bus))
                        .unwrap_or(false);
                    if is_busport
                        && !matches!(kind0, NetKind::Power | NetKind::Ground)
                        && !touches_passive(&net.all_point_ids())
                    {
                        let port_box = point_to_box.get(&(port_pid as u32)).map(|&b| b as i64);
                        // Port's signal members (in declaration order), filter out power/ground names
                        let members: Vec<i64> = table
                            .children_of(port_pid as u32)
                            .into_iter()
                            .filter(|c| !naming::is_power_rail(&extract_last_segment(&c.path)))
                            .map(|c| c.id as i64)
                            .collect();
                        if let Some(pbox) = port_box {
                            if members.len() >= n {
                                let mut ok = true;
                                let mut split: Vec<(String, Vec<EndpointRef>)> = Vec::new();
                                for (i, &peer) in many.iter().enumerate() {
                                    let mep = make_child_endpoint(members[i], pbox);
                                    match make_endpoint(peer) {
                                        Some(pe) => {
                                            let nm = if !mep.pin_name.is_empty() {
                                                mep.pin_name.clone()
                                            } else {
                                                net.name.clone()
                                            };
                                            split.push((nm, vec![mep, pe]));
                                        }
                                        None => {
                                            ok = false;
                                            break;
                                        }
                                    }
                                }
                                if ok && split.len() == n {
                                    for (i, (nm, eps)) in split.into_iter().enumerate() {
                                        let nid = if i == 0 {
                                            net.nid
                                        } else {
                                            let x = synth_nid;
                                            synth_nid += 1;
                                            x
                                        };
                                        out.push(VizNet::new(
                                            nid,
                                            nm,
                                            NetKind::Signal,
                                            NetRole::Signal,
                                            eps,
                                        ));
                                        // ★ P9-A2: propagate provenance to split nets
                                        if let Some(ref ss) = net.source_span {
                                            out.last_mut().unwrap().source_span = Some(ss.clone());
                                        }
                                        if let Some(ref pg) = net.trunk {
                                            out.last_mut().unwrap().trunk = Some(pg.clone());
                                        }
                                        if let Some(d) = net.trunk_ref {
                                            out.last_mut().unwrap().trunk_ref = Some(d);
                                        }
                                        if let Some(shape) = &net.shape {
                                            out.last_mut().unwrap().shape = Some(shape.clone());
                                        }
                                    }
                                    crate::velog!(
                                        "[graph] ✓ expanded collapsed bus/port '{}' -> {} signal nets",
                                        net.name, n
                                    );
                                    continue; // already expanded -> skip subsequent construction for this net
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── ★ NtoN(n) bus -> split into n independent point-to-point Signal nets ──
        //   When a bundle interface is expanded in sub-graph, each side is n **independent pins** (NtoN: aligned shape,
        //   member i <-> member i). Old logic promoted the whole to NetKind::Bus(n) -> BusBundle draws as "trunk + taps"
        //   thick line, multiple ones stacked together look like a braided tree. Here changed to: each end of member i
        //   connects into a 2-point Signal net, each goes its own orthogonal line, no more merged trunk.
        //   Note: collapsed ports (1 pin -> n flags) in main graph are Broadcast(n), not NtoN, so don't enter
        //   this branch -> doesn't affect main graph; only true "both sides expanded to n pins" gets split. Power/ground not split.
        #[allow(deprecated)]
        if let ConnectionType::NtoN(_n) = net.connection_type() {
            let kind0 = naming::classify_net(&net.name);
            // ★ FIX: `connection_type()` only compares the **lengths** of the two groups (net.rs:87),
            // but the two groups are a byproduct of net merging —— when the endpoints of an
            // equipotential node formed by merging multiple connections happen to form [n, n], it
            // is misjudged as an n-bit bus. Observed: the 4-point node
            // `@CAP5.2 ~ @RES6.2 ~ @CAP2.2 ~ u2.6` was split into two unconnected nets
            // `@CAP2.2~@RES6.2` and `@CAP5.2~u2.6` —— the node no longer exists, which is a rewrite
            // of electrical facts, not a layout preference.
            // Criterion: a real bus never passes through a discrete two-terminal passive device (see is_real_bus()).
            if let Some(n) = is_real_bus(net, &kind0, &touches_passive) {
                let group_a: Vec<i64> = net.nets[0].iter().copied().collect();
                let group_b: Vec<i64> = net.nets[1].iter().copied().collect();
                if group_a.len() == n && group_b.len() == n {
                    let mut split_ok = true;
                    let mut members: Vec<(String, Vec<EndpointRef>)> = Vec::new();
                    for (a, b) in group_a.iter().zip(group_b.iter()) {
                        match (make_endpoint(*a), make_endpoint(*b)) {
                            (Some(ea), Some(eb)) => {
                                // Member net name: take the more specific pin name (signal name), fallback net.name.
                                //   Name only affects label/classification, connectivity is determined by endpoints -> doesn't affect electrical correctness.
                                let name = if !eb.pin_name.is_empty() && eb.pin_name != net.name {
                                    eb.pin_name.clone()
                                } else if !ea.pin_name.is_empty() && ea.pin_name != net.name {
                                    ea.pin_name.clone()
                                } else {
                                    net.name.clone()
                                };
                                members.push((name, vec![ea, eb]));
                            }
                            _ => {
                                split_ok = false;
                                break;
                            }
                        }
                    }
                    if split_ok && members.len() == n {
                        for (i, (name, eps)) in members.into_iter().enumerate() {
                            let nid = if i == 0 {
                                net.nid
                            } else {
                                let x = synth_nid;
                                synth_nid += 1;
                                x
                            };
                            out.push(VizNet::new(
                                nid,
                                name,
                                NetKind::Signal,
                                NetRole::Signal,
                                eps,
                            ));
                            // ★ P9-A2: propagate provenance to split nets
                            if let Some(ref ss) = net.source_span {
                                out.last_mut().unwrap().source_span = Some(ss.clone());
                            }
                            if let Some(ref pg) = net.trunk {
                                out.last_mut().unwrap().trunk = Some(pg.clone());
                            }
                            if let Some(d) = net.trunk_ref {
                                out.last_mut().unwrap().trunk_ref = Some(d);
                            }
                            if let Some(shape) = &net.shape {
                                out.last_mut().unwrap().shape = Some(shape.clone());
                            }
                        }
                        continue; // already split by member -> skip whole Bus construction below
                    }
                    // Split failed (some endpoint missing box mapping) -> fall back to original whole construction, don't drop net.
                }
            }
        }

        // ── Original: one VizNet per net ──
        // ★ FIX: Each endpoint is pushed only once. make_endpoint already does box query + pin info +
        //   EndpointRef::full internally; the old code below was redundantly constructing and pushing
        //   again → endpoints doubled, topology() counts a 2-point net as 4 points → misjudges
        //   Star/MultiDriver. Endpoints with no box mapping (make_endpoint = None) are discarded
        //   here, and which ones are lost is uniformly reported by net_probe at the boundary.
        let mut endpoints = Vec::new();
        for pid in net.all_point_ids() {
            if let Some(e) = make_endpoint(pid) {
                endpoints.push(e);
            } else if pid >= 0 {
                // ── D4: GHOST_PORT detection ────────────────────────────────
                // Fire when a net endpoint can't be mapped to any box in the
                // current layer. This includes placeholder pins (id ≥ 8e9) and
                // pins whose InstTable entry exists but isn't mapped to any box.
                let msg = crate::errcodes::format_msg(
                    crate::errcodes::GHOST_PORT,
                    &[&net.name, &pid as &dyn std::fmt::Display],
                );
                // Anchor at the failing endpoint's own source position (the
                // declared pin/port/net) instead of pos 0 (renders as file:1:1,
                // un-navigable). The endpoint may live in a *different* file
                // than the block being drawn — a module-boundary pin declared in
                // the child module's file — so use the entry's explicit
                // SourcePos when it has one (the graph build's current_uri is
                // only an approximation). Wiring site wins, declaration site is
                // the fallback; if the entry is synthesized with no position,
                // anchor on the net's own origin span before giving up.
                let entry = table.get_entry(pid as u32);
                // ── D4b: module-net origin override ─────────────────────────
                // When the failing endpoint is the module's OWN net pseudo
                // entry (a Bus/Label child of this block — the net has no
                // physical box anywhere in the layer), the wiring/declaration
                // chain below can only reach a *statement head* token (all the
                // segment nets of a series statement share it), which may be a
                // different net's name on the same line or an unrelated line.
                // Prefer the net's defining token: its declaration (conduit
                // `ref`, io/port bus-member row) when declared, else its
                // earliest net-name reference (a usage-born `[A, B]` label's
                // own token). Real boundary-crossing pins keep the chain.
                let module_net_origin = {
                    // Any Bus/Label pseudo entry under this block is the net's
                    // own non-physical marker (direct net label, or a declared
                    // bus member child of a Bus) — real physical pins / ports
                    // are Pin/Port kinds and never match. Gate on the net name
                    // resolving in this module's origin map: a boundary pin
                    // ghost (e.g. `usb.vin/GND`) never appears there, so those
                    // keep the wiring/declaration chain below.
                    let is_net_pseudo = entry.as_ref().is_some_and(|e| {
                        matches!(
                            e.kind,
                            crate::instant::insttab::InstKind::Bus
                                | crate::instant::insttab::InstKind::Label
                        )
                    });
                    let uri = entry
                        .as_ref()
                        .map(|e| e.def_uri.clone())
                        .or_else(|| net.source_span.as_ref().map(|s| s.uri.clone()))
                        .unwrap_or_default();
                    if is_net_pseudo {
                        table
                            .net_origin()
                            .get(&(block.bid as u32))
                            .and_then(|m| m.get(&net.name))
                            .map(|off| crate::semantic::common::SourcePos::new(uri, *off))
                    } else {
                        None
                    }
                };
                let anchor = module_net_origin
                    .or_else(|| {
                        entry
                            .and_then(|e| e.src_pos.clone())
                            .or_else(|| entry.and_then(|e| e.fallback_pos.clone()))
                    })
                    .or_else(|| net.source_span.clone());
                match anchor {
                    Some(sp) => crate::db::diagnostic::diagnostic::diagnostic_log_at(
                        crate::errcodes::GHOST_PORT,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        sp.uri.clone(),
                        sp.offset,
                        1,
                        &msg,
                        &[],
                    ),
                    None => crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::GHOST_PORT,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        0,
                        1,
                        &msg,
                        &[],
                    ),
                }
            }
        }

        if endpoints.is_empty() {
            continue;
        }

        // Initial NetKind: guess by name (goes through naming, see P04)
        let mut kind = naming::classify_net(&net.name);
        // ★ §5② (classification-retirement-design): power/ground classification is
        // ATTR-DRIVEN — the net's declared supply identity (`net.attr.role`),
        // never its name. A declared Hot net is Power, a declared Ret/Reference
        // net is Ground (name-independent); a legacy net with no declaration
        // (attr None) that only LOOKS like a rail by name is Signal (ruling ① —
        // undeclared: never judged, never guessed, no power/ground symbol drawn).
        match net.attr.as_ref().map(|a| a.role) {
            Some(AttrRole::Hot) => kind = NetKind::Power,
            Some(AttrRole::Ret) | Some(AttrRole::Reference) => kind = NetKind::Ground,
            _ => {
                if matches!(kind, NetKind::Power | NetKind::Ground) {
                    kind = NetKind::Signal;
                }
            }
        }

        // If net has NtoN topology and width > 1, promote to Bus
        //
        // ── ★ P1-4 ────────────────────────────────────────────────────────
        // But **power/ground are never upgraded**: V3V3/GND's fan-out (one power feeds N chips)
        // is physically still power, not a bus.
        //
        // ── ★ iter 7 ──────────────────────────────────────────────────────
        // Same guard as the split branch above: `connection_type()` only compares the lengths of
        // the two groups, so a merged equipotential node that happens to form [n,n] is misjudged
        // as an n-bit bus. Here the consequence is not splitting the net but kind=Bus(n) →
        // dispatch.rs:241 unconditionally takes BusBundle → a 4-endpoint node is drawn as a brown
        // thick trunk + taps (observed with __net_4).
        // Criterion is the same: a real bus never passes through a discrete two-terminal passive device (see is_real_bus()).
        if let Some(n) = is_real_bus(net, &kind, &touches_passive) {
            kind = NetKind::Bus(n);
        }

        // ★ M0-2: compute NetRole from NetKind
        let role = match &kind {
            NetKind::Power | NetKind::Ground => {
                // Try to extract voltage from endpoint member_info
                let volt = net.all_point_ids().iter().find_map(|&pid| {
                    table
                        .get_entry(pid as u32)
                        .and_then(|e| e.member_info.as_ref())
                        .and_then(|mi| mi.voltage.as_ref())
                        .map(|v| v.to_string())
                });
                NetRole::Rail { volt }
            }
            NetKind::Bus(n) => NetRole::Bus { width: *n },
            _ => NetRole::Signal,
        };

        out.push(VizNet::new(
            net.nid,
            net.name.clone(),
            kind,
            role,
            endpoints,
        ));
        // ★ P7-3: the power net spec (class + driver_pin + volt) resolved by the
        // projection layer is passed through as-is; the layout's rail trichotomy
        // (R-1/R-2/R-3) consumes it.
        if let Some(spec) = &net.rail {
            out.last_mut().unwrap().rail = Some(spec.clone());
        }
        // ★ §4 (classification-retirement-design): pass the declared supply
        // identity mirror through as-is; drawing consumers key on `attr.role`
        // (Ret/Reference → ground side, Hot → supply, None → Signal), never on
        // the net name. Split nets (SPI / NtoN bus expansion above) do not
        // mirror attr.
        if let Some(attr) = &net.attr {
            out.last_mut().unwrap().attr = Some(attr.clone());
        }
        // ★ P9-A2: pass through source_span and trunk
        if let Some(ref ss) = net.source_span {
            out.last_mut().unwrap().source_span = Some(ss.clone());
        }
        if let Some(ref pg) = net.trunk {
            out.last_mut().unwrap().trunk = Some(pg.clone());
        }
        if let Some(d) = net.trunk_ref {
            out.last_mut().unwrap().trunk_ref = Some(d);
        }
        if let Some(shape) = &net.shape {
            out.last_mut().unwrap().shape = Some(shape.clone());
        }
        // ★ Module-port drawing: the boundary marker travels with the net. The
        // module-frame pass reads it to name a layer's own boundary by the port
        // the net crosses. Like `attr`, a split net does not mirror it.
        if let Some(bi) = &net.boundary {
            out.last_mut().unwrap().boundary = Some(bi.clone());
        }
    }

    out
}

// ============================================================================
// ★ Node conservation probe: building the graph must not change electrical facts
// ============================================================================

/// Every net on the block side must have its endpoint set appear verbatim in some VizNet;
/// splitting is only allowed on **real buses** and must be recorded explicitly.
fn probe_node_conservation(block: &McVecBlock, nets: &[VizNet], _point_to_box: &HashMap<u32, u32>) {
    for bn in &block.nets {
        let pts: std::collections::HashSet<i64> = bn.all_point_ids().into_iter().collect();
        let covered = nets.iter().any(|vn| {
            let vp: std::collections::HashSet<i64> =
                vn.endpoints.iter().map(|e| e.pin_id).collect();
            pts.is_subset(&vp)
        });
        if !covered {
            crate::velog!(
                "[graph] ✗ NODE SPLIT: block net '{}' ({} pts) is not fully carried by any VizNet \
                 —— the equipotential node was split apart; every downstream topology model will read the wrong graph",
                bn.name,
                pts.len()
            );
        }
    }
}

// ============================================================================
// Internal helper -- point_id -> box_id mapping
// ============================================================================

/// Build `point_id -> box_id` mapping (covering all descendants of each box)
fn build_point_to_box(table: &InstTable, boxes: &[McVecBox]) -> HashMap<u32, u32> {
    let mut point_to_box: HashMap<u32, u32> = HashMap::new();

    for b in boxes {
        if b.id < 0 {
            continue;
        }
        let bid = b.id as u32;

        match b.kind {
            BoxKind::TwoPin | BoxKind::MultiPin => {
                map_all_descendants(table, bid, bid, &mut point_to_box);
                point_to_box.insert(bid, bid);
            }
            BoxKind::SubModule => {
                map_all_descendants(table, bid, bid, &mut point_to_box);
                point_to_box.insert(bid, bid);
            }
            BoxKind::PowerLabel => {
                point_to_box.insert(bid, bid);
                map_all_descendants(table, bid, bid, &mut point_to_box);
            }
            BoxKind::Dot => {
                point_to_box.insert(bid, bid);
            }
            BoxKind::PortTerminal => {
                map_all_descendants(table, bid, bid, &mut point_to_box);
                point_to_box.insert(bid, bid);
            }
        }
    }

    crate::velog!(
        "[graph] build_point_to_box: {} mappings across {} boxes",
        point_to_box.len(),
        boxes.len()
    );
    point_to_box
}

/// BFS map all descendant IDs of `box_id` to `mapping_to`
fn map_all_descendants(
    table: &InstTable,
    box_id: u32,
    mapping_to: u32,
    out: &mut HashMap<u32, u32>,
) {
    use std::collections::VecDeque;
    let mut queue: VecDeque<u32> = VecDeque::new();
    let mut visited: std::collections::HashSet<u32> = std::collections::HashSet::new();
    queue.push_back(box_id);
    visited.insert(box_id);

    while let Some(cur) = queue.pop_front() {
        for child in table.children_of(cur) {
            if visited.insert(child.id) {
                out.entry(child.id).or_insert(mapping_to);
                queue.push_back(child.id);
            }
        }
    }
}

// (★ P03: deleted `edge_type_from_connection` and `generate_edges_from_net`
//  those two functions just split multi-endpoint net into pairwise binary edges, after P03 cut the
//  dual-track this path is no longer needed. A net's topology is computed on-the-fly by
//  `VizNet::topology()`.)

// (★ P7-3 deletion: synthesize_rail_nets / collect_exposed_labels / bfs_collect_labels
//  — the same-name label synthesis machinery removed wholesale —— criteria now read
//  port declarations and the post-projection real nets.)

// ── ★ Phase 1.46b: Adjust Virtual Top Module Border position/size ─────────────────────────────
//
// After layout computes positions for all other boxes, adjust the SubModule border box
// to properly surround the internal components.
//
// This function finds all negative-ID SubModule boxes (created by Phase 1.46) and
// adjusts their position and size to surround the internal components.

/// Adjust SubModule border boxes to surround internal components.
/// This should be called after layout has positioned all boxes.
pub fn layout_post_adjust_borders(graph: &mut McVecGraph) {
    // Find all border box indices (negative ID SubModules)
    let border_indices: Vec<usize> = graph
        .boxes
        .iter()
        .enumerate()
        .filter(|(_, b)| b.id < 0 && b.kind == BoxKind::SubModule)
        .map(|(i, _)| i)
        .collect();

    if border_indices.is_empty() {
        return;
    }

    let padding = 30.0; // padding around internal content

    // Calculate the bounds of all non-border, non-power-rail boxes
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for b in &graph.boxes {
        // Skip border boxes and power rails
        if b.id < 0 && b.kind == BoxKind::SubModule {
            continue;
        }
        if b.kind == BoxKind::PowerLabel {
            continue;
        }

        // Include this box's bounds
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
        max_x = max_x.max(b.x + b.w);
        max_y = max_y.max(b.y + b.h);
    }

    // Only adjust if we found valid bounds
    if min_x != f64::MAX && max_x != f64::MIN {
        for &idx in &border_indices {
            if let Some(border) = graph.boxes.get_mut(idx) {
                border.x = min_x - padding;
                border.y = min_y - padding - 20.0; // extra space for title
                border.w = max_x - min_x + padding * 2.0;
                border.h = max_y - min_y + padding * 2.0 + 20.0; // extra for title
            }
        }
    }
}
