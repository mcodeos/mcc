// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Milestone 6 — Layered Placement Prototype：语义驱动分层布局原型
//!
//! **Status: experimental** — kept for comparison via `RenderOpts::layered()`.
//!
//! A read-only semantic analysis + BFS-based rank/lane assignment that maps
//! logical placement to geometric coordinates. This is an experimental path
//! (not the default) that consumes `SemanticModel` to produce more controlled
//! x/y placement than the existing FlowLayouter.
//!
//! ## Pipeline
//! ```text
//! Phase 0  Semantic snapshot
//! Phase 1  Prepare graph
//! Phase 2  Build logical placement
//! Phase 3  Order lanes within ranks
//! Phase 4  Solve geometry
//! Phase 5  Pin placement / post placement
//! Phase 6  Normalize / canvas
//! ```
//!
//! ## Usage
//! ```ignore
//! let opts = RenderOpts::layered();
//! let (doc, metrics) = render_with_metrics(graph, opts);
//! ```

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::vector::graph::netdef::IoDirection;
use crate::vector::graph::McVecGraph;
use crate::viz::layout::components::build_adjacency;
use crate::viz::layout::entry_points::{
    assign_entry_points_coarse, enforce_unique_offsets, promote_synthetic_pins, split_shared_pins,
};
use crate::viz::layout::flow::eject_flags_from_boxes;
use crate::viz::layout::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use crate::viz::layout::rails::{explode_power_rails_to_flags, is_rail_box};
use crate::viz::layout::size::assign_default_sizes;
use crate::viz::pins::{pin_anchor_pipeline, PinAnchorConfig};
use crate::viz::semantic::SemanticModel;
use crate::viz::traits::Layouter;

// ============================================================================
// PlacementWarning
// ============================================================================

/// A warning produced during placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementWarning {
    pub message: String,
}

// ============================================================================
// PreferredRegion
// ============================================================================

/// Preferred region for a box in the logical layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferredRegion {
    Left,
    Center,
    Right,
    Top,
    Bottom,
    NearBox(i64),
    Peripheral,
    Unknown,
}

// ============================================================================
// PlacementReason
// ============================================================================

/// Reason for a box's placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementReason {
    Hub,
    InputSide,
    OutputSide,
    PowerRail,
    GroundRail,
    SignalChain,
    PassiveChain,
    ComponentGroup,
    ConnectivityBfs,
    Isolated,
    Fallback,
}

// ============================================================================
// LogicalBoxPlacement
// ============================================================================

/// Logical placement of a single box (rank/lane/grid).
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalBoxPlacement {
    pub box_id: i64,
    pub rank: i32,
    pub lane: i32,
    pub group_id: Option<usize>,
    pub align_group: Option<usize>,
    pub preferred_region: PreferredRegion,
    pub fixed: bool,
    pub reason: PlacementReason,
}

// ============================================================================
// LogicalPlacement
// ============================================================================

/// Full logical placement for a graph.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LogicalPlacement {
    pub boxes: BTreeMap<i64, LogicalBoxPlacement>,
    pub columns: BTreeMap<i32, Vec<i64>>,
    pub root_box_id: Option<i64>,
    pub warnings: Vec<PlacementWarning>,
}

