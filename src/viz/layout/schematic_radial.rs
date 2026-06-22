// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW (P07, S6) — Center-outward schematic Layouter
//!
//! ## What problem does this file solve
//! Top-level layout previously used `HierarchicalLayouter` (Sugiyama layering), producing "power on top,
//! ground on bottom, signals flowing horizontally" in **block diagram style**, not the schematic convention of "main chip in center, peripherals radiating outward".
//! `RadialLayouter` is also radial, but only 2 rings + O(N²) angle mutual-push, N > 8 gets messy.
//!
//! `SchematicRadialLayouter` is the new default top-level algorithm:
//! - **Center anchoring**: automatically find the most hub-like IC as anchor, place at canvas center
//! - **Bucketing**: other boxes bucketed by "electrical relationship with anchor"
//!   - power/ground rails: top / bottom
//!   - bypass capacitors (spanning power-ground): stick close to anchor's power pin
//!   - direct neighbors: place first ring by quadrant inferred from IoDirection
//!   - second-degree neighbors: BFS along parent extension line for second ring
//!   - isolated: fill corners
//! - **Scalable to 30+ nodes**: not hardcoded 2 rings, radius adapts to box count per quadrant
//!
//! ## Algorithm Overview — Anchor + Bucket + Place
//!
//! ```text
//! ┌────────────────────────────────────┐
//! │ 1. ANCHOR                          │
//! │    pick_anchor: degree + Symbol +  │
//! │       name + pin_count composite score │
//! └─────────────┬──────────────────────┘
//!               ▼
//! ┌────────────────────────────────────┐
//! │ 2. BUCKET                          │
//! │    bucket_boxes:                    │
//! │      anchor / power_rails /          │
//! │      ground_rails / bypass_caps /    │
//! │      direct_neighbors / isolated     │
//! └─────────────┬──────────────────────┘
//!               ▼
//! ┌────────────────────────────────────┐
//! │ 3. PLACE                           │
//! │    a. anchor → canvas center            │
//! │    b. power rails top row             │
//! │    c. ground rails bottom row            │
//! │    d. bypass_caps stick to anchor's         │
//! │       VCC-GND pin pair                  │
//! │    e. direct_neighbors by quadrant distribution      │
//! │    f. outer rings: BFS along extension line        │
//! │    g. isolated fill corners                │
//! └─────────────┬──────────────────────┘
//!               ▼
//! ┌────────────────────────────────────┐
//! │ 4. POST                            │
//! │    a. resolve_overlaps_iterative    │
//! │    b. P06 entry_points_refine       │
//! │    c. recompute_sizes_with_pin_count│
//! │    d. resolve_overlaps + normalize  │
//! └────────────────────────────────────┘
//! ```
//!
//! ## Cooperation with P06
//! Full call sequence reuses P06's two-round entry_points scheduling. After layout computes coordinates, call
//! `assign_entry_points_refine` to rearrange pins towards neighbors, then (optional) recompute size.
//!
//! ## Cooperation with P10
//! This file **doesn't know** about ChannelMap. Layout output (box coordinates) is input for P10 channel
//! extraction, P10 scheduler builds ChannelMap itself after layout completes.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::vector::graph::{naming, BoxKind, McVecGraph, NetKind, Symbol};

use super::components::build_adjacency;
use super::entry_points::{assign_entry_points_coarse, assign_entry_points_refine};
use super::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use super::overlap::resolve_overlaps_iterative;
use super::size::{assign_default_sizes, recompute_sizes_with_pin_count};
use crate::viz::traits::Layouter;

// ============================================================================
// Configuration parameters
// ============================================================================

/// Top-level layouter: center IC + surrounding devices radiating (schematic convention)
pub struct SchematicRadialLayouter {
    /// Extra margin from canvas to outermost box
    pub canvas_margin: f64,
    /// Radius of direct neighbor first ring
    pub anchor_radius: f64,
    /// Radius increment of second-degree neighbor ring (second ring ≈ anchor_radius + outer_radius_increment)
    pub outer_radius_increment: f64,
    /// Offset distance from bypass capacitor to anchor (outward extension)
    pub bypass_offset: f64,
    /// Minimum angle between boxes within same quadrant (rad)
    pub min_quadrant_angle: f64,
}

