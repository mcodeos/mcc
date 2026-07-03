// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Coordinate normalization: shift all boxes so minimum coordinates ≥ margin
//!
//! Called at the last step of the layout main flow, ensuring SVG viewBox starts from (0,0).

use crate::vector::graph::McVecGraph;

/// Margin (left / top)
pub const CANVAS_MARGIN: f64 = 30.0;
/// Extra padding for bottom-right of canvas
pub const CANVAS_PADDING: f64 = 60.0;

/// Shift all coordinates so `min_x >= MARGIN` and `min_y >= MARGIN`
pub fn normalize_positions(graph: &mut McVecGraph) {
    if graph.boxes.is_empty() {
        return;
    }

    let min_x = graph.boxes.iter().map(|b| b.x).fold(f64::MAX, f64::min);
    let min_y = graph.boxes.iter().map(|b| b.y).fold(f64::MAX, f64::min);

    let shift_x = if min_x < CANVAS_MARGIN {
        CANVAS_MARGIN - min_x
    } else {
        0.0
    };
    let shift_y = if min_y < CANVAS_MARGIN {
        CANVAS_MARGIN - min_y
    } else {
        0.0
    };

    if shift_x > 0.0 || shift_y > 0.0 {
        for b in &mut graph.boxes {
            b.x += shift_x;
            b.y += shift_y;
        }
    }
}

/// Re-run position normalization (public alias used by the render pipeline after
/// post-layout passes such as `place_series_passives`, which may push a box to a
/// negative coordinate). Idempotent: a no-op when everything is already ≥ margin.
pub fn renormalize(graph: &mut McVecGraph) {
    normalize_positions(graph);
}

/// Compute normalized canvas size `(width, height)`
///
/// Tolerant of negative minimums: even if a post-layout pass left a box at a negative
/// coordinate (before `renormalize` runs, or in an unnormalized graph), the canvas still
/// covers the full bounding box instead of clipping content off the top-left.
pub fn compute_canvas(graph: &McVecGraph) -> (f64, f64) {
    if graph.boxes.is_empty() {
        return (200.0, 100.0);
    }
    let min_x = graph
        .boxes
        .iter()
        .map(|b| b.x)
        .fold(f64::MAX, f64::min)
        .min(0.0);
    let min_y = graph
        .boxes
        .iter()
        .map(|b| b.y)
        .fold(f64::MAX, f64::min)
        .min(0.0);
    let max_x = graph.boxes.iter().map(|b| b.x + b.w).fold(0.0f64, f64::max);
    let max_y = graph.boxes.iter().map(|b| b.y + b.h).fold(0.0f64, f64::max);
    (
        (max_x - min_x) + CANVAS_PADDING,
        (max_y - min_y) + CANVAS_PADDING,
    )
}
