// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Box size calculation
//!
//! Different `BoxKind` use different minimum sizes; name / pin count will stretch some dimensions.
//! Called by layout before computing coordinates, sets `box.w` / `box.h`.
//!
//! ## ★ P05 (S3) Changes
//! - `MultiPin` (IC) now shows each pin name (`VCC`/`RX`/`TX`...), box needs to be larger
//! - `TwoPin` (R/C/L/D) now renders with real symbols (zigzag/parallel lines/semicircle/triangle), box is flatter and wider than before
//! - Increased default width to make room for pin labels
//!
//! ## ★ P06 (S5) Changes
//! Added [`recompute_sizes_with_pin_count`]: after the second-round refinement reassigns pins to sides,
//! the pin count on one side may grow (Generic pins switch from Left/Right to Top/Bottom, etc.),
//! this function detects the change and updates box sizes according to the new pin distribution,
//! **keeping the center fixed** to avoid large displacement.

use crate::vector::graph::{BoxKind, EntrySide, McVecBox, McVecGraph};

// ============================================================================
// Public constants (shared by components / chain / radial)
// ============================================================================

/// Minimum spacing between boxes (overlap check + chain gap baseline)
pub const MIN_GAP: f64 = 40.0;

// ============================================================================
// Main API
// ============================================================================

/// Compute (width, height) based on box kind + name + pin count
///
/// ## ★ P05 Changes
/// - TwoPin: 90×44 → **110×40** (flat-wide, accommodate zigzag/parallel lines + top/bottom labels)
/// - MultiPin: compute height by `entry_points` per-side pin count, increase inner padding for pin name
/// - PowerLabel: 64×32 → **50×40** (triangle arrow + label)
pub fn box_size(b: &McVecBox) -> (f64, f64) {
    match b.kind {
        BoxKind::TwoPin => {
            // P05: widened (leave room for designator above + value below, also let zigzag/parallel lines shine)
            (110.0, 40.0)
        }
        BoxKind::MultiPin => {
            // P05: more precise via entry_points (fallback to old formula when empty)
            let (w, h) = ic_size(b);
            (w, h)
        }
        BoxKind::SubModule => {
            // ★ FIX (subgraph fix · step two): sub-modules also scale by port count,
            // same metric as MultiPin (reuse ic_size: compute height by max single-side pin count in entry_points,
            // compute width by pin name + center name; fallback to pin_count estimate when entry_points is empty).
            // No longer hardcoded 64 height. Name/class name width as floor, height floor 84 (fits class name row).
            let (w_ic, h_ic) = ic_size(b);
            let center_chars = b.name.chars().count().max(b.class_name.chars().count());
            let name_w = 150.0_f64.max(center_chars as f64 * 11.0 + 36.0);
            (w_ic.max(name_w), h_ic.max(84.0))
        }
        BoxKind::PowerLabel => {
            // P05: triangle arrow + text above, needs more height
            (50.0, 40.0)
        }
    }
}

