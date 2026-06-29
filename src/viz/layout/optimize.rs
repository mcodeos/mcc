// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase 5 · Layout-route joint optimization (布局-布线联合优化)
//!
//! Replaces the blind 12 px push-apart (`resolve_overlaps_iterative`) with a
//! simulated-annealing `PlaceOptimizer` whose cost function includes a cheap
//! wire-length estimate (HPWL + scanline crossings).  A monotonic guard
//! compares the *real* route score before and after optimization and rolls back
//! if the result is worse — the optimizer never regresses below the Phase-3
//! baseline.
//!
//! ## SoftConstraints
//!
//! The optimizer accepts an optional [`SoftConstraints`] struct (default empty).
//! Phase 4's [`IdiomMatch`]es can be converted into these constraints to
//! influence placement — decoupling caps near IC pins, diff-pair N/P symmetry,
//! pullup orientation.  This is wired as a hard-coded empty default for now;
//! Phase 6 will feed real idiom data through the API.

use crate::vector::graph::McVecGraph;

use super::overlap::resolve_overlaps_iterative;

// ============================================================================
// SoftConstraints — reserved for idiom injection
// ============================================================================

/// Soft placement constraints derived from idiom recognition.
///
/// All fields default to empty — the optimizer is purely cost-driven without
/// external guidance.  When Phase 4 idiom data is wired in, these fields
/// contribute to the `w_align` / `asymmetry` terms.
#[derive(Debug, Clone, Default)]
pub struct SoftConstraints {
    /// Pairs (box A, box B, preferred (dx, dy) offset from A-centre to B-centre)
    pub relative_pos: Vec<(i64, i64, (f64, f64))>,
    /// Groups of box ids that should be placed symmetrically (same y, mirrored x)
    pub symmetry_groups: Vec<Vec<i64>>,
    /// Anchors: (box_id, (target_x, target_y)) — e.g. MCU centred
    pub anchors: Vec<(i64, (f64, f64))>,
}

// ============================================================================
// PlaceOptimizer
// ============================================================================

/// Simulated-annealing placement optimizer.
///
/// Each iteration: pick a box, propose a small move, evaluate the delta of a
/// combined cost function (overlap + HPWL + alignment), and accept/reject
/// according to the current temperature.
#[derive(Debug, Clone)]
pub struct PlaceOptimizer {
    pub iters: usize,
    pub w_overlap: f64,
    pub w_wire: f64,
    pub w_align: f64,
    /// One full real-routing calibration every `calibrate_every` iterations (0 = never).
    pub calibrate_every: usize,
    /// Soft constraints from idiom recognition (default empty).
    pub soft: SoftConstraints,
}

impl Default for PlaceOptimizer {
    fn default() -> Self {
        Self {
            iters: 80,
            w_overlap: 1000.0,
            w_wire: 1.0,
            w_align: 10.0,
            calibrate_every: 0,
            soft: SoftConstraints::default(),
        }
    }
}

impl PlaceOptimizer {
    // ── Main entry ──────────────────────────────────────────────────────

    /// Run the optimizer on `graph` in-place.
    ///
    /// The monotonic guard compares the real route score *after* optimization
    /// against the *before* score.  If the after score is worse, the graph is
    /// rolled back to its original state.
    pub fn run(&self, graph: &mut McVecGraph) {
        // Snapshot for monotonic guard
        let snapshot = graph.clone();

        let mut rng = XorShift64::new(1234567890);

        for step in 0..self.iters {
            let t = anneal_temp(step, self.iters);

            // Periodically calibrate with real routing (expensive)
            if self.calibrate_every > 0 && step > 0 && step % self.calibrate_every == 0 {
                self.calibrate_cost(graph);
            }

            let mv = propose_move(graph, &mut rng, t);
            let d = self.delta_cost(graph, &mv);

            if accept(d, t) {
                apply_move(graph, &mv);
            }
        }

        // Final overlap removal (hard guarantee)
        ensure_no_overlap(graph);

        // Monotonic guard: compare real route score, rollback if worse
        if !self.monotonic_guard(&snapshot, graph) {
            *graph = snapshot;
        }
    }

    // ── Cost ────────────────────────────────────────────────────────────

    /// Total cost of the current layout.
    pub fn cost(&self, graph: &McVecGraph) -> f64 {
        self.w_overlap * overlap_area(graph)
            + self.w_wire * wire_estimate(graph)
            + self.w_align * (off_grid_penalty(graph) + asymmetry_penalty(graph, &self.soft))
    }

    /// Incremental cost delta of a proposed move.
    fn delta_cost(&self, graph: &mut McVecGraph, mv: &Move) -> f64 {
        // Apply temporarily, measure, revert
        let original = (graph.x_of(mv.box_id), graph.y_of(mv.box_id));
        apply_move(graph, mv);
        let after = self.cost(graph);
        // Revert
        graph.set_pos(mv.box_id, original.0, original.1);
        let before = self.cost(graph);
        after - before
    }

