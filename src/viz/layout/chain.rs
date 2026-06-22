// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Chain topology detection + linear horizontal layout
//!
//! Use case: power chains (USB → LDO → DCDC → MCU) — this kind of linear flow.
//! Putting the chain around a circle doesn't read as "flow direction", so detect it separately and lay out horizontally.

use std::collections::{HashMap, HashSet};

use crate::vector::graph::McVecGraph;

/// Try to linearize a connected component into a chain
///
/// ## Criteria
/// 1. All nodes have degree ≤ 2
/// 2. Exactly 2 nodes have degree = 1 (endpoints), others have degree = 2
/// 3. Walking from either endpoint visits all nodes in the component
///
/// Returns `Vec<box_id>` in topological order if satisfied, else `None`.
pub fn try_linearize_chain(comp: &[i64], adj: &HashMap<i64, Vec<i64>>) -> Option<Vec<i64>> {
    // Step 1: degree distribution
    let mut endpoints: Vec<i64> = Vec::new(); // degree == 1
    let mut middles: Vec<i64> = Vec::new(); // degree == 2
    for &id in comp {
        let d = adj.get(&id).map(|v| v.len()).unwrap_or(0);
        match d {
            0 => return None,
            1 => endpoints.push(id),
            2 => middles.push(id),
            _ => return None, // has hub → not a pure chain
        }
    }

    if endpoints.len() != 2 {
        return None;
    }
    if endpoints.len() + middles.len() != comp.len() {
        return None;
    }

    // Step 2: walk from either endpoint
    let start = endpoints[0];
    let mut chain: Vec<i64> = Vec::with_capacity(comp.len());
    let mut visited: HashSet<i64> = HashSet::new();
    chain.push(start);
    visited.insert(start);

    let mut cur = start;
    while chain.len() < comp.len() {
        let neighbors = adj.get(&cur).cloned().unwrap_or_default();
        let next = neighbors.into_iter().find(|n| !visited.contains(n));
        match next {
            Some(n) => {
                chain.push(n);
                visited.insert(n);
                cur = n;
            }
            None => return None,
        }
    }

    Some(chain)
}

/// Lay out a chain linearly along the x-axis, all boxes y-center aligned
///
/// Leave `CHAIN_GAP` between adjacent boxes, vertically center small boxes to the largest box's midline ——
/// this way the right/left midpoints of adjacent boxes align, rendered as a horizontal line.
///
/// ## Return
/// `(used_width, used_height)` —— rectangle this chain occupies
pub fn layout_chain_horizontal(
    graph: &mut McVecGraph,
    chain: &[i64],
    start_x: f64,
    start_y: f64,
) -> (f64, f64) {
    const CHAIN_GAP: f64 = 50.0;

    let max_h: f64 = chain
        .iter()
        .filter_map(|id| graph.boxes.iter().find(|b| b.id == *id))
        .map(|b| b.h)
        .fold(0.0f64, f64::max);
    let row_cy = start_y + max_h / 2.0;

    let mut cur_x = start_x;
    for &id in chain {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
            b.x = cur_x;
            b.y = row_cy - b.h / 2.0;
            cur_x += b.w + CHAIN_GAP;
        }
    }

    let used_w = (cur_x - CHAIN_GAP - start_x).max(0.0);
    (used_w, max_h)
}
