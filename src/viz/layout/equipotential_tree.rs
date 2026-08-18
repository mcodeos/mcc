// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ R-N (discipline 29): Equipotential tree — four-layer architecture
//!
//! Layer 1: Topology   — pure logic, zero coordinates
//! Layer 2: Layout     — places boxes by topology, writes x/y/w/h and entry_points
//! Layer 3: Geometry   — compute segments + junction dots from topology + placed coords
//! Layer 4: Render     — segments → SVG (in equipotential_tree_render.rs)
//!
//! Each net is rendered as ONE connected orthogonal tree, not n-1 independent edges.
//!
//! Forms: 2-point → single line, 3-point → T-shape, 4-point → cross, n-point → comb

use std::collections::BTreeMap;

use crate::vector::graph::{BoxKind, EntrySide, McVecGraph, NetKind, PinSlot, VizNet};
use crate::vector::model::RailClass;

// ============================================================================
// Constants
// ============================================================================

/// Minimum pin pitch for R-D box sizing
pub const PIN_PITCH: f64 = 40.0;

/// Margin on each end of pin row
pub const PIN_MARGIN: f64 = 20.0;

/// Gap from anchor right edge to trunk
pub const TRUNK_GAP: f64 = 100.0;

/// Gap from anchor right edge to single-pin junction
pub const JUNCTION_GAP: f64 = 220.0;

/// Gap from trunk to member box
pub const MEMBER_GAP: f64 = 60.0;

/// ★ Vertical drop from a single-pin junction to the NetLabel / Ground symbol
/// below it (see realize + build_symbols — both read this constant).
pub const SYMBOL_DROP: f64 = 60.0;

/// ★ E4: Fixed symbol size for two-pin passive components (R/C/L/D).
/// R-D formula (pin_count × PIN_PITCH + 2 × MARGIN) applies only to MultiPin boxes.
pub const TWO_PIN_SYMBOL_W: f64 = 60.0;
pub const TWO_PIN_SYMBOL_H: f64 = 20.0;

// ============================================================================
// Layer 1: Topology (zero coordinates)
// ============================================================================

/// Trunk direction: derived from the anchor's pin edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrunkAxis {
    /// Pins on left/right edge → trunk is vertical
    Vertical,
    /// Pins on top/bottom edge → trunk is horizontal
    Horizontal,
}

/// A group of pins on the same box.
#[derive(Debug, Clone)]
pub struct PinGroup {
    pub box_id: i64,
    /// Pin IDs sorted by pin number (deterministic)
    pub pin_ids: Vec<i64>,
    /// Number of pins in this group
    pub pin_count: usize,
}

/// Terminal symbol type (topology level, no coordinates).
#[derive(Debug, Clone)]
pub enum Terminal {
    Ground,
    NetLabel(String),
    Port { name: String },
}

/// Net topology: pure logic, zero coordinates.
#[derive(Debug, Clone)]
pub struct NetTopology {
    pub net_name: String,
    pub net_kind: NetKind,
    /// ★ Power rail (DC-interface Power member, from the projection-layer
    /// rail spec): renders as a bus label, even with a single real group.
    pub is_power_rail: bool,
    /// Anchor box ID
    pub anchor: i64,
    /// Groups: anchor first, then non-anchor sorted by box_id (deterministic)
    pub groups: Vec<PinGroup>,
    /// Terminal symbols for this net
    pub terminals: Vec<Terminal>,
    /// Trunk axis derived from anchor's pin edge
    pub trunk_axis: TrunkAxis,
}

/// Build topology for all nets. Zero coordinates — does not read b.x / b.y / b.w / b.h.
pub fn build_topology(graph: &McVecGraph) -> Vec<NetTopology> {
    let mut topos = Vec::new();

    for net in &graph.nets {
        // Skip nets with <1 real endpoint (need at least one box to anchor)
        let real_count = net
            .endpoints
            .iter()
            .filter(|ep| {
                graph
                    .boxes
                    .iter()
                    .any(|b| b.id == ep.box_id && b.kind != BoxKind::PowerLabel)
            })
            .count();
        if real_count < 1 {
            continue;
        }

        if let Some(topo) = build_one_topology(net, graph) {
            topos.push(topo);
        }
    }

    topos
}

