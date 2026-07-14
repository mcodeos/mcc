// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW (Step 2) —— Trunk-tap routing for multi-endpoint Signal (Steiner-tree simplified)
//!
//! ## Motivation
//! Before Step 2, multi-endpoint Signal was routed through `OrthogonalRouter`'s
//! pairwise decomposition (N endpoints → N*(N-1)/2 independent Manhattan polylines).
//! The same logical wire was drawn C(N,2) times, overlapping and crossing each other —
//! a visual disaster.
//!
//! Step 2's solution: treat the multi-endpoint net as a hyperedge, expressed as one
//! **trunk** + multiple **taps**. This is the `BusBundleRouter` algorithm, now
//! generalized to all multi-endpoint Signals.
//!
//! ## Critical role of the pin stub
//! Exit point + exit direction (filled in by Step 1 from EntryPoint) give us the pin's
//! **natural extension direction**. If we don't draw a pin stub, connecting the exit
//! point directly to the trunk will produce a "pin exits right, but the first segment
//! goes up" picture that violates physical intuition.
//!
//! Add an 8-15px pin stub: the pin first walks a short distance along the exit
//! direction, then L-turns to the trunk. Visually natural.
//!
//! ## Algorithm
//! ```text
//! 1. For each endpoint: (exit point, exit direction) = compute_exit_for_pin
//! 2. Pin stub: exit point → walk PIN_STUB_LEN along exit direction
//! 3. Trunk direction:
//!    - Most endpoints exit L/R (horizontal exit) → trunk **vertical** (taps extend horizontally, natural)
//!    - Most endpoints exit T/B (vertical exit) → trunk **horizontal** (taps extend vertically, natural)
//!    - Tied: use stub_ends bbox aspect ratio
//! 4. Trunk position: mean of stub_ends along the perpendicular direction
//!                                          (★ P09: prefer obstacle-free axis)
//!                                          (★ P10: prefer ChannelMap's reserved slot)
//! 5. Trunk length: covers full span of stub_ends along main axis (+overhang)
//! 6. Each stub_end → trunk via L-shaped tap
//! 7. Drop a junction at each tap landing point (T-shaped node)
//! ```
//!
//! ## Degenerate cases
//! - Endpoints ≤ 1: empty route
//! - Endpoints = 2: direct Manhattan (no trunk needed)
//! - Endpoints ≥ 3: trunk-tap
//!
//! ## ★ P09 (S5) refactor
//! The trunk axis was previously chosen as the mean of stub_ends, often crossing
//! boxes in the middle. `choose_trunk_axis` searches for the position that crosses
//! the **fewest obstacles** in a ± range near the mean, while considering preference
//! for proximity to the original mean.
//!
//! ## ★ P10 (S6) refactor
//! `BuildOptions` adds `channels: Option<&mut ChannelMap>`. When **multiple trunks
//! coexist**:
//! - Prefer `channels.reserve_horizontal/vertical` to reserve a slot inside a channel
//! - Reserved slots won't be occupied by subsequent nets → trunks are staggered
//! - Falls back to P09 behavior (`choose_trunk_axis`) when channels aren't usable
//!
//! ## P11 / P12 TODO
//! - The current router still builds ObstacleMap internally; P10 reuses channels but
//!   obstacle isn't moved outside yet

use crate::vector::graph::{McVecGraph, Point, Route, Segment, VizNet};

use super::channels::ChannelMap;
use super::obstacles::ObstacleMap;
use super::orthogonal::orthogonal_path;
use super::side::{compute_exit_for_pin, ExitSide};
use crate::viz::traits::Router;

// ============================================================================
// Constants
// ============================================================================

/// Minimum distance a pin must walk after exit before being allowed to turn
///
/// Too short (<6) still looks like a direct corner from the box edge; too long (>15)
/// wastes canvas space.
pub const PIN_STUB_LEN: f64 = 10.0;

// ============================================================================
// TrunkTapRouter
// ============================================================================

/// Router using one trunk + taps for multi-endpoint nets (default for multi-endpoint Signal)
pub struct TrunkTapRouter;

