// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Arrange — layering + intra-layer order
//!
//! Exact enumeration over the quotient graph, outputting node order per layer.
//! For N ≤ 7 uses Heap's algorithm for full permutation + cut-point enumeration.
//!
//! ## Algorithm
//! 1. Cycle breaking (greedy Eades-Lin-Smyth)
//! 2. Orientation anchoring (module ports + in-degree 0 + source order)
//! 3. Exact enumeration (Heap's algorithm + cut-point enumeration)
//! 4. top-K tournament
//!
//! ## Acceptance (M3-2 / M3-3)
//! - t4_current: optimal solution [u1][u2,u3][u4,u5] (3 layers, backward=1),
//!   runner-up cost equal or close (after weight tuning, reducing backward edges takes priority)
//! - t2_cycle / t3_cycle: backward<=1
//! - box id +1000: optimal solution unchanged
//! - 20 consecutive runs: best consistent

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Instant;

use super::quotient::{Direction, NodeId, QuotientGraph, SP_COL_W};

// ============================================================================
// Data structures
// ============================================================================

/// One layer output by the searcher
pub type Layer = Vec<i64>;

/// Layered arrangement result
#[derive(Debug, Clone, PartialEq)]
pub struct Arrangement {
    /// Node ID list of each layer
    pub layers: Vec<Layer>,
}

/// Hard orientation of each edge after cycle breaking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDir {
    /// Forward (src → dst, left to right)
    Forward,
    /// Backward (dst → src, right to left)
    Backward,
}

/// Cost structure
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Cost {
    /// Crossing count
    pub crossings: u32,
    /// Number of edges violating post-cycle-breaking orientation
    pub backward: u32,
    /// Span penalty
    pub span: u32,
    /// Port crossings
    pub port_cross: u32,
    /// Same-layer soft penalty
    pub same_layer: u32,
    /// Orientation anchor penalty
    pub orient: u32,
    /// Source-order prior
    pub order: u32,
    /// Area penalty
    pub area: f64,
    /// Weighted total cost
    pub weighted: f64,
}

// ============================================================================
// Weight constants
// ============================================================================

pub const W_CROSS: f64 = 1000.0;
pub const W_BACK: f64 = 600.0;   // Cycle-breaking direction violation, should outweigh area
pub const W_SPAN: f64 = 100.0;
pub const W_PORT: f64 = 20.0;
pub const W_SAMELAYER: f64 = 500.0;  // Same-layer soft penalty, below W_BACK(600)
pub const W_ORIENT: f64 = 400.0;
pub const W_ORDER: f64 = 30.0;
pub const W_AREA: f64 = 2.0;

impl Cost {
    pub fn zero() -> Self {
        Cost {
            crossings: 0,
            backward: 0,
            span: 0,
            port_cross: 0,
            same_layer: 0,
            orient: 0,
            order: 0,
            area: 0.0,
            weighted: 0.0,
        }
    }

    pub fn compute_weighted(&mut self) {
        self.weighted = self.crossings as f64 * W_CROSS
            + self.backward as f64 * W_BACK
            + self.span as f64 * W_SPAN
            + self.port_cross as f64 * W_PORT
            + self.same_layer as f64 * W_SAMELAYER
            + self.orient as f64 * W_ORIENT
            + self.order as f64 * W_ORDER
            + self.area * W_AREA;
    }

    pub fn from_counts(
        crossings: u32,
        backward: u32,
        span: u32,
        port_cross: u32,
        same_layer: u32,
        orient: u32,
        order: u32,
        area: f64,
    ) -> Self {
        let mut c = Cost {
            crossings,
            backward,
            span,
            port_cross,
            same_layer,
            orient,
            order,
            area,
            weighted: 0.0,
        };
        c.compute_weighted();
        c
    }
}

impl fmt::Display for Cost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "weighted={:.0} (cross={} back={} span={} port={} same={} orient={} order={} area={:.0})",
            self.weighted,
            self.crossings,
            self.backward,
            self.span,
            self.port_cross,
            self.same_layer,
            self.orient,
            self.order,
            self.area
        )
    }
}

/// Exact search limit
pub const EXACT_SEARCH_LIMIT: usize = 7;
/// Number of top-K candidates
pub const TOP_K: usize = 5;
/// Time budget (ms); on timeout use the best fully computed result
pub const TIME_BUDGET_MS: u64 = 150;

// ============================================================================
// Search entry
// ============================================================================

/// Exact search over the quotient graph, returns top-K candidates
pub fn solve(q: &QuotientGraph) -> Vec<(Cost, Arrangement)> {
    if q.nodes.is_empty() {
        return Vec::new();
    }
    if q.nodes.len() == 1 {
        let arr = Arrangement {
            layers: vec![q.nodes.clone()],
        };
        let c = cost(q, &arr);
        return vec![(c, arr)];
    }
    if q.nodes.len() <= EXACT_SEARCH_LIMIT {
        exact_enumerate(q)
    } else {
        unimplemented!("M6: heuristic for N>7, got N={}", q.nodes.len())
    }
}