impl LogicalPlacement {
    /// Build a logical placement from a graph and its semantic model.
    pub fn from_graph_and_semantics(
        graph: &McVecGraph,
        semantic: &SemanticModel,
        config: &LayeredLayouter,
    ) -> Self {
        let mut warnings: Vec<PlacementWarning> = Vec::new();

        // ── Select root / hub ──
        let root_id = Self::select_root(graph, semantic, &mut warnings);

        // ── Build adjacency ──
        let adj = build_adjacency(graph);

        // ── Assign ranks via BFS from root ──
        let (_ranks, rank_map) = Self::assign_ranks(graph, &adj, root_id, semantic, &mut warnings);

        // ── Build columns (rank → box_ids) ──
        let columns: BTreeMap<i32, Vec<i64>> = {
            let mut cols: BTreeMap<i32, Vec<i64>> = BTreeMap::new();
            for (&id, &r) in &rank_map {
                cols.entry(r).or_default().push(id);
            }
            for v in cols.values_mut() {
                v.sort();
            }
            cols
        };

        // ── Assign lanes within each rank ──
        let lane_map = Self::assign_lanes(graph, &columns, &adj, &rank_map, config);

        // ── Build logical box placements ──
        let mut boxes: BTreeMap<i64, LogicalBoxPlacement> = BTreeMap::new();
        for (&id, &rank) in &rank_map {
            let lane = lane_map.get(&id).copied().unwrap_or(0);
            let reason = if id == root_id {
                PlacementReason::Hub
            } else if rank < 0 {
                PlacementReason::InputSide
            } else if rank > 0 {
                PlacementReason::OutputSide
            } else {
                PlacementReason::ConnectivityBfs
            };
            let region = if id == root_id {
                PreferredRegion::Center
            } else if rank < 0 {
                PreferredRegion::Left
            } else if rank > 0 {
                PreferredRegion::Right
            } else {
                PreferredRegion::Unknown
            };

            boxes.insert(
                id,
                LogicalBoxPlacement {
                    box_id: id,
                    rank,
                    lane,
                    group_id: None,
                    align_group: None,
                    preferred_region: region,
                    fixed: false,
                    reason,
                },
            );
        }

        LogicalPlacement {
            boxes,
            columns,
            root_box_id: Some(root_id),
            warnings,
        }
    }

    /// Select the root box: use semantic hub, then fall back to degree / first box.
    fn select_root(
        graph: &McVecGraph,
        semantic: &SemanticModel,
        _warnings: &mut Vec<PlacementWarning>,
    ) -> i64 {
        // 1. Highest hub_score from semantic
        if let Some((&id, _)) = semantic
            .boxes
            .iter()
            .filter(|(_, bs)| bs.is_hub_candidate)
            .max_by_key(|(_, bs)| bs.hub_score)
        {
            return id;
        }

        // 2. Signal chain hub
        if let Some(sc) = semantic.signal_chains.first() {
            return sc.hub_id;
        }

        // 3. Max degree non-flag box
        let adj = build_adjacency(graph);
        if let Some(b) = graph
            .boxes
            .iter()
            .filter(|b| !is_rail_box(b))
            .max_by_key(|b| adj.get(&b.id).map(|v| v.len()).unwrap_or(0))
        {
            if let Some(n) = adj.get(&b.id) {
                if !n.is_empty() {
                    return b.id;
                }
            }
        }

        // 4. First non-flag box
        graph
            .boxes
            .iter()
            .find(|b| !is_rail_box(b))
            .map(|b| b.id)
            .unwrap_or(graph.boxes[0].id)
    }

