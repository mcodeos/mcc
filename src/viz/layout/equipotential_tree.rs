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

use crate::vector::graph::netdef::IoDirection;
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

/// Gap from trunk to member box
pub const MEMBER_GAP: f64 = 60.0;

/// ★ Drop from the tree to a terminal symbol (Ground symbol / NetLabel bus
/// circle). The lane span is extended by this amount past the last tap point
/// when the net carries a Ground terminal, so the ground symbol hangs on its
/// own short wire instead of sitting on the last pin's tooth; NetLabel/BusLabel
/// symbols are placed this far off the trunk and connected by a stub wire.
pub const SYMBOL_DROP: f64 = 60.0;

/// ★ E4: Fixed symbol size for two-pin passive components (R/C/L/D).
/// R-D formula (pin_count × PIN_PITCH + 2 × MARGIN) applies only to MultiPin boxes.
pub const TWO_PIN_SYMBOL_W: f64 = 60.0;
pub const TWO_PIN_SYMBOL_H: f64 = 20.0;

/// Minimum anchor box width (device_layout_v2.md sec.4.5) when the North/South
/// sides carry few or no pins.
pub const MIN_BOX_W: f64 = 120.0;

// ============================================================================
// Region / Lane — direction as a first-class citizen (device_layout_v2.md)
// ============================================================================

/// A net's orientation relative to the layer anchor. Pins inherit the Region of
/// the net they belong to — no pass hardcodes `Right`/`Left` anymore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Region {
    North,
    West,
    East,
    South,
}

impl Region {
    /// Unit outward normal: (dx, dy). W=(-1,0) E=(1,0) N=(0,-1) S=(0,1)
    pub fn outward(self) -> (f64, f64) {
        match self {
            Region::North => (0.0, -1.0),
            Region::West => (-1.0, 0.0),
            Region::East => (1.0, 0.0),
            Region::South => (0.0, 1.0),
        }
    }

    /// The box edge a pin in this region lives on.
    pub fn entry_side(self) -> EntrySide {
        match self {
            Region::North => EntrySide::Top,
            Region::West => EntrySide::Left,
            Region::East => EntrySide::Right,
            Region::South => EntrySide::Bottom,
        }
    }

    /// Is the trunk axis vertical (W/E) or horizontal (N/S)?
    pub fn axis_vertical(self) -> bool {
        matches!(self, Region::West | Region::East)
    }
}

/// A net's trunk lane — the single source of truth for trunk coordinates.
///
/// Written exactly once by P3 `resolve_lanes`; P4 (place members) and
/// P5 (realize) only read it. This kills the "Layer 2 and Layer 3 each compute
/// x" bug class: layout and render can no longer drift apart on trunk position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lane {
    pub region: Region,
    /// Channel index within the region (0 = nearest the anchor).
    pub index: usize,
    /// Trunk axis coordinate: x for W/E trunks, y for N/S trunks.
    /// For the single-pin (horizontal) form this is the junction x.
    pub axis: f64,
    /// Extent along the trunk: (y_min, y_max) for W/E, (x_min, x_max) for N/S.
    pub span: (f64, f64),
}

impl Default for Lane {
    fn default() -> Self {
        Lane {
            region: Region::East,
            index: 0,
            axis: 0.0,
            span: (0.0, 0.0),
        }
    }
}

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
    /// ★ P3: trunk lane — single source of truth for trunk coordinates.
    /// Written once by `resolve_lanes`; place_members/realize read it.
    pub lane: Lane,
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
    // ★ PR3: Ground nets are always kept too — a single-group ground net (e.g.
    // the input decoupling cap's ground after the GND label pseudo endpoint is
    // projected away) must still render its ground symbol.
    let is_ground_net = net.kind == NetKind::Ground;
    if groups.len() < 2 && terminals.is_empty() && !is_power_rail_net && !is_ground_net {
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
        lane: Lane::default(),
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

/// ★ P1: assign each net's Region from its electrical role (device_layout_v2.md sec.3).
/// Pure function of (graph, topos) — layout and render both replay it, so the
/// regions are guaranteed identical. Rules, in order:
///   1. Ground net / Ground pin        → South
///   2. Power net, anchor is driver    → East  (the rail leaves the anchor)
///   3. Power net, anchor not driver   → West  (the rail enters the anchor)
///   4. Input pin                      → West
///   5. Output / Bidir / Bus pin       → East
///   6. Passive / Unknown              → balance (W/E with fewer pins so far)
///   7. Net not touching the layer anchor → inherit the region of a net that
///      shares one of its member boxes (lane-index smallest, then name).
///   Fallback: East (with a `[region] fallback` log — a hit means the design
///   missed a rule class).
pub fn assign_regions(graph: &McVecGraph, topos: &mut [NetTopology]) -> usize {
    let layer_anchor = layer_anchor_id(topos);
    let mut resolved: Vec<bool> = vec![false; topos.len()];
    let mut west_pins: BTreeMap<i64, usize> = BTreeMap::new();
    let mut east_pins: BTreeMap<i64, usize> = BTreeMap::new();

    // Pass 1: direct rules for nets that touch the layer anchor.
    for (i, topo) in topos.iter_mut().enumerate() {
        let touches = topo.groups.iter().any(|g| g.box_id == layer_anchor);
        if !touches {
            continue;
        }
        let region = match direct_region(graph, topo) {
            Some(r) => r,
            // Passive / Unknown → balance: W/E with fewer pins so far.
            None => {
                let w = west_pins.get(&topo.anchor).copied().unwrap_or(0);
                let e = east_pins.get(&topo.anchor).copied().unwrap_or(0);
                if w <= e {
                    Region::West
                } else {
                    Region::East
                }
            }
        };
        topo.lane.region = region;
        resolved[i] = true;
        let pin_cnt = topo.groups.first().map(|g| g.pin_ids.len()).unwrap_or(1);
        match region {
            Region::West => *west_pins.entry(topo.anchor).or_default() += pin_cnt,
            Region::East => *east_pins.entry(topo.anchor).or_default() += pin_cnt,
            _ => {}
        }
    }

    // Pass 2: inheritance — nets not touching the layer anchor share a member
    // box with a regioned net; inherit its region. Iterate to a fixed point
    // (a net's partner may itself be resolved by inheritance).
    for _ in 0..topos.len() {
        for i in 0..topos.len() {
            if resolved[i] {
                continue;
            }
            if let Some(r) = inherited_region(topos, i, &resolved) {
                topos[i].lane.region = r;
                resolved[i] = true;
            }
        }
    }

    // Pass 3: fallback — a hit here means the design missed a rule class.
    // The count is returned so assertion 6 (fallback == 0) can be tested.
    let mut fallback = 0usize;
    for (i, topo) in topos.iter_mut().enumerate() {
        if !resolved[i] {
            fallback += 1;
            crate::vlog!("[region] fallback: net '{}' → East", topo.net_name);
            topo.lane.region = Region::East;
        }
    }
    fallback
}

/// The layer anchor: the box referenced by the most topologies as anchor.
fn layer_anchor_id(topos: &[NetTopology]) -> i64 {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for t in topos {
        *counts.entry(t.anchor).or_default() += 1;
    }
    counts
        .iter()
        .max_by_key(|(_, c)| **c)
        .map(|(id, _)| *id)
        .unwrap_or(topos.first().map(|t| t.anchor).unwrap_or(0))
}

/// Direct region rules for a net that touches the layer anchor.
/// `None` = Passive/Unknown → caller applies the balance rule.
fn direct_region(graph: &McVecGraph, topo: &NetTopology) -> Option<Region> {
    if topo.net_kind == NetKind::Ground {
        return Some(Region::South);
    }
    if topo.net_kind == NetKind::Power {
        let driver_on_anchor = find_net(graph, &topo.net_name)
            .and_then(|n| n.rail.as_ref())
            .and_then(|r| r.driver_pin)
            .is_some_and(|dp| topo.groups.first().is_some_and(|g| g.pin_ids.contains(&dp)));
        if driver_on_anchor {
            return Some(Region::East);
        }
        // ★ Fall back to the anchor pin IO when no rail driver is recorded
        // (module-internal rails often carry `rail: None`): an Output pin means
        // the anchor drives the rail → East; anything else enters → West.
        return match anchor_pin_io(graph, topo) {
            Some(IoDirection::Output) | Some(IoDirection::Bidir) => Some(Region::East),
            _ => Some(Region::West),
        };
    }
    match anchor_pin_io(graph, topo) {
        Some(IoDirection::Input) => Some(Region::West),
        Some(IoDirection::Output | IoDirection::Bidir) => Some(Region::East),
        Some(IoDirection::Power) => Some(Region::West),
        Some(IoDirection::Ground) => Some(Region::South),
        _ => None,
    }
}

/// For a net that does not touch the layer anchor: inherit the region of a net
/// that shares one of its member boxes and is already resolved.
/// Multiple candidates → smallest lane index, tiebreak net name.
fn inherited_region(topos: &[NetTopology], idx: usize, resolved: &[bool]) -> Option<Region> {
    let topo = &topos[idx];
    let member_box_ids: Vec<i64> = topo.groups.iter().map(|g| g.box_id).collect();
    let mut candidates: Vec<(usize, &str, Region)> = Vec::new();
    for (j, other) in topos.iter().enumerate() {
        if j == idx || !resolved[j] {
            continue;
        }
        let shares = other
            .groups
            .iter()
            .any(|g| member_box_ids.contains(&g.box_id));
        if shares {
            candidates.push((other.lane.index, other.net_name.as_str(), other.lane.region));
        }
    }
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    candidates.first().map(|(_, _, r)| *r)
}

/// Majority IO direction of this net's pins on its anchor box.
fn anchor_pin_io(graph: &McVecGraph, topo: &NetTopology) -> Option<IoDirection> {
    let anchor_id = topo.anchor;
    let anchor_box = graph.boxes.iter().find(|b| b.id == anchor_id);
    let mut counts: Vec<(IoDirection, usize)> = Vec::new();
    if let Some(net) = find_net(graph, &topo.net_name) {
        for ep in &net.endpoints {
            if ep.box_id != anchor_id {
                continue;
            }
            let io = if ep.io_type != IoDirection::Unknown {
                ep.io_type
            } else {
                anchor_box
                    .and_then(|b| b.pins.iter().find(|p| p.id == ep.pin_id))
                    .map(|p| p.io)
                    .unwrap_or(IoDirection::Unknown)
            };
            if let Some(e) = counts.iter_mut().find(|(d, _)| *d == io) {
                e.1 += 1;
            } else {
                counts.push((io, 1));
            }
        }
    }
    // Prefer the most common definite direction; a definite direction beats
    // Unknown/Passive on ties.
    counts
        .into_iter()
        .max_by_key(|(io, cnt)| (*cnt, is_definite(*io) as u8))
        .map(|(io, _)| io)
}

fn is_definite(io: IoDirection) -> bool {
    io != IoDirection::Unknown && io != IoDirection::Passive
}

fn find_net<'a>(graph: &'a McVecGraph, name: &str) -> Option<&'a VizNet> {
    graph.nets.iter().find(|n| n.name == name)
}