impl Router for TrunkTapRouter {
    fn route(&self, graph: &McVecGraph, net: &mut VizNet) {
        let exits = collect_exits(graph, net);

        // ★ P09: build obstacle map, exclude this net's endpoint boxes
        let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
        let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);

        let route = match exits.len() {
            0 | 1 => Route::new(),
            2 => build_two_point_route(&exits),
            _ => build_trunk_tap_route(
                &exits,
                BuildOptions {
                    obstacles: Some(&obstacles),
                    ..Default::default()
                },
            ),
        };

        net.route = Some(route);
    }
    fn name(&self) -> &'static str {
        "trunk_tap"
    }
}

// ============================================================================
// Shared helper: build_trunk_tap_route
// ============================================================================

/// Trunk-tap construction parameters
///
/// ## ★ P09 (S5) changes
/// Added `obstacles` field, allowing the router to avoid obstacles when picking the
/// trunk axis. When `None`, falls back to the pre-P09 behavior (axis = mean of stub_ends).
///
/// ## ★ P10 (S6) changes
/// Added `channels` field. When **multiple trunks coexist**, use `reserve_horizontal/vertical`
/// to reserve a slot in the channel, avoiding multiple trunks overlapping.
/// When `None`, falls back to P09 behavior (`choose_trunk_axis` picks obstacle-free mean).
#[derive(Default)]
pub struct BuildOptions<'a> {
    /// Trunk extension length on both ends (Bus uses 12 so the thick line extends past
    /// the outermost tap, looking more like a bus trunk)
    pub trunk_overhang: f64,
    /// Obstacle map (None = no obstacle avoidance, for old callers)
    pub obstacles: Option<&'a ObstacleMap>,
    /// Channel map (None = don't participate in channel coordination, P09 behavior)
    pub channels: Option<&'a mut ChannelMap>,
    /// net id (only used to mark owner when reserving channel)
    pub net_id: i64,
}

// ============================================================================
// ★ P10 (S6) end-to-end entry — channel-aware TrunkTapRouter
// ============================================================================

/// P10 main entry: channel-aware trunk-tap routing for one net
///
/// Called by `scheduler::route_one_net_with_channels` when dispatching TrunkTap/TrunkTapWithWarning.
pub fn route_trunk_tap_with_channels(
    graph: &McVecGraph,
    net: &mut VizNet,
    channels: &mut ChannelMap,
) {
    let exits = collect_exits(graph, net);
    let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
    let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);
    let net_id = net.nid;

    let route = match exits.len() {
        0 | 1 => Route::new(),
        2 => build_two_point_route(&exits),
        _ => build_trunk_tap_route(
            &exits,
            BuildOptions {
                obstacles: Some(&obstacles),
                channels: Some(channels),
                net_id,
                ..Default::default()
            },
        ),
    };

    net.route = Some(route);
}