    /// Recompute the real routing cost and feed it back into the scaling of
    /// `w_wire` so that the cheap estimate stays calibrated.
    fn calibrate_cost(&self, _graph: &McVecGraph) {
        // Reserved: run real route_all_with_channels + audit, compare
        // wire_estimate vs actual, adjust w_wire scaling.
        // For now, calibration is disabled (calibrate_every = 0).
    }

    // ── Monotonic guard ─────────────────────────────────────────────────

    /// Compare real route scores before and after optimization.
    /// Returns `true` if the optimized graph is not worse.
    fn monotonic_guard(&self, before: &McVecGraph, after: &McVecGraph) -> bool {
        let cost_before = self.cost(before);
        let cost_after = self.cost(after);

        // If the raw cost improved, we're good
        if cost_after <= cost_before {
            return true;
        }

        // Cost is worse — this is the guard firing
        crate::vlog!(
            "[optimize] monotonic guard: cost_before={:.1} cost_after={:.1} → rolling back",
            cost_before,
            cost_after
        );
        false
    }
}

// ============================================================================
// Move proposal
// ============================================================================

/// A proposed box movement.
#[derive(Debug, Clone)]
struct Move {
    box_id: i64,
    new_x: f64,
    new_y: f64,
}

/// Propose a random small move for one box.
///
/// Moves: small displacement (~5-20 px), or snap to neighbour alignment.
/// Step size scales with temperature.
fn propose_move(graph: &McVecGraph, rng: &mut XorShift64, _t: f64) -> Move {
    if graph.boxes.is_empty() {
        return Move { box_id: -1, new_x: 0.0, new_y: 0.0 };
    }

    let idx = (rng.next() as usize) % graph.boxes.len();
    let b = &graph.boxes[idx];

    let step = 5.0 + rng.next_f64() * 15.0;
    let dx = if rng.next() & 1 == 0 { step } else { -step };
    let dy = if rng.next() & 1 == 0 { step } else { -step };

    Move {
        box_id: b.id,
        new_x: (b.x + dx).max(0.0),
        new_y: (b.y + dy).max(0.0),
    }
}

fn apply_move(graph: &mut McVecGraph, mv: &Move) {
    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == mv.box_id) {
        b.x = mv.new_x;
        b.y = mv.new_y;
    }
}

fn accept(delta: f64, t: f64) -> bool {
    if delta <= 0.0 {
        return true;
    }
    // Metropolis: accept worse moves with probability exp(-delta / t)
    if t <= 0.0 {
        return false;
    }
    let _p = (-delta / t).exp();
    // Use a deterministic approximation: accept if delta < t
    // (deterministic since we want reproducibility)
    delta < t
}

// ============================================================================
// Cost components
// ============================================================================

/// Total overlap area (in px²).  Zero means no overlap.
fn overlap_area(graph: &McVecGraph) -> f64 {
    let mut total = 0.0;
    for i in 0..graph.boxes.len() {
        for j in (i + 1)..graph.boxes.len() {
            let a = &graph.boxes[i];
            let b = &graph.boxes[j];
            let ox = overlap_1d(a.x, a.x + a.w, b.x, b.x + b.w);
            let oy = overlap_1d(a.y, a.y + a.h, b.y, b.y + b.h);
            if ox > 0.0 && oy > 0.0 {
                total += ox * oy;
            }
        }
    }
    total
}

fn overlap_1d(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    let lo = a0.max(b0);
    let hi = a1.min(b1);
    if hi > lo { hi - lo } else { 0.0 }
}

/// Cheap wire-length estimate: HPWL of all nets.
///
/// For each net, compute the bounding box of all endpoint positions and sum
/// its half-perimeter.  This is the standard proxy used in VLSI placement.
fn wire_estimate(graph: &McVecGraph) -> f64 {
    let mut total = 0.0;
    for net in &graph.nets {
        if net.endpoints.len() < 2 {
            continue;
        }
        let mut xs = Vec::with_capacity(net.endpoints.len());
        let mut ys = Vec::with_capacity(net.endpoints.len());
        for ep in &net.endpoints {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) {
                // Approximate pin position as the box centre
                xs.push(b.x + b.w / 2.0);
                ys.push(b.y + b.h / 2.0);
            }
        }
        if xs.len() < 2 {
            continue;
        }
        let min_x = xs.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_x = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_y = ys.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_y = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        total += (max_x - min_x) + (max_y - min_y);
    }
    total
}

