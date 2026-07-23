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
//! ## Node / edge / terminal model
//! ★ **Precondition**: `graph.nets` must already be one-net-per-equipotential-node.
//! That is *not* what the builder emits on every path — see `coalesce.rs`, which must
//! run before this model (it does, from `flow.rs::phase_prepare`). Without it a pin
//! shared by two connections splits one node across two nets and every passive on it
//! reads as touching 3 nets → `PassiveNetCount` bail.
//!
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
//! ## Reduction rules (in priority order, one per loop turn)
//! 1. **self-loop**  — both ends on one node → bail (shorted component)
//! 2. **parallel**   — the lexicographically smallest node pair carrying >= 2 edges
//! 3. **series**     — lowest-index non-terminal node of degree exactly 2
//! 4. **pendant**    — lowest-index non-terminal node of degree 1: the branch hanging
//!    off it is *not* part of the two-terminal network (a decoupling cap to GND, a
//!    test point). It is pruned into `SpModel::stubs` instead of jamming the reduction.
//!    Without this rule a single `CAP() ~ GND` made the whole model bail — and bail
//!    with `stuck_net: usize::MAX`, since the stuck-node search only looks for degree
//!    >= 3 and a pendant has degree 1.
//! 5. otherwise → success (one edge between the terminals) or `NonSpBridge`.
//!
//! ## ★ Orientation comes from the tree, never from distance
//! An earlier version oriented each edge by BFS distance from the left terminal. On the
//! golden netlist `dist(E) == dist(C) == 2`, so `C5` fell through to "order by net index"
//! and was oriented **backwards** — its Left entry point landed on the node that is
//! electrically to its right, crossing its own leads. The coordinate tests could not see
//! it (they assert `(x_slot, y_row)` only) and the rail test could not either (a reversed
//! `C5` still leaves `N5` on a single row).
//!
//! So: reduction produces *unordered* spans, and a single top-down pass
//! ([`orient_tree`]) then assigns every node its `(a, b)` from the parent's span —
//! chaining `Series` children end-to-end from `a` to `b`. This is exact, needs no
//! distance metric, and simultaneously fixes the left→right ordering of series children
//! (which used to be "sort by distance, tie-break on box id" = guess by designator).
//!
//! This file NEVER touches geometry (x/y/w/h/entry_points). `sp_place.rs` does that.

use std::collections::{HashMap, HashSet};

use crate::vector::graph::McVecGraph;

use super::rails::is_rail_box;

// ============================================================================
// SP tree
// ============================================================================

/// A node of the series-parallel decomposition. Each node also carries its
/// **span** `(a, b)` = the two terminal nets it connects between, with `a` the end
/// nearer the left terminal. The span is authoritative only after [`orient_tree`].
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
    /// A single 2-pin passive edge. `order` = index in `graph.boxes` = 源码书写顺序。
    Leaf {
        box_id: i64,
        name: String,
        order: usize,
    },
    /// Children in left→right order.
    Series(Vec<SpTree>),
    /// Children in top→bottom order.
    Parallel(Vec<SpTree>),
}