/// IC size calculation (P05)
///
/// Determined by entry_points: N pins on each left/right side → box height ≈ N * 18px + top/bottom margins
/// Widened to accommodate: pin name (left/right ~30px char width each) + middle component name (40px)
fn ic_size(b: &McVecBox) -> (f64, f64) {
    if b.entry_points.is_empty() {
        // No entry_points filled (layout before P06, or 0-pin box)
        // Estimate by pin_count — simultaneously raise the floor for better readability
        let center_chars = b.name.chars().count().max(b.class_name.chars().count());
        let w = 150.0_f64.max(center_chars as f64 * 9.0 + 64.0);
        let h: f64 = 84.0_f64.max(30.0 + (b.pin_count as f64 / 2.0).ceil() * 20.0);
        return (w, h);
    }

    // Count pins on each side
    let left_pins = b
        .entry_points
        .iter()
        .filter(|e| e.side == EntrySide::Left)
        .count();
    let right_pins = b
        .entry_points
        .iter()
        .filter(|e| e.side == EntrySide::Right)
        .count();
    let top_pins = b
        .entry_points
        .iter()
        .filter(|e| e.side == EntrySide::Top)
        .count();
    let bottom_pins = b
        .entry_points
        .iter()
        .filter(|e| e.side == EntrySide::Bottom)
        .count();

    let max_side_v = left_pins.max(right_pins);
    let max_side_h = top_pins.max(bottom_pins);

    // Width: longest pin name on left + middle (component name / class name, take the wider) + longest pin name on right
    let longest_pin_name = b
        .entry_points
        .iter()
        .map(|e| e.pin_name.chars().count())
        .max()
        .unwrap_or(0);
    let pin_name_w = (longest_pin_name as f64) * 7.5;
    // Middle area must accommodate: component name + class name (take the wider) + designator, with enough width
    let center_chars = b.name.chars().count().max(b.class_name.chars().count());
    let center_w = (center_chars as f64) * 8.5 + 40.0;
    let w_from_side_pins = (pin_name_w * 2.0 + center_w).max(150.0);
    let w_from_top_pins = (top_pins.max(bottom_pins) as f64 * 34.0 + 48.0).max(150.0);
    let w = w_from_side_pins.max(w_from_top_pins);

    // Height: max pin count on left/right * spacing, plus top/bottom margins + space for name/class name/designator 3 rows
    // (previously 36 + N*18 too cramped; increased to 52 + N*20, floor 84, so pin names and middle 3 rows are not crowded)
    let h_from_side = 52.0 + max_side_v as f64 * 20.0;
    let h_from_top = 60.0 + max_side_h as f64 * 18.0;
    let h = h_from_side.max(h_from_top).max(84.0);

    (w, h)
}

/// Set `w` / `h` of all boxes to default values
///
/// Any `Layouter` should call this once before computing coordinates, otherwise subsequent overlap checks will fail.
pub fn assign_default_sizes(graph: &mut crate::vector::graph::McVecGraph) {
    for b in &mut graph.boxes {
        let (w, h) = box_size(b);
        b.w = w;
        b.h = h;
    }
}

// ============================================================================
// ★ P06 (S5) recompute_sizes_with_pin_count
// ============================================================================

