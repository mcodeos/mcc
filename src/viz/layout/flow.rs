// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Stage A + B —— Connectivity-driven top-level flow layout engine (FlowLayouter)
//!
//! ## What problem does this file solve
//! `SchematicRadialLayouter` only models "each box ↔ anchor", spreading all modules
//! equidistantly around MCU, crossings are **forced** by layout. `FlowLayouter` uses **full edge** information for layout.
//!
//! ## Stage A (implemented)
//! - A2: First explode power rails into local flags (see `rails.rs`), flags extracted from core layout, no trunk.
//! - A1: Core modules layered by connectivity + barycenter to remove crossings, flags placed next to consumer.
//!
//! ## Stage B (this time)
//! Stage A first version ranker used "directed edge longest-path" as main approach, but most top-level connections are io/directionless,
//! causing many nodes to be mistakenly identified as rank0 sources, all piled into hub column → vertical spaghetti. This rewrite:
//!
//! - **B1 — hub-BFS layering**: rank = **undirected BFS distance** with hub as root. Direction only used for
//!   *selecting root* (main chip → directed source → max degree) and determining left/right orientation for isolated components. Hub's neighbors
//!   must fall in adjacent columns, no longer stacked in same column.
//! - **B2 — Dual-side layout (hub-specific)**: When "dominant hub" is detected (degree far exceeds others), place hub
//!   in middle column, its branches (connected subgraph of core minus hub, keep whole group) distribute to
//!   left/right sides by height → rank with sign (negative=left, 0=hub, positive=right). Wires fan out to both sides, column height halved.
//! - **B3 — Flag de-overlap**: Multiple power flags on same side of same box spread evenly centered along the edge.
//!
//! ## Reuse
//! size / entry_points / overlap / normalize all reuse existing helpers.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::vector::graph::naming;
use crate::vector::graph::net_def::IoDirection;
use crate::vector::graph::{EntrySide, McVecBox, McVecGraph, NetKind, Symbol};

use super::components::{build_adjacency, find_connected_components};
use super::entry_points::{
    assign_entry_points_coarse, enforce_unique_offsets, promote_synthetic_pins, split_shared_pins,
};
use super::ladder_model::LadderModel;
use super::ladder_place::{apply_ladder_model, LadderGeometry};
use super::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use super::optimize::PlaceOptimizer;
use super::rails::{explode_power_rails_to_flags, is_rail_box};
use super::size::{assign_default_sizes, recompute_sizes_with_pin_count};
use crate::viz::traits::Layouter;

// ============================================================================
// FlowLayouter
// ============================================================================

pub struct FlowLayouter {
    /// Column pitch (actual takes max(this value, widest box + gap))
    pub col_pitch: f64,
    /// Vertical spacing between adjacent boxes in same column
    pub row_pitch: f64,
    /// Distance from flag to consumer edge
    pub flag_gap: f64,
    /// Number of barycenter crossing removal sweeps (bidirectional, each direction counts as one round)
    pub bary_sweeps: usize,
    /// Dual-side layout trigger threshold: hub degree ≥ this value and > second highest degree to enable dual-side
    pub hub_min_degree: usize,
    /// ★ FIX (subgraph): whether to recompute box size by pin name/number after pin assignment (activates box_size pin-aware path). Top-level = false (size unchanged), sub-level = true (enlarge uC/SubModule).
    pub recompute_sizes: bool,
    /// Routing mode switch for multi-terminal single-driver nets / buses (router/scheduler reads graph.fanout_star):
    /// - `true`  = hub-star: all loads converge to **the same pin point on the driver device**, multiple wires fan out from that point.
    /// - `false` = TrunkTap / BusBundle: one trunk + each pin taps in separately (standard schematic practice).
    ///
    /// ★ Change: default changed from `true` to `false`. `true` was originally to cover up "top-level synthetic endpoint collapse"
    /// (this issue is now fundamentally fixed by **unconditionally** calling `promote_synthetic_pins` in layout phase), but it draws
    /// single-driver multi-load nets as "several wires fanning out from one point", not following schematic conventions. After changing to `false`, each pin
    /// connects at its own exit point then wires out.
    pub fanout_star: bool,
    /// 05b: hub keep semantic sides (Input=Left, Output=Right). true = old behavior, false = connectivity-first.
    pub hub_keep_semantic: bool,
    /// Ladder model + committed geometry (populated by Phase B when the graph is a clean
    /// two-lane bridged-passive ladder). `None` = graph is not a ladder, or model bailed.
    pub ladder: Option<(LadderModel, LadderGeometry)>,
}

impl Default for FlowLayouter {
    fn default() -> Self {
        Self {
            col_pitch: 480.0,
            row_pitch: 220.0,
            flag_gap: 64.0,
            bary_sweeps: 6,
            hub_min_degree: 4,
            recompute_sizes: false,
            fanout_star: false,
            hub_keep_semantic: false,
            ladder: None,
        }
    }
}

impl FlowLayouter {
    /// Configuration for sub-layer: IC anchoring + more compact spacing (passive components are small, many in quantity)
    pub fn sub() -> Self {
        Self {
            col_pitch: 360.0,
            row_pitch: 120.0,
            flag_gap: 60.0,
            bary_sweeps: 8,
            hub_min_degree: 3,
            recompute_sizes: true,
            fanout_star: false,
            hub_keep_semantic: false,
            ladder: None,
        }
    }

    /// 相位 1 · Prepare — 拓扑归一 + 粗粒度 pin。
    ///
    /// 写：graph.fanout 相关的合成/拆分结构、盒子初始尺寸、coarse entry_points。
    fn phase_prepare(&self, graph: &mut McVecGraph) {
        explode_power_rails_to_flags(graph);
        promote_synthetic_pins(graph);
        split_shared_pins(graph);
        assign_default_sizes(graph);
        assign_entry_points_coarse(graph);
    }

    /// 备用出口 A — 完全无连接图：pin-aware 重算尺寸后走网格铺满画布。
    fn exit_grid(&self, graph: &mut McVecGraph) -> (f64, f64) {
        assign_default_sizes(graph);
        place_grid(graph);
        enforce_unique_offsets(graph);
        normalize_positions(graph);
        compute_canvas(graph)
    }

    /// 相位 2 · Size — 只增不减的尺寸调整。
    fn phase_size(&self, graph: &mut McVecGraph) {
        if self.recompute_sizes {
            recompute_sizes_with_pin_count(graph);
        }
        size_by_core_fanout(graph);
        floor_box_sizes(graph);
        probe_degenerate_boxes(graph, "after phase_size");
    }

    /// 相位 3 · Placement — 只写盒子位置（x/y），全部先于 pin_place。
    ///
    /// 返回 (root_id, isolated_ids) 供后续相位使用。
    fn phase_placement(&self, graph: &mut McVecGraph) -> (i64, HashSet<i64>) {
        let ranks = assign_flow_ranks(graph, self.hub_min_degree);
        let columns = order_columns(graph, &ranks, self.bary_sweeps);
        self.place_columns(graph, &columns);
        refine_y_coordinates(graph, 4, self.row_pitch);
        PlaceOptimizer::default().run(graph);

        let root_id = ranks
            .iter()
            .find(|(_, r)| **r == 0)
            .map(|(id, _)| *id)
            .unwrap_or(graph.boxes[0].id);

        let isolated_ids = compute_isolated_ids(graph, root_id);

        align_leaf_to_neighbor(graph, root_id);
        group_supply_modules(graph, &isolated_ids);

        (root_id, isolated_ids)
    }

    /// 相位 5 · Post — 几何保持的盒子移动，pin_place 之后安全。
    fn phase_post(
        &self,
        graph: &mut McVecGraph,
        flag_boxes: Vec<McVecBox>,
        flag_meta: &FlagMeta,
        isolated_ids: &HashSet<i64>,
    ) {
        graph.boxes.extend(flag_boxes);
        self.place_flags(graph, flag_meta);
        park_isolated_components(graph, isolated_ids);
        eject_flags_from_boxes(graph);
        normalize_positions(graph);
    }
}