/// Place boxes by topology. Writes x/y/w/h and entry_points on boxes,
/// sets geom_locked = true. Overrides FlowLayouter placement.
///
/// Pipeline: P2 place anchor → P3 resolve lanes → P4 place members.
/// P3 is the single producer of trunk coordinates; P4/P5 only consume them.
pub fn place_by_topology(graph: &mut McVecGraph, topos: &mut [NetTopology]) {
    if topos.is_empty() {
        return;
    }

    let layer_anchor = layer_anchor_id(topos);

    // P1: assign regions (semantic, pure)
    assign_regions(graph, topos);

    // P2: place the layer anchor box (pin side / box size / PinSlots)
    assign_anchor_slots(graph, layer_anchor, topos);

    // P3: resolve trunk lanes — single source of truth for trunk coordinates
    resolve_lanes(graph, topos);

    // P4: place member boxes by reading the lanes (never recompute x)
    place_members(graph, topos);

    // ★ PR4: after members are placed, re-envelope the lane span over all tap
    // points (anchor pins + member taps) so the trunk reaches every tap.
    envelop_lanes(graph, topos);
}

/// ★ P3: resolve trunk lanes. Pure function of (graph, topos) — both the layout
/// phase and the render phase call it on the post-anchor-placement graph, so the
/// lanes (trunk/junction x and trunk y-span) are guaranteed identical.
///
/// Region-aware (device_layout_v2.md sec.5): the lane axis is the anchor edge plus
/// the region's outward normal × (LANE_GAP + index × LANE_PITCH). The span here
/// is only the *seed* — the envelope of the net's ANCHOR pins along the trunk
/// direction. P4 distributes members inside it; once members are placed,
/// `envelop_lanes` recomputes the span as the envelope of all tap points
/// (anchor + members), so the trunk always reaches its taps without a
/// hand-tuned per-NetKind extension table.
pub fn resolve_lanes(graph: &McVecGraph, topos: &mut [NetTopology]) {
    // Per-(anchor, region) lane index so each region's channels count from 0.
    let mut lane_counter: BTreeMap<(i64, Region), usize> = BTreeMap::new();
    for topo in topos.iter_mut() {
        let key = (topo.anchor, topo.lane.region);
        let idx = lane_counter.entry(key).or_insert(0);
        topo.lane.index = *idx;
        *idx += 1;

        let (ax, ay, aw, ah) = anchor_box_rect(graph, topo.anchor);
        let (dx, dy) = topo.lane.region.outward();
        let gap = TRUNK_GAP + (topo.lane.index as f64) * TRUNK_GAP;
        let anchor_edge = match topo.lane.region {
            Region::West => ax,
            Region::East => ax + aw,
            Region::North => ay,
            Region::South => ay + ah,
        };
        // Trunk axis: x for W/E (vertical trunk), y for N/S (horizontal trunk).
        let axis = if topo.lane.region.axis_vertical() {
            anchor_edge + dx * gap
        } else {
            anchor_edge + dy * gap
        };
        // Initial span: anchor pins only (P4's distribution seed).
        let span = anchor_pin_range(graph, topo, topo.lane.region.axis_vertical());
        topo.lane.axis = axis;
        topo.lane.span = span;
    }
}

/// Bounding rect of a box (fallback 0,0,120,60 when missing).
fn anchor_box_rect(graph: &McVecGraph, box_id: i64) -> (f64, f64, f64, f64) {
    graph
        .boxes
        .iter()
        .find(|b| b.id == box_id)
        .map(|b| (b.x, b.y, b.w, b.h))
        .unwrap_or((0.0, 0.0, 120.0, 60.0))
}

/// Range of the anchor group's pins along the trunk direction.
/// `vertical_trunk`: true for W/E (y range), false for N/S (x range).
fn anchor_pin_range(graph: &McVecGraph, topo: &NetTopology, vertical_trunk: bool) -> (f64, f64) {
    let anchor_group = topo.groups.first();
    let anchor_box = anchor_group.and_then(|g| graph.boxes.iter().find(|b| b.id == g.box_id));
    if let (Some(b), Some(g)) = (anchor_box, anchor_group) {
        let mut vals: Vec<f64> = Vec::new();
        for &pid in &g.pin_ids {
            if let Some(s) = slot_of(b, pid) {
                let (px, py) = slot_point(b, s);
                vals.push(if vertical_trunk { py } else { px });
            }
        }
        if vals.is_empty() {
            let c = if vertical_trunk {
                b.y + b.h / 2.0
            } else {
                b.x + b.w / 2.0
            };
            (c, c)
        } else {
            let lo = vals.iter().cloned().fold(f64::MAX, f64::min);
            let hi = vals.iter().cloned().fold(f64::MIN, f64::max);
            (lo, hi)
        }
    } else {
        (300.0, 460.0)
    }
}

