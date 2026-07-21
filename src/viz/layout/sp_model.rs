// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Series-Parallel (SP) topology model — pure A-phase, no geometry.
//!
//! Sibling of `ladder_model`. Where `ladder` handles the *bridged* two-lane mesh
//! (a K4/Wheatstone minor), this handles the *clean nested* series-parallel tree.
//! The two are complementary and their trigger sets do not overlap:
//!   * a pure SP graph has no `BridgePassive` tag → `ladder` bails `NoBridge`;
//!   * a bridged graph is not series-parallel → `sp` bails `NonSpBridge`.
//!
//! ## Node / edge / terminal model (post-coalesce)
//! `graph.nets` already carries **coalesced equipotential nodes** (union-find done
//! upstream in the builder). So:
//!   * node   = `VizNet` (net index)
//!   * edge   = `is_two_pin_passive()` box, spanning exactly 2 nets
//!   * terminal = a **non-rail, non-two-pin-passive** box (the IC / port).
//!
//! ### ★ Deviation from `ladder`'s anchor rule (deliberate)
//! `ladder` requires an anchor to touch **>= 2 nets**. An SP network's terminals
//! are *single* connection points (e.g. `u1.6` touches only `N1`), so that rule
//! would find zero anchors here. SP instead takes the two non-passive boxes as the
//! terminals and each terminal's single net as its terminal node. A terminal that
//! touches != 1 net means the network has > 2 effective terminals → bail.
//!
//! This file NEVER touches geometry (x/y/w/h/entry_points). `sp_place.rs` does that.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::vector::graph::McVecGraph;

use super::rails::is_rail_box;

// ============================================================================
// SP tree
// ============================================================================

/// A node of the series-parallel decomposition. Each node also carries its
/// **span** `(a, b)` = the two terminal nets it connects between (a = lower BFS
/// distance from the left terminal), which the ordering/placement passes read.
#[derive(Debug, Clone)]
pub struct SpTree {
    pub kind: SpKind,
    /// span endpoint nearer the left terminal
    pub a: usize,
    /// span endpoint nearer the right terminal
    pub b: usize,
}

#[derive(Debug, Clone)]
pub enum SpKind {
    /// A single 2-pin passive edge.
    Leaf { box_id: i64, name: String },
    /// Children in left→right order.
    Series(Vec<SpTree>),
    /// Children in top→bottom order.
    Parallel(Vec<SpTree>),
}

impl SpTree {
    fn leaf(box_id: i64, name: String, a: usize, b: usize) -> SpTree {
        SpTree { kind: SpKind::Leaf { box_id, name }, a, b }
    }

    /// Minimum box id among the leaves (deterministic tie-break key).
    pub fn min_id(&self) -> i64 {
        match &self.kind {
            SpKind::Leaf { box_id, .. } => *box_id,
            SpKind::Series(cs) | SpKind::Parallel(cs) => {
                cs.iter().map(|c| c.min_id()).min().unwrap_or(i64::MAX)
            }
        }
    }

    /// Leaf count (element count of this sub-network).
    pub fn leaves(&self) -> usize {
        match &self.kind {
            SpKind::Leaf { .. } => 1,
            SpKind::Series(cs) | SpKind::Parallel(cs) => cs.iter().map(|c| c.leaves()).sum(),
        }
    }

    /// Human-readable recursive expression, e.g. `(R1 + C2) ∥ (R3 + ...)`.
    pub fn expr(&self) -> String {
        fn join(cs: &[SpTree], sep: &str) -> String {
            cs.iter()
                .map(|c| match c.kind {
                    SpKind::Leaf { .. } => c.expr(),
                    _ => format!("({})", c.expr()),
                })
                .collect::<Vec<_>>()
                .join(sep)
        }
        match &self.kind {
            SpKind::Leaf { name, .. } => name.clone(),
            SpKind::Series(cs) => join(cs, " + "),
            SpKind::Parallel(cs) => join(cs, " ∥ "),
        }
    }

