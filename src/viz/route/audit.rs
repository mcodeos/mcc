// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Collision metrics (M1)
//!
//! Objectively counts three types of crossings in a wired graph, as the ruler for
//! "industrial-grade zero-collision":
//! - box_box  : two component boxes (after inflation) overlap each other
//! - wire_box : a wire passes through a component box that is **not its own endpoint**
//! - wire_wire: two **cross-net** wires cross / overlap
//!
//! Pure geometry, no side effects. Recommend calling once after route_all and
//! eprintln-ing the numbers; after A\* reservation (M2 stage 2) goes online, wire_wire
//! should drop significantly —— this is the metric to judge if it's really solved.
//!
//! Note: lines in the same net sharing a point at a junction is normal, so wire_wire
//! only checks **cross-net** pairs; wire_box excludes the line's own endpoint boxes
//! (a pin touching the box edge is normal).

use crate::vector::graph::{McVecGraph, Segment};

const EPS: f64 = 0.5; // Tolerance for floating-point comparison
const BOX_INFLATE: f64 = 2.0; // Inflation for box collision detection
const RECT_SHRINK: f64 = 1.0; // Shrinkage for ignoring edge-grazing
const TOUCH: f64 = 0.5; // Tolerance for "touching" detection

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CollisionReport {
    pub box_box: usize,
    pub wire_box: usize,
    pub wire_wire: usize,
    pub details: Vec<String>,
}

impl CollisionReport {
    pub fn total(&self) -> usize {
        self.box_box + self.wire_box + self.wire_wire
    }
    fn merge(&mut self, other: CollisionReport) {
        self.box_box += other.box_box;
        self.wire_box += other.wire_box;
        self.wire_wire += other.wire_wire;
        self.details.extend(other.details);
    }
}

/// Audit the **current layer** (graph.boxes + graph.nets). For sub-graphs use `audit_all`.
pub fn audit_collisions(graph: &McVecGraph) -> CollisionReport {
    let mut rep = CollisionReport::default();
    let keep_details = graph.boxes.len() + graph.nets.len() < 400; // Big graph: don't pile up details

    // ── 1) box-box ──
    let boxes = &graph.boxes;
    for i in 0..boxes.len() {
        for j in (i + 1)..boxes.len() {
            let a = &boxes[i];
            let b = &boxes[j];
            if rects_overlap(a.x, a.y, a.w, a.h, b.x, b.y, b.w, b.h, BOX_INFLATE) {
                rep.box_box += 1;
                if keep_details {
                    rep.details.push(format!(
                        "box-box: '{}'(id={}) overlaps '{}'(id={})",
                        a.name, a.id, b.name, b.id
                    ));
                }
            }
        }
    }

    // ── 2) wire-box ──
    for net in &graph.nets {
        let route = match &net.route {
            Some(r) => r,
            None => continue,
        };
        // Endpoint box ids of this net (excluded)
        let ep_ids: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
        for seg in &route.segments {
            for b in boxes {
                if ep_ids.contains(&b.id) {
                    continue;
                }
                if seg_hits_rect(seg, b.x, b.y, b.w, b.h) {
                    rep.wire_box += 1;
                    if keep_details {
                        rep.details.push(format!(
                            "wire-box: net '{}'(nid={}) passes through '{}'(id={})",
                            net.name, net.nid, b.name, b.id
                        ));
                    }
                }
            }
        }
    }

    // ── 3) wire-wire (cross-net) ──
    // Flatten into (net_id, &seg) list, judge pairs across net_ids
    let mut segs: Vec<(i64, &Segment)> = Vec::new();
    for net in &graph.nets {
        if let Some(r) = &net.route {
            for s in &r.segments {
                segs.push((net.nid, s));
            }
        }
    }
    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            if segs[i].0 == segs[j].0 {
                continue; // Same net sharing a point is normal
            }
            if seg_seg_collide(segs[i].1, segs[j].1) {
                rep.wire_wire += 1;
                if keep_details && rep.details.len() < 200 {
                    rep.details
                        .push(format!("wire-wire: nid={} × nid={}", segs[i].0, segs[j].0));
                }
            }
        }
    }

    rep
}