impl Default for SchematicRadialLayouter {
    fn default() -> Self {
        Self {
            canvas_margin: 80.0,
            anchor_radius: 240.0,
            outer_radius_increment: 200.0,
            bypass_offset: 100.0,
            min_quadrant_angle: 0.3, // ~17°
        }
    }
}

impl Layouter for SchematicRadialLayouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        // ── Phase 0: coarse entry_points + size ──
        assign_default_sizes(graph);
        assign_entry_points_coarse(graph);

        if graph.boxes.len() == 1 {
            graph.boxes[0].x = CANVAS_MARGIN;
            graph.boxes[0].y = CANVAS_MARGIN;
            return compute_canvas(graph);
        }

        // ── Phase 1: ANCHOR ──
        let anchor_id = pick_anchor(graph);
        // Center point temporarily uses (1000, 800) to give coordinates a starting point, finally normalize_positions pulls to origin
        let center = (1000.0_f64, 800.0_f64);
        place_anchor_at_center(graph, anchor_id, center);

        // ── Phase 2: BUCKET ──
        let buckets = bucket_boxes(graph, anchor_id);
        eprintln!(
            "[layout::schematic_radial] anchor={} buckets: power={} ground={} bypass={} direct={} isolated={}",
            anchor_id,
            buckets.power_rails.len(),
            buckets.ground_rails.len(),
            buckets.bypass_caps.len(),
            buckets.direct_neighbors.len(),
            buckets.isolated.len(),
        );

        // ── Phase 3: PLACE (by bucket) ──
        place_rails(graph, &buckets.power_rails, center, true);
        place_rails(graph, &buckets.ground_rails, center, false);
        place_bypass_caps(graph, anchor_id, &buckets.bypass_caps, self.bypass_offset);
        place_quadrants(
            graph,
            anchor_id,
            &buckets.direct_neighbors,
            self.anchor_radius,
        );

        // ── Phase 3.b: OUTER RINGS (second-degree and beyond neighbors) ──
        let placed_so_far: HashSet<i64> = std::iter::once(anchor_id)
            .chain(buckets.bypass_caps.iter().copied())
            .chain(buckets.direct_neighbors.iter().copied())
            .chain(buckets.power_rails.iter().copied())
            .chain(buckets.ground_rails.iter().copied())
            .collect();
        let remaining: Vec<i64> = graph
            .boxes
            .iter()
            .map(|b| b.id)
            .filter(|id| !placed_so_far.contains(id) && !buckets.isolated.contains(id))
            .collect();
        place_outer_rings(
            graph,
            anchor_id,
            &placed_so_far,
            &remaining,
            self.outer_radius_increment,
        );

        // ── Phase 3.c: ISOLATED corner fill ──
        place_isolated(graph, &buckets.isolated, center);

        // ── Phase 4: POST (overlap + refine + size) ──
        resolve_overlaps_iterative(graph, 30);

        // P06 round 2: refine pin sides
        assign_entry_points_refine(graph);
        let resized = recompute_sizes_with_pin_count(graph);
        if resized {
            resolve_overlaps_iterative(graph, 5);
        }

        normalize_positions(graph);
        compute_canvas(graph)
    }

    fn name(&self) -> &'static str {
        "schematic_radial"
    }
}

// ============================================================================
// 1. ANCHOR — pick center box
// ============================================================================

/// Pick anchor box (center IC)
///
/// Score = degree × 15 + symbol_bonus + name_bonus + pin_count × 2
///
/// **symbol_bonus**: Ic = 200, Module = 180, MultiPin = 100, TwoPin = 0, PowerRail = -∞ (excluded)
/// **name_bonus**: `naming::is_main_chip` (contains mcu/cpu/soc/fpga) → +300
pub fn pick_anchor(graph: &McVecGraph) -> i64 {
    if graph.boxes.is_empty() {
        return 0;
    }
    let adj = build_adjacency(graph);

    let mut best_id = graph.boxes[0].id;
    let mut best_score: i64 = i64::MIN;

    for b in &graph.boxes {
        // PowerRail never acts as anchor
        if matches!(b.symbol, Symbol::PowerRail { .. }) {
            continue;
        }
        let deg = adj.get(&b.id).map(|v| v.len()).unwrap_or(0) as i64;
        let symbol_bonus = match b.symbol {
            Symbol::Ic => 200,
            Symbol::Module => 180,
            // BoxKind fallback
            _ => match b.kind {
                BoxKind::SubModule => 180,
                BoxKind::MultiPin => 100,
                BoxKind::TwoPin => 0,
                BoxKind::PowerLabel => -1000,
            },
        };
        let name_bonus = if naming::is_main_chip(&b.name) {
            300
        } else {
            0
        };
        let score = deg * 15 + symbol_bonus + name_bonus + b.pin_count as i64 * 2;
        if score > best_score {
            best_score = score;
            best_id = b.id;
        }
    }
    if crate::viz::debug::dump_enabled() {
        eprintln!("[layout::schematic_radial] anchor={best_id} score={best_score}");
    }
    best_id
}

