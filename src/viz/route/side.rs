// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `ExitSide` + exit point / direction computation
//!
//! A connection point is essentially "a point on one edge of a box", and the normal
//! direction of the endpoint determines which direction the line "should" first leave
//! the box. This is the pre-data the router uses to output Manhattan paths.
//!
//! ## EntryPoint-priority strategy
//! If the box's `entry_points` field is filled in by layout (added in P1),
//! [`compute_exit_for_pin`] uses the precise position; otherwise falls back to
//! [`compute_exit_to`]'s "guess the nearest edge by relative direction" algorithm
//! (preserves existing behavior).

use crate::vector::graph::{EntrySide, McVecBox};

// ============================================================================
// ExitSide
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl ExitSide {
    pub fn is_horizontal(self) -> bool {
        matches!(self, ExitSide::Left | ExitSide::Right)
    }
}

impl From<EntrySide> for ExitSide {
    fn from(s: EntrySide) -> Self {
        match s {
            EntrySide::Left => ExitSide::Left,
            EntrySide::Right => ExitSide::Right,
            EntrySide::Top => ExitSide::Top,
            EntrySide::Bottom => ExitSide::Bottom,
        }
    }
}

// ============================================================================
// Compute exit point
// ============================================================================

/// Given the from box, compute the exit point + direction towards the to box
///
/// This is the "guess" algorithm preserving old behavior —— pick the nearest edge
/// by the relative angle of the two box centers.
/// Does not depend on EntryPoint data.
pub fn compute_exit_to(from: &McVecBox, to: &McVecBox) -> ((f64, f64), ExitSide) {
    let fcx = from.x + from.w / 2.0;
    let fcy = from.y + from.h / 2.0;
    let tcx = to.x + to.w / 2.0;
    let tcy = to.y + to.h / 2.0;

    let dx = tcx - fcx;
    let dy = tcy - fcy;
    let angle = dy.atan2(dx);
    let pi = std::f64::consts::PI;

    if angle.abs() <= pi / 4.0 {
        ((from.x + from.w, fcy), ExitSide::Right)
    } else if angle > pi / 4.0 && angle <= 3.0 * pi / 4.0 {
        ((fcx, from.y + from.h), ExitSide::Bottom)
    } else if angle < -pi / 4.0 && angle >= -3.0 * pi / 4.0 {
        ((fcx, from.y), ExitSide::Top)
    } else {
        ((from.x, fcy), ExitSide::Left)
    }
}

/// Given box + pin ID, use the precise EntryPoint position to compute exit point + direction
///
/// Falls back to `compute_exit_to(from, hint_target)` on failure (`hint_target` is
/// used to guess direction).
pub fn compute_exit_for_pin(
    from: &McVecBox,
    pin_id: i64,
    hint_target: Option<&McVecBox>,
) -> ((f64, f64), ExitSide) {
    if let Some(ep) = from.find_entry(pin_id) {
        let (px, py) = pin_position_from_entry(from, ep);
        return ((px, py), ExitSide::from(ep.side.clone()));
    }
    // Fallback: no EntryPoint data → guess by direction
    if let Some(target) = hint_target {
        return compute_exit_to(from, target);
    }
    // Truly no clue: default to right exit, at midpoint
    let cy = from.y + from.h / 2.0;
    ((from.x + from.w, cy), ExitSide::Right)
}

/// Translate EntryPoint's (side, offset) into absolute coordinates
fn pin_position_from_entry(b: &McVecBox, ep: &crate::vector::graph::EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Top => (b.x + b.w * ep.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * ep.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * ep.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * ep.offset),
    }
}
