// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Post-routing processing: at cross-net "real cross" points, insert a small
//! upward semicircle bump (jumper/bridge) on the **horizontal line**, so a reader
//! can tell at a glance that "these two just cross, they're not connected" —— this
//! is the standard way to handle unavoidable crossings in industrial-grade schematics.
//!
//! Self-contained: only modifies `route.segments` (replaces the straight crossing
//! with a polyline approximation of "straight → semicircle → straight"), existing
//! renderer (per-net path drawing) still works, **no need to touch the render orchestrator**.
//!
//! Hook: call `wire_hops::apply_wire_hops(&mut graph);` once between api.rs Phase 2
//! (route) and Phase 3 (render).

use crate::vector::graph::{McVecGraph, Point, Segment};
use std::collections::HashMap;
use std::f64::consts::PI;

const HOP_R: f64 = 5.0; // Bump radius (too big looks clumsy, too small hard to see; 5 is good)
const EPS: f64 = 0.5;
const ARC_STEPS: usize = 6; // How many polyline segments to approximate a semicircle (6 is smooth enough)

/// Entry: add wire hops at cross-net cross points of all nets in current layer.
pub fn apply_wire_hops(graph: &mut McVecGraph) {
    // 1. Flatten all segments: (net_idx, seg_idx, Segment copy)
    let mut segs: Vec<(usize, usize, Segment)> = Vec::new();
    for (ni, net) in graph.nets.iter().enumerate() {
        if let Some(r) = &net.route {
            for (si, s) in r.segments.iter().enumerate() {
                segs.push((ni, si, *s));
            }
        }
    }

    // 2. Find cross-net real crosses, record hop points on the **horizontal** segment
    let mut hops: HashMap<(usize, usize), Vec<f64>> = HashMap::new();
    for a in 0..segs.len() {
        for b in (a + 1)..segs.len() {
            if segs[a].0 == segs[b].0 {
                continue; // Same net: no hop
            }
            if let Some((px, h_is_a)) = cross_x(&segs[a].2, &segs[b].2) {
                let key = if h_is_a {
                    (segs[a].0, segs[a].1)
                } else {
                    (segs[b].0, segs[b].1)
                };
                hops.entry(key).or_default().push(px);
            }
        }
    }
    if hops.is_empty() {
        return;
    }

    // 3. Rebuild segments per net: split horizontal segments with hops and insert bumps, others unchanged
    for (ni, net) in graph.nets.iter_mut().enumerate() {
        let route = match &mut net.route {
            Some(r) => r,
            None => continue,
        };
        let mut rebuilt: Vec<Segment> = Vec::with_capacity(route.segments.len() + 4);
        for (si, s) in route.segments.iter().enumerate() {
            match hops.get(&(ni, si)) {
                Some(xs) => emit_with_hops(s, xs, &mut rebuilt),
                None => rebuilt.push(*s),
            }
        }
        route.segments = rebuilt;
    }
}

/// Whether two segments are real crosses (strict interior, excluding endpoint T-junctions).
/// Returns (cross point x, whether the first segment is the horizontal one). Only H×V processed.
fn cross_x(a: &Segment, b: &Segment) -> Option<(f64, bool)> {
    let a_h = (a.from.y - a.to.y).abs() < EPS;
    let a_v = (a.from.x - a.to.x).abs() < EPS;
    let b_h = (b.from.y - b.to.y).abs() < EPS;
    let b_v = (b.from.x - b.to.x).abs() < EPS;
    let (h, v, h_is_a) = if a_h && b_v {
        (a, b, true)
    } else if a_v && b_h {
        (b, a, false)
    } else {
        return None; // Parallel / diagonal / degenerate, skip
    };
    let hy = h.from.y;
    let (hx0, hx1) = (h.from.x.min(h.to.x), h.from.x.max(h.to.x));
    let vx = v.from.x;
    let (vy0, vy1) = (v.from.y.min(v.to.y), v.from.y.max(v.to.y));
    // Strict interior: neither segment touches at their own endpoints, only then is it a "crossing"
    // (endpoint T-junction doesn't draw a hop)
    if vx > hx0 + EPS && vx < hx1 - EPS && hy > vy0 + EPS && hy < vy1 - EPS {
        Some((vx, h_is_a))
    } else {
        None
    }
}