fn place_anchor_at_center(graph: &mut McVecGraph, anchor_id: i64, center: (f64, f64)) {
    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == anchor_id) {
        b.x = center.0 - b.w / 2.0;
        b.y = center.1 - b.h / 2.0;
    }
}

// ============================================================================
// 2. BUCKET — bucket other boxes
// ============================================================================

/// Bucket result
#[derive(Debug, Default)]
pub struct BoxBuckets {
    /// Center anchor
    pub anchor: i64,
    /// Power rail/label (PowerRail, non-ground)
    pub power_rails: Vec<i64>,
    /// Ground rail/label (PowerRail, ground)
    pub ground_rails: Vec<i64>,
    /// Bypass capacitor (2-terminal TwoPin, one end connects Power net, other end connects Ground net)
    pub bypass_caps: Vec<i64>,
    /// Direct neighbor (shares at least one non-rail net with anchor)
    pub direct_neighbors: Vec<i64>,
    /// Fully isolated (no net connection)
    pub isolated: Vec<i64>,
}

impl BoxBuckets {
    /// Whether this id is in power or ground rail bucket
    pub fn is_rail(&self, id: i64) -> bool {
        self.power_rails.contains(&id) || self.ground_rails.contains(&id)
    }
}

/// Bucket all non-anchor boxes into one of 6 buckets
pub fn bucket_boxes(graph: &McVecGraph, anchor_id: i64) -> BoxBuckets {
    let mut buckets = BoxBuckets {
        anchor: anchor_id,
        ..Default::default()
    };

    // 1. Collect (box_id, connected_net_kinds)
    let connected_kinds = collect_connected_net_kinds(graph);
    let adj = build_adjacency(graph);
    let anchor_neighbors: HashSet<i64> = adj
        .get(&anchor_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();

    for b in &graph.boxes {
        if b.id == anchor_id {
            continue;
        }

        // Power / Ground rail (Symbol-based priority)
        if let Symbol::PowerRail { is_ground } = b.symbol {
            if is_ground {
                buckets.ground_rails.push(b.id);
            } else {
                buckets.power_rails.push(b.id);
            }
            continue;
        }
        // Fallback: PowerLabel uses name
        if matches!(b.kind, BoxKind::PowerLabel) {
            let u = b.name.to_uppercase();
            if u.contains("GND") || u.contains("VSS") {
                buckets.ground_rails.push(b.id);
            } else {
                buckets.power_rails.push(b.id);
            }
            continue;
        }

        // Bypass capacitor: 2-terminal + one Power + one Ground
        if matches!(b.kind, BoxKind::TwoPin) && b.pin_count == 2 {
            let nk = connected_kinds.get(&b.id).cloned().unwrap_or_default();
            let has_power = nk.iter().any(|k| matches!(k, NetKind::Power));
            let has_ground = nk.iter().any(|k| matches!(k, NetKind::Ground));
            if has_power && has_ground {
                buckets.bypass_caps.push(b.id);
                continue;
            }
        }

        // Direct neighbor
        if anchor_neighbors.contains(&b.id) {
            buckets.direct_neighbors.push(b.id);
            continue;
        }

        // Fully isolated (no neighbors)
        let degree = adj.get(&b.id).map(|v| v.len()).unwrap_or(0);
        if degree == 0 {
            buckets.isolated.push(b.id);
            continue;
        }

        // Second-degree and beyond neighbors (left for place_outer_rings, doesn't enter any bucket)
    }

    buckets
}

/// box_id → NetKind set of nets that box connects to
fn collect_connected_net_kinds(graph: &McVecGraph) -> HashMap<i64, Vec<NetKind>> {
    let mut out: HashMap<i64, Vec<NetKind>> = HashMap::new();
    for net in &graph.nets {
        for ep in &net.endpoints {
            out.entry(ep.box_id).or_default().push(net.kind.clone());
        }
    }
    out
}

// ============================================================================
// 3. PLACE — power / ground rails top / bottom row
// ============================================================================

fn place_rails(graph: &mut McVecGraph, rail_ids: &[i64], center: (f64, f64), top: bool) {
    if rail_ids.is_empty() {
        return;
    }
    let (cx, cy) = center;
    let y_offset = if top { -380.0 } else { 380.0 };
    let y_target = cy + y_offset;
    let spacing = 80.0;
    let total_w = (rail_ids.len() as f64) * spacing;
    let start_x = cx - total_w / 2.0 + spacing / 2.0;

    for (i, &id) in rail_ids.iter().enumerate() {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
            b.x = start_x + i as f64 * spacing - b.w / 2.0;
            b.y = y_target - b.h / 2.0;
        }
    }
}

