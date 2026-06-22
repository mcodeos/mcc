// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Adjacency list construction + connected component partition
//!
//! Entry-point utility for multi-strategy scheduling: partition the graph by "which boxes are connected to each other",
//! each chunk independently picks a suitable layouter (chain / radial / hierarchical).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::vector::graph::McVecGraph;

// ============================================================================
// Adjacency list
// ============================================================================

/// Convert `graph.nets` into undirected adjacency list `box_id → neighbor list`
///
/// N endpoints on a net are all pairwise adjacent. Only count relationships where **both endpoints are in `graph.boxes`**,
/// drop dangling endpoints.
///
/// ## ★ P03 (S1) Changes
/// Previously read both `graph.edges` (old binary) + `graph.nets` (new hyperedge), and de-duplicated.
/// P03 removes the dual track, now **only reads graph.nets**. McVecEdge field still exists but is no longer populated.
///
/// Test fixtures (`hierarchical.rs` test) have also been migrated to push VizNet, this function's behavior after going through nets
/// is fully equivalent to the old edges path.
pub fn build_adjacency(graph: &McVecGraph) -> HashMap<i64, Vec<i64>> {
    let id_set: HashSet<i64> = graph.boxes.iter().map(|b| b.id).collect();
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for b in &graph.boxes {
        adj.insert(b.id, Vec::new());
    }

    // Use a set for de-duplication (avoid pushing the same (a,b) twice from one net)
    let mut seen_pairs: HashSet<(i64, i64)> = HashSet::new();
    let record_edge =
        |a: i64, b: i64, adj: &mut HashMap<i64, Vec<i64>>, seen: &mut HashSet<(i64, i64)>| {
            let key = if a <= b { (a, b) } else { (b, a) };
            if !seen.insert(key) {
                return;
            }
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        };

    for net in &graph.nets {
        let box_ids: Vec<i64> = net
            .box_ids()
            .into_iter()
            .filter(|id| id_set.contains(id))
            .collect();
        // All box pairs on one net get connected edges
        for i in 0..box_ids.len() {
            for j in (i + 1)..box_ids.len() {
                record_edge(box_ids[i], box_ids[j], &mut adj, &mut seen_pairs);
            }
        }
    }

    adj
}

/// Compute each box's degree (number of neighbors)
pub fn build_degrees(graph: &McVecGraph, adj: &HashMap<i64, Vec<i64>>) -> HashMap<i64, usize> {
    graph
        .boxes
        .iter()
        .map(|b| (b.id, adj.get(&b.id).map(|v| v.len()).unwrap_or(0)))
        .collect()
}

// ============================================================================
// Connected component partition (BFS flood-fill)
// ============================================================================

/// BFS flood-fill component partition
///
/// Returns `Vec<Vec<box_id>>` —— each inner Vec is a connected component.
/// Singletons (isolated) also appear as single-element components.
///
/// Order is not guaranteed —— upper callers sort by "component size" before use.
pub fn find_connected_components(
    boxes: &[crate::vector::graph::McVecBox],
    adj: &HashMap<i64, Vec<i64>>,
) -> Vec<Vec<i64>> {
    let mut visited: HashSet<i64> = HashSet::new();
    let mut components: Vec<Vec<i64>> = Vec::new();

    for b in boxes {
        if visited.contains(&b.id) {
            continue;
        }
        let mut comp = Vec::new();
        let mut queue: VecDeque<i64> = VecDeque::new();
        queue.push_back(b.id);
        visited.insert(b.id);

        while let Some(cur) = queue.pop_front() {
            comp.push(cur);
            if let Some(neighbors) = adj.get(&cur) {
                for &n in neighbors {
                    if !visited.contains(&n) {
                        visited.insert(n);
                        queue.push_back(n);
                    }
                }
            }
        }
        components.push(comp);
    }

    components
}

/// Partition components into (multi-box, single-box) two groups, multi-box sorted by box count descending
pub fn partition_components(components: Vec<Vec<i64>>) -> (Vec<Vec<i64>>, Vec<Vec<i64>>) {
    let (mut multi, single): (Vec<_>, Vec<_>) = components.into_iter().partition(|c| c.len() > 1);
    multi.sort_by_key(|c| std::cmp::Reverse(c.len()));
    (multi, single)
}
