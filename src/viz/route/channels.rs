// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW (P10, S6) — Channel Routing
//!
//! ## What problem does this file solve
//! P09 (S5) solved "wires don't pass through boxes", but **multiple wires can still
//! overlap each other**:
//! - 3 trunks all picking y_median of stub_ends ≈ y=200 → 3 lines crowd on the same y
//! - 5 lines in a 30px narrow gap between two boxes → visually indistinguishable
//! - Multiple orthogonal turn points at the same x → looks like a black dot blob
//!
//! P10 introduces "channels": horizontal/vertical blank areas between box rows are
//! routing channels, each line **reserves** a slot in the channel, subsequent routing
//! must avoid already-reserved slots.
//!
//! ## Data model
//!
//! ```text
//!  ┌──── HChannel y_top=50, y_bottom=120 ─────┐
//!  │  slot y=70  (net 1) [x: 100..400]         │
//!  │  slot y=85  (net 2) [x: 500..900]         │  ← same channel, different y
//!  │  slot y=85  (net 3) [x: 100..400]         │  ← same y but x doesn't overlap → OK
//!  └──────────────────────────────────────────┘
//!  ┌──── HChannel y_top=300, y_bottom=420 ────┐
//!  │  ...                                       │
//!  └──────────────────────────────────────────┘
//! ```
//!
//! A channel is a horizontal blank band (`y_top` to `y_bottom`) between rows of boxes.
//! Multiple nets in a channel stagger via different `y` slots; multiple nets at the
//! same `y` must have non-overlapping `x` ranges.
//!
//! ## API summary
//!
//! ```ignore
//! // 1. Extract channels from a laid-out graph
//! let mut channels = ChannelMap::build(graph, MIN_GAP);
//!
//! // 2. Reserve position for a horizontal trunk
//! let actual_y = channels.reserve_horizontal(
//!     x_start, x_end,   // trunk span
//!     preferred_y,      // desired y (usually endpoints' y_median)
//!     net.nid,
//!     LINE_GAP,         // minimum gap between slots in same channel
//! ).unwrap_or(preferred_y);  // fall back to preferred when channel is full
//! ```
//!
//! ## Division of labor with ObstacleMap
//! - **ObstacleMap (P09)**: wire vs box conflict
//! - **ChannelMap (P10)**: wire vs wire conflict
//!
//! Both are used together: the router first uses obstacles to pick non-box-colliding
//! candidates, then uses channel reservation to pick a specific y/x that doesn't
//! conflict with reserved slots.
//!
//! ## Known simplifications
//! - Only H/V channels, no grid A* (that's a larger future refactor)
//! - When "channel is full" fall back to preferred y (allow some overlap), no rip-up & reroute
//! - Channel bounds = canvas bounds (not strictly limited to in-row x); can be optimized later

use std::collections::HashMap;

use crate::vector::graph::McVecGraph;

// ============================================================================
// HChannel / HSlot — horizontal channel
// ============================================================================

/// A horizontal channel (blank area between y_top → y_bottom)
#[derive(Debug, Clone)]
pub struct HChannel {
    /// Top y of channel (smaller is upper)
    pub y_top: f64,
    /// Bottom y of channel
    pub y_bottom: f64,
    /// Left x of channel
    pub x_min: f64,
    /// Right x of channel
    pub x_max: f64,
    /// Slots already reserved in the channel
    pub slots: Vec<HSlot>,
}

/// A horizontal slot (a specific horizontal line's y position + x span)
#[derive(Debug, Clone, Copy)]
pub struct HSlot {
    pub y: f64,
    pub x_min: f64,
    pub x_max: f64,
    pub owner_net_id: i64,
}

impl HChannel {
    pub fn height(&self) -> f64 {
        self.y_bottom - self.y_top
    }
    pub fn center_y(&self) -> f64 {
        (self.y_top + self.y_bottom) / 2.0
    }
}

// ============================================================================
// VChannel / VSlot — vertical channel
// ============================================================================

#[derive(Debug, Clone)]
pub struct VChannel {
    pub x_left: f64,
    pub x_right: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub slots: Vec<VSlot>,
}