/// Build a trunk-tap route from a set of (exit point, exit direction)
///
/// Public for reuse by `BusBundleRouter` —— bus routing is essentially trunk-tap with
/// thick-line styling.
pub fn build_trunk_tap_route<'a>(
    exits: &[((f64, f64), ExitSide)],
    opts: BuildOptions<'a>,
) -> Route {
    let mut route = Route::new();

    if exits.len() < 2 {
        return route;
    }

    // ── Step 1: pin stubs ──
    let stub_ends: Vec<(f64, f64)> = exits
        .iter()
        .map(|(pt, side)| stub_end_of(*pt, *side))
        .collect();

    for (i, (exit_pt, _)) in exits.iter().enumerate() {
        let se = stub_ends[i];
        route.segments.push(Segment {
            from: Point::new(exit_pt.0, exit_pt.1),
            to: Point::new(se.0, se.1),
        });
    }

    // ── Step 2: trunk direction ──
    let n_horiz_exit = exits.iter().filter(|(_, s)| s.is_horizontal()).count();
    let n_vert_exit = exits.len() - n_horiz_exit;

    let min_x = stub_ends.iter().map(|p| p.0).fold(f64::MAX, f64::min);
    let max_x = stub_ends.iter().map(|p| p.0).fold(f64::MIN, f64::max);
    let min_y = stub_ends.iter().map(|p| p.1).fold(f64::MAX, f64::min);
    let max_y = stub_ends.iter().map(|p| p.1).fold(f64::MIN, f64::max);
    let span_x = max_x - min_x;
    let span_y = max_y - min_y;

    // ── Step 2.5: spine detection (Iter 1.5) ──
    // If ≥2 endpoints are collinear with facing exits, pin the trunk to that axis.
    let (trunk_horizontal, pinned_axis) = match detect_spine(exits, &stub_ends) {
        Some((h, axis)) => {
            crate::vlog!(
                "[route::trunk_tap] spine detected: {} @ {axis:.0}",
                if h { "horizontal" } else { "vertical" }
            );
            (h, Some(axis))
        }
        None => {
            // Original majority-vote logic
            let th = match n_vert_exit.cmp(&n_horiz_exit) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => span_x >= span_y,
            };
            (th, None)
        }
    };

    // ── Step 3: trunk position ──
    let trunk_axis = if let Some(axis) = pinned_axis {
        // Spine pinned: use the geometric axis directly, skip channel/obstacle search
        axis
    } else {
        // P10 prefers channels.reserve_*; failed/unavailable → P09 (choose_trunk_axis); fallback → mean
        let n = stub_ends.len() as f64;
        let mean_axis = if trunk_horizontal {
            stub_ends.iter().map(|p| p.1).sum::<f64>() / n
        } else {
            stub_ends.iter().map(|p| p.0).sum::<f64>() / n
        };

        let p09_axis = if let Some(obstacles) = opts.obstacles {
            choose_trunk_axis(&stub_ends, trunk_horizontal, obstacles)
        } else {
            mean_axis
        };

        let net_id = opts.net_id;
        if let Some(channels) = opts.channels {
            let reserved = if trunk_horizontal {
                channels.reserve_horizontal(min_x, max_x, p09_axis, net_id)
            } else {
                channels.reserve_vertical(min_y, max_y, p09_axis, net_id)
            };
            match reserved {
                Some(actual) => {
                    crate::vlog!(
                        "[route::trunk_tap] net_id={net_id} trunk in channel @ {actual:.0} (prefer {p09_axis:.0}, mean {mean_axis:.0})"
                    );
                    actual
                }
                None => {
                    crate::vlog!(
                        "[route::trunk_tap] net_id={net_id} no channel available, fallback to {p09_axis:.0}"
                    );
                    p09_axis
                }
            }
        } else {
            p09_axis
        }
    };

    // ── Step 4: trunk segment ──
    let oh = opts.trunk_overhang;
    let trunk = if trunk_horizontal {
        Segment {
            from: Point::new(min_x - oh, trunk_axis),
            to: Point::new(max_x + oh, trunk_axis),
        }
    } else {
        Segment {
            from: Point::new(trunk_axis, min_y - oh),
            to: Point::new(trunk_axis, max_y + oh),
        }
    };
    route.segments.push(trunk);

    // ── Step 5: tap (stub_end → trunk) + junction ──
    for &(sx, sy) in &stub_ends {
        let trunk_pt = if trunk_horizontal {
            Point::new(sx, trunk_axis)
        } else {
            Point::new(trunk_axis, sy)
        };

        // stub_end is already on the trunk → don't draw redundant tap
        let already_on_trunk = (trunk_pt.x - sx).abs() < 0.5 && (trunk_pt.y - sy).abs() < 0.5;
        if !already_on_trunk {
            route.segments.push(Segment {
                from: Point::new(sx, sy),
                to: trunk_pt,
            });
            // Only push junction when tap lands inside the trunk (not at endpoints).
            // A tap hitting the trunk endpoint is a corner, not a T-junction.
            let at_endpoint = (trunk_pt.x - trunk.from.x).abs() < 0.5
                && (trunk_pt.y - trunk.from.y).abs() < 0.5
                || (trunk_pt.x - trunk.to.x).abs() < 0.5 && (trunk_pt.y - trunk.to.y).abs() < 0.5;
            if !at_endpoint {
                route.junctions.push(trunk_pt);
            }
        }
    }

    route
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Compute the coordinates of an endpoint "after walking PIN_STUB_LEN along the exit direction"
fn stub_end_of(exit: (f64, f64), side: ExitSide) -> (f64, f64) {
    match side {
        ExitSide::Left => (exit.0 - PIN_STUB_LEN, exit.1),
        ExitSide::Right => (exit.0 + PIN_STUB_LEN, exit.1),
        ExitSide::Top => (exit.0, exit.1 - PIN_STUB_LEN),
        ExitSide::Bottom => (exit.0, exit.1 + PIN_STUB_LEN),
    }
}