impl Layouter for FlowLayouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        eprintln!("{}", super::chain::extract_signal_chains(graph).dump(graph));

        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        graph.fanout_star = self.fanout_star;

        // ── 相位 1 · Prepare：拓扑归一 + 粗粒度 pin ──
        self.phase_prepare(graph);

        // 备用出口 A：完全无连接 → 网格布局（早退）
        if is_fully_disconnected(graph) {
            return self.exit_grid(graph);
        }

        // ── 相位 2 · Size：pin-aware 尺寸 + 按扇出增高 ──
        self.phase_size(graph);

        // 备用出口 B：单盒子（早退）
        if graph.boxes.len() == 1 {
            graph.boxes[0].x = CANVAS_MARGIN;
            graph.boxes[0].y = CANVAS_MARGIN;
            return compute_canvas(graph);
        }

        // 抽出 flags 供核心布局（Post 相位归位）
        let (flag_boxes, flag_meta) = split_flags(graph);

        // 备用出口 C：抽走 flag 后只剩空（早退）
        if graph.boxes.is_empty() {
            graph.boxes.extend(flag_boxes);
            place_single_row(graph);
            return compute_canvas(graph);
        }

        // ── 相位 3 · Placement（只写盒子位置）+ PROBE-B 契约校验 ──
        let ep_snap = probe_ep_snapshot(graph);
        let (root_id, isolated_ids) = self.phase_placement(graph);
        probe_no_ep_writes("phase_placement", graph, &ep_snap);

        // 旧路径照跑：模型命中时被 ladder_place 完全覆盖，模型 bail 时兜底
        super::two_lane_ladder::try_two_lane_ladder(graph);

        // ── M11+M12 Idiom-aware placement (pre-pin) ──
        // 在 phase_placement 之后、pin_place_pipeline 之前移动 satellite 器件。
        // 只移动 cap/resistor 等 satellite，不移动 IC/connector/module 等 anchor。
        // 跳过 ladder-locked (geom_locked) 的盒子。
        // M12: score-all candidates → deterministic best, with determinism report.
        {
            let protected: std::collections::HashSet<i64> = graph
                .boxes
                .iter()
                .filter(|b| b.geom_locked)
                .map(|b| b.id)
                .collect();
            let model = crate::viz::idiom::place::analyze_idiom_placement(graph, &protected);
            let report = crate::viz::idiom::place::apply_idiom_placement_pre_pins(graph, &model);
            if report.idioms_detected > 0 {
                eprintln!("{}", report.report_line());
            }
            let mut det_report =
                crate::viz::stability::report::DeterminismReport::from_graph(graph);
            det_report = det_report.with_idiom(
                &model.instances,
                &model.constraints,
                &report.selected_candidates,
            );
            eprintln!("{}", det_report.report_line());
        }

        // ── 相位 4 · PinPlacement：EntryPoint 唯一写者 + hub 几何唯一终定者 ──
        super::pin_place::pin_place_pipeline(graph, Some(root_id), true, self.hub_keep_semantic);
        probe_degenerate_boxes(graph, "after pin_place");

        // ★ Ladder：net 表推导的确定性摆位。必须在 pin_place 之后 —— 做最后一个写者。
        // 模型命中时覆盖 two_lane_ladder 写的一切；模型 bail 时旧路径已兜底。
        if let Some(m) = super::ladder_model::try_build_ladder_model(graph) {
            apply_ladder_model(graph, &m);
        }

        // ── 相位 5 · Post：几何保持的移动，pin_place 之后安全 ──
        self.phase_post(graph, flag_boxes, &flag_meta, &isolated_ids);

        compute_canvas(graph)
    }

    fn name(&self) -> &'static str {
        "flow"
    }
}

// ============================================================================
// Flag extraction / metadata
// ============================================================================

/// flag_id → (consumer_box_id, consumer_pin_id, is_ground, net_id, net_name)
struct FlagTarget {
    consumer_box_id: i64,
    consumer_pin_id: i64,
    is_ground: bool,
    net_id: i64,
    net_name: String,
}

type FlagMeta = HashMap<i64, FlagTarget>;

fn split_flags(graph: &mut McVecGraph) -> (Vec<McVecBox>, FlagMeta) {
    let flag_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    let mut meta: FlagMeta = HashMap::new();
    for net in &graph.nets {
        let flag_ep = net.endpoints.iter().find(|e| flag_ids.contains(&e.box_id));
        let cons_ep = net.endpoints.iter().find(|e| !flag_ids.contains(&e.box_id));
        if let (Some(fe), Some(ce)) = (flag_ep, cons_ep) {
            let is_gnd = matches!(net.kind, NetKind::Ground);
            meta.insert(
                fe.box_id,
                FlagTarget {
                    consumer_box_id: ce.box_id,
                    consumer_pin_id: ce.pin_id,
                    is_ground: is_gnd,
                    net_id: net.nid,
                    net_name: net.name.clone(),
                },
            );
        }
    }

    let flags: Vec<_> = graph
        .boxes
        .iter()
        .filter(|b| flag_ids.contains(&b.id))
        .cloned()
        .collect();
    graph.boxes.retain(|b| !flag_ids.contains(&b.id));
    (flags, meta)
}

// ============================================================================
// Size: height ∝ signal net count (vertical stretch, let parallel wire bundles spread apart)
// ============================================================================

/// Box height scaled by "total pin count" (only increase, never decrease).
///
/// Pin count ≈ connected net count. More connections → taller box, pins on left/right naturally spread out;
/// also ensures boxes like moddcdc with "few signals but many power outputs" have enough vertical space to spread flags.
fn size_by_core_fanout(graph: &mut McVecGraph) {
    const PITCH: f64 = 28.0; // Vertical spacing reserved for each pin
    const PAD: f64 = 26.0;
    for b in &mut graph.boxes {
        if is_rail_box(b) {
            continue; // flags stay small
        }
        let n = b.entry_points.len().max(b.pins.len()) as f64;
        let want_h = n * PITCH + PAD;
        if want_h > b.h {
            b.h = want_h;
        }
    }
}

// ★ PR-A: `align_hub_to_spokes` moved to `pin_place::align_hub_to_spokes` (now a pass inside
//   pin_place_pipeline, so pin_place stays the single writer of EntryPoint.{side,offset}).

/// ★ P0b — leaf aligns to neighbor (dual of align_hub_to_spokes).
///
/// align_hub only stretches hub to align peripherals; leaf↔leaf (mic↔speaker) or connections not covered by hub,
/// lines still slant→bend. This pass for each non-hub box **with only one core neighbor**, shifts entire box vertically,
/// aligning "its pin cluster connecting to that neighbor" with "neighbor's corresponding pin cluster" (single net → perfectly horizontal line). Collision
/// check before shift, give up if hitting other boxes (alignment is soft constraint, doesn't break "no overlap" hard constraint).
///
/// Must run **before** align_hub_to_spokes: leaves position first, hub stretches to cover final leaf position →
/// two-step convergence, no oscillation (hub doesn't move leaves, leaf movement has collision guard).
fn align_leaf_to_neighbor(graph: &mut McVecGraph, hub_id: i64) {
    // Current coordinate snapshot (owned, avoid borrow conflict with later iter_mut)
    let rects: HashMap<i64, (f64, f64, f64, f64)> = graph
        .boxes
        .iter()
        .map(|b| (b.id, (b.x, b.y, b.w, b.h)))
        .collect();
    let mut pin_y: HashMap<(i64, i64), f64> = HashMap::new();
    for b in &graph.boxes {
        for e in &b.entry_points {
            pin_y.insert((b.id, e.pin_id), pin_abs(b, &e.side, e.offset).1);
        }
    }

    // Compute candidate shift amounts
    let mut shifts: Vec<(i64, f64)> = Vec::new();
    for b in &graph.boxes {
        if b.id == hub_id || is_rail_box(b) {
            continue;
        }
        let mut neighbors: HashSet<i64> = HashSet::new();
        let mut pairs: Vec<(f64, f64)> = Vec::new(); // (this pin Y, neighbor pin Y)
        for net in &graph.nets {
            let mine: Vec<i64> = net
                .endpoints
                .iter()
                .filter(|e| e.box_id == b.id)
                .map(|e| e.pin_id)
                .collect();
            if mine.is_empty() {
                continue;
            }
            // Only recognize "positioned real boxes" as opposite end (flags not in boxes now → auto excluded)
            let other = net
                .endpoints
                .iter()
                .find(|e| e.box_id != b.id && rects.contains_key(&e.box_id));
            let oe = match other {
                Some(e) => e,
                None => continue,
            };
            neighbors.insert(oe.box_id);
            let nbr_y = pin_y
                .get(&(oe.box_id, oe.pin_id))
                .copied()
                .unwrap_or_else(|| {
                    rects
                        .get(&oe.box_id)
                        .map(|r| r.1 + r.3 / 2.0)
                        .unwrap_or(0.0)
                });
            for pid in &mine {
                if let Some(&sy) = pin_y.get(&(b.id, *pid)) {
                    pairs.push((sy, nbr_y));
                }
            }
        }
        // Only align leaves with "single core neighbor" (multi-neighbor direction unclear, leave to router)
        if neighbors.len() != 1 || pairs.is_empty() {
            continue;
        }
        let delta = pairs.iter().map(|(s, n)| n - s).sum::<f64>() / pairs.len() as f64;
        if delta.abs() < 1.0 {
            continue;
        }
        shifts.push((b.id, delta));
    }

    let mut moved = 0usize;
    for (bid, delta) in shifts {
        let (x, y, w, h) = rects.get(&bid).copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
        let target = (x, y + delta, w, h);
        const GAP: f64 = 12.0;
        let collides = graph
            .boxes
            .iter()
            .any(|o| o.id != bid && rects_overlap(target, (o.x, o.y, o.w, o.h), GAP));
        if collides {
            continue;
        }
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == bid) {
            b.y += delta;
            moved += 1;
        }
    }
    crate::vlog!(
        "[flow::align_leaf] graph '{}' bid={}: moved {} leaf(s) to align with neighbor",
        graph.name,
        graph.bid,
        moved
    );
}

/// Do two rectangles (x,y,w,h) still overlap after leaving gap
fn rects_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64), gap: f64) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    !(ax + aw + gap <= bx || bx + bw + gap <= ax || ay + ah + gap <= by || by + bh + gap <= ay)
}