#[derive(Debug, Clone, Copy)]
pub struct VSlot {
    pub x: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub owner_net_id: i64,
}

impl VChannel {
    pub fn width(&self) -> f64 {
        self.x_right - self.x_left
    }
    pub fn center_x(&self) -> f64 {
        (self.x_left + self.x_right) / 2.0
    }
}

// ============================================================================
// ChannelMap — main structure
// ============================================================================

#[derive(Debug, Clone)]
pub struct ChannelMap {
    pub horizontal: Vec<HChannel>,
    pub vertical: Vec<VChannel>,
    /// Minimum (y / x) gap between adjacent slots in the same channel (avoid wires crowding)
    pub line_gap: f64,
}

impl ChannelMap {
    /// Empty channel map (all reserve_* return None) —— equivalent to pre-P10 behavior
    pub fn empty() -> Self {
        Self {
            horizontal: Vec::new(),
            vertical: Vec::new(),
            line_gap: 6.0,
        }
    }

    /// Extract channels from a laid-out graph
    ///
    /// Algorithm:
    /// 1. Project all boxes onto the y axis to get (y_top, y_bot) interval list
    /// 2. Sort by y_top, merge intersecting / adjacent (gap < `merge_gap`) intervals into "box rows"
    /// 3. Blank between adjacent box rows (gap ≥ `min_channel_height`) = one HChannel
    /// 4. Same on X axis → VChannel
    ///
    /// `min_channel_height` (default 16px) prevents treating "5px gap between two boxes" as a channel.
    pub fn build(graph: &McVecGraph, line_gap: f64) -> Self {
        Self::build_with_options(graph, line_gap, 8.0, 16.0)
    }

    /// Full-parameter version (for testing or tuning)
    pub fn build_with_options(
        graph: &McVecGraph,
        line_gap: f64,
        merge_gap: f64,
        min_channel_height: f64,
    ) -> Self {
        if graph.boxes.is_empty() {
            return Self {
                horizontal: Vec::new(),
                vertical: Vec::new(),
                line_gap,
            };
        }

        // canvas bounds (use box bounding box + margin)
        let (canvas_x_min, canvas_y_min, canvas_x_max, canvas_y_max) = canvas_bounds(graph);

        // ── Horizontal channels ──
        let mut y_intervals: Vec<(f64, f64)> =
            graph.boxes.iter().map(|b| (b.y, b.y + b.h)).collect();
        y_intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let merged_rows = merge_intervals(&y_intervals, merge_gap);

        let mut horizontal = Vec::new();
        // Space from top to first row is also a channel (wires can route above the board)
        if let Some(&(first_top, _)) = merged_rows.first() {
            if first_top - canvas_y_min >= min_channel_height {
                horizontal.push(HChannel {
                    y_top: canvas_y_min,
                    y_bottom: first_top,
                    x_min: canvas_x_min,
                    x_max: canvas_x_max,
                    slots: Vec::new(),
                });
            }
        }
        // Between rows
        for w in merged_rows.windows(2) {
            let (_, prev_bot) = w[0];
            let (next_top, _) = w[1];
            if next_top - prev_bot >= min_channel_height {
                horizontal.push(HChannel {
                    y_top: prev_bot,
                    y_bottom: next_top,
                    x_min: canvas_x_min,
                    x_max: canvas_x_max,
                    slots: Vec::new(),
                });
            }
        }
        // Last row to bottom
        if let Some(&(_, last_bot)) = merged_rows.last() {
            if canvas_y_max - last_bot >= min_channel_height {
                horizontal.push(HChannel {
                    y_top: last_bot,
                    y_bottom: canvas_y_max,
                    x_min: canvas_x_min,
                    x_max: canvas_x_max,
                    slots: Vec::new(),
                });
            }
        }

        // ── Vertical channels ──
        let mut x_intervals: Vec<(f64, f64)> =
            graph.boxes.iter().map(|b| (b.x, b.x + b.w)).collect();
        x_intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let merged_cols = merge_intervals(&x_intervals, merge_gap);

        let mut vertical = Vec::new();
        if let Some(&(first_left, _)) = merged_cols.first() {
            if first_left - canvas_x_min >= min_channel_height {
                vertical.push(VChannel {
                    x_left: canvas_x_min,
                    x_right: first_left,
                    y_min: canvas_y_min,
                    y_max: canvas_y_max,
                    slots: Vec::new(),
                });
            }
        }
        for w in merged_cols.windows(2) {
            let (_, prev_right) = w[0];
            let (next_left, _) = w[1];
            if next_left - prev_right >= min_channel_height {
                vertical.push(VChannel {
                    x_left: prev_right,
                    x_right: next_left,
                    y_min: canvas_y_min,
                    y_max: canvas_y_max,
                    slots: Vec::new(),
                });
            }
        }
        if let Some(&(_, last_right)) = merged_cols.last() {
            if canvas_x_max - last_right >= min_channel_height {
                vertical.push(VChannel {
                    x_left: last_right,
                    x_right: canvas_x_max,
                    y_min: canvas_y_min,
                    y_max: canvas_y_max,
                    slots: Vec::new(),
                });
            }
        }

        eprintln!(
            "[route::channels] built {} H-channels, {} V-channels (canvas {}x{}, line_gap={})",
            horizontal.len(),
            vertical.len(),
            (canvas_x_max - canvas_x_min) as i32,
            (canvas_y_max - canvas_y_min) as i32,
            line_gap,
        );

        Self {
            horizontal,
            vertical,
            line_gap,
        }
    }