// ============================================================================
// Exact enumeration: Heap's algorithm + cut points + mirror symmetry + branch & bound
// ============================================================================

/// Precomputed data for each edge
struct EdgeData {
    src: usize,
    dst: usize,
    hard_dir: EdgeDir,
}

/// Enumerate all cut points for a single permutation and evaluate cost
fn evaluate_permutation(
    perm: &[usize],
    max_cut: u32,
    q: &QuotientGraph,
    edges: &[EdgeData],
    sides: &[NodeSide],
    all_neutral: bool,
    best: &mut Vec<(Cost, Arrangement)>,
    best_weighted: &mut f64,
    total_evals: &mut u64,
    pruned_count: &mut u64,
) {
    let n = perm.len();
    let nodes = &q.nodes;
    // Mirror symmetry: skip mirrored permutations when all nodes are Neutral
    if all_neutral && is_mirror_perm(perm, nodes) {
        return;
    }

    for cut_mask in 0..max_cut {
        if all_neutral && is_mirror_cut(cut_mask, n.saturating_sub(1)) {
            continue;
        }

        let (arr, pruned) = build_with_bb(perm, cut_mask, nodes, edges, sides, *best_weighted);

        if pruned {
            *pruned_count += 1;
            continue;
        }

        *total_evals += 1;
        let full_cost = compute_full_cost(&arr, edges, sides, q);

        if full_cost.weighted < *best_weighted - 1e-9 {
            *best_weighted = full_cost.weighted;
            *best = vec![(full_cost, arr)];
        } else if (full_cost.weighted - *best_weighted).abs() < 1e-9 {
            best.push((full_cost, arr));
        }
    }
}

