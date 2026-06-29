// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Manhattan orthogonal polyline routing
//!
//! ## Algorithm (Iter 8 rewrite, true Manhattan)
//! Four cases by exit direction of both ends:
//! 1. Both ends exit horizontally (L/R) → `H-V-H` Z shape
//! 2. Both ends exit vertically (T/B) → `V-H-V` Z shape
//! 3. Source horizontal + dest vertical   → `H-V` L shape
//! 4. Source vertical + dest horizontal   → `V-H` L shape
//!
//! **Key constraint**: each segment only varies along a single coordinate axis, never diagonal.
//!
//! ## Old bug warning (before Iter 8)
//! The old implementation in cases 3/4 used `(mid_x, sy)` as the only turn, the second
//! segment `(mid_x, sy) → (dx, dy)` was diagonal → the whole picture was tilted.
//! Iter 8 changed to 4 independent topologies, each guaranteed all-orthogonal.
//!
//! ## ★ P09 (S5) obstacle-aware refactor
//! `OrthogonalRouter::route` builds [`ObstacleMap`] first before writing pairwise
//! polylines; candidates whose turn points hit boxes are eliminated, and only when
//! no non-colliding path exists does it fall back to the detour algorithm.
//! The pure function `orthogonal_path` (doesn't know about obstacles) is preserved
//! for reuse by trunk_tap and other modules.
//!
//! ## ★ P10 (S6) channel-aware refactor
//! Added `route_orthogonal_with_channels`: scheduler passes in ChannelMap,
//! - **HVH Z shape**: the middle V segment reserves an x in vertical channel
//! - **VHV Z shape**: the middle H segment reserves a y in horizontal channel
//! Multiple ortho lines' turn points no longer concentrate on the same x/y, visually staggered.

use crate::vector::graph::{McVecBox, McVecGraph, Point, Route, Segment, VizNet};

use super::channels::ChannelMap;
use super::obstacles::{best_orthogonal_path, ObstacleMap};
use super::side::ExitSide;
use crate::viz::traits::Router;

// ============================================================================
// Public pure function: orthogonal_path
// ============================================================================

/// Compute Manhattan polyline path between two points, return all turn points
///
/// Input: start / end + their respective exit directions
/// Output: `Vec<(x, y)>` —— at least 2 points (start + end)
pub fn orthogonal_path(
    sp: (f64, f64),
    dp: (f64, f64),
    s_side: ExitSide,
    d_side: ExitSide,
) -> Vec<(f64, f64)> {
    let (sx, sy) = sp;
    let (dx, dy) = dp;
    const EPS: f64 = 1.5;

    // Degenerate: same-direction exit and already aligned → straight line.
    //   (After P0 makes connected pins collinear, most 2-point nets go here, 0 turns.)
    if (sy - dy).abs() < EPS && s_side.is_horizontal() && d_side.is_horizontal() {
        return vec![sp, dp];
    }
    if (sx - dx).abs() < EPS && !s_side.is_horizontal() && !d_side.is_horizontal() {
        return vec![sp, dp];
    }

    match (s_side.is_horizontal(), d_side.is_horizontal()) {
        (true, true) => {
            // H-V-H Z shape. ★ P1: vertical segment not in midair, hugs **consumer-side** box
            //   (elbow near target), so when multiple parallel lines converge into the
            //   same box, the vertical segments cluster in the same column next to
            //   the target → comb-tooth shape, tidy; and the long leg is horizontal,
            //   visually smoother.
            let ex = elbow_near_dest(sx, dx, d_side, /*horizontal=*/ true);
            vec![sp, (ex, sy), (ex, dy), dp]
        }
        (false, false) => {
            // V-H-V Z shape, horizontal segment hugs target
            let ey = elbow_near_dest(sy, dy, d_side, /*horizontal=*/ false);
            vec![sp, (sx, ey), (dx, ey), dp]
        }
        (true, false) => vec![sp, (dx, sy), dp], // L: source horizontal → dest vertical, turn (dx, sy)
        (false, true) => vec![sp, (sx, dy), dp], // L: source vertical → dest horizontal, turn (sx, dy)
    }
}