/// Split a horizontal segment by hop points (x coords), inserting an upward semicircle
/// bump at each hop.
fn emit_with_hops(seg: &Segment, xs_in: &[f64], out: &mut Vec<Segment>) {
    // Only horizontal segments; defensive: non-horizontal passes through unchanged
    if (seg.from.y - seg.to.y).abs() >= EPS {
        out.push(*seg);
        return;
    }
    let y = seg.from.y;
    let reversed = seg.from.x > seg.to.x;
    let dir = if reversed { -1.0 } else { 1.0 };

    // Sort along the wire direction + drop ones too close (prevent bumps overlapping)
    let mut xs: Vec<f64> = xs_in.to_vec();
    if reversed {
        xs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    }
    let mut kept: Vec<f64> = Vec::new();
    for &x in &xs {
        if kept
            .last()
            .is_none_or(|&last| (x - last).abs() >= 2.0 * HOP_R + 1.0)
        {
            kept.push(x);
        }
    }

    let mut cur = seg.from.x;
    for &hx in &kept {
        push_line(out, cur, y, hx - dir * HOP_R, y); // straight to bump entry
        push_arc(out, hx, y, dir); // semicircle bump
        cur = hx + dir * HOP_R;
    }
    push_line(out, cur, y, seg.to.x, y); // closing straight
}

fn push_line(out: &mut Vec<Segment>, x0: f64, y0: f64, x1: f64, y1: f64) {
    if (x0 - x1).abs() < 1e-6 && (y0 - y1).abs() < 1e-6 {
        return; // skip zero-length segment
    }
    out.push(Segment {
        from: Point::new(x0, y0),
        to: Point::new(x1, y1),
    });
}

/// Draw an upward semicircle at (cx, y): entry (cx - dir·R, y), apex (cx, y-R),
/// exit (cx + dir·R, y).
fn push_arc(out: &mut Vec<Segment>, cx: f64, y: f64, dir: f64) {
    let start_theta = if dir > 0.0 { PI } else { 0.0 };
    let mut prev = Point::new(
        cx + HOP_R * start_theta.cos(),
        y - HOP_R * start_theta.sin(),
    );
    for k in 1..=ARC_STEPS {
        let t = k as f64 / ARC_STEPS as f64;
        let theta = if dir > 0.0 { PI * (1.0 - t) } else { PI * t };
        let p = Point::new(cx + HOP_R * theta.cos(), y - HOP_R * theta.sin());
        out.push(Segment { from: prev, to: p });
        prev = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment {
            from: Point::new(x0, y0),
            to: Point::new(x1, y1),
        }
    }

    #[test]
    fn detects_true_cross() {
        // Horizontal (0,10)-(20,10) × vertical (10,0)-(10,20) → crosses at (10,10), a is horizontal
        let r = cross_x(&seg(0.0, 10.0, 20.0, 10.0), &seg(10.0, 0.0, 10.0, 20.0));
        assert_eq!(r, Some((10.0, true)));
    }

    #[test]
    fn endpoint_t_not_crossed() {
        // Vertical endpoint lands on horizontal line (T-junction) → don't draw hop
        let r = cross_x(&seg(0.0, 10.0, 20.0, 10.0), &seg(10.0, 10.0, 10.0, 20.0));
        assert_eq!(r, None);
    }

    #[test]
    fn hop_expands_segment() {
        // One horizontal segment, one hop in the middle → split into multiple
        // segments (straight + arc + straight)
        let mut out = Vec::new();
        emit_with_hops(&seg(0.0, 10.0, 20.0, 10.0), &[10.0], &mut out);
        assert!(out.len() > 2); // at least front-straight + several arcs + back-straight
                                // First segment starts at x=0
        assert!((out.first().unwrap().from.x - 0.0).abs() < 1e-6);
        // Last segment ends at x=20
        assert!((out.last().unwrap().to.x - 20.0).abs() < 1e-6);
    }
}