    /// Reserve a y position for a horizontal line (x_start → x_end @ y_pref)
    ///
    /// Behavior:
    /// 1. Find HChannel closest to `preferred_y` (channel's x range must cover the requested segment)
    /// 2. Probe slot candidate positions from center outward inside channel (step `line_gap`)
    /// 3. The first position that doesn't conflict with reserved slots (same y ± line_gap and x range overlap) = answer
    /// 4. All conflict → `None` (caller falls back to preferred y)
    ///
    /// Returns `Some(actual_y)` indicating the y position actually assigned in the channel.
    pub fn reserve_horizontal(
        &mut self,
        x_start: f64,
        x_end: f64,
        preferred_y: f64,
        net_id: i64,
    ) -> Option<f64> {
        let (lo, hi) = if x_start < x_end {
            (x_start, x_end)
        } else {
            (x_end, x_start)
        };

        // Step 1: find channel closest to preferred_y that can contain [lo, hi]
        let mut best_ch: Option<usize> = None;
        let mut best_dist = f64::INFINITY;
        for (i, ch) in self.horizontal.iter().enumerate() {
            if ch.x_min > lo || ch.x_max < hi {
                continue; // channel x doesn't cover requested segment
            }
            let cy = ch.center_y();
            let d = (preferred_y - cy).abs();
            if d < best_dist {
                best_dist = d;
                best_ch = Some(i);
            }
        }
        let ch_idx = best_ch?;

        // Step 2: find empty slot inside channel (probe outward from preferred_y)
        let line_gap = self.line_gap;
        let ch = &mut self.horizontal[ch_idx];
        let mid = ch.center_y();
        // Candidate y: mid, mid+gap, mid-gap, mid+2gap, mid-2gap, ...
        // Prefer preferred_y → actually probe outward from preferred_y (but limit within channel)
        let start_y = preferred_y.clamp(ch.y_top + line_gap, ch.y_bottom - line_gap);
        let mut candidates = Vec::new();
        candidates.push(start_y);
        let max_steps = ((ch.height() / line_gap) as usize).min(40);
        for step in 1..=max_steps {
            let off = step as f64 * line_gap;
            let up = start_y - off;
            let dn = start_y + off;
            if up >= ch.y_top + line_gap {
                candidates.push(up);
            }
            if dn <= ch.y_bottom - line_gap {
                candidates.push(dn);
            }
        }
        // Fallback: channel center
        candidates.push(mid);

        // Step 3: pick the first non-conflicting
        for cand in candidates {
            let conflicts = ch
                .slots
                .iter()
                .any(|s| (s.y - cand).abs() < line_gap && overlap_1d(s.x_min, s.x_max, lo, hi));
            if !conflicts {
                ch.slots.push(HSlot {
                    y: cand,
                    x_min: lo,
                    x_max: hi,
                    owner_net_id: net_id,
                });
                return Some(cand);
            }
        }
        None
    }