/// Z-shape middle segment position: hugs the exit side of the target box (with a fixed
/// approach gap), falls back to midpoint if out of bounds.
/// When `horizontal=true` returns the x of the vertical segment (uses d_side Left/Right);
/// otherwise returns the y of the horizontal segment (Top/Bottom).
fn elbow_near_dest(s: f64, d: f64, d_side: ExitSide, horizontal: bool) -> f64 {
    const APPROACH: f64 = 20.0;
    let cand = if horizontal {
        match d_side {
            ExitSide::Left => d - APPROACH,
            ExitSide::Right => d + APPROACH,
            _ => (s + d) / 2.0,
        }
    } else {
        match d_side {
            ExitSide::Top => d - APPROACH,
            ExitSide::Bottom => d + APPROACH,
            _ => (s + d) / 2.0,
        }
    };
    // Must not cross over the source (otherwise the line folds back) → use only if in [min,max], else midpoint
    let (lo, hi) = (s.min(d), s.max(d));
    if cand >= lo && cand <= hi {
        cand
    } else {
        (s + d) / 2.0
    }
}

/// SVG path `d` attribute string
pub fn points_to_svg_d(points: &[(f64, f64)]) -> String {
    let mut out = String::new();
    for (i, &(x, y)) in points.iter().enumerate() {
        if i == 0 {
            out.push_str(&format!("M{x:.1},{y:.1}"));
        } else {
            out.push_str(&format!(" L{x:.1},{y:.1}"));
        }
    }
    out
}

/// Compute label anchor for Manhattan path (midpoint of the middle segment)
///
/// - H-V-H / V-H-V: path bbox center
/// - L shape: slightly inside the turn point
pub fn label_anchor(
    sp: (f64, f64),
    dp: (f64, f64),
    s_side: ExitSide,
    d_side: ExitSide,
) -> (f64, f64) {
    let (sx, sy) = sp;
    let (dx, dy) = dp;
    match (s_side.is_horizontal(), d_side.is_horizontal()) {
        (true, true) | (false, false) => ((sx + dx) / 2.0, (sy + dy) / 2.0),
        (true, false) => (dx, (sy + dy) / 2.0),
        (false, true) => ((sx + dx) / 2.0, dy),
    }
}

// ============================================================================
// OrthogonalRouter (Router trait impl)
// ============================================================================

/// Route a multi-endpoint [`VizNet`] as a set of Manhattan polylines (write to `net.route`)
///
/// Simplified algorithm: connect endpoints pairwise, each pair walks a Manhattan polyline.
/// (Same behavior as the old `render_edge`, just separating "path computation" and
/// "SVG output".)
///
/// For more complex multi-endpoint topologies, use
/// [`super::star::StarRouter`] / [`super::bus_bundle::BusBundleRouter`].
pub struct OrthogonalRouter;

impl Router for OrthogonalRouter {
    fn route(&self, graph: &McVecGraph, net: &mut VizNet) {
        let mut route = Route::new();

        if net.endpoints.len() < 2 {
            net.route = Some(route);
            return;
        }

        // ★ P09: build obstacle map (exclude all endpoint boxes of this net)
        let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
        let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);

        // pairwise: each pair of endpoints walks an obstacle-aware path
        for i in 0..net.endpoints.len() {
            for j in (i + 1)..net.endpoints.len() {
                let a = &net.endpoints[i];
                let b = &net.endpoints[j];
                let box_a = graph.boxes.iter().find(|x| x.id == a.box_id);
                let box_b = graph.boxes.iter().find(|x| x.id == b.box_id);
                if let (Some(ba), Some(bb)) = (box_a, box_b) {
                    let (sp, ss) = super::side::compute_exit_for_pin(ba, a.pin_id, Some(bb));
                    let (dp, ds) = super::side::compute_exit_for_pin(bb, b.pin_id, Some(ba));

                    // Prefer direction-aware orthogonal_path; if hits obstacles, use
                    // best_orthogonal_path (tries 4 L/Z candidates and detours)
                    let pts = orthogonal_path(sp, dp, ss, ds);
                    let segs_from_pts: Vec<(f64, f64, f64, f64)> = pts
                        .windows(2)
                        .map(|w| (w[0].0, w[0].1, w[1].0, w[1].1))
                        .collect();

                    let final_segs = if obstacles.first_hit(&segs_from_pts).is_none() {
                        // Direction-aware path doesn't collide, use directly
                        segs_from_pts
                    } else {
                        // Collision, switch to obstacle-aware best pick
                        best_orthogonal_path(sp.0, sp.1, dp.0, dp.1, &obstacles)
                    };

                    for (x1, y1, x2, y2) in final_segs {
                        route.segments.push(Segment {
                            from: Point::new(x1, y1),
                            to: Point::new(x2, y2),
                        });
                    }
                }
            }
        }