/// Collect (exit point, exit direction) for all endpoints of a net
fn collect_exits(graph: &McVecGraph, net: &VizNet) -> Vec<((f64, f64), ExitSide)> {
    net.endpoints
        .iter()
        .filter_map(|e| {
            graph
                .boxes
                .iter()
                .find(|b| b.id == e.box_id)
                .map(|b| compute_exit_for_pin(b, e.pin_id, None))
        })
        .collect()
}

/// 2-endpoint net: walk Manhattan, no trunk needed
fn build_two_point_route(exits: &[((f64, f64), ExitSide)]) -> Route {
    let mut route = Route::new();
    let (a, sa) = exits[0];
    let (b, sb) = exits[1];
    let pts = orthogonal_path(a, b, sa, sb);
    for w in pts.windows(2) {
        route.segments.push(Segment {
            from: Point::new(w[0].0, w[0].1),
            to: Point::new(w[1].0, w[1].1),
        });
    }
    route
}

// ============================================================================
// ★ Iter 1.5 — spine detection: geometrically detect the trunk axis from
//    collinear facing pins (e.g. series resistors on a lane)
// ============================================================================

/// Detect if ≥2 endpoints form a collinear spine with facing exit directions.
///
/// When series components (like resistors on a lane) sit on the same horizontal
/// line with their pins facing each other, the trunk should be pinned to that
/// line — no channel search, no obstacle avoidance. This produces a clean
/// T-shaped tap for any vertical stub (e.g. a bridge capacitor).
///
/// Returns `Some((horizontal, axis))` if a spine is detected, `None` otherwise.
fn detect_spine(
    exits: &[((f64, f64), ExitSide)],
    stub_ends: &[(f64, f64)],
) -> Option<(bool /*horizontal*/, f64 /*axis*/)> {
    // ── Horizontal spine: cluster by y ──
    if let Some(y) = spine_on_axis(exits, stub_ends, true) {
        return Some((true, y));
    }
    // ── Vertical spine: cluster by x ──
    if let Some(x) = spine_on_axis(exits, stub_ends, false) {
        return Some((false, x));
    }
    None
}