/// Directed edge statistics: (indeg, outdeg) (driver = Output/Bidir, sink = Input)
fn directed_degrees(
    graph: &McVecGraph,
    core_set: &HashSet<i64>,
) -> (HashMap<i64, usize>, HashMap<i64, usize>) {
    let mut indeg: HashMap<i64, usize> = core_set.iter().map(|&id| (id, 0)).collect();
    let mut outdeg: HashMap<i64, usize> = core_set.iter().map(|&id| (id, 0)).collect();
    let mut seen: HashSet<(i64, i64)> = HashSet::new();
    for net in &graph.nets {
        let mut drivers: Vec<i64> = Vec::new();
        let mut sinks: Vec<i64> = Vec::new();
        for e in &net.endpoints {
            if !core_set.contains(&e.box_id) {
                continue;
            }
            match e.io_type {
                IoDirection::Output | IoDirection::Bidir => drivers.push(e.box_id),
                IoDirection::Input => sinks.push(e.box_id),
                _ => {}
            }
        }
        for &d in &drivers {
            for &s in &sinks {
                if d != s && seen.insert((d, s)) {
                    *outdeg.entry(d).or_default() += 1;
                    *indeg.entry(s).or_default() += 1;
                }
            }
        }
    }
    (indeg, outdeg)
}

/// Choose root: main chip → IC (most pins) → directed source (max outdeg) → max degree → first
fn choose_root(
    graph: &McVecGraph,
    adj: &HashMap<i64, Vec<i64>>,
    indeg: &HashMap<i64, usize>,
    outdeg: &HashMap<i64, usize>,
) -> i64 {
    if let Some(b) = graph.boxes.iter().find(|b| naming::is_main_chip(&b.name)) {
        return b.id;
    }
    // Sub-layer anchoring: prefer IC with most pins (top-level module is Module, won't match → behavior unchanged)
    if let Some(b) = graph
        .boxes
        .iter()
        .filter(|b| matches!(b.symbol, Symbol::Ic))
        .max_by_key(|b| b.pin_count)
    {
        return b.id;
    }
    let src = graph
        .boxes
        .iter()
        .filter(|b| {
            indeg.get(&b.id).copied().unwrap_or(0) == 0
                && outdeg.get(&b.id).copied().unwrap_or(0) > 0
        })
        .max_by_key(|b| outdeg.get(&b.id).copied().unwrap_or(0))
        .map(|b| b.id);
    if let Some(s) = src {
        return s;
    }
    // Iter 1: exclude two-pin passives from max-degree fallback —
    // aligns with chain::find_hub semantics so flow and chain agree on the hub.
    graph
        .boxes
        .iter()
        .filter(|b| !b.is_two_pin_passive())
        .max_by_key(|b| adj.get(&b.id).map(|v| v.len()).unwrap_or(0))
        .map(|b| b.id)
        .unwrap_or(graph.boxes[0].id)
}

/// Signed rank for each core box (negative=left, 0=hub, positive=right)
fn assign_flow_ranks(graph: &McVecGraph, hub_min_degree: usize) -> HashMap<i64, i32> {
    let core_ids: Vec<i64> = graph.boxes.iter().map(|b| b.id).collect();
    let core_set: HashSet<i64> = core_ids.iter().copied().collect();
    let adj = build_adjacency(graph); // flags already extracted → core adjacency
    let (indeg, outdeg) = directed_degrees(graph, &core_set);
    let root = choose_root(graph, &adj, &indeg, &outdeg);

    // ── Global undirected BFS distance (mag) ──
    let mut mag: HashMap<i64, i32> = HashMap::new();
    mag.insert(root, 0);
    let mut q: VecDeque<i64> = VecDeque::new();
    q.push_back(root);
    while let Some(u) = q.pop_front() {
        let mu = mag[&u];
        for &v in adj.get(&u).into_iter().flatten() {
            if !mag.contains_key(&v) {
                mag.insert(v, mu + 1);
                q.push_back(v);
            }
        }
    }
    // ── Isolated components (BFS can't reach root): each from local source / min id, mag = 1 + local depth ──
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
                if visited.insert(v) {
                    cq.push_back(v);
                }
            }
        }
        let comp_set: HashSet<i64> = comp.iter().copied().collect();
        let lroot = comp
            .iter()
            .copied()
            .filter(|id| {
                indeg.get(id).copied().unwrap_or(0) == 0 && outdeg.get(id).copied().unwrap_or(0) > 0
            })
            .min()
            .unwrap_or_else(|| *comp.iter().min().unwrap());
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
            mag.insert(k, 1 + v); // offset 1, isolated components start at hub's right column
        }
    }

    // ── Is dominant hub (star-shaped) ──
    let root_deg = adj.get(&root).map(|v| v.len()).unwrap_or(0);
    let second_deg = graph
        .boxes
        .iter()
        .filter(|b| b.id != root)
        .map(|b| adj.get(&b.id).map(|v| v.len()).unwrap_or(0))
        .max()
        .unwrap_or(0);
    let n = core_ids.len();
    let root_box = graph.boxes.iter().find(|b| b.id == root);
    let root_is_ic = root_box
        .map(|b| matches!(b.symbol, Symbol::Ic))
        .unwrap_or(false);
    // ★ Main chip (name contains mcu/cpu/soc/fpga...) even if symbol is Module counts as hub candidate.
    //   Top-level controller collapses to Module (not Ic), previously only Ic took loose two-sided gate → controller treated as normal source
    //   node, single-sided layering → "stick to left, peripherals all on right". Include main chip in loose gate, let it radiate from center to both sides.
    let root_is_main_chip = root_box
        .map(|b| naming::is_main_chip(&b.name))
        .unwrap_or(false);
    let dominant = (root_deg >= hub_min_degree
        && root_deg > second_deg
        && (root_deg as f64) >= 0.4 * (n as f64 - 1.0))
        // Sub-layer IC / any-layer main chip: is "most connected (≥ second place) and ≥3" core → radiate from center to both sides,
        //   don't stack into one column. This is exactly what user wants: "core components radiate outward from center".
        || ((root_is_ic || root_is_main_chip) && root_deg >= 3 && root_deg >= second_deg);

    if !dominant {
        crate::vlog!(
            "[layout::flow] root={} (deg={}), single-sided layering",
            root,
            root_deg
        );
        return mag;
    }

    // ── Two-sided: branches = connected subgraph of (core minus root); assign entire groups to left/right, balance by height ──
    let branches = branches_excluding(root, &adj, &core_ids);
    let box_h: HashMap<i64, f64> = graph.boxes.iter().map(|b| (b.id, b.h)).collect();
    let mut branch_h: Vec<(usize, f64)> = branches
        .iter()
        .enumerate()
        .map(|(i, br)| {
            (
                i,
                br.iter()
                    .map(|id| box_h.get(id).copied().unwrap_or(60.0))
                    .sum(),
            )
        })
        .collect();
    branch_h.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut left_h = 0.0_f64;
    let mut right_h = 0.0_f64;
    let mut side_of: HashMap<usize, i32> = HashMap::new();
    for (bi, h) in branch_h {
        if left_h <= right_h {
            side_of.insert(bi, -1);
            left_h += h;
        } else {
            side_of.insert(bi, 1);
            right_h += h;
        }
    }

    let mut rank: HashMap<i64, i32> = HashMap::new();
    rank.insert(root, 0);
    for (bi, br) in branches.iter().enumerate() {
        let s = *side_of.get(&bi).unwrap_or(&1);
        for &id in br {
            let m = mag.get(&id).copied().unwrap_or(1).max(1);
            rank.insert(id, s * m);
        }
    }
    for &id in &core_ids {
        rank.entry(id).or_insert(1);
    }

    let min_r = rank.values().copied().min().unwrap_or(0);
    let max_r = rank.values().copied().max().unwrap_or(0);
    crate::vlog!(
        "[layout::flow] root={} (deg={}), two-sided: columns [{}..{}], {} branch(es)",
        root,
        root_deg,
        min_r,
        max_r,
        branches.len()
    );
    rank
}

/// Connected subgraph of core minus root (used for assigning entire groups to left/right)
fn branches_excluding(root: i64, adj: &HashMap<i64, Vec<i64>>, core_ids: &[i64]) -> Vec<Vec<i64>> {
    let mut visited: HashSet<i64> = HashSet::new();
    visited.insert(root);
    let mut out: Vec<Vec<i64>> = Vec::new();
    for &start in core_ids {
        if visited.contains(&start) {
            continue;
        }
        let mut comp: Vec<i64> = Vec::new();
        let mut q: VecDeque<i64> = VecDeque::new();
        q.push_back(start);
        visited.insert(start);
        while let Some(u) = q.pop_front() {
            comp.push(u);
            for &v in adj.get(&u).into_iter().flatten() {
                if v == root {
                    continue;
                }
                if visited.insert(v) {
                    q.push_back(v);
                }
            }
        }
        out.push(comp);
    }
    out
}

// ============================================================================
// Isolated component parking
// ============================================================================