        net.route = Some(route);
    }

    fn name(&self) -> &'static str {
        "orthogonal"
    }
}

// ============================================================================
// ★ P10 (S6) — channel-aware end-to-end entry
// ============================================================================

/// P10 main entry: channel-aware orthogonal pairwise routing
///
/// For each pair of endpoints:
/// 1. Compute obstacle-aware path (4 L/Z candidates, detour on collision) per P09
/// 2. **Reserve** the path segments in channels: long horizontal segments go to H channel,
///    long vertical segments go to V channel
/// 3. If a different y/x is reserved in the channel → adjust that segment's position
///    (with adjacent segments' endpoints synced)
///
/// Short segments (< MIN_CHANNEL_SEGMENT pixels) don't enter channels, avoiding too
/// many noisy slots.
pub fn route_orthogonal_with_channels(
    graph: &McVecGraph,
    net: &mut VizNet,
    channels: &mut ChannelMap,
) {
    let mut route = Route::new();
    if net.endpoints.len() < 2 {
        net.route = Some(route);
        return;
    }

    // ★ Pin escape refactor: two-pin passive endpoints are **not** excluded from
    //   obstacles (otherwise this net's own line would graze/pierce the resistor
    //   body to reach the pin). Only exclude non-passive endpoints (large
    //   modules/flags, routing to their pins is safe, they have many pins and a
    //   large body, excluding is needed to reach them).
    let exclude: Vec<i64> = net
        .endpoints
        .iter()
        .filter(|e| !box_is_two_pin_passive(graph, e.box_id))
        .map(|e| e.box_id)
        .collect();
    let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);
    let net_id = net.nid;

    for i in 0..net.endpoints.len() {
        for j in (i + 1)..net.endpoints.len() {
            let a = &net.endpoints[i];
            let b = &net.endpoints[j];
            let box_a = graph.boxes.iter().find(|x| x.id == a.box_id);
            let box_b = graph.boxes.iter().find(|x| x.id == b.box_id);
            if let (Some(ba), Some(bb)) = (box_a, box_b) {
                let (sp, ss) = super::side::compute_exit_for_pin(ba, a.pin_id, Some(bb));
                let (dp, ds) = super::side::compute_exit_for_pin(bb, b.pin_id, Some(ba));

                // ★ Escape point: passive endpoint first walks PIN_ESCAPE outward
                //   along pin direction, main path starts from escape point, then a
                //   short stub connects back to the pin. The stub is perpendicular
                //   to the box edge, pointing outward → never pierces the body.
                let (ra, stub_a) = pin_escape(ba, sp, ss);
                let (rb, stub_b) = pin_escape(bb, dp, ds);

                // Step 1: use P09 to compute base path (between escape points)
                let pts = orthogonal_path(ra, rb, ss, ds);
                let base_segs: Vec<(f64, f64, f64, f64)> = pts
                    .windows(2)
                    .map(|w| (w[0].0, w[0].1, w[1].0, w[1].1))
                    .collect();

                let mut final_segs = if obstacles.first_hit(&base_segs).is_none() {
                    base_segs
                } else {
                    best_orthogonal_path(ra.0, ra.1, rb.0, rb.1, &obstacles)
                };

                // Step 2: use channels to adjust elbow (middle long segment) position
                final_segs = adjust_path_to_channels(final_segs, channels, net_id);

                // Splice: stub_a (pin → escape) + main path + stub_b (escape → pin)
                if let Some(s) = stub_a {
                    route.segments.push(Segment {
                        from: Point::new(s.0, s.1),
                        to: Point::new(s.2, s.3),
                    });
                }
                for (x1, y1, x2, y2) in final_segs {
                    route.segments.push(Segment {
                        from: Point::new(x1, y1),
                        to: Point::new(x2, y2),
                    });
                }
                if let Some(s) = stub_b {
                    route.segments.push(Segment {
                        from: Point::new(s.0, s.1),
                        to: Point::new(s.2, s.3),
                    });
                }
            }
        }
    }

    net.route = Some(route);
}