/// Recompute size after second-round refinement, keeping center fixed
///
/// ## Trigger Scenario
/// `assign_entry_points_refine` reassigns Generic pins to sides (from Left to Top, etc.),
/// pin count on one side grows → box should grow taller/wider accordingly.
///
/// ## Algorithm
/// 1. Call `box_size(b)` for each box, which reads `entry_points` to reflect new pin distribution
/// 2. Compare new and current sizes, update if any dimension changes > 1px
/// 3. **Keep center fixed**: `x += (old_w - new_w) / 2`, `y += (old_h - new_h) / 2`
///    (avoid large box displacement caused by refinement, subsequent overlap fix cost is too high)
///
/// ## Return Value
/// `bool` —— whether any box's size changed. Caller can decide whether to re-run
/// `resolve_overlaps_iterative` (lightweight, usually only a few rounds to converge).
///
/// ## Recursion
/// Does not recurse sub-graphs —— caller decides per sub_graphs as needed.
pub fn recompute_sizes_with_pin_count(graph: &mut McVecGraph) -> bool {
    let mut changed = false;
    let mut delta_count = 0usize;

    for b in &mut graph.boxes {
        // Only recompute for boxes with entry_points (no pin info → no change possible)
        if b.entry_points.is_empty() {
            continue;
        }
        let (nw, nh) = box_size(b);
        if (nw - b.w).abs() > 1.0 || (nh - b.h).abs() > 1.0 {
            // Keep center fixed
            b.x += (b.w - nw) / 2.0;
            b.y += (b.h - nh) / 2.0;
            b.w = nw;
            b.h = nh;
            changed = true;
            delta_count += 1;
        }
    }

    if changed {
        crate::vlog!(
            "[size::recompute] graph '{}' bid={}: {} boxes resized after refine",
            graph.name,
            graph.bid,
            delta_count
        );
    }
    changed
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{EntryPoint, IoSummary};

    fn mk_ic_with_pins(side_counts: [usize; 4]) -> McVecBox {
        // side_counts = [top, right, bottom, left]
        let mut b = McVecBox::new(
            1,
            "U1".into(),
            String::new(),
            BoxKind::MultiPin,
            side_counts.iter().sum(),
            IoSummary::new(),
        );
        b.x = 100.0;
        b.y = 100.0;
        let mut eps = Vec::new();
        let mut pin_id_counter = 1_i64;
        for (i, count) in side_counts.iter().enumerate() {
            let side = match i {
                0 => EntrySide::Top,
                1 => EntrySide::Right,
                2 => EntrySide::Bottom,
                _ => EntrySide::Left,
            };
            for k in 0..*count {
                eps.push(EntryPoint {
                    pin_id: pin_id_counter,
                    pin_name: format!("p{}", pin_id_counter),
                    side: side.clone(),
                    offset: (k as f64 + 0.5) / (*count as f64),
                });
                pin_id_counter += 1;
            }
        }
        b.entry_points = eps;
        b
    }

    #[test]
    fn p06_recompute_returns_false_when_no_change() {
        // box already sized by pin distribution, recompute should not change
        let mut g = McVecGraph::new(0, "test".into());
        let mut b = mk_ic_with_pins([1, 3, 1, 3]);
        let (w, h) = box_size(&b);
        b.w = w;
        b.h = h;
        g.boxes.push(b);

        let changed = recompute_sizes_with_pin_count(&mut g);
        assert!(!changed, "fresh sizes should already match box_size()");
    }

    #[test]
    fn p06_recompute_grows_box_when_pin_count_increases() {
        // initially 1 pin on left/right each → shorter
        // then artificially push more pins to both sides → recompute should make box taller
        let mut g = McVecGraph::new(0, "test".into());
        let mut b = mk_ic_with_pins([1, 1, 1, 1]);
        let (w0, h0) = box_size(&b);
        b.w = w0;
        b.h = h0;
        // Now sneakily add 4 pins to left/right (simulate refine moving Top/Bottom pins over)
        for i in 0..4 {
            b.entry_points.push(EntryPoint {
                pin_id: 100 + i,
                pin_name: format!("extra{}", i),
                side: EntrySide::Left,
                offset: 0.5,
            });
            b.entry_points.push(EntryPoint {
                pin_id: 200 + i,
                pin_name: format!("xtra{}", i),
                side: EntrySide::Right,
                offset: 0.5,
            });
        }
        g.boxes.push(b);

        let changed = recompute_sizes_with_pin_count(&mut g);
        assert!(changed, "adding 8 pins should grow the box");
        let b_ref = &g.boxes[0];
        assert!(
            b_ref.h > h0,
            "height should increase from {} to {}",
            h0,
            b_ref.h
        );
    }

    #[test]
    fn p06_recompute_preserves_center() {
        // box center (cx, cy) should be the same before and after resize
        let mut g = McVecGraph::new(0, "test".into());
        let mut b = mk_ic_with_pins([0, 1, 0, 1]);
        b.w = 100.0;
        b.h = 60.0;
        b.x = 200.0;
        b.y = 150.0;
        // center = (250, 180)
        let old_cx = b.x + b.w / 2.0;
        let old_cy = b.y + b.h / 2.0;
        // Increase pin count to force grow
        for i in 0..6 {
            b.entry_points.push(EntryPoint {
                pin_id: 100 + i,
                pin_name: format!("p{}", i),
                side: EntrySide::Left,
                offset: 0.5,
            });
        }
        g.boxes.push(b);

        recompute_sizes_with_pin_count(&mut g);
        let new_b = &g.boxes[0];
        let new_cx = new_b.x + new_b.w / 2.0;
        let new_cy = new_b.y + new_b.h / 2.0;
        assert!(
            (new_cx - old_cx).abs() < 0.5,
            "center x should be preserved"
        );
        assert!(
            (new_cy - old_cy).abs() < 0.5,
            "center y should be preserved"
        );
    }

    #[test]
    fn p06_recompute_skips_boxes_without_entry_points() {
        // boxes without entry_points should be skipped (won't trigger ic_size fallback path)
        let mut g = McVecGraph::new(0, "test".into());
        let mut b = McVecBox::new(
            1,
            "x".into(),
            String::new(),
            BoxKind::MultiPin,
            5,
            IoSummary::new(),
        );
        b.w = 100.0;
        b.h = 80.0;
        b.x = 0.0;
        b.y = 0.0;
        // intentionally don't fill entry_points
        g.boxes.push(b);

        let changed = recompute_sizes_with_pin_count(&mut g);
        assert!(!changed, "boxes without entry_points should be skipped");
        // original size unchanged
        assert_eq!(g.boxes[0].w, 100.0);
        assert_eq!(g.boxes[0].h, 80.0);
    }
}