/// ★ Compute "isolated component" box set: those connected components **not containing hub**.
///
/// When to call: must be after split_flags, before place_flags (flags extracted → build_adjacency
/// is pure core adjacency, won't miscount components due to per-consumer flags).
///
/// Example: usbsocket↔modldo only connected via Vin, only power (became flag) between it and main circuit (mcu...) →
/// They are a connected component without hub → all enter isolated set. moddcdc if has real connection (like [VCC_1V2,GND]
/// bundle net) to main → in hub component → not in isolated set → stays in main layout.
fn compute_isolated_ids(graph: &McVecGraph, hub_id: i64) -> HashSet<i64> {
    let adj = build_adjacency(graph);
    let comps = find_connected_components(&graph.boxes, &adj);
    let mut out = HashSet::new();
    for c in &comps {
        if c.contains(&hub_id) {
            continue;
        }
        for &id in c {
            out.insert(id);
        }
    }
    if !out.is_empty() {
        crate::vlog!(
            "[layout::flow] isolated components: {} box(es) not connected to hub {}",
            out.len(),
            hub_id
        );
    }
    out
}

/// ★ Shift isolated components as a whole to open area below main body (rigid shift, preserves internal relative layout).
///
/// Main layout calculated normally (isolated boxes participated in placement, but this pass moves them as a group at the end → main
/// body box positions unaffected). Isolated box flags (V5V etc) found by net and moved together, no one left behind.
///
/// When to call: after place_flags **completed** (flags positioned to move together), before normalize (after shift,
/// normalize + recalculate canvas).
fn park_isolated_components(graph: &mut McVecGraph, isolated_ids: &HashSet<i64>) {
    if isolated_ids.is_empty() {
        return;
    }

    // 1. Flags of isolated boxes also need to move: find flags with "one end is flag, other end is isolated box" by net.
    let flag_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();
    let mut move_set: HashSet<i64> = isolated_ids.clone();
    for net in &graph.nets {
        let flag = net.endpoints.iter().find(|e| flag_ids.contains(&e.box_id));
        let cons = net
            .endpoints
            .iter()
            .find(|e| isolated_ids.contains(&e.box_id));
        if let (Some(f), Some(_)) = (flag, cons) {
            move_set.insert(f.box_id);
        }
    }

    // 2. Main body bounding box (non move_set) bottom-left + isolated cluster (move_set) top-left
    let (mut main_minx, mut main_maxy) = (f64::MAX, f64::MIN);
    let (mut iso_minx, mut iso_miny) = (f64::MAX, f64::MAX);
    for b in &graph.boxes {
        if move_set.contains(&b.id) {
            iso_minx = iso_minx.min(b.x);
            iso_miny = iso_miny.min(b.y);
        } else {
            main_minx = main_minx.min(b.x);
            main_maxy = main_maxy.max(b.y + b.h);
        }
    }
    // All isolated boxes (no main body) → don't move (no "open area" concept)
    if !main_maxy.is_finite() || !iso_minx.is_finite() {
        return;
    }

    // 3. Parking spot: whitespace below main body, left-aligned with main body left edge. Rigid shift entire isolated box + flag group.
    const GAP: f64 = 160.0;
    let dx = main_minx - iso_minx;
    let dy = (main_maxy + GAP) - iso_miny;
    let mut moved = 0usize;
    for b in &mut graph.boxes {
        if move_set.contains(&b.id) {
            b.x += dx;
            b.y += dy;
            moved += 1;
        }
    }
    crate::vlog!(
        "[layout::flow] parked {} isolated box(es) (+flags) to open area below main (dx={:.0}, dy={:.0})",
        moved, dx, dy
    );
}

// ============================================================================
// Supply chain grouping — consolidate power modules into bottom row
// ============================================================================

/// Is power supply module (USB power socket / LDO / DCDC / regulator...).

/// Criteria: not power flag (PowerRail symbol), and name/class name contains power supply keywords.
/// Covers usbsocket(POWER_SYS) / modldo(POWER_LDO) / moddcdc(POWER_DCDC) in image.
pub(crate) fn is_supply_module(b: &McVecBox) -> bool {
    if b.symbol.is_power_rail() {
        return false; // power flag itself is not "module"
    }
    let hay = format!("{} {}", b.name, b.class_name).to_uppercase();
    const TOK: &[&str] = &[
        "POWER", "LDO", "DCDC", "REGULAT", "VREG", "PMIC", "PMU", "BUCK", "BOOST", "CHARGER",
    ];
    TOK.iter().any(|t| hay.contains(t))
}