/// **Pin escape** for two-pin passive endpoints: returns (main path start point,
/// optional stub segment `pin→escape`).
///
/// Passive endpoint → escape = pin walks `PIN_ESCAPE` pixels outward along exit
/// direction (lands outside the body), main path starts from escape point and
/// bypasses the body (body is an obstacle at this point), stub connects escape
/// back to the pin. The stub is perpendicular to the box edge, pointing outward,
/// and cannot pierce the body.
/// Non-passive endpoints don't escape (use pin point directly, same as pre-change).
fn pin_escape(
    b: &McVecBox,
    pin: (f64, f64),
    side: ExitSide,
) -> ((f64, f64), Option<(f64, f64, f64, f64)>) {
    if !b.is_two_pin_passive() {
        return (pin, None);
    }
    // Escape distance must be > obstacle inflate (8px), otherwise escape point is still inside inflated rect
    const PIN_ESCAPE: f64 = 12.0;
    let target = match side {
        ExitSide::Left => (pin.0 - PIN_ESCAPE, pin.1),
        ExitSide::Right => (pin.0 + PIN_ESCAPE, pin.1),
        ExitSide::Top => (pin.0, pin.1 - PIN_ESCAPE),
        ExitSide::Bottom => (pin.0, pin.1 + PIN_ESCAPE),
    };
    (target, Some((pin.0, pin.1, target.0, target.1)))
}

/// Whether the box is a two-pin passive (R/C/L/D)
fn box_is_two_pin_passive(graph: &McVecGraph, box_id: i64) -> bool {
    graph
        .boxes
        .iter()
        .find(|b| b.id == box_id)
        .map(|b| b.is_two_pin_passive())
        .unwrap_or(false)
}

/// Short segment threshold: segments shorter than this don't enter channels
/// (avoid short stubs occupying slots)
const MIN_CHANNEL_SEGMENT: f64 = 40.0;

/// ── ★ Phase E.2 ──
/// **Maximum allowed displacement** for elbow channel snapping. If the channel
/// reservation pulls the elbow further than this, skip the snap and keep the natural
/// elbow —— otherwise we'll see cases like the hbl main layer where `__net_10`/`__net_11`
/// signals were forcibly dragged from the middle of mcu513-speaker (x≈1066) to the
/// rightmost channel (x≈1361), a +295px big detour, visually looking like the
/// signal line "randomly runs to the corner".
///
/// Trade-off: the goal of channel snapping is to let multiple parallel wires share
/// a column / row for tidiness, but when the layer has too few usable channels
/// (this example has only 2 V-channels), the only approximate match may be hundreds
/// of pixels from the preferred position, and the snap's tidiness is far outweighed
/// by the ugliness of the detour. 80px is a compromise: snaps within a few component
/// widths are fine, cross-canvas detours are rejected.
const MAX_SNAP_DISTANCE: f64 = 80.0;