/// Cluster stub_ends along one axis and check for a spine.
///
/// `horizontal=true` → cluster by y, require horizontal exits, facing Left/Right.
/// `horizontal=false` → cluster by x, require vertical exits, facing Top/Bottom.
fn spine_on_axis(
    exits: &[((f64, f64), ExitSide)],
    stub_ends: &[(f64, f64)],
    horizontal: bool,
) -> Option<f64> {
    let tol = 1.0;

    // Group stub_ends by axis coordinate (y for horizontal, x for vertical)
    let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
    for (i, &(sx, sy)) in stub_ends.iter().enumerate() {
        let coord = if horizontal { sy } else { sx };
        let mut found = false;
        for (g_coord, ref mut indices) in groups.iter_mut() {
            if (coord - *g_coord).abs() <= tol {
                indices.push(i);
                // Recompute centroid
                let sum: f64 = indices
                    .iter()
                    .map(|&j| {
                        if horizontal {
                            stub_ends[j].1
                        } else {
                            stub_ends[j].0
                        }
                    })
                    .sum();
                *g_coord = sum / indices.len() as f64;
                found = true;
                break;
            }
        }
        if !found {
            groups.push((coord, vec![i]));
        }
    }

    // Find the largest cluster with ≥ 2 members
    let best = groups.into_iter().max_by_key(|(_, v)| v.len())?;
    if best.1.len() < 2 {
        return None;
    }
    let (cluster_axis, indices) = best;

    // All exits in this cluster must be along the spine direction
    // (horizontal for horizontal spine, vertical for vertical spine)
    let all_aligned = indices.iter().all(|&i| {
        let (_, side) = exits[i];
        if horizontal {
            side.is_horizontal()
        } else {
            !side.is_horizontal()
        }
    });
    if !all_aligned {
        return None;
    }

    // Check for facing pairs
    if horizontal {
        let right_xs: Vec<f64> = indices
            .iter()
            .filter(|&&i| matches!(exits[i].1, ExitSide::Right))
            .map(|&i| stub_ends[i].0)
            .collect();
        let left_xs: Vec<f64> = indices
            .iter()
            .filter(|&&i| matches!(exits[i].1, ExitSide::Left))
            .map(|&i| stub_ends[i].0)
            .collect();
        if right_xs.is_empty() || left_xs.is_empty() {
            return None;
        }
        let max_right = right_xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_left = left_xs.iter().cloned().fold(f64::INFINITY, f64::min);
        if max_right >= min_left {
            return None; // not facing
        }
    } else {
        let bottom_ys: Vec<f64> = indices
            .iter()
            .filter(|&&i| matches!(exits[i].1, ExitSide::Bottom))
            .map(|&i| stub_ends[i].1)
            .collect();
        let top_ys: Vec<f64> = indices
            .iter()
            .filter(|&&i| matches!(exits[i].1, ExitSide::Top))
            .map(|&i| stub_ends[i].1)
            .collect();
        if bottom_ys.is_empty() || top_ys.is_empty() {
            return None;
        }
        let max_bottom = bottom_ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_top = top_ys.iter().cloned().fold(f64::INFINITY, f64::min);
        if max_bottom >= min_top {
            return None; // not facing
        }
    }

    Some(cluster_axis)
}

// ============================================================================
// ★ P09 (S5) choose_trunk_axis — obstacle-aware trunk position selection
// ============================================================================

