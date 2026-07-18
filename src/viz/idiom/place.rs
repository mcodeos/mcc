// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M11+M12 — Idiom placement application with deterministic scoring
//!
//! Consumes `IdiomPlacementModel` and applies low-risk placement adjustments
//! to satellite boxes (caps, resistors) in the graph. Does NOT move anchors,
//! protected boxes, or ladder-locked geometry.
//!
//! M12 upgrade: score-all candidates → deterministic best. No first-fit-wins.

use std::collections::HashSet;

use crate::vector::graph::McVecGraph;
use crate::viz::stability::key::StableDecisionKey;
use crate::viz::stability::score::{self, DeterministicScore, PlacementCandidate};

use super::model::{AnchorSide, IdiomPlacementModel, PlacementConstraint, PlacementDecisionRecord};
use super::report::{IdiomPlacementReport, IdiomPlacementSkipReason};

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
/// M12: Uses score-all → deterministic best instead of first-fit-wins.
/// Returns a report of what was done, including selected candidates for determinism tracking.
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

    for (constraint_idx, constraint) in model.constraints.iter().enumerate() {
        // Skip protected boxes
        if protected_set.contains(&constraint.target_box_id) {
            report.protected_skips += 1;
            report.idioms_skipped += 1;
            *report
                .skip_reasons
                .entry(IdiomPlacementSkipReason::Protected)
                .or_insert(0) += 1;
            continue;
        }

        match constraint.kind {
            super::model::ConstraintKind::NearAnchor => {
                apply_near_anchor_scored(
                    graph,
                    constraint,
                    constraint_idx,
                    &protected_set,
                    &mut report,
                );
            }
            super::model::ConstraintKind::PinSideIntent => {
                apply_pin_side_intent(graph, constraint);
                report.idioms_applied += 1;
                *report
                    .by_kind_applied
                    .entry(constraint.source_kind)
                    .or_insert(0) += 1;
            }
            _ => {
                report.idioms_skipped += 1;
                *report
                    .skip_reasons
                    .entry(IdiomPlacementSkipReason::NoConstraint)
                    .or_insert(0) += 1;
            }
        }
    }

    report.warnings = model.warnings.clone();
    report
}

// ============================================================================
// M12: Score-all candidate placement
// ============================================================================

/// Score all candidate positions and select the deterministic best safe one.
fn apply_near_anchor_scored(
    graph: &mut McVecGraph,
    c: &PlacementConstraint,
    constraint_idx: usize,
    protected: &HashSet<i64>,
    report: &mut IdiomPlacementReport,
) {
    let target_idx = match graph.boxes.iter().position(|b| b.id == c.target_box_id) {
        Some(i) => i,
        None => {
            report.idioms_skipped += 1;
            *report
                .skip_reasons
                .entry(IdiomPlacementSkipReason::TargetMissing)
                .or_insert(0) += 1;
            return;
        }
    };
    let anchor_idx = match graph.boxes.iter().position(|b| b.id == c.anchor_box_id) {
        Some(i) => i,
        None => {
            report.idioms_skipped += 1;
            *report
                .skip_reasons
                .entry(IdiomPlacementSkipReason::AnchorMissing)
                .or_insert(0) += 1;
            return;
        }
    };

    let anchor = &graph.boxes[anchor_idx];
    let target = &graph.boxes[target_idx];
    let anchor_cx = anchor.x + anchor.w / 2.0;
    let anchor_cy = anchor.y + anchor.h / 2.0;
    let (min_dist, max_dist) = c.distance_range.unwrap_or((40.0, 120.0));

    // Generate all candidate positions with prescribed side order
    let candidates = generate_scored_candidates(
        c,
        constraint_idx,
        anchor_cx,
        anchor_cy,
        anchor.w,
        anchor.h,
        target.w,
        target.h,
        min_dist,
        max_dist,
        graph,
        target_idx,
        protected,
    );

    report.candidate_count += candidates.len();

    // Choose the deterministic best safe candidate
    let best = candidates
        .iter()
        .filter(|cand| cand.score.is_safe())
        .min_by(|a, b| a.score.cmp(&b.score));

    if let Some(best_cand) = best {
        let new_x = best_cand.x - target.w / 2.0;
        let new_y = best_cand.y - target.h / 2.0;
        graph.boxes[target_idx].x = new_x;
        graph.boxes[target_idx].y = new_y;
        report.idioms_applied += 1;
        *report.by_kind_applied.entry(c.source_kind).or_insert(0) += 1;

        // Record selected candidate for determinism tracking
        report.selected_candidates.push(PlacementDecisionRecord {
            source_kind: c.source_kind,
            target_box_id: c.target_box_id,
            anchor_box_id: c.anchor_box_id,
            candidate_index: best_cand.candidate_index,
            score_hash: format!("{:?}", best_cand.score),
        });
    } else {
        report.idioms_skipped += 1;
        report.collision_skips += 1;
        *report
            .skip_reasons
            .entry(IdiomPlacementSkipReason::AllCandidatesCollide)
            .or_insert(0) += 1;
    }
}

