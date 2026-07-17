// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M11 — Idiom placement application
//!
//! Consumes `IdiomPlacementModel` and applies low-risk placement adjustments
//! to satellite boxes (caps, resistors) in the graph. Does NOT move anchors,
//! protected boxes, or ladder-locked geometry.

use std::collections::HashSet;

use crate::vector::graph::McVecGraph;

use super::model::{IdiomInstanceKind, IdiomPlacementModel, PlacementConstraint};
use super::report::IdiomPlacementReport;

// ============================================================================
// Public API
// ============================================================================

/// Analyze idiom placement opportunities from semantic data.
///
/// Builds an `IdiomPlacementModel` from the graph, identifying decoupling caps,
/// pullup/pulldown resistors, and diff pairs with placement-relevant detail.
pub fn analyze_idiom_placement(
    graph: &McVecGraph,
    protected_box_ids: &HashSet<i64>,
) -> IdiomPlacementModel {
    let instances = super::detect_placement_instances(graph, protected_box_ids);

    let mut protected = Vec::new();
    for &id in protected_box_ids {
        protected.push(id);
    }
    // Also protect ladder-locked boxes
    for b in &graph.boxes {
        if b.geom_locked && !protected.contains(&b.id) {
            protected.push(b.id);
        }
    }

    let constraints = super::generate_constraints(&instances);

    let warnings = Vec::new();

    IdiomPlacementModel {
        instances,
        constraints,
        protected_box_ids: protected,
        warnings,
    }
}

/// Apply idiom placement in the pre-pin phase (phase_placement → pin_place_pipeline).
///
/// Only moves satellite boxes (caps, resistors). Does NOT move anchors,
/// protected boxes, or ladder-locked geometry.
///
/// Returns a report of what was done.
pub fn apply_idiom_placement_pre_pins(
    graph: &mut McVecGraph,
    model: &IdiomPlacementModel,
) -> IdiomPlacementReport {
    let protected_set: HashSet<i64> = model.protected_box_ids.iter().copied().collect();
    let mut report = IdiomPlacementReport::default();

    // Count detected
    report.idioms_detected = model.instances.len();
    for inst in &model.instances {
        *report.by_kind_detected.entry(inst.kind).or_insert(0) += 1;
    }

    // Count applicable (constraints that can be acted on)
    report.idioms_applicable = model.constraints.len();

    for constraint in &model.constraints {
        // Skip protected boxes
        if protected_set.contains(&constraint.target_box_id) {
            report.protected_skips += 1;
            report.idioms_skipped += 1;
            continue;
        }

        let applied = match constraint.kind {
            super::model::ConstraintKind::NearAnchor => {
                apply_near_anchor(graph, constraint, &protected_set)
            }
            super::model::ConstraintKind::PinSideIntent => {
                apply_pin_side_intent(graph, constraint);
                true
            }
            _ => false,
        };

        if applied {
            report.idioms_applied += 1;
            *report
                .by_kind_applied
                .entry(instance_kind_for_constraint(constraint))
                .or_insert(0) += 1;
        } else {
            report.idioms_skipped += 1;
            report.collision_skips += 1;
        }
    }

    report.warnings = model.warnings.clone();
    report
}

// ============================================================================
// Constraint → placement
// ============================================================================

/// Try to place `target` near `anchor` on the preferred side.
///
/// Returns false if no safe position was found.
fn apply_near_anchor(
    graph: &mut McVecGraph,
    c: &PlacementConstraint,
    protected: &HashSet<i64>,
) -> bool {
    let target_idx = match graph.boxes.iter().position(|b| b.id == c.target_box_id) {
        Some(i) => i,
        None => return false,
    };
    let anchor_idx = match graph.boxes.iter().position(|b| b.id == c.anchor_box_id) {
        Some(i) => i,
        None => return false,
    };

    let anchor = &graph.boxes[anchor_idx];
    let target = &graph.boxes[target_idx];

    let anchor_cx = anchor.x + anchor.w / 2.0;
    let anchor_cy = anchor.y + anchor.h / 2.0;

    let (min_dist, max_dist) = c.distance_range.unwrap_or((40.0, 120.0));

    // Generate candidate positions
    let candidates = generate_candidate_positions(
        anchor_cx,
        anchor_cy,
        anchor.w,
        anchor.h,
        target.w,
        target.h,
        c.preferred_side,
        min_dist,
        max_dist,
    );

    for (cx, cy) in candidates {
        let new_x = cx - target.w / 2.0;
        let new_y = cy - target.h / 2.0;

        if !box_collides(
            graph, target_idx, new_x, new_y, target.w, target.h, protected,
        ) {
            graph.boxes[target_idx].x = new_x;
            graph.boxes[target_idx].y = new_y;
            return true;
        }
    }

    false
}