    /// Assign ranks via BFS from root. Negative = left, 0 = hub, positive = right.
    fn assign_ranks(
        graph: &McVecGraph,
        adj: &HashMap<i64, Vec<i64>>,
        root_id: i64,
        semantic: &SemanticModel,
        _warnings: &mut Vec<PlacementWarning>,
    ) -> (Vec<i32>, HashMap<i64, i32>) {
        let core_ids: Vec<i64> = graph.boxes.iter().map(|b| b.id).collect();
        let core_set: HashSet<i64> = core_ids.iter().copied().collect();

        // BFS from root: absolute distance
        let mut mag: HashMap<i64, i32> = HashMap::new();
        mag.insert(root_id, 0);
        let mut q: VecDeque<i64> = VecDeque::new();
        q.push_back(root_id);
        while let Some(u) = q.pop_front() {
            let mu = mag[&u];
            for &v in adj.get(&u).into_iter().flatten() {
                if core_set.contains(&v) && !mag.contains_key(&v) {
                    mag.insert(v, mu + 1);
                    q.push_back(v);
                }
            }
        }

        // Isolated components
        let mut visited: HashSet<i64> = mag.keys().copied().collect();
        for &start in &core_ids {
            if visited.contains(&start) {
                continue;
            }
            let mut comp: Vec<i64> = Vec::new();
            let mut cq: VecDeque<i64> = VecDeque::new();
            cq.push_back(start);
            visited.insert(start);
            while let Some(u) = cq.pop_front() {
                comp.push(u);
                for &v in adj.get(&u).into_iter().flatten() {
                    if core_set.contains(&v) && visited.insert(v) {
                        cq.push_back(v);
                    }
                }
            }
            let comp_set: HashSet<i64> = comp.iter().copied().collect();
            let lroot = comp.iter().copied().min().unwrap_or(start);
            let mut lmag: HashMap<i64, i32> = HashMap::new();
            lmag.insert(lroot, 0);
            let mut lq: VecDeque<i64> = VecDeque::new();
            lq.push_back(lroot);
            while let Some(u) = lq.pop_front() {
                let mu = lmag[&u];
                for &v in adj.get(&u).into_iter().flatten() {
                    if comp_set.contains(&v) && !lmag.contains_key(&v) {
                        lmag.insert(v, mu + 1);
                        lq.push_back(v);
                    }
                }
            }
            for (k, v) in lmag {
                mag.insert(k, 1 + v);
            }
        }

        // ── Determine left/right sign based on semantic pin directions ──
        let mut rank: HashMap<i64, i32> = HashMap::new();
        rank.insert(root_id, 0);

        // Collect direction hints: for each box connected to root, check if pins are mostly input or output
        let root_nbrs: Vec<i64> = adj.get(&root_id).cloned().unwrap_or_default();
        let mut left_side: HashSet<i64> = HashSet::new();
        let mut right_side: HashSet<i64> = HashSet::new();

        for &nbr in &root_nbrs {
            if !core_set.contains(&nbr) {
                continue;
            }
            // Check pin semantics for direction hint
            let mut input_count = 0usize;
            let mut output_count = 0usize;
            for pin in semantic.pins.values() {
                if pin.key.box_id != nbr {
                    continue;
                }
                match pin.io_direction {
                    IoDirection::Input => input_count += 1,
                    IoDirection::Output => output_count += 1,
                    _ => {}
                }
            }
            if input_count > output_count {
                left_side.insert(nbr);
            } else if output_count > input_count {
                right_side.insert(nbr);
            }
        }

        // BFS assign sign: propagate from root neighbors
        let mut sign_q: VecDeque<i64> = VecDeque::new();
        for &nbr in &root_nbrs {
            if left_side.contains(&nbr) {
                rank.insert(nbr, -(mag[&nbr]));
                sign_q.push_back(nbr);
            } else if right_side.contains(&nbr) {
                rank.insert(nbr, mag[&nbr]);
                sign_q.push_back(nbr);
            }
        }

        while let Some(u) = sign_q.pop_front() {
            let su = rank[&u];
            let is_left = su < 0;
            for &v in adj.get(&u).into_iter().flatten() {
                if rank.contains_key(&v) || !core_set.contains(&v) {
                    continue;
                }
                let mv = mag[&v];
                if is_left {
                    rank.insert(v, -mv);
                } else {
                    rank.insert(v, mv);
                }
                sign_q.push_back(v);
            }
        }

        // Remaining unassigned: use BFS sign from any neighbor, or default to positive
        for &id in &core_ids {
            if rank.contains_key(&id) {
                continue;
            }
            let m = mag.get(&id).copied().unwrap_or(1);
            // Check if any neighbor has a sign
            let neighbor_sign = adj
                .get(&id)
                .into_iter()
                .flatten()
                .find_map(|v| rank.get(v).copied());
            if let Some(s) = neighbor_sign {
                rank.insert(id, if s < 0 { -m } else { m });
            } else {
                rank.insert(id, m);
            }
        }

        let mut vals: Vec<i32> = rank.values().copied().collect();
        vals.sort();
        vals.dedup();
        (vals, rank)
    }

