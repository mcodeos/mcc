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

use crate::instant::inst_table::{InstEntry, InstKind, InstTable};

use super::super::model::{ConnectionType, McVecBlock};
use super::box_def::{BoxPin, CustomSymbol, IoSummary, McVecBox, PinLayout};
use super::detect::{
    compute_io, detect_kind, detect_symbol, extract_designator, extract_last_segment,
    parse_pin_number, translate_io_type, warn_if_pin_mismatch, DetectedKind,
};
use super::graph_def::McVecGraph;
use super::kinds::{BoxKind, NetKind};
use super::naming;
use super::net_def::{EndpointRef, IoDirection, VizNet};
use super::symbol::Symbol;

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
            }
        })
        .collect()
}

/// ★ Unified wiring point for reserved interfaces ①+②. Today both resolvers return `None`
/// -> doesn't change any rendering; fill in one function body each to activate.
fn apply_reserved_overrides(b: &mut McVecBox) {
    let cls = b.class_name.clone();
    if let Some(layout) = component_pin_layout(&cls) {
        b.set_layout_hint(layout);
    }
    if let Some(sym) = resolve_custom_symbol(&cls) {
        b.set_custom_symbol(sym);
    }
}

/// ★ Reserved interface ①: query a component class's custom pin layout. **To be wired** -- currently
/// returns None (falls through to heuristic edge assignment).
///
/// Activate: use class_name to get `McComponent` (workspace component table), convert
/// `comp.layout` (core `McLayout{left,right,top,bottom}`) four edges to drawing-side [`PinLayout`]
/// (after confirming `McLayout`'s edge element types, map to `Vec<String>`).
fn component_pin_layout(_class_name: &str) -> Option<PinLayout> {
    None
}

/// ★ Reserved interface ②: query user-uploaded custom symbols for this component class.
/// **To be wired** -- currently returns None (uses system symbols).
///
/// Activate: use class_name to query user symbol library (the `HashMap<class_name, svg_body>`
/// built at upload), if hit return `Some(CustomSymbol { source: class_name.into(), svg_body })`,
/// if not uploaded return None.
fn resolve_custom_symbol(_class_name: &str) -> Option<CustomSymbol> {
    None
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
            let mut b = McVecBox::new_v2(
                id as i64, name, class_name, kind, symbol, designator, None, pin_count, io,
            );
            b.set_pins(box_pins);
            warn_if_pin_mismatch(&b);
            apply_reserved_overrides(&mut b); // ★ Reserved: layout / custom symbol (default no-op)
            Some(b)
        }
        DetectedKind::SubModule {
            port_count,
            class_name,
        } => {
            let ports = table.get_ports_of(id);
            let io = compute_io(&ports);
            let box_pins = build_box_pins(&ports, &class_name);
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
            );
            b.set_pins(box_pins);
            Some(b)
        }
        DetectedKind::PowerLabel => {
            let symbol = Symbol::PowerRail {
                is_ground: naming::is_ground(&name),
            };
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
            ))
        }
        DetectedKind::Skip => None,
    }
}

// ============================================================================
// Main entry
// ============================================================================

