// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW (P09, S5) — Obstacle-Aware routing infrastructure
//!
//! ## What problem does this file solve
//! Before S4, all routers (orthogonal / star / trunk_tap) assumed the canvas is
//! empty, directly drawing L-shape / 3-segment polylines between two points;
//! **lines passing through other boxes** was a daily occurrence.
//!
//! P09 introduces [`ObstacleMap`]: records the rectangular areas of all boxes on
//! the canvas, and the router checks this map when selecting polylines, detouring
//! the entire path if it hits a box.
//!
//! ## Design trade-offs
//! - **Don't change Router trait signature** —— trait is still `route(&graph, &mut net)`.
//!   Each router builds its own `ObstacleMap::from_graph(graph, exclude)` internally.
//!   Slight redundancy (one layer N nets rebuilds N times, O(N*M)).
//!   200 nets * 200 boxes = 40k rect constructions, microsecond level, no optimization.
//! - **Obstacle avoidance doesn't seek shortest**: on collision, 4-direction detour,
//!   pick the first non-colliding. Full grid A* is P10 (channel routing)'s job.
//! - **Only handles orthogonal segments** (axis-aligned). Diagonals use bbox test
//!   for rough judgement.
//!   All current routers output Manhattan paths, so no more refined judgement is needed.
//!
//! ## Data model
//! ```text
//!                 ┌─────────────────────┐
//!                 │  ObstacleMap        │
//!                 │                     │
//!                 │  rects: Vec<Rect>   │  ← inflated rectangles of all boxes
//!                 │  exclude_ids        │  ← boxes excluded when routing this net
//!                 │                     │
//!                 │  inflate: f64       │  ← box edge push-out distance (avoid grazing)
//!                 └─────────────────────┘
//! ```
//!
//! ## Usage
//! ```ignore
//! impl Router for OrthogonalRouter {
//!     fn route(&self, graph: &McVecGraph, net: &mut VizNet) {
//!         // Exclude this net's endpoint boxes (they are the routing start/end, not obstacles)
//!         let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
//!         let obstacles = ObstacleMap::from_graph(graph, 10.0, &exclude);
//!
//!         // Check obstacles when picking paths
//!         for path in candidates {
//!             if obstacles.first_hit(&path).is_none() { return path; }
//!         }
//!         // All collide, detour
//!         detour_around(start, end, &obstacles)
//!     }
//! }
//! ```
//!
//! ## Extensions after P10
//! Currently `ObstacleMap` only holds box rectangles. P10 channel routing will
//! introduce "already-reserved trunk segments" as obstacles too; at that point replace
//! [`Rect`] with an enum (Box / Trunk / Forbidden zone).

use std::collections::HashSet;

use crate::vector::graph::{McVecBox, McVecGraph};

// ============================================================================
// Rect basic geometry
// ============================================================================

/// Axis-aligned rectangle (a rectangular obstacle area on the canvas)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// Build Rect from a box, edges pushed out by `inflate` pixels
    ///
    /// `inflate > 0` keeps the path from grazing the box edge (visual padding).
    /// Recommend 8-12px.
    pub fn from_box(b: &McVecBox, inflate: f64) -> Rect {
        Rect {
            x: b.x - inflate,
            y: b.y - inflate,
            w: b.w + 2.0 * inflate,
            h: b.h + 2.0 * inflate,
        }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    /// Whether a point is inside the rectangle (including boundary)
    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }

    /// Whether horizontal segment `(x1, y) → (x2, y)` passes through this rect
    pub fn intersects_horizontal(&self, y: f64, x1: f64, x2: f64) -> bool {
        if y < self.y || y > self.bottom() {
            return false;
        }
        let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
        // Intervals overlap (including boundary)
        hi >= self.x && lo <= self.right()
    }

    /// Whether vertical segment `(x, y1) → (x, y2)` passes through this rect
    pub fn intersects_vertical(&self, x: f64, y1: f64, y2: f64) -> bool {
        if x < self.x || x > self.right() {
            return false;
        }
        let (lo, hi) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
        hi >= self.y && lo <= self.bottom()
    }

    /// Whether any segment `(sx,sy) → (tx,ty)` intersects this rect
    ///
    /// Currently the router only produces orthogonal segments (horizontal/vertical),
    /// diagonals use bbox overlap for rough judgement.
    pub fn intersects_segment(&self, sx: f64, sy: f64, tx: f64, ty: f64) -> bool {
        const EPS: f64 = 0.1;
        if (sx - tx).abs() < EPS {
            // Vertical
            self.intersects_vertical(sx, sy, ty)
        } else if (sy - ty).abs() < EPS {
            // Horizontal
            self.intersects_horizontal(sy, sx, tx)
        } else {
            // Diagonal: bbox test (rough judgement, router known not to produce diagonals)
            let lx = sx.min(tx);
            let rx = sx.max(tx);
            let ty_lo = sy.min(ty);
            let ty_hi = sy.max(ty);
            !(rx < self.x || lx > self.right() || ty_hi < self.y || ty_lo > self.bottom())
        }
    }
}