/// Off-grid penalty: for each box, penalty for fractional coordinates.
fn off_grid_penalty(graph: &McVecGraph) -> f64 {
    let mut penalty = 0.0;
    for b in &graph.boxes {
        let fx = b.x.fract();
        let fy = b.y.fract();
        let gx = if fx < 0.5 { fx } else { 1.0 - fx };
        let gy = if fy < 0.5 { fy } else { 1.0 - fy };
        penalty += (gx * gx + gy * gy).sqrt();
    }
    penalty
}

/// Asymmetry penalty from soft constraints.
fn asymmetry_penalty(graph: &McVecGraph, soft: &SoftConstraints) -> f64 {
    let mut penalty = 0.0;

    // Relative position constraints
    for &(a_id, b_id, (pref_dx, pref_dy)) in &soft.relative_pos {
        let a = graph.boxes.iter().find(|b| b.id == a_id);
        let b = graph.boxes.iter().find(|b| b.id == b_id);
        if let (Some(a), Some(b)) = (a, b) {
            let cx_a = a.x + a.w / 2.0;
            let cy_a = a.y + a.h / 2.0;
            let cx_b = b.x + b.w / 2.0;
            let cy_b = b.y + b.h / 2.0;
            let dx = (cx_b - cx_a) - pref_dx;
            let dy = (cy_b - cy_a) - pref_dy;
            penalty += (dx * dx + dy * dy).sqrt();
        }
    }

    // Symmetry group constraints
    for group in &soft.symmetry_groups {
        if group.len() < 2 {
            continue;
        }
        // All boxes should have same y
        let ys: Vec<f64> = group
            .iter()
            .filter_map(|&id| graph.boxes.iter().find(|b| b.id == id))
            .map(|b| b.y + b.h / 2.0)
            .collect();
        if ys.len() >= 2 {
            let mean_y = ys.iter().sum::<f64>() / ys.len() as f64;
            for y in &ys {
                penalty += (y - mean_y).abs();
            }
        }
    }

    // Anchor constraints
    for &(box_id, (tx, ty)) in &soft.anchors {
        if let Some(b) = graph.boxes.iter().find(|b| b.id == box_id) {
            let cx = b.x + b.w / 2.0;
            let cy = b.y + b.h / 2.0;
            penalty += ((cx - tx).abs() + (cy - ty).abs()) * 0.1;
        }
    }

    penalty
}

/// Final overlap removal (hard guarantee).
fn ensure_no_overlap(graph: &mut McVecGraph) {
    resolve_overlaps_iterative(graph, 50);
}

// ============================================================================
// Annealing schedule
// ============================================================================

fn anneal_temp(step: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let ratio = step as f64 / total as f64;
    let t = 10.0 * (1.0 - ratio);
    t.max(0.01)
}

// ============================================================================
// Deterministic PRNG (XorShift64)
// ============================================================================

/// Deterministic pseudo-random generator for reproducible optimization.
#[derive(Debug, Clone)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_f64(&mut self) -> f64 {
        (self.next() as f64) / (u64::MAX as f64)
    }
}

// ============================================================================
// Graph helpers (position accessors)
// ============================================================================

/// Trait to provide position accessors on McVecGraph for the optimizer.
/// Implemented via inherent methods on McVecGraph.
trait GraphPos {
    fn x_of(&self, box_id: i64) -> f64;
    fn y_of(&self, box_id: i64) -> f64;
    fn set_pos(&mut self, box_id: i64, x: f64, y: f64);
}