// ============================================================================
// 3. PLACE — bypass capacitors stick to anchor power pin
// ============================================================================

/// Place bypass capacitors on the outside of anchor's VCC-GND pin pair
fn place_bypass_caps(
    graph: &mut McVecGraph,
    anchor_id: i64,
    bypass_ids: &[i64],
    bypass_offset: f64,
) {
    if bypass_ids.is_empty() {
        return;
    }
    // Find anchor (immutable borrow scoped)
    let (vcc_pairs, gnd_pairs, anchor_x, anchor_y, anchor_w, anchor_h);
    {
        let anchor = match graph.boxes.iter().find(|b| b.id == anchor_id) {
            Some(b) => b,
            None => return,
        };
        anchor_x = anchor.x;
        anchor_y = anchor.y;
        anchor_w = anchor.w;
        anchor_h = anchor.h;
        vcc_pairs = anchor
            .entry_points
            .iter()
            .filter(|e| naming::is_power(&e.pin_name))
            .map(|e| (e.side.clone(), e.offset, e.pin_id))
            .collect::<Vec<_>>();
        gnd_pairs = anchor
            .entry_points
            .iter()
            .filter(|e| naming::is_ground(&e.pin_name))
            .map(|e| (e.side.clone(), e.offset, e.pin_id))
            .collect::<Vec<_>>();
    }

    if vcc_pairs.is_empty() || gnd_pairs.is_empty() {
        // No power/ground pin → treat cap as normal direct neighbor, spread (fallback)
        for (i, &cap_id) in bypass_ids.iter().enumerate() {
            let angle = std::f64::consts::PI * 0.5 + (i as f64) * 0.3;
            let r = 280.0;
            let cx = anchor_x + anchor_w / 2.0 + r * angle.cos();
            let cy = anchor_y + anchor_h / 2.0 + r * angle.sin();
            set_box_center(graph, cap_id, cx, cy);
        }
        return;
    }

    // pair power-ground (pair by offset, simple greedy)
    let pairs = pair_power_ground_pins(&vcc_pairs, &gnd_pairs);

    let (acx, acy) = (anchor_x + anchor_w / 2.0, anchor_y + anchor_h / 2.0);

    for (slot_idx, &cap_id) in bypass_ids.iter().enumerate() {
        let (vcc_side, vcc_offset, gnd_side, gnd_offset) = pairs[slot_idx % pairs.len()].clone();

        // pin absolute position (based on anchor geometry)
        let (vcc_px, vcc_py) = pin_abs_pos(
            anchor_x, anchor_y, anchor_w, anchor_h, &vcc_side, vcc_offset,
        );
        let (gnd_px, gnd_py) = pin_abs_pos(
            anchor_x, anchor_y, anchor_w, anchor_h, &gnd_side, gnd_offset,
        );

        // cap position: midpoint of VCC pin and GND pin, offset bypass_offset outward from anchor
        let mid_x = (vcc_px + gnd_px) / 2.0;
        let mid_y = (vcc_py + gnd_py) / 2.0;
        let (ox, oy) = outward_unit(acx, acy, mid_x, mid_y);
        // Multiple caps in same slot: extend slot * 50px along outward direction
        let extra = (slot_idx / pairs.len()) as f64 * 50.0;
        let cx = mid_x + ox * (bypass_offset + extra);
        let cy = mid_y + oy * (bypass_offset + extra);

        set_box_center(graph, cap_id, cx, cy);
    }
}

