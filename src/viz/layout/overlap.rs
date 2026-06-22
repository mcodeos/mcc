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