/// Generate all candidate positions with deterministic ordering and scoring.
fn generate_scored_candidates(
    c: &PlacementConstraint,
    constraint_idx: usize,
    anchor_cx: f64,
    anchor_cy: f64,
    anchor_w: f64,
    anchor_h: f64,
    target_w: f64,
    target_h: f64,
    _min_dist: f64,
    max_dist: f64,
    graph: &McVecGraph,
    target_idx: usize,
    protected: &HashSet<i64>,
) -> Vec<PlacementCandidate> {
    let gap = 20.0;
    let mut candidates = Vec::new();

    let side = c.preferred_side.unwrap_or(AnchorSide::Above);

    // M12: Prescribed side order for deterministic candidate generation
    let sides = match side {
        AnchorSide::Above => vec![
            AnchorSide::Above,
            AnchorSide::Right,
            AnchorSide::Left,
            AnchorSide::Below,
        ],
        AnchorSide::Below => vec![
            AnchorSide::Below,
            AnchorSide::Right,
            AnchorSide::Left,
            AnchorSide::Above,
        ],
        AnchorSide::Left => vec![
            AnchorSide::Left,
            AnchorSide::Above,
            AnchorSide::Below,
            AnchorSide::Right,
        ],
        AnchorSide::Right => vec![
            AnchorSide::Right,
            AnchorSide::Above,
            AnchorSide::Below,
            AnchorSide::Left,
        ],
    };

    // M12: Fixed offset sequence for deterministic ordering
    let offsets = [0.0, 1.0, -1.0, 2.0, -2.0];

    let mut candidate_idx = 0;
    for (side_idx, s) in sides.iter().enumerate() {
        let (base_cx, base_cy) = match s {
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

        // Generate offset positions
        for &offset_factor in &offsets {
            let (cx, cy) = if offset_factor == 0.0 {
                (base_cx, base_cy)
            } else if matches!(s, AnchorSide::Above | AnchorSide::Below) {
                (base_cx + offset_factor * max_dist * 0.3, base_cy)
            } else {
                (base_cx, base_cy + offset_factor * max_dist * 0.3)
            };

            let (safe, _collision) =
                check_candidate(graph, target_idx, cx, cy, target_w, target_h, protected);

            // Compute distance to ideal anchor side position
            let dist = score::quantized_manhattan(cx, cy, base_cx, base_cy);

            let side_pen = score::side_penalty(side_idx);

            let canvas_pen = if cx - target_w / 2.0 < 0.0 || cy - target_h / 2.0 < 0.0 {
                1000
            } else {
                0
            };

            let score = if !safe {
                DeterministicScore::collision(StableDecisionKey::new(
                    0, // phase_rank
                    0, // decision_kind_rank
                    c.priority as i32,
                    c.target_box_id,
                    c.anchor_box_id,
                    0,
                    0,
                    candidate_idx,
                ))
            } else {
                DeterministicScore::zero(StableDecisionKey::new(
                    0,
                    0,
                    c.priority as i32,
                    c.target_box_id,
                    c.anchor_box_id,
                    0,
                    0,
                    candidate_idx,
                ))
                .with_distance(dist as i32)
                .with_side(side_pen)
                .with_canvas(canvas_pen)
            };

            candidates.push(PlacementCandidate::new(
                c.target_box_id,
                c.anchor_box_id,
                side_idx,
                candidate_idx,
                cx,
                cy,
                score,
            ));

            candidate_idx += 1;
            let _ = constraint_idx;
        }
    }

    candidates
}

/// Check if a candidate position is safe (no collision).
fn check_candidate(
    graph: &McVecGraph,
    skip_idx: usize,
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    protected: &HashSet<i64>,
) -> (bool, bool) {
    let x = cx - w / 2.0;
    let y = cy - h / 2.0;
    let collides = box_collides(graph, skip_idx, x, y, w, h, protected);
    (!collides, collides)
}

fn box_collides(
    graph: &McVecGraph,
    skip_idx: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    _protected: &HashSet<i64>,
) -> bool {
    for (i, b) in graph.boxes.iter().enumerate() {
        if i == skip_idx {
            continue;
        }
        if rects_overlap(x, y, w, h, b.x, b.y, b.w, b.h) {
            return true;
        }
    }
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