impl SpTree {
    fn leaf(box_id: i64, name: String, order: usize, a: usize, b: usize) -> SpTree {
        SpTree {
            kind: SpKind::Leaf {
                box_id,
                name,
                order,
            },
            a,
            b,
        }
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

    /// 子树里最靠前的书写位置（确定性排序键）。
    pub fn min_order(&self) -> usize {
        match &self.kind {
            SpKind::Leaf { order, .. } => *order,
            SpKind::Series(cs) | SpKind::Parallel(cs) => {
                cs.iter()
                    .map(|c| c.min_order())
                    .min()
                    .unwrap_or(usize::MAX)
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

    /// Every leaf's box id, in tree order (left→right, top→bottom).
    pub fn leaf_ids(&self) -> Vec<i64> {
        let mut out = Vec::new();
        self.collect_leaf_ids(&mut out);
        out
    }

    fn collect_leaf_ids(&self, out: &mut Vec<i64>) {
        match &self.kind {
            SpKind::Leaf { box_id, .. } => out.push(*box_id),
            SpKind::Series(cs) | SpKind::Parallel(cs) => {
                for c in cs {
                    c.collect_leaf_ids(out);
                }
            }
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

    /// 按**源语法**打印（`-` 串联 / `+` 并联），便于和作者写的表达式逐字对照。
    pub fn expr_source_syntax(&self) -> String {
        fn join(cs: &[SpTree], sep: &str) -> String {
            cs.iter()
                .map(|c| match c.kind {
                    SpKind::Leaf { .. } => c.expr_source_syntax(),
                    _ => format!("({})", c.expr_source_syntax()),
                })
                .collect::<Vec<_>>()
                .join(sep)
        }
        match &self.kind {
            SpKind::Leaf { name, .. } => name.clone(),
            SpKind::Series(cs) => join(cs, " - "),
            SpKind::Parallel(cs) => join(cs, " + "),
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

/// A branch that hangs off the two-terminal network by one node (bypass cap to GND,
/// test point, unterminated stub). Not part of the SP tree; `sp_place` drops it
/// vertically below its attachment node and leaves its wiring to the router.
#[derive(Debug, Clone)]
pub struct SpStub {
    /// Node the branch hangs from (a node of the main network).
    pub node: usize,
    /// The far end — a node of degree 1 (often a rail flag's stub net).
    pub dangling: usize,
    /// The branch itself, already SP-reduced and oriented `node → dangling`.
    pub tree: SpTree,
}

#[derive(Debug, Clone)]
pub struct SpModel {
    pub root: SpTree,
    /// Terminal net indices (nodes).
    pub left_node: usize,
    pub right_node: usize,
    /// Terminal boxes (drawn as side anchors by `sp_place`).
    pub left_box: i64,
    pub right_box: i64,
    /// Pendant branches pruned out of the reduction, in prune order.
    pub stubs: Vec<SpStub>,
}

impl SpModel {
    /// Every box the model owns geometry for (terminals excluded).
    pub fn placed_boxes(&self) -> Vec<i64> {
        let mut ids = self.root.leaf_ids();
        for s in &self.stubs {
            ids.extend(s.tree.leaf_ids());
        }
        ids
    }
}

#[derive(Debug, Clone)]
pub enum SpBail {
    /// Not exactly two non-rail, non-passive terminal boxes.
    AnchorCount(usize),
    /// A terminal box touches != 1 net (network has > 2 effective terminals).
    TerminalFanout { box_id: i64, nets: usize },
    /// A 2-pin passive touching != 2 nets. ★ If this fires with `nets: 3` on an
    /// otherwise sane netlist, `coalesce.rs` did not run: the pin is shared by two
    /// un-merged connections.
    PassiveNetCount { box_id: i64, nets: usize },
    /// An edge whose two ends coalesce to the same net (shorted component).
    SelfLoop { box_id: i64 },
    /// Reduction stuck: no parallel pair, no degree-2 node, no pendant left.
    NonSpBridge {
        stuck_net: usize,
        residual: Vec<i64>,
    },
    /// The terminals are not connected through passives at all.
    Disconnected { left: usize, right: usize },
}

impl std::fmt::Display for SpBail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpBail::AnchorCount(n) => write!(f, "terminal box count = {n} (need exactly 2)"),
            SpBail::TerminalFanout { box_id, nets } => {
                write!(
                    f,
                    "terminal #{box_id} touches {nets} net(s) (need exactly 1)"
                )
            }
            SpBail::PassiveNetCount { box_id, nets } => {
                write!(
                    f,
                    "passive #{box_id} touches {nets} net(s) (need exactly 2) \
                     — did coalesce_equipotential_nets run?"
                )
            }
            SpBail::SelfLoop { box_id } => {
                write!(f, "#{box_id} is a self-loop (both ends one net)")
            }
            SpBail::NonSpBridge {
                stuck_net,
                residual,
            } => {
                write!(
                    f,
                    "non series-parallel: stuck at net {stuck_net}, residual edges {residual:?}"
                )
            }
            SpBail::Disconnected { left, right } => {
                write!(f, "terminals {left} and {right} are not connected")
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
            let left_name = graph
                .boxes
                .iter()
                .find(|b| b.id == m.left_box)
                .map(|b| b.display_label().to_string())
                .unwrap_or_else(|| "?".into());
            let right_name = graph
                .boxes
                .iter()
                .find(|b| b.id == m.right_box)
                .map(|b| b.display_label().to_string())
                .unwrap_or_else(|| "?".into());
            crate::vlog!("[sp-model] {}", m.root.expr());
            crate::vlog!(
                "[sp-model] src: {} - ({}) - {}",
                left_name,
                m.root.expr_source_syntax(),
                right_name
            );
            for s in &m.stubs {
                crate::vlog!(
                    "[sp-model] stub at node {}: {} (pruned)",
                    s.node,
                    s.tree.expr()
                );
            }
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
            return Err(SpBail::TerminalFanout {
                box_id: id,
                nets: nets.len(),
            });
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
    let mut in_edges: Vec<(i64, String, usize, usize, usize)> = Vec::new(); // + order
    for (bi, b) in graph.boxes.iter().enumerate() {
        if !b.is_two_pin_passive() {
            continue;
        }
        let nets = box_nets.get(&b.id).cloned().unwrap_or_default();
        // A shorted passive has *both* pins on one node, so it reads as "touches 1 net".
        // Name it for what it is instead of hiding it behind a count mismatch.
        if nets.len() == 1 {
            return Err(SpBail::SelfLoop { box_id: b.id });
        }
        if nets.len() != 2 {
            return Err(SpBail::PassiveNetCount {
                box_id: b.id,
                nets: nets.len(),
            });
        }
        in_edges.push((
            b.id,
            b.display_label().to_string(),
            nets[0],
            nets[1],
            bi,
        ));
    }

    // ── 3. Reduction (graph-agnostic core) ──────────────────────────────────
    let (mut root, mut stubs) = reduce(n_nets, &in_edges, left_node, right_node)?;

    // ── 4. Orientation + ordering (pure tree passes, in this order) ─────────
    orient_tree(&mut root, left_node, right_node);
    order_parallel(&mut root, true);
    for s in &mut stubs {
        orient_tree(&mut s.tree, s.node, s.dangling);
        order_parallel(&mut s.tree, false);
    }

    Ok(SpModel {
        root,
        left_node,
        right_node,
        left_box,
        right_box,
        stubs,
    })
}

// ============================================================================
// Reduction core (operates purely on net indices + edges)
// ============================================================================

struct WEdge {
    a: usize,
    b: usize,
    tree: SpTree,
}

impl WEdge {
    fn pair(&self) -> (usize, usize) {
        (self.a.min(self.b), self.a.max(self.b))
    }
    fn touches(&self, nd: usize) -> bool {
        self.a == nd || self.b == nd
    }
    fn other(&self, nd: usize) -> usize {
        if self.a == nd {
            self.b
        } else {
            self.a
        }
    }
}

/// Reduce the edge set to a single terminal-to-terminal edge, pruning pendants.
///
/// Spans produced here are **unordered** — `orient_tree` fixes direction afterwards.
fn reduce(
    n: usize,
    edges_in: &[(i64, String, usize, usize, usize)],
    left: usize,
    right: usize,
) -> Result<(SpTree, Vec<SpStub>), SpBail> {
    let mut edges: Vec<WEdge> = edges_in
        .iter()
        .map(|(id, name, a, b, order)| WEdge {
            a: *a,
            b: *b,
            tree: SpTree::leaf(*id, name.clone(), *order, *a, *b),
        })
        .collect();
    let mut stubs: Vec<SpStub> = Vec::new();

    let is_terminal = |nd: usize| nd == left || nd == right;
    let degree = |edges: &[WEdge], nd: usize| edges.iter().filter(|e| e.touches(nd)).count();

    loop {
        // (1) self-loop (before parallel, so it can't masquerade as a doubled pair)
        if let Some(e) = edges.iter().find(|e| e.a == e.b) {
            return Err(SpBail::SelfLoop {
                box_id: e.tree.min_id(),
            });
        }

        // (2) parallel: smallest node pair carrying >= 2 edges (multi-edge absorbed at once)
        if let Some(p) = find_parallel_pair(&edges) {
            let (grp, rest): (Vec<WEdge>, Vec<WEdge>) =
                edges.into_iter().partition(|e| e.pair() == p);
            let (a, b) = (grp[0].a, grp[0].b);
            let mut children = Vec::new();
            for e in grp {
                match e.tree.kind {
                    SpKind::Parallel(cs) => children.extend(cs), // flatten
                    _ => children.push(e.tree),
                }
            }
            let mut edges2 = rest;
            edges2.push(WEdge {
                a,
                b,
                tree: SpTree {
                    kind: SpKind::Parallel(children),
                    a,
                    b,
                },
            });
            edges = edges2;
            continue;
        }

        // (3) series: lowest-index non-terminal node with exactly two incident edges
        let victim = (0..n)
            .filter(|&nd| !is_terminal(nd))
            .find(|&nd| degree(&edges, nd) == 2);
        if let Some(x) = victim {
            let inc: Vec<usize> = edges
                .iter()
                .enumerate()
                .filter(|(_, e)| e.touches(x))
                .map(|(k, _)| k)
                .collect();
            let (i, j) = (inc[0], inc[1]);
            let e2 = edges.remove(j); // j > i, so remove j first
            let e1 = edges.remove(i);
            let (na, nb) = (e1.other(x), e2.other(x));
            let mut children = Vec::new();
            for t in [e1.tree, e2.tree] {
                match t.kind {
                    SpKind::Series(cs) => children.extend(cs), // flatten
                    _ => children.push(t),
                }
            }
            edges.push(WEdge {
                a: na,
                b: nb,
                tree: SpTree {
                    kind: SpKind::Series(children),
                    a: na,
                    b: nb,
                },
            });
            continue;
        }

        // (4) pendant: lowest-index non-terminal node of degree 1 → prune to a stub.
        //     A bypass cap to GND lands here; without this rule it would deadlock the
        //     reduction and surface as a bogus `NonSpBridge`.
        let pendant = (0..n)
            .filter(|&nd| !is_terminal(nd))
            .find(|&nd| degree(&edges, nd) == 1);
        if let Some(x) = pendant {
            let k = edges.iter().position(|e| e.touches(x)).expect("degree 1");
            let e = edges.remove(k);
            let attach = e.other(x);
            stubs.push(SpStub {
                node: attach,
                dangling: x,
                tree: e.tree,
            });
            continue;
        }

        // (5a) success: single edge between the two terminals
        if edges.len() == 1 && edges[0].pair() == (left.min(right), left.max(right)) {
            let root = edges.pop().expect("len == 1").tree;
            return Ok((root, stubs));
        }

        // (5b) nothing left to join the terminals
        if edges.is_empty() {
            return Err(SpBail::Disconnected { left, right });
        }

        // (5c) stuck → non series-parallel (Wheatstone / K4 minor)
        let stuck = (0..n)
            .filter(|&nd| !is_terminal(nd))
            .find(|&nd| degree(&edges, nd) >= 3)
            .unwrap_or_else(|| {
                // no interior hub: report the lowest node still carrying an edge, so the
                // log never prints `usize::MAX`
                edges.iter().map(|e| e.a.min(e.b)).min().unwrap_or(left)
            });
        let mut residual: Vec<i64> = edges.iter().map(|e| e.tree.min_id()).collect();
        residual.sort_unstable();
        return Err(SpBail::NonSpBridge {
            stuck_net: stuck,
            residual,
        });
    }
}

/// Lexicographically smallest node pair carrying >= 2 edges. Deterministic regardless
/// of the order edges happen to sit in the working vector.
fn find_parallel_pair(edges: &[WEdge]) -> Option<(usize, usize)> {
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut best: Option<(usize, usize)> = None;
    for e in edges {
        let p = e.pair();
        if !seen.insert(p) && best.map_or(true, |b| p < b) {
            best = Some(p);
        }
    }
    best
}

// ============================================================================
// Orientation + ordering (top-down tree passes)
// ============================================================================

/// Assign every node its `(a, b)` from the parent's span, and put `Series` children in
/// true electrical order by chaining them `a → b`.
///
/// Children arrive with the *right* pair of endpoints but an arbitrary direction, so the
/// chain walk only ever tests membership — it never trusts the incoming `a`/`b` order.
pub fn orient_tree(t: &mut SpTree, a: usize, b: usize) {
    t.a = a;
    t.b = b;
    match &mut t.kind {
        SpKind::Leaf { .. } => {}
        // every parallel branch spans the same pair as its parent
        SpKind::Parallel(cs) => {
            for c in cs.iter_mut() {
                orient_tree(c, a, b);
            }
        }
        SpKind::Series(cs) => {
            let mut pool: Vec<Option<SpTree>> = std::mem::take(cs).into_iter().map(Some).collect();
            let mut chained: Vec<SpTree> = Vec::with_capacity(pool.len());
            let mut cur = a;
            for _ in 0..pool.len() {
                // candidates incident to `cur`; tie-break on min_order for determinism
                let pick = (0..pool.len())
                    .filter(|&i| {
                        pool[i]
                            .as_ref()
                            .map(|c| c.a == cur || c.b == cur)
                            .unwrap_or(false)
                    })
                    .min_by_key(|&i| {
                        pool[i]
                            .as_ref()
                            .map(|c| c.min_order())
                            .unwrap_or(usize::MAX)
                    });
                let Some(i) = pick else { break };
                let mut child = pool[i].take().expect("picked a live slot");
                let next = if child.a == cur { child.b } else { child.a };
                orient_tree(&mut child, cur, next);
                chained.push(child);
                cur = next;
            }
            // Defensive: a well-formed series chain consumes every child. If the walk
            // broke early (should be impossible), keep the leftovers rather than drop
            // components off the schematic.
            for leftover in pool.into_iter().flatten() {
                let (ca, cb) = (leftover.a, leftover.b);
                let mut c = leftover;
                orient_tree(&mut c, ca, cb);
                chained.push(c);
            }
            *cs = chained;
        }
    }
}

/// Vertical (top→bottom) ordering of parallel branches — a pure *rendering* choice, kept
/// separate from topology on purpose.
///
/// Uniform rule: `min_order()` asc — 作者书写顺序，上到下。
/// 根与非根同一条规则，少一个特例。
pub fn order_parallel(t: &mut SpTree, _is_root: bool) {
    match &mut t.kind {
        SpKind::Leaf { .. } => {}
        SpKind::Series(cs) => {
            for c in cs {
                order_parallel(c, false);
            }
        }
        SpKind::Parallel(cs) => {
            cs.sort_by_key(|c| c.min_order());
            for c in cs {
                order_parallel(c, false);
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

// ============================================================================
// Tests — topology only (geometry lives in sp_place)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::netdef::{EndpointRef, VizNet};
    use crate::vector::graph::{BoxKind, McVecBox, McVecGraph, NetKind, Symbol};

    fn term(id: i64, name: &str, outputs: usize) -> McVecBox {
        let mut io = IoSummary::new();
        io.outputs = outputs;
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Ic,
            None,
            None,
            1,
            io,
        )
    }
    fn passive(id: i64, name: &str, sym: Symbol) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::TwoPin,
            sym,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
        )
    }
    fn net(nid: i64, name: &str, eps: &[(i64, i64)]) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            NetKind::Signal,
            eps.iter()
                .map(|&(b, p)| EndpointRef::new(b, p, format!("p{p}")))
                .collect(),
        )
    }

    /// Coalesced golden: 5 nodes, 6 edges.
    fn golden() -> McVecGraph {
        let mut g = McVecGraph::new(1, "main".into());
        g.boxes.push(passive(1, "R1", Symbol::Resistor));
        g.boxes.push(passive(2, "C2", Symbol::Capacitor));
        g.boxes.push(passive(3, "R3", Symbol::Resistor));
        g.boxes.push(passive(4, "R4", Symbol::Resistor));
        g.boxes.push(passive(5, "C5", Symbol::Capacitor));
        g.boxes.push(passive(6, "R6", Symbol::Resistor));
        g.boxes.push(term(101, "u1", 1));
        g.boxes.push(term(102, "u2", 0));
        g.nets.push(net(0, "N1", &[(101, 6), (1, 11), (3, 31)]));
        g.nets.push(net(1, "N2", &[(1, 12), (2, 21)]));
        g.nets
            .push(net(2, "N3", &[(2, 22), (102, 6), (5, 52), (6, 62)]));
        g.nets.push(net(3, "N4", &[(3, 32), (4, 41), (6, 61)]));
        g.nets.push(net(4, "N5", &[(4, 42), (5, 51)]));
        g
    }

    fn span_of(t: &SpTree, id: i64) -> Option<(usize, usize)> {
        match &t.kind {
            SpKind::Leaf { box_id, .. } if *box_id == id => Some((t.a, t.b)),
            SpKind::Leaf { .. } => None,
            SpKind::Series(cs) | SpKind::Parallel(cs) => cs.iter().find_map(|c| span_of(c, id)),
        }
    }

    #[test]
    fn golden_expression_and_size() {
        let m = build_sp_model(&golden()).expect("should be SP");
        assert_eq!(m.root.expr(), "(R1 + C2) ∥ (R3 + ((R4 + C5) ∥ R6))");
        assert_eq!(m.root.size(), (3.0, 3.0));
        assert!(m.stubs.is_empty());
    }

    /// ★ The bug the coordinate tests could not see: `dist(N5) == dist(N3) == 2`, so the
    /// old distance-based orientation fell back to net index and put `C5` in backwards.
    #[test]
    fn every_leaf_is_oriented_left_to_right() {
        let m = build_sp_model(&golden()).unwrap();
        // node indices: N1=0 (left), N2=1, N3=2 (right), N4=3, N5=4
        assert_eq!(span_of(&m.root, 1), Some((0, 1)), "R1: N1 → N2");
        assert_eq!(span_of(&m.root, 2), Some((1, 2)), "C2: N2 → N3");
        assert_eq!(span_of(&m.root, 3), Some((0, 3)), "R3: N1 → N4");
        assert_eq!(span_of(&m.root, 4), Some((3, 4)), "R4: N4 → N5");
        assert_eq!(
            span_of(&m.root, 5),
            Some((4, 2)),
            "C5: N5 → N3 (was reversed)"
        );
        assert_eq!(span_of(&m.root, 6), Some((3, 2)), "R6: N4 → N3");
    }

    #[test]
    fn series_children_are_chained_not_sorted_by_id() {
        // C9 — R1 — R7 in series between the terminals: designator order is 1,7,9 but the
        // electrical order is 9,1,7. Distance-sort + id tie-break used to get this wrong.
        let mut g = McVecGraph::new(1, "main".into());
        g.boxes.push(passive(1, "R1", Symbol::Resistor));
        g.boxes.push(passive(7, "R7", Symbol::Resistor));
        g.boxes.push(passive(9, "C9", Symbol::Capacitor));
        g.boxes.push(term(101, "u1", 1));
        g.boxes.push(term(102, "u2", 0));
        g.nets.push(net(0, "L", &[(101, 6), (9, 91)]));
        g.nets.push(net(1, "X", &[(9, 92), (1, 11)]));
        g.nets.push(net(2, "Y", &[(1, 12), (7, 71)]));
        g.nets.push(net(3, "R", &[(7, 72), (102, 6)]));
        let m = build_sp_model(&g).unwrap();
        assert_eq!(m.root.expr(), "C9 + R1 + R7");
        assert_eq!(m.root.leaf_ids(), vec![9, 1, 7]);
    }

    /// ★ A single decoupling cap used to kill the whole model — and report
    /// `stuck_net: 18446744073709551615`, because the stuck search only looks for
    /// degree >= 3 while a pendant has degree 1.
    #[test]
    fn pendant_branch_is_pruned_not_fatal() {
        let mut g = golden();
        // C7 hangs off N4 (node 3) down to a new dangling node 5
        g.boxes.push(passive(7, "C7", Symbol::Capacitor));
        g.nets[3]
            .endpoints
            .push(EndpointRef::new(7, 71, "p71".to_string()));
        g.nets.push(net(5, "N6", &[(7, 72)]));

        let m = build_sp_model(&g).expect("pendant must not break the reduction");
        assert_eq!(m.root.expr(), "(R1 + C2) ∥ (R3 + ((R4 + C5) ∥ R6))");
        assert_eq!(m.stubs.len(), 1);
        assert_eq!(m.stubs[0].node, 3, "attached at N4");
        assert_eq!(m.stubs[0].tree.leaf_ids(), vec![7]);
        assert_eq!(m.stubs[0].tree.a, 3, "oriented node → dangling");
        assert_eq!(m.stubs[0].tree.b, 5);
        assert!(m.placed_boxes().contains(&7));
    }

    #[test]
    fn wheatstone_bridge_bails_non_sp_with_a_real_node() {
        let mut g = McVecGraph::new(1, "main".into());
        for (id, nm) in [(1, "R1"), (2, "R2"), (3, "R3"), (4, "R4"), (5, "R5")] {
            g.boxes.push(passive(id, nm, Symbol::Resistor));
        }
        g.boxes.push(term(101, "u1", 1));
        g.boxes.push(term(102, "u2", 0));
        g.nets.push(net(0, "L", &[(101, 6), (1, 11), (2, 21)]));
        g.nets.push(net(1, "X", &[(1, 12), (3, 31), (5, 51)]));
        g.nets.push(net(2, "Y", &[(2, 22), (4, 41), (5, 52)]));
        g.nets.push(net(3, "R", &[(3, 32), (4, 42), (102, 6)]));
        match build_sp_model(&g) {
            Err(SpBail::NonSpBridge {
                stuck_net,
                residual,
            }) => {
                assert!(stuck_net == 1 || stuck_net == 2, "must name a real node");
                assert_eq!(residual, vec![1, 2, 3, 4, 5]);
            }
            other => panic!("expected NonSpBridge, got {other:?}"),
        }
    }

    #[test]
    fn self_loop_bails() {
        let mut g = McVecGraph::new(1, "main".into());
        g.boxes.push(passive(20, "R20", Symbol::Resistor));
        g.boxes.push(term(101, "u1", 1));
        g.boxes.push(term(102, "u2", 0));
        g.nets.push(net(0, "L", &[(101, 6), (20, 201), (20, 202)]));
        g.nets.push(net(1, "R", &[(102, 6)]));
        match build_sp_model(&g) {
            Err(SpBail::SelfLoop { box_id }) => assert_eq!(box_id, 20),
            other => panic!("expected SelfLoop, got {other:?}"),
        }
    }

    #[test]
    fn empty_graph_does_not_panic() {
        let g = McVecGraph::new(1, "main".into());
        assert!(matches!(build_sp_model(&g), Err(SpBail::AnchorCount(0))));
    }

    /// ★ 源表达式本身就是一棵 SP 树，"作者写的表达式" 与 "从 netlist 反推出来的表达式" 必须逐字相等。
    #[test]
    fn recovered_expression_matches_the_authored_source() {
        let m = build_sp_model(&golden()).unwrap();
        assert_eq!(
            m.root.expr_source_syntax(),
            "(R1 - C2) + (R3 - ((R4 - C5) + R6))"
        );
    }

    /// ★ 作者书写顺序驱动并联分支的上下序：把源里两条支路对调（等价于 R3 分支先写），上下应该跟着换。
    #[test]
    fn authored_branch_order_drives_top_to_bottom() {
        let mut g = golden();
        let i = g.boxes.iter().position(|b| b.id == 3).unwrap();
        let b3 = g.boxes.remove(i);
        g.boxes.insert(0, b3); // R3 变成第一个书写的元件
        let m = build_sp_model(&g).unwrap();
        assert_eq!(m.root.expr(), "(R3 + ((R4 + C5) ∥ R6)) ∥ (R1 + C2)");
    }

    /// dump 顺序：__net_0=B, __net_1=E, __net_2=D, __net_3=C(右端子), __net_4=A(左端子)
    fn real_netlist() -> McVecGraph {
        let mut g = McVecGraph::new(1, "main".into());
        g.boxes.push(passive(1, "R1", Symbol::Resistor));
        g.boxes.push(passive(2, "C2", Symbol::Capacitor));
        g.boxes.push(passive(3, "R3", Symbol::Resistor));
        g.boxes.push(passive(4, "R4", Symbol::Resistor));
        g.boxes.push(passive(5, "C5", Symbol::Capacitor));
        g.boxes.push(passive(6, "R6", Symbol::Resistor));
        g.boxes.push(term(101, "u1", 1));
        g.boxes.push(term(102, "u2", 0));
        g.nets.push(net(0, "__net_0", &[(1, 12), (2, 21)]));
        g.nets.push(net(1, "__net_1", &[(4, 42), (5, 51)]));
        g.nets.push(net(2, "__net_2", &[(4, 41), (6, 61), (3, 32)]));
        g.nets
            .push(net(3, "__net_3", &[(5, 52), (6, 62), (2, 22), (102, 6)]));
        g.nets.push(net(4, "__net_4", &[(1, 11), (3, 31), (101, 6)]));
        g
    }

    /// ★ 节点下标顺序无关：真实 dump 顺序与 fixture 不同，但结果完全一致。
    #[test]
    fn real_netlist_node_order_is_irrelevant() {
        let m = build_sp_model(&real_netlist()).expect("still SP");
        assert_eq!(m.root.expr(), "(R1 + C2) ∥ (R3 + ((R4 + C5) ∥ R6))");
        assert_eq!(m.root.size(), (3.0, 3.0));
        assert_eq!((m.left_node, m.right_node), (4, 3));
        // 朝向按真实下标：C5 = E(1) → C(3)
        assert_eq!(span_of(&m.root, 5), Some((1, 3)));
        assert_eq!(span_of(&m.root, 1), Some((4, 0)), "R1: A → B");
    }
}