/// Pair power pins and ground pins by offset proximity
///
/// Simple greedy: sort + pair by same index; if count mismatched, leftover not paired.
fn pair_power_ground_pins(
    vcc: &[(crate::vector::graph::EntrySide, f64, i64)],
    gnd: &[(crate::vector::graph::EntrySide, f64, i64)],
) -> Vec<(
    crate::vector::graph::EntrySide,
    f64,
    crate::vector::graph::EntrySide,
    f64,
)> {
    let n = vcc.len().min(gnd.len());
    let mut v = vcc.to_vec();
    let mut g = gnd.to_vec();
    v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    g.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    (0..n)
        .map(|i| (v[i].0.clone(), v[i].1, g[i].0.clone(), g[i].1))
        .collect()
}

fn pin_abs_pos(
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
    side: &crate::vector::graph::EntrySide,
    offset: f64,
) -> (f64, f64) {
    use crate::vector::graph::EntrySide;
    match side {
        EntrySide::Top => (bx + bw * offset, by),
        EntrySide::Bottom => (bx + bw * offset, by + bh),
        EntrySide::Left => (bx, by + bh * offset),
        EntrySide::Right => (bx + bw, by + bh * offset),
    }
}

fn outward_unit(cx: f64, cy: f64, px: f64, py: f64) -> (f64, f64) {
    let dx = px - cx;
    let dy = py - cy;
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    (dx / len, dy / len)
}

fn set_box_center(graph: &mut McVecGraph, id: i64, cx: f64, cy: f64) {
    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
        b.x = cx - b.w / 2.0;
        b.y = cy - b.h / 2.0;
    }
}

// ============================================================================
// 3. PLACE — direct neighbors four-quadrant distribution
// ============================================================================

#[derive(Hash, Eq, PartialEq, Copy, Clone, Debug)]
enum Quadrant {
    Right,
    Bottom,
    Left,
    Top,
}

impl Quadrant {
    fn center_angle(self) -> f64 {
        use std::f64::consts::*;
        match self {
            Quadrant::Right => 0.0,
            Quadrant::Bottom => FRAC_PI_2,
            Quadrant::Left => PI,
            Quadrant::Top => 3.0 * FRAC_PI_2,
        }
    }
}

fn place_quadrants(graph: &mut McVecGraph, anchor_id: i64, neighbors: &[i64], radius: f64) {
    if neighbors.is_empty() {
        return;
    }
    let (acx, acy) = {
        let a = match graph.boxes.iter().find(|b| b.id == anchor_id) {
            Some(b) => b,
            None => return,
        };
        (a.x + a.w / 2.0, a.y + a.h / 2.0)
    };

    // Determine each neighbor's target quadrant
    let mut buckets: HashMap<Quadrant, Vec<i64>> = HashMap::new();
    for &n in neighbors {
        let q = pick_quadrant_for(graph, anchor_id, n);
        buckets.entry(q).or_default().push(n);
    }

    // Within each quadrant sort by current position (degree / id) then spread evenly
    for (quad, ids) in &mut buckets {
        let n = ids.len();
        let center_angle = quad.center_angle();
        let arc_half = std::f64::consts::FRAC_PI_3; // within ±60°, leave 30° for quadrant boundary
                                                    // Adaptive radius: more boxes in quadrant → outward
        let r = radius * (1.0 + (n as f64 - 1.0) * 0.05);

        // Stable sort (by id)
        ids.sort();

        for (i, &id) in ids.iter().enumerate() {
            let frac = if n == 1 {
                0.5
            } else {
                i as f64 / (n - 1) as f64
            };
            let angle = center_angle - arc_half + frac * 2.0 * arc_half;
            let x = acx + r * angle.cos();
            let y = acy + r * angle.sin();
            set_box_center(graph, id, x, y);
        }
    }
}