/// Build `McVecGraph` from `McVecBlock` + `InstTable`
///
/// Top-level call (`is_top_level = true`) runs **P0-3**: synthesize undeclared power/ground
/// PowerLabels at the top level (typical scenario: hbl's main only declares V1V2/V3V3/V5V Ports,
/// no main.GND, but sub-modules all expose `GND` ports). Sub-graph recursion
/// (`is_top_level = false`) doesn't synthesize, avoiding adding a set of power symbols out of
/// thin air at every layer.
pub fn build_mc_vec_graph(block: &McVecBlock, table: &InstTable) -> McVecGraph {
    let graph = build_mc_vec_graph_inner(block, table, /*is_top_level=*/ true);
    super::net_probe::probe_block_to_graph(block, &graph); // ★ NEW
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

    // ── Phase 1: block.insts -> boxes (duck typing recognition) ──
    let mut box_ids_set: std::collections::HashSet<u32> = std::collections::HashSet::new();

    for &iid in &block.insts {
        if iid < 0 {
            continue;
        }
        let id = iid as u32;
        if box_ids_set.contains(&id) {
            continue;
        }
        let entry = match table.get_entry(id) {
            Some(e) => e,
            None => continue,
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
                eprintln!(
                    "[graph] ✓ Component: {name} (class={class_name}, symbol={symbol}, pins={pin_count})"
                );
                let mut b = McVecBox::new_v2(
                    id as i64, name, class_name, kind, symbol, designator, value, pin_count, io,
                );
                b.set_pins(box_pins);
                warn_if_pin_mismatch(&b);
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
                eprintln!("[graph] ✓ SubModule: {name} (class={class_name}, ports={port_count})");
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
                );
                b.set_pins(box_pins);
                graph.boxes.push(b);
                box_ids_set.insert(id);
            }
            DetectedKind::PowerLabel => {
                eprintln!("[graph] ✓ PowerLabel: {name}");
                // ★ P01: PowerRail symbol with is_ground bit
                let symbol = Symbol::PowerRail {
                    is_ground: naming::is_ground(&name),
                };
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
                ));
                box_ids_set.insert(id);
            }
            DetectedKind::Skip => {
                if entry.kind == InstKind::Bus {
                    for member in &table.children_of(id) {
                        let mname = extract_last_segment(&member.path);
                        if naming::is_power_rail(&mname) && !box_ids_set.contains(&member.id) {
                            eprintln!("[graph] ✓ PowerLabel (bus member): {mname}");
                            let symbol = Symbol::PowerRail {
                                is_ground: naming::is_ground(&mname),
                            };
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
                            ));
                            box_ids_set.insert(member.id);
                        }
                    }
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
                        eprintln!(
                            "[graph] ✓ Synthesized Component (from net endpoint): {} \
                             (class={}, symbol={}, pins={}) -- visit.rs missed this",
                            parent_name, parent_entry.class_name, symbol, pin_count
                        );
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
                        );
                        b.set_pins(box_pins);
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
            //   - `main.mcu513.SPI/SCLK`   (kind=Label, parent=mcu513.SPI Port, 1012)
            //   - `main.mcu513.UART0`     (kind=Port,  parent=mcu513,           1007)
            //   - `main.mcu513.DAC_OUT`   (kind=Port,  parent=mcu513,           1007)
            //   - `main.mcu513.SPK_MUTE`  (kind=Port,  parent=mcu513,           1007)
            //
            // Old logic only checked if the **direct parent** (above line 247-250) was a known box
            // -- for `SPI/SCLK` type, the direct parent is `mcu513.SPI` Port (id 1012) not in
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
                        eprintln!(
                            "[graph] ⚠ ITER-3 lift: ancestor walk exceeded {} hops for '{}', \
                             aborting (suspect cycle in InstTable parent chain)",
                            MAX_HOPS, entry.path
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
                    eprintln!(
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
                    eprintln!(
                        "[graph] ✓ Box from net endpoint (self is {:?}): {}",
                        entry.kind, name
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
                // contains mcu513's children: CAP/RES/uC etc., not including mcu513 itself).
                //
                // Old logic: such endpoints would fall to `✗ Skipping unresolved endpoint`, the
                // sub-layer render loses mcu513's own Port/Label edge labels, drill-down sees
                // a bunch of dangling connections (user feedback "second level has issues").
                //
                // Examples (mcu513 inner layer, block.bid=1010):
                //   - `main.mcu513.UART0`        Port,  parent=1010 -> direct hit
                //   - `main.mcu513.DAC_OUT`      Port,  parent=1010 -> direct hit
                //   - `main.mcu513.[VCC_1V2, GND]` Port,  parent=1010 -> direct hit
                //   - `main.mcu513.SPI/SCLK`     Label, parent=1015 (SPI Port), \
                //                                       grandparent=1010 -> two-hop hit
                //   - `main.mcu513.AVDD09_CAP`   Label, parent=1010 -> direct hit
                //                                                    (internal signal label)
                //
                // Fix: after hit, create a PowerLabel (actually "boundary label" reusing the same
                // BoxKind, visually an arrow + name, suitable for Port label semantics) so that
                // Phase 2 BFS can map the corresponding connection endpoints to this box, drill-down
                // no longer loses labels.
                //
                // Only triggers when `!is_top_level`: at the top layer, the module's own Port is
                // already absorbed by the parent layer's SubModule box (line 247-250
                // parent-in-box_ids_set check), it didn't reach here.
                if !is_top_level && block.bid >= 0 {
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
                        eprintln!(
                            "[graph] ✓ Phase-E1 boundary label: '{}' (kind={:?}, hops={}) \
                             -> label box (layer bid={})",
                            entry.path, entry.kind, hops, layer_bid
                        );
                        // Using PowerLabel/PowerRail reuses the existing BoxKind, geometrically a
                        // named arrow, which matches the conventional drawing of Port labels in
                        // schematics. is_ground still uses naming::is_ground check -- GND goes to
                        // downward triangle, others (UART0/SPI.SCLK/DAC_OUT/[VCC_1V2,GND]/...)
                        // go to upward arrow.
                        let is_ground = naming::is_ground(&name);
                        let symbol = Symbol::PowerRail { is_ground };
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
                        ));
                        box_ids_set.insert(u);
                        continue;
                    }
                }

                eprintln!(
                    "[graph] ✗ Skipping unresolved endpoint '{}' (kind={:?}, parent_id={:?}) \
                     -- not a power rail / not a bus label / parent not a Component. \
                     This endpoint will not have a box drawn for it.",
                    entry.path, entry.kind, entry.parent_id
                );
                continue;
            }

            eprintln!(
                "[graph] ✓ PowerLabel (from net endpoint): {} (kind={:?})",
                name, entry.kind
            );
            let symbol = Symbol::PowerRail {
                is_ground: naming::is_ground(&name),
            };
            graph.boxes.push(McVecBox::new_v2(
                u as i64,
                name,
                String::new(),
                BoxKind::PowerLabel,
                symbol,
                None,
                None,
                0,
                IoSummary::new(),
            ));
            box_ids_set.insert(u);
        }
    }

    // ── ★ Phase 1.55: empty module with ports → create SubModule box ─────────────────────────
    //
    // When a module has only port declarations (no internal instances), Phase 1 creates no boxes
    // and Phase 1.5 finds no net endpoints to synthesize. The result is an empty graph → empty SVG.
    //
    // This phase creates a SubModule box for the module itself, with its ports as pins, so the
    // viz can render a module frame with port pins on the edges.
    if graph.boxes.is_empty() && block.bid >= 0 {
        let mod_id = block.bid as u32;
        if let Some(mod_entry) = table.get_entry(mod_id) {
            let ports = table.get_ports_of(mod_id);
            if !ports.is_empty() {
                let class_name = mod_entry.class_name.clone();
                let io = compute_io(&ports);
                let box_pins = build_box_pins(&ports, &class_name);
                let port_count = ports.len();
                eprintln!(
                    "[graph] ✓ Phase 1.55: empty module '{}' (bid={}) has {} ports, creating SubModule box",
                    root_name, mod_id, port_count
                );
                let mut b = McVecBox::new_v2(
                    mod_id as i64,
                    root_name.clone(),
                    class_name,
                    BoxKind::SubModule,
                    Symbol::Module,
                    None,
                    None,
                    port_count,
                    io,
                );
                b.set_pins(box_pins);
                graph.boxes.push(b);
            }
        }
    }

    // ★ S3.5: After Phase 1.5, print an overview of box type distribution, to quickly spot
    // anomalies like "all PowerLabel"
    let mut count_by_kind = [0usize; 4]; // TwoPin/MultiPin/SubModule/PowerLabel
    for b in &graph.boxes {
        let i = match b.kind {
            BoxKind::TwoPin => 0,
            BoxKind::MultiPin => 1,
            BoxKind::SubModule => 2,
            BoxKind::PowerLabel => 3,
        };
        count_by_kind[i] += 1;
    }
    eprintln!(
        "[graph] '{}' box inventory: total={}, TwoPin={}, MultiPin={}, SubModule={}, PowerLabel={}",
        root_name,
        graph.boxes.len(),
        count_by_kind[0],
        count_by_kind[1],
        count_by_kind[2],
        count_by_kind[3],
    );
    if !graph.boxes.is_empty() && count_by_kind[0] + count_by_kind[1] + count_by_kind[2] == 0 {
        eprintln!(
            "[graph] '{}' WARNING: all {} boxes are PowerLabel -- \
             likely visit.rs missed components or Phase 1.5 misclassified endpoints",
            root_name,
            graph.boxes.len()
        );
    }

    // ── ★ P0-3 Phase 1.6: top-level synthesize missing power/ground PowerLabels ─────────────────────────
    //
    // Trigger condition: in code like hbl.mc, the top-level main module only explicitly declares
    // V1V2/V3V3/V5V power Ports, but sub-modules all expose `GND` ports. Phase 1 doesn't
    // automatically create `main.GND`, consequences:
    //   1. The top-level "ground" row (radial::ground_rails bucket) is empty -> visually asymmetric
    //      (top has V3V3/V5V/V1V2 triangles, bottom is empty)
    //   2. `GND` not in the `toplevel_rails` set -> Phase 3.5's same-name signal synthesis will
    //      synthesize each pair of sub-modules' GND-GND into an independent net, producing
    //      N*(N-1)/2 cross-graph spider webs.
    //
    // Fix: before Phase 3, scan (a) `block.nets` names (b) SubModule children's exposed labels,
    // collect all "is power/ground but top-level doesn't have a corresponding PowerLabel" names,
    // synthesize PowerLabel placeholders. Give a **unique positive id** to avoid `b.id as u32`
    // wrap-around issues in build_point_to_box / synthesize_rail_nets due to negative numbers;
    // simultaneously high-base ids (starting from 1e9) won't collide with real InstTable ids.
    //
    // Only effective at the top level (`is_top_level == true`): sub-graph recursion doesn't repeat.
    if is_top_level {
        let mut existing_rail_upper: std::collections::HashSet<String> = graph
            .boxes
            .iter()
            .filter(|b| b.kind == BoxKind::PowerLabel)
            .map(|b| b.name.to_uppercase())
            .collect();

        // Collect "should have but doesn't" power/ground names (keep original case, priority GND > VSS > V3V3 ...)
        let mut needed: Vec<String> = Vec::new();
        let mut needed_upper: std::collections::HashSet<String> = std::collections::HashSet::new();
        let consider =
            |name: &str,
             needed: &mut Vec<String>,
             needed_upper: &mut std::collections::HashSet<String>,
             existing_rail_upper: &std::collections::HashSet<String>| {
                if name.is_empty() {
                    return;
                }
                if !naming::is_power_rail(name) {
                    return;
                }
                let u = name.to_uppercase();
                if existing_rail_upper.contains(&u) || needed_upper.contains(&u) {
                    return;
                }
                needed_upper.insert(u);
                needed.push(name.to_string());
            };

        // (a) Net names themselves: net named GND / V3V3 but no corresponding PowerLabel at top level
        for net in &block.nets {
            consider(
                &net.name,
                &mut needed,
                &mut needed_upper,
                &existing_rail_upper,
            );
        }

        // (b) Sub-modules' external power/ground port names: even if net name is anonymous like `__net_N`,
        //     as long as the sub-module exposes GND, the top level should have a GND triangle to absorb it.
        for b in &graph.boxes {
            if b.kind != BoxKind::SubModule || b.id < 0 {
                continue;
            }
            for child in table.children_of(b.id as u32) {
                let cname = extract_last_segment(&child.path);
                consider(&cname, &mut needed, &mut needed_upper, &existing_rail_upper);
            }
        }

        if !needed.is_empty() {
            // Choose a stable starting point far above InstTable real ids, avoiding u32 wrap / collision
            const SYNTH_ID_BASE: i64 = 1_000_000_000;
            let mut next_synth_id: i64 = graph
                .boxes
                .iter()
                .map(|b| b.id)
                .max()
                .unwrap_or(0)
                .max(SYNTH_ID_BASE)
                + 1;

            for name in &needed {
                let is_ground = naming::is_ground(name);
                let symbol = Symbol::PowerRail { is_ground };
                eprintln!(
                    "[graph] ✓ Phase 1.6 synthesized top-level PowerLabel: {name} \
                     (id={next_synth_id}, is_ground={is_ground}) -- no explicit '{name}' Port at root"
                );
                graph.boxes.push(McVecBox::new_v2(
                    next_synth_id,
                    name.clone(),
                    String::new(),
                    BoxKind::PowerLabel,
                    symbol,
                    None,
                    None,
                    0,
                    IoSummary::new(),
                ));
                existing_rail_upper.insert(name.to_uppercase());
                next_synth_id += 1;
            }
        }
    }

    // ── Phase 2: build point_to_box mapping ──
    let point_to_box = build_point_to_box(table, &graph.boxes);

    eprintln!(
        "[graph] Phase 2 done: {} point->box mappings across {} boxes",
        point_to_box.len(),
        graph.boxes.len(),
    );

    // ── ★ Phase 3: VizNet (only network model after P03) ──
    //
    // Keep multi-endpoint topology directly, no longer split into "pairwise" pairs.
    // Before P03, this simultaneously filled `graph.edges` (binary) and `graph.nets`, P03 cut the former.
    graph.nets = generate_viznets_from_block(block, &point_to_box, table);

    eprintln!(
        "[graph] Phase 3 done: {} VizNet(s) generated (hyperedge model)",
        graph.nets.len()
    );

    // ── Phase 3.5: same-name label synthesize "power/signal rail" nets (★ P03 refactor) ──
    //
    // Before P03 produced `McVecEdge` written to `edge_map`, P03 changed to produce `VizNet` added
    // to `graph.nets`. Synthesized net's endpoints have `pin_id = -1` (no real pin), router/renderer
    // seeing this will fall back to exiting from the box edge midpoint.
    let synth = synthesize_rail_nets(table, &graph.boxes, &mut graph.nets);
    if synth > 0 {
        eprintln!("[graph] synthesized {synth} rail net(s) via same-name label match");
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
) -> Vec<VizNet> {
    let mut out = Vec::with_capacity(block.nets.len());

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
                    if is_busport && !matches!(kind0, NetKind::Power | NetKind::Ground) {
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
                                        out.push(VizNet::new(nid, nm, NetKind::Signal, eps));
                                    }
                                    eprintln!(
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
        if let ConnectionType::NtoN(n) = net.connection_type() {
            let kind0 = naming::classify_net(&net.name);
            if n > 1 && net.nets.len() == 2 && !matches!(kind0, NetKind::Power | NetKind::Ground) {
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
                            out.push(VizNet::new(nid, name, NetKind::Signal, eps));
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
        // is physically still power, not a bus. Old code unconditionally upgraded causing "V3V3 [2]"
        // brown thick trunk + `[n]` label suffix anomaly, while pushing router to BusBundleRouter
        // to draw as bus trunk. After fix, power/ground continue to StarRouter, maintaining red
        // thin line + triangle symbol.
        if let ConnectionType::NtoN(n) = net.connection_type() {
            if n > 1 && !matches!(kind, NetKind::Power | NetKind::Ground) {
                kind = NetKind::Bus(n);
            }
        }

        out.push(VizNet::new(net.nid, net.name.clone(), kind, endpoints));
    }

    out
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
        }
    }

    eprintln!(
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

// ============================================================================
// Internal helper -- same-name signal synthesized rail (Iter 6, P03 refactored to produce VizNet)
// ============================================================================

/// Scan all boxes' "exposed signal name sets", pairwise intersect to synthesize `VizNet` (rail-synth)
///
/// ## ★ P03 refactor
/// Previously produced `McVecEdge` written to `edge_map`, P03 changed to produce `VizNet` directly
/// appended to `graph.nets`. Synthesized `VizNet` has these characteristics:
/// - Both endpoints have `pin_id = -1` (synthesized endpoint, no real pin)
/// - `kind = naming::classify_net(name)` (classified by representative name Power/Ground/Signal)
/// - `pin_name = "(rail)"` (unified placeholder name for endpoints, router/renderer can recognize)
///
/// Returns the count of newly synthesized nets.
fn synthesize_rail_nets(table: &InstTable, boxes: &[McVecBox], nets: &mut Vec<VizNet>) -> usize {
    // Step 1: For each box collect exposed signal set
    let mut exposed: HashMap<u32, (BoxKind, HashMap<String, String>)> = HashMap::new();
    for b in boxes {
        if b.id < 0 {
            continue;
        }
        let bid = b.id as u32;
        let labels = collect_exposed_labels(table, bid, b);
        if labels.is_empty() {
            continue;
        }
        exposed.insert(bid, (b.kind.clone(), labels));
    }

    if exposed.len() < 2 {
        return 0;
    }

    // Step 1b: Top-level PowerLabel rail name set (for "redundancy suppression")
    let toplevel_rails: std::collections::HashSet<String> = boxes
        .iter()
        .filter(|b| b.kind == BoxKind::PowerLabel)
        .map(|b| {
            if !b.name.is_empty() {
                b.name.to_uppercase()
            } else {
                table
                    .get_entry(b.id as u32)
                    .map(|e| extract_last_segment(&e.path).to_uppercase())
                    .unwrap_or_default()
            }
        })
        .filter(|s| !s.is_empty())
        .collect();

    // ★ P03: already existing (box-pair, net_name) set, avoid duplicate synthesis
    let mut existing_pairs: std::collections::HashSet<(i64, i64, String)> =
        std::collections::HashSet::new();
    for n in nets.iter() {
        let ids = n.box_ids();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let key = if ids[i] <= ids[j] {
                    (ids[i], ids[j], n.name.to_uppercase())
                } else {
                    (ids[j], ids[i], n.name.to_uppercase())
                };
                existing_pairs.insert(key);
            }
        }
    }

    // Allocate nids for synthesized nets: start from existing max + 1
    let mut next_nid: i64 = nets.iter().map(|n| n.nid).max().unwrap_or(-1) + 1;

    // Step 2: pairwise to compute intersection
    let mut ids: Vec<u32> = exposed.keys().copied().collect();
    ids.sort();

    let mut synth_count = 0;

    // ── ★ ITER-4: PowerLabel-anchored hyperedge synthesis ─────────────────────────────────
    //
    // Symptom: old rail-synth only did pairwise pairing, N SubModules all exposing GND produces N
    //   `PowerLabel(GND) <-> SubModule_k` 2-endpoint VizNets. Router receives N independent
    //   "Power/Ground x TwoPoint" -> all go Orthogonal, each draws one line, middle area is
    //   N GND long jumpers crossing each other in a spider web (hbl measured 6 independent GND orthogonal).
    //
    // Fix: before the pairwise loop, do a "PowerLabel-anchored hyperedge merge":
    //   - For each top-level PowerLabel P (e.g. GND / V3V3 / V1V2):
    //     scan all non-PowerLabel boxes, check if their exposed labels contain P's own label
    //     (typical case: SubModule exposes "GND" port).
    //   - If **>= 2** boxes hit, synthesize 1 **single hyperedge** VizNet `[P, b1, b2, ..., bN]`,
    //     endpoint count = 1 + N >= 3. Router seeing >= 3 endpoints of Power/Ground automatically
    //     goes TrunkTap, one trunk + multiple taps, visually like real schematic power rails.
    //   - Simultaneously register all relevant box-pairs into `existing_pairs`, letting the
    //     subsequent pairwise loop **not** re-synthesize the same (PowerLabel, sub_i) or
    //     (sub_i, sub_j).
    //   - When only 1 box hits, **don't** synthesize hyperedge, leave to pairwise loop to produce
    //     2-endpoint net (consistent with old behavior, avoid regression).
    //
    // Compatibility: this step only **reduces** rail-synth net count, doesn't introduce
    // PowerLabel<->PowerLabel mismatches (skip same-kind pairing), and doesn't conflict with the
    // "two non-PowerLabel" no-merge rule (P1-5) -- our hyperedges are always anchored by PowerLabel,
    // the rest are SubModule/MultiPin.
    {
        // Collect PowerLabel ids, decide iteration order (id ascending ensures determinism)
        let pl_ids: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|id| matches!(exposed[id].0, BoxKind::PowerLabel))
            .collect();

        for pl_id in &pl_ids {
            let (_pl_kind, pl_labs) = &exposed[pl_id];
            // PowerLabel's exposed labels generally only have 1 (its own name),
            // exceptional cases like Bus form PowerLabel may have multiple child labels, handle in loop
            for pl_label_upper in pl_labs.keys() {
                let mut connected: Vec<u32> = Vec::new();
                for other_id in &ids {
                    if other_id == pl_id {
                        continue;
                    }
                    let (other_kind, other_labs) = &exposed[other_id];
                    // PowerLabel<->PowerLabel don't merge (consistent with existing P03 rule)
                    if matches!(other_kind, BoxKind::PowerLabel) {
                        continue;
                    }
                    if other_labs.contains_key(pl_label_upper) {
                        connected.push(*other_id);
                    }
                }
                // Only 1 (or 0) hits -> don't form hyperedge, let subsequent pairwise loop handle
                if connected.len() < 2 {
                    continue;
                }
                // Representative name: take PowerLabel's own original case label
                let repr_name = pl_labs
                    .get(pl_label_upper)
                    .cloned()
                    .unwrap_or_else(|| pl_label_upper.clone());

                // ★ Skip already existing same-name hyperedges (previous iter4 already synthesized, defensive)
                let pl_marker_key = (
                    *pl_id as i64,
                    *pl_id as i64, // self-pair as "already synthesized hyperedge" marker
                    repr_name.to_uppercase(),
                );
                if existing_pairs.contains(&pl_marker_key) {
                    continue;
                }

                // Construct endpoints: [PowerLabel, c1, c2, ...]
                let mut endpoints: Vec<EndpointRef> = Vec::with_capacity(1 + connected.len());
                endpoints.push(EndpointRef::new(*pl_id as i64, -1, "(rail)"));
                for c in &connected {
                    endpoints.push(EndpointRef::new(*c as i64, -1, "(rail)"));
                }

                let kind = naming::classify_net(&repr_name);
                let net = VizNet::new(next_nid, repr_name.clone(), kind, endpoints);
                nets.push(net);
                next_nid += 1;
                synth_count += 1;

                // Mark all relevant box-pairs as covered, letting pairwise loop skip:
                //   (a) (pl, c_k) each pair marked
                //   (b) (c_i, c_j) same name also marked (avoid producing sub<->sub same-name small lines below outside P1-5)
                //   (c) (pl, pl) self-pair as hyperedge already exists marker
                existing_pairs.insert(pl_marker_key);
                for c in &connected {
                    let key = if (*pl_id as i64) <= (*c as i64) {
                        (*pl_id as i64, *c as i64, repr_name.to_uppercase())
                    } else {
                        (*c as i64, *pl_id as i64, repr_name.to_uppercase())
                    };
                    existing_pairs.insert(key);
                }
                for i in 0..connected.len() {
                    for j in (i + 1)..connected.len() {
                        let (ci, cj) = (connected[i] as i64, connected[j] as i64);
                        let key = if ci <= cj {
                            (ci, cj, repr_name.to_uppercase())
                        } else {
                            (cj, ci, repr_name.to_uppercase())
                        };
                        existing_pairs.insert(key);
                    }
                }

                eprintln!(
                    "[graph]   + ITER-4 synth hyperedge: PowerLabel #{} '{}' -> {} non-rail endpoints ({:?})",
                    pl_id,
                    repr_name,
                    connected.len(),
                    connected
                );
            }
        }
    }

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let (a, b) = (ids[i], ids[j]);
            let (ka, labs_a) = &exposed[&a];
            let (kb, labs_b) = &exposed[&b];

            if matches!((ka, kb), (BoxKind::PowerLabel, BoxKind::PowerLabel)) {
                continue;
            }

            let (small_labs, big_labs) = if labs_a.len() <= labs_b.len() {
                (labs_a, labs_b)
            } else {
                (labs_b, labs_a)
            };
            let common: Vec<&String> = small_labs
                .keys()
                .filter(|k| big_labs.contains_key(*k))
                .collect();
            if common.is_empty() {
                continue;
            }

            // Redundancy suppression
            let both_non_rail =
                !matches!(ka, BoxKind::PowerLabel) && !matches!(kb, BoxKind::PowerLabel);
            let effective_common: Vec<&String> = if both_non_rail && !toplevel_rails.is_empty() {
                common
                    .iter()
                    .copied()
                    .filter(|k| !toplevel_rails.contains(*k))
                    .collect()
            } else {
                common.clone()
            };
            if effective_common.is_empty() {
                continue;
            }

            // Representative name selection: non-power-label preferred, ties in dictionary order
            let mut candidates: Vec<String> =
                effective_common.iter().map(|s| (*s).clone()).collect();
            candidates.sort_by_key(|upper| {
                let orig = labs_a
                    .get(upper)
                    .or_else(|| labs_b.get(upper))
                    .cloned()
                    .unwrap_or_default();
                (naming::is_power_rail(&orig) as u8, upper.clone())
            });
            let repr_upper: String = candidates.into_iter().next().unwrap();
            let repr_name = labs_a
                .get(&repr_upper)
                .or_else(|| labs_b.get(&repr_upper))
                .cloned()
                .unwrap_or_default();

            // ── ★ P1-5: cross-sub-module power/ground no longer synthesize "end-to-end" nets ─────────────────────────
            //
            // Old behavior: SubModule A exposes GND, SubModule B also exposes GND -> synthesize an A<->B
            // "GND" line, N sub-modules pairwise is N*(N-1)/2 cross-graph spider webs
            // (12+ blue lines stuffed in middle area).
            //
            // New behavior: power/ground are drawn by "symbol" semantics -- each endpoint draws its own
            // small triangle, taken in by SubModule<->top-level PowerLabel pairing (top-level PowerLabel
            // is guaranteed by P0-3 in Phase 1.6). So here **skip power/ground pairing between two non-
            // PowerLabels**, letting N SubModules each connect to the top-level GND triangle,
            // instead of drawing N*(N-1)/2 lines between each other.
            //
            // Note keep SubModule <-> PowerLabel path -- it's exactly the carrier of "connecting to top-level GND".
            if naming::is_power_rail(&repr_name) {
                let both_non_rail =
                    !matches!(ka, BoxKind::PowerLabel) && !matches!(kb, BoxKind::PowerLabel);
                if both_non_rail {
                    eprintln!(
                        "[graph]   - skip synth (Iter 6, P1-5): #{a} <-> #{b} via '{repr_name}' \
                         (both non-rail, power/ground delegated to top-level PowerLabel)"
                    );
                    continue;
                }
            }

            // ★ P03: check if nets already have same-name net connecting same two boxes, if so skip
            let dup_key = if (a as i64) <= (b as i64) {
                (a as i64, b as i64, repr_name.to_uppercase())
            } else {
                (b as i64, a as i64, repr_name.to_uppercase())
            };
            if existing_pairs.contains(&dup_key) {
                continue;
            }

            // ★ P03: synthesize VizNet (both synthesized endpoints pin_id=-1)
            let kind = naming::classify_net(&repr_name);
            let net = VizNet::new(
                next_nid,
                repr_name.clone(),
                kind,
                vec![
                    EndpointRef::new(a as i64, -1, "(rail)"),
                    EndpointRef::new(b as i64, -1, "(rail)"),
                ],
            );
            nets.push(net);
            existing_pairs.insert(dup_key);
            next_nid += 1;
            synth_count += 1;
            eprintln!(
                "[graph]   + synth net (Iter 6, P03): #{} <-> #{} via '{}' ({} common, {} effective)",
                a,
                b,
                repr_name,
                common.len(),
                effective_common.len()
            );
        }
    }

    synth_count
}