/// ★ PR4: span enveloping — recompute each net's lane span as the min/max of
/// ALL tap points along the trunk direction: the anchor pins plus every member's
/// entry pin (the point where its tap lands on this trunk).
///
/// This is the general fix for "the trunk does not reach the cap hanging below"
/// (device_layout_v2.md sec.5): trunk length is the envelope of the endpoint set,
/// not a hand-computed extension amount. The old per-NetKind
/// `extend_trunk_for_symbols` table (Ground +60 / Power −20 / Signal +40) is
/// deleted — it was patching a span that ignored member taps.
///
/// Runs after P4 `place_members` in the layout phase, and is replayed right
/// after `resolve_lanes` in the render phase. Both run on the same placed graph
/// (member boxes already carry PinSlots), so the recomputed span is identical —
/// keeping the `lanes_layout_match_render` invariant. `realize` then reads this
/// enveloped span and never touches a layout constant.
///
/// The span intentionally covers ONLY tap points (anchor + members). Terminal
/// symbols (Ground / NetLabel / BusLabel) do NOT extend it: `realize` hangs
/// them off the trunk's far end on their own short wire, so the trunk itself
/// never runs through another component (which extending span_hi used to do).
pub fn envelop_lanes(graph: &McVecGraph, topos: &mut [NetTopology]) {
    for topo in topos.iter_mut() {
        let vertical_trunk = topo.lane.region.axis_vertical();
        let mut vals: Vec<f64> = Vec::new();

        // Anchor pins: each tooth lands on the trunk at the pin's position.
        if let Some(group) = topo.groups.first() {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) {
                for &pid in &group.pin_ids {
                    if let Some(s) = slot_of(b, pid) {
                        let (px, py) = slot_point(b, s);
                        vals.push(if vertical_trunk { py } else { px });
                    }
                }
            }
        }

        // Member taps: the exact point `realize` connects to the trunk (group's
        // first pin, read from the placed box's PinSlots).
        for group in topo.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
                continue;
            };
            let (mx, my) = member_pin_point(b, group);
            vals.push(if vertical_trunk { my } else { mx });
        }

        if !vals.is_empty() {
            let lo = vals.iter().cloned().fold(f64::MAX, f64::min);
            let hi = vals.iter().cloned().fold(f64::MIN, f64::max);
            topo.lane.span = (lo, hi);
        }
    }
}

/// Absolute coordinate of a pin slot (single source of truth).
fn slot_point(b: &crate::vector::graph::McVecBox, s: &PinSlot) -> (f64, f64) {
    match s.side {
        EntrySide::Top => (b.x + b.w * s.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * s.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * s.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * s.offset),
    }
}

/// P4: place member boxes, reading trunk coordinates from `topo.lane`.
fn place_members(graph: &mut McVecGraph, topos: &[NetTopology]) {
    for (idx, topo) in topos.iter().enumerate() {
        place_members_for_topo(graph, topos, idx, topo);
    }
}

// ============================================================================
// ★ E3: MemberRole — electrical role determines placement (device_layout_v2.md sec.3.3)
// ============================================================================

/// Electrical role of a member box, decided by where the member's OTHER pin's
/// net Region lies relative to this net's Region:
///   * other Region opposite (W↔E / N↔S) → `Series`: spans between two trunks
///   * other Region orthogonal (W vs S, etc.) → `Shunt`: hangs off the trunk
///   * single pin → `Stub`; three+ pins → `Sink`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberRole {
    Series,
    Shunt,
    Stub,
    Sink,
}

/// The effective region of the other net a member's other pin connects to,
/// plus that net's trunk axis (for Series). Ground nets with an inherited W/E
/// lane still hang South (the ground channel always runs downward).
fn other_net_info(topos: &[NetTopology], idx: usize, group: &PinGroup) -> Option<(Region, f64)> {
    let this_pins: std::collections::BTreeSet<i64> = group.pin_ids.iter().cloned().collect();
    for (j, other) in topos.iter().enumerate() {
        if j == idx {
            continue;
        }
        if let Some(g) = other.groups.iter().find(|g| g.box_id == group.box_id) {
            if g.pin_ids.iter().any(|p| !this_pins.contains(p)) {
                let region = other.lane.region;
                let eff = if other.net_kind == NetKind::Ground
                    && (region == Region::West || region == Region::East)
                {
                    Region::South
                } else {
                    region
                };
                return Some((eff, other.lane.axis));
            }
        }
    }
    None
}

fn is_opposite_region(a: Region, b: Region) -> bool {
    matches!(
        (a, b),
        (Region::West, Region::East)
            | (Region::East, Region::West)
            | (Region::North, Region::South)
            | (Region::South, Region::North)
    )
}

fn member_role_of(
    member: &crate::vector::graph::McVecBox,
    topo: &NetTopology,
    other: Option<Region>,
) -> MemberRole {
    match member.pins.len() {
        0 | 1 => MemberRole::Stub,
        2 => match other {
            Some(ro) if is_opposite_region(topo.lane.region, ro) => MemberRole::Series,
            _ => MemberRole::Shunt,
        },
        _ => MemberRole::Sink,
    }
}