/// Pick target quadrant for neighbor
///
/// Basis: which side is the anchor's pin at the anchor ↔ neighbor shared net's anchor end
fn pick_quadrant_for(graph: &McVecGraph, anchor_id: i64, neighbor_id: i64) -> Quadrant {
    use crate::vector::graph::EntrySide;
    let anchor = match graph.boxes.iter().find(|b| b.id == anchor_id) {
        Some(b) => b,
        None => return Quadrant::Right,
    };

    for net in &graph.nets {
        let anchor_ep = net.endpoints.iter().find(|e| e.box_id == anchor_id);
        let has_neighbor = net.endpoints.iter().any(|e| e.box_id == neighbor_id);
        if let (Some(aep), true) = (anchor_ep, has_neighbor) {
            if let Some(side) = anchor
                .entry_points
                .iter()
                .find(|e| e.pin_id == aep.pin_id)
                .map(|e| e.side.clone())
            {
                return match side {
                    EntrySide::Right => Quadrant::Right,
                    EntrySide::Bottom => Quadrant::Bottom,
                    EntrySide::Left => Quadrant::Left,
                    EntrySide::Top => Quadrant::Top,
                };
            }
        }
    }
    Quadrant::Right
}

// ============================================================================
// 3. PLACE — outer rings (BFS along parent extension line)
// ============================================================================

fn place_outer_rings(
    graph: &mut McVecGraph,
    anchor_id: i64,
    placed_so_far: &HashSet<i64>,
    remaining: &[i64],
    radius_increment: f64,
) {
    if remaining.is_empty() {
        return;
    }
    let (acx, acy) = {
        let a = match graph.boxes.iter().find(|b| b.id == anchor_id) {
            Some(b) => b,
            None => return,
        };
        (a.x + a.w / 2.0, a.y + a.h / 2.0)
    };

    let adj = build_adjacency(graph);
    let remaining_set: HashSet<i64> = remaining.iter().copied().collect();
    let mut placed = placed_so_far.clone();
    let mut queue: VecDeque<i64> = placed_so_far.iter().copied().collect();

    let mut iters = 0_u32;
    while let Some(parent_id) = queue.pop_front() {
        iters += 1;
        if iters > 1000 {
            // safety
            break;
        }

        // Find all unvisited neighbors of parent (in remaining)
        let new_nbrs: Vec<i64> = adj
            .get(&parent_id)
            .map(|v| {
                v.iter()
                    .filter(|n| !placed.contains(n) && remaining_set.contains(n))
                    .copied()
                    .collect()
            })
            .unwrap_or_default();

        if new_nbrs.is_empty() {
            continue;
        }

        let (px, py) = {
            let p = graph.boxes.iter().find(|b| b.id == parent_id);
            match p {
                Some(b) => (b.x + b.w / 2.0, b.y + b.h / 2.0),
                None => continue,
            }
        };
        let (ox, oy) = outward_unit(acx, acy, px, py);
        let perp = (-oy, ox);

        for (i, &n) in new_nbrs.iter().enumerate() {
            // At radius_increment outside parent, fan left-right
            let m = new_nbrs.len() as f64;
            let spread = if m == 1.0 {
                0.0
            } else {
                (i as f64 - (m - 1.0) / 2.0) * 0.4
            };
            let cx = px + ox * radius_increment + perp.0 * radius_increment * spread * 0.3;
            let cy = py + oy * radius_increment + perp.1 * radius_increment * spread * 0.3;
            set_box_center(graph, n, cx, cy);
            placed.insert(n);
            queue.push_back(n);
        }
    }

    // Remaining unplaced (island sub-graphs etc): fallback to upper-right
    let still_unplaced: Vec<i64> = remaining
        .iter()
        .copied()
        .filter(|id| !placed.contains(id))
        .collect();
    if !still_unplaced.is_empty() {
        eprintln!(
            "[layout::schematic_radial] {} disconnected boxes pushed to corner",
            still_unplaced.len()
        );
        let corner_x = acx + 700.0;
        let corner_y = acy - 500.0;
        for (i, id) in still_unplaced.iter().enumerate() {
            set_box_center(
                graph,
                *id,
                corner_x + (i % 4) as f64 * 130.0,
                corner_y + (i / 4) as f64 * 90.0,
            );
        }
    }
}

// ============================================================================
// 3. PLACE — isolated boxes placed at outermost layer
// ============================================================================