fn build_one_topology(net: &VizNet, graph: &McVecGraph) -> Option<NetTopology> {
    // Group real-box endpoints by box_id (BTreeMap for determinism)
    let mut real_groups: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut terminals: Vec<Terminal> = Vec::new();

    // ★ Power-rail net: a DC-interface Power member (e.g. `vin.POWER_SYS` from
    // `io vin{POWER_SYS, GND}::DC(5V)`). The projection layer drops the
    // port-side pseudo endpoint (rail rule (c)), so such a net may carry a
    // single real group — it still renders, terminated by a bus label.
    let is_power_rail_net = net
        .rail
        .as_ref()
        .is_some_and(|r| r.class == RailClass::Power)
        || net.kind == NetKind::Power;

    for ep in &net.endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
            continue;
        };
        if b.kind == BoxKind::PowerLabel || b.kind == BoxKind::PortTerminal {
            // ★ Label-kind endpoints (PowerLabel / PortTerminal) become terminal
            // symbols, so they are NOT placed/laid out as physical boxes. A
            // PortTerminal such as `USB_VBUS` (from `usbsock.VBUS -> USB_VBUS`)
            // is an inline net label, not a component — it must render as a
            // BusLabel/NetLabel circle+text, not a square box.
            match &net.kind {
                NetKind::Ground => {
                    if !terminals.iter().any(|t| matches!(t, Terminal::Ground)) {
                        terminals.push(Terminal::Ground);
                    }
                }
                _ => {
                    if !terminals.iter().any(|t| matches!(t, Terminal::NetLabel(_))) {
                        terminals.push(Terminal::NetLabel(net.name.clone()));
                    }
                }
            }
        } else {
            real_groups.entry(ep.box_id).or_default().push(ep.pin_id);
        }
    }

    if real_groups.is_empty() {
        return None;
    }

    // Sort pin IDs within each group for determinism
    for pins in real_groups.values_mut() {
        pins.sort();
    }

    // Anchor: most pins, tiebreak: degree → source_line → box_id
    let anchor = select_anchor_deterministic(&real_groups, net, graph);

    // Build pin groups: anchor first, then non-anchor sorted by box_id
    let mut groups: Vec<PinGroup> = Vec::new();

    // Anchor group first
    if let Some(pins) = real_groups.get(&anchor) {
        groups.push(PinGroup {
            box_id: anchor,
            pin_ids: pins.clone(),
            pin_count: pins.len(),
        });
    }

    // Non-anchor groups sorted by box_id
    let mut non_anchor: Vec<(i64, &Vec<i64>)> = real_groups
        .iter()
        .filter(|(&id, _)| id != anchor)
        .map(|(&id, pins)| (id, pins))
        .collect();
    non_anchor.sort_by_key(|(id, _)| *id);

    for (box_id, pins) in non_anchor {
        groups.push(PinGroup {
            box_id,
            pin_ids: pins.clone(),
            pin_count: pins.len(),
        });
    }

    // ★ F5: skip nets with only the anchor (no non-anchor groups)
    // BUT only if there are no terminals — a power net with a label should still render.
    // Power-rail nets (single real group after the port pseudo endpoint was
    // dropped by projection) are kept: they render as anchor + bus label.
    if groups.len() < 2 && terminals.is_empty() && !is_power_rail_net {
        return None;
    }

    // Trunk axis: derived from anchor's entry_points
    let trunk_axis = trunk_axis_from_anchor(anchor, &real_groups[&anchor], graph);

    // Add net-kind-based terminals (only if not already added from PowerLabel)
    if is_power_rail_net {
        // ★ Power rail: bus label stripped of the port prefix
        // ("vin.POWER_SYS" -> "POWER_SYS", "V3V3.VCC" -> "VCC")
        if !terminals.iter().any(|t| matches!(t, Terminal::NetLabel(_))) {
            let label = net
                .name
                .rsplit_once('.')
                .map_or(net.name.clone(), |(_, leaf)| leaf.to_string());
            terminals.push(Terminal::NetLabel(label));
        }
    } else {
        match &net.kind {
            NetKind::Ground => {
                if !terminals.iter().any(|t| matches!(t, Terminal::Ground)) {
                    terminals.push(Terminal::Ground);
                }
            }
            NetKind::Signal => {
                if !net.name.is_empty()
                    && !net.name.starts_with("__net")
                    && !terminals.iter().any(|t| matches!(t, Terminal::NetLabel(_)))
                {
                    terminals.push(Terminal::NetLabel(net.name.clone()));
                }
            }
            NetKind::SubModuleIO => {
                // ★ The PortTerminal endpoint (e.g. USB_VBUS) was already
                // extracted as a NetLabel above; do not emit a duplicate Port
                // label that would overlap it.
                if !terminals
                    .iter()
                    .any(|t| matches!(t, Terminal::Port { .. } | Terminal::NetLabel(_)))
                {
                    terminals.push(Terminal::Port {
                        name: net.name.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    Some(NetTopology {
        net_name: net.name.clone(),
        net_kind: net.kind.clone(),
        is_power_rail: is_power_rail_net,
        anchor,
        groups,
        terminals,
        trunk_axis,
    })
}

/// Select anchor deterministically: most pins, tiebreak: degree → source_line → box_id.
fn select_anchor_deterministic(
    groups: &BTreeMap<i64, Vec<i64>>,
    net: &VizNet,
    graph: &McVecGraph,
) -> i64 {
    // Compute degree for each box (number of nets it connects to)
    let mut degree: BTreeMap<i64, usize> = BTreeMap::new();
    for n in &graph.nets {
        for ep in &n.endpoints {
            if groups.contains_key(&ep.box_id) {
                *degree.entry(ep.box_id).or_default() += 1;
            }
        }
    }

    // Find source line for each endpoint (lowest source_line wins)
    // Use pin_number as a deterministic proxy for source ordering
    let source_line: BTreeMap<i64, usize> = net
        .endpoints
        .iter()
        .filter(|ep| groups.contains_key(&ep.box_id))
        .map(|ep| (ep.box_id, ep.pin_number.unwrap_or(0) as usize))
        .collect();

    groups
        .iter()
        .max_by(|(id_a, pins_a), (id_b, pins_b)| {
            pins_a
                .len()
                .cmp(&pins_b.len())
                .then_with(|| {
                    degree
                        .get(id_a)
                        .unwrap_or(&0)
                        .cmp(degree.get(id_b).unwrap_or(&0))
                })
                .then_with(|| {
                    // ★ F5: lower source_line wins — reverse order for max_by
                    source_line
                        .get(id_b)
                        .unwrap_or(&0)
                        .cmp(source_line.get(id_a).unwrap_or(&0))
                })
                .then_with(|| id_a.cmp(id_b))
        })
        .map(|(&id, _)| id)
        .unwrap_or(0)
}

/// Determine trunk axis from anchor's pin count (topology), NOT from entry_points.
/// Device layer: anchor pins are all on the right side → Vertical trunk for multi-pin,
/// horizontal for single-pin (trunk is the line from pin to junction).
fn trunk_axis_from_anchor(_anchor_id: i64, pin_ids: &[i64], _graph: &McVecGraph) -> TrunkAxis {
    if pin_ids.len() > 1 {
        TrunkAxis::Vertical
    } else {
        TrunkAxis::Horizontal
    }
}

// ============================================================================
// Layer 2: Layout (topology determines coordinates)
// ============================================================================

/// Place boxes by topology. Writes x/y/w/h and entry_points on boxes,
/// sets geom_locked = true. Overrides FlowLayouter placement.
pub fn place_by_topology(graph: &mut McVecGraph, topos: &[NetTopology]) {
    if topos.is_empty() {
        return;
    }

    // Find the layer anchor: the box referenced by the most topologies as anchor
    let mut anchor_counts: BTreeMap<i64, usize> = BTreeMap::new();
    for topo in topos {
        *anchor_counts.entry(topo.anchor).or_default() += 1;
    }
    let layer_anchor = anchor_counts
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(id, _)| *id)
        .unwrap_or(topos[0].anchor);

    // Place the layer anchor box: left side, all pins on right edge
    let anchor_right = place_layer_anchor(graph, layer_anchor);

    // Place member boxes for each topology
    for (topo_idx, topo) in topos.iter().enumerate() {
        if topo.trunk_axis == TrunkAxis::Horizontal {
            // Single pin: horizontal trunk from anchor pin, junction at JUNCTION_GAP
            // ★ F5: stagger junction_x per topology to avoid overlapping members
            let junction_x = anchor_right + JUNCTION_GAP + (topo_idx as f64) * MEMBER_GAP;
            let anchor_pin_y = get_anchor_pin_y(graph, topo);

            place_members_single_pin(graph, topo, junction_x, anchor_pin_y);
        } else {
            // Multiple pins: vertical trunk at TRUNK_GAP
            let trunk_x = anchor_right + TRUNK_GAP;
            // trunk_x_offset for this topology (multiple topologies on same anchor)
            let trunk_x_offset = trunk_x + (topo_idx as f64) * TRUNK_GAP;

            place_members_multi_pin(graph, topo, trunk_x_offset);
        }
    }
}

/// Place the layer anchor box. Returns anchor_right_edge x.
fn place_layer_anchor(graph: &mut McVecGraph, anchor_id: i64) -> f64 {
    let Some(anchor_box) = graph.boxes.iter_mut().find(|b| b.id == anchor_id) else {
        return 300.0;
    };

    // ★ E1: R-D uses physical pin count, not entry_points.len()
    let pin_count = anchor_box.pins.len().max(1);

    // R-D: box height = pin_count × PIN_PITCH + 2 × PIN_MARGIN
    let box_h = pin_count as f64 * PIN_PITCH + 2.0 * PIN_MARGIN;
    let box_w = 220.0;

    anchor_box.x = 80.0;
    anchor_box.y = 100.0;
    anchor_box.w = box_w;
    anchor_box.h = box_h;
    anchor_box.geom_locked = true;

    // ★ E1: generate PinSlots for ALL physical pins (single source of truth)
    assign_pin_slots(anchor_box, EntrySide::Right);

    // ★ F1: entry_points in device layer are downgraded to connectivity-only
    // (which pin is on which net). Geometry (offset) comes from slots only.
    for ep in anchor_box.entry_points.iter_mut() {
        ep.side = EntrySide::Right;
    }

    anchor_box.x + anchor_box.w
}

/// ★ E1: Generate PinSlots for every physical pin on a box.
/// This is the single source of truth for pin geometry — renderers read only slots.
fn assign_pin_slots(b: &mut crate::vector::graph::McVecBox, side: EntrySide) {
    let n = b.pins.len();
    if n == 0 {
        return;
    }
    b.slots.clear();
    // Build set of connected pin IDs
    let connected: std::collections::HashSet<i64> =
        b.entry_points.iter().map(|ep| ep.pin_id).collect();
    for (i, p) in b.pins.iter().enumerate() {
        let name = if p.description.is_empty() {
            p.pin_id.clone()
        } else {
            p.description.clone()
        };
        b.slots.push(PinSlot {
            pin_id: p.id,
            number: i as u32,
            name,
            side,
            offset: (i as f64 + 1.0) / (n as f64 + 1.0),
            connected: connected.contains(&p.id),
        });
    }
}

/// ★ E1: PinSlots for a Series two-pin member (R/C/L/D inline on a path).
/// Pin 1 faces the junction (entry), pin 2 faces away (exit) — both at
/// mid-height of their side, matching the horizontal resistor symbol.
fn assign_series_slots(b: &mut crate::vector::graph::McVecBox) {
    let connected: std::collections::HashSet<i64> =
        b.entry_points.iter().map(|ep| ep.pin_id).collect();
    b.slots.clear();
    for (i, p) in b.pins.iter().enumerate() {
        let side = if i == 0 {
            EntrySide::Left
        } else {
            EntrySide::Right
        };
        let name = if p.description.is_empty() {
            p.pin_id.clone()
        } else {
            p.description.clone()
        };
        b.slots.push(PinSlot {
            pin_id: p.id,
            number: i as u32,
            name,
            side,
            offset: 0.5,
            connected: connected.contains(&p.id),
        });
    }
    // Keep entry_points connectivity in sync with the split sides
    for (i, ep) in b.entry_points.iter_mut().enumerate() {
        ep.side = if i == 0 {
            EntrySide::Left
        } else {
            EntrySide::Right
        };
    }
}

/// Get the y position of the anchor pin for a single-pin topology.
fn get_anchor_pin_y(graph: &McVecGraph, topo: &NetTopology) -> f64 {
    let anchor_group = topo.groups.first();
    let anchor_box = anchor_group.and_then(|g| graph.boxes.iter().find(|b| b.id == g.box_id));

    if let (Some(b), Some(g)) = (anchor_box, anchor_group) {
        if let Some(&pid) = g.pin_ids.first() {
            // ★ F1: read from slots (single source of truth), not entry_points
            if let Some(slot) = slot_of(b, pid) {
                return b.y + b.h * slot.offset;
            }
        }
        // Fallback: center of box
        b.y + b.h / 2.0
    } else {
        140.0
    }
}

/// ★ F1: find a PinSlot by pin_id. Single source of truth for pin geometry.
fn slot_of(b: &crate::vector::graph::McVecBox, pin_id: i64) -> Option<&PinSlot> {
    b.slots.iter().find(|s| s.pin_id == pin_id)
}

// ============================================================================
// ★ E3: MemberRole — electrical role determines placement
// ============================================================================

/// Electrical role of a member box, determines how it connects to the trunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberRole {
    /// Two-pin passive (R/C/L/D): one pin connects to trunk, the other extends
    /// further — it is a **pathway**, not a dead-end.
    Series,
    /// Single-pin device (TestPoint, terminal): hangs as a short vertical stub
    /// off the trunk.
    Stub,
    /// Multi-pin device: distributed along the trunk, connection pins face the trunk.
    Sink,
}

fn role_of(b: &crate::vector::graph::McVecBox) -> MemberRole {
    match b.pins.len() {
        1 => MemberRole::Stub,
        2 if b.is_two_pin_passive() => MemberRole::Series,
        _ => MemberRole::Sink,
    }
}

/// Place member boxes for a single-pin topology (horizontal trunk).
fn place_members_single_pin(
    graph: &mut McVecGraph,
    topo: &NetTopology,
    junction_x: f64,
    junction_y: f64,
) {
    let non_anchor: Vec<&PinGroup> = topo.groups.iter().skip(1).collect();

    let mut stub_above = true; // alternate Stub above/below

    for group in &non_anchor {
        let Some(member_box) = graph.boxes.iter_mut().find(|b| b.id == group.box_id) else {
            continue;
        };
        if member_box.geom_locked {
            continue;
        }

        let role = role_of(member_box);

        // ★ F5: set TwoPin size BEFORE computing x/y (center depends on w/h)
        if member_box.kind == BoxKind::TwoPin {
            member_box.w = TWO_PIN_SYMBOL_W;
            member_box.h = TWO_PIN_SYMBOL_H;
        }
        // ★ F5-fix: zero-size boxes (e.g. port labels) get a default size
        if member_box.w <= 0.0 {
            member_box.w = 80.0;
        }
        if member_box.h <= 0.0 {
            member_box.h = 20.0;
        }

        match role {
            MemberRole::Stub => {
                // Hang as short vertical stub, alternate above/below junction
                if stub_above {
                    member_box.x = junction_x - member_box.w / 2.0;
                    member_box.y = junction_y - MEMBER_GAP - member_box.h;
                } else {
                    member_box.x = junction_x - member_box.w / 2.0;
                    member_box.y = junction_y + MEMBER_GAP;
                }
                stub_above = !stub_above;
            }
            MemberRole::Series => {
                // Series: continue along the horizontal trunk (right of junction)
                member_box.x = junction_x + MEMBER_GAP;
                member_box.y = junction_y - member_box.h / 2.0;
            }
            MemberRole::Sink => {
                // Sink: right of junction, center-aligned
                member_box.x = junction_x + MEMBER_GAP;
                member_box.y = junction_y - member_box.h / 2.0;
            }
        }

        member_box.geom_locked = true;

        // Orient pins toward junction
        let member_side = if member_box.x < junction_x {
            EntrySide::Right
        } else {
            EntrySide::Left
        };

        // ★ F1: entry_points downgraded to connectivity-only, no offset
        for ep in &mut member_box.entry_points {
            ep.side = member_side;
        }
        // ★ E1: generate PinSlots
        // ★ Series members are inline on the path: pin 1 faces the junction,
        // pin 2 faces away — so the next net (e.g. POWER_SYS behind R0603)
        // can start its trunk at the far pin without crossing the body.
        if role == MemberRole::Series && member_box.pins.len() == 2 {
            assign_series_slots(member_box);
        } else {
            assign_pin_slots(member_box, member_side);
        }
    }
}

/// Place member boxes for a multi-pin topology (vertical trunk).
fn place_members_multi_pin(graph: &mut McVecGraph, topo: &NetTopology, trunk_x: f64) {
    let anchor_group = topo.groups.first();
    let anchor_box = anchor_group.and_then(|g| graph.boxes.iter().find(|b| b.id == g.box_id));

    // ★ F1: read from slots (single source of truth)
    let (anchor_y_min, anchor_y_max) = if let Some(b) = anchor_box {
        let ys: Vec<f64> = anchor_group
            .unwrap()
            .pin_ids
            .iter()
            .filter_map(|&pid| slot_of(b, pid).map(|s| b.y + b.h * s.offset))
            .collect();
        if ys.is_empty() {
            // ★ F5-fix: anchor box has no PinSlots (e.g. zero-size port box).
            // Fall back to box center; avoid f64::MAX/f64::MIN overflow.
            let cy = b.y + b.h / 2.0;
            eprintln!(
                "[equi-tree]   WARN: anchor '{}' id={} has no slots for net '{}' — using center y={:.0}",
                b.name, b.id, topo.net_name, cy,
            );
            (cy, cy)
        } else {
            let y_min = ys.iter().cloned().fold(f64::MAX, f64::min);
            let y_max = ys.iter().cloned().fold(f64::MIN, f64::max);
            (y_min, y_max)
        }
    } else {
        (300.0, 460.0)
    };

    let non_anchor: Vec<&PinGroup> = topo.groups.iter().skip(1).collect();
    let member_count = non_anchor.len();

    for (i, group) in non_anchor.iter().enumerate() {
        if let Some(member_box) = graph.boxes.iter_mut().find(|b| b.id == group.box_id) {
            if member_box.geom_locked {
                continue;
            }
            // Distribute along trunk
            let attach_y = if member_count > 1 {
                anchor_y_min
                    + (anchor_y_max - anchor_y_min) * (i as f64 + 1.0) / (member_count as f64 + 1.0)
            } else {
                (anchor_y_min + anchor_y_max) / 2.0
            };

            // ★ F5: set TwoPin size BEFORE computing x/y (center depends on w/h)
            if member_box.kind == BoxKind::TwoPin {
                member_box.w = TWO_PIN_SYMBOL_W;
                member_box.h = TWO_PIN_SYMBOL_H;
            }
            // ★ F5-fix: zero-size boxes (e.g. port labels) get a default size
            if member_box.w <= 0.0 {
                member_box.w = 80.0;
            }
            if member_box.h <= 0.0 {
                member_box.h = 20.0;
            }

            member_box.x = trunk_x + MEMBER_GAP;
            member_box.y = attach_y - member_box.h / 2.0;
            member_box.geom_locked = true;

            // ★ F1: entry_points downgraded to connectivity-only, no offset
            for ep in &mut member_box.entry_points {
                ep.side = EntrySide::Left;
            }
            // ★ E1: generate PinSlots
            assign_pin_slots(member_box, EntrySide::Left);
        }
    }
}

// ============================================================================
// Layer 3: Geometry (topology + placed coords → segments + dots)
// ============================================================================

/// A line segment.
#[derive(Debug, Clone)]
pub struct Segment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Terminal symbol type.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeSymbolKind {
    Ground,
    Power,
    NetLabel,
    /// ★ F5: Bus label — circle with text at the end of the line
    BusLabel,
    PortLabel,
}

/// A terminal symbol placed on the tree.
#[derive(Debug, Clone)]
pub struct TreeSymbol {
    pub kind: TreeSymbolKind,
    pub x: f64,
    pub y: f64,
    pub label: String,
}

/// An equipotential tree for one net.
#[derive(Debug)]
pub struct EquiTree {
    pub net_name: String,
    pub net_kind: NetKind,
    /// All line segments
    pub segments: Vec<Segment>,
    /// Junction dots (degree >= 3)
    pub junction_dots: Vec<(f64, f64)>,
    /// Terminal symbols
    pub symbols: Vec<TreeSymbol>,
}

/// Compute geometry from topology + placed graph. Zero judgment.
pub fn realize(topo: &NetTopology, graph: &McVecGraph) -> EquiTree {
    let mut segments: Vec<Segment> = Vec::new();
    let mut degree_map: BTreeMap<(i64, i64), u8> = BTreeMap::new();

    let anchor_group = topo.groups.first();
    let anchor_box = anchor_group.and_then(|g| graph.boxes.iter().find(|b| b.id == g.box_id));
    let anchor_right = anchor_box.map(|b| b.x + b.w).unwrap_or(0.0);

    // ★ F1: read anchor pin y positions from slots (single source of truth)
    let anchor_pin_ys: Vec<f64> = anchor_box
        .map(|b| {
            anchor_group
                .unwrap()
                .pin_ids
                .iter()
                .filter_map(|&pid| slot_of(b, pid).map(|s| b.y + b.h * s.offset))
                .collect()
        })
        .unwrap_or_default();

    if anchor_pin_ys.is_empty() {
        // Fallback: no anchor pins, return empty tree
        return EquiTree {
            net_name: topo.net_name.clone(),
            net_kind: topo.net_kind.clone(),
            segments,
            junction_dots: vec![],
            symbols: build_symbols(topo, 0.0, 0.0, 0.0, 0.0, graph),
        };
    }

    let mut trunk_y_min = anchor_pin_ys.iter().cloned().fold(f64::MAX, f64::min);
    let mut trunk_y_max = anchor_pin_ys.iter().cloned().fold(f64::MIN, f64::max);

    if topo.trunk_axis == TrunkAxis::Horizontal {
        // Single pin: horizontal trunk from anchor pin to junction
        let anchor_pin_y = anchor_pin_ys[0];
        let junction_x = anchor_right + JUNCTION_GAP;

        // Trunk: anchor pin → junction
        let seg = Segment {
            x1: anchor_right,
            y1: anchor_pin_y,
            x2: junction_x,
            y2: anchor_pin_y,
        };
        add_segment(&seg, &mut segments, &mut degree_map);

        // Member taps
        for group in topo.groups.iter().skip(1) {
            if let Some(member_box) = graph.boxes.iter().find(|b| b.id == group.box_id) {
                // ★ F2: read member pin position from PinSlot (single source of truth)
                let (member_x, member_y) = if let Some(&pid) = group.pin_ids.first() {
                    if let Some(slot) = slot_of(member_box, pid) {
                        match slot.side {
                            EntrySide::Top => {
                                (member_box.x + member_box.w * slot.offset, member_box.y)
                            }
                            EntrySide::Bottom => (
                                member_box.x + member_box.w * slot.offset,
                                member_box.y + member_box.h,
                            ),
                            EntrySide::Left => {
                                (member_box.x, member_box.y + member_box.h * slot.offset)
                            }
                            EntrySide::Right => (
                                member_box.x + member_box.w,
                                member_box.y + member_box.h * slot.offset,
                            ),
                        }
                    } else {
                        (member_box.x, member_box.y + member_box.h / 2.0)
                    }
                } else {
                    (member_box.x, member_box.y + member_box.h / 2.0)
                };

                // Determine if member is above, right, or below
                if (member_y - anchor_pin_y).abs() < 10.0 && member_x > junction_x {
                    // Right: horizontal tap from junction
                    let seg = Segment {
                        x1: junction_x,
                        y1: anchor_pin_y,
                        x2: member_x,
                        y2: member_y,
                    };
                    add_segment(&seg, &mut segments, &mut degree_map);
                } else {
                    // Above/below (or left): L-shaped tap — vertical drop to the
                    // member's y, then horizontal run to the member pin. Without
                    // the horizontal run the wire stops short of the member
                    // (e.g. TP1 sitting above the USB_VBUS junction).
                    let vseg = Segment {
                        x1: junction_x,
                        y1: anchor_pin_y,
                        x2: junction_x,
                        y2: member_y,
                    };
                    add_segment(&vseg, &mut segments, &mut degree_map);
                    let hseg = Segment {
                        x1: junction_x,
                        y1: member_y,
                        x2: member_x,
                        y2: member_y,
                    };
                    add_segment(&hseg, &mut segments, &mut degree_map);
                }
            }
        }

        // ★ Vertical drop from the junction to NetLabel / Ground symbols
        // (they sit SYMBOL_DROP below the junction — see build_symbols).
        let has_drop_symbol = topo
            .terminals
            .iter()
            .any(|t| matches!(t, Terminal::NetLabel(_) | Terminal::Ground));
        if has_drop_symbol {
            let seg = Segment {
                x1: junction_x,
                y1: anchor_pin_y,
                x2: junction_x,
                y2: anchor_pin_y + SYMBOL_DROP,
            };
            add_segment(&seg, &mut segments, &mut degree_map);
        }
    } else {
        // Multiple pins: vertical trunk
        let trunk_x = anchor_right + TRUNK_GAP;

        let (ext_y_min, ext_y_max) =
            extend_trunk_for_symbols(trunk_y_min, trunk_y_max, &topo.net_kind);
        trunk_y_min = ext_y_min;
        trunk_y_max = ext_y_max;

        // Trunk: vertical line
        let seg = Segment {
            x1: trunk_x,
            y1: ext_y_min,
            x2: trunk_x,
            y2: ext_y_max,
        };
        add_segment(&seg, &mut segments, &mut degree_map);

        // Teeth: horizontal from each anchor pin to trunk
        for &y in &anchor_pin_ys {
            let seg = Segment {
                x1: anchor_right,
                y1: y,
                x2: trunk_x,
                y2: y,
            };
            add_segment(&seg, &mut segments, &mut degree_map);
        }

        // Member taps: horizontal from member pin to trunk
        for group in topo.groups.iter().skip(1) {
            if let Some(member_box) = graph.boxes.iter().find(|b| b.id == group.box_id) {
                // ★ F2: read member pin position from PinSlot (single source of truth)
                let (member_x, member_y) = if let Some(&pid) = group.pin_ids.first() {
                    if let Some(slot) = slot_of(member_box, pid) {
                        match slot.side {
                            EntrySide::Top => {
                                (member_box.x + member_box.w * slot.offset, member_box.y)
                            }
                            EntrySide::Bottom => (
                                member_box.x + member_box.w * slot.offset,
                                member_box.y + member_box.h,
                            ),
                            EntrySide::Left => {
                                (member_box.x, member_box.y + member_box.h * slot.offset)
                            }
                            EntrySide::Right => (
                                member_box.x + member_box.w,
                                member_box.y + member_box.h * slot.offset,
                            ),
                        }
                    } else {
                        (member_box.x, member_box.y + member_box.h / 2.0)
                    }
                } else {
                    (member_box.x, member_box.y + member_box.h / 2.0)
                };
                let seg = Segment {
                    x1: member_x,
                    y1: member_y,
                    x2: trunk_x,
                    y2: member_y,
                };
                add_segment(&seg, &mut segments, &mut degree_map);
            }
        }
    }

    // ★ F4: Junction dot fix — count internal points too.
    // add_segment only counts segment ENDPOINTS. For a comb-shaped tree, tooth
    // endpoints that land on the trunk interior get degree=1 (only the tooth's
    // endpoint counted). Fix: for each segment, check if any other segment's
    // endpoint lies on its interior, and increment the degree at that point.
    for (i, si) in segments.iter().enumerate() {
        for (j, sj) in segments.iter().enumerate() {
            if i == j {
                continue;
            }
            for &(ex, ey) in &[(sj.x1, sj.y1), (sj.x2, sj.y2)] {
                let ix = ex.round() as i64;
                let iy = ey.round() as i64;
                let sx1 = si.x1.round() as i64;
                let sy1 = si.y1.round() as i64;
                let sx2 = si.x2.round() as i64;
                let sy2 = si.y2.round() as i64;
                // Skip if this point is already an endpoint of si
                if (ix == sx1 && iy == sy1) || (ix == sx2 && iy == sy2) {
                    continue;
                }
                // Check if (ex, ey) lies on segment si
                if point_on_segment(ex, ey, si) {
                    *degree_map.entry((ix, iy)).or_default() += 1;
                }
            }
        }
    }

    // Junction dots: degree >= 3
    let junction_dots: Vec<(f64, f64)> = degree_map
        .iter()
        .filter(|(_, &deg)| deg >= 3)
        .map(|(&(x, y), _)| (x as f64, y as f64))
        .collect();

    // Symbols
    let symbols = build_symbols(
        topo,
        anchor_right + TRUNK_GAP,
        trunk_y_min,
        trunk_y_max,
        anchor_right,
        graph,
    );

    EquiTree {
        net_name: topo.net_name.clone(),
        net_kind: topo.net_kind.clone(),
        segments,
        junction_dots,
        symbols,
    }
}

fn add_segment(
    seg: &Segment,
    segments: &mut Vec<Segment>,
    degree_map: &mut BTreeMap<(i64, i64), u8>,
) {
    let x1 = seg.x1.round() as i64;
    let y1 = seg.y1.round() as i64;
    let x2 = seg.x2.round() as i64;
    let y2 = seg.y2.round() as i64;

    *degree_map.entry((x1, y1)).or_default() += 1;
    *degree_map.entry((x2, y2)).or_default() += 1;

    segments.push(seg.clone());
}

/// ★ F4: check if a point lies on a segment (within rounding tolerance).
fn point_on_segment(px: f64, py: f64, seg: &Segment) -> bool {
    let x1 = seg.x1;
    let y1 = seg.y1;
    let x2 = seg.x2;
    let y2 = seg.y2;
    let eps = 0.5; // tolerance for f64 rounding
    if (x1 - x2).abs() < eps {
        // Vertical segment
        (px - x1).abs() < eps && py >= y1.min(y2) - eps && py <= y1.max(y2) + eps
    } else if (y1 - y2).abs() < eps {
        // Horizontal segment
        (py - y1).abs() < eps && px >= x1.min(x2) - eps && px <= x1.max(x2) + eps
    } else {
        false
    }
}

fn extend_trunk_for_symbols(y_min: f64, y_max: f64, net_kind: &NetKind) -> (f64, f64) {
    let mut y_min = y_min;
    let mut y_max = y_max;
    match net_kind {
        NetKind::Ground => {
            y_max += 60.0;
        }
        NetKind::Power => {
            y_min -= 20.0;
        }
        NetKind::Signal => {
            y_max += 40.0;
        }
        _ => {}
    }
    (y_min, y_max)
}

fn build_symbols(
    topo: &NetTopology,
    trunk_x: f64,
    trunk_y_min: f64,
    trunk_y_max: f64,
    _anchor_right: f64,
    graph: &McVecGraph,
) -> Vec<TreeSymbol> {
    let mut symbols = Vec::new();

    let is_single_pin = topo.trunk_axis == TrunkAxis::Horizontal;

    // ★ F4: check if there is already a label box for this net (e.g. PowerLabel),
    // to avoid drawing a duplicate label symbol. PortTerminal is excluded: it is
    // now extracted as a Terminal::NetLabel (see build_one_topology), so it must
    // not suppress the symbol it is meant to produce.
    let has_label_box = graph
        .boxes
        .iter()
        .any(|b| matches!(b.kind, BoxKind::PowerLabel | BoxKind::Dot) && b.name == topo.net_name);

    for term in &topo.terminals {
        match term {
            Terminal::Ground => {
                if is_single_pin {
                    // Single pin: GND at junction + SYMBOL_DROP below
                    let junction_x = _anchor_right + JUNCTION_GAP;
                    let junction_y = trunk_y_min; // same as anchor_pin_y
                    symbols.push(TreeSymbol {
                        kind: TreeSymbolKind::Ground,
                        x: junction_x,
                        y: junction_y + SYMBOL_DROP,
                        label: String::new(),
                    });
                } else {
                    symbols.push(TreeSymbol {
                        kind: TreeSymbolKind::Ground,
                        x: trunk_x,
                        y: trunk_y_max,
                        label: String::new(),
                    });
                }
            }
            Terminal::NetLabel(name) => {
                // ★ F4: skip if there is already a label box for this net
                if has_label_box {
                    continue;
                }
                // ★ F5: power rails (incl. DC-interface Power members like
                // vin.POWER_SYS) always use BusLabel (circle + text).
                // Bus names (containing "BUS" or "_VBUS") also use BusLabel.
                let is_bus = topo.is_power_rail || name.contains("BUS") || name.contains("_VBUS");
                let kind = if is_bus {
                    TreeSymbolKind::BusLabel
                } else {
                    TreeSymbolKind::NetLabel
                };
                if is_single_pin {
                    let junction_x = _anchor_right + JUNCTION_GAP;
                    let junction_y = trunk_y_min;
                    symbols.push(TreeSymbol {
                        kind,
                        x: junction_x,
                        y: junction_y + SYMBOL_DROP,
                        label: name.clone(),
                    });
                } else {
                    symbols.push(TreeSymbol {
                        kind,
                        x: trunk_x,
                        y: trunk_y_min - 10.0,
                        label: name.clone(),
                    });
                }
            }
            Terminal::Port { name } => {
                if is_single_pin {
                    let junction_x = _anchor_right + JUNCTION_GAP;
                    let junction_y = trunk_y_min;
                    symbols.push(TreeSymbol {
                        kind: TreeSymbolKind::PortLabel,
                        x: junction_x + 140.0,
                        y: junction_y,
                        label: name.clone(),
                    });
                } else {
                    symbols.push(TreeSymbol {
                        kind: TreeSymbolKind::PortLabel,
                        x: trunk_x,
                        y: trunk_y_min - 10.0,
                        label: name.clone(),
                    });
                }
            }
        }
    }

    symbols
}

// ============================================================================
// Main entry points
// ============================================================================

/// ★ E2: Layout device layer — topology + placement.
/// Called during the layout phase (before render). Writes x/y/w/h and
/// PinSlots on boxes, sets geom_locked = true.
pub fn layout_device_layer(graph: &mut McVecGraph) {
    let topos = build_topology(graph);
    eprintln!(
        "[equi-tree] layout_device_layer: {} nets, {} topos, {} boxes",
        graph.nets.len(),
        topos.len(),
        graph.boxes.len(),
    );
    for t in &topos {
        eprintln!(
            "[equi-tree]   topo: net='{}' anchor={} groups={} trunk={:?}",
            t.net_name,
            t.anchor,
            t.groups.len(),
            t.trunk_axis,
        );
    }
    place_by_topology(graph, &topos);
    // Log placed box positions
    let mut placed_count = 0;
    let mut unplaced_count = 0;
    for b in &graph.boxes {
        if b.geom_locked {
            eprintln!(
                "[equi-tree]   placed box: '{}' id={} x={:.0} y={:.0} w={:.0} h={:.0}",
                b.name, b.id, b.x, b.y, b.w, b.h,
            );
            placed_count += 1;
        } else {
            unplaced_count += 1;
        }
    }
    // ★ Fallback: boxes not in any topology get a default position (stacked right)
    if unplaced_count > 0 {
        eprintln!(
            "[equi-tree]   {} unplaced boxes — assigning fallback positions",
            unplaced_count,
        );
        let mut fallback_x = 500.0;
        let fallback_y = 100.0;
        for b in &mut graph.boxes {
            if !b.geom_locked && b.kind != BoxKind::PowerLabel {
                b.x = fallback_x;
                b.y = fallback_y;
                b.w = 120.0;
                b.h = 60.0;
                b.geom_locked = true;
                fallback_x += 160.0;
                eprintln!(
                    "[equi-tree]   fallback box: '{}' id={} x={:.0} y={:.0}",
                    b.name, b.id, b.x, b.y,
                );
            }
        }
    }
    eprintln!(
        "[equi-tree] layout_device_layer done: {} placed, {} fallback",
        placed_count, unplaced_count,
    );
}

/// Build equipotential trees for all nets in the graph (render phase, read-only).
/// Calls build_topology → realize. Does NOT modify the graph.
pub fn build_all_trees(graph: &McVecGraph) -> Vec<EquiTree> {
    let topos = build_topology(graph);
    eprintln!(
        "[equi-tree] build_all_trees: {} nets, {} topos",
        graph.nets.len(),
        topos.len(),
    );
    let mut trees = Vec::new();
    for t in &topos {
        let tree = realize(t, graph);
        eprintln!(
            "[equi-tree]   tree: net='{}' segments={} dots={} symbols={}",
            tree.net_name,
            tree.segments.len(),
            tree.junction_dots.len(),
            tree.symbols.len(),
        );
        trees.push(tree);
    }
    trees
}