/// Shunt slots: the pin on this net (entry) faces the tap, the other pin
/// (exit) faces the hang direction (outward). Both at mid-edge.
fn assign_shunt_slots(
    b: &mut crate::vector::graph::McVecBox,
    entry_pin_id: i64,
    entry_side: EntrySide,
) {
    let exit_side = opposite_side(entry_side);
    let connected: std::collections::HashSet<i64> =
        b.entry_points.iter().map(|ep| ep.pin_id).collect();
    b.slots.clear();
    for (i, p) in b.pins.iter().enumerate() {
        let side = if p.id == entry_pin_id {
            entry_side
        } else {
            exit_side
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
    // entry_points connectivity-only side sync
    for ep in b.entry_points.iter_mut() {
        if let Some(slot) = b.slots.iter().find(|s| s.pin_id == ep.pin_id) {
            ep.side = slot.side;
            ep.offset = slot.offset;
        }
    }
}

/// Place the non-anchor member boxes of one net by their electrical role.
/// Shunt members hang along the other net's outward direction (decoupling caps
/// hang vertically toward Ground); Series members span between two trunks;
/// Stub/Sink members stay outward of the trunk (device_layout_v2.md sec.6 P4).
fn place_members_for_topo(
    graph: &mut McVecGraph,
    topos: &[NetTopology],
    idx: usize,
    topo: &NetTopology,
) {
    let (dx, dy) = topo.lane.region.outward();
    let vertical_trunk = topo.lane.region.axis_vertical();
    let axis = topo.lane.axis;
    // Distribute members along the lane span (the actual trunk extent, extended
    // for symbols by P3) so member taps always land on the trunk.
    let (span_lo, span_hi) = topo.lane.span;

    let non_anchor: Vec<&PinGroup> = topo.groups.iter().skip(1).collect();
    let member_count = non_anchor.len();
    if member_count == 0 {
        return;
    }

    // The side of a member box that faces the trunk (opposite the region's
    // entry edge). Used for Stub/Sink and as the default for Shunt.
    let inner_side = opposite_side(topo.lane.region.entry_side());

    for (i, group) in non_anchor.iter().enumerate() {
        let Some(member_box) = graph.boxes.iter_mut().find(|b| b.id == group.box_id) else {
            continue;
        };
        if member_box.geom_locked {
            continue;
        }

        let other = other_net_info(topos, idx, group);
        let role = member_role_of(member_box, topo, other.map(|(r, _)| r));

        // Distribute along the trunk span (interior points, not the ends).
        let frac = if member_count > 1 {
            (i as f64 + 1.0) / (member_count as f64 + 1.0)
        } else {
            0.5
        };
        let along = span_lo + (span_hi - span_lo) * frac;
        // Tap point on this trunk at `along`.
        let (tap_x, tap_y) = if vertical_trunk {
            (axis, along)
        } else {
            (along, axis)
        };

        match role {
            MemberRole::Shunt => {
                // Hang along the OTHER net's outward direction. Other in N/S →
                // vertical (h > w, the decoupling-cap-to-ground look); other in
                // W/E → horizontal. Fall back to this net's outward when the
                // other pin's net is unknown.
                let hang_region = other.map(|(r, _)| r).unwrap_or(topo.lane.region);
                let (hdx, hdy) = hang_region.outward();
                let (w, h) = if hang_region.axis_vertical() {
                    (TWO_PIN_SYMBOL_W, TWO_PIN_SYMBOL_H)
                } else {
                    (TWO_PIN_SYMBOL_H, TWO_PIN_SYMBOL_W)
                };
                member_box.w = w;
                member_box.h = h;
                if hdy != 0.0 {
                    // Vertical hang (N/S): entry pin at the tap, body extends down/up.
                    member_box.x = tap_x - w / 2.0;
                    member_box.y = if hdy > 0.0 { tap_y } else { tap_y - h };
                } else {
                    // Horizontal hang (W/E): entry pin at the tap, body extends right/left.
                    member_box.x = if hdx > 0.0 { tap_x } else { tap_x - w };
                    member_box.y = tap_y - h / 2.0;
                }
                member_box.geom_locked = true;
                let entry_side = opposite_side(hang_region.entry_side());
                let entry_pin_id = group.pin_ids.first().copied().unwrap_or_default();
                assign_shunt_slots(member_box, entry_pin_id, entry_side);
            }
            MemberRole::Series => {
                // Span between this trunk and the opposite trunk.
                let (w, h) = (TWO_PIN_SYMBOL_W, TWO_PIN_SYMBOL_H);
                member_box.w = w;
                member_box.h = h;
                let other_axis = other.map(|(_, ax)| ax).unwrap_or(axis + MEMBER_GAP * 2.0);
                let x_lo = axis.min(other_axis);
                let x_hi = axis.max(other_axis);
                member_box.x = (x_lo + x_hi) / 2.0 - w / 2.0;
                member_box.y = along - h / 2.0;
                member_box.geom_locked = true;
                for ep in &mut member_box.entry_points {
                    ep.side = inner_side;
                }
                assign_pin_slots(member_box, inner_side);
            }
            MemberRole::Stub => {
                // Perpendicular short hang, alternate above/below (or left/right).
                let (w, h) = (member_box.w.max(40.0), member_box.h.max(20.0));
                member_box.w = w;
                member_box.h = h;
                if vertical_trunk {
                    let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                    member_box.x = axis + dx * (w / 2.0 + MEMBER_GAP);
                    member_box.y = tap_y + side * (h / 2.0 + MEMBER_GAP);
                } else {
                    let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                    member_box.x = tap_x + side * (w / 2.0 + MEMBER_GAP);
                    member_box.y = axis + dy * (h / 2.0 + MEMBER_GAP);
                }
                member_box.geom_locked = true;
                for ep in &mut member_box.entry_points {
                    ep.side = inner_side;
                }
                assign_pin_slots(member_box, inner_side);
            }
            MemberRole::Sink => {
                // Multi-pin device: distributed along the trunk, pins face the trunk.
                if member_box.w <= 0.0 {
                    member_box.w = 80.0;
                }
                if member_box.h <= 0.0 {
                    member_box.h = 20.0;
                }
                let (ox, oy) = (dx * MEMBER_GAP, dy * MEMBER_GAP);
                if vertical_trunk {
                    member_box.x = axis + ox;
                    member_box.y = along - member_box.h / 2.0;
                } else {
                    member_box.x = along - member_box.w / 2.0;
                    member_box.y = axis + oy;
                }
                member_box.geom_locked = true;
                for ep in &mut member_box.entry_points {
                    ep.side = inner_side;
                }
                assign_pin_slots(member_box, inner_side);
            }
        }
    }
}

fn opposite_side(side: EntrySide) -> EntrySide {
    match side {
        EntrySide::Top => EntrySide::Bottom,
        EntrySide::Bottom => EntrySide::Top,
        EntrySide::Left => EntrySide::Right,
        EntrySide::Right => EntrySide::Left,
    }
}

/// ★ P2: assign anchor pin slots by Region (device_layout_v2.md sec.4).
/// Pins inherit the Region of the net they belong to; box size is driven by the
/// single most-crowded side, not the total pin count.
fn assign_anchor_slots(graph: &mut McVecGraph, anchor_id: i64, topos: &[NetTopology]) {
    let Some(anchor_box) = graph.boxes.iter_mut().find(|b| b.id == anchor_id) else {
        return;
    };

    // pin_id → EntrySide, from the nets this pin belongs to (first net wins,
    // deterministic by topo order).
    let mut pin_side: BTreeMap<i64, EntrySide> = BTreeMap::new();
    for topo in topos.iter().filter(|t| t.anchor == anchor_id) {
        let side = topo.lane.region.entry_side();
        if let Some(g) = topo.groups.first() {
            for &pid in &g.pin_ids {
                pin_side.entry(pid).or_insert(side);
            }
        }
    }

    // Bucket physical pins by side, preserving physical order for stability.
    let mut west: Vec<i64> = Vec::new();
    let mut east: Vec<i64> = Vec::new();
    let mut north: Vec<i64> = Vec::new();
    let mut south: Vec<i64> = Vec::new();
    for p in &anchor_box.pins {
        match pin_side.get(&p.id).copied().unwrap_or(EntrySide::Right) {
            EntrySide::Left => west.push(p.id),
            EntrySide::Right => east.push(p.id),
            EntrySide::Top => north.push(p.id),
            EntrySide::Bottom => south.push(p.id),
        }
    }

    // ★ Box size by single-side max (device_layout_v2.md sec.4.5).
    let box_h = west.len().max(east.len()).max(1) as f64 * PIN_PITCH + 2.0 * PIN_MARGIN;
    let box_w = (north.len().max(south.len()) as f64 * PIN_PITCH + 2.0 * PIN_MARGIN).max(MIN_BOX_W);
    anchor_box.x = 80.0;
    anchor_box.y = 100.0;
    anchor_box.w = box_w;
    anchor_box.h = box_h;
    anchor_box.geom_locked = true;

    // Assign slots per side.
    anchor_box.slots.clear();
    assign_side_slots(anchor_box, &west, EntrySide::Left);
    assign_side_slots(anchor_box, &east, EntrySide::Right);
    assign_side_slots(anchor_box, &north, EntrySide::Top);
    assign_side_slots(anchor_box, &south, EntrySide::Bottom);

    // ★ Sync entry_points with the per-side slot layout: the render draws pins
    // from entry_points (side + offset), so both must agree. Geometry stays in
    // PinSlot (single source of truth); entry_points mirror it for rendering.
    // Device-layer graphs often carry empty entry_points, so synthesize them
    // for connected pins from the slots (unconnected pins stay NC / X-mark).
    let mut connected_pins: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for topo in topos.iter().filter(|t| t.anchor == anchor_id) {
        if let Some(g) = topo.groups.first() {
            connected_pins.extend(g.pin_ids.iter().cloned());
        }
    }
    for ep in anchor_box.entry_points.iter_mut() {
        if let Some(slot) = anchor_box.slots.iter().find(|s| s.pin_id == ep.pin_id) {
            ep.side = slot.side;
            ep.offset = slot.offset;
        }
    }
    if anchor_box.entry_points.is_empty() {
        for &pid in &connected_pins {
            let Some(slot) = anchor_box.slots.iter().find(|s| s.pin_id == pid) else {
                continue;
            };
            let name = anchor_box
                .pins
                .iter()
                .find(|p| p.id == pid)
                .map(|p| {
                    if p.description.is_empty() {
                        p.pin_id.clone()
                    } else {
                        p.description.clone()
                    }
                })
                .unwrap_or_else(|| pid.to_string());
            anchor_box
                .entry_points
                .push(crate::vector::graph::boxdef::EntryPoint {
                    pin_id: pid,
                    pin_name: name,
                    side: slot.side,
                    offset: slot.offset,
                });
        }
    }
}

/// Assign PinSlots for the given pins on one box side.
fn assign_side_slots(b: &mut crate::vector::graph::McVecBox, pin_ids: &[i64], side: EntrySide) {
    let n = pin_ids.len();
    if n == 0 {
        return;
    }
    let connected: std::collections::HashSet<i64> =
        b.entry_points.iter().map(|ep| ep.pin_id).collect();
    for (i, &pid) in pin_ids.iter().enumerate() {
        let name = b
            .pins
            .iter()
            .find(|p| p.id == pid)
            .map(|p| {
                if p.description.is_empty() {
                    p.pin_id.clone()
                } else {
                    p.description.clone()
                }
            })
            .unwrap_or_else(|| pid.to_string());
        b.slots.push(PinSlot {
            pin_id: pid,
            number: i as u32,
            name,
            side,
            offset: (i as f64 + 1.0) / (n as f64 + 1.0),
            connected: connected.contains(&pid),
        });
    }
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

/// ★ F1: find a PinSlot by pin_id. Single source of truth for pin geometry.
fn slot_of(b: &crate::vector::graph::McVecBox, pin_id: i64) -> Option<&PinSlot> {
    b.slots.iter().find(|s| s.pin_id == pin_id)
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
    /// Direction from the tree's attachment node toward this symbol — the
    /// direction of the stub wire that connects it. The renderer uses it to
    /// place label text on the side away from the tree (so a sideways label
    /// never writes over the trunk).
    pub dir: (f64, f64),
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
/// ★ P5 is read-only for coordinates: trunk axis and span come exclusively from
/// `topo.lane` (axis written by P3 resolve_lanes, span re-enveloped over all
/// tap points by PR4 `envelop_lanes`). `realize` owns no layout constant — it
/// only reads the Lane and the placed PinSlots, then connects the points.
pub fn realize(topo: &NetTopology, graph: &McVecGraph) -> EquiTree {
    let mut segments: Vec<Segment> = Vec::new();
    let mut degree_map: BTreeMap<(i64, i64), u8> = BTreeMap::new();

    let anchor_group = topo.groups.first();
    let anchor_box = anchor_group.and_then(|g| graph.boxes.iter().find(|b| b.id == g.box_id));

    // ★ P3 lane — single source of truth. axis = trunk coordinate (x for W/E,
    // y for N/S); span = extent along the trunk direction.
    let lane = topo.lane;
    let vertical_trunk = lane.region.axis_vertical();
    let axis = lane.axis;
    let (span_lo, span_hi) = lane.span;

    // Anchor pin points (from slots — single source of truth).
    let anchor_pins: Vec<(f64, f64)> = anchor_box
        .map(|b| {
            anchor_group
                .unwrap()
                .pin_ids
                .iter()
                .filter_map(|&pid| slot_of(b, pid).map(|s| slot_point(b, s)))
                .collect()
        })
        .unwrap_or_default();

    if anchor_pins.is_empty() {
        // Fallback: no anchor pins, return empty tree
        return EquiTree {
            net_name: topo.net_name.clone(),
            net_kind: topo.net_kind.clone(),
            segments,
            junction_dots: vec![],
            symbols: build_symbols(topo, lane, graph),
        };
    }

    // Trunk: one line along the trunk direction at `axis`.
    let trunk = if vertical_trunk {
        Segment {
            x1: axis,
            y1: span_lo,
            x2: axis,
            y2: span_hi,
        }
    } else {
        Segment {
            x1: span_lo,
            y1: axis,
            x2: span_hi,
            y2: axis,
        }
    };
    add_segment(&trunk, &mut segments, &mut degree_map);

    // Teeth: from each anchor pin to the trunk (perpendicular to the trunk).
    for &(px, py) in &anchor_pins {
        let seg = if vertical_trunk {
            Segment {
                x1: px,
                y1: py,
                x2: axis,
                y2: py,
            }
        } else {
            Segment {
                x1: px,
                y1: py,
                x2: px,
                y2: axis,
            }
        };
        add_segment(&seg, &mut segments, &mut degree_map);
    }

    // Member taps: from each member pin to the trunk (perpendicular).
    for group in topo.groups.iter().skip(1) {
        let Some(member_box) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
            continue;
        };
        let (mx, my) = member_pin_point(member_box, group);
        let seg = if vertical_trunk {
            Segment {
                x1: mx,
                y1: my,
                x2: axis,
                y2: my,
            }
        } else {
            Segment {
                x1: mx,
                y1: my,
                x2: mx,
                y2: axis,
            }
        };
        add_segment(&seg, &mut segments, &mut degree_map);
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

    // Symbols — read from the lane (P3), never recompute junction/trunk x.
    let mut symbols = build_symbols(topo, lane, graph);

    // ★ Terminal-symbol connection: the Ground / NetLabel / BusLabel symbol
    // hangs off the trunk's far end on its own short wire. Instead of forcing a
    // direction (up for labels, or blindly extending the trunk end for GND),
    // find the node where the symbol attaches (the trunk's far end) and pick
    // the FIRST free direction (down → up → left → right): free means the stub
    // wire neither passes through a component box nor overlaps an existing
    // segment of this net. This keeps the GND/bus connection clean and never
    // on top of another line. Added after the junction-dot pass, so the stub
    // never creates a spurious dot.
    let (node_x, node_y) = if vertical_trunk {
        (axis, span_hi)
    } else {
        (span_hi, axis)
    };
    for sym in symbols.iter_mut() {
        if !matches!(
            sym.kind,
            TreeSymbolKind::Ground | TreeSymbolKind::NetLabel | TreeSymbolKind::BusLabel
        ) {
            continue;
        }
        // Try the far end first; if every direction there is crowded (a member
        // box hugs the trunk end), fall back to the near end so the symbol
        // still finds a clean spot instead of drawing through the crowd.
        let (attach, dir) = match pick_stub_dir(graph, &segments, (node_x, node_y)) {
            Some(dir) => ((node_x, node_y), Some(dir)),
            None => {
                let (alt_x, alt_y) = if vertical_trunk {
                    (axis, span_lo)
                } else {
                    (span_lo, axis)
                };
                match pick_stub_dir(graph, &segments, (alt_x, alt_y)) {
                    Some(dir) => ((alt_x, alt_y), Some(dir)),
                    // Truly no free spot on either end (a degenerate trunk with
                    // a member box right on the node). Sit the symbol at the
                    // node WITHOUT a wire, so it never draws a line through
                    // another component.
                    None => ((node_x, node_y), None),
                }
            }
        };
        if let Some(dir) = dir {
            sym.dir = dir;
            sym.x = attach.0 + dir.0 * SYMBOL_DROP;
            sym.y = attach.1 + dir.1 * SYMBOL_DROP;
            add_segment(
                &Segment {
                    x1: attach.0,
                    y1: attach.1,
                    x2: sym.x,
                    y2: sym.y,
                },
                &mut segments,
                &mut degree_map,
            );
        } else {
            sym.dir = (0.0, 1.0);
            sym.x = attach.0;
            sym.y = attach.1;
        }
    }

    EquiTree {
        net_name: topo.net_name.clone(),
        net_kind: topo.net_kind.clone(),
        segments,
        junction_dots,
        symbols,
    }
}

/// Pick the first free direction (down, up, left, right) for a terminal-symbol
/// stub from the trunk's far end. A direction is free when the stub wire
/// neither passes through a component box nor overlaps an existing segment of
/// this net beyond the attachment node (a collinear continuation of the trunk,
/// e.g. hanging down from a vertical trunk, is not an overlap). `None` means
/// every direction is blocked (e.g. a member box crowds the node).
fn pick_stub_dir(graph: &McVecGraph, segments: &[Segment], node: (f64, f64)) -> Option<(f64, f64)> {
    for (dx, dy) in [(0.0, 1.0), (0.0, -1.0), (-1.0, 0.0), (1.0, 0.0)] {
        let ex = node.0 + dx * SYMBOL_DROP;
        let ey = node.1 + dy * SYMBOL_DROP;
        let hits_box = graph.boxes.iter().any(|b| {
            if matches!(
                b.kind,
                BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
            ) {
                return false;
            }
            segment_hits_box(node.0, node.1, ex, ey, b.x, b.y, b.w, b.h)
        });
        if hits_box {
            continue;
        }
        let overlaps = segments
            .iter()
            .any(|s| segments_overlap(node.0, node.1, ex, ey, s.x1, s.y1, s.x2, s.y2));
        if overlaps {
            continue;
        }
        return Some((dx, dy));
    }
    None
}

/// Does the axis-aligned segment (ax,ay)-(bx,by) pass through the interior of
/// the box (x,y)-(x+w,y+h)? Grazing along an edge counts as a hit.
fn segment_hits_box(ax: f64, ay: f64, bx: f64, by: f64, x: f64, y: f64, w: f64, h: f64) -> bool {
    let eps = 0.5;
    if (ax - bx).abs() < 0.01 {
        // vertical segment
        if !(x - eps <= ax && ax <= x + w + eps) {
            return false;
        }
        let lo = ay.min(by);
        let hi = ay.max(by);
        return lo.max(y - eps) + eps < hi.min(y + h + eps);
    }
    if (ay - by).abs() < 0.01 {
        // horizontal segment
        if !(y - eps <= ay && ay <= y + h + eps) {
            return false;
        }
        let lo = ax.min(bx);
        let hi = ax.max(bx);
        return lo.max(x - eps) + eps < hi.min(x + w + eps);
    }
    false
}

/// Do two axis-aligned segments share more than a single endpoint — either a
/// parallel overlap over a length, or a proper interior crossing?
fn segments_overlap(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    dx: f64,
    dy: f64,
) -> bool {
    let eps = 0.5;
    let va = (ax - bx).abs() < eps; // seg1 vertical
    let vb = (cx - dx).abs() < eps; // seg2 vertical
    let ha = (ay - by).abs() < eps; // seg1 horizontal
    let hb = (cy - dy).abs() < eps; // seg2 horizontal
    if va && vb {
        if (ax - cx).abs() > eps {
            return false;
        }
        return ay.min(by).max(cy.min(dy)) + eps < ay.max(by).min(cy.max(dy));
    }
    if ha && hb {
        if (ay - cy).abs() > eps {
            return false;
        }
        return ax.min(bx).max(cx.min(dx)) + eps < ax.max(bx).min(cx.max(dx));
    }
    if va && hb {
        // seg1 vertical, seg2 horizontal — crossing at (ax, cy)
        if !(cx.min(dx) - eps <= ax && ax <= cx.max(dx) + eps) {
            return false;
        }
        if !(ay.min(by) - eps <= cy && cy <= ay.max(by) + eps) {
            return false;
        }
        let on_a = (cy - ay).abs() > eps && (cy - by).abs() > eps;
        let on_b = (ax - cx).abs() > eps && (ax - dx).abs() > eps;
        return on_a && on_b;
    }
    if ha && vb {
        // seg1 horizontal, seg2 vertical — crossing at (cx, ay)
        if !(cx.min(dx) - eps <= ax && ax <= cx.max(dx) + eps) {
            return false;
        }
        if !(ay.min(by) - eps <= cy && cy <= ay.max(by) + eps) {
            return false;
        }
        let on_a = (cx - ax).abs() > eps && (cx - bx).abs() > eps;
        let on_b = (ay - cy).abs() > eps && (ay - dy).abs() > eps;
        return on_a && on_b;
    }
    false
}

/// Absolute position of a member group's first pin (from PinSlots).
fn member_pin_point(member_box: &crate::vector::graph::McVecBox, group: &PinGroup) -> (f64, f64) {
    if let Some(&pid) = group.pin_ids.first() {
        if let Some(slot) = slot_of(member_box, pid) {
            return slot_point(member_box, slot);
        }
    }
    (member_box.x, member_box.y + member_box.h / 2.0)
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

fn build_symbols(topo: &NetTopology, lane: Lane, graph: &McVecGraph) -> Vec<TreeSymbol> {
    let mut symbols = Vec::new();

    let vertical_trunk = lane.region.axis_vertical();
    // ★ P3 lane — single source of truth: axis = trunk coordinate, span = extent.
    let axis = lane.axis;
    let (_, span_hi) = lane.span;

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
                // Ground symbol hangs BELOW the trunk's far end (span_hi), on
                // its own short wire. realize re-picks a free direction and
                // wires it; here we only seed the default (down).
                let (x, y) = if vertical_trunk {
                    (axis, span_hi + SYMBOL_DROP)
                } else {
                    (span_hi, axis + SYMBOL_DROP)
                };
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::Ground,
                    x,
                    y,
                    label: String::new(),
                    dir: (0.0, 1.0),
                });
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
                // Seed position: off the trunk's far end (down by default);
                // realize re-picks a free direction and wires it.
                let (x, y) = if vertical_trunk {
                    (axis, span_hi + SYMBOL_DROP)
                } else {
                    (span_hi, axis + SYMBOL_DROP)
                };
                symbols.push(TreeSymbol {
                    kind,
                    x,
                    y,
                    label: name.clone(),
                    dir: (0.0, 1.0),
                });
            }
            Terminal::Port { name } => {
                let (x, y) = if vertical_trunk {
                    (axis, span_hi + SYMBOL_DROP)
                } else {
                    (span_hi, axis + SYMBOL_DROP)
                };
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::PortLabel,
                    x: x + 140.0,
                    y,
                    label: name.clone(),
                    dir: (0.0, 1.0),
                });
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
    let mut topos = build_topology(graph);
    eprintln!(
        "[equi-tree] layout_device_layer: {} nets, {} topos, {} boxes",
        graph.nets.len(),
        topos.len(),
        graph.boxes.len(),
    );
    place_by_topology(graph, &mut topos);
    // Log topology regions (assigned by P1 inside place_by_topology).
    for t in &topos {
        eprintln!(
            "[equi-tree]   topo: net='{}' anchor={} groups={} region={:?}",
            t.net_name,
            t.anchor,
            t.groups.len(),
            t.lane.region,
        );
    }
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
/// Replays P0 (build_topology) → P1 (assign_regions) → P3 (resolve_lanes) → P5 (realize).
/// Does NOT modify the graph.
pub fn build_all_trees(graph: &McVecGraph) -> Vec<EquiTree> {
    let mut topos = build_topology(graph);
    eprintln!(
        "[equi-tree] build_all_trees: {} nets, {} topos",
        graph.nets.len(),
        topos.len(),
    );
    // ★ P1 + P3 replay: assign regions and resolve lanes identically to the
    // layout phase, so render-side trunk coordinates always match the members.
    assign_regions(graph, &mut topos);
    resolve_lanes(graph, &mut topos);
    // ★ PR4: span enveloping replay — members are already placed in the graph
    // (layout phase), so the recomputed span (anchor + member taps) matches the
    // layout phase exactly; realize then reads only this enveloped Lane.
    envelop_lanes(graph, &mut topos);
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

/// ★ Content-adaptive canvas (fix "circuit clipped at negative coordinates").
///
/// The SVG viewBox is `0 0 W H`, so any content with a negative x/y (West
/// trunks, left-side caps, symbols above the anchor) is silently clipped by the
/// current canvas logic which only grows the max (positive) extent. Here we:
///   1. compute the bounding box of ALL rendered content (boxes + tree segments
///      + junction dots + symbols, min and max in both axes),
///   2. shift every box so the content starts at the canvas margin (brings
///      negative-x/y content back into the visible `0 0 W H` viewBox),
///   3. return a canvas sized to the content (`content + 2×margin`).
///
/// The render phase calls `build_all_trees` again on the shifted graph, so the
/// re-derived trees are consistent with the shifted boxes.
pub fn fit_content_to_canvas(graph: &mut McVecGraph, trees: &[EquiTree]) -> (f64, f64) {
    let margin = crate::viz::layout::normalize::CANVAS_MARGIN;

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    // Boxes (including zero-size labels; their symbols render small).
    for b in &graph.boxes {
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
        max_x = max_x.max(b.x + b.w);
        max_y = max_y.max(b.y + b.h);
    }
    // Tree geometry (segments / junction dots / symbols).
    for t in trees {
        for s in &t.segments {
            min_x = min_x.min(s.x1).min(s.x2);
            min_y = min_y.min(s.y1).min(s.y2);
            max_x = max_x.max(s.x1).max(s.x2);
            max_y = max_y.max(s.y1).max(s.y2);
        }
        for &(jx, jy) in &t.junction_dots {
            min_x = min_x.min(jx);
            min_y = min_y.min(jy);
            max_x = max_x.max(jx);
            max_y = max_y.max(jy);
        }
        for sym in &t.symbols {
            // rough text width estimate: ~7px per char at font-size 10
            let label_w = sym.label.len() as f64 * 7.0;
            min_x = min_x.min(sym.x);
            min_y = min_y.min(sym.y);
            max_x = max_x.max(sym.x + label_w);
            max_y = max_y.max(sym.y + 24.0);
        }
    }
    if min_x == f64::MAX {
        return (200.0, 100.0); // no content
    }

    // Shift everything so the content starts at the margin.
    let shift_x = margin - min_x;
    let shift_y = margin - min_y;
    if shift_x.abs() > 0.01 || shift_y.abs() > 0.01 {
        for b in &mut graph.boxes {
            b.x += shift_x;
            b.y += shift_y;
            for lp in &mut b.label_placements {
                lp.x += shift_x;
                lp.y += shift_y;
            }
        }
    }

    let w = (max_x - min_x) + 2.0 * margin;
    let h = (max_y - min_y) + 2.0 * margin;
    // Modest floor so tiny layers still get a usable "paper".
    (w.max(300.0), h.max(200.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{BoxPin, IoSummary, PortDir};
    use crate::vector::graph::kinds::{BoxKind, NetKind};
    use crate::vector::graph::netdef::{EndpointRef, IoDirection, NetRole};
    use crate::vector::graph::symbol::Symbol;
    use crate::vector::graph::{LayerStyle, McVecBox, VizNet};

    fn mk_ic(id: i64, pin_count: usize, pin_ids: &[i64]) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            format!("U{id}"),
            "IC.TEST".into(),
            BoxKind::MultiPin,
            Symbol::Ic,
            None,
            None,
            pin_count,
            IoSummary::new(),
            format!("U{id}"),
            vec![],
        );
        for (i, pid) in pin_ids.iter().enumerate() {
            b.pins.push(BoxPin {
                id: *pid,
                pin_id: (i + 1).to_string(),
                description: String::new(),
                io: IoDirection::Unknown,
                port_dir: PortDir::None,
            });
        }
        b
    }

    fn mk_two_pin(id: i64, name: &str, pin_ids: &[i64]) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            "CAP.TEST".into(),
            BoxKind::TwoPin,
            Symbol::Capacitor,
            None,
            None,
            2,
            IoSummary::new(),
            name.into(),
            vec![],
        );
        for (i, pid) in pin_ids.iter().enumerate() {
            b.pins.push(BoxPin {
                id: *pid,
                pin_id: (i + 1).to_string(),
                description: String::new(),
                io: IoDirection::Unknown,
                port_dir: PortDir::None,
            });
        }
        b
    }

    fn mk_net(nid: i64, name: &str, kind: NetKind, endpoints: &[(i64, i64)]) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            kind,
            NetRole::Signal,
            endpoints
                .iter()
                .map(|&(b, p)| EndpointRef::new(b, p, ""))
                .collect(),
        )
    }

    /// Build a two-net device layer: a multi-pin anchor IC + two series caps,
    /// one per net. Covers both a vertical-trunk lane (PWR, 2 anchor pins) and a
    /// single-pin horizontal-trunk lane (GND).
    fn build_test_graph() -> McVecGraph {
        let mut g = McVecGraph::new(100, "test".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_ic(1, 3, &[11, 12, 13]));
        g.boxes.push(mk_two_pin(2, "CAP_1", &[21, 22]));
        g.boxes.push(mk_two_pin(3, "CAP_2", &[31, 32]));
        g.nets.push(mk_net(
            201,
            "PWR",
            NetKind::Power,
            &[(1, 11), (1, 12), (2, 21)],
        ));
        g.nets
            .push(mk_net(202, "GND", NetKind::Ground, &[(1, 13), (3, 31)]));
        g
    }

    /// ★ Assertion 1 (device_layout_v2.md §7.1): the layout phase and the render
    /// phase must compute identical lanes. Both call `assign_regions` + `resolve_lanes`
    /// + `envelop_lanes` on the post-anchor-placement graph; if they drift, members
    /// and trunks diverge ("two layers each compute x" bug class).
    #[test]
    fn lanes_layout_match_render() {
        let mut g = build_test_graph();
        let mut layout_topos = build_topology(&g);
        place_by_topology(&mut g, &mut layout_topos);

        // Render side: replay P0 + P1 + P3 + PR4-envelope on the same (now
        // placed) graph — members already carry PinSlots from the layout phase.
        let mut render_topos = build_topology(&g);
        assign_regions(&g, &mut render_topos);
        resolve_lanes(&g, &mut render_topos);
        envelop_lanes(&g, &mut render_topos);

        assert_eq!(layout_topos.len(), render_topos.len());
        for (a, b) in layout_topos.iter().zip(render_topos.iter()) {
            assert_eq!(
                a.lane, b.lane,
                "lane for net '{}' differs between layout and render",
                a.net_name
            );
        }
    }

    /// ★ Assertion 1b: two topologies on the same anchor must not share a lane —
    /// each gets its own channel offset, otherwise members would overlap.
    #[test]
    fn lanes_are_per_topology() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);
        assert!(topos.len() >= 2);
        assert_ne!(
            topos[0].lane.axis, topos[1].lane.axis,
            "two topologies on one anchor must occupy distinct lanes"
        );
    }

    /// ★ Assertion 2 (device_layout_v2.md §7.2): no dangling segments — every
    /// segment endpoint must land on a pin point, on a terminal symbol, or on
    /// another segment of the same net. Catches the "line crossing the whole
    /// graph" bug class.
    #[test]
    fn no_dangling_segments() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let trees = build_all_trees(&g);
        assert_eq!(trees.len(), topos.len());
        for (topo, tree) in topos.iter().zip(trees.iter()) {
            let dangling = dangling_segments(topo, tree, &g);
            assert!(
                dangling.is_empty(),
                "net '{}' has dangling segment endpoints: {:?}",
                topo.net_name,
                dangling
            );
        }
    }

    /// ★ Terminal-symbol wire hygiene: every stub that connects a Ground /
    /// NetLabel / BusLabel symbol to the tree must NOT pass through any
    /// component box. This is the "GND/bus connecting line must not lie on top
    /// of another element" requirement — realized picks a free direction.
    #[test]
    fn terminal_wires_clear_of_boxes() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);
        let trees = build_all_trees(&g);

        for tree in &trees {
            for sym in &tree.symbols {
                if !matches!(
                    sym.kind,
                    TreeSymbolKind::Ground | TreeSymbolKind::NetLabel | TreeSymbolKind::BusLabel
                ) {
                    continue;
                }
                for seg in &tree.segments {
                    let ends_at_sym =
                        (seg.x2 - sym.x).abs() < 0.01 && (seg.y2 - sym.y).abs() < 0.01;
                    if !ends_at_sym {
                        continue;
                    }
                    for b in &g.boxes {
                        if matches!(
                            b.kind,
                            BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
                        ) {
                            continue;
                        }
                        assert!(
                            !segment_hits_box(seg.x1, seg.y1, seg.x2, seg.y2, b.x, b.y, b.w, b.h),
                            "net '{}' terminal wire ({:.0},{:.0})-({:.0},{:.0}) crosses box '{}'",
                            tree.net_name,
                            seg.x1,
                            seg.y1,
                            seg.x2,
                            seg.y2,
                            b.name
                        );
                    }
                }
            }
        }
    }

    /// ★ PR4 assertion: the enveloped lane span must cover ALL tap points along
    /// the trunk direction — the anchor pins AND every member's tap (where its
    /// line lands on this trunk). This is what makes the trunk reach a cap
    /// hanging below the anchor's own pin range; before the envelope, the span
    /// only covered anchor pins (plus a hand-tuned per-NetKind table) and a
    /// member hanging beyond the anchor range left a dangling tap.
    #[test]
    fn span_envelops_member_taps() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        for topo in &topos {
            let vertical_trunk = topo.lane.region.axis_vertical();
            let mut taps: Vec<f64> = Vec::new();

            if let Some(group) = topo.groups.first() {
                let b = g.boxes.iter().find(|b| b.id == group.box_id).unwrap();
                for &pid in &group.pin_ids {
                    let s = slot_of(b, pid).unwrap();
                    let (px, py) = slot_point(b, s);
                    taps.push(if vertical_trunk { py } else { px });
                }
            }
            for group in topo.groups.iter().skip(1) {
                let b = g.boxes.iter().find(|b| b.id == group.box_id).unwrap();
                let (mx, my) = member_pin_point(b, group);
                taps.push(if vertical_trunk { my } else { mx });
            }

            let lo = taps.iter().cloned().fold(f64::MAX, f64::min);
            let hi = taps.iter().cloned().fold(f64::MIN, f64::max);
            assert!(
                topo.lane.span.0 <= lo + 0.001 && topo.lane.span.1 >= hi - 0.001,
                "net '{}' span {:?} does not envelop its tap points [{:.1}, {:.1}]",
                topo.net_name,
                topo.lane.span,
                lo,
                hi
            );
        }
    }

    /// ★ PR4: the trunk must extend to reach a member whose tap point lies BEYOND
    /// the anchor pin range — the classic decoupling cap whose other pin hangs
    /// down to a ground net farther out than the anchor's own ground pins.
    /// Before span enveloping the ground trunk ended at the anchor pins and the
    /// cap's ground tap dangled off the end.
    #[test]
    fn trunk_reaches_member_beyond_anchor_range() {
        let mut g = McVecGraph::new(300, "beyond".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_ic(1, 3, &[11, 12, 13]));
        g.boxes.push(mk_two_pin(2, "CAP_1", &[21, 22]));
        // Power rail (West): anchor pins 11,12 ↔ cap pin 21.
        g.nets.push(mk_net(
            401,
            "PWR",
            NetKind::Power,
            &[(1, 11), (1, 12), (2, 21)],
        ));
        // Ground (South): anchor pin 13 ↔ cap pin 22. The cap is placed by the
        // power net (hangs down from the West trunk), so its ground pin ends up
        // West of the anchor's own pin range.
        g.nets
            .push(mk_net(402, "GND", NetKind::Ground, &[(1, 13), (2, 22)]));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let trees = build_all_trees(&g);
        let gnd = topos.iter().find(|t| t.net_name == "GND").unwrap();
        let gnd_tree = trees.iter().find(|t| t.net_name == "GND").unwrap();

        // The cap's ground pin must be covered by the enveloped GND span
        // (horizontal trunk → span is the x-range).
        let cap = g.boxes.iter().find(|b| b.id == 2).unwrap();
        let gnd_grp = gnd.groups.iter().find(|g| g.box_id == 2).unwrap();
        let (mx, _) = member_pin_point(cap, gnd_grp);
        assert!(
            gnd.lane.span.0 <= mx + 0.001 && gnd.lane.span.1 >= mx - 0.001,
            "GND trunk span {:?} does not reach cap ground tap x={:.1}",
            gnd.lane.span,
            mx
        );

        // And the realized trunk actually touches the tap (no dangling segment).
        let dangling = dangling_segments(gnd, gnd_tree, &g);
        assert!(
            dangling.is_empty(),
            "GND has dangling segments: {:?}",
            dangling
        );
    }

    /// Collect segment indices whose endpoints are neither a net pin point, a
    /// terminal symbol position, nor on another segment of the same net.
    fn dangling_segments(topo: &NetTopology, tree: &EquiTree, graph: &McVecGraph) -> Vec<usize> {
        let mut pin_points: Vec<(f64, f64)> = Vec::new();
        for grp in &topo.groups {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == grp.box_id) {
                for &pid in &grp.pin_ids {
                    if let Some(s) = slot_of(b, pid) {
                        let (px, py) = match s.side {
                            EntrySide::Top => (b.x + b.w * s.offset, b.y),
                            EntrySide::Bottom => (b.x + b.w * s.offset, b.y + b.h),
                            EntrySide::Left => (b.x, b.y + b.h * s.offset),
                            EntrySide::Right => (b.x + b.w, b.y + b.h * s.offset),
                        };
                        pin_points.push((px, py));
                    }
                }
            }
        }
        let sym_points: Vec<(f64, f64)> = tree.symbols.iter().map(|s| (s.x, s.y)).collect();
        let on_point = |ex: f64, ey: f64, pts: &[(f64, f64)], tol: f64| -> bool {
            pts.iter()
                .any(|&(px, py)| (px - ex).abs() <= tol && (py - ey).abs() <= tol)
        };

        let mut dangling = Vec::new();
        for (idx, seg) in tree.segments.iter().enumerate() {
            for (ex, ey) in [(seg.x1, seg.y1), (seg.x2, seg.y2)] {
                let ok = on_point(ex, ey, &pin_points, 1.0)
                    || on_point(ex, ey, &sym_points, 16.0) // labels sit a few px off the trunk
                    || tree
                        .segments
                        .iter()
                        .enumerate()
                        .any(|(j, other)| j != idx && point_on_segment(ex, ey, other));
                if !ok {
                    dangling.push(idx);
                    break;
                }
            }
        }
        dangling
    }

    /// ★ Assertion 3 (device_layout_v2.md §7.3): no single side is overloaded —
    /// `max(side_count) ≤ ceil(total_pins / 2) + 1`. "5 pins on the right" fails.
    #[test]
    fn anchor_side_not_overloaded() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        // Anchor is box 1 (referenced by both topologies).
        let anchor = g.boxes.iter().find(|b| b.id == 1).expect("anchor box");
        let total = anchor.pins.len();
        let mut counts: [usize; 4] = [0; 4];
        for s in &anchor.slots {
            let idx = match s.side {
                EntrySide::Left => 0,
                EntrySide::Right => 1,
                EntrySide::Top => 2,
                EntrySide::Bottom => 3,
            };
            counts[idx] += 1;
        }
        let max_side = *counts.iter().max().unwrap();
        let bound = (total + 1) / 2 + 1; // ceil(total/2) + 1
        assert!(
            max_side <= bound,
            "side overloaded: max={} total={} bound={}",
            max_side,
            total,
            bound
        );
    }

    /// ★ Assertion 4 (device_layout_v2.md §7.4): every Ground-direction pin must
    /// live on the South edge (Region::South.entry_side()).
    #[test]
    fn ground_pins_on_south() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        for topo in &topos {
            if topo.lane.region != Region::South {
                continue;
            }
            let anchor_box = g.boxes.iter().find(|b| b.id == topo.anchor).unwrap();
            for &pid in &topo.groups.first().unwrap().pin_ids {
                let slot = anchor_box.slots.iter().find(|s| s.pin_id == pid).unwrap();
                assert_eq!(
                    slot.side,
                    EntrySide::Bottom,
                    "Ground pin {} of net '{}' is not on the South edge",
                    pid,
                    topo.net_name
                );
            }
        }
    }

    /// ★ Assertion 6 (device_layout_v2.md §7.6): the `[region] fallback` path is
    /// never hit — every net resolves to a definite Region by a direct rule or
    /// inheritance.
    #[test]
    fn no_region_fallback() {
        let g = build_test_graph();
        let mut topos = build_topology(&g);
        let fallbacks = assign_regions(&g, &mut topos);
        assert_eq!(fallbacks, 0, "unexpected [region] fallback hits");
    }

    /// ★ Assertion 5 (device_layout_v2.md §7.5): a two-pin passive whose OTHER
    /// pin net is N/S (Ground below a power rail) hangs vertically — h > w.
    /// This is the decoupling-cap-to-ground look.
    #[test]
    fn shunt_cap_hangs_vertical() {
        let mut g = McVecGraph::new(200, "shunt".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_ic(1, 4, &[11, 12, 13, 14]));
        g.boxes.push(mk_two_pin(2, "CAP_1", &[21, 22]));
        // Power rail (West): anchor pins 11,12 ↔ cap pin 21.
        g.nets.push(mk_net(
            301,
            "PWR",
            NetKind::Power,
            &[(1, 11), (1, 12), (2, 21)],
        ));
        // Ground (South): anchor pins 13,14 ↔ cap pin 22.
        g.nets.push(mk_net(
            302,
            "GND",
            NetKind::Ground,
            &[(1, 13), (1, 14), (2, 22)],
        ));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        // The IC (box 1) must be the layer anchor of both nets.
        assert!(
            topos.iter().all(|t| t.anchor == 1),
            "test IC should anchor both nets"
        );
        let cap = g.boxes.iter().find(|b| b.id == 2).expect("cap box");
        assert!(
            cap.h > cap.w,
            "Shunt cap to a South ground must hang vertically, got w={} h={}",
            cap.w,
            cap.h
        );
        // And it must sit below the power trunk (hanging down).
        let pwr = topos
            .iter()
            .find(|t| t.net_name == "PWR")
            .expect("PWR topo");
        let pwr_axis = pwr.lane.axis;
        assert!(
            cap.y >= pwr_axis,
            "Shunt cap should hang downward from the power trunk, cap.y={} axis={}",
            cap.y,
            pwr_axis
        );
    }
}