/// Given a set of stub_ends + trunk direction + obstacle map, pick the trunk axis coordinate (y or x)
///
/// ## Algorithm
/// Candidates = mean y/x ± several discrete positions near box edges (extracted from obstacles).
/// Score = number of obstacle hits + distance penalty from original mean. Lowest score wins.
///
/// `trunk_horizontal = true`:
///   - axis is y coordinate, trunk spans horizontally (x from stub min_x to stub max_x)
///   - Candidate y = mean, plus each obstacle box's top/bottom ± margin
/// `trunk_horizontal = false`:
///   - axis is x coordinate, trunk spans vertically
fn choose_trunk_axis(
    stub_ends: &[(f64, f64)],
    trunk_horizontal: bool,
    obstacles: &ObstacleMap,
) -> f64 {
    if stub_ends.is_empty() {
        return 0.0;
    }
    let n = stub_ends.len() as f64;
    let mean = if trunk_horizontal {
        stub_ends.iter().map(|p| p.1).sum::<f64>() / n
    } else {
        stub_ends.iter().map(|p| p.0).sum::<f64>() / n
    };

    // trunk span range: [min, max] of stub_ends along the main axis
    let (range_lo, range_hi) = if trunk_horizontal {
        let lo = stub_ends.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let hi = stub_ends
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max);
        (lo, hi)
    } else {
        let lo = stub_ends.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let hi = stub_ends
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        (lo, hi)
    };

    // Candidate positions (mean first, then 8px above/below each obstacle)
    let margin = 8.0;
    let mut candidates: Vec<f64> = vec![mean];
    for r in &obstacles.rects {
        if trunk_horizontal {
            candidates.push(r.y - margin);
            candidates.push(r.bottom() + margin);
        } else {
            candidates.push(r.x - margin);
            candidates.push(r.right() + margin);
        }
    }
    candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    candidates.dedup_by(|a, b| (*a - *b).abs() < 0.1);

    // Score: collision count (heavy) + distance from mean (light)
    let (best, _best_score) = candidates
        .into_iter()
        .map(|axis| {
            let hits = if trunk_horizontal {
                obstacles
                    .rects
                    .iter()
                    .filter(|r| r.intersects_horizontal(axis, range_lo, range_hi))
                    .count()
            } else {
                obstacles
                    .rects
                    .iter()
                    .filter(|r| r.intersects_vertical(axis, range_lo, range_hi))
                    .count()
            };
            let dist_penalty = (axis - mean).abs() * 0.01; // 100px distance = cost of 1 hit
            let score = hits as f64 + dist_penalty;
            (axis, score)
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((mean, 0.0));

    best
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn h_exit(x: f64, y: f64) -> ((f64, f64), ExitSide) {
        ((x, y), ExitSide::Right)
    }

    fn v_exit(x: f64, y: f64) -> ((f64, f64), ExitSide) {
        ((x, y), ExitSide::Top)
    }

    #[test]
    fn test_stub_end_directions() {
        assert_eq!(stub_end_of((100.0, 50.0), ExitSide::Right), (110.0, 50.0));
        assert_eq!(stub_end_of((100.0, 50.0), ExitSide::Left), (90.0, 50.0));
        assert_eq!(stub_end_of((100.0, 50.0), ExitSide::Top), (100.0, 40.0));
        assert_eq!(stub_end_of((100.0, 50.0), ExitSide::Bottom), (100.0, 60.0));
    }

    #[test]
    fn test_3_horizontal_exits_make_vertical_trunk() {
        // 3 pins all exit right, trunk should be vertical, on their right side
        let exits = vec![
            h_exit(100.0, 50.0),
            h_exit(100.0, 100.0),
            h_exit(100.0, 150.0),
        ];
        let r = build_trunk_tap_route(&exits, BuildOptions::default());

        // Stubs: 3 segments + trunk: 1 segment + taps: ≤ 3 segments
        assert!(r.segments.len() >= 4);

        // All stub_ends are on the trunk → no taps, no junctions
        assert_eq!(r.junctions.len(), 0);

        // trunk should be vertical (from.x == to.x), at the x mean of stub_ends
        // stub_ends all at x=110.0, so trunk_x = 110.0
        let trunk = &r.segments[3]; // [stub0, stub1, stub2, trunk, ...]
        assert!((trunk.from.x - trunk.to.x).abs() < 0.5); // vertical
        assert!((trunk.from.x - 110.0).abs() < 0.5);
    }

    #[test]
    fn test_3_vertical_exits_make_horizontal_trunk() {
        // 3 pins all exit from the top, trunk should be horizontal, above them
        let exits = vec![
            v_exit(50.0, 100.0),
            v_exit(150.0, 100.0),
            v_exit(250.0, 100.0),
        ];
        let r = build_trunk_tap_route(&exits, BuildOptions::default());

        // trunk horizontal, y = 90 (100 - PIN_STUB_LEN, all stub_ends at y=90)
        let trunk = &r.segments[3];
        assert!((trunk.from.y - trunk.to.y).abs() < 0.5);
        assert!((trunk.from.y - 90.0).abs() < 0.5);
    }

    #[test]
    fn test_trunk_overhang_extends_bus_trunk() {
        let exits = vec![
            v_exit(100.0, 200.0),
            v_exit(200.0, 200.0),
            v_exit(300.0, 200.0),
        ];
        let r = build_trunk_tap_route(
            &exits,
            BuildOptions {
                trunk_overhang: 12.0,
                ..Default::default()
            },
        );

        let trunk = &r.segments[3];
        // trunk spans stub_ends.x ∈ [100, 300], + overhang 12 each side
        let (tmin, tmax) = if trunk.from.x < trunk.to.x {
            (trunk.from.x, trunk.to.x)
        } else {
            (trunk.to.x, trunk.from.x)
        };
        assert!((tmin - 88.0).abs() < 0.5); // 100 - 12
        assert!((tmax - 312.0).abs() < 0.5); // 300 + 12
    }

    #[test]
    fn test_two_point_uses_manhattan_not_trunk() {
        let exits = vec![h_exit(100.0, 50.0), h_exit(300.0, 150.0)];
        let r = build_two_point_route(&exits);
        // Manhattan L-shape (1 bend) or Z-shape (2 bends), should not have a trunk
        // Should only have 2-3 segments
        assert!(r.segments.len() <= 3);
        // No junction (2 endpoints have no T node)
        assert_eq!(r.junctions.len(), 0);
    }

    #[test]
    fn test_stub_segments_emerge_from_pins() {
        // One pin exits right: stub should be (px, py) → (px + STUB_LEN, py)
        let exits = vec![h_exit(50.0, 50.0), h_exit(50.0, 100.0), h_exit(50.0, 150.0)];
        let r = build_trunk_tap_route(&exits, BuildOptions::default());

        // First 3 segments are stubs, check the 1st
        let stub0 = &r.segments[0];
        assert_eq!(stub0.from.x, 50.0);
        assert_eq!(stub0.from.y, 50.0);
        assert_eq!(stub0.to.x, 60.0); // 50 + PIN_STUB_LEN
        assert_eq!(stub0.to.y, 50.0); // y unchanged (exits right)
    }

    // ========================================================================
    // ★ P09 (S5) choose_trunk_axis tests
    // ========================================================================

    use super::super::obstacles::{ObstacleMap, Rect};

    #[test]
    fn p09_trunk_axis_picks_mean_when_no_obstacles() {
        let stub_ends = vec![(0.0, 100.0), (200.0, 110.0), (400.0, 120.0)];
        let empty = ObstacleMap::empty();
        let axis = choose_trunk_axis(&stub_ends, true, &empty);
        // mean 110, no obstacles → should pick 110
        assert!((axis - 110.0).abs() < 0.5);
    }

    #[test]
    fn p09_trunk_axis_avoids_obstacle_at_mean() {
        // stub_ends average y = 100, deliberately place a box covering y=100 range
        let stub_ends = vec![(0.0, 90.0), (200.0, 100.0), (400.0, 110.0)];
        let mut obstacles = ObstacleMap::empty();
        obstacles.rects.push(Rect {
            x: 50.0,
            y: 95.0,
            w: 300.0,
            h: 20.0, // covers y in [95, 115]
        });

        let axis = choose_trunk_axis(&stub_ends, true, &obstacles);
        // Should not pick mean (100) because it's inside the [95, 115] obstacle
        // Should pick 95 - 8 = 87 or 115 + 8 = 123
        let is_above = (axis - 87.0).abs() < 1.0;
        let is_below = (axis - 123.0).abs() < 1.0;
        assert!(
            is_above || is_below,
            "axis {} should be 87 (above) or 123 (below)",
            axis
        );
    }

    #[test]
    fn p09_trunk_horizontal_avoids_box_horizontally() {
        // Multi-endpoint net spans, box in the middle, trunk should avoid
        let stub_ends = vec![(0.0, 50.0), (200.0, 50.0), (400.0, 50.0)];
        let mut obstacles = ObstacleMap::empty();
        // at y=50, there's a box in x in [150, 250]
        obstacles.rects.push(Rect {
            x: 150.0,
            y: 40.0,
            w: 100.0,
            h: 20.0,
        });

        let axis = choose_trunk_axis(&stub_ends, true, &obstacles);
        // Should not be in [40, 60] range (would hit)
        assert!(axis < 40.0 || axis > 60.0, "axis {} hits obstacle", axis);
    }

    #[test]
    fn p09_trunk_vertical_uses_x_axis() {
        // trunk_horizontal=false → pick x coordinate
        let stub_ends = vec![(100.0, 0.0), (110.0, 100.0), (90.0, 200.0)];
        let empty = ObstacleMap::empty();
        let axis = choose_trunk_axis(&stub_ends, false, &empty);
        // x mean 100
        assert!((axis - 100.0).abs() < 0.5);
    }

    #[test]
    fn p09_route_avoids_box_between_endpoints() {
        // End-to-end: trunk_tap should avoid box in the middle
        // 4 endpoints all exit right (h_exit), trunk should be vertical
        // but we also place a box to make the horizontal trunk want to avoid
        // This test mainly ensures route doesn't panic + path is reasonable
        let exits = vec![h_exit(0.0, 100.0), h_exit(0.0, 150.0), h_exit(0.0, 200.0)];
        let mut obstacles = ObstacleMap::empty();
        obstacles.rects.push(Rect {
            x: 5.0,
            y: 140.0,
            w: 20.0,
            h: 30.0,
        });

        let r = build_trunk_tap_route(
            &exits,
            BuildOptions {
                trunk_overhang: 0.0,
                obstacles: Some(&obstacles),
                ..Default::default()
            },
        );
        // Should at least have stubs + trunk
        assert!(!r.segments.is_empty());
        // P09 shifts trunk away from obstacle → middle tap lands inside trunk → 1 junction
        assert_eq!(r.junctions.len(), 1);
    }

    // ========================================================================
    // ★ Iter 1.5 — spine detection tests
    // ========================================================================

    fn l_exit(x: f64, y: f64) -> ((f64, f64), ExitSide) {
        ((x, y), ExitSide::Left)
    }
    fn b_exit(x: f64, y: f64) -> ((f64, f64), ExitSide) {
        ((x, y), ExitSide::Bottom)
    }

    #[test]
    fn spine_horizontal_two_facing_pins() {
        // Simulates net_1: RES1.2 exits Right, RES3.1 exits Left (facing),
        // CAP2.1 exits Top (vertical stub). Spine should be horizontal at y=71.
        let exits = vec![
            h_exit(445.0, 71.0), // RES1.2: Right exit, x=445, y=71
            l_exit(505.0, 71.0), // RES3.1: Left exit,  x=505, y=71
            v_exit(420.0, 60.3), // CAP2.1: Top exit,   x=420, y=60.3
        ];
        let r = build_trunk_tap_route(&exits, BuildOptions::default());

        // Stubs: 3 segments + trunk: 1 segment + tap (CAP2.1): 1 segment = 5
        assert_eq!(r.segments.len(), 5);

        // CAP2.1's tap lands at trunk endpoint (x=420 = min_x) → corner, not T-junction.
        // (Iter 3 column grid will move CAP2 inside the trunk, producing a true T.)
        assert_eq!(r.junctions.len(), 0);

        // Trunk should be horizontal
        let trunk = &r.segments[3]; // [stub0, stub1, stub2, trunk, ...]
        assert!((trunk.from.y - trunk.to.y).abs() < 0.5); // horizontal
                                                          // Trunk axis at y=71
        assert!((trunk.from.y - 71.0).abs() < 0.5);
    }

    #[test]
    fn ic_fanout_still_vertical() {
        // IC fanout: 8 pins all exiting Right from the same x, different y.
        // No spine should be detected (all exits same direction, no facing pairs).
        // Falls back to majority vote → vertical trunk. Behavior unchanged.
        let exits: Vec<((f64, f64), ExitSide)> = (0..8)
            .map(|i| h_exit(110.0, 50.0 + i as f64 * 50.0))
            .collect();
        let r = build_trunk_tap_route(&exits, BuildOptions::default());

        // 8 stubs + trunk = 9 segments (all stub_ends on trunk → no taps)
        assert_eq!(r.segments.len(), 9);
        assert_eq!(r.junctions.len(), 0);

        // Trunk should be vertical
        let trunk = &r.segments[8]; // last segment is trunk
        assert!((trunk.from.x - trunk.to.x).abs() < 0.5); // vertical
        assert!((trunk.from.x - 120.0).abs() < 0.5); // 110 + PIN_STUB_LEN
    }
}