fn place_isolated(graph: &mut McVecGraph, isolated: &[i64], center: (f64, f64)) {
    if isolated.is_empty() {
        return;
    }
    // Spread in a row along anchor's lower-right corner
    let (cx, _cy) = center;
    let y = center.1 + 600.0;
    let start_x = cx - (isolated.len() as f64) * 70.0;
    for (i, &id) in isolated.iter().enumerate() {
        set_box_center(graph, id, start_x + i as f64 * 140.0, y);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::net_def::IoDirection;
    use crate::vector::graph::{EndpointRef, EntryPoint, EntrySide, IoSummary, NetKind, VizNet};

    fn mk_box(id: i64, name: &str, kind: BoxKind, sym: Symbol, pins: usize) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            sym,
            None,
            None,
            pins,
            IoSummary::new(),
        );
        b.w = 100.0;
        b.h = 60.0;
        b
    }

    fn mk_net(nid: i64, name: &str, kind: NetKind, eps: Vec<(i64, i64)>) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            kind,
            eps.into_iter()
                .map(|(bx, pn)| {
                    EndpointRef::with_io(bx, pn, format!("p{}", pn), IoDirection::Unknown)
                })
                .collect(),
        )
    }

    // ────────────────────────────────────────────────────────────────────────
    // pick_anchor
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p07_pick_anchor_prefers_ic_with_main_chip_name() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes
            .push(mk_box(1, "MCU_U1", BoxKind::MultiPin, Symbol::Ic, 32));
        g.boxes
            .push(mk_box(2, "R1", BoxKind::TwoPin, Symbol::Unknown, 2));
        g.boxes
            .push(mk_box(3, "C1", BoxKind::TwoPin, Symbol::Unknown, 2));
        // Connect all so degree isn't 0
        g.nets
            .push(mk_net(10, "sig1", NetKind::Signal, vec![(1, 1), (2, 1)]));
        g.nets
            .push(mk_net(11, "sig2", NetKind::Signal, vec![(1, 2), (3, 1)]));

        let anchor = pick_anchor(&g);
        assert_eq!(anchor, 1, "MCU IC should win the anchor selection");
    }

    #[test]
    fn p07_pick_anchor_excludes_power_rails() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(
            1,
            "VCC",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: false },
            1,
        ));
        g.boxes
            .push(mk_box(2, "U1", BoxKind::MultiPin, Symbol::Ic, 8));
        g.nets
            .push(mk_net(10, "n", NetKind::Power, vec![(1, 1), (2, 1)]));
        let anchor = pick_anchor(&g);
        assert_eq!(anchor, 2, "PowerRail must never be anchor");
    }

    // ────────────────────────────────────────────────────────────────────────
    // bucket_boxes
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p07_bucket_classifies_correctly() {
        let mut g = McVecGraph::new(0, "test".into());
        // anchor = MCU
        g.boxes
            .push(mk_box(1, "MCU", BoxKind::MultiPin, Symbol::Ic, 8));
        // direct neighbor = sensor (signal)
        g.boxes
            .push(mk_box(2, "Sensor", BoxKind::SubModule, Symbol::Module, 4));
        // bypass cap (spanning Power + Ground)
        g.boxes
            .push(mk_box(3, "C1", BoxKind::TwoPin, Symbol::Unknown, 2));
        // VCC label
        g.boxes.push(mk_box(
            4,
            "VCC",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: false },
            1,
        ));
        // GND label
        g.boxes.push(mk_box(
            5,
            "GND",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: true },
            1,
        ));
        // isolated (no nets)
        g.boxes
            .push(mk_box(6, "TP1", BoxKind::TwoPin, Symbol::Unknown, 2));

        // nets
        g.nets
            .push(mk_net(10, "sig", NetKind::Signal, vec![(1, 1), (2, 1)]));
        g.nets.push(mk_net(
            11,
            "VCC",
            NetKind::Power,
            vec![(4, 1), (1, 2), (3, 1)],
        ));
        g.nets.push(mk_net(
            12,
            "GND",
            NetKind::Ground,
            vec![(5, 1), (1, 3), (3, 2)],
        ));

        let buckets = bucket_boxes(&g, 1);

        assert_eq!(buckets.anchor, 1);
        assert!(buckets.power_rails.contains(&4));
        assert!(buckets.ground_rails.contains(&5));
        assert!(
            buckets.bypass_caps.contains(&3),
            "C1 should be bypass (spans power+ground)"
        );
        assert!(
            buckets.direct_neighbors.contains(&2),
            "Sensor should be direct neighbor of MCU"
        );
        assert!(buckets.isolated.contains(&6), "TP1 has no nets");
    }

    #[test]
    fn p07_bucket_non_bypass_cap_goes_to_direct() {
        // 2-terminal cap only connects signal (not power+ground bypass) → should go to direct neighbor
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes
            .push(mk_box(1, "MCU", BoxKind::MultiPin, Symbol::Ic, 8));
        g.boxes
            .push(mk_box(2, "C_AC", BoxKind::TwoPin, Symbol::Unknown, 2));
        g.nets.push(mk_net(
            10,
            "ac_couple",
            NetKind::Signal,
            vec![(1, 1), (2, 1)],
        ));

        let buckets = bucket_boxes(&g, 1);
        assert!(buckets.direct_neighbors.contains(&2));
        assert!(!buckets.bypass_caps.contains(&2));
    }

    // ────────────────────────────────────────────────────────────────────────
    // pair_power_ground_pins
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p07_pair_power_ground_simple() {
        let vcc = vec![(EntrySide::Top, 0.3, 1_i64), (EntrySide::Top, 0.7, 2_i64)];
        let gnd = vec![
            (EntrySide::Bottom, 0.3, 10_i64),
            (EntrySide::Bottom, 0.7, 11_i64),
            (EntrySide::Bottom, 0.9, 12_i64),
        ];
        let pairs = pair_power_ground_pins(&vcc, &gnd);
        assert_eq!(pairs.len(), 2, "min(2 vcc, 3 gnd) = 2 pairs");
    }

    // ────────────────────────────────────────────────────────────────────────
    // pick_quadrant_for
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p07_pick_quadrant_uses_anchor_pin_side() {
        let mut g = McVecGraph::new(0, "test".into());
        let mut anchor = mk_box(1, "MCU", BoxKind::MultiPin, Symbol::Ic, 4);
        // Give anchor a pin on Right side
        anchor.entry_points.push(EntryPoint {
            pin_id: 100,
            pin_name: "OUT".into(),
            side: EntrySide::Right,
            offset: 0.5,
        });
        g.boxes.push(anchor);
        g.boxes
            .push(mk_box(2, "nbr", BoxKind::SubModule, Symbol::Module, 1));
        g.nets
            .push(mk_net(10, "n", NetKind::Signal, vec![(1, 100), (2, 1)]));

        let q = pick_quadrant_for(&g, 1, 2);
        assert_eq!(q, Quadrant::Right);
    }

    // ────────────────────────────────────────────────────────────────────────
    // End-to-end (no panic + roughly meets expectations)
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p07_end_to_end_small_circuit() {
        let mut g = McVecGraph::new(0, "test".into());
        // 1 IC + 4 resistors + 1 VCC + 1 GND
        g.boxes
            .push(mk_box(1, "MCU_U1", BoxKind::MultiPin, Symbol::Ic, 8));
        for i in 2..=5 {
            g.boxes.push(mk_box(
                i,
                &format!("R{}", i - 1),
                BoxKind::TwoPin,
                Symbol::Unknown,
                2,
            ));
        }
        g.boxes.push(mk_box(
            6,
            "VCC",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: false },
            1,
        ));
        g.boxes.push(mk_box(
            7,
            "GND",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: true },
            1,
        ));

        for r in 2..=5 {
            g.nets.push(mk_net(
                100 + r,
                &format!("sig{}", r),
                NetKind::Signal,
                vec![(1, r), (r, 1)],
            ));
        }
        g.nets
            .push(mk_net(200, "vcc", NetKind::Power, vec![(6, 1), (1, 100)]));
        g.nets
            .push(mk_net(201, "gnd", NetKind::Ground, vec![(7, 1), (1, 101)]));

        let layouter = SchematicRadialLayouter::default();
        let (cw, ch) = layouter.layout(&mut g);

        // No panic + canvas reasonable
        assert!(cw > 0.0 && ch > 0.0);

        // All boxes are placed (x/y not all 0)
        let placed_count = g.boxes.iter().filter(|b| b.x != 0.0 || b.y != 0.0).count();
        assert!(
            placed_count >= 6,
            "at least 6 boxes should be placed; got {}",
            placed_count
        );
    }
}