/// Audit the whole graph (recurses sub_graphs, accumulates numbers)
pub fn audit_all(graph: &McVecGraph) -> CollisionReport {
    let mut rep = audit_collisions(graph);
    for sub in &graph.sub_graphs {
        rep.merge(audit_all(sub));
    }
    rep
}

/// Whether the net at `net_index` crosses wires of **other nets**, or passes through
/// boxes of **non-own endpoints** (geometric check, doesn't rely on reservation
/// ownership). For scheduler's rip-up & reroute (M4) —— symmetric detection, both
/// long and short lines can identify conflicts.
pub fn net_has_conflict(graph: &McVecGraph, net_index: usize) -> bool {
    let net = match graph.nets.get(net_index) {
        Some(n) => n,
        None => return false,
    };
    let route = match &net.route {
        Some(r) => r,
        None => return false,
    };
    let ep_ids: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();

    // Pass-through box (excluding own endpoint boxes)
    for seg in &route.segments {
        for b in &graph.boxes {
            if ep_ids.contains(&b.id) {
                continue;
            }
            if seg_hits_rect(seg, b.x, b.y, b.w, b.h) {
                return true;
            }
        }
    }
    // Cross-net wire crossing
    for (oi, other) in graph.nets.iter().enumerate() {
        if oi == net_index {
            continue;
        }
        let oroute = match &other.route {
            Some(r) => r,
            None => continue,
        };
        for sa in &route.segments {
            for sb in &oroute.segments {
                if seg_seg_collide(sa, sb) {
                    return true;
                }
            }
        }
    }
    false
}

// ── Geometry primitives ──────────────────────────────────────────────────────────────

/// Whether two rectangles (each +inflate) actually overlap (area > 0)
fn rects_overlap(
    ax: f64,
    ay: f64,
    aw: f64,
    ah: f64,
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
    inflate: f64,
) -> bool {
    let (ax0, ay0, ax1, ay1) = (
        ax - inflate,
        ay - inflate,
        ax + aw + inflate,
        ay + ah + inflate,
    );
    let (bx0, by0, bx1, by1) = (
        bx - inflate,
        by - inflate,
        bx + bw + inflate,
        by + bh + inflate,
    );
    ax0 < bx1 - EPS && bx0 < ax1 - EPS && ay0 < by1 - EPS && by0 < ay1 - EPS
}

/// Whether an orthogonal segment passes through the rectangle's interior
/// (box shrunk by RECT_SHRINK, ignoring edge-grazing)
fn seg_hits_rect(s: &Segment, rx: f64, ry: f64, rw: f64, rh: f64) -> bool {
    let (sx0, sx1) = (s.from.x.min(s.to.x), s.from.x.max(s.to.x));
    let (sy0, sy1) = (s.from.y.min(s.to.y), s.from.y.max(s.to.y));
    let (rx0, ry0) = (rx + RECT_SHRINK, ry + RECT_SHRINK);
    let (rx1, ry1) = (rx + rw - RECT_SHRINK, ry + rh - RECT_SHRINK);
    if rx1 <= rx0 || ry1 <= ry0 {
        return false; // Box too small
    }
    // Segment bbox (orthogonal segment = the line itself) intersects the shrunk box
    sx1 > rx0 && sx0 < rx1 && sy1 > ry0 && sy0 < ry1
}

/// Whether two orthogonal segments cross / overlap (for cross-net collision check)
fn seg_seg_collide(a: &Segment, b: &Segment) -> bool {
    let a_h = (a.from.y - a.to.y).abs() < EPS; // a horizontal
    let b_h = (b.from.y - b.to.y).abs() < EPS;
    match (a_h, b_h) {
        (true, false) => cross_hv(a, b),
        (false, true) => cross_hv(b, a),
        (true, true) => {
            // Both horizontal: same y and x ranges really overlap
            (a.from.y - b.from.y).abs() < EPS
                && ranges_overlap(
                    a.from.x.min(a.to.x),
                    a.from.x.max(a.to.x),
                    b.from.x.min(b.to.x),
                    b.from.x.max(b.to.x),
                )
        }
        (false, false) => {
            // Both vertical: same x and y ranges really overlap
            (a.from.x - b.from.x).abs() < EPS
                && ranges_overlap(
                    a.from.y.min(a.to.y),
                    a.from.y.max(a.to.y),
                    b.from.y.min(b.to.y),
                    b.from.y.max(b.to.y),
                )
        }
    }
}