// ============================================================================
// ObstacleMap
// ============================================================================

/// Obstacle map for one graph layer (inflated rectangles of all non-excluded boxes)
#[derive(Debug, Clone)]
pub struct ObstacleMap {
    /// All obstacle rectangles
    pub rects: Vec<Rect>,
    /// Set of excluded box IDs (routing sources/sinks)
    pub exclude_ids: HashSet<i64>,
    /// Inflate value used at construction (kept for debug)
    pub inflate: f64,
}

impl ObstacleMap {
    /// Empty map (no obstacles, equivalent to pre-P09 behavior)
    pub fn empty() -> Self {
        Self {
            rects: Vec::new(),
            exclude_ids: HashSet::new(),
            inflate: 0.0,
        }
    }

    /// Build from graph: exclude specified boxes, all others inflated by `inflate` pixels as obstacles
    ///
    /// ## Exclusion rule
    /// When routing a net, all endpoint boxes of that net should be excluded
    /// (they are the path start/end and should not be obstacles, otherwise the
    /// router can never "approach its own endpoints").
    pub fn from_graph(graph: &McVecGraph, inflate: f64, exclude: &[i64]) -> Self {
        let exclude_ids: HashSet<i64> = exclude.iter().copied().collect();
        let rects = graph
            .boxes
            .iter()
            .filter(|b| !exclude_ids.contains(&b.id))
            .map(|b| Rect::from_box(b, inflate))
            .collect();
        Self {
            rects,
            exclude_ids,
            inflate,
        }
    }

    /// For a polyline (`Vec<(x1,y1,x2,y2)>`), return the first rect hit (sequential scan)
    pub fn first_hit(&self, segments: &[(f64, f64, f64, f64)]) -> Option<&Rect> {
        for seg in segments {
            for r in &self.rects {
                if r.intersects_segment(seg.0, seg.1, seg.2, seg.3) {
                    return Some(r);
                }
            }
        }
        None
    }

    /// At which y is a horizontal line, check whether the whole segment hits an obstacle
    pub fn any_hits_horizontal(&self, y: f64, x1: f64, x2: f64) -> bool {
        self.rects
            .iter()
            .any(|r| r.intersects_horizontal(y, x1, x2))
    }

    /// At which x is a vertical line, check whether the whole segment hits an obstacle
    pub fn any_hits_vertical(&self, x: f64, y1: f64, y2: f64) -> bool {
        self.rects.iter().any(|r| r.intersects_vertical(x, y1, y2))
    }

    /// Whether a point is inside any obstacle (for StarRouter's hub site selection)
    pub fn point_inside_any(&self, px: f64, py: f64) -> bool {
        self.rects.iter().any(|r| r.contains_point(px, py))
    }
}

// ============================================================================
// Candidate path generation + scoring (for router to call)
// ============================================================================

/// A polyline segment (4-tuple form: from_x, from_y, to_x, to_y)
pub type Seg = (f64, f64, f64, f64);