/// Consolidate power supply modules into **bottom row** (schematic convention: power area centralized).
///
/// - Only moves when ≥2 (single doesn't form "chain").
/// - Placed below current core bounding box; ordered left→right by current x (roughly preserves USB→LDO→DCDC power flow order).
/// - When to call: before place_flags → after power modules moved, their flags automatically stick to side.
/// - Power distribution to peripherals goes via same-name flags (no connections), so moving to bottom doesn't lengthen those; real power transfer between modules
///   (Vin/V5V etc) becomes shorter due to proximity placement.
fn group_supply_modules(graph: &mut McVecGraph, exclude: &HashSet<i64>) {
    let ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_supply_module(b) && !exclude.contains(&b.id))
        .map(|b| b.id)
        .collect();
    if ids.len() < 2 {
        return; // fewer than 2, not a chain, don't move (avoid damaging single power component's existing position)
    }

    // Sort by current x left→right (preserve relative power flow order; connected two power modules mostly already x-adjacent).
    let xs: HashMap<i64, f64> = graph.boxes.iter().map(|b| (b.id, b.x)).collect();
    let mut order = ids;
    order.sort_by(|a, b| {
        xs.get(a)
            .unwrap_or(&0.0)
            .partial_cmp(xs.get(b).unwrap_or(&0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Bounding box only counts **non-power components** (signal main body), letting power row snug against main body below, not lowered by power components' old positions. Safe starting point for degenerate case of all power components (no others).
    let supply_set: HashSet<i64> = order.iter().copied().collect();
    let (mut min_x, mut max_y) = (f64::MAX, f64::MIN);
    let mut found = false;
    for b in &graph.boxes {
        if supply_set.contains(&b.id) {
            continue;
        }
        found = true;
        min_x = min_x.min(b.x);
        max_y = max_y.max(b.y + b.h);
    }
    if !found || !max_y.is_finite() {
        min_x = CANVAS_MARGIN;
        max_y = CANVAS_MARGIN;
    }

    const ROW_GAP: f64 = 140.0; // vertical spacing from main body above (leaving room for flags + connections)
    const H_GAP: f64 = 90.0; // horizontal spacing between modules
    let row_y = max_y + ROW_GAP;
    let mut cur_x = min_x;
    for id in &order {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == *id) {
            b.x = cur_x;
            b.y = row_y;
            cur_x += b.w + H_GAP;
        }
    }
    crate::vlog!(
        "[layout::flow] supply-chain: grouped {} power module(s) into bottom row (y={:.0})",
        order.len(),
        row_y
    );
}

// ============================================================================
// barycenter de-crossing
// ============================================================================

fn order_columns(graph: &McVecGraph, ranks: &HashMap<i64, i32>, sweeps: usize) -> Vec<Vec<i64>> {
    // signed rank → sort dedup → column index
    let mut vals: Vec<i32> = ranks.values().copied().collect();
    vals.sort();
    vals.dedup();
    let col_of: HashMap<i32, usize> = vals.iter().enumerate().map(|(i, &v)| (v, i)).collect();

    let mut cols: Vec<Vec<i64>> = vec![Vec::new(); vals.len()];
    for (&id, &r) in ranks {
        if let Some(&c) = col_of.get(&r) {
            cols[c].push(id);
        }
    }
    for c in cols.iter_mut() {
        c.sort();
    }

    let adj = build_adjacency(graph);
    let max_col = cols.len().saturating_sub(1);
    for sweep in 0..sweeps {
        if sweep % 2 == 0 {
            for r in 1..=max_col {
                reorder_by_ref(&mut cols, r, r - 1, &adj);
            }
        } else {
            for r in (0..max_col).rev() {
                reorder_by_ref(&mut cols, r, r + 1, &adj);
            }
        }
    }

    cols.retain(|c| !c.is_empty());
    cols
}

fn reorder_by_ref(cols: &mut [Vec<i64>], r: usize, ref_r: usize, adj: &HashMap<i64, Vec<i64>>) {
    let ref_row: Vec<i64> = cols[ref_r].clone();
    let ref_index: HashMap<i64, usize> =
        ref_row.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    let mut row = std::mem::take(&mut cols[r]);
    let cur_index: HashMap<i64, usize> = row.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    row.sort_by(|&a, &b| {
        let ka = barycenter(a, &ref_index, adj, cur_index[&a]);
        let kb = barycenter(b, &ref_index, adj, cur_index[&b]);
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
    cols[r] = row;
}

fn barycenter(
    id: i64,
    ref_index: &HashMap<i64, usize>,
    adj: &HashMap<i64, Vec<i64>>,
    fallback_idx: usize,
) -> f64 {
    let idxs: Vec<usize> = adj
        .get(&id)
        .map(|nbs| {
            nbs.iter()
                .filter_map(|n| ref_index.get(n).copied())
                .collect()
        })
        .unwrap_or_default();
    if idxs.is_empty() {
        fallback_idx as f64
    } else {
        idxs.iter().sum::<usize>() as f64 / idxs.len() as f64
    }
}

// ============================================================================
// ★ P5 — Column-internal Y coordinate refinement (Sugiyama coordinate assignment phase)
// ============================================================================

/// ★ P5 switch: if this pass introduces regression, change to false → fully restore pre-change coordinates (zero-risk rollback).
const ENABLE_Y_REFINE: bool = true;

/// ★ P5 — Column-internal Y coordinate refinement (Sugiyama coordinate assignment phase, currently missing from pipeline).
///
/// order_columns only sets order within column, place_columns stacks at equal intervals → box Y unrelated to neighbors, wires slant through.
/// This pass preserves column order, repeatedly pulls each box toward "median of neighbor center Y", then uses order-preserving minimum spacing projection
/// (PAVA) to land positions. Only modifies Y, x unchanged, bounded iteration. `row_gap` = minimum vertical gap between adjacent boxes in column
/// (pass self.row_pitch → only align/spread, not compress, most conservative).
fn refine_y_coordinates(graph: &mut McVecGraph, iters: usize, row_gap: f64) {
    if !ENABLE_Y_REFINE || graph.boxes.len() < 3 {
        return;
    }
    let adj = build_adjacency(graph); // flags already removed → core connections (power/ground go through flags, don't constrain layout)

    // Group into columns by x (this pass doesn't modify x → group once). x quantized to 4px tolerance.
    let mut col_of: HashMap<i64, Vec<i64>> = HashMap::new();
    for b in &graph.boxes {
        col_of
            .entry((b.x / 4.0).round() as i64)
            .or_default()
            .push(b.id);
    }
    let mut col_keys: Vec<i64> = col_of.keys().copied().collect();
    col_keys.sort();

    const DAMP: f64 = 0.8; // Fraction to move toward median each pass (< 1 prevents overshoot)

    for sweep in 0..iters {
        // Alternate left-right, so displacement propagates both ways
        let keys: Vec<i64> = if sweep % 2 == 0 {
            col_keys.clone()
        } else {
            col_keys.iter().rev().copied().collect()
        };

        for ck in keys {
            let ids = match col_of.get(&ck) {
                Some(v) => v.clone(),
                None => continue,
            };
            if ids.is_empty() {
                continue;
            }

            // Current position snapshot (including previous columns updated in this sweep → Gauss-Seidel, fast convergence)
            let cy: HashMap<i64, f64> = graph
                .boxes
                .iter()
                .map(|b| (b.id, b.y + b.h / 2.0))
                .collect();
            let hmap: HashMap<i64, f64> = graph.boxes.iter().map(|b| (b.id, b.h)).collect();

            // Sort within column by current y ascending (= preserve existing order)
            let mut ordered = ids.clone();
            ordered.sort_by(|a, b| {
                cy.get(a)
                    .unwrap_or(&0.0)
                    .partial_cmp(cy.get(b).unwrap_or(&0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let h: Vec<f64> = ordered
                .iter()
                .map(|id| *hmap.get(id).unwrap_or(&0.0))
                .collect();

            // Each box's desired top Y = (damped neighbor center median) − h/2; if no neighbors keep current.
            let desired_top: Vec<f64> = ordered
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let cur_c = *cy.get(id).unwrap_or(&0.0);
                    let mut ns: Vec<f64> = adj
                        .get(id)
                        .into_iter()
                        .flatten()
                        .filter_map(|n| cy.get(n).copied())
                        .collect();
                    let tgt_c = if ns.is_empty() {
                        cur_c
                    } else {
                        ns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        let m = ns.len();
                        let med = if m % 2 == 1 {
                            ns[m / 2]
                        } else {
                            (ns[m / 2 - 1] + ns[m / 2]) / 2.0
                        };
                        cur_c + DAMP * (med - cur_c)
                    };
                    tgt_c - h[i] / 2.0
                })
                .collect();

            // PAVA order-preserving minimum spacing projection: require y[i+1] ≥ y[i] + h[i] + row_gap.
            //   Let s[i]=Σ_{k<i}(h[k]+gap), u[i]=y[i]−s[i] → constraint becomes u non-decreasing; for
            //   t[i]=desired_top[i]−s[i] do order-preserving regression to get the closest feasible u.
            let n = ordered.len();
            let mut s = vec![0.0_f64; n];
            for i in 1..n {
                s[i] = s[i - 1] + h[i - 1] + row_gap;
            }
            let t: Vec<f64> = (0..n).map(|i| desired_top[i] - s[i]).collect();
            let u = pava(&t);

            for i in 0..n {
                let new_top = u[i] + s[i];
                if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == ordered[i]) {
                    b.y = new_top;
                }
            }
        }
    }

    crate::vlog!(
        "[layout::flow] P5 y-refine: {} sweeps over {} column(s)",
        iters,
        col_keys.len()
    );
}

/// Order-preserving regression (pool adjacent violators): returns the closest **non-decreasing** sequence to `t` (L2 optimal).
fn pava(t: &[f64]) -> Vec<f64> {
    let mut val: Vec<f64> = Vec::with_capacity(t.len());
    let mut wt: Vec<f64> = Vec::with_capacity(t.len());
    for &ti in t {
        val.push(ti);
        wt.push(1.0);
        // Last two blocks violate non-decreasing (prev > next) → merge taking weighted mean
        while val.len() >= 2 && val[val.len() - 2] > val[val.len() - 1] {
            let (v2, w2) = (val.pop().unwrap(), wt.pop().unwrap());
            let (v1, w1) = (val.pop().unwrap(), wt.pop().unwrap());
            val.push((v1 * w1 + v2 * w2) / (w1 + w2));
            wt.push(w1 + w2);
        }
    }
    // Expand back to each element
    let mut out = Vec::with_capacity(t.len());
    for (v, w) in val.iter().zip(wt.iter()) {
        for _ in 0..(*w as usize) {
            out.push(*v);
        }
    }
    out
}

// ============================================================================
// Placement
// ============================================================================

impl FlowLayouter {
    fn place_columns(&self, graph: &mut McVecGraph, columns: &[Vec<i64>]) {
        if columns.is_empty() {
            return;
        }
        let max_w = graph.boxes.iter().map(|b| b.w).fold(0.0_f64, f64::max);
        let pitch = self.col_pitch.max(max_w + 80.0);

        // Box height lookup: first take as owned HashMap, so the closure below borrows hmap not graph,
        //   then placement phase can do graph.boxes.iter_mut() normally (otherwise closure holding &graph conflicts with mutable borrow).
        let hmap: std::collections::HashMap<i64, f64> =
            graph.boxes.iter().map(|b| (b.id, b.h)).collect();
        let box_h = |id: i64| -> f64 { hmap.get(&id).copied().unwrap_or(0.0) };

        // ── Fold each rank column into a "near-square" sub-column grid ──
        //   If a rank has multiple boxes (typical: hub's bunch of peripheral neighbors BFS distance all=1 → all fall in same
        //   rank → old version squashed into a sparse vertical bar, large empty space on both sides), split into k sub-columns horizontally
        //   by target height. k = round(sqrt(column total height / column spacing)) → grid width ≈ height, fill the 2D space next to hub,
        //   leaving maximum routing margin. Single-box column (like hub itself) / short column → k=1, behavior matches old version, chain
        //   /small graph no regression. Each sub-column height balanced (column total height / k), no column stuffed full and another empty.
        let mut bands: Vec<Vec<Vec<i64>>> = Vec::new(); // bands[col] = sub-column set of that column
        for col in columns {
            let n = col.len();
            let tallest_in_col = col.iter().map(|&id| box_h(id)).fold(0.0_f64, f64::max);
            let total_h: f64 = col.iter().map(|&id| box_h(id)).sum::<f64>()
                + if n > 1 {
                    (n - 1) as f64 * self.row_pitch
                } else {
                    0.0
                };
            // Expected sub-column count (grid near-square); single-box column naturally gets 1.
            let k = ((total_h / pitch).sqrt().round() as usize).max(1);
            // Each sub-column target height: evenly divided, but at least fits the column's tallest box.
            let target = (total_h / k as f64).max(tallest_in_col);

            let mut subcols: Vec<Vec<i64>> = vec![Vec::new()];
            let mut cur_h = 0.0_f64;
            for &id in col {
                let h = box_h(id);
                let empty = subcols.last().map(|s| s.is_empty()).unwrap_or(true);
                let add = if empty { h } else { self.row_pitch + h };
                if !empty && cur_h + add > target {
                    subcols.push(vec![id]); // doesn't fit → open new sub-column
                    cur_h = h;
                } else {
                    subcols.last_mut().unwrap().push(id);
                    cur_h += add;
                }
            }
            bands.push(subcols);
        }

        // Sub-column stack height
        let band_h = |sc: &[i64]| -> f64 {
            let sum: f64 = sc.iter().map(|&id| box_h(id)).sum();
            let gaps = if sc.len() > 1 {
                (sc.len() - 1) as f64 * self.row_pitch
            } else {
                0.0
            };
            sum + gaps
        };

        // Global vertical centering baseline = tallest sub-column
        let max_h = bands
            .iter()
            .flatten()
            .map(|sc| band_h(sc))
            .fold(0.0_f64, f64::max);
        let mid_y = CANVAS_MARGIN + max_h / 2.0;

        // ── Placement: horizontal cursor advances by "sub-column" (each sub-column takes one pitch); within column stack vertically centered ──
        let mut cx = CANVAS_MARGIN + max_w / 2.0;
        for subcols in &bands {
            for sc in subcols {
                let h = band_h(sc);
                let mut cur_top = mid_y - h / 2.0;
                for &id in sc {
                    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
                        b.x = cx - b.w / 2.0;
                        b.y = cur_top;
                        cur_top += b.h + self.row_pitch;
                    }
                }
                cx += pitch;
            }
        }
    }

    /// Flag sticks to consumer (Stage F): adaptive edge-sticking + four-direction orientation
    ///
    /// No longer hardcoded "power up / ground down". For each consumer:
    /// - Compute "signal neighbor direction" (towards connected non-flag boxes)
    /// - Power flag sticks to **most away from neighbors** empty edge, ground sticks to the next empty other edge
    /// - Move the corresponding power/ground pin to that edge too (stub straight out, no detour)
    /// - Flag's single pin faces consumer; renderer side (power_rail.rs) draws symbol based on this edge's four-direction
    fn place_flags(&self, graph: &mut McVecGraph, meta: &FlagMeta) {
        let flag_ids: HashSet<i64> = meta.keys().copied().collect();

        let centers: HashMap<i64, (f64, f64)> = graph
            .boxes
            .iter()
            .map(|b| (b.id, (b.x + b.w / 2.0, b.y + b.h / 2.0)))
            .collect();

        // consumer → signal neighbor direction (sum of unit vectors pointing to connected non-flag boxes)
        let mut nbr_dir: HashMap<i64, (f64, f64)> = HashMap::new();
        let mut nbr_axis: HashMap<i64, (f64, f64)> = HashMap::new(); // (busy_x, busy_y) absolute magnitude
        for net in &graph.nets {
            let cores: Vec<i64> = net
                .endpoints
                .iter()
                .map(|e| e.box_id)
                .filter(|id| !flag_ids.contains(id))
                .collect();
            for &a in &cores {
                for &b in &cores {
                    if a == b {
                        continue;
                    }
                    if let (Some(&(ax, ay)), Some(&(bx, by))) = (centers.get(&a), centers.get(&b)) {
                        let (dx, dy) = (bx - ax, by - ay);
                        let l = (dx * dx + dy * dy).sqrt().max(1.0);
                        let e = nbr_dir.entry(a).or_insert((0.0, 0.0));
                        e.0 += dx / l;
                        e.1 += dy / l;
                        let ax_e = nbr_axis.entry(a).or_insert((0.0, 0.0));
                        ax_e.0 += (dx / l).abs();
                        ax_e.1 += (dy / l).abs();
                    }
                }
            }
        }

        // consumer → [(flag_id, name, is_gnd, consumer_pin)]
        let mut by_consumer: HashMap<i64, Vec<(i64, String, bool, i64)>> = HashMap::new();
        for (&fid, ft) in meta.iter() {
            let name = graph
                .boxes
                .iter()
                .find(|b| b.id == fid)
                .map(|b| b.name.clone())
                .unwrap_or_default();
            by_consumer.entry(ft.consumer_box_id).or_default().push((
                fid,
                name,
                ft.is_ground,
                ft.consumer_pin_id,
            ));
        }

        let mut flag_place: HashMap<i64, (f64, f64, EntrySide)> = HashMap::new();
        let mut pin_moves: Vec<(i64, i64, EntrySide, f64)> = Vec::new();

        for (&cbox, flags) in &by_consumer {
            let consumer = match graph.boxes.iter().find(|b| b.id == cbox) {
                Some(b) => b.clone(),
                None => continue,
            };
            let nd = nbr_dir.get(&cbox).copied().unwrap_or((1.0, 0.0));
            let ndl = (nd.0 * nd.0 + nd.1 * nd.1).sqrt();
            let ndu = if ndl > 1e-6 {
                (nd.0 / ndl, nd.1 / ndl)
            } else {
                (0.0, 0.0)
            };
            // Normalize busy axes (both sides connected → horizontally busy; one side → that direction busy)
            let na = nbr_axis.get(&cbox).copied().unwrap_or((1.0, 0.0));
            let nsum = (na.0 + na.1).max(1e-6);
            let (busy_x, busy_y) = (na.0 / nsum, na.1 / nsum);

            // Score 4 edges: 1.5×away-from-neighbor direction − busy axis penalty − stub length penalty (short stubs preferred)
            let edges = [
                EntrySide::Top,
                EntrySide::Bottom,
                EntrySide::Left,
                EntrySide::Right,
            ];
            let (ccx, ccy) = (consumer.x + consumer.w / 2.0, consumer.y + consumer.h / 2.0);
            let mut scored: Vec<(EntrySide, f64)> = edges
                .iter()
                .map(|e| {
                    let (nx, ny, _) = outward_and_opposite(e);
                    let dir_term = -(nx * ndu.0 + ny * ndu.1);
                    let axis_pen = if nx.abs() > 0.5 { busy_x } else { busy_y };
                    // Estimate stub length: distance from consumer center to edge midpoint + flag_gap
                    let (ex, ey) = edge_midpoint(&consumer, e);
                    let est_dist = ((ex - ccx).powi(2) + (ey - ccy).powi(2)).sqrt();
                    let stub_pen = if est_dist > LONG_PG_STUB {
                        (est_dist - LONG_PG_STUB) / 10.0
                    } else {
                        0.0
                    };
                    (e.clone(), 1.5 * dir_term - axis_pen - stub_pen)
                })
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let power_edge = scored[0].0.clone();
            let ground_edge = scored
                .iter()
                .find(|(e, _)| *e != power_edge)
                .map(|(e, _)| e.clone())
                .unwrap_or_else(|| power_edge.clone());

            // Distribute to sides
            let mut on_edge: HashMap<EntrySide, Vec<(i64, String, i64)>> = HashMap::new();
            for (fid, name, is_gnd, pin) in flags {
                let e = if *is_gnd {
                    ground_edge.clone()
                } else {
                    power_edge.clone()
                };
                on_edge
                    .entry(e)
                    .or_default()
                    .push((*fid, name.clone(), *pin));
            }

            // Each edge: horizontal edge by label width, vertical edge by longitudinal pitch, centered and spread; move pins + place flags
            for (edge, mut items) in on_edge {
                items.sort_by(|a, b| a.0.cmp(&b.0));
                let (ox, oy, opp) = outward_and_opposite(&edge);
                let tang = (-oy, ox);
                let (ecx, ecy) = edge_midpoint(&consumer, &edge);
                let is_vert = matches!(edge, EntrySide::Left | EntrySide::Right);
                let widths: Vec<f64> = items
                    .iter()
                    .map(|(_, n, _)| if is_vert { 42.0 } else { label_width(n) })
                    .collect();
                let total: f64 = widths.iter().sum::<f64>().max(1.0);
                let mut cursor = -total / 2.0;
                for (i, (fid, _name, pin)) in items.iter().enumerate() {
                    let w = widths[i];
                    let t = cursor + w / 2.0;
                    cursor += w;
                    let px = ecx + tang.0 * t;
                    let py = ecy + tang.1 * t;
                    let bx = px + ox * self.flag_gap;
                    let by = py + oy * self.flag_gap;
                    flag_place.insert(*fid, (bx, by, opp.clone()));
                    let off = offset_along_edge(&consumer, &edge, px, py);
                    let clamped = off.clamp(0.05, 0.95);
                    pin_moves.push((cbox, *pin, edge.clone(), clamped));
                    // Diagnostic: if clamped significantly, the flag is beyond edge bounds
                    if (clamped - off).abs() > 0.01 {
                        crate::vlog!(
                            "[viz::flow] flag offset clamped: cbox={} pin={} edge={:?} off={:.3} clamped={:.3}",
                            cbox, pin, edge, off, clamped
                        );
                    }
                }
            }
        }

        // Apply: move pins (power/ground pins moved to selected edge)
        for b in &mut graph.boxes {
            for (bid, pin, side, off) in &pin_moves {
                if b.id == *bid {
                    if let Some(ep) = b.entry_points.iter_mut().find(|e| e.pin_id == *pin) {
                        ep.side = side.clone();
                        ep.offset = off.clamp(0.05, 0.95);
                    }
                }
            }
        }
        // Apply: place flags + single pin faces consumer
        for b in &mut graph.boxes {
            if let Some(v) = flag_place.get(&b.id) {
                b.x = v.0 - b.w / 2.0;
                b.y = v.1 - b.h / 2.0;
                if let Some(ep) = b.entry_points.first_mut() {
                    ep.side = v.2.clone();
                    ep.offset = 0.5;
                }
            }
        }
    }
}

fn place_single_row(graph: &mut McVecGraph) {
    let mut cur_x = CANVAS_MARGIN;
    let y = CANVAS_MARGIN;
    for b in &mut graph.boxes {
        b.x = cur_x;
        b.y = y;
        cur_x += b.w + 60.0;
    }
}

/// Whether graph is "fully disconnected" —— no cross-box net (≥2 boxes but no inter-box connections).
///
/// Such graphs through flow layering will collapse to sparse single column (see notes in layout), better to use grid arrangement.
fn is_fully_disconnected(graph: &McVecGraph) -> bool {
    graph.boxes.len() >= 2 && !graph.nets.iter().any(|n| n.is_inter_box())
}

/// Grid arrangement: place boxes in near-square (slightly wider) grid covering the canvas.
///
/// For fully disconnected graphs —— no connection info to follow, arrange neatly in grid to avoid sparse single column.
/// - Column count takes `round(sqrt(n) * 1.25)`, making layout slightly wider than square (fits horizontal canvas better);
/// - **Preserve existing box order** (don't reorder, safer), fill cells row-first;
/// - Each column width = widest box in that column, each row height = tallest box in that row, boxes centered in their cells;
/// - Column gap / row gap fixed and moderate (not flow's row_pitch=220 large row spacing).
fn place_grid(graph: &mut McVecGraph) {
    let n = graph.boxes.len();
    if n == 0 {
        return;
    }

    let cols = (((n as f64).sqrt() * 1.25).round() as usize).clamp(1, n);
    let rows = (n + cols - 1) / cols;

    const COL_GAP: f64 = 70.0;
    const ROW_GAP: f64 = 60.0;

    // Each column max width / each row max height (row-first filling)
    let mut col_w = vec![0.0_f64; cols];
    let mut row_h = vec![0.0_f64; rows];
    for (i, b) in graph.boxes.iter().enumerate() {
        let c = i % cols;
        let r = i / cols;
        if b.w > col_w[c] {
            col_w[c] = b.w;
        }
        if b.h > row_h[r] {
            row_h[r] = b.h;
        }
    }

    // Each column starting x / each row starting y (prefix sum + gap), starting from canvas outer margin
    let mut col_x = vec![0.0_f64; cols];
    let mut acc_x = CANVAS_MARGIN;
    for c in 0..cols {
        col_x[c] = acc_x;
        acc_x += col_w[c] + COL_GAP;
    }
    let mut row_y = vec![0.0_f64; rows];
    let mut acc_y = CANVAS_MARGIN;
    for r in 0..rows {
        row_y[r] = acc_y;
        acc_y += row_h[r] + ROW_GAP;
    }

    // Each box centered in its cell
    for (i, b) in graph.boxes.iter_mut().enumerate() {
        let c = i % cols;
        let r = i / cols;
        b.x = col_x[c] + (col_w[c] - b.w) / 2.0;
        b.y = row_y[r] + (row_h[r] - b.h) / 2.0;
    }
}

// ── Geometry utilities ──

/// Absolute coordinates of edge midpoint
fn edge_midpoint(b: &McVecBox, side: &EntrySide) -> (f64, f64) {
    match side {
        EntrySide::Top => (b.x + b.w / 2.0, b.y),
        EntrySide::Bottom => (b.x + b.w / 2.0, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h / 2.0),
        EntrySide::Right => (b.x + b.w, b.y + b.h / 2.0),
    }
}

/// Pin's absolute coordinates (by side + offset)
pub(crate) fn pin_abs(b: &McVecBox, side: &EntrySide, offset: f64) -> (f64, f64) {
    match side {
        EntrySide::Top => (b.x + b.w * offset, b.y),
        EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
    }
}

/// Point (px,py) on a given edge → relative position offset along edge [0,1]
fn offset_along_edge(b: &McVecBox, side: &EntrySide, px: f64, py: f64) -> f64 {
    match side {
        EntrySide::Top | EntrySide::Bottom => {
            if b.w.abs() < 1e-6 {
                0.5
            } else {
                (px - b.x) / b.w
            }
        }
        EntrySide::Left | EntrySide::Right => {
            if b.h.abs() < 1e-6 {
                0.5
            } else {
                (py - b.y) / b.h
            }
        }
    }
}

/// Rough estimate of label width (occupancy width when spreading along edge)
fn label_width(name: &str) -> f64 {
    (name.chars().count() as f64 * 8.0 + 14.0).max(34.0)
}

/// (outward_x, outward_y, opposite_side)
fn outward_and_opposite(side: &EntrySide) -> (f64, f64, EntrySide) {
    match side {
        EntrySide::Top => (0.0, -1.0, EntrySide::Bottom),
        EntrySide::Bottom => (0.0, 1.0, EntrySide::Top),
        EntrySide::Left => (-1.0, 0.0, EntrySide::Right),
        EntrySide::Right => (1.0, 0.0, EntrySide::Left),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, EndpointRef, IoSummary, Symbol, VizNet};

    fn mk_mod(id: i64, name: &str, pins: usize) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::SubModule,
            Symbol::Module,
            None,
            None,
            pins,
            IoSummary::new(),
        );
        b.h = 60.0;
        b
    }

    fn mk_rail(id: i64, name: &str, is_ground: bool) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground },
            None,
            None,
            1,
            IoSummary::new(),
        )
    }

    /// Signal chain src→mid→sink: root picks directed source src, single-sided, column index increasing
    #[test]
    fn flow_chain_left_to_right() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "src", 2));
        g.boxes.push(mk_mod(2, "mid", 2));
        g.boxes.push(mk_mod(3, "sink", 2));
        g.nets.push(VizNet::new(
            10,
            "a".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(1, 11, "OUT", IoDirection::Output),
                EndpointRef::with_io(2, 21, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(VizNet::new(
            11,
            "b".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(2, 22, "OUT", IoDirection::Output),
                EndpointRef::with_io(3, 31, "IN", IoDirection::Input),
            ],
        ));
        let ranks = assign_flow_ranks(&g, 4);
        assert!(ranks[&1] < ranks[&2]);
        assert!(ranks[&2] < ranks[&3]);
    }

    /// Dominant hub (1 center connecting 5 leaves): hub=0, leaves split to left/right sides (negative and positive)
    #[test]
    fn flow_star_is_two_sided() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "hub", 8));
        for i in 2..=6 {
            g.boxes.push(mk_mod(i, &format!("leaf{}", i), 2));
        }
        for i in 2..=6 {
            g.nets.push(VizNet::new(
                100 + i,
                format!("s{}", i),
                NetKind::Signal,
                vec![
                    EndpointRef::with_io(1, 10 + i, "io", IoDirection::Bidir),
                    EndpointRef::with_io(i, 1, "io", IoDirection::Bidir),
                ],
            ));
        }
        let ranks = assign_flow_ranks(&g, 4);
        assert_eq!(ranks[&1], 0, "hub at column 0");
        let has_left = (2..=6).any(|i| ranks[&i] < 0);
        let has_right = (2..=6).any(|i| ranks[&i] > 0);
        assert!(has_left && has_right, "leaves split to both sides");
    }

    /// End-to-end + power rail: no panic, flag count correct, canvas reasonable
    #[test]
    fn flow_end_to_end_with_rails() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "mcu", 6));
        g.boxes.push(mk_mod(2, "spk", 4));
        g.boxes.push(mk_rail(100, "V3V3", false));
        g.boxes.push(mk_rail(101, "GND", true));
        g.nets.push(VizNet::new(
            10,
            "dac".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(1, 11, "DAC_OUT", IoDirection::Output),
                EndpointRef::with_io(2, 21, "DAC_IN", IoDirection::Input),
            ],
        ));
        g.nets.push(VizNet::new(
            11,
            "V3V3".into(),
            NetKind::Power,
            vec![
                EndpointRef::with_io(100, 1001, "V3V3", IoDirection::Power),
                EndpointRef::with_io(1, 12, "VDD", IoDirection::Power),
                EndpointRef::with_io(2, 22, "VDD", IoDirection::Power),
            ],
        ));
        g.nets.push(VizNet::new(
            12,
            "GND".into(),
            NetKind::Ground,
            vec![
                EndpointRef::with_io(101, 1011, "GND", IoDirection::Ground),
                EndpointRef::with_io(1, 13, "GND", IoDirection::Ground),
                EndpointRef::with_io(2, 23, "GND", IoDirection::Ground),
            ],
        ));

        let (cw, ch) = FlowLayouter::default().layout(&mut g);
        assert!(cw > 0.0 && ch > 0.0);
        let flags = g.boxes.iter().filter(|b| is_rail_box(b)).count();
        assert_eq!(flags, 4, "2 consumers × 2 rails = 4 flags");
        assert_eq!(g.boxes.len(), 6);
    }

    fn mk_supply(id: i64, name: &str, class: &str, x: f64, y: f64) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            class.into(),
            BoxKind::SubModule,
            Symbol::Module,
            None,
            None,
            3,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 80.0;
        b.h = 80.0;
        b
    }

    #[test]
    fn supply_module_detection() {
        let usb = mk_supply(1, "usbsocket", "POWER_SYS", 0.0, 0.0);
        let ldo = mk_supply(2, "modldo", "POWER_LDO", 0.0, 0.0);
        let dcdc = mk_supply(3, "moddcdc", "POWER_DCDC", 0.0, 0.0);
        let mcu = mk_supply(4, "mcu513", "US513", 0.0, 0.0); // Main controller, no power token
        assert!(is_supply_module(&usb));
        assert!(is_supply_module(&ldo));
        assert!(is_supply_module(&dcdc));
        assert!(
            !is_supply_module(&mcu),
            "Main controller is not a power module"
        );
        assert!(
            !is_supply_module(&mk_rail(5, "V3V3", false)),
            "Power flag is not a power module"
        );
    }

    #[test]
    fn supply_chain_grouped_to_bottom_row() {
        let mut g = McVecGraph::new(0, "main".into());
        // Signal component flash on top (y=0..100); two power modules initially scattered
        let mut flash = mk_mod(10, "flash", 4);
        flash.x = 0.0;
        flash.y = 0.0;
        flash.w = 100.0;
        flash.h = 100.0;
        g.boxes.push(flash);
        g.boxes
            .push(mk_supply(1, "usbsocket", "POWER_SYS", 500.0, 50.0));
        g.boxes
            .push(mk_supply(2, "modldo", "POWER_LDO", 200.0, 400.0));

        group_supply_modules(&mut g, &HashSet::new());

        let uy = g.boxes.iter().find(|b| b.id == 1).unwrap().y;
        let ly = g.boxes.iter().find(|b| b.id == 2).unwrap().y;
        assert!(
            (uy - ly).abs() < 1e-6,
            "Two power modules should be in same row"
        );
        assert!(
            uy > 100.0,
            "Power row should be below signal main body (flash bottom=100)"
        );
        let fy = g.boxes.iter().find(|b| b.id == 10).unwrap().y;
        assert!((fy - 0.0).abs() < 1e-6, "Non-power components don't move");
        // By original x left→right: modldo (original x=200) is left of usbsocket (original x=500)
        let lx = g.boxes.iter().find(|b| b.id == 2).unwrap().x;
        let ux = g.boxes.iter().find(|b| b.id == 1).unwrap().x;
        assert!(lx < ux, "Bottom row by original x left→right");
    }

    #[test]
    fn supply_single_module_untouched() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(mk_supply(1, "usbsocket", "POWER_SYS", 500.0, 50.0));
        group_supply_modules(&mut g, &HashSet::new());
        let b = g.boxes.iter().find(|b| b.id == 1).unwrap();
        assert!(
            (b.x - 500.0).abs() < 1e-6 && (b.y - 50.0).abs() < 1e-6,
            "Single power module not a chain, don't move"
        );
    }
}

