// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Plan — the sole contract between the searcher and the geometry layer
//!
//! `Plan` is the searcher's entire output and the geometry layer's entire input.
//! Read-only once produced; `geom::apply` is the only function allowed to write coordinates.

use crate::vector::graph::boxdef::McVecBox;

// ============================================================================
// Zone plans
// ============================================================================

/// Paper position plan of a single zone
#[derive(Debug, Clone)]
pub struct ZonePlan {
    /// Zone index (corresponding to the zone id in ZoneTree)
    pub zone: usize,
    /// Box ids of this zone
    pub box_ids: Vec<i64>,
    /// Paper rect (x, y, w, h)
    pub rect: Rect,
    /// Title anchor position
    pub title_anchor: Point,
    /// Title text
    pub title: String,
}

/// Paper rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 2D point
#[derive(Debug, Clone, Copy, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// ============================================================================
// Cut-set decisions
// ============================================================================

/// Cut-set decision of an edge: wire or label
#[derive(Debug, Clone)]
pub struct CutDecision {
    /// Endpoint pair (box_a, box_b) or (box_id, port_id)
    pub edge: (i64, i64),
    /// true = wire (draw a line), false = label (draw a label)
    pub is_wire: bool,
}

// ============================================================================
// Layered arrangement
// ============================================================================

/// Layered arrangement inside a single zone (filled by M3)
#[derive(Debug, Clone, Default)]
pub struct Arrangement {
    /// Owning zone id
    pub zone: usize,
    /// Layer → box ids in that layer (left to right)
    pub layers: Vec<Vec<i64>>,
}

// ============================================================================
// Plan
// ============================================================================

/// Layout plan: the searcher's entire output, the geometry layer's entire input.
///
/// Read-only once produced; [`super::geom::apply`] is the only function allowed
/// to write coordinates.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Partitions and their paper positions
    pub zones: Vec<ZonePlan>,
    /// Which edges go label (filled by M4)
    pub cuts: Vec<CutDecision>,
    /// Layering inside each zone (filled by M3)
    pub arrangements: Vec<Arrangement>,
    /// Canvas size
    pub canvas: (f64, f64),
}

impl Plan {
    /// Create a trivial Plan: all nodes degenerate into one zone, arrangement empty for now.
    pub fn trivial(boxes: &[McVecBox]) -> Self {
        let canvas = (800.0, 600.0);
        Self {
            zones: vec![ZonePlan {
                zone: 0,
                box_ids: Vec::new(),
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: canvas.0,
                    h: canvas.1,
                },
                title_anchor: Point { x: 0.0, y: 0.0 },
                title: String::new(),
            }],
            cuts: Vec::new(),
            arrangements: Vec::new(),
            canvas,
        }
    }
}