/// Generate multiple candidate L/Z-shape paths: given start/end + "natural exit
/// direction" at both ends
///
/// Outputs multiple candidate paths, caller uses `score_path` or `obstacles.first_hit`
/// to pick the best.
pub fn candidate_paths_orthogonal(sx: f64, sy: f64, tx: f64, ty: f64) -> Vec<Vec<Seg>> {
    let mut out = Vec::with_capacity(4);
    // 1. H-V-H Z shape (midpoint x at (sx+tx)/2)
    let mid_x = (sx + tx) / 2.0;
    out.push(vec![
        (sx, sy, mid_x, sy),
        (mid_x, sy, mid_x, ty),
        (mid_x, ty, tx, ty),
    ]);
    // 2. V-H-V Z shape (midpoint y at (sy+ty)/2)
    let mid_y = (sy + ty) / 2.0;
    out.push(vec![
        (sx, sy, sx, mid_y),
        (sx, mid_y, tx, mid_y),
        (tx, mid_y, tx, ty),
    ]);
    // 3. L shape (horizontal first, then vertical)
    out.push(vec![(sx, sy, tx, sy), (tx, sy, tx, ty)]);
    // 4. L shape (vertical first, then horizontal)
    out.push(vec![(sx, sy, sx, ty), (sx, ty, tx, ty)]);
    out
}

/// Compute total length of a polyline
pub fn total_length(segs: &[Seg]) -> f64 {
    segs.iter()
        .map(|(x1, y1, x2, y2)| (x2 - x1).abs() + (y2 - y1).abs())
        .sum()
}

/// Score a polyline: hit obstacle = heavy penalty, then shorter is better, fewer
/// turns is better
///
/// Lower is better. Obstacle-hit weight 1000 is far greater than path length,
/// ensuring non-collision paths are always preferred.
pub fn score_path(segs: &[Seg], obstacles: &ObstacleMap) -> f64 {
    let collision = if obstacles.first_hit(segs).is_some() {
        1000.0
    } else {
        0.0
    };
    let length = total_length(segs);
    let turns = segs.len() as f64;
    collision + length + turns * 20.0
}

/// Pick the lowest-scoring among 4 candidate L/Z-shapes; if all hit obstacles, fall
/// back to [`detour_around`]
pub fn best_orthogonal_path(
    sx: f64,
    sy: f64,
    tx: f64,
    ty: f64,
    obstacles: &ObstacleMap,
) -> Vec<Seg> {
    let candidates = candidate_paths_orthogonal(sx, sy, tx, ty);
    let scored: Vec<(f64, Vec<Seg>)> = candidates
        .into_iter()
        .map(|p| (score_path(&p, obstacles), p))
        .collect();
    let (best_score, best) = scored
        .into_iter()
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap();

    // Still colliding → detour
    if best_score >= 1000.0 {
        return detour_around(sx, sy, tx, ty, obstacles);
    }
    best
}