// ============================================================================
// PROBE-B — 校验 Placement 相位没有偷写 EntryPoint
// ----------------------------------------------------------------------------
// 只在 MC_VIZ_DUMP 启用时运行；debug_assert 在新代码违约时 panic。
// 预期日志：
//   [PROBE-B] ✓ phase_placement respected phase contract (no entry_point writes)
// ============================================================================

/// 快照 (box_id, pin_id) → (side 判别串, offset)。
fn probe_ep_snapshot(graph: &McVecGraph) -> HashMap<(i64, i64), (String, f64)> {
    let mut m = HashMap::new();
    for b in &graph.boxes {
        for ep in &b.entry_points {
            m.insert((b.id, ep.pin_id), (format!("{:?}", ep.side), ep.offset));
        }
    }
    m
}

fn probe_no_ep_writes(pass: &str, graph: &McVecGraph, before: &HashMap<(i64, i64), (String, f64)>) {
    if !crate::viz::debug::dump_enabled() {
        return;
    }
    let mut violations = 0usize;
    for b in &graph.boxes {
        for ep in &b.entry_points {
            let now = (format!("{:?}", ep.side), ep.offset);
            match before.get(&(b.id, ep.pin_id)) {
                Some(old) if *old != now => {
                    violations += 1;
                    crate::vlog!(
                        "[PROBE-B] ✗ {} wrote entry_point on box#{} pin {}: {:?} → {:?}",
                        pass,
                        b.id,
                        ep.pin_id,
                        old,
                        now
                    );
                }
                None => {
                    violations += 1;
                    crate::vlog!(
                        "[PROBE-B] ✗ {} added entry_point on box#{} pin {}",
                        pass,
                        b.id,
                        ep.pin_id
                    );
                }
                _ => {}
            }
        }
    }
    if violations == 0 {
        crate::vlog!(
            "[PROBE-B] ✓ {} respected phase contract (no entry_point writes)",
            pass
        );
    }
    debug_assert!(
        violations == 0,
        "[PROBE-B] {} violated Plan B phase contract: {} entry_point write(s)",
        pass,
        violations
    );
}