    /// Symmetric version: reserve x position for vertical line (y_start → y_end @ x_pref)
    pub fn reserve_vertical(
        &mut self,
        y_start: f64,
        y_end: f64,
        preferred_x: f64,
        net_id: i64,
    ) -> Option<f64> {
        let (lo, hi) = if y_start < y_end {
            (y_start, y_end)
        } else {
            (y_end, y_start)
        };

        let mut best_ch: Option<usize> = None;
        let mut best_dist = f64::INFINITY;
        for (i, ch) in self.vertical.iter().enumerate() {
            if ch.y_min > lo || ch.y_max < hi {
                continue;
            }
            let cx = ch.center_x();
            let d = (preferred_x - cx).abs();
            if d < best_dist {
                best_dist = d;
                best_ch = Some(i);
            }
        }
        let ch_idx = best_ch?;

        let line_gap = self.line_gap;
        let ch = &mut self.vertical[ch_idx];
        let mid = ch.center_x();
        let start_x = preferred_x.clamp(ch.x_left + line_gap, ch.x_right - line_gap);
        let mut candidates = Vec::new();
        candidates.push(start_x);
        let max_steps = ((ch.width() / line_gap) as usize).min(40);
        for step in 1..=max_steps {
            let off = step as f64 * line_gap;
            let lft = start_x - off;
            let rt = start_x + off;
            if lft >= ch.x_left + line_gap {
                candidates.push(lft);
            }
            if rt <= ch.x_right - line_gap {
                candidates.push(rt);
            }
        }
        candidates.push(mid);

        for cand in candidates {
            let conflicts = ch
                .slots
                .iter()
                .any(|s| (s.x - cand).abs() < line_gap && overlap_1d(s.y_min, s.y_max, lo, hi));
            if !conflicts {
                ch.slots.push(VSlot {
                    x: cand,
                    y_min: lo,
                    y_max: hi,
                    owner_net_id: net_id,
                });
                return Some(cand);
            }
        }
        None
    }

    /// Total number of reserved slots (debug)
    pub fn total_slots(&self) -> usize {
        self.horizontal.iter().map(|c| c.slots.len()).sum::<usize>()
            + self.vertical.iter().map(|c| c.slots.len()).sum::<usize>()
    }

    /// Slot count grouped by owner_net_id (debug)
    pub fn slots_per_net(&self) -> HashMap<i64, usize> {
        let mut out: HashMap<i64, usize> = HashMap::new();
        for ch in &self.horizontal {
            for s in &ch.slots {
                *out.entry(s.owner_net_id).or_insert(0) += 1;
            }
        }
        for ch in &self.vertical {
            for s in &ch.slots {
                *out.entry(s.owner_net_id).or_insert(0) += 1;
            }
        }
        out
    }
}

// ============================================================================
// helpers
// ============================================================================

/// Merge intersecting / adjacent (gap < `gap`) intervals
fn merge_intervals(intervals: &[(f64, f64)], gap: f64) -> Vec<(f64, f64)> {
    if intervals.is_empty() {
        return Vec::new();
    }
    let mut sorted = intervals.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = vec![sorted[0]];
    for &(lo, hi) in &sorted[1..] {
        let last = out.last_mut().unwrap();
        if lo - last.1 < gap {
            last.1 = last.1.max(hi);
        } else {
            out.push((lo, hi));
        }
    }
    out
}

fn overlap_1d(a_lo: f64, a_hi: f64, b_lo: f64, b_hi: f64) -> bool {
    a_hi >= b_lo && b_hi >= a_lo
}