/// h = horizontal segment, v = vertical segment: whether they cross/touch within
/// each other's range (including endpoints, TOUCH tolerance).
/// Including endpoints is the key: otherwise rip-up might shift a "crossing" to
/// "endpoint touch" and think it's solved; audit reports 0 but visually they still
/// cross.
fn cross_hv(h: &Segment, v: &Segment) -> bool {
    let hy = h.from.y;
    let (hx0, hx1) = (h.from.x.min(h.to.x), h.from.x.max(h.to.x));
    let vx = v.from.x;
    let (vy0, vy1) = (v.from.y.min(v.to.y), v.from.y.max(v.to.y));
    vx >= hx0 - TOUCH && vx <= hx1 + TOUCH && hy >= vy0 - TOUCH && hy <= vy1 + TOUCH
}

/// Cross point of two orthogonal segments (only returns when H×V actually cross;
/// parallel/overlapping/non-intersecting return None).
/// For scheduler to accumulate historical congestion cost at crossing points.
pub fn seg_cross_point(a: &Segment, b: &Segment) -> Option<(f64, f64)> {
    let a_h = (a.from.y - a.to.y).abs() < EPS;
    let b_h = (b.from.y - b.to.y).abs() < EPS;
    let (h, v) = match (a_h, b_h) {
        (true, false) => (a, b),
        (false, true) => (b, a),
        _ => return None, // Parallel (overlap counted separately, here only point-crossing)
    };
    if cross_hv(h, v) {
        Some((v.from.x, h.from.y))
    } else {
        None
    }
}

fn ranges_overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    a0 < b1 - EPS && b0 < a1 - EPS
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::Point;

    fn seg(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment {
            from: Point::new(x0, y0),
            to: Point::new(x1, y1),
        }
    }

    #[test]
    fn hv_cross_counts() {
        // Horizontal y=10 x[0,20] × vertical x=10 y[0,20] → interior crossing
        assert!(seg_seg_collide(
            &seg(0.0, 10.0, 20.0, 10.0),
            &seg(10.0, 0.0, 10.0, 20.0)
        ));
    }

    #[test]
    fn hv_touch_at_endpoint_counts() {
        // Vertical segment's endpoint exactly lands on horizontal line (T-junction)
        // → now counts (includes endpoint detection, this is the bug being fixed)
        assert!(seg_seg_collide(
            &seg(0.0, 10.0, 20.0, 10.0),
            &seg(10.0, 10.0, 10.0, 20.0)
        ));
    }

    #[test]
    fn parallel_overlap_counts() {
        // Two horizontal lines at same y, x ranges overlap
        assert!(seg_seg_collide(
            &seg(0.0, 5.0, 20.0, 5.0),
            &seg(10.0, 5.0, 30.0, 5.0)
        ));
    }

    #[test]
    fn parallel_apart_not_counted() {
        assert!(!seg_seg_collide(
            &seg(0.0, 5.0, 20.0, 5.0),
            &seg(0.0, 9.0, 20.0, 9.0)
        ));
    }

    #[test]
    fn seg_through_box() {
        // Horizontal line passes through box (40..60, 40..60)
        assert!(seg_hits_rect(
            &seg(0.0, 50.0, 100.0, 50.0),
            40.0,
            40.0,
            20.0,
            20.0
        ));
        // Line outside the box
        assert!(!seg_hits_rect(
            &seg(0.0, 0.0, 100.0, 0.0),
            40.0,
            40.0,
            20.0,
            20.0
        ));
    }

    #[test]
    fn boxes_overlap_detect() {
        assert!(rects_overlap(
            0.0, 0.0, 10.0, 10.0, 5.0, 5.0, 10.0, 10.0, 0.0
        ));
        assert!(!rects_overlap(
            0.0, 0.0, 10.0, 10.0, 50.0, 50.0, 10.0, 10.0, 0.0
        ));
    }
}