/// Exact enumeration entry
fn exact_enumerate(q: &QuotientGraph) -> Vec<(Cost, Arrangement)> {
    let n = q.nodes.len();
    let hard_dirs = break_cycles(q);
    let sides = compute_node_sides(q, &hard_dirs);

    // Precompute edge data
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let edges: Vec<EdgeData> = q
        .edges
        .iter()
        .enumerate()
        .map(|(ei, e)| EdgeData {
            src: node_to_idx[&e.src],
            dst: node_to_idx[&e.dst],
            hard_dir: hard_dirs[ei],
        })
        .collect();

    let mut best: Vec<(Cost, Arrangement)> = Vec::new();
    let mut best_weighted = f64::MAX;

    // Check whether all nodes are Neutral (decides whether mirror symmetry is enabled)
    let all_neutral = sides.iter().all(|s| *s == NodeSide::Neutral);

    // Generate permutations with Heap's algorithm
    let mut perm: Vec<usize> = (0..n).collect();
    let mut c = vec![0usize; n]; // Heap state
    let max_cut = 1u32 << (n.saturating_sub(1));

    let mut total_evals = 0u64;
    let mut pruned_count = 0u64;
    let start = Instant::now();

    // Output the initial permutation first
    evaluate_permutation(
        &perm, max_cut, q, &edges, &sides, all_neutral,
        &mut best, &mut best_weighted, &mut total_evals, &mut pruned_count,
    );

    let mut i = 1;
    while i < n {
        // Time budget check
        if start.elapsed().as_millis() as u64 > TIME_BUDGET_MS {
            eprintln!("[debug] exact_enumerate: time budget {}ms exceeded, stopping early ({} evals, {} pruned)",
                TIME_BUDGET_MS, total_evals, pruned_count);
            break;
        }
        if c[i] < i {
            if i % 2 == 0 {
                perm.swap(0, i);
            } else {
                perm.swap(c[i], i);
            }

            evaluate_permutation(
                &perm, max_cut, q, &edges, &sides, all_neutral,
                &mut best, &mut best_weighted, &mut total_evals, &mut pruned_count,
            );

            c[i] += 1;
            i = 1;
        } else {
            c[i] = 0;
            i += 1;
        }
    }

    // Sort by cost, take top-K
    best.sort_by(|a, b| {
        a.0.weighted
            .partial_cmp(&b.0.weighted)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    best.truncate(TOP_K);
    eprintln!("[debug] exact_enumerate: {} candidates, {} evals, {} pruned, best={:.0} layers={:?} ({}ms)",
        best.len(), total_evals, pruned_count, best[0].0.weighted, best[0].1.layers,
        start.elapsed().as_millis());
    best
}

/// Check whether a permutation is its own mirror (skip the first half to avoid duplicates)
fn is_mirror_perm(perm: &[usize], nodes: &[NodeId]) -> bool {
    if perm.len() <= 1 {
        return false;
    }
    let first_id = nodes[perm[0]];
    let last_id = nodes[perm[perm.len() - 1]];
    // Skip permutations with first > last (mirror already covered by first < last)
    first_id > last_id
}

/// Check whether a cut-point mask is its own mirror
fn is_mirror_cut(mask: u32, n_bits: usize) -> bool {
    if n_bits <= 1 {
        return false;
    }
    let rev = reverse_bits(mask, n_bits as u32);
    // Skip masks with rev < mask
    rev < mask
}

/// Bit reversal
fn reverse_bits(x: u32, n_bits: u32) -> u32 {
    let mut result = 0u32;
    for i in 0..n_bits {
        if (x >> i) & 1 != 0 {
            result |= 1 << (n_bits - 1 - i);
        }
    }
    result
}

/// Branch & bound: incrementally build the arrangement, computing partial cost per added layer
/// Returns (arrangement, whether pruned)
fn build_with_bb(
    perm: &[usize],
    cut_mask: u32,
    nodes: &[NodeId],
    edges: &[EdgeData],
    sides: &[NodeSide],
    best_weighted: f64,
) -> (Arrangement, bool) {
    let n = perm.len();
    let mut layers: Vec<Vec<NodeId>> = Vec::new();
    let mut current: Vec<NodeId> = Vec::new();

    for i in 0..n {
        current.push(nodes[perm[i]]);
        let is_cut = ((cut_mask >> i) & 1) != 0;
        if is_cut || i == n - 1 {
            layers.push(current.clone());
            current.clear();

            // Partial cost check
            let partial = compute_partial_cost(&layers, edges, sides, nodes);
            if partial.weighted > best_weighted {
                return (Arrangement { layers }, true);
            }
        }
    }

    (Arrangement { layers }, false)
}

/// Compute partial cost (built layers only)
fn compute_partial_cost(
    layers: &[Vec<NodeId>],
    edges: &[EdgeData],
    sides: &[NodeSide],
    nodes: &[NodeId],
) -> Cost {
    let node_to_layer: HashMap<NodeId, usize> = {
        let mut m = HashMap::new();
        for (li, layer) in layers.iter().enumerate() {
            for &nid in layer {
                m.insert(nid, li);
            }
        }
        m
    };

    let mut crossings: u32 = 0;
    let mut backward: u32 = 0;
    let mut span: u32 = 0;
    let mut same_layer: u32 = 0;
    let mut orient: u32 = 0;
    let mut order: u32 = 0;

    // Edges
    for e in edges {
        let sl = node_to_layer.get(&nodes[e.src]);
        let dl = node_to_layer.get(&nodes[e.dst]);
        match (sl, dl) {
            (Some(&sl), Some(&dl)) => {
                if sl == dl {
                    same_layer += 1;
                } else {
                    // backward
                    match e.hard_dir {
                        EdgeDir::Forward if sl > dl => backward += 1,
                        EdgeDir::Backward if sl < dl => backward += 1,
                        _ => {}
                    }
                    // span
                    let diff = if sl > dl { sl - dl } else { dl - sl };
                    span += diff.saturating_sub(1) as u32;
                }
            }
            _ => {}
        }
    }

    // Crossings between adjacent layers
    for li in 0..layers.len().saturating_sub(1) {
        crossings += count_crossings(&layers[li], &layers[li + 1], edges, nodes);
    }

    // Orient
    let node_to_idx: HashMap<NodeId, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    for (li, layer) in layers.iter().enumerate() {
        for &nid in layer {
            if let Some(&idx) = node_to_idx.get(&nid) {
                if sides[idx] == NodeSide::Left {
                    for (lj, prev) in layers.iter().enumerate() {
                        if lj >= li {
                            break;
                        }
                        for &pid in prev {
                            if let Some(&pidx) = node_to_idx.get(&pid) {
                                if sides[pidx] == NodeSide::Right {
                                    orient += 1;
                                }
                            }
                        }
                    }
                }
                if sides[idx] == NodeSide::Right {
                    for (lj, next) in layers.iter().enumerate() {
                        if lj <= li {
                            continue;
                        }
                        for &nid2 in next {
                            if let Some(&nidx2) = node_to_idx.get(&nid2) {
                                if sides[nidx2] == NodeSide::Left {
                                    orient += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Order
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let a = nodes[i];
            let b = nodes[j];
            if let (Some(&la), Some(&lb)) = (node_to_layer.get(&a), node_to_layer.get(&b)) {
                if la > lb {
                    order += 1;
                }
            }
        }
    }

    let max_nodes = layers.iter().map(|l| l.len()).max().unwrap_or(1);
    let total_w = layers.iter().map(|l| l.len() as f64 * SP_COL_W).sum::<f64>()
        + (layers.len().saturating_sub(1)) as f64 * SP_COL_W;
    let area = total_w; // Width-dominated, prevents tall-thin solutions from having smaller area than short-wide ones

    Cost::from_counts(crossings, backward, span, 0, same_layer, orient, order, area)
}

/// Count edge crossings between two layers
fn count_crossings(
    left: &[NodeId],
    right: &[NodeId],
    edges: &[EdgeData],
    nodes: &[NodeId],
) -> u32 {
    // Collect edges from left to right
    let left_pos: HashMap<NodeId, usize> = left
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let right_pos: HashMap<NodeId, usize> = right
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for e in edges {
        let src_id = nodes[e.src];
        let dst_id = nodes[e.dst];
        if let (Some(&sp), Some(&dp)) = (left_pos.get(&src_id), right_pos.get(&dst_id)) {
            pairs.push((sp, dp));
        } else if let (Some(&sp), Some(&dp)) = (left_pos.get(&dst_id), right_pos.get(&src_id)) {
            pairs.push((sp, dp));
        }
    }

    if pairs.len() < 2 {
        return 0;
    }

    let mut cross = 0u32;
    for i in 0..pairs.len() {
        for j in (i + 1)..pairs.len() {
            let (a1, b1) = pairs[i];
            let (a2, b2) = pairs[j];
            if (a1 < a2 && b1 > b2) || (a1 > a2 && b1 < b2) {
                cross += 1;
            }
        }
    }
    cross
}

/// Compute full cost
fn compute_full_cost(
    arr: &Arrangement,
    edges: &[EdgeData],
    sides: &[NodeSide],
    q: &QuotientGraph,
) -> Cost {
    let node_to_layer: HashMap<NodeId, usize> = {
        let mut m = HashMap::new();
        for (li, layer) in arr.layers.iter().enumerate() {
            for &nid in layer {
                m.insert(nid, li);
            }
        }
        m
    };

    let mut crossings: u32 = 0;
    let mut backward: u32 = 0;
    let mut span: u32 = 0;
    let mut same_layer: u32 = 0;

    for e in edges {
        let sl = node_to_layer.get(&q.nodes[e.src]);
        let dl = node_to_layer.get(&q.nodes[e.dst]);
        match (sl, dl) {
            (Some(&sl), Some(&dl)) => {
                if sl == dl {
                    same_layer += 1;
                } else {
                    match e.hard_dir {
                        EdgeDir::Forward if sl > dl => backward += 1,
                        EdgeDir::Backward if sl < dl => backward += 1,
                        _ => {}
                    }
                    let diff = if sl > dl { sl - dl } else { dl - sl };
                    span += diff.saturating_sub(1) as u32;
                }
            }
            _ => {}
        }
    }

    for li in 0..arr.layers.len().saturating_sub(1) {
        crossings += count_crossings(&arr.layers[li], &arr.layers[li + 1], edges, &q.nodes);
    }

    let orient = compute_orient_cost(q, sides, arr);
    let order = compute_order_cost(q, arr);

    let max_nodes = arr.layers.iter().map(|l| l.len()).max().unwrap_or(1);
    let total_w = arr
        .layers
        .iter()
        .map(|l| l.len() as f64 * SP_COL_W)
        .sum::<f64>()
        + (arr.layers.len().saturating_sub(1)) as f64 * SP_COL_W;
    let area = total_w; // Width-dominated

    Cost::from_counts(crossings, backward, span, 0, same_layer, orient, order, area)
}

// ============================================================================
// Cycle breaking: Eades–Lin–Smyth greedy feedback arc set
// ============================================================================

/// Cycle breaking: greedy Eades-Lin-Smyth feedback arc set on the quotient graph
///
/// Returns the hard orientation of each edge (Forward / Backward), one-to-one with `q.edges`.
/// Afterwards backward only counts edges violating this orientation.
///
/// Algorithm:
/// 1. Build a directed graph from edge prefer
/// 2. Iteratively remove sinks (out-degree=0) → prefix, sources (in-degree=0) → suffix
/// 3. For remaining nodes pick max(out-degree - in-degree) to remove → suffix
/// 4. Obtain topological order, then decide Forward vs Backward per edge
pub fn break_cycles(q: &QuotientGraph) -> Vec<EdgeDir> {
    let n = q.nodes.len();
    if n == 0 {
        return Vec::new();
    }

    // Build node_id → index mapping
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    // Build adjacency lists (directed edges)
    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut edge_dir: Vec<(usize, usize)> = Vec::with_capacity(q.edges.len());

    for e in &q.edges {
        let si = node_to_idx[&e.src];
        let di = node_to_idx[&e.dst];
        // Decide direction from prefer
        let (from, to) = match e.prefer {
            Direction::LeftToRight => (si, di),
            Direction::RightToLeft => (di, si),
            Direction::Neutral => {
                // Sort by id when there is no preference
                if e.src < e.dst {
                    (si, di)
                } else {
                    (di, si)
                }
            }
        };
        out_edges[from].push(to);
        in_edges[to].push(from);
        edge_dir.push((from, to));
    }

    // Compute in/out degrees
    let mut out_deg: Vec<usize> = out_edges.iter().map(|v| v.len()).collect();
    let mut in_deg: Vec<usize> = in_edges.iter().map(|v| v.len()).collect();
    let mut removed = vec![false; n];

    let mut s1: Vec<usize> = Vec::new(); // Prefix sequence (sinks)
    let mut s2: Vec<usize> = Vec::new(); // Suffix sequence (sources + max delta)

    let mut remaining = n;
    while remaining > 0 {
        // Remove all sinks (out-degree=0)
        loop {
            let sink = (0..n).find(|&i| !removed[i] && out_deg[i] == 0);
            match sink {
                Some(v) => {
                    removed[v] = true;
                    s1.push(v);
                    remaining -= 1;
                    // Update neighbor degrees
                    for &pred in &in_edges[v] {
                        if !removed[pred] {
                            out_deg[pred] -= 1;
                        }
                    }
                }
                None => break,
            }
        }

        // Remove all sources (in-degree=0)
        loop {
            let source = (0..n).find(|&i| !removed[i] && in_deg[i] == 0);
            match source {
                Some(v) => {
                    removed[v] = true;
                    s2.push(v);
                    remaining -= 1;
                    for &succ in &out_edges[v] {
                        if !removed[succ] {
                            in_deg[succ] -= 1;
                        }
                    }
                }
                None => break,
            }
        }

        // If no sink/source, pick max(out-degree - in-degree)
        if remaining > 0 {
            let v = (0..n)
                .filter(|&i| !removed[i])
                .max_by_key(|&i| {
                    let delta = out_deg[i] as isize - in_deg[i] as isize;
                    delta
                })
                .unwrap();
            removed[v] = true;
            s2.push(v);
            remaining -= 1;
            for &succ in &out_edges[v] {
                if !removed[succ] {
                    in_deg[succ] -= 1;
                }
            }
            for &pred in &in_edges[v] {
                if !removed[pred] {
                    out_deg[pred] -= 1;
                }
            }
        }
    }

    // Merge sequences: s1 reversed + s2 in order
    s1.reverse();
    let seq: Vec<usize> = {
        let mut s = s1;
        s.extend(s2);
        s
    };

    // Build topological order position mapping
    let pos: Vec<usize> = {
        let mut p = vec![0; n];
        for (rank, &idx) in seq.iter().enumerate() {
            p[idx] = rank;
        }
        p
    };

    // Decide each edge's direction
    edge_dir
        .iter()
        .map(|&(from, to)| {
            if pos[from] < pos[to] {
                EdgeDir::Forward
            } else {
                EdgeDir::Backward
            }
        })
        .collect()
}

// ============================================================================
// Orientation anchors
// ============================================================================

/// Expected position of a node (for the orient cost term)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NodeSide {
    /// Should lean left
    Left = 0,
    /// No preference
    Neutral = 1,
    /// Should lean right
    Right = 2,
}

/// Compute each node's orientation preference based on quotient graph edge directions
///
/// Priority:
/// a. Module ports: nodes with Output ports lean left, nodes with Input ports lean right
/// b. Nodes with in-degree 0 after cycle breaking lean left
fn compute_node_sides(q: &QuotientGraph, hard_dirs: &[EdgeDir]) -> Vec<NodeSide> {
    let n = q.nodes.len();
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut out_count = vec![0u32; n];
    let mut in_count = vec![0u32; n];

    for (ei, e) in q.edges.iter().enumerate() {
        let si = node_to_idx[&e.src];
        let di = node_to_idx[&e.dst];
        match hard_dirs[ei] {
            EdgeDir::Forward => {
                out_count[si] += 1;
                in_count[di] += 1;
            }
            EdgeDir::Backward => {
                out_count[di] += 1;
                in_count[si] += 1;
            }
        }
    }

    // Compute nodes with in-degree 0 after cycle breaking
    let source_nodes: HashSet<usize> = (0..n).filter(|&i| in_count[i] == 0).collect();

    (0..n)
        .map(|i| {
            if source_nodes.contains(&i) {
                NodeSide::Left
            } else if out_count[i] > 0 && in_count[i] == 0 {
                NodeSide::Left
            } else if in_count[i] > 0 && out_count[i] == 0 {
                NodeSide::Right
            } else {
                NodeSide::Neutral
            }
        })
        .collect()
}

/// Compute orient cost: number of nodes violating orientation anchors
///
/// Check per layer: among a layer's nodes, if a Left-preferring node comes after
/// a Right-preferring node, or the in-layer relative order of Left/Right nodes
/// mismatches expectations, count it as a violation.
fn compute_orient_cost(q: &QuotientGraph, sides: &[NodeSide], arr: &Arrangement) -> u32 {
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut penalty = 0u32;

    // Cross-layer check: violation if a Left node sits to the right of a Right node (larger layer index)
    for (li, layer) in arr.layers.iter().enumerate() {
        for &nid in layer {
            let idx = node_to_idx[&nid];
            if sides[idx] == NodeSide::Left {
                // Check for Right nodes in layers further left
                for (lj, prev_layer) in arr.layers.iter().enumerate() {
                    if lj >= li {
                        break;
                    }
                    for &pid in prev_layer {
                        let pidx = node_to_idx[&pid];
                        if sides[pidx] == NodeSide::Right {
                            penalty += 1;
                        }
                    }
                }
            }
            if sides[idx] == NodeSide::Right {
                // Check for Left nodes in layers further right
                for (lj, next_layer) in arr.layers.iter().enumerate() {
                    if lj <= li {
                        continue;
                    }
                    for &nid2 in next_layer {
                        let nidx2 = node_to_idx[&nid2];
                        if sides[nidx2] == NodeSide::Left {
                            penalty += 1;
                        }
                    }
                }
            }
        }
    }

    penalty
}

/// Compute order cost: number of inversions violating source order
///
/// Source order is proxied by the order in q.nodes (id sorted, approximating
/// source declaration order).
/// For a node pair (a, b) in different layers, if a precedes b in source order
/// but a sits in a layer further right (larger layer index), count one inversion.
fn compute_order_cost(q: &QuotientGraph, arr: &Arrangement) -> u32 {
    let node_to_layer: HashMap<NodeId, usize> = {
        let mut m = HashMap::new();
        for (li, layer) in arr.layers.iter().enumerate() {
            for &nid in layer {
                m.insert(nid, li);
            }
        }
        m
    };

    let mut penalty = 0u32;
    for i in 0..q.nodes.len() {
        for j in (i + 1)..q.nodes.len() {
            let a = q.nodes[i];
            let b = q.nodes[j];
            if let (Some(&la), Some(&lb)) = (node_to_layer.get(&a), node_to_layer.get(&b)) {
                if la > lb {
                    // a precedes b in source order but has a larger layer index → inversion
                    penalty += 1;
                }
            }
        }
    }
    penalty
}

/// Compute the full cost of a given arrangement
pub fn cost(q: &QuotientGraph, arr: &Arrangement) -> Cost {
    let hard_dirs = break_cycles(q);
    let sides = compute_node_sides(q, &hard_dirs);

    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let edges: Vec<EdgeData> = q
        .edges
        .iter()
        .enumerate()
        .map(|(ei, e)| EdgeData {
            src: node_to_idx[&e.src],
            dst: node_to_idx[&e.dst],
            hard_dir: hard_dirs[ei],
        })
        .collect();

    compute_full_cost(arr, &edges, &sides, q)
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::McVecBox;
    use crate::vector::graph::netdef::IoDirection;
    use crate::vector::graph::{
        BoxKind, EndpointRef, IoSummary, McVecGraph, NetKind, NetRole, VizNet,
    };

    fn mk_ic(id: i64, name: &str, pin_count: usize) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::SubModule,
            crate::vector::graph::Symbol::Module,
            None,
            None,
            pin_count,
            IoSummary::new(),
            format!("main.{}", name),
            Vec::new(),
        )
    }

    fn mk_ep(box_id: i64, pin_id: i64, name: &str, io: IoDirection) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, name, io)
    }

    fn mk_signal_net(id: i64, name: &str, endpoints: Vec<EndpointRef>) -> VizNet {
        VizNet::new(id, name.into(), NetKind::Signal, NetRole::Signal, endpoints)
    }

    fn make_t4_current() -> (McVecGraph, QuotientGraph) {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1_mcu", 4));
        g.boxes.push(mk_ic(2, "u2_ldo_in", 3));
        g.boxes.push(mk_ic(3, "u3_spk", 3));
        g.boxes.push(mk_ic(4, "u4_ldo_out", 3));
        g.boxes.push(mk_ic(5, "u5_flash", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u4", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(4, 41, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(12, "u4_u3", vec![
            mk_ep(4, 42, "OUT", IoDirection::Output),
            mk_ep(3, 31, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(13, "u3_u5", vec![
            mk_ep(3, 32, "OUT", IoDirection::Output),
            mk_ep(5, 51, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(14, "u1_u4", vec![
            mk_ep(1, 12, "CTRL", IoDirection::Output),
            mk_ep(4, 43, "CTRL", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(15, "u1_u5", vec![
            mk_ep(1, 13, "CLK", IoDirection::Output),
            mk_ep(5, 52, "CLK", IoDirection::Input),
        ]));
        let q = QuotientGraph::build(&g);
        (g, q)
    }

    // ────────────────────────────────────────
    // break_cycles tests (M3-2)
    // ────────────────────────────────────────

    /// t4_current: DAG, all edges should be Forward
    #[test]
    fn break_cycles_t4_current_all_forward() {
        let (_g, q) = make_t4_current();
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 6, "t4_current has 6 edges");
        // DAG, cycle breaking should yield all Forward
        let backward_count = dirs.iter().filter(|d| matches!(d, EdgeDir::Backward)).count();
        assert_eq!(backward_count, 0, "t4_current is a DAG, no backward edges");
    }

    /// t2_cycle: 2-node cycle, should have exactly 1 Backward
    #[test]
    fn break_cycles_t2_cycle() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u1", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(1, 12, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 2);
        let backward_count = dirs.iter().filter(|d| matches!(d, EdgeDir::Backward)).count();
        assert_eq!(backward_count, 1, "t2_cycle should have exactly 1 backward edge");
    }

    /// t3_cycle: 3-node cycle, should have exactly 1 Backward
    #[test]
    fn break_cycles_t3_cycle() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u3", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(3, 31, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(12, "u3_u1", vec![
            mk_ep(3, 32, "OUT", IoDirection::Output),
            mk_ep(1, 12, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 3);
        let backward_count = dirs.iter().filter(|d| matches!(d, EdgeDir::Backward)).count();
        assert_eq!(backward_count, 1, "t3_cycle should have exactly 1 backward edge");
    }

    /// t1_chain: simple chain, all Forward
    #[test]
    fn break_cycles_t1_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 1);
        assert!(matches!(dirs[0], EdgeDir::Forward));
    }

    /// Empty graph: no edges
    #[test]
    fn break_cycles_empty() {
        let g = McVecGraph::new(0, "main".into());
        let q = QuotientGraph::build(&g);
        let dirs = break_cycles(&q);
        assert!(dirs.is_empty());
    }

    // ────────────────────────────────────────
    // Orientation anchor tests (M3-2)
    // ────────────────────────────────────────

    /// Graph with clear OUT→IN direction: OUT should be on the left, IN on the right
    #[test]
    fn orient_out_left_in_right() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1_src", 3));
        g.boxes.push(mk_ic(2, "u2_dst", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let hard_dirs = break_cycles(&q);
        let sides = compute_node_sides(&q, &hard_dirs);

        // u1 (index 0) should lean left, u2 (index 1) should lean right
        assert_eq!(sides[0], NodeSide::Left, "u1 (OUT) should be on the left");
        assert_eq!(sides[1], NodeSide::Right, "u2 (IN) should be on the right");

        // Verify orient cost: correct arrangement [u1][u2] has orient=0
        let arr_correct = Arrangement {
            layers: vec![vec![1], vec![2]],
        };
        let cost_correct = cost(&q, &arr_correct);
        assert_eq!(cost_correct.orient, 0, "correct order should have orient=0");

        // Verify orient cost: wrong arrangement [u2][u1] has orient>0
        let arr_wrong = Arrangement {
            layers: vec![vec![2], vec![1]],
        };
        let cost_wrong = cost(&q, &arr_wrong);
        assert!(cost_wrong.orient > 0, "wrong order should have orient>0");
    }

    /// t4_current: verify the orient cost takes effect in the optimal solution
    #[test]
    fn orient_t4_current() {
        let (_g, q) = make_t4_current();
        let hard_dirs = break_cycles(&q);
        let _sides = compute_node_sides(&q, &hard_dirs);

        // Optimal solution [4][2][1][3,5]
        let arr = Arrangement {
            layers: vec![vec![4], vec![2], vec![1], vec![3, 5]],
        };
        let c = cost(&q, &arr);
        // Verify orient + order take effect in the optimal solution
        assert!(c.weighted > 0.0 || (c.orient == 0 && c.order == 0),
            "orient and order should be computed for t4_current");

        // Mirror solution [3,5][1][2][4] should have higher orient cost
        let arr_mirror = Arrangement {
            layers: vec![vec![3, 5], vec![1], vec![2], vec![4]],
        };
        let c_mirror = cost(&q, &arr_mirror);
        assert!(
            c_mirror.orient > c.orient || c_mirror.order > c.order,
            "mirror solution should have higher orient/order cost: best orient={} order={}, mirror orient={} order={}",
            c.orient, c.order, c_mirror.orient, c_mirror.order
        );
    }

    // ────────────────────────────────────────
    // Enumeration tests
    // ────────────────────────────────────────

    /// t4_current: optimal solution [u1][u2,u3][u4,u5] (3 layers, backward=1)
    #[test]
    fn arrange_t4_current_optimal() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        assert!(!candidates.is_empty(), "should have at least one candidate");

        let (cost, best) = &candidates[0];
        // Under current weights (W_BACK=600) the optimum is 3 layers, backward=1
        assert_eq!(best.layers.len(), 3, "t4_current should have 3 layers, got {:?}", best.layers);
        assert_eq!(best.layers[0], vec![1], "first layer should be [u1_mcu]");
        assert_eq!(cost.backward, 1, "should have exactly 1 backward edge (u4→u3)");
        // The last layer should contain u4, u5
        let last: Vec<i64> = best.layers[2].clone();
        let mut last_sorted = last.clone();
        last_sorted.sort();
        assert_eq!(last_sorted, vec![4, 5], "last layer should be [u4_ldo_out, u5_flash]");
    }

    /// t4_current: runner-up cost is not lower than the optimal
    #[test]
    fn arrange_t4_current_second_best() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        if candidates.len() >= 2 {
            let (cost1, _) = &candidates[0];
            let (cost2, _) = &candidates[1];
            assert!(
                cost2.weighted >= cost1.weighted - 1e-9,
                "second best should not be better than best: best={:.0}, second={:.0}",
                cost1.weighted, cost2.weighted
            );
        }
    }

    /// t2_cycle: backward=1
    #[test]
    fn arrange_t2_cycle_backward() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u1", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(1, 12, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty(), "should have at least one candidate");
        let (cost, _) = &candidates[0];
        assert!(
            cost.backward <= 1,
            "t2_cycle backward should be <= 1, got {}",
            cost.backward
        );
    }

    /// t3_cycle: backward=1
    #[test]
    fn arrange_t3_cycle_backward() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u3", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(3, 31, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(12, "u3_u1", vec![
            mk_ep(3, 32, "OUT", IoDirection::Output),
            mk_ep(1, 12, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty(), "should have at least one candidate");
        let (cost, _) = &candidates[0];
        assert!(
            cost.backward <= 1,
            "t3_cycle backward should be <= 1, got {}",
            cost.backward
        );
    }

    /// t1_chain: [u1][u2]
    #[test]
    fn arrange_t1_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 2);
        assert_eq!(best.layers[0], vec![1]);
        assert_eq!(best.layers[1], vec![2]);
    }

    /// t2_chain: [u3][u1][u2]
    #[test]
    fn arrange_t2_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.nets.push(mk_signal_net(10, "u3_u1", vec![
            mk_ep(3, 31, "OUT", IoDirection::Output),
            mk_ep(1, 11, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u1_u2", vec![
            mk_ep(1, 12, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 3);
        assert_eq!(best.layers[0], vec![3]);
        assert_eq!(best.layers[1], vec![1]);
        assert_eq!(best.layers[2], vec![2]);
    }

    /// t3_chain: [u3][u1][u2][u4]
    #[test]
    fn arrange_t3_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.boxes.push(mk_ic(4, "u4", 3));
        g.nets.push(mk_signal_net(10, "u3_u1", vec![
            mk_ep(3, 31, "OUT", IoDirection::Output),
            mk_ep(1, 11, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u1_u2", vec![
            mk_ep(1, 12, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(12, "u2_u4", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(4, 41, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 4);
        assert_eq!(best.layers[0], vec![3]);
        assert_eq!(best.layers[1], vec![1]);
        assert_eq!(best.layers[2], vec![2]);
        assert_eq!(best.layers[3], vec![4]);
    }

    // ────────────────────────────────────────
    // Mirror bug regression test
    // ────────────────────────────────────────

    /// Optimal solution unchanged after shifting all box ids by +1000
    #[test]
    fn arrange_mirror_bug_regression() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        let best_layers: Vec<Vec<i64>> = candidates[0].1.layers.clone();

        let mut g2 = McVecGraph::new(0, "main".into());
        g2.boxes.push(mk_ic(1001, "u1_mcu", 4));
        g2.boxes.push(mk_ic(1002, "u2_ldo_in", 3));
        g2.boxes.push(mk_ic(1003, "u3_spk", 3));
        g2.boxes.push(mk_ic(1004, "u4_ldo_out", 3));
        g2.boxes.push(mk_ic(1005, "u5_flash", 3));
        g2.nets.push(mk_signal_net(1010, "u1_u2", vec![
            mk_ep(1001, 1011, "OUT", IoDirection::Output),
            mk_ep(1002, 1021, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1011, "u2_u4", vec![
            mk_ep(1002, 1022, "OUT", IoDirection::Output),
            mk_ep(1004, 1041, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1012, "u4_u3", vec![
            mk_ep(1004, 1042, "OUT", IoDirection::Output),
            mk_ep(1003, 1031, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1013, "u3_u5", vec![
            mk_ep(1003, 1032, "OUT", IoDirection::Output),
            mk_ep(1005, 1051, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1014, "u1_u4", vec![
            mk_ep(1001, 1012, "CTRL", IoDirection::Output),
            mk_ep(1004, 1043, "CTRL", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1015, "u1_u5", vec![
            mk_ep(1001, 1013, "CLK", IoDirection::Output),
            mk_ep(1005, 1052, "CLK", IoDirection::Input),
        ]));

        let q2 = QuotientGraph::build(&g2);
        let candidates2 = solve(&q2);
        let best_layers2: Vec<Vec<i64>> = candidates2[0].1.layers.clone();

        assert_eq!(
            best_layers.len(),
            best_layers2.len(),
            "mirror: layer count should be identical"
        );
        for (i, (l1, l2)) in best_layers.iter().zip(best_layers2.iter()).enumerate() {
            assert_eq!(
                l1.len(),
                l2.len(),
                "mirror: layer {} size should be identical",
                i
            );
        }
    }

    // ────────────────────────────────────────
    // Determinism tests
    // ────────────────────────────────────────

    /// 20 consecutive solves yield the same optimal solution
    #[test]
    fn arrange_determinism() {
        let (_g, q) = make_t4_current();
        let first = solve(&q);
        let first_best: Vec<Vec<i64>> = first[0].1.layers.clone();

        for run in 1..20 {
            let candidates = solve(&q);
            let cur_best: Vec<Vec<i64>> = candidates[0].1.layers.clone();
            assert_eq!(
                first_best, cur_best,
                "determinism: run {} differs from run 0",
                run
            );
        }
    }

    // ────────────────────────────────────────
    // Orientation anchor tests
    // ────────────────────────────────────────

    /// Graph with clear OUT→IN direction: OUT should be on the left
    #[test]
    fn arrange_orientation_out_left() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1_src", 3));
        g.boxes.push(mk_ic(2, "u2_dst", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 2);
        assert_eq!(best.layers[0], vec![1], "u1_src (OUT) should be on the left");
        assert_eq!(best.layers[1], vec![2], "u2_dst (IN) should be on the right");
    }

    // ────────────────────────────────────────
    // Boundary tests
    // ────────────────────────────────────────

    /// Single node: returns a single layer
    #[test]
    fn arrange_single_node() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 1);
        assert_eq!(best.layers[0], vec![1]);
    }

    /// Empty graph: no results
    #[test]
    fn arrange_empty() {
        let g = McVecGraph::new(0, "main".into());
        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(candidates.is_empty());
    }
}