    /// Assign lanes within each rank using barycenter ordering.
    fn assign_lanes(
        graph: &McVecGraph,
        columns: &BTreeMap<i32, Vec<i64>>,
        adj: &HashMap<i64, Vec<i64>>,
        rank_map: &HashMap<i64, i32>,
        config: &LayeredLayouter,
    ) -> HashMap<i64, i32> {
        let _ = graph;
        let mut lane: HashMap<i64, i32> = HashMap::new();

        // Initial lane assignment: stable order within each column
        for (_, col) in columns {
            for (i, &id) in col.iter().enumerate() {
                lane.insert(id, i as i32);
            }
        }

        // Barycenter sweeps
        let sweeps = config.bary_sweeps;
        let sorted_ranks: Vec<i32> = {
            let mut v: Vec<i32> = columns.keys().copied().collect();
            v.sort();
            v
        };
        if sorted_ranks.len() < 2 {
            return lane;
        }

        for sweep in 0..sweeps {
            if sweep % 2 == 0 {
                // Left to right
                for w in sorted_ranks.windows(2) {
                    let (ref_r, r) = (w[0], w[1]);
                    reorder_rank_by_ref(columns, adj, rank_map, r, ref_r, &mut lane);
                }
            } else {
                // Right to left
                for w in sorted_ranks.windows(2).rev() {
                    let (r, ref_r) = (w[0], w[1]);
                    reorder_rank_by_ref(columns, adj, rank_map, r, ref_r, &mut lane);
                }
            }
        }

        lane
    }
}