    /// (w, h) box-packing size in grid units (Phase 2).
    pub fn size(&self) -> (f64, f64) {
        match &self.kind {
            SpKind::Leaf { .. } => (1.0, 1.0),
            SpKind::Series(cs) => {
                let w = cs.iter().map(|c| c.size().0).sum();
                let h = cs.iter().map(|c| c.size().1).fold(0.0, f64::max);
                (w, h)
            }
            SpKind::Parallel(cs) => {
                let w = cs.iter().map(|c| c.size().0).fold(0.0, f64::max);
                let h = cs.iter().map(|c| c.size().1).sum();
                (w, h)
            }
        }
    }
}

// ============================================================================
// Model + bail
// ============================================================================

#[derive(Debug, Clone)]
pub struct SpModel {
    pub root: SpTree,
    /// Terminal net indices (nodes).
    pub left_node: usize,
    pub right_node: usize,
    /// Terminal boxes (drawn as side anchors by `sp_place`).
    pub left_box: i64,
    pub right_box: i64,
}

#[derive(Debug, Clone)]
pub enum SpBail {
    /// Not exactly two non-rail, non-passive terminal boxes.
    AnchorCount(usize),
    /// A terminal box touches != 1 net (network has > 2 effective terminals).
    TerminalFanout { box_id: i64, nets: usize },
    /// A 2-pin passive touching != 2 nets.
    PassiveNetCount { box_id: i64, nets: usize },
    /// An edge whose two ends coalesce to the same net (shorted component).
    SelfLoop { box_id: i64 },
    /// Reduction stuck: a node of degree >= 3 with no parallel pair remains.
    NonSpBridge { stuck_net: usize, residual: Vec<i64> },
}

impl std::fmt::Display for SpBail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpBail::AnchorCount(n) => write!(f, "terminal box count = {n} (need exactly 2)"),
            SpBail::TerminalFanout { box_id, nets } => {
                write!(f, "terminal #{box_id} touches {nets} net(s) (need exactly 1)")
            }
            SpBail::PassiveNetCount { box_id, nets } => {
                write!(f, "passive #{box_id} touches {nets} net(s) (need exactly 2)")
            }
            SpBail::SelfLoop { box_id } => write!(f, "#{box_id} is a self-loop (both ends one net)"),
            SpBail::NonSpBridge { stuck_net, residual } => {
                write!(f, "non series-parallel: stuck at net {stuck_net}, residual edges {residual:?}")
            }
        }
    }
}

// ============================================================================
// Public entry (logging wrapper, mirrors ladder_model)
// ============================================================================

/// What the layouter calls. Dumps the model on success, names the bail on failure.
pub fn try_build_sp_model(graph: &McVecGraph) -> Option<SpModel> {
    match build_sp_model(graph) {
        Ok(m) => {
            crate::vlog!("[sp-model] {}", m.root.expr());
            Some(m)
        }
        Err(reason) => {
            crate::vlog!("[sp-model] bail: {reason}");
            None
        }
    }
}

