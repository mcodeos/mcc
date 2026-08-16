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
//! - `graph.edges` field kept but no longer populated
//!
//! Second phase: the entire `McVecEdge` / `EdgeType` can be deprecated, requires first
//! migrating the `from_table.rs` legacy builder (P03 doesn't touch it for now).

use std::collections::HashMap;

use crate::instant::insttab::{InstEntry, InstKind, InstTable};

use super::super::model::netshape::{GroupRole, NetShape};
use super::super::model::{ConnectionType, McVecBlock, McVecNet};
use super::boxdef::{
    BoxPin, CustomSymbol, EntryPoint, EntrySide, IoSummary, McVecBox, PinConstraint, PinLayout,
    PortDir, VisualRole,
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

/// Typed chips (detect.rs Phase F.1) don't register pins as independent `Pin` children, only have
/// `pin_count` estimated from class_name (`guess_chip_pin_count`). Here we synthesize "placeholder
/// pins" based on the estimated count (common name uses the index, no description), letting these
/// components with **no pin data** also display pins, rather than an empty square.
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

/// ★ Unified wiring point for component pin-layout and project SVG overrides.
fn apply_reserved_overrides(b: &mut McVecBox) {
    let cls = b.class_name.clone();
    if let Some(layout) = component_pin_layout(&cls) {
        b.set_layout_hint(layout);
        b.pin_constraint = PinConstraint::FixedOrder;
    }
    if let Some(sym) = resolve_custom_symbol(&cls) {
        b.set_custom_symbol(sym);
    }
}

/// ★ Reserved interface ①: query a component class's custom pin layout.
///
/// Looks up the component by class_name in workspace + global tables, reads `comp.layout`
/// (core `McLayout{left,right,top,bottom}`) and converts each edge's `Vec<u32>` pin numbers
/// to `Vec<String>` for drawing-side [`PinLayout`].
///
/// Returns `None` when the component is not found or all four layout edges are empty
/// (falls through to heuristic edge assignment).
fn component_pin_layout(class_name: &str) -> Option<PinLayout> {
    let comp = crate::db::cmie::tables::WORKSPACE.component_by_class(class_name)?;
    let layout = &comp.layout;
    if layout.left.is_empty()
        && layout.right.is_empty()
        && layout.top.is_empty()
        && layout.bottom.is_empty()
    {
        return None;
    }
    Some(PinLayout {
        left: layout.left.iter().map(|n| n.to_string()).collect(),
        right: layout.right.iter().map(|n| n.to_string()).collect(),
        top: layout.top.iter().map(|n| n.to_string()).collect(),
        bottom: layout.bottom.iter().map(|n| n.to_string()).collect(),
    })
}

/// ★ Project SVG interface: query the validated project-local symbol registry by class name.
/// Missing, invalid, or undeclared symbols return `None` and keep the system renderer fallback.
fn resolve_custom_symbol(class_name: &str) -> Option<CustomSymbol> {
    super::custom_symbol::resolve_project_symbol(class_name)
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
            // typed-chip (Phase F.1): no registered Pin children -> use estimated pin count to synthesize placeholder pins
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
            Some(b)
        }
        DetectedKind::PowerLabel => {
            let symbol = Symbol::PowerRail {
                is_ground: naming::is_ground(&name),
            };
            let inst_path = entry.path.clone();
            let scope_chain = compute_scope_chain(&inst_path);
            Some(McVecBox::new_v2(
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
    super::netprobe::probe_block_to_graph(&projected, &graph); // ★ NEW
    super::netprobe::probe_block_to_graph_per_layer(&projected, &graph); // ★ P7-6 inventory
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

    eprintln!(
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
                // typed-chip (Phase F.1): no registered Pin children -> use estimated pin count to synthesize placeholder pins
                if box_pins.is_empty() && pin_count > 0 {
                    box_pins = placeholder_pins(id as i64, pin_count);
                }
                // ★ P01: compute symbol / designator in one pass
                let symbol = detect_symbol(table, id, &kind);
                let designator = extract_designator(&name);
                let value: Option<String> = None; // pass2 model has no value field yet, P01 leaves None
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
                graph.boxes.push(b);
                box_ids_set.insert(id);
            }
            DetectedKind::PowerLabel => {
                crate::velog!("[graph] ✓ PowerLabel: {name}");
                // ★ P01: PowerRail symbol with is_ground bit
                let symbol = Symbol::PowerRail {
                    is_ground: naming::is_ground(&name),
                };
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
            DetectedKind::Label => {
                // ★ P7-6: Label entries are literal views of port declarations, not
                // drawable boxes. Skip them — same as the backfill path.
                crate::velog!("[graph] Phase 1 skip Label: '{}' (id={})", name, id);
            }
            DetectedKind::Skip => {
                if entry.kind == InstKind::Bus {
                    for member in &table.children_of(id) {
                        let mname = extract_last_segment(&member.path);
                        if naming::is_power_rail(&mname) && !box_ids_set.contains(&member.id) {
                            crate::velog!("[graph] ✓ PowerLabel (bus member): {mname}");
                            let symbol = Symbol::PowerRail {
                                is_ground: naming::is_ground(&mname),
                            };
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
                    // ★ M4-1B: backfill fitted components not in block.insts
                    if let Some(b) = make_box_from_id(table, child.id) {
                        eprintln!(
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
                        eprintln!(
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

    // ── ★ Phase 1.46: Virtual Top Module Border ──
    // Create a dashed border for the top-level module, but do not render the module name (avoid a "main" label).
    if is_top_level {
        let has_components = block.insts.iter().any(|&iid| {
            if iid < 0 {
                return false;
            }
            table
                .get_entry(iid as u32)
                .map_or(false, |e| matches!(e.kind, InstKind::Component))
        });

        if has_components {
            let first_component_id = block
                .insts
                .iter()
                .find(|&iid| {
                    if *iid < 0 {
                        return false;
                    }
                    table
                        .get_entry(*iid as u32)
                        .map_or(false, |e| matches!(e.kind, InstKind::Component))
                })
                .copied();

            if let Some(comp_id) = first_component_id {
                let border_id = -(comp_id as i64);
                if !box_ids_set.contains(&(border_id as u32)) {
                    let internal_count = block
                        .insts
                        .iter()
                        .filter(|&iid| {
                            if *iid < 0 {
                                return false;
                            }
                            table.get_entry(*iid as u32).map_or(false, |e| {
                                matches!(e.kind, InstKind::Component | InstKind::Label)
                            })
                        })
                        .count();

                    // ★ Use an empty string as name to avoid rendering a "main" label
                    let mut b = McVecBox::new_v2(
                        border_id,
                        String::new(), // name = "" → no label rendered
                        String::new(), // class_name = ""
                        BoxKind::SubModule,
                        Symbol::Module,
                        None,
                        None,
                        internal_count.max(1),
                        IoSummary::new(),
                        String::new(), // inst_path
                        vec![],        // scope_chain
                    );
                    b.w = 800.0;
                    b.h = 600.0;
                    b.x = 0.0;
                    b.y = 0.0;
                    graph.boxes.push(b);
                    box_ids_set.insert(border_id as u32);
                }
            }
        }
    }

    // ── Phase 1.5: supplement missing boxes from block.nets endpoints ──
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
                    if parent_entry.kind == InstKind::Component && !box_ids_set.contains(&parent_id)
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
                let mut cursor: Option<u32> = table.get_entry(parent_id).and_then(|p| p.parent_id);
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

            // ★ S3.5 Fix B: tighten -- only create PowerLabel in two cases:
            //   (1) name really looks like power/ground (naming::is_power_rail)
            //   (2) Bus kind and name is signal-like (entire bus as label, like MIC{P,N})
            // Pure Label kind (especially SPI/UART sub-ports CSN/MOSI/10) is no longer misjudged.
            let looks_like_power = naming::is_power_rail(&name);
            let looks_like_bus_label = entry.kind == InstKind::Bus && naming::is_signal_like(&name);
            if !looks_like_power && !looks_like_bus_label {
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
                        crate::velog!(
                            "[graph] ✓ Phase-E1 boundary label: '{}' (kind={:?}, hops={}) \
                             -> label box (layer bid={})",
                            entry.path,
                            entry.kind,
                            hops,
                            layer_bid
                        );
                        // Using PowerLabel/PowerRail reuses the existing BoxKind, geometrically a
                        // named arrow, which matches the conventional drawing of Port labels in
                        // schematics. is_ground still uses naming::is_ground check -- GND goes to
                        // downward triangle, others (UART0/SPI.SCLK/DAC_OUT/[VCC_1V2,GND]/...)
                        // go to upward arrow.
                        let is_ground = naming::is_ground(&name);
                        let symbol = Symbol::PowerRail { is_ground };
                        let inst_path = entry.path.clone();
                        let scope_chain = compute_scope_chain(&inst_path);
                        graph.boxes.push(McVecBox::new_v2(
                            u as i64,
                            name.clone(),
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
                        box_ids_set.insert(u);
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

            crate::velog!(
                "[graph] ✓ PowerLabel (from net endpoint): {} (kind={:?})",
                name,
                entry.kind
            );
            let symbol = Symbol::PowerRail {
                is_ground: naming::is_ground(&name),
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

    // ── ★ P7-8: PortTerminal creation from BoundaryInfo markers ──────────────────────
    // Replaces Phase 1.45 (deleted) and Phase E.1 (merged into this step).
    // For each projected net with a BoundaryInfo marker (non-rail pseudo endpoint),
    // create one PortTerminal box per port group. The PortTerminal box's id equals
    // the port_group_id so that build_point_to_box maps all member endpoints to it.
    {
        let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut count = 0usize;
        for net in &block.nets {
            if let Some(ref bi) = net.boundary {
                if seen.insert(bi.port_group_id) {
                    let port_name = bi.port_name.clone();
                    let io = bi.io;
                    // PortTerminal: 1 pin, small fixed size, placed at canvas edge by layout
                    let mut io_summary = IoSummary::new();
                    match io {
                        IoDirection::Input => io_summary.inputs += 1,
                        IoDirection::Output => io_summary.outputs += 1,
                        _ => io_summary.other += 1,
                    }
                    let inst_path = table
                        .get_entry(bi.port_group_id as u32)
                        .map(|e| e.path.clone())
                        .unwrap_or_default();
                    let scope_chain = compute_scope_chain(&inst_path);
                    let mut b = McVecBox::new_v2(
                        bi.port_group_id,
                        port_name.clone(),
                        String::new(),
                        BoxKind::PortTerminal,
                        Symbol::PortTerminal { io },
                        None,
                        None,
                        1,
                        io_summary,
                        inst_path,
                        scope_chain,
                    );
                    // Single pin named after the port group
                    b.entry_points = vec![EntryPoint {
                        pin_id: bi.port_group_id,
                        pin_name: port_name.clone(),
                        side: match io {
                            IoDirection::Input => EntrySide::Left,
                            IoDirection::Output => EntrySide::Right,
                            _ => EntrySide::Right,
                        },
                        offset: 0.5,
                    }];
                    b.set_pins(vec![BoxPin {
                        id: bi.port_group_id,
                        pin_id: bi.port_group_id.to_string(),
                        description: String::new(),
                        io,
                        port_dir: PortDir::None,
                    }]);
                    crate::velog!(
                        "[graph] ✓ P7-8 PortTerminal: '{}' (id={}, io={:?})",
                        b.name,
                        b.id,
                        io
                    );
                    graph.boxes.push(b);
                    box_ids_set.insert(bi.port_group_id as u32);
                    count += 1;
                }
            }
        }
        if count > 0 {
            crate::velog!(
                "[graph] P7-8: created {} PortTerminal box(es) across {} boundary net(s)",
                count,
                block.nets.iter().filter(|n| n.boundary.is_some()).count()
            );
        }
    }

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
    // Before P03, this simultaneously filled `graph.edges` (binary) and `graph.nets`, P03 cut the former.

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
            if let crate::instant::insttab::InstOrigin::FuncCall { ref fn_name } = b.origin {
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
            GroupRole::BusLanes(n) => *n,
            GroupRole::Scalar => 1,
        };
        let right_n = match &shape.groups[1] {
            GroupRole::Broadcast(n) => *n,
            GroupRole::BusLanes(n) => *n,
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
                crate::db::diagnostic::diagnostic::diagnostic_log(
                    crate::errcodes::GHOST_PORT,
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                    0,
                    1,
                    &crate::errcodes::format_msg(
                        crate::errcodes::GHOST_PORT,
                        &[&net.name, &pid as &dyn std::fmt::Display],
                    ),
                    &[],
                );
            }
        }

        if endpoints.is_empty() {
            continue;
        }

        // Initial NetKind: guess by name (goes through naming, see P04)
        let mut kind = naming::classify_net(&net.name);

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