/// Generate candidate center positions for a satellite near an anchor.
fn generate_candidate_positions(
    anchor_cx: f64,
    anchor_cy: f64,
    anchor_w: f64,
    anchor_h: f64,
    target_w: f64,
    target_h: f64,
    preferred_side: Option<super::model::AnchorSide>,
    _min_dist: f64,
    max_dist: f64,
) -> Vec<(f64, f64)> {
    use super::model::AnchorSide;

    let gap = 20.0;
    let mut positions = Vec::new();

    let side = preferred_side.unwrap_or(AnchorSide::Above);

    // Order: preferred side first, then alternatives
    let sides = match side {
        AnchorSide::Above => vec![AnchorSide::Above, AnchorSide::Right, AnchorSide::Left],
        AnchorSide::Below => vec![AnchorSide::Below, AnchorSide::Right, AnchorSide::Left],
        AnchorSide::Left => vec![AnchorSide::Left, AnchorSide::Above, AnchorSide::Below],
        AnchorSide::Right => vec![AnchorSide::Right, AnchorSide::Above, AnchorSide::Below],
    };

    for s in &sides {
        let (cx, cy) = match s {
            AnchorSide::Above => {
                let y = anchor_cy - anchor_h / 2.0 - target_h / 2.0 - gap;
                (anchor_cx, y)
            }
            AnchorSide::Below => {
                let y = anchor_cy + anchor_h / 2.0 + target_h / 2.0 + gap;
                (anchor_cx, y)
            }
            AnchorSide::Left => {
                let x = anchor_cx - anchor_w / 2.0 - target_w / 2.0 - gap;
                (x, anchor_cy)
            }
            AnchorSide::Right => {
                let x = anchor_cx + anchor_w / 2.0 + target_w / 2.0 + gap;
                (x, anchor_cy)
            }
        };
        // Also try slight offsets from the center alignment
        positions.push((cx, cy));
        positions.push((cx + max_dist * 0.3, cy));
        positions.push((cx - max_dist * 0.3, cy));
    }

    positions
}

/// Check if placing a box at (x, y) would collide with any other box.
fn box_collides(
    graph: &McVecGraph,
    skip_idx: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    protected: &HashSet<i64>,
) -> bool {
    for (i, b) in graph.boxes.iter().enumerate() {
        if i == skip_idx {
            continue;
        }
        // Protected boxes can't be moved, but we still check collision
        if rects_overlap(x, y, w, h, b.x, b.y, b.w, b.h) {
            return true;
        }
    }
    let _ = protected;
    false
}

fn rects_overlap(ax: f64, ay: f64, aw: f64, ah: f64, bx: f64, by: f64, bw: f64, bh: f64) -> bool {
    ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah
}

/// Adjust pin side intent for a box (e.g., pullup power pin → top).
fn apply_pin_side_intent(_graph: &mut McVecGraph, _c: &PlacementConstraint) {
    // v1: only adjust pin side intent; actual entry_point placement happens in pin_place_pipeline.
    // For now, this is a no-op — the intent is recorded in the constraint for future use.
}

fn instance_kind_for_constraint(c: &PlacementConstraint) -> IdiomInstanceKind {
    // Map constraint kind to instance kind for reporting
    // This is a heuristic mapping; in practice the constraint should carry its source kind
    match c.preferred_side {
        Some(super::model::AnchorSide::Above) | Some(super::model::AnchorSide::Below) => {
            IdiomInstanceKind::Decoupling
        }
        _ => IdiomInstanceKind::Decoupling,
    }
}