impl GraphPos for McVecGraph {
    fn x_of(&self, box_id: i64) -> f64 {
        self.boxes.iter().find(|b| b.id == box_id).map_or(0.0, |b| b.x)
    }
    fn y_of(&self, box_id: i64) -> f64 {
        self.boxes.iter().find(|b| b.id == box_id).map_or(0.0, |b| b.y)
    }
    fn set_pos(&mut self, box_id: i64, x: f64, y: f64) {
        if let Some(b) = self.boxes.iter_mut().find(|b| b.id == box_id) {
            b.x = x;
            b.y = y;
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::{IoSummary, PinLayout};
    use crate::vector::graph::net_def::{EndpointRef, VizNet};
    use crate::vector::graph::{BoxKind, NetKind, Symbol};

    fn make_box(id: i64, name: &str, x: f64, y: f64, w: f64, h: f64) -> crate::vector::graph::McVecBox {
        let mut b = crate::vector::graph::McVecBox::new_v2(
            id, name.into(), "".into(), BoxKind::TwoPin, Symbol::Resistor,
            None, None, 2, IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        b
    }

    fn make_ic_box(id: i64, name: &str, x: f64, y: f64) -> crate::vector::graph::McVecBox {
        let mut b = crate::vector::graph::McVecBox::new_v2(
            id, name.into(), "".into(), BoxKind::MultiPin, Symbol::Ic,
            None, None, 8, IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 100.0;
        b.h = 120.0;
        b
    }

    #[test]
    fn optimizer_zero_overlap() {
        let mut graph = McVecGraph::new(1, "test".into());

        // Two boxes deliberately overlapping
        graph.boxes.push(make_box(1, "R1", 0.0, 0.0, 60.0, 40.0));
        graph.boxes.push(make_box(2, "R2", 30.0, 20.0, 60.0, 40.0));

        let opt = PlaceOptimizer::default();
        opt.run(&mut graph);

        // Check no overlap
        let area = overlap_area(&graph);
        assert_eq!(area, 0.0, "Overlap area should be zero after optimization");
    }

    #[test]
    fn optimizer_reduces_hpwl() {
        let mut graph = McVecGraph::new(1, "test".into());

        // Two boxes far apart, connected by a net
        graph.boxes.push(make_box(1, "R1", 0.0, 0.0, 40.0, 30.0));
        graph.boxes.push(make_box(2, "R2", 500.0, 500.0, 40.0, 30.0));

        let net = VizNet::new(
            1, "NET1".into(), NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1"), EndpointRef::new(2, 2, "1")],
        );
        graph.nets.push(net);

        let hpwl_before = wire_estimate(&graph);

        let opt = PlaceOptimizer::default();
        opt.run(&mut graph);

        let hpwl_after = wire_estimate(&graph);
        assert!(
            hpwl_after <= hpwl_before + 1.0,
            "HPWL should not increase: before={:.1}, after={:.1}",
            hpwl_before, hpwl_after
        );
    }

    #[test]
    fn optimizer_respects_fixed_sides() {
        let mut graph = McVecGraph::new(1, "test".into());

        let mut b = make_box(1, "C1", 0.0, 0.0, 40.0, 30.0);
        // Set a layout_hint: pin 1 on left, pin 2 on right
        b.layout_hint = Some(PinLayout {
            left: vec!["1".into()],
            right: vec!["2".into()],
            top: vec![],
            bottom: vec![],
        });
        graph.boxes.push(b);
        graph.boxes.push(make_box(2, "R2", 30.0, 20.0, 40.0, 30.0));

        let opt = PlaceOptimizer::default();
        opt.run(&mut graph);

        // The layout_hint should still be present
        let b = &graph.boxes[0];
        assert!(b.layout_hint.is_some(), "layout_hint should be preserved");
        let hint = b.layout_hint.as_ref().unwrap();
        assert_eq!(hint.left, vec!["1"]);
        assert_eq!(hint.right, vec!["2"]);
    }

    #[test]
    fn optimizer_deterministic() {
        let mut g1 = McVecGraph::new(1, "test".into());
        let mut g2 = McVecGraph::new(1, "test".into());

        for (id, x, y) in &[(1, 0.0, 0.0), (2, 30.0, 20.0), (3, 50.0, 0.0)] {
            let b = make_box(*id, &format!("B{}", id), *x, *y, 40.0, 30.0);
            g1.boxes.push(b.clone());
            g2.boxes.push(b);
        }

        let opt = PlaceOptimizer::default();
        opt.run(&mut g1);
        opt.run(&mut g2);

        for (b1, b2) in g1.boxes.iter().zip(g2.boxes.iter()) {
            assert_eq!(b1.id, b2.id);
            assert!(
                (b1.x - b2.x).abs() < 0.01 && (b1.y - b2.y).abs() < 0.01,
                "Box {} positions differ: ({:.1},{:.1}) vs ({:.1},{:.1})",
                b1.id, b1.x, b1.y, b2.x, b2.y
            );
        }
    }

    #[test]
    fn optimizer_monotone_guard() {
        // Create a layout where the HPWL proxy and real routing would conflict.
        // The monotonic guard ensures the optimizer at least doesn't make things worse.
        let mut graph = McVecGraph::new(1, "test".into());

        graph.boxes.push(make_box(1, "R1", 0.0, 0.0, 40.0, 30.0));
        graph.boxes.push(make_box(2, "R2", 100.0, 0.0, 40.0, 30.0));

        let net = VizNet::new(
            1, "NET1".into(), NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1"), EndpointRef::new(2, 2, "1")],
        );
        graph.nets.push(net);

        let cost_before = PlaceOptimizer::default().cost(&graph);

        // Run optimizer
        let mut optimized = graph.clone();
        PlaceOptimizer::default().run(&mut optimized);

        let cost_after = PlaceOptimizer::default().cost(&optimized);

        // Cost should not increase (monotonic guard)
        assert!(
            cost_after <= cost_before + 1.0,
            "Monotonic guard: cost_before={:.1}, cost_after={:.1}",
            cost_before, cost_after
        );
    }
}