/// Move the "middle long segment" of a path to the channel reserved position
///
/// For a 3-segment polyline (HVH or VHV), the middle segment is the elbow. We change
/// its y/x to the value reserved in the channel, and sync the endpoints of the
/// adjacent two segments.
///
/// The algorithm is conservative: only adjusts the **true middle segment** (the 2nd
/// of 3 segments, and at least MIN_CHANNEL_SEGMENT long). Other cases are returned
/// as-is.
fn adjust_path_to_channels(
    segs: Vec<(f64, f64, f64, f64)>,
    channels: &mut ChannelMap,
    net_id: i64,
) -> Vec<(f64, f64, f64, f64)> {
    if segs.len() != 3 {
        return segs; // L shape (2 segments) or more complex detour: not handled
    }
    let s0 = segs[0];
    let s1 = segs[1];
    let s2 = segs[2];

    // Determine if s1 is horizontal or vertical
    let is_horiz = (s1.1 - s1.3).abs() < 0.1; // y unchanged → horizontal
    let is_vert = (s1.0 - s1.2).abs() < 0.1; // x unchanged → vertical

    if is_horiz && !is_vert {
        // VHV Z shape: s1 is the horizontal middle segment
        let len = (s1.2 - s1.0).abs();
        if len < MIN_CHANNEL_SEGMENT {
            return vec![s0, s1, s2];
        }
        let pref_y = s1.1;
        if let Some(new_y) = channels.reserve_horizontal(s1.0, s1.2, pref_y, net_id) {
            // ── Phase E.2: reject too-far snaps (see MAX_SNAP_DISTANCE comment) ──
            let delta = (new_y - pref_y).abs();
            if delta > MAX_SNAP_DISTANCE {
                crate::vlog!(
                    "[route::orthogonal] net_id={net_id} elbow y snap rejected: {pref_y:.0} → {new_y:.0} \
                     (Δ {delta:.0} > MAX_SNAP_DISTANCE {MAX_SNAP_DISTANCE:.0}), keeping natural elbow"
                );
                return vec![s0, s1, s2];
            }
            if delta > 0.5 {
                crate::vlog!(
                    "[route::orthogonal] net_id={net_id} elbow y {pref_y:.0} → {new_y:.0} (channel)"
                );
                // s0.to.y = new_y, s1.from.y = s1.to.y = new_y, s2.from.y = new_y
                return vec![
                    (s0.0, s0.1, s0.2, new_y),
                    (s1.0, new_y, s1.2, new_y),
                    (s2.0, new_y, s2.2, s2.3),
                ];
            }
        }
    } else if is_vert && !is_horiz {
        // HVH Z shape: s1 is the vertical middle segment
        let len = (s1.3 - s1.1).abs();
        if len < MIN_CHANNEL_SEGMENT {
            return vec![s0, s1, s2];
        }
        let pref_x = s1.0;
        if let Some(new_x) = channels.reserve_vertical(s1.1, s1.3, pref_x, net_id) {
            // ── Phase E.2: reject too-far snaps ──
            let delta = (new_x - pref_x).abs();
            if delta > MAX_SNAP_DISTANCE {
                crate::vlog!(
                    "[route::orthogonal] net_id={net_id} elbow x snap rejected: {pref_x:.0} → {new_x:.0} \
                     (Δ {delta:.0} > MAX_SNAP_DISTANCE {MAX_SNAP_DISTANCE:.0}), keeping natural elbow"
                );
                return vec![s0, s1, s2];
            }
            if delta > 0.5 {
                crate::vlog!(
                    "[route::orthogonal] net_id={net_id} elbow x {pref_x:.0} → {new_x:.0} (channel)"
                );
                return vec![
                    (s0.0, s0.1, new_x, s0.3),
                    (new_x, s1.1, new_x, s1.3),
                    (new_x, s2.1, s2.2, s2.3),
                ];
            }
        }
    }
    vec![s0, s1, s2]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_h_v_h_zshape() {
        let path = orthogonal_path(
            (100.0, 100.0),
            (300.0, 200.0),
            ExitSide::Right,
            ExitSide::Left,
        );
        assert_eq!(path.len(), 4, "H-V-H 4 nodes 3 segments");
        // First segment horizontal, second vertical, third horizontal
        assert!((path[0].1 - path[1].1).abs() < 0.5);
        assert!((path[1].0 - path[2].0).abs() < 0.5);
        assert!((path[2].1 - path[3].1).abs() < 0.5);
    }

    #[test]
    fn test_l_shape_h_to_v() {
        let path = orthogonal_path(
            (100.0, 100.0),
            (300.0, 250.0),
            ExitSide::Right,
            ExitSide::Top,
        );
        assert_eq!(path.len(), 3, "L shape 3 nodes 2 segments");
        assert!((path[0].1 - path[1].1).abs() < 0.5);
        assert!((path[1].0 - path[2].0).abs() < 0.5);
    }

    #[test]
    fn test_all_segments_are_orthogonal_for_diagonal() {
        // Old bug scenario: two horizontal exits + dst at lower-right of src
        let path = orthogonal_path(
            (100.0, 100.0),
            (400.0, 300.0),
            ExitSide::Right,
            ExitSide::Left,
        );
        for w in path.windows(2) {
            let dx_eq = (w[0].0 - w[1].0).abs() < 0.5;
            let dy_eq = (w[0].1 - w[1].1).abs() < 0.5;
            assert!(
                dx_eq || dy_eq,
                "segment {:?} → {:?} is diagonal",
                w[0],
                w[1]
            );
        }
    }
}