/// Detour on collision: find the first hit rect, try 4 directions (up/down/left/right) to go around
///
/// Pick the first shortest path that doesn't hit another rect. All fail → fall back
/// to shortest (allow collision, better than no line).
pub fn detour_around(sx: f64, sy: f64, tx: f64, ty: f64, obstacles: &ObstacleMap) -> Vec<Seg> {
    // 0) If a direct candidate is clean, use it directly (shortest preferred)
    let direct = candidate_paths_orthogonal(sx, sy, tx, ty);
    for p in &direct {
        if obstacles.first_hit(p).is_none() {
            return p.clone();
        }
    }

    // 1) ★ Scan "clean corridor": step outward perpendicular to the main axis, find
    //    the first 3-segment detour channel that **doesn't hit anything**. This can
    //    bypass a whole cluster of boxes (not just the first blocker), and also
    //    handles vertical nets (the old logic had zero-displacement, invalid
    //    up/down detour for vertical nets).
    let dx = (tx - sx).abs();
    let dy = (ty - sy).abs();
    const STEP: f64 = 16.0;
    const MAX_RING: i32 = 80; // Scan up to ~1280px, enough to cross the middle band

    if dy >= dx {
        // Vertical-dominant → find a clean **vertical corridor** x = cx:
        //   (sx,sy) → (cx,sy) → (cx,ty) → (tx,ty)
        let center = (sx + tx) / 2.0;
        for k in 0..=MAX_RING {
            for cx in corridor_offsets(center, k as f64 * STEP) {
                let path = vec![(sx, sy, cx, sy), (cx, sy, cx, ty), (cx, ty, tx, ty)];
                if obstacles.first_hit(&path).is_none() {
                    return path;
                }
            }
        }
    } else {
        // Horizontal-dominant → find a clean **horizontal corridor** y = cy:
        //   (sx,sy) → (sx,cy) → (tx,cy) → (tx,ty)
        let center = (sy + ty) / 2.0;
        for k in 0..=MAX_RING {
            for cy in corridor_offsets(center, k as f64 * STEP) {
                let path = vec![(sx, sy, sx, cy), (sx, cy, tx, cy), (tx, cy, tx, ty)];
                if obstacles.first_hit(&path).is_none() {
                    return path;
                }
            }
        }
    }

    // 2) All blocked, no clean corridor found → fall back to shortest direct
    //    (at least there is a line, better than no line)
    direct
        .into_iter()
        .min_by(|a, b| {
            total_length(a)
                .partial_cmp(&total_length(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap()
}

/// Corridor candidate offsets: `offset == 0` → `[center]`; otherwise symmetric
/// outward expansion `[center-offset, center+offset]`
fn corridor_offsets(center: f64, offset: f64) -> Vec<f64> {
    if offset == 0.0 {
        vec![center]
    } else {
        vec![center - offset, center + offset]
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary};

    fn mk_box(id: i64, x: f64, y: f64, w: f64, h: f64) -> McVecBox {
        let mut b = McVecBox::new(
            id,
            format!("box{}", id),
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
    // Rect geometry
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn rect_contains_point_basic() {
        let r = Rect {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
        };
        assert!(r.contains_point(15.0, 15.0));
        assert!(r.contains_point(10.0, 10.0)); // boundary inclusive
        assert!(r.contains_point(30.0, 30.0)); // boundary inclusive
        assert!(!r.contains_point(5.0, 5.0));
        assert!(!r.contains_point(35.0, 35.0));
    }

    #[test]
    fn rect_horizontal_through_middle() {
        let r = Rect {
            x: 50.0,
            y: 50.0,
            w: 100.0,
            h: 50.0,
        };
        assert!(r.intersects_horizontal(75.0, 0.0, 200.0));
        // above
        assert!(!r.intersects_horizontal(20.0, 0.0, 200.0));
        // to the right
        assert!(!r.intersects_horizontal(75.0, 200.0, 300.0));
        // to the left
        assert!(!r.intersects_horizontal(75.0, -100.0, 0.0));
    }

    #[test]
    fn rect_vertical_through_middle() {
        let r = Rect {
            x: 50.0,
            y: 50.0,
            w: 50.0,
            h: 100.0,
        };
        assert!(r.intersects_vertical(75.0, 0.0, 200.0));
        assert!(!r.intersects_vertical(20.0, 0.0, 200.0));
        assert!(!r.intersects_vertical(75.0, 200.0, 300.0));
    }

    #[test]
    fn rect_segment_axis_aligned() {
        let r = Rect {
            x: 100.0,
            y: 100.0,
            w: 50.0,
            h: 50.0,
        };
        // horizontal pass-through
        assert!(r.intersects_segment(50.0, 125.0, 200.0, 125.0));
        // vertical pass-through
        assert!(r.intersects_segment(125.0, 50.0, 125.0, 200.0));
        // no pass-through
        assert!(!r.intersects_segment(0.0, 0.0, 50.0, 50.0));
    }

    // ────────────────────────────────────────────────────────────────────────
    // ObstacleMap
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn obstacle_map_excludes_endpoints() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 0.0, 0.0, 100.0, 100.0));
        g.boxes.push(mk_box(2, 200.0, 0.0, 100.0, 100.0));
        g.boxes.push(mk_box(3, 100.0, 50.0, 50.0, 50.0)); // middle obstacle

        // Exclude endpoints 1 + 2, only 3 is an obstacle
        let om = ObstacleMap::from_graph(&g, 0.0, &[1, 2]);
        assert_eq!(om.rects.len(), 1);
        assert_eq!(om.rects[0].x, 100.0);
    }

    #[test]
    fn obstacle_map_inflate_works() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 100.0, 100.0, 50.0, 50.0));
        let om = ObstacleMap::from_graph(&g, 10.0, &[]);
        let r = &om.rects[0];
        // box was (100,100,50,50) → inflated rect (90, 90, 70, 70)
        assert_eq!(r.x, 90.0);
        assert_eq!(r.y, 90.0);
        assert_eq!(r.w, 70.0);
        assert_eq!(r.h, 70.0);
    }

    #[test]
    fn empty_obstacle_map_never_hits() {
        let om = ObstacleMap::empty();
        let path = vec![(0.0, 0.0, 1000.0, 1000.0)];
        assert!(om.first_hit(&path).is_none());
    }

    // ────────────────────────────────────────────────────────────────────────
    // Candidate paths / detour
    // ────────────────────────────────────────────────────────────────────────

    #[test]
    fn candidate_paths_emits_four() {
        let cands = candidate_paths_orthogonal(0.0, 0.0, 200.0, 100.0);
        assert_eq!(cands.len(), 4);
        // Each is axis-aligned
        for path in &cands {
            for &(x1, y1, x2, y2) in path {
                let dx_zero = (x1 - x2).abs() < 0.1;
                let dy_zero = (y1 - y2).abs() < 0.1;
                assert!(
                    dx_zero || dy_zero,
                    "segment ({},{} → {},{}) is not axis-aligned",
                    x1,
                    y1,
                    x2,
                    y2
                );
            }
        }
    }

    #[test]
    fn best_path_picks_unobstructed() {
        // box1 at (0,0)-(100,100), box2 at (300,0)-(400,100), middle box3 at (150,40)-(200,60)
        // direct (50,50) → (350,50) would pass through box3
        // best_orthogonal_path should detour
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(3, 150.0, 40.0, 50.0, 20.0)); // middle obstacle
        let om = ObstacleMap::from_graph(&g, 5.0, &[]);

        let path = best_orthogonal_path(50.0, 50.0, 350.0, 50.0, &om);
        // Verify no collision with box3
        assert!(
            om.first_hit(&path).is_none(),
            "best path should not hit obstacle"
        );
    }

    #[test]
    fn detour_avoids_blocker() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 100.0, 50.0, 50.0, 50.0));
        let om = ObstacleMap::from_graph(&g, 0.0, &[]);

        let path = detour_around(0.0, 75.0, 200.0, 75.0, &om);
        assert!(
            om.first_hit(&path).is_none(),
            "detour should produce clean path"
        );
        // At least one segment y is above or below the blocker
        let off_axis = path.iter().any(|&(_, y1, _, y2)| y1 < 50.0 || y2 > 100.0);
        assert!(off_axis, "detour should leave the y=75 axis");
    }

    #[test]
    fn no_obstacle_picks_shortest_candidate() {
        let om = ObstacleMap::empty();
        let path = best_orthogonal_path(0.0, 0.0, 100.0, 50.0, &om);
        // Any candidate is fine, but shouldn't be empty
        assert!(!path.is_empty());
    }

    #[test]
    fn point_inside_any_detects_correctly() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 100.0, 100.0, 50.0, 50.0));
        let om = ObstacleMap::from_graph(&g, 0.0, &[]);
        assert!(om.point_inside_any(125.0, 125.0));
        assert!(!om.point_inside_any(50.0, 50.0));
    }
}