/// Build the SP model from the net list. Pure; never touches geometry.
pub fn build_sp_model(graph: &McVecGraph) -> Result<SpModel, SpBail> {
    let n_nets = graph.nets.len();
    if n_nets == 0 {
        return Err(SpBail::AnchorCount(0));
    }
    let box_nets = box_net_index(graph);

    // ── 1. Terminals: the non-rail, non-two-pin-passive boxes (exactly 2) ────
    //    (NOTE: no ">= 2 nets" clause — SP terminals are single connection points.)
    let mut terminals: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.id >= 0 && !is_rail_box(b) && !b.is_two_pin_passive())
        .filter(|b| box_nets.get(&b.id).map(|v| !v.is_empty()).unwrap_or(false))
        .map(|b| b.id)
        .collect();
    terminals.sort_unstable();
    if terminals.len() != 2 {
        return Err(SpBail::AnchorCount(terminals.len()));
    }

    // each terminal box must touch exactly one net → that net is its terminal node
    let term_node = |id: i64| -> Result<usize, SpBail> {
        let nets = box_nets.get(&id).cloned().unwrap_or_default();
        if nets.len() != 1 {
            return Err(SpBail::TerminalFanout { box_id: id, nets: nets.len() });
        }
        Ok(nets[0])
    };
    let n0 = term_node(terminals[0])?;
    let n1 = term_node(terminals[1])?;

    // left = more outputs (source); tie-break on lower id (deterministic)
    let out_of = |id: i64| -> usize {
        graph
            .boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.io_summary.outputs)
            .unwrap_or(0)
    };
    let ((left_box, left_node), (right_box, right_node)) =
        if out_of(terminals[0]) >= out_of(terminals[1]) {
            ((terminals[0], n0), (terminals[1], n1))
        } else {
            ((terminals[1], n1), (terminals[0], n0))
        };

    // ── 2. Edges: every 2-pin passive spans exactly two nets ─────────────────
    let mut in_edges: Vec<(i64, String, usize, usize)> = Vec::new();
    for b in &graph.boxes {
        if !b.is_two_pin_passive() {
            continue;
        }
        let nets = box_nets.get(&b.id).cloned().unwrap_or_default();
        if nets.len() != 2 {
            return Err(SpBail::PassiveNetCount { box_id: b.id, nets: nets.len() });
        }
        in_edges.push((b.id, b.display_label().to_string(), nets[0], nets[1]));
    }

    // ── 3. Reduction (graph-agnostic core) ──────────────────────────────────
    let root = reduce(n_nets, &in_edges, left_node, right_node)?;

    Ok(SpModel { root, left_node, right_node, left_box, right_box })
}

// ============================================================================
// Reduction core (operates purely on net indices + edges)
// ============================================================================

struct WEdge {
    a: usize,
    b: usize,
    tree: SpTree,
}

fn bfs_dist(n: usize, edges: &[(i64, String, usize, usize)], left: usize) -> Vec<usize> {
    let mut adj = vec![Vec::<usize>::new(); n];
    for (_, _, a, b) in edges {
        adj[*a].push(*b);
        adj[*b].push(*a);
    }
    let mut dist = vec![usize::MAX; n];
    let mut q = VecDeque::new();
    dist[left] = 0;
    q.push_back(left);
    while let Some(u) = q.pop_front() {
        for &v in &adj[u] {
            if dist[v] == usize::MAX {
                dist[v] = dist[u] + 1;
                q.push_back(v);
            }
        }
    }
    dist
}