// ============================================================================
// NaN guard — 根因守卫 + 哨兵
// ============================================================================

const MIN_BOX_W: f64 = 24.0;
const MIN_BOX_H: f64 = 24.0;
const SIZE_EPS: f64 = 1e-6;

/// Threshold for long power/ground stub (same as special::LONG_PG_STUB).
const LONG_PG_STUB: f64 = 120.0;

/// 根因守卫：退化盒子兜底到最小尺寸，切断 NaN 传播链。
/// 必须在 release 也跑。
pub fn floor_box_sizes(graph: &mut McVecGraph) {
    let mut fixed = 0usize;
    for b in &mut graph.boxes {
        if !b.w.is_finite() || b.w <= SIZE_EPS {
            b.w = MIN_BOX_W;
            fixed += 1;
        }
        if !b.h.is_finite() || b.h <= SIZE_EPS {
            b.h = MIN_BOX_H;
            fixed += 1;
        }
        if !b.x.is_finite() {
            b.x = 0.0;
        }
        if !b.y.is_finite() {
            b.y = 0.0;
        }
    }
    if fixed > 0 {
        crate::vlog!(
            "[layout::flow] floor_box_sizes: repaired {} degenerate dimension(s)",
            fixed
        );
    }
}

/// 哨兵：逐层报告退化盒子与 NaN/Inf 的 entry_point offset。
fn probe_degenerate_boxes(graph: &McVecGraph, tag: &str) {
    if !crate::viz::debug::dump_enabled() {
        return;
    }
    let mut bad_size: Vec<String> = Vec::new();
    let mut bad_pos: Vec<String> = Vec::new();
    let mut bad_off: Vec<String> = Vec::new();
    for b in &graph.boxes {
        if !b.w.is_finite() || b.w <= SIZE_EPS || !b.h.is_finite() || b.h <= SIZE_EPS {
            bad_size.push(format!("{}(w={:.1},h={:.1})", b.name, b.w, b.h));
        }
        if !b.x.is_finite() || !b.y.is_finite() {
            bad_pos.push(format!("{}(x={:.1},y={:.1})", b.name, b.x, b.y));
        }
        if b.x.abs() > 1e7 || b.y.abs() > 1e7 {
            bad_pos.push(format!("{}(x={:.0},y={:.0} 荒诞)", b.name, b.x, b.y));
        }
        for ep in &b.entry_points {
            if !ep.offset.is_finite() {
                bad_off.push(format!("{}#pin{}", b.name, ep.pin_id));
            }
        }
    }
    if bad_size.is_empty() && bad_pos.is_empty() && bad_off.is_empty() {
        crate::vlog!("[PROBE-NAN] layer '{}' {}: clean", graph.name, tag);
    } else {
        crate::vlog!(
            "[PROBE-NAN] layer '{}' {}: bad_size={:?} bad_pos={:?} bad_offset={:?}",
            graph.name,
            tag,
            bad_size,
            bad_pos,
            bad_off
        );
    }
}