/// Reorder boxes in rank `r` based on barycenter of neighbors in `ref_r`.
fn reorder_rank_by_ref(
    columns: &BTreeMap<i32, Vec<i64>>,
    adj: &HashMap<i64, Vec<i64>>,
    rank_map: &HashMap<i64, i32>,
    r: i32,
    ref_r: i32,
    lane: &mut HashMap<i64, i32>,
) {
    let ref_col = match columns.get(&ref_r) {
        Some(c) => c,
        None => return,
    };
    let ref_index: HashMap<i64, usize> =
        ref_col.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    let col = match columns.get(&r) {
        Some(c) => c.clone(),
        None => return,
    };

    let mut sorted = col;
    sorted.sort_by(|&a, &b| {
        let ba = barycenter_lane(a, &ref_index, adj);
        let bb = barycenter_lane(b, &ref_index, adj);
        ba.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (i, &id) in sorted.iter().enumerate() {
        lane.insert(id, i as i32);
    }
    let _ = rank_map;
}

fn barycenter_lane(id: i64, ref_index: &HashMap<i64, usize>, adj: &HashMap<i64, Vec<i64>>) -> f64 {
    let idxs: Vec<usize> = adj
        .get(&id)
        .map(|nbs| {
            nbs.iter()
                .filter_map(|n| ref_index.get(n).copied())
                .collect()
        })
        .unwrap_or_default();
    if idxs.is_empty() {
        0.0
    } else {
        idxs.iter().sum::<usize>() as f64 / idxs.len() as f64
    }
}

// ============================================================================
// GeometryBoxPlacement
// ============================================================================

/// Geometry placement of a single box.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryBoxPlacement {
    pub box_id: i64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

// ============================================================================
// GeometryPlacement
// ============================================================================

/// Full geometry placement for a graph.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GeometryPlacement {
    pub boxes: BTreeMap<i64, GeometryBoxPlacement>,
    pub canvas: (f64, f64),
}

impl GeometryPlacement {
    /// Solve geometry from logical placement.
    pub fn solve(logical: &LogicalPlacement, graph: &McVecGraph, config: &LayeredLayouter) -> Self {
        let mut wp: HashMap<i64, f64> = HashMap::new();
        let mut hp: HashMap<i64, f64> = HashMap::new();
        for b in &graph.boxes {
            wp.insert(b.id, b.w);
            hp.insert(b.id, b.h);
        }

        let w = |id: i64| -> f64 { wp.get(&id).copied().unwrap_or(60.0) };
        let h = |id: i64| -> f64 { hp.get(&id).copied().unwrap_or(40.0) };

        // ── Compute column widths ──
        let sorted_ranks: Vec<i32> = {
            let mut v: Vec<i32> = logical.columns.keys().copied().collect();
            v.sort();
            v
        };

        let mut col_width: HashMap<i32, f64> = HashMap::new();
        for r in &sorted_ranks {
            let max_w = logical
                .columns
                .get(r)
                .map(|col| col.iter().map(|&id| w(id)).fold(0.0_f64, f64::max))
                .unwrap_or(0.0);
            col_width.insert(*r, max_w.max(config.col_pitch));
        }

        // ── Column x positions ──
        let mut col_x: HashMap<i32, f64> = HashMap::new();
        let mut cx = CANVAS_MARGIN;
        for r in &sorted_ranks {
            col_x.insert(*r, cx);
            cx += col_width.get(r).copied().unwrap_or(config.col_pitch);
        }

        // ── Compute max column height ──
        let max_col_h: f64 = sorted_ranks
            .iter()
            .map(|r| {
                logical
                    .columns
                    .get(r)
                    .map(|col| {
                        let sum_h: f64 = col.iter().map(|&id| h(id)).sum();
                        let gaps = if col.len() > 1 {
                            (col.len() - 1) as f64 * config.row_pitch
                        } else {
                            0.0
                        };
                        sum_h + gaps
                    })
                    .unwrap_or(0.0)
            })
            .fold(0.0_f64, f64::max);

        let mid_y = CANVAS_MARGIN + max_col_h / 2.0;

        // ── Place boxes ──
        let mut boxes: BTreeMap<i64, GeometryBoxPlacement> = BTreeMap::new();
        for r in &sorted_ranks {
            let col = match logical.columns.get(r) {
                Some(c) => c,
                None => continue,
            };
            let col_h: f64 = {
                let sum_h: f64 = col.iter().map(|&id| h(id)).sum();
                let gaps = if col.len() > 1 {
                    (col.len() - 1) as f64 * config.row_pitch
                } else {
                    0.0
                };
                sum_h + gaps
            };

            let mut cur_y = mid_y - col_h / 2.0;
            for &id in col {
                let bw = w(id);
                let bh = h(id);
                let x = col_x.get(r).copied().unwrap_or(CANVAS_MARGIN) + (col_width[r] - bw) / 2.0;
                boxes.insert(
                    id,
                    GeometryBoxPlacement {
                        box_id: id,
                        x,
                        y: cur_y,
                        w: bw,
                        h: bh,
                    },
                );
                cur_y += bh + config.row_pitch;
            }
        }

        // Canvas
        let max_x = sorted_ranks
            .last()
            .map(|r| {
                col_x.get(r).copied().unwrap_or(0.0) + col_width.get(r).copied().unwrap_or(0.0)
            })
            .unwrap_or(200.0)
            + CANVAS_MARGIN;
        let max_y = mid_y + max_col_h / 2.0 + CANVAS_MARGIN;

        GeometryPlacement {
            boxes,
            canvas: (max_x, max_y),
        }
    }
}

// ============================================================================
// LayeredLayouter
// ============================================================================

/// M6 semantic-driven layered placement prototype.
pub struct LayeredLayouter {
    pub col_pitch: f64,
    pub row_pitch: f64,
    pub group_gap: f64,
    pub flag_gap: f64,
    pub bary_sweeps: usize,
    pub recompute_sizes: bool,
}

impl Default for LayeredLayouter {
    fn default() -> Self {
        Self {
            col_pitch: 420.0,
            row_pitch: 180.0,
            group_gap: 100.0,
            flag_gap: 64.0,
            bary_sweeps: 6,
            recompute_sizes: false,
        }
    }
}

impl LayeredLayouter {
    /// Sub-layer configuration (more compact).
    pub fn sub() -> Self {
        Self {
            col_pitch: 320.0,
            row_pitch: 110.0,
            group_gap: 70.0,
            flag_gap: 56.0,
            bary_sweeps: 8,
            recompute_sizes: true,
        }
    }
}

impl Layouter for LayeredLayouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        // ── Phase 1: Prepare graph ──
        explode_power_rails_to_flags(graph);
        promote_synthetic_pins(graph);
        split_shared_pins(graph);
        assign_default_sizes(graph);
        assign_entry_points_coarse(graph);

        // Early exit A: fully disconnected → grid
        if is_fully_disconnected(graph) {
            place_single_row(graph);
            return compute_canvas(graph);
        }

        // Early exit B: single box
        if graph.boxes.len() == 1 {
            graph.boxes[0].x = CANVAS_MARGIN;
            graph.boxes[0].y = CANVAS_MARGIN;
            return compute_canvas(graph);
        }

        // ── Phase 0: Semantic snapshot ──
        let semantic = SemanticModel::analyze(graph);

        // ── Phase 2: Build logical placement ──
        let logical = LogicalPlacement::from_graph_and_semantics(graph, &semantic, self);

        // ── Phase 4: Solve geometry ──
        let geometry = GeometryPlacement::solve(&logical, graph, self);

        // ── Write back x/y to graph ──
        for (id, geo) in &geometry.boxes {
            if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == *id) {
                b.x = geo.x;
                b.y = geo.y;
            }
        }

        // ── Phase 5: Pin placement (M7 PinAnchorModel) ──
        let root_id = logical.root_box_id.unwrap_or(graph.boxes[0].id);
        let _report = pin_anchor_pipeline(
            graph,
            Some(&semantic),
            PinAnchorConfig {
                hub_id: Some(root_id),
                lr_only: true,
                hub_keep_semantic: false,
                allow_power_ground_top_bottom: true,
                align_hub_to_spokes: true,
                straighten_facing_pairs: true,
            },
        );

        // ── Phase 6: Post / normalize ──
        eject_flags_from_boxes(graph);
        enforce_unique_offsets(graph);
        normalize_positions(graph);
        compute_canvas(graph)
    }

    fn name(&self) -> &'static str {
        "layered"
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn is_fully_disconnected(graph: &McVecGraph) -> bool {
    if graph.nets.is_empty() {
        return true;
    }
    let adj = build_adjacency(graph);
    let core_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| !is_rail_box(b))
        .map(|b| b.id)
        .collect();
    if core_ids.len() <= 1 {
        return false;
    }
    let start = core_ids[0];
    let mut visited: HashSet<i64> = HashSet::new();
    let mut q: VecDeque<i64> = VecDeque::new();
    q.push_back(start);
    visited.insert(start);
    while let Some(u) = q.pop_front() {
        for &v in adj.get(&u).into_iter().flatten() {
            if visited.insert(v) {
                q.push_back(v);
            }
        }
    }
    core_ids.iter().any(|id| !visited.contains(id))
}