fn reduce(
    n: usize,
    edges_in: &[(i64, String, usize, usize)],
    left: usize,
    right: usize,
) -> Result<SpTree, SpBail> {
    let dist = bfs_dist(n, edges_in, left);
    let orient = |a: usize, b: usize| -> (usize, usize) {
        if dist[a] <= dist[b] {
            (a, b)
        } else {
            (b, a)
        }
    };

    let mut edges: Vec<WEdge> = edges_in
        .iter()
        .map(|(id, name, a, b)| {
            let (a, b) = orient(*a, *b);
            WEdge { a, b, tree: SpTree::leaf(*id, name.clone(), a, b) }
        })
        .collect();

    let is_terminal = |nd: usize| nd == left || nd == right;

    loop {
        // (1) self-loop (before parallel, so it can't masquerade as a doubled pair)
        if let Some(e) = edges.iter().find(|e| e.a == e.b) {
            return Err(SpBail::SelfLoop { box_id: e.tree.min_id() });
        }

        // (2) parallel: lowest unordered pair carrying >= 2 edges (multi-edge absorbed at once)
        let pair = find_parallel_pair(&edges);
        if let Some(p) = pair {
            let (grp, rest): (Vec<WEdge>, Vec<WEdge>) = edges
                .into_iter()
                .partition(|e| (e.a.min(e.b), e.a.max(e.b)) == p);
            let (a, b) = (grp[0].a, grp[0].b);
            let mut children = Vec::new();
            for e in grp {
                match e.tree.kind {
                    SpKind::Parallel(cs) => children.extend(cs), // flatten
                    _ => children.push(e.tree),
                }
            }
            let mut edges2 = rest;
            edges2.push(WEdge { a, b, tree: SpTree { kind: SpKind::Parallel(children), a, b } });
            edges = edges2;
            continue;
        }

        // (3) series: lowest-id non-terminal node with exactly two distinct incident edges
        let victim = (0..n)
            .filter(|&nd| !is_terminal(nd))
            .find(|&nd| edges.iter().filter(|e| e.a == nd || e.b == nd).count() == 2);
        if let Some(x) = victim {
            let inc: Vec<usize> = edges
                .iter()
                .enumerate()
                .filter(|(_, e)| e.a == x || e.b == x)
                .map(|(k, _)| k)
                .collect();
            let (i, j) = (inc[0], inc[1]);
            let e2 = edges.remove(j);
            let e1 = edges.remove(i);
            let other = |e: &WEdge| if e.a == x { e.b } else { e.a };
            let (na, nb) = orient(other(&e1), other(&e2));
            let mut children = Vec::new();
            for t in [e1.tree, e2.tree] {
                match t.kind {
                    SpKind::Series(cs) => children.extend(cs), // flatten
                    _ => children.push(t),
                }
            }
            edges.push(WEdge { a: na, b: nb, tree: SpTree { kind: SpKind::Series(children), a: na, b: nb } });
            continue;
        }

        // (4) success: single edge between the two terminals
        if edges.len() == 1
            && (edges[0].a.min(edges[0].b), edges[0].a.max(edges[0].b))
                == (left.min(right), left.max(right))
        {
            let mut root = edges.pop().unwrap().tree;
            normalize(&mut root, true, &dist);
            return Ok(root);
        }

        // stuck → non series-parallel (Wheatstone / K4 minor)
        let stuck = (0..n)
            .filter(|&nd| !is_terminal(nd))
            .find(|&nd| edges.iter().filter(|e| e.a == nd || e.b == nd).count() >= 3)
            .unwrap_or(usize::MAX);
        let residual = edges.iter().map(|e| e.tree.min_id()).collect();
        return Err(SpBail::NonSpBridge { stuck_net: stuck, residual });
    }
}

fn find_parallel_pair(edges: &[WEdge]) -> Option<(usize, usize)> {
    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            let pi = (edges[i].a.min(edges[i].b), edges[i].a.max(edges[i].b));
            let pj = (edges[j].a.min(edges[j].b), edges[j].a.max(edges[j].b));
            if pi == pj {
                return Some(pi);
            }
        }
    }
    None
}

/// Deterministic child ordering.
/// * series: (min span-distance asc, min box-id asc) → left-to-right
/// * parallel: root → (leaf-count asc, min-id asc) so the shortest path is the
///   visual backbone on top; non-root → (min-id asc).
fn normalize(t: &mut SpTree, is_root: bool, dist: &[usize]) {
    match &mut t.kind {
        SpKind::Leaf { .. } => {}
        SpKind::Series(cs) => {
            cs.sort_by(|x, y| {
                (dist[x.a].min(dist[x.b]), x.min_id()).cmp(&(dist[y.a].min(dist[y.b]), y.min_id()))
            });
            for c in cs {
                normalize(c, false, dist);
            }
        }
        SpKind::Parallel(cs) => {
            if is_root {
                cs.sort_by(|x, y| (x.leaves(), x.min_id()).cmp(&(y.leaves(), y.min_id())));
            } else {
                cs.sort_by(|x, y| x.min_id().cmp(&y.min_id()));
            }
            for c in cs {
                normalize(c, false, dist);
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// box_id → net indices it touches (deduped, ascending). Same shape as
/// `ladder_model::box_net_index`.
fn box_net_index(graph: &McVecGraph) -> HashMap<i64, Vec<usize>> {
    let mut out: HashMap<i64, Vec<usize>> = HashMap::new();
    for (ni, net) in graph.nets.iter().enumerate() {
        let mut seen: HashSet<i64> = HashSet::new();
        for e in &net.endpoints {
            if seen.insert(e.box_id) {
                out.entry(e.box_id).or_default().push(ni);
            }
        }
    }
    for v in out.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    out
}
