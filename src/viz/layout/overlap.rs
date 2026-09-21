// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Overlap removal (force-directed push apart colliding boxes)
//!
//! Iteratively called (~30 rounds) at the last step of the layout main flow until stable.

use crate::vector::graph::McVecGraph;

use super::size::MIN_GAP;

/// Single round: check all box pairs, push overlapping pairs' centers in opposite directions
///
/// Returns `true` if any movement this round, `false` if no overlaps (iteration can stop).
pub fn resolve_overlaps(graph: &mut McVecGraph) -> bool {
    let n = graph.boxes.len();
    let mut moved = false;

    let positions: Vec<(i64, f64, f64, f64, f64)> = graph
        .boxes
        .iter()
        .map(|b| (b.id, b.x, b.y, b.w, b.h))
        .collect();

    for i in 0..n {
        for j in (i + 1)..n {
            let (_, ax, ay, aw, ah) = positions[i];
            let (_, bx, by, bw, bh) = positions[j];

            let overlap_x = (ax + aw + MIN_GAP) > bx && (bx + bw + MIN_GAP) > ax;
            let overlap_y = (ay + ah + MIN_GAP) > by && (by + bh + MIN_GAP) > ay;

            if overlap_x && overlap_y {
                let acx = ax + aw / 2.0;
                let acy = ay + ah / 2.0;
                let bcx = bx + bw / 2.0;
                let bcy = by + bh / 2.0;

                let dx = bcx - acx;
                let dy = bcy - acy;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);

                let push_x = dx / dist * 12.0;
                let push_y = dy / dist * 12.0;

                let id_i = positions[i].0;
                let id_j = positions[j].0;

                if let Some(bi) = graph.boxes.iter_mut().find(|b| b.id == id_i) {
                    bi.x -= push_x;
                    bi.y -= push_y;
                }
                if let Some(bj) = graph.boxes.iter_mut().find(|b| b.id == id_j) {
                    bj.x += push_x;
                    bj.y += push_y;
                }
                moved = true;
            }
        }
    }
    moved
}

/// Iteratively call `resolve_overlaps` until stable (or hit `max_iter` limit)
pub fn resolve_overlaps_iterative(graph: &mut McVecGraph, max_iter: usize) {
    for _ in 0..max_iter {
        if !resolve_overlaps(graph) {
            break;
        }
    }
}

// === Device-layer variant ===

use crate::vector::graph::{BoxKind, McVecBox};

/// Clearance between separated bodies, in px.
const SEP_GAP: f64 = 24.0;
/// Sweep bound: each sweep pushes every overlap right once, so a stack of N
/// coincident parts settles in at most N sweeps.
const SEP_SWEEPS: usize = 64;

/// Separate overlapping component boxes on the X axis only. The device
/// pipeline's rows and trunks are y-based — a y shift moves a box off its own
/// row trunk (the M7.4 lesson) — so the later box in (y, x, id) order is
/// pushed right until nothing intersects, and the trees re-derived at render
/// follow the moved anchors. Returns the number of boxes moved.
pub fn separate_overlaps_x(graph: &mut McVecGraph) -> usize {
    let mut total = 0usize;
    for _ in 0..SEP_SWEEPS {
        let mut moved = 0usize;
        let mut order: Vec<usize> = (0..graph.boxes.len()).collect();
        order.sort_by(|&a, &b| {
            let (a, b) = (&graph.boxes[a], &graph.boxes[b]);
            (a.y, a.x, a.id)
                .partial_cmp(&(b.y, b.x, b.id))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (pos, &i) in order.iter().enumerate() {
            if !separable(&graph.boxes[i]) {
                continue;
            }
            for &j in order.iter().skip(pos + 1) {
                if !separable(&graph.boxes[j]) {
                    continue;
                }
                let hit = {
                    let (a, b) = (&graph.boxes[i], &graph.boxes[j]);
                    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
                };
                if !hit {
                    continue;
                }
                let dx = graph.boxes[i].x + graph.boxes[i].w + SEP_GAP - graph.boxes[j].x;
                if dx > 0.0 {
                    graph.boxes[j].x += dx;
                    moved += 1;
                }
            }
        }
        total += moved;
        if moved == 0 {
            break;
        }
    }
    total
}

/// The kinds this pass separates: real placed parts. Power flags hang off
/// pins, containers own the canvas.
fn separable(b: &McVecBox) -> bool {
    b.id >= 0 && matches!(b.kind, BoxKind::TwoPin | BoxKind::MultiPin)
}

#[cfg(test)]
mod sep_tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::{LayerStyle, Symbol};

    fn cap(id: i64, name: &str, x: f64, y: f64) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            "CAP".into(),
            BoxKind::TwoPin,
            Symbol::Capacitor,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
            name.into(),
            Vec::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 60.0;
        b.h = 20.0;
        b
    }

    #[test]
    fn coincident_parts_separate_deterministically() {
        let mut g = McVecGraph::new(1, "m".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(cap(1, "_C1", 100.0, 90.0));
        g.boxes.push(cap(2, "_C3", 100.0, 90.0));
        separate_overlaps_x(&mut g);
        let (a, b) = (&g.boxes[0], &g.boxes[1]);
        assert_eq!(a.x, 100.0, "the first in order never moves");
        assert!(b.x >= a.x + a.w + SEP_GAP - 0.01, "later one cleared right");
    }

    #[test]
    fn disjoint_parts_never_move() {
        let mut g = McVecGraph::new(1, "m".into());
        g.boxes.push(cap(1, "C1", 0.0, 0.0));
        g.boxes.push(cap(2, "C2", 200.0, 0.0));
        assert_eq!(separate_overlaps_x(&mut g), 0);
    }

    #[test]
    fn a_stack_settles() {
        let mut g = McVecGraph::new(1, "m".into());
        for i in 0..6 {
            g.boxes.push(cap(i + 1, &format!("_C{i}"), 100.0, 90.0));
        }
        separate_overlaps_x(&mut g);
        for i in 0..g.boxes.len() {
            for j in i + 1..g.boxes.len() {
                let (a, b) = (&g.boxes[i], &g.boxes[j]);
                assert!(
                    !(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h),
                    "boxes {i} and {j} still overlap"
                );
            }
        }
    }
}