fn place_single_row(graph: &mut McVecGraph) {
    let mut cx = CANVAS_MARGIN;
    let cy = CANVAS_MARGIN;
    for b in &mut graph.boxes {
        b.x = cx;
        b.y = cy;
        cx += b.w + 80.0;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{BoxPin, IoSummary};
    use crate::vector::graph::netdef::{EndpointRef, IoDirection};
    use crate::vector::graph::{BoxKind, McVecBox, McVecGraph, NetKind, Symbol, VizNet};

    fn mk_box(
        id: i64,
        name: &str,
        kind: BoxKind,
        symbol: Symbol,
        pin_count: usize,
        x: f64,
        y: f64,
    ) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            symbol,
            None,
            None,
            pin_count,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 60.0;
        b.h = 40.0;
        b
    }

    fn mk_ic(id: i64, name: &str, pin_count: usize) -> McVecBox {
        mk_box(id, name, BoxKind::MultiPin, Symbol::Ic, pin_count, 0.0, 0.0)
    }

    fn add_pin(b: &mut McVecBox, pin_id: i64, name: &str, io: IoDirection) {
        b.pins.push(BoxPin {
            id: pin_id,
            pin_id: name.into(),
            description: name.into(),
            io,
        });
    }

    fn ep(box_id: i64, pin_id: i64, pin_name: &str, io: IoDirection) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, pin_name, io)
    }

    // ── Test: hub at rank 0 ──

    #[test]
    fn hub_at_rank_zero() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_ic(1, "U1", 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let mut r = mk_box(2, "R1", BoxKind::TwoPin, Symbol::Resistor, 2, 0.0, 0.0);
        add_pin(&mut r, 1, "1", IoDirection::Passive);
        add_pin(&mut r, 2, "2", IoDirection::Passive);
        graph.boxes.push(r);

        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(1, 4, "OUT", IoDirection::Output),
                ep(2, 1, "1", IoDirection::Input),
            ],
        ));

        let semantic = SemanticModel::analyze(&graph);
        let config = LayeredLayouter::default();
        let logical = LogicalPlacement::from_graph_and_semantics(&graph, &semantic, &config);

        assert_eq!(logical.root_box_id, Some(1));
        let hub = &logical.boxes[&1];
        assert_eq!(hub.rank, 0);
        assert_eq!(hub.reason, PlacementReason::Hub);
    }

    // ── Test: input-ish box on left ──

    #[test]
    fn input_ish_box_on_left() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_ic(1, "U1", 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let mut conn = mk_box(2, "J1", BoxKind::MultiPin, Symbol::Ic, 2, 0.0, 0.0);
        add_pin(&mut conn, 1, "1", IoDirection::Input);
        add_pin(&mut conn, 2, "2", IoDirection::Input);
        graph.boxes.push(conn);

        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(2, 1, "1", IoDirection::Input),
                ep(1, 3, "IN", IoDirection::Input),
            ],
        ));

        let semantic = SemanticModel::analyze(&graph);
        let config = LayeredLayouter::default();
        let logical = LogicalPlacement::from_graph_and_semantics(&graph, &semantic, &config);

        // Connector J1 has only Input pins → should be left of hub
        let conn_placement = &logical.boxes[&2];
        assert!(
            conn_placement.rank < 0,
            "expected rank < 0, got {}",
            conn_placement.rank
        );
    }

    // ── Test: output-ish box on right ──

    #[test]
    fn output_ish_box_on_right() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_ic(1, "U1", 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let mut conn = mk_box(2, "J1", BoxKind::MultiPin, Symbol::Ic, 2, 0.0, 0.0);
        add_pin(&mut conn, 1, "1", IoDirection::Output);
        add_pin(&mut conn, 2, "2", IoDirection::Output);
        graph.boxes.push(conn);

        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(1, 4, "OUT", IoDirection::Output),
                ep(2, 1, "1", IoDirection::Output),
            ],
        ));

        let semantic = SemanticModel::analyze(&graph);
        let config = LayeredLayouter::default();
        let logical = LogicalPlacement::from_graph_and_semantics(&graph, &semantic, &config);

        // Connector J1 has only Output pins → should be right of hub
        let conn_placement = &logical.boxes[&2];
        assert!(
            conn_placement.rank > 0,
            "expected rank > 0, got {}",
            conn_placement.rank
        );
    }

    // ── Test: deterministic ──

    #[test]
    fn logical_placement_deterministic() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_ic(1, "U1", 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let mut r = mk_box(2, "R1", BoxKind::TwoPin, Symbol::Resistor, 2, 0.0, 0.0);
        add_pin(&mut r, 1, "1", IoDirection::Passive);
        add_pin(&mut r, 2, "2", IoDirection::Passive);
        graph.boxes.push(r);

        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(1, 4, "OUT", IoDirection::Output),
                ep(2, 1, "1", IoDirection::Input),
            ],
        ));

        let semantic = SemanticModel::analyze(&graph);
        let config = LayeredLayouter::default();
        let a = LogicalPlacement::from_graph_and_semantics(&graph, &semantic, &config);
        let b = LogicalPlacement::from_graph_and_semantics(&graph, &semantic, &config);
        assert_eq!(a, b);
    }

    // ── Test: geometry no overlap within same rank ──

    #[test]
    fn geometry_no_overlap_same_rank() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_ic(1, "U1", 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let mut r1 = mk_box(2, "R1", BoxKind::TwoPin, Symbol::Resistor, 2, 0.0, 0.0);
        add_pin(&mut r1, 1, "1", IoDirection::Output);
        add_pin(&mut r1, 2, "2", IoDirection::Output);
        graph.boxes.push(r1);

        let mut r2 = mk_box(3, "R2", BoxKind::TwoPin, Symbol::Resistor, 2, 0.0, 0.0);
        add_pin(&mut r2, 1, "1", IoDirection::Output);
        add_pin(&mut r2, 2, "2", IoDirection::Output);
        graph.boxes.push(r2);

        graph.nets.push(VizNet::new(
            1,
            "SIG1".into(),
            NetKind::Signal,
            vec![
                ep(1, 4, "OUT", IoDirection::Output),
                ep(2, 1, "1", IoDirection::Output),
            ],
        ));
        graph.nets.push(VizNet::new(
            2,
            "SIG2".into(),
            NetKind::Signal,
            vec![
                ep(1, 4, "OUT", IoDirection::Output),
                ep(3, 1, "1", IoDirection::Output),
            ],
        ));

        let semantic = SemanticModel::analyze(&graph);
        let config = LayeredLayouter::default();
        let logical = LogicalPlacement::from_graph_and_semantics(&graph, &semantic, &config);
        let geometry = GeometryPlacement::solve(&logical, &graph, &config);

        // Check no overlap between boxes in same rank
        for (_, col) in &logical.columns {
            for i in 0..col.len() {
                for j in (i + 1)..col.len() {
                    let a = &geometry.boxes[&col[i]];
                    let b = &geometry.boxes[&col[j]];
                    let overlap =
                        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                    assert!(
                        !overlap,
                        "boxes {} and {} overlap in same rank",
                        col[i], col[j]
                    );
                }
            }
        }
    }

    // ── Test: LayeredLayouter produces valid layout ──

    #[test]
    fn layered_layouter_produces_valid_layout() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_ic(1, "U1", 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let mut r = mk_box(2, "R1", BoxKind::TwoPin, Symbol::Resistor, 2, 0.0, 0.0);
        add_pin(&mut r, 1, "1", IoDirection::Passive);
        add_pin(&mut r, 2, "2", IoDirection::Passive);
        graph.boxes.push(r);

        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(1, 4, "OUT", IoDirection::Output),
                ep(2, 1, "1", IoDirection::Input),
            ],
        ));

        let layouter = LayeredLayouter::default();
        let canvas = layouter.layout(&mut graph);

        assert!(canvas.0 > 0.0);
        assert!(canvas.1 > 0.0);
        assert!(graph
            .boxes
            .iter()
            .all(|b| b.x.is_finite() && b.y.is_finite()));
    }

    // ── Test: name ──

    #[test]
    fn layered_layouter_name() {
        let layouter = LayeredLayouter::default();
        assert_eq!(layouter.name(), "layered");
    }

    // ── Test: empty graph ──

    #[test]
    fn layered_empty_graph() {
        let mut graph = McVecGraph::new(0, "test".into());
        let layouter = LayeredLayouter::default();
        let canvas = layouter.layout(&mut graph);
        assert_eq!(canvas, (200.0, 100.0));
    }
}