/// Collect "exposed signal name set" for a box (UPPER -> original case name)
fn collect_exposed_labels(table: &InstTable, box_id: u32, b: &McVecBox) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();

    match b.kind {
        BoxKind::PowerLabel => {
            let name = if !b.name.is_empty() {
                b.name.clone()
            } else {
                table
                    .get_entry(box_id)
                    .map(|e| extract_last_segment(&e.path))
                    .unwrap_or_default()
            };
            if !name.is_empty() {
                out.insert(name.to_uppercase(), name);
            }
            if let Some(e) = table.get_entry(box_id) {
                if e.kind == InstKind::Bus {
                    for child in table.children_of(box_id) {
                        let cname = extract_last_segment(&child.path);
                        if naming::is_signal_like(&cname) {
                            out.insert(cname.to_uppercase(), cname);
                        }
                    }
                }
            }
        }
        BoxKind::SubModule => {
            bfs_collect_labels(table, box_id, /*collect_pins=*/ false, &mut out);
        }
        BoxKind::MultiPin => {
            bfs_collect_labels(table, box_id, /*collect_pins=*/ true, &mut out);
        }
        BoxKind::TwoPin => {
            // Intentionally empty set -- passive components don't participate in shared signal name matching
        }
    }

    out
}

/// BFS all descendants of `start`, collect "signalized" names by kind + naming rules
fn bfs_collect_labels(
    table: &InstTable,
    start: u32,
    collect_pins: bool,
    out: &mut HashMap<String, String>,
) {
    use std::collections::{HashSet, VecDeque};
    let mut queue: VecDeque<u32> = VecDeque::new();
    let mut visited: HashSet<u32> = HashSet::new();
    queue.push_back(start);
    visited.insert(start);

    while let Some(cur) = queue.pop_front() {
        for child in table.children_of(cur) {
            if !visited.insert(child.id) {
                continue;
            }
            queue.push_back(child.id);

            let name = extract_last_segment(&child.path);
            let take = match child.kind {
                InstKind::Label | InstKind::Bus | InstKind::Port => naming::is_signal_like(&name),
                InstKind::Pin => collect_pins && naming::is_signal_like(&name),
                InstKind::Module | InstKind::Component => false,
            };
            if take {
                out.insert(name.to_uppercase(), name);
            }
        }
    }
}