// ============================================================================
// Flag ejection — 把压到别的盒子上的 flag 推出去
// ============================================================================

const FLAG_EJECT_GAP: f64 = 6.0;

/// 把压到别的盒子上的 flag 推出去。只移动 flag；非 flag 盒子作为固定障碍。
pub fn eject_flags_from_boxes(graph: &mut McVecGraph) {
    let flag_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();
    if flag_ids.is_empty() {
        return;
    }

    let mut moved = 0usize;
    for fid in flag_ids {
        for _ in 0..8 {
            let (fx, fy, fw, fh) = match graph.boxes.iter().find(|b| b.id == fid) {
                Some(f) => (f.x, f.y, f.w, f.h),
                None => break,
            };
            let (fcx, fcy) = (fx + fw / 2.0, fy + fh / 2.0);

            let mut best: Option<(f64, f64, f64)> = None;
            for o in &graph.boxes {
                if o.id == fid {
                    continue;
                }
                let ix = (fx + fw).min(o.x + o.w) - fx.max(o.x);
                let iy = (fy + fh).min(o.y + o.h) - fy.max(o.y);
                if ix > -FLAG_EJECT_GAP && iy > -FLAG_EJECT_GAP {
                    let cost_x = ix + FLAG_EJECT_GAP;
                    let cost_y = iy + FLAG_EJECT_GAP;
                    let (ocx, ocy) = (o.x + o.w / 2.0, o.y + o.h / 2.0);
                    let (cost, px, py) = if cost_x <= cost_y {
                        let dir = if fcx < ocx { -1.0 } else { 1.0 };
                        (cost_x, dir * cost_x, 0.0)
                    } else {
                        let dir = if fcy < ocy { -1.0 } else { 1.0 };
                        (cost_y, 0.0, dir * cost_y)
                    };
                    if best.map_or(true, |(c, _, _)| cost > c) {
                        best = Some((cost, px, py));
                    }
                }
            }

            match best {
                Some((_, px, py)) => {
                    if let Some(f) = graph.boxes.iter_mut().find(|b| b.id == fid) {
                        f.x += px;
                        f.y += py;
                    }
                    moved += 1;
                }
                None => break,
            }
        }
    }

    if moved > 0 {
        crate::vlog!(
            "[layout::flow] eject_flags_from_boxes: nudged flag(s) {} time(s)",
            moved
        );
    }
}
