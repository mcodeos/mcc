// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Stage A + B —— Connectivity-driven top-level flow layout engine (FlowLayouter)
//!
//! **Status: default** — the primary layout engine for both top-level and sub-level.
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
use crate::vector::graph::netdef::IoDirection;
use crate::vector::graph::{AnchorHint, BoxKind, EntrySide, McVecBox, McVecGraph, Symbol};

use super::components::{build_adjacency, find_connected_components};
use super::entry_points::{
    assign_entry_points_coarse, enforce_unique_offsets, promote_synthetic_pins, split_shared_pins,
};
use super::ladder_model::LadderModel;
use super::ladder_place::LadderGeometry;
use super::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use super::optimize::PlaceOptimizer;
use super::rails::{classify_rails, is_rail_box};
use super::size::{assign_default_sizes, recompute_sizes_with_pin_count};
use crate::viz::layout_model::SchematicLayoutModel;
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
    /// Phase D — SchematicLayoutModel: unified layout intent for low-risk rules.
    /// When set, FlowLayouter applies connector edge intent, power/ground vertical
    /// region intent, and bus trunk corridor intent after phase_placement.
    pub schematic_model: Option<SchematicLayoutModel>,
    /// ★ P7-0: whether this instance is the sub-level configuration (`sub()`).
    /// Used by the v2 experimental branch to set `graph.is_submodule`, and by
    /// the per-layer startup log to tag `circuit_flow(sub)`.
    pub is_sub_layout: bool,
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
            schematic_model: None,
            is_sub_layout: false,
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
            schematic_model: None,
            is_sub_layout: true,
        }
    }

    /// ★ P7-0: rebuild self with a SchematicLayoutModel attached, **keeping
    /// every parameter of this instance** (sub() stays sub(), default stays default).
    /// Unlike the by-value builder above, this one works through `&self`
    /// (needed by `Layouter::with_model` on a trait object).
    pub fn clone_with_model(&self, model: SchematicLayoutModel) -> Self {
        Self {
            col_pitch: self.col_pitch,
            row_pitch: self.row_pitch,
            flag_gap: self.flag_gap,
            bary_sweeps: self.bary_sweeps,
            hub_min_degree: self.hub_min_degree,
            recompute_sizes: self.recompute_sizes,
            fanout_star: self.fanout_star,
            hub_keep_semantic: self.hub_keep_semantic,
            ladder: None,
            schematic_model: Some(model),
            is_sub_layout: self.is_sub_layout,
        }
    }

    /// Phase D — attach a SchematicLayoutModel for low-risk layout intent consumption.
    pub fn with_schematic_model(mut self, model: SchematicLayoutModel) -> Self {
        self.schematic_model = Some(model);
        self
    }

    /// Phase 1 · Prepare — topology normalization + coarse pins.
    ///
    /// Writes: fanout-related synth/split structures in graph, initial box sizes, coarse entry_points.
    fn phase_prepare(&self, graph: &mut McVecGraph) {
        // ── ★ P7-3: rail triage (R-1/R-2/R-3 + top-level C5), runs first ──────────
        //   Rail nets are replaced here by driver segment edges + pin decorations
        //   (not in boxes); every later pass (coalesce / pin_place / islands /
        //   passive_inline) sees a pure signal graph.
        classify_rails(graph, /*is_top=*/ !self.is_sub_layout);
        // ★ P8-3 R-B: for the main layer, hide Ground nets and decorations.
        // GND exists in the netlist for ERC but is invisible in the main diagram
        // because hbl.mc never explicitly mentions GND.
        filter_ground_nets_for_main(graph);
        // ★ P7-9: pin facade — filter SubModule pins to only those used in nets.
        // Collapses member ports to port groups, removes R-3 (consumer power) pins.
        // Must run after classify_rails (R-3 detection) and before assign_default_sizes
        // (pin count drives box size).
        super::facade::pin_facade(graph);
        // ★ First reduce "one net per connection" into "one net per equipotential point".
        // The entire layout stack (sp_model / ladder_model / chain / trunk_tap) assumes
        // net == node, but the visit.rs builder path does no cross-net merging for
        // anonymous device pins (FIX-B only recognizes InstKind::Pin). Without this
        // step first, SP reports PassiveNetCount{nets:3} on golden.
        super::coalesce::coalesce_equipotential_nets(graph);
        promote_synthetic_pins(graph);
        split_shared_pins(graph);
        assign_default_sizes(graph);
        assign_entry_points_coarse(graph);
    }

    /// Fallback exit A — fully disconnected graph: pin-aware size recompute then grid-fill the canvas.
    fn exit_grid(&self, graph: &mut McVecGraph) -> (f64, f64) {
        assign_default_sizes(graph);
        place_grid(graph);
        enforce_unique_offsets(graph);
        normalize_positions(graph);
        compute_canvas(graph)
    }

    /// Phase 2 · Size — grow-only size adjustment.
    fn phase_size(&self, graph: &mut McVecGraph) {
        if self.recompute_sizes {
            recompute_sizes_with_pin_count(graph);
        }
        size_by_core_fanout(graph);
        floor_box_sizes(graph);
        probe_degenerate_boxes(graph, "after phase_size");
    }

    /// Phase 3 · Placement — writes only box positions (x/y), all before pin_place.
    ///
    /// Returns (root_id, isolated_ids) for later phases.
    fn phase_placement(&self, graph: &mut McVecGraph) -> (i64, HashSet<i64>) {
        // ── ★ P7-7: anchor hinted boxes before main placement ───────────────
        // Boxes with anchor_hint are placed at their host pin's position and
        // locked, so they skip rank/column/park entirely.
        let mut anchored_s3 = 0usize;
        let mut anchored_total = 0usize;
        {
            // Collect anchor hints first (can't borrow graph mutably while iterating)
            let hints: Vec<(i64, AnchorHint)> = graph
                .boxes
                .iter()
                .filter_map(|b| b.anchor_hint.clone().map(|h| (b.id, h)))
                .collect();
            for (box_id, hint) in &hints {
                let host = graph.boxes.iter().find(|b| b.id == hint.host_box);
                let host_pin = host.and_then(|h| h.find_entry(hint.host_pin).cloned());
                if let (Some(host), Some(pin)) = (host, host_pin) {
                    // Compute the pin's absolute position on the host box
                    let pin_x = match pin.side {
                        EntrySide::Left => host.x,
                        EntrySide::Right => host.x + host.w,
                        _ => host.x + host.w * pin.offset,
                    };
                    let pin_y = match pin.side {
                        EntrySide::Top => host.y,
                        EntrySide::Bottom => host.y + host.h,
                        _ => host.y + host.h * pin.offset,
                    };
                    // Place anchored box below the host pin (one grid step down)
                    let grid = self.row_pitch.max(20.0);
                    let target_x = pin_x;
                    let target_y = pin_y + grid;
                    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == *box_id) {
                        b.x = target_x - b.w / 2.0;
                        b.y = target_y;
                        b.geom_locked = true;
                        anchored_total += 1;
                        // S3: |Δx| <= 1 grid step
                        let dx = (b.x + b.w / 2.0 - pin_x).abs();
                        if dx <= grid {
                            anchored_s3 += 1;
                        }
                    }
                }
            }
        }
        crate::vlog!(
            "[layout::rails] P7-7: anchored {} passive(s), S3 |Δx|<=1grid hit {}/{}",
            anchored_total,
            anchored_s3,
            anchored_total,
        );

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
        // ★ P7-3: group_supply_modules (keyword-table supply modules to the bottom row)
        // is deleted —— driver segment edges wire power modules into the main flow;
        // ranking by flow direction suffices (target figure 1 keeps the power chain
        // inside the main diagram).

        (root_id, isolated_ids)
    }

    /// Phase 5 · Post — geometry-preserving box moves, safe after pin_place.
    ///
    /// ★ P7-3: the flag machine (split/place/eject) is deleted —— terminals are no
    /// longer boxes (graph.rail_decorations, discipline 11); no flags to re-home or eject.
    fn phase_post(&self, graph: &mut McVecGraph, isolated_ids: &HashSet<i64>) {
        park_isolated_components(graph, isolated_ids);
        normalize_positions(graph);
    }

    /// Phase D — apply low-risk layout intent from SchematicLayoutModel.
    ///
    /// Called after phase_placement and before pin_place_pipeline.
    /// Only applies safe, non-destructive nudges:
    /// 1. Connector edge intent — nudge connectors toward canvas edges
    /// 2. Power/ground vertical region — ensure power flags above, ground below
    /// 3. Bus trunk corridor — add spacing between bus groups
    /// 4. Label space reservation — increase row_pitch for label-heavy boxes
    fn apply_schematic_model(&self, graph: &mut McVecGraph) {
        let model = match &self.schematic_model {
            Some(m) => m,
            None => return,
        };

        let mut connector_ids: HashSet<i64> = HashSet::new();
        let mut power_box_ids: HashSet<i64> = HashSet::new();
        let mut ground_box_ids: HashSet<i64> = HashSet::new();
        let mut label_heavy_ids: HashSet<i64> = HashSet::new();

        for entry in &model.boxes {
            use crate::viz::layout_model::BoxLayoutRole;
            match entry.role {
                BoxLayoutRole::Connector => {
                    connector_ids.insert(entry.box_id);
                }
                BoxLayoutRole::PowerRail => {
                    power_box_ids.insert(entry.box_id);
                }
                _ => {}
            }
            if entry.label_pressure.needs_designator_space || entry.label_pressure.needs_value_space
            {
                label_heavy_ids.insert(entry.box_id);
            }
        }

        for rail in &model.rail_plan {
            if rail.is_ground {
                ground_box_ids.insert(rail.net_id);
            } else {
                power_box_ids.insert(rail.net_id);
            }
        }

        if connector_ids.is_empty()
            && power_box_ids.is_empty()
            && ground_box_ids.is_empty()
            && label_heavy_ids.is_empty()
        {
            return;
        }

        // Compute current canvas bounds
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;
        for b in &graph.boxes {
            min_x = min_x.min(b.x);
            max_x = max_x.max(b.x + b.w);
            min_y = min_y.min(b.y);
            max_y = max_y.max(b.y + b.h);
        }

        // 1. Connector edge intent: nudge connectors toward nearest canvas edge
        for b in &mut graph.boxes {
            if b.geom_locked {
                continue;
            }
            if connector_ids.contains(&b.id) {
                let cx = b.x + b.w / 2.0;
                let cy = b.y + b.h / 2.0;
                let dist_left = cx - min_x;
                let dist_right = max_x - cx;
                let dist_top = cy - min_y;
                let dist_bottom = max_y - cy;
                let min_h = dist_left.min(dist_right);
                let min_v = dist_top.min(dist_bottom);

                if min_h < min_v {
                    if dist_left < dist_right {
                        b.x = min_x + CANVAS_MARGIN;
                    } else {
                        b.x = max_x - b.w - CANVAS_MARGIN;
                    }
                } else {
                    if dist_top < dist_bottom {
                        b.y = min_y + CANVAS_MARGIN;
                    } else {
                        b.y = max_y - b.h - CANVAS_MARGIN;
                    }
                }
            }
        }

        // 2. Power/ground vertical region: spread power above center, ground below
        let mid_y = (min_y + max_y) / 2.0;
        for b in &mut graph.boxes {
            if b.geom_locked {
                continue;
            }
            // Nudge power-related boxes above midline
            if power_box_ids.contains(&b.id) {
                if b.y + b.h / 2.0 > mid_y {
                    b.y = (min_y + CANVAS_MARGIN).max(b.y - self.row_pitch);
                }
            }
            // Nudge ground-related boxes below midline
            if ground_box_ids.contains(&b.id) {
                if b.y + b.h / 2.0 < mid_y {
                    b.y = (max_y - b.h - CANVAS_MARGIN).min(b.y + self.row_pitch);
                }
            }
        }

        // 3. Label space: add extra vertical gap for label-heavy boxes
        if !label_heavy_ids.is_empty() {
            let extra_gap = 20.0;
            let mut boxes_by_y: Vec<usize> = (0..graph.boxes.len()).collect();
            boxes_by_y.sort_by(|&a, &b| {
                graph.boxes[a]
                    .y
                    .partial_cmp(&graph.boxes[b].y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for w in boxes_by_y.windows(2) {
                let (i, j) = (w[0], w[1]);
                if graph.boxes[i].geom_locked || graph.boxes[j].geom_locked {
                    continue;
                }
                let has_label = label_heavy_ids.contains(&graph.boxes[i].id)
                    || label_heavy_ids.contains(&graph.boxes[j].id);
                if !has_label {
                    continue;
                }
                let gap = graph.boxes[j].y - (graph.boxes[i].y + graph.boxes[i].h);
                if gap < self.row_pitch + extra_gap {
                    let shift = (self.row_pitch + extra_gap - gap) / 2.0;
                    // Shift boxes below this one down
                    for k in j..graph.boxes.len() {
                        if graph.boxes[k].geom_locked {
                            continue;
                        }
                        graph.boxes[k].y += shift;
                    }
                }
            }
        }
    }
}

impl Layouter for FlowLayouter {
    fn name(&self) -> &'static str {
        if self.is_sub_layout {
            "circuit_flow(sub)"
        } else {
            "circuit_flow"
        }
    }

    /// ★ P7-0: model injection keeps THIS instance's parameters alive.
    fn with_model(
        &self,
        model: crate::viz::layout_model::SchematicLayoutModel,
    ) -> Option<Box<dyn Layouter>> {
        Some(Box::new(self.clone_with_model(model)))
    }

    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        // ★ Wire/Label split: store col_pitch so the wire_label_split pass can read it.
        graph.col_pitch = self.col_pitch;

        // ★ P7-0: per-layer baseline log — first reading for P7-1's renderdiff.
        crate::vlog!(
            "[layout] layer '{}' layouter={} col_pitch={} row_pitch={} bary_sweeps={} hub_min_degree={} recompute={}",
            graph.name,
            self.name(),
            self.col_pitch,
            self.row_pitch,
            self.bary_sweeps,
            self.hub_min_degree,
            self.recompute_sizes
        );

        // ★ M2-1: v2 strangler pipeline. MC_LAYOUT_V2=1 takes the new path;
        // the default is the old path, with not one line of the old path changed.
        // ★ P7-0: the only reader of is_submodule is v2; it is set here,
        // api.rs no longer writes this field unconditionally.
        if std::env::var("MC_LAYOUT_V2").as_deref() == Ok("1") {
            graph.is_submodule = self.is_sub_layout;
            let plan = super::v2::solve(graph);
            super::v2::geom::apply(graph, &plan);
            return plan.canvas;
        }

        mcc_dbg!(
            "viz",
            "{}",
            super::chain::extract_signal_chains(graph).dump(graph)
        );

        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        graph.fanout_star = self.fanout_star;

        // ── Phase 1 · Prepare: topology normalization + coarse pins ──
        let g_snap = graph.geom_snapshot();
        self.phase_prepare(graph);
        graph.claim_geom_changes(&g_snap, "1.prepare");

        // Fallback exit A: fully disconnected → grid layout (early exit)
        if is_fully_disconnected(graph) {
            return self.exit_grid(graph);
        }

        // ── Phase 2 · Size: pin-aware sizes + fanout-based height growth ──
        let g_snap = graph.geom_snapshot();
        self.phase_size(graph);
        graph.claim_geom_changes(&g_snap, "2.size");

        // Fallback exit B: single box (early exit)
        if graph.boxes.len() == 1 {
            graph.boxes[0].x = CANVAS_MARGIN;
            graph.boxes[0].y = CANVAS_MARGIN;
            return compute_canvas(graph);
        }

        // ── Phase 3 · Placement (writes only box positions) + PROBE-B contract check ──
        let ep_snap = probe_ep_snapshot(graph);
        let g_snap = graph.geom_snapshot();

        // ★ B2: root layer radial layout — fixed positions by structural role.
        // Sub-layers continue to use the generic flow layout pipeline.
        let (root_id, isolated_ids) = if graph.is_root {
            super::radial::place_radial(graph);
            graph.claim_geom_changes(&g_snap, "3.radial");
            // Root is the hub box; no isolated boxes in radial layout.
            let root = graph
                .boxes
                .iter()
                .max_by(|a, b| {
                    let wa = a.w * a.h;
                    let wb = b.w * b.h;
                    // Prefer MultiPin/SubModule as root
                    let score_a = if matches!(a.kind, BoxKind::MultiPin | BoxKind::SubModule) {
                        wa + 10000.0
                    } else {
                        wa
                    };
                    let score_b = if matches!(b.kind, BoxKind::MultiPin | BoxKind::SubModule) {
                        wb + 10000.0
                    } else {
                        wb
                    };
                    score_a
                        .partial_cmp(&score_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|b| b.id)
                .unwrap_or(graph.boxes[0].id);
            (root, HashSet::new())
        } else {
            let (root_id, isolated_ids) = self.phase_placement(graph);
            graph.claim_geom_changes(&g_snap, "3.placement");
            (root_id, isolated_ids)
        };
        // ★ M15: the PROBE-B contract ("phase_placement writes no entry_point")
        // holds for the flow layout, but the ROOT runs `place_radial` here, and
        // its `setup_facade_entry_points` writes facade entry_points BY DESIGN.
        // The probe only applies to sub-layers, where a write would be a bug.
        if !graph.is_root {
            probe_no_ep_writes("phase_placement", graph, &ep_snap);
        }

        // ── Phase D · SchematicLayoutModel: low-risk layout intent ──
        // ★ B2: skip for root — radial layout already placed all boxes.
        if !graph.is_root {
            let g_snap = graph.geom_snapshot();
            self.apply_schematic_model(graph);
            graph.claim_geom_changes(&g_snap, "4.schematic_model");

            // The old path still runs: fully overridden by ladder_place on model hit, fallback when the model bails
            let g_snap = graph.geom_snapshot();
            super::two_lane_ladder::try_two_lane_ladder(graph);
            graph.claim_geom_changes(&g_snap, "5.two_lane");

            // ── M11+M12 Idiom-aware placement (pre-pin) ──
            {
                let protected: std::collections::HashSet<i64> = graph
                    .boxes
                    .iter()
                    .filter(|b| b.geom_locked)
                    .map(|b| b.id)
                    .collect();
                let model = crate::viz::idiom::place::analyze_idiom_placement(graph, &protected);
                let g_snap = graph.geom_snapshot();
                let report =
                    crate::viz::idiom::place::apply_idiom_placement_pre_pins(graph, &model);
                graph.claim_geom_changes(&g_snap, "6.idiom");
                if report.idioms_detected > 0 {
                    mcc_dbg!("viz", "{}", report.report_line());
                }
                let mut det_report =
                    crate::viz::stability::report::DeterminismReport::from_graph(graph);
                det_report = det_report.with_idiom(
                    &model.instances,
                    &model.constraints,
                    &report.selected_candidates,
                );
                mcc_dbg!("viz", "{}", det_report.report_line());
            }
        }

        // ── Phase 4 · PinPlacement: sole writer of EntryPoint + sole finalizer of hub geometry ──
        // ★ B2: skip for root — radial layout already placed all boxes with geom_locked.
        if !graph.is_root {
            let g_snap = graph.geom_snapshot();
            super::pin_place::pin_place_pipeline(
                graph,
                Some(root_id),
                true,
                self.hub_keep_semantic,
            );
            graph.claim_geom_changes(&g_snap, "7.pin_place");
            probe_degenerate_boxes(graph, "after pin_place");
        }

        // ★ Island dispatcher: skip for root — radial layout already placed all boxes.
        if !graph.is_root {
            let decomp = super::islands::decompose(graph);
            let g_snap = graph.geom_snapshot();
            super::islands::apply_islands(graph, &decomp);
            graph.claim_geom_changes(&g_snap, "8.islands");
        }

        // ── Phase 5 · Post: geometry-preserving moves, safe after pin_place ──
        // ★ B2: skip for root — all boxes are geom_locked.
        if !graph.is_root {
            let g_snap = graph.geom_snapshot();
            self.phase_post(graph, &isolated_ids);
            graph.claim_geom_changes(&g_snap, "9.post");
        }

        compute_canvas(graph)
    }
}

// (★ P7-3 removed: FlagTarget / FlagMeta / split_flags —— flags are no longer boxes,
//  no need to extract before core layout or re-home in the Post phase.)

// ============================================================================
// ★ P8-3 R-B: filter Ground nets for main layer
// ============================================================================

/// For the main layer, hide Ground nets and their decorations.
/// GND exists in the netlist for ERC but is invisible in the main diagram
/// because hbl.mc never explicitly mentions GND.
fn filter_ground_nets_for_main(graph: &mut McVecGraph) {
    if !graph.is_root {
        return;
    }

    let before_nets = graph.nets.len();
    let before_deco = graph.rail_decorations.len();

    // Remove Ground nets
    graph
        .nets
        .retain(|n| !matches!(n.kind, crate::vector::graph::NetKind::Ground));
    // Remove Ground decorations
    graph.rail_decorations.retain(|d| !d.is_ground);

    let after_nets = graph.nets.len();
    let after_deco = graph.rail_decorations.len();

    if before_nets != after_nets || before_deco != after_deco {
        crate::vlog!(
            "[R-B] main: filtered {} Ground net(s) and {} Ground decoration(s) \
             (GND invisible in main per R-B rule)",
            before_nets - after_nets,
            before_deco - after_deco
        );
    }
}

// ============================================================================
// ★ P8-4: main compass layout (obsolete — replaced by radial layout in B2)
// ============================================================================
// Size: height ∝ signal net count (vertical stretch, let parallel wire bundles spread apart)
// ============================================================================

/// Box height scaled by "total pin count" (only increase, never decrease).
///
/// Pin count ≈ connected net count. More connections → taller box, pins on left/right naturally spread out;
/// also ensures boxes like dcdc with "few signals but many power outputs" have enough vertical space to spread flags.
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
    // ★ P7-8: PortTerminal boxes never participate in hub election
    let is_core = |b: &&McVecBox| b.kind != BoxKind::PortTerminal;
    if let Some(b) = graph
        .boxes
        .iter()
        .filter(|b| is_core(b) && naming::is_main_chip(&b.name))
        .next()
    {
        return b.id;
    }
    // Sub-layer anchoring: prefer IC with most pins (top-level module is Module, won't match → behavior unchanged)
    if let Some(b) = graph
        .boxes
        .iter()
        .filter(|b| is_core(b) && matches!(b.symbol, Symbol::Ic))
        .max_by_key(|b| b.pin_count)
    {
        return b.id;
    }
    let src = graph
        .boxes
        .iter()
        .filter(|b| {
            is_core(b)
                && indeg.get(&b.id).copied().unwrap_or(0) == 0
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
        .filter(|b| is_core(b) && !b.is_two_pin_passive())
        .max_by_key(|b| adj.get(&b.id).map(|v| v.len()).unwrap_or(0))
        .map(|b| b.id)
        .unwrap_or(graph.boxes[0].id)
}

/// Signed rank for each core box (negative=left, 0=hub, positive=right)
fn assign_flow_ranks(graph: &McVecGraph, hub_min_degree: usize) -> HashMap<i64, i32> {
    let core_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| !b.geom_locked && b.kind != BoxKind::PortTerminal)
        .map(|b| b.id)
        .collect();
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
/// Example: usbsocket↔ldo only connected via Vin, only power (became flag) between it and main circuit (mcu...) →
/// They are a connected component without hub → all enter isolated set. dcdc if has real connection (like [VCC_1V2,GND]
/// bundle net) to main → in hub component → not in isolated set → stays in main layout.
/// ★ P7-3 acceptance item: this set must be empty for the main layer (driver segment
/// edges wire power modules into the main flow, so no more "power-only" islands).
/// pub for integration tests to assert.
pub fn compute_isolated_ids(graph: &McVecGraph, hub_id: i64) -> HashSet<i64> {
    let adj = build_adjacency(graph);
    let comps = find_connected_components(&graph.boxes, &adj);
    // ★ P7-3: a SubModule with negative id is Phase 1.46's top-level dashed border
    // (netless, purely visual), not a component, and does not enter the isolated set ——
    // otherwise park_isolated_components would move the border out of the canvas body.
    let border: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.id < 0 && b.kind == crate::vector::graph::BoxKind::SubModule)
        .map(|b| b.id)
        .collect();
    // ★ P7-7: boxes with anchor_hint are not isolated — they are placed by the
    // anchor placer in phase_placement, not parked to empty space.
    let anchored: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.anchor_hint.is_some())
        .map(|b| b.id)
        .collect();
    // ★ P7-8: PortTerminal boxes are at the canvas edge, not isolated.
    let port_terminals: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.kind == crate::vector::graph::BoxKind::PortTerminal)
        .map(|b| b.id)
        .collect();
    let mut out = HashSet::new();
    for c in &comps {
        if c.contains(&hub_id) {
            continue;
        }
        for &id in c {
            if border.contains(&id) {
                continue;
            }
            if anchored.contains(&id) {
                continue;
            }
            if port_terminals.contains(&id) {
                continue;
            }
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

    // ★ P7-3: flags no longer exist (terminals are pin decorations); islands only need
    // to rigidly shift the boxes themselves.
    let move_set: HashSet<i64> = isolated_ids.clone();

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

// (★ P7-3 removed: is_supply_module / group_supply_modules ——
//  the name keyword table (POWER/LDO/DCDC/...) was a specimen of anti-pattern §2.3,
//  the whole chain deleted. Driver segment edges already wire power modules into the
//  main flow; ranking by flow direction suffices.)

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

    // (★ P7-3 removed: place_flags —— flags are not boxes and need no placement;
    //  terminals render as pin decorations.)
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
    use crate::vector::graph::NetKind;
    use crate::vector::graph::{BoxKind, EndpointRef, IoSummary, NetRole, Symbol, VizNet};

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
            name.to_string(),
            Vec::new(),
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
            name.to_string(),
            Vec::new(),
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
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "OUT", IoDirection::Output),
                EndpointRef::with_io(2, 21, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(VizNet::new(
            11,
            "b".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(2, 22, "OUT", IoDirection::Output),
                EndpointRef::with_io(3, 31, "IN", IoDirection::Input),
            ],
        ));
        let ranks = assign_flow_ranks(&g, 4);
        assert!(ranks[&1] < ranks[&2]);
        assert!(ranks[&2] < ranks[&3]);
    }
}

// ============================================================================
// PROBE-B — verify the Placement phase doesn't sneak-write EntryPoint
// ----------------------------------------------------------------------------
// Runs only when MC_VIZ_DUMP is enabled; debug_assert panics when new code violates.
// Expected log:
//   [PROBE-B] ✓ phase_placement respected phase contract (no entry_point writes)
// ============================================================================

/// Snapshot (box_id, pin_id) → (side discriminant string, offset).
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
// NaN guard — root-cause guard + sentinel
// ============================================================================

const MIN_BOX_W: f64 = 24.0;
const MIN_BOX_H: f64 = 24.0;
const SIZE_EPS: f64 = 1e-6;

/// Threshold for long power/ground stub (same as special::LONG_PG_STUB).
const LONG_PG_STUB: f64 = 120.0;

/// Root-cause guard: floor degenerate boxes to minimum size, cutting the NaN
/// propagation chain.
/// Must also run in release.
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

/// Sentinel: report degenerate boxes and NaN/Inf entry_point offsets per layer.
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
            bad_pos.push(format!("{}(x={:.0},y={:.0} absurd)", b.name, b.x, b.y));
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
// (★ P7-3 removed: eject_flags_from_boxes —— flags are not boxes, no flags to eject.)