fn canvas_bounds(graph: &McVecGraph) -> (f64, f64, f64, f64) {
    let margin = 40.0;
    let x_min = graph
        .boxes
        .iter()
        .map(|b| b.x)
        .fold(f64::INFINITY, f64::min)
        - margin;
    let y_min = graph
        .boxes
        .iter()
        .map(|b| b.y)
        .fold(f64::INFINITY, f64::min)
        - margin;
    let x_max = graph
        .boxes
        .iter()
        .map(|b| b.x + b.w)
        .fold(f64::NEG_INFINITY, f64::max)
        + margin;
    let y_max = graph
        .boxes
        .iter()
        .map(|b| b.y + b.h)
        .fold(f64::NEG_INFINITY, f64::max)
        + margin;
    (x_min, y_min, x_max, y_max)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary, McVecBox};

    fn mk_box(id: i64, x: f64, y: f64, w: f64, h: f64) -> McVecBox {
        let mut b = McVecBox::new(
            id,
            format!("b{}", id),
            String::new(),
            BoxKind::MultiPin,
            1,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        b
    }

    // ────────────────────────────────────────────────────────────────────────
    // merge_intervals
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p10_merge_basic() {
        let m = merge_intervals(&[(0.0, 10.0), (5.0, 15.0)], 0.0);
        assert_eq!(m, vec![(0.0, 15.0)]);
    }

    #[test]
    fn p10_merge_gap_below_threshold() {
        // (0,10) and (12,20), gap=5: |12-10|=2 < 5 → merge
        let m = merge_intervals(&[(0.0, 10.0), (12.0, 20.0)], 5.0);
        assert_eq!(m, vec![(0.0, 20.0)]);
    }

    #[test]
    fn p10_merge_gap_above_threshold() {
        // (0,10) and (20,30), gap=5: don't merge
        let m = merge_intervals(&[(0.0, 10.0), (20.0, 30.0)], 5.0);
        assert_eq!(m, vec![(0.0, 10.0), (20.0, 30.0)]);
    }

    // ────────────────────────────────────────────────────────────────────────
    // build
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p10_build_extracts_channel_between_rows() {
        // Two rows of boxes: row 1 at y=0..100, row 2 at y=200..300, 100px blank in middle = one H channel
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 0.0, 0.0, 100.0, 100.0));
        g.boxes.push(mk_box(2, 200.0, 0.0, 100.0, 100.0));
        g.boxes.push(mk_box(3, 0.0, 200.0, 100.0, 100.0));
        g.boxes.push(mk_box(4, 200.0, 200.0, 100.0, 100.0));

        let cm = ChannelMap::build(&g, 6.0);
        // At least 1 H channel (between y=100 and y=200)
        let mid_channels: Vec<&HChannel> = cm
            .horizontal
            .iter()
            .filter(|c| c.y_top >= 100.0 && c.y_bottom <= 200.0)
            .collect();
        assert!(!mid_channels.is_empty(), "expected H channel between rows");
    }

    #[test]
    fn p10_build_no_boxes_yields_empty() {
        let g = McVecGraph::new(0, "test".into());
        let cm = ChannelMap::build(&g, 6.0);
        assert!(cm.horizontal.is_empty());
        assert!(cm.vertical.is_empty());
    }

    #[test]
    fn p10_build_vertical_channels() {
        // Two columns of boxes, blank in middle → V channel
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 0.0, 0.0, 100.0, 200.0));
        g.boxes.push(mk_box(2, 300.0, 0.0, 100.0, 200.0));
        let cm = ChannelMap::build(&g, 6.0);
        let mid: Vec<&VChannel> = cm
            .vertical
            .iter()
            .filter(|c| c.x_left >= 100.0 && c.x_right <= 300.0)
            .collect();
        assert!(!mid.is_empty(), "expected V channel between columns");
    }

    // ────────────────────────────────────────────────────────────────────────
    // reserve_horizontal
    // ────────────────────────────────────────────────────────────────────────

    fn hand_built_map() -> ChannelMap {
        ChannelMap {
            horizontal: vec![HChannel {
                y_top: 100.0,
                y_bottom: 200.0,
                x_min: 0.0,
                x_max: 1000.0,
                slots: Vec::new(),
            }],
            vertical: Vec::new(),
            line_gap: 10.0,
        }
    }

    #[test]
    fn p10_reserve_first_call_picks_preferred() {
        let mut cm = hand_built_map();
        let y = cm.reserve_horizontal(0.0, 500.0, 150.0, 1);
        assert_eq!(y, Some(150.0));
    }

    #[test]
    fn p10_reserve_overlapping_x_gets_different_y() {
        // Both nets walk horizontally from [0, 500], both prefer y=150 → should get different y
        let mut cm = hand_built_map();
        let y1 = cm.reserve_horizontal(0.0, 500.0, 150.0, 1).unwrap();
        let y2 = cm.reserve_horizontal(0.0, 500.0, 150.0, 2).unwrap();
        assert!((y1 - y2).abs() >= 10.0, "should be separated by ≥ line_gap");
    }

    #[test]
    fn p10_reserve_non_overlapping_x_can_share_y() {
        // Both nets prefer y=150, but x doesn't overlap → can share same y
        let mut cm = hand_built_map();
        let y1 = cm.reserve_horizontal(0.0, 100.0, 150.0, 1).unwrap();
        let y2 = cm.reserve_horizontal(500.0, 700.0, 150.0, 2).unwrap();
        assert!(
            (y1 - y2).abs() < 0.5,
            "non-overlapping x ranges should share preferred y"
        );
    }

    #[test]
    fn p10_reserve_returns_none_when_channel_doesnt_cover() {
        // No channel covers [2000, 3000]
        let mut cm = hand_built_map();
        let y = cm.reserve_horizontal(2000.0, 3000.0, 150.0, 1);
        assert_eq!(y, None);
    }

    #[test]
    fn p10_reserve_returns_none_when_no_channels() {
        let mut cm = ChannelMap::empty();
        let y = cm.reserve_horizontal(0.0, 100.0, 50.0, 1);
        assert_eq!(y, None);
    }

    // ────────────────────────────────────────────────────────────────────────
    // reserve_vertical (symmetric)
    // ────────────────────────────────────────────────────────────────────────

    fn hand_built_v_map() -> ChannelMap {
        ChannelMap {
            horizontal: Vec::new(),
            vertical: vec![VChannel {
                x_left: 100.0,
                x_right: 200.0,
                y_min: 0.0,
                y_max: 1000.0,
                slots: Vec::new(),
            }],
            line_gap: 10.0,
        }
    }

    #[test]
    fn p10_reserve_vertical_picks_preferred_x() {
        let mut cm = hand_built_v_map();
        let x = cm.reserve_vertical(0.0, 500.0, 150.0, 1);
        assert_eq!(x, Some(150.0));
    }

    #[test]
    fn p10_reserve_vertical_two_nets_different_x() {
        let mut cm = hand_built_v_map();
        let x1 = cm.reserve_vertical(0.0, 500.0, 150.0, 1).unwrap();
        let x2 = cm.reserve_vertical(0.0, 500.0, 150.0, 2).unwrap();
        assert!((x1 - x2).abs() >= 10.0);
    }

    // ────────────────────────────────────────────────────────────────────────
    // bookkeeping
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn p10_total_slots_counts_correctly() {
        let mut cm = hand_built_map();
        cm.reserve_horizontal(0.0, 100.0, 150.0, 1);
        cm.reserve_horizontal(200.0, 300.0, 150.0, 2);
        cm.reserve_horizontal(400.0, 500.0, 150.0, 3);
        assert_eq!(cm.total_slots(), 3);
    }

    #[test]
    fn p10_slots_per_net() {
        let mut cm = hand_built_map();
        cm.reserve_horizontal(0.0, 100.0, 150.0, 1);
        cm.reserve_horizontal(200.0, 300.0, 150.0, 1); // same net
        cm.reserve_horizontal(400.0, 500.0, 150.0, 2);
        let map = cm.slots_per_net();
        assert_eq!(map.get(&1).copied(), Some(2));
        assert_eq!(map.get(&2).copied(), Some(1));
    }
}
