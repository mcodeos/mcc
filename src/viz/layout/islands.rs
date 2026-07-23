// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Island decomposition — pure topology, no geometry.
//!
//! Where `sp_model` and `ladder_model` each assume they own the entire layer, this
//! module decomposes a mixed graph into independent **islands** (connected components
//! along passive edges only), so each island can be handed to the right model.
//!
//! ## Algorithm
//! 1. Build a passive-edge graph: nodes = nets, edges = two-pin passive boxes
//! 2. Find connected components (islands) along passive edges only
//! 3. For each island, find its **boundary nodes** (nets touched by non-passive boxes)
//! 4. Classify:
//!    - 2 boundary nodes → two-terminal → try SP, then ladder
//!    - 1 boundary node  → stub (decoupling cap, test point)
//!    - ≥3 boundary nodes → multi-port (fallback to generic flow for now)
//! 5. Direct connections: nets with 0 passive boxes between two terminals → direct band
//!
//! ## Phase 1 (this commit): decompose + log only, no geometry changes.

use std::collections::{HashMap, HashSet};

use crate::vector::graph::{EntryPoint, EntrySide, McVecGraph, Point};

use super::entry_points::distribute_terminal_pins;
use super::ladder_model::LadderModel;
use super::ladder_place::apply_ladder_model_at;
use super::rails::is_rail_box;
use super::sp_model::{build_sp_tree, SpModel, SubNet};
use super::sp_place::apply_sp_model_at;

// ============================================================================
// Island model
// ============================================================================

/// A connected component of passive edges.
#[derive(Debug, Clone)]
pub struct Island {
    /// The nets (node indices) in this island.
    pub nodes: Vec<usize>,
    /// (box_id, label, node_a, node_b) — passive edges.
    pub edges: Vec<(i64, String, usize, usize)>,
    /// Boundary nets — touched by non-passive, non-rail boxes.
    pub boundaries: Vec<usize>,
}

/// Classification of an island.
///
/// The 2D criterion `(boundary_boxes, boundary_nets)` decides:
/// - (2, 2) → **Sp** (two-terminal, one net per terminal → SP tree)
/// - (2, n>2) → **Ladder** (two terminals, k lanes each → rung-ladder)
/// - (1, _) → **Stub** (pendant branch, decoupling cap)
/// - otherwise → **MultiPort** (fallback to generic flow)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IslandKind {
    /// Two-terminal, one net per terminal → try SP tree.
    Sp,
    /// Two terminals, k lanes each → try rung-ladder.
    Ladder { lanes: usize },
    /// Single boundary: stub (decoupling cap, etc.).
    Stub,
    /// Three or more boundary boxes: multi-port, fallback to generic flow.
    MultiPort,
}

/// A direct wire between two terminals (no passive components on the path).
#[derive(Debug, Clone)]
pub struct DirectBand {
    pub net: usize,
    pub left_box: i64,
    pub right_box: i64,
}

/// Result of decomposition.
#[derive(Debug, Clone)]
pub struct Decomposition {
    pub islands: Vec<Island>,
    pub direct_bands: Vec<DirectBand>,
}

// ============================================================================
// ★ Band assembly — Phase B/C/D
// ============================================================================

/// A band that can be stacked vertically in the island assembly.
/// Each band owns its passives and reports its terminal pins, but never
/// places terminals — that's done once globally in Phase D.
enum Band {
    Sp {
        model: SpModel,
        island_idx: usize,
    },
    Ladder {
        model: LadderModel,
        island_idx: usize,
    },
    Direct {
        db: DirectBand,
        left_pin: i64,
        right_pin: i64,
    },
}

/// Grid → pixel constants. Shared with sp_place; ladder uses its own internally.
const COL_W: f64 = 120.0;
const ROW_H: f64 = 80.0;
const MARGIN: f64 = 60.0;
const TERM_GAP: f64 = COL_W * 0.4;
const BAND_GAP: f64 = 60.0;
/// Conservative column width for ladder bands (col_step ≥ SLOT_MIN=180,
/// typical = (110+82+32).max(180) = 224).
const LADDER_COL_W: f64 = 224.0;
/// Minimum vertical clearance between two passive elements in adjacent lanes.
const PASSIVE_GAP: f64 = 8.0;

impl Band {
    /// Per-lane row height used for pin y-offset calculation.
    /// SP / Direct: ROW_H; Ladder: ROW_H.max(rung_h + PASSIVE_GAP).
    fn row_h(&self, graph: &McVecGraph) -> f64 {
        match self {
            Band::Sp { .. } | Band::Direct { .. } => ROW_H,
            Band::Ladder { model, .. } => {
                let mut rung_h = 0.0f64;
                for s in &model.series {
                    if let Some(b) = graph.boxes.iter().find(|x| x.id == s.box_id) {
                        rung_h = rung_h.max(b.w.min(b.h));
                    }
                }
                ROW_H.max(rung_h + PASSIVE_GAP)
            }
        }
    }

    /// Pixel extent: (width_px, height_px). Bands are stacked by height,
    /// and `x_right` is computed from the max width.
    /// SP: cols×COL_W; Ladder: cols×LADDER_COL_W, rows×row_h where
    /// row_h = ROW_H.max(rung_h + PASSIVE_GAP); Direct: COL_W.
    fn extent_px(&self, graph: &McVecGraph) -> (f64, f64) {
        match self {
            Band::Sp { model, .. } => {
                let (cols, rows) = model.size();
                (cols * COL_W, rows * ROW_H)
            }
            Band::Ladder { model, .. } => {
                let (cols, rows) = model.size();
                let row_h = self.row_h(graph);
                (cols * LADDER_COL_W, rows * row_h)
            }
            Band::Direct { .. } => (COL_W, ROW_H),
        }
    }

    /// Terminal pins this band uses: (left_pins, right_pins), ordered top→bottom.
    fn terminal_pins(&self) -> (Vec<i64>, Vec<i64>) {
        match self {
            Band::Sp { model, .. } => model.terminal_pins(),
            Band::Ladder { model, .. } => model.terminal_pins(),
            Band::Direct {
                left_pin,
                right_pin,
                ..
            } => (vec![*left_pin], vec![*right_pin]),
        }
    }

    /// The two terminal boxes this band connects to.
    fn terminal_boxes(&self) -> (i64, i64) {
        match self {
            Band::Sp { model, .. } => (model.left_box, model.right_box),
            Band::Ladder { model, .. } => (model.left, model.right),
            Band::Direct { db, .. } => (db.left_box, db.right_box),
        }
    }

    /// Place passives only (no terminals). Each band's passives are placed
    /// relative to the band's origin.
    fn place_passives(&self, graph: &mut McVecGraph, origin: Point, x_right: f64) {
        match self {
            Band::Sp { model, .. } => {
                apply_sp_model_at(graph, model, origin, x_right);
            }
            Band::Ladder { model, .. } => {
                apply_ladder_model_at(graph, model, origin, x_right);
            }
            Band::Direct { .. } => {
                // Direct band has no passives to place — it's just a wire
            }
        }
    }
}

// ============================================================================
// ★ TerminalGraph — model the terminal-to-band wiring
// ============================================================================

/// A bipartite graph: terminals ↔ bands.
/// Each band connects exactly two terminals; each terminal may connect to
/// multiple bands. This graph is used to derive a **linear order** of terminals
/// along the x-axis, then place bands in the gaps between adjacent terminals.
struct TerminalGraph {
    /// terminal box_id → indices of bands connected to it
    incident: std::collections::BTreeMap<i64, Vec<usize>>,
    /// band index → (terminal_a, terminal_b)
    ends: Vec<(i64, i64)>,
}

impl TerminalGraph {
    fn any_node(&self) -> i64 {
        *self.incident.keys().next().unwrap()
    }

    /// Neighbour terminals of `t` (the other end of each band incident to `t`).
    fn adjacent(&self, t: i64) -> Vec<i64> {
        let mut neighbours: Vec<i64> = Vec::new();
        if let Some(indices) = self.incident.get(&t) {
            for &bi in indices {
                let (a, b) = self.ends[bi];
                neighbours.push(if a == t { b } else { a });
            }
        }
        neighbours.sort_unstable();
        neighbours.dedup();
        neighbours
    }

    /// BFS: find the terminal farthest from `start`.
    fn bfs_farthest(&self, start: i64) -> i64 {
        let mut dist: HashMap<i64, usize> = HashMap::new();
        let mut queue: std::collections::VecDeque<i64> = std::collections::VecDeque::new();
        dist.insert(start, 0);
        queue.push_back(start);
        let mut farthest = start;
        while let Some(t) = queue.pop_front() {
            let d = dist[&t];
            for n in self.adjacent(t) {
                if !dist.contains_key(&n) {
                    dist.insert(n, d + 1);
                    queue.push_back(n);
                    if d + 1 > dist[&farthest] {
                        farthest = n;
                    }
                }
            }
        }
        farthest
    }

    /// BFS: return the shortest path `a → … → b` as a vector.
    fn path_between(&self, a: i64, b: i64) -> Vec<i64> {
        let mut parent: HashMap<i64, i64> = HashMap::new();
        let mut queue: std::collections::VecDeque<i64> = std::collections::VecDeque::new();
        parent.insert(a, a);
        queue.push_back(a);
        'bfs: while let Some(t) = queue.pop_front() {
            for n in self.adjacent(t) {
                if !parent.contains_key(&n) {
                    parent.insert(n, t);
                    queue.push_back(n);
                    if n == b {
                        break 'bfs;
                    }
                }
            }
        }
        let mut path = Vec::new();
        let mut cur = b;
        while cur != a {
            path.push(cur);
            cur = parent[&cur];
        }
        path.push(a);
        path.reverse();
        path
    }

    /// ★ Derive a left-to-right linear order of terminals.
    ///
    /// 1. Double BFS to find the diameter path (the main chain).
    /// 2. Branch nodes (degree > 2 forks) are inserted next to their anchor
    ///    on the chain.
    ///
    /// Star / two-terminal are degenerate cases — byte-identical to the old
    /// centre-based layout. Chain (3+ terminals) is the new supported shape.
    fn linear_order(&self) -> Vec<i64> {
        let start = self.any_node();
        let a = self.bfs_farthest(start);
        let b = self.bfs_farthest(a);
        let mut order = self.path_between(a, b);

        // Insert branch nodes adjacent to their anchor on the chain
        let order_set: HashSet<i64> = order.iter().copied().collect();
        let mut branches: Vec<i64> = self
            .incident
            .keys()
            .filter(|t| !order_set.contains(t))
            .copied()
            .collect();
        branches.sort_unstable(); // deterministic

        for t in branches {
            if let Some(anchor) = self
                .adjacent(t)
                .iter()
                .find(|n| order_set.contains(n))
                .copied()
            {
                let at = order.iter().position(|x| *x == anchor).unwrap();
                // Insert to the right of the anchor
                order.insert(at + 1, t);
            }
        }

        order
    }
}

/// Build a TerminalGraph from raw terminal pairs (before models are built).
/// `island_pairs` are `(island_idx, term_a, term_b)` from `find_terminals`.
/// Direct bands are offset by `island_pairs.len()` to avoid index collision.
fn build_terminal_graph_from_pairs(
    island_pairs: &[(usize, i64, i64)],
    direct_bands: &[DirectBand],
) -> TerminalGraph {
    let mut incident: std::collections::BTreeMap<i64, Vec<usize>> =
        std::collections::BTreeMap::new();
    let mut ends: Vec<(i64, i64)> = Vec::with_capacity(island_pairs.len() + direct_bands.len());

    for &(i, a, b) in island_pairs {
        incident.entry(a).or_default().push(i);
        incident.entry(b).or_default().push(i);
        ends.push((a, b));
    }

    let offset = island_pairs.len();
    for (i, db) in direct_bands.iter().enumerate() {
        let bi = offset + i;
        incident.entry(db.left_box).or_default().push(bi);
        incident.entry(db.right_box).or_default().push(bi);
        ends.push((db.left_box, db.right_box));
    }

    TerminalGraph { incident, ends }
}

/// Given an island's terminal pair (a, b) and the terminal chain order,
/// return the correct (left_box, right_box) ordering.
///
/// Left = earlier in the chain, right = later. This naturally handles
/// 2-terminal, star, and chain topologies without special cases.
fn ordered_terminals_by_chain(a: i64, b: i64, order: &[i64]) -> (i64, i64) {
    let pos_a = order.iter().position(|&t| t == a).unwrap();
    let pos_b = order.iter().position(|&t| t == b).unwrap();
    if pos_a < pos_b {
        (a, b)
    } else {
        (b, a)
    }
}

// ============================================================================
// Public entry
// ============================================================================

/// Decompose a graph into islands. Pure; never touches geometry.
/// Logs the result via `crate::vlog!`.
pub fn decompose(graph: &McVecGraph) -> Decomposition {
    let n_nets = graph.nets.len();

    // ── 0. Box lookup table (O(1) instead of O(boxes) per find) ────────────
    let box_by_id: HashMap<i64, &crate::vector::graph::McVecBox> =
        graph.boxes.iter().map(|b| (b.id, b)).collect();

    // ── 1. Collect passive edges (two-pin passives) ─────────────────────────
    //    Split into two sets:
    //    · all_passive_boxes: every `is_two_pin_passive()` — used for
    //      boundary/terminal detection (so a 1-net or self-loop passive
    //      doesn't masquerade as a non-passive box).
    //    · passive_edges: only those touching exactly 2 nets — used for
    //      connectivity (DSU, island membership).
    let mut passive_edges: Vec<(i64, String, usize, usize)> = Vec::new();
    let mut all_passive_boxes: HashSet<i64> = HashSet::new();
    for b in &graph.boxes {
        if !b.is_two_pin_passive() {
            continue;
        }
        all_passive_boxes.insert(b.id);
        let nets: Vec<usize> = graph
            .nets
            .iter()
            .enumerate()
            .filter(|(_, n)| n.endpoints.iter().any(|e| e.box_id == b.id))
            .map(|(i, _)| i)
            .collect();
        if nets.len() == 2 {
            passive_edges.push((b.id, b.display_label().to_string(), nets[0], nets[1]));
        } else {
            crate::vlog!(
                "[islands] passive #{} '{}' touches {} net(s) (not 2) — excluded from connectivity, kept in all_passive_boxes for boundary detection",
                b.id,
                b.display_label(),
                nets.len()
            );
        }
    }

    // ── 2. Union-find: passive-edge connected components ────────────────────
    let mut dsu = Dsu::new(n_nets);
    for &(_, _, a, b) in &passive_edges {
        dsu.union(a, b);
    }

    // ── 3. Group nets into islands ──────────────────────────────────────────
    let mut root_to_nodes: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut net_has_passive = vec![false; n_nets];
    for (_, _, a, b) in &passive_edges {
        net_has_passive[*a] = true;
        net_has_passive[*b] = true;
    }
    for ni in 0..n_nets {
        if net_has_passive[ni] {
            let root = dsu.find(ni);
            root_to_nodes.entry(root).or_default().push(ni);
        }
    }

    // ── 4. Build islands (deterministic order) ──────────────────────────────
    let mut islands: Vec<Island> = Vec::new();
    let mut roots: Vec<usize> = root_to_nodes.keys().copied().collect();
    roots.sort_unstable(); // ★ deterministic
    for root in &roots {
        let nodes = &root_to_nodes[root];
        let node_set: HashSet<usize> = nodes.iter().copied().collect();
        let edges: Vec<(i64, String, usize, usize)> = passive_edges
            .iter()
            .filter(|(_, _, a, b)| node_set.contains(a) && node_set.contains(b))
            .cloned()
            .collect();
        // Find boundary nets: touched by non-passive, non-rail boxes.
        // ★ Use all_passive_boxes (not passive_boxes) so a 1-net passive
        //   doesn't count as a non-passive box and create a fake boundary.
        let boundaries: Vec<usize> = nodes
            .iter()
            .copied()
            .filter(|&ni| {
                graph.nets[ni].endpoints.iter().any(|e| {
                    let is_passive = all_passive_boxes.contains(&e.box_id);
                    let is_rail = box_by_id
                        .get(&e.box_id)
                        .map(|b| is_rail_box(b))
                        .unwrap_or(false);
                    !is_passive && !is_rail
                })
            })
            .collect();
        islands.push(Island {
            nodes: nodes.clone(),
            edges,
            boundaries,
        });
    }

    // ── 5. Find direct bands (nets with no passive boxes, two terminals) ─────
    //    ★ Deterministic: sort by left_box then by net index.
    let mut direct_bands: Vec<DirectBand> = Vec::new();
    for ni in 0..n_nets {
        if net_has_passive[ni] {
            continue;
        }
        let mut non_passive_boxes: Vec<i64> = graph.nets[ni]
            .endpoints
            .iter()
            .filter(|e| {
                !all_passive_boxes.contains(&e.box_id)
                    && !box_by_id
                        .get(&e.box_id)
                        .map(|b| is_rail_box(b))
                        .unwrap_or(false)
            })
            .map(|e| e.box_id)
            .collect();
        non_passive_boxes.sort_unstable();
        non_passive_boxes.dedup();
        if non_passive_boxes.len() == 2 {
            direct_bands.push(DirectBand {
                net: ni,
                left_box: non_passive_boxes[0],
                right_box: non_passive_boxes[1],
            });
        }
    }
    // ★ Deterministic: sort by left_box then net
    direct_bands.sort_by_key(|db| (db.left_box, db.net));

    let result = Decomposition {
        islands,
        direct_bands,
    };

    // ── LOG ──────────────────────────────────────────────────────────────────
    log_decomposition(graph, &result, &box_by_id);

    result
}

// ============================================================================
// ★ apply_islands — 将分解结果落成几何（Phase 2: band 装配）
// ============================================================================

/// Try to apply island-based layout. Returns `true` if **at least one** island was
/// claimed and placed (per-island claiming). Successful islands are locked with
/// `geom_locked = true`; failed islands are left for the fallback (L2/L3/L4).
///
/// ## Algorithm
/// 1. Phase A: For each island, build a model (SP or Ladder). Track which succeed.
///    - Stubs are logged only (pendant branches, decoupling caps).
///    - MultiPort islands → not claimed.
/// 2. Phase 0.5: Rebuild terminal graph from **successful** pairs + direct bands,
///    derive the chain order.
/// 3. Phase B: Collect bands from successful models + direct bands.
/// 4. Phase C+D: Chain-based layout — stack bands in gaps, place terminals once.
///
/// Every early return and every placement failure logs exactly what went wrong.
pub fn apply_islands(graph: &mut McVecGraph, d: &Decomposition) -> bool {
    if d.islands.is_empty() && d.direct_bands.is_empty() {
        crate::vlog!("[islands] no islands and no direct bands — nothing to claim");
        return false;
    }

    // ── Phase 0.5: initial terminal graph from ALL pairs (for chain-based
    //    ordering during model building). Rebuilt later from successful pairs only.
    let initial_order = {
        let box_by_id: HashMap<i64, &crate::vector::graph::McVecBox> =
            graph.boxes.iter().map(|b| (b.id, b)).collect();

        let mut pairs: Vec<(usize, i64, i64)> = Vec::new();
        for (i, isl) in d.islands.iter().enumerate() {
            let kind = classify(graph, isl, &box_by_id);
            match kind {
                IslandKind::Sp | IslandKind::Ladder { .. } => {
                    if let Some(pair) = find_terminals(graph, isl, &box_by_id) {
                        pairs.push((i, pair.0, pair.1));
                    }
                }
                _ => {}
            }
        }

        let tg = build_terminal_graph_from_pairs(&pairs, &d.direct_bands);

        // Log terminal graph
        for (&tid, indices) in &tg.incident {
            let name = box_label(&box_by_id, tid);
            crate::vlog!("[islands] terminal graph: {}(deg={})", name, indices.len());
        }

        let order = tg.linear_order();
        let names: Vec<String> = order.iter().map(|&t| box_label(&box_by_id, t)).collect();
        crate::vlog!("[islands] terminal chain: {}", names.join(" — "));

        order
    }; // box_by_id dropped

    // ★ Per-island claiming: track which island indices were successfully claimed.
    let mut claimed: HashSet<usize> = HashSet::new();
    let mut sp_models: Vec<(SpModel, usize)> = Vec::new();
    let mut ladder_models: Vec<(LadderModel, usize)> = Vec::new();

    // ── Phase A: build models with chain-determined ordering ────────────────
    {
        let box_by_id: HashMap<i64, &crate::vector::graph::McVecBox> =
            graph.boxes.iter().map(|b| (b.id, b)).collect();

        for (i, isl) in d.islands.iter().enumerate() {
            let kind = classify(graph, isl, &box_by_id);
            let isl_boxes = boundary_boxes(graph, isl, &box_by_id);
            match kind {
                IslandKind::Stub => {
                    crate::vlog!(
                        "[islands] island#{i} is a stub ({} edges, boundary_nets={:?}) — log only, not placed",
                        isl.edges.len(),
                        isl.boundaries
                    );
                }
                IslandKind::MultiPort => {
                    let b_labels: Vec<String> = isl
                        .boundaries
                        .iter()
                        .map(|&ni| graph.nets[ni].name.clone())
                        .collect();
                    crate::vlog!(
                        "[islands] island#{i} is MultiPort ({} boundary_boxes: {:?}, {} boundary_nets: {:?}) — not claimed, falling back",
                        isl_boxes.len(),
                        isl_boxes,
                        b_labels.len(),
                        b_labels
                    );
                }
                IslandKind::Sp => {
                    let (a, b) = match find_terminals(graph, isl, &box_by_id) {
                        Some(pair) => pair,
                        None => {
                            crate::vlog!(
                                "[islands] island#{i} (Sp): cannot find 2 terminal boxes — not claimed"
                            );
                            continue;
                        }
                    };

                    // ★ Ordering by chain, not by id sort
                    let (left_box, right_box) = ordered_terminals_by_chain(a, b, &initial_order);

                    // ★ Debug assertion: terminals must belong to this island
                    debug_assert!(
                        isl_boxes.contains(&left_box) && isl_boxes.contains(&right_box),
                        "island#{i}: 给模型的端子 ({left_box}, {right_box}) 不是这个岛的边界盒 {isl_boxes:?}"
                    );

                    let passive_boxes: Vec<i64> =
                        isl.edges.iter().map(|(id, _, _, _)| *id).collect();
                    let sub = SubNet {
                        nodes: isl.nodes.clone(),
                        passive_boxes,
                        left_box,
                        right_box,
                        orientation_fixed: true,
                    };

                    let left_name = box_label(&box_by_id, left_box);
                    let right_name = box_label(&box_by_id, right_box);
                    crate::vlog!(
                        "[islands] island#{i}: Sp terminals={}~{} — trying SP",
                        left_name,
                        right_name
                    );

                    match build_sp_tree(graph, &sub) {
                        Ok(model) => {
                            crate::vlog!(
                                "[islands] island#{i}: SP model built — {}",
                                model.root.expr()
                            );
                            sp_models.push((model, i));
                            claimed.insert(i);
                        }
                        Err(e) => {
                            crate::vlog!(
                                "[islands] island#{i}: SP bail — {e} — not claimed, falling back"
                            );
                        }
                    }
                }
                IslandKind::Ladder { lanes } => {
                    let (a, b) = match find_terminals(graph, isl, &box_by_id) {
                        Some(pair) => pair,
                        None => {
                            crate::vlog!(
                                "[islands] island#{i} (Ladder): cannot find 2 terminal boxes — not claimed"
                            );
                            continue;
                        }
                    };

                    // ★ Ordering by chain, not by id sort
                    let (left_box, right_box) = ordered_terminals_by_chain(a, b, &initial_order);

                    // ★ Debug assertion: terminals must belong to this island
                    debug_assert!(
                        isl_boxes.contains(&left_box) && isl_boxes.contains(&right_box),
                        "island#{i}: 给模型的端子 ({left_box}, {right_box}) 不是这个岛的边界盒 {isl_boxes:?}"
                    );

                    let left_name = box_label(&box_by_id, left_box);
                    let right_name = box_label(&box_by_id, right_box);
                    crate::vlog!(
                        "[islands] island#{i}: Ladder{{lanes={lanes}}} terminals={}~{} — trying ladder",
                        left_name,
                        right_name
                    );

                    match super::ladder_model::build_ladder_model_on(
                        graph, left_box, right_box, isl,
                    ) {
                        Ok(model) => {
                            crate::vlog!(
                                "[islands] island#{i}: ladder model built — {} lanes, {} cols",
                                model.n_lanes,
                                model.n_cols
                            );
                            ladder_models.push((model, i));
                            claimed.insert(i);
                        }
                        Err(e) => {
                            crate::vlog!(
                                "[islands] island#{i}: ladder bail — {e} — not claimed, falling back"
                            );
                        }
                    }
                }
            }
        }
    } // ★ box_by_id dropped here — mutable borrow is now safe

    // ── Log coverage before any geometry changes ────────────────────────────
    let total = d.islands.len();
    let n_claimed = claimed.len();
    let fallback = total - n_claimed;
    let coverage_pct = if total > 0 {
        n_claimed as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    crate::vlog!(
        "[islands] LAYOUT-MODEL: islands={total} claimed={n_claimed} fallback={fallback} coverage={coverage_pct:.0}%"
    );
    // ★ Store coverage for the fidelity gate (read by compute_fidelity in select.rs)
    graph.islands_claimed = n_claimed;
    graph.islands_total = total;

    if claimed.is_empty() && d.direct_bands.is_empty() {
        crate::vlog!("[islands] no islands claimed — falling back to whole-graph models");
        return false;
    }

    // ── Phase 0.5b: rebuild terminal graph from ★successful★ pairs only ────
    //    This ensures the chain only includes terminals that actually have bands,
    //    so failed islands' terminals are left unlocked for the fallback.
    let (order, box_owned) = {
        let box_by_id: HashMap<i64, &crate::vector::graph::McVecBox> =
            graph.boxes.iter().map(|b| (b.id, b)).collect();

        let mut pairs: Vec<(usize, i64, i64)> = Vec::new();
        for (i, isl) in d.islands.iter().enumerate() {
            if !claimed.contains(&i) {
                continue;
            }
            let kind = classify(graph, isl, &box_by_id);
            match kind {
                IslandKind::Sp | IslandKind::Ladder { .. } => {
                    if let Some(pair) = find_terminals(graph, isl, &box_by_id) {
                        pairs.push((i, pair.0, pair.1));
                    }
                }
                _ => {}
            }
        }

        let tg = build_terminal_graph_from_pairs(&pairs, &d.direct_bands);

        let order = tg.linear_order();
        let names: Vec<String> = order.iter().map(|&t| box_label(&box_by_id, t)).collect();
        crate::vlog!(
            "[islands] claimed terminal chain ({} terminals): {}",
            names.len(),
            names.join(" — ")
        );

        let box_owned: HashMap<i64, String> = graph
            .boxes
            .iter()
            .map(|b| (b.id, b.display_label().to_string()))
            .collect();

        (order, box_owned)
    }; // box_by_id dropped

    // ── Phase B: collect bands from successful models + direct bands ────────
    let mut bands: Vec<Band> = Vec::new();

    for (model, i) in sp_models {
        if !claimed.contains(&i) {
            continue;
        }
        crate::vlog!("[islands] band SP island#{i}: size={:?}", model.size());
        bands.push(Band::Sp {
            model,
            island_idx: i,
        });
    }
    for (model, i) in ladder_models {
        if !claimed.contains(&i) {
            continue;
        }
        crate::vlog!("[islands] band Ladder island#{i}: size={:?}", model.size());
        bands.push(Band::Ladder {
            model,
            island_idx: i,
        });
    }
    for db in &d.direct_bands {
        let left_name = box_owned
            .get(&db.left_box)
            .cloned()
            .unwrap_or_else(|| "?".into());
        let right_name = box_owned
            .get(&db.right_box)
            .cloned()
            .unwrap_or_else(|| "?".into());
        let net = &graph.nets[db.net];
        let left_pin = net
            .endpoints
            .iter()
            .find(|e| e.box_id == db.left_box)
            .map(|e| e.pin_id)
            .unwrap_or(-1);
        let right_pin = net
            .endpoints
            .iter()
            .find(|e| e.box_id == db.right_box)
            .map(|e| e.pin_id)
            .unwrap_or(-1);
        crate::vlog!(
            "[islands] band Direct net[{}] '{}' : {}~{} — pins ({}, {})",
            db.net,
            net.name,
            left_name,
            right_name,
            left_pin,
            right_pin
        );
        bands.push(Band::Direct {
            db: db.clone(),
            left_pin,
            right_pin,
        });
    }

    if bands.is_empty() {
        crate::vlog!("[islands] no bands to place");
        return false;
    }

    // ── Phase C+D: chain-based layout ────────────────────────────────────────
    apply_chain_layout(graph, &bands, &order, &box_owned)
}

/// Collect the boundary boxes for an island — the non-passive, non-rail boxes
/// that touch the island's boundary nets. Returns a sorted vector for determinism.
fn boundary_boxes(
    graph: &McVecGraph,
    isl: &Island,
    box_by_id: &HashMap<i64, &crate::vector::graph::McVecBox>,
) -> Vec<i64> {
    let mut boxes: Vec<i64> = Vec::new();
    for &ni in &isl.boundaries {
        for ep in &graph.nets[ni].endpoints {
            if let Some(b) = box_by_id.get(&ep.box_id) {
                if b.id >= 0 && !is_rail_box(b) && !b.is_two_pin_passive() {
                    boxes.push(b.id);
                }
            }
        }
    }
    boxes.sort_unstable();
    boxes.dedup();
    boxes
}

/// 2D classification: (boundary_boxes, boundary_nets).
///
/// The original `classify(isl)` counted boundary **nets** — which is wrong for
/// multi-lane ladders (4 nets → 2 terminals → should be Ladder, not MultiPort).
fn classify(
    graph: &McVecGraph,
    isl: &Island,
    box_by_id: &HashMap<i64, &crate::vector::graph::McVecBox>,
) -> IslandKind {
    let boxes = boundary_boxes(graph, isl, box_by_id);
    match (boxes.len(), isl.boundaries.len()) {
        (2, 2) => IslandKind::Sp,
        (2, n) if n > 2 => IslandKind::Ladder { lanes: n / 2 },
        (1, _) => IslandKind::Stub,
        _ => IslandKind::MultiPort,
    }
}

fn box_label(box_by_id: &HashMap<i64, &crate::vector::graph::McVecBox>, id: i64) -> String {
    box_by_id
        .get(&id)
        .map(|b| b.display_label().to_string())
        .unwrap_or_else(|| "?".into())
}

/// Find the two terminal boxes for a TwoTerminal island.
///
/// Each boundary net is touched by non-passive, non-rail boxes. Collecting them
/// across all boundary nets should yield exactly 2 unique terminal boxes.
fn find_terminals(
    graph: &McVecGraph,
    isl: &Island,
    box_by_id: &HashMap<i64, &crate::vector::graph::McVecBox>,
) -> Option<(i64, i64)> {
    let mut terminals: Vec<i64> = Vec::new();

    for &ni in &isl.boundaries {
        for ep in &graph.nets[ni].endpoints {
            if let Some(b) = box_by_id.get(&ep.box_id) {
                if b.id >= 0 && !is_rail_box(b) && !b.is_two_pin_passive() {
                    terminals.push(b.id);
                }
            }
        }
    }

    terminals.sort_unstable();
    terminals.dedup();

    if terminals.len() == 2 {
        Some((terminals[0], terminals[1]))
    } else {
        crate::vlog!(
            "[islands] find_terminals: expected 2 terminal boxes, got {}: {:?}",
            terminals.len(),
            terminals
        );
        None
    }
}

// ============================================================================
// ★ Chain-based layout — unified Phase C+D for any number of terminals
// ============================================================================

/// Compute the preferred width (w) for a terminal box.
fn terminal_width(graph: &McVecGraph, id: i64) -> f64 {
    const MIN_W: f64 = COL_W * 1.1;
    let cur_w = graph
        .boxes
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.w)
        .unwrap_or(0.0);
    MIN_W.max(cur_w)
}

/// Compute the preferred height (h) for a terminal box given its stack height.
fn terminal_height(graph: &McVecGraph, id: i64, stack_h: f64) -> f64 {
    const PIN_PITCH: f64 = 28.0;
    const PAD: f64 = 26.0;
    let pins = graph
        .boxes
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.pins.len().max(x.entry_points.len()).max(x.pin_count))
        .unwrap_or(0);
    let cur_h = graph
        .boxes
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.h)
        .unwrap_or(0.0);
    ((pins as f64) * PIN_PITCH + PAD)
        .max(stack_h + PAD)
        .max(cur_h)
}

/// Place a terminal box with connected pins on the given side, unconnected
/// pins on the far edge. Called once per terminal in Phase D.
fn place_terminal_box(
    graph: &mut McVecGraph,
    box_id: i64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    facing: EntrySide,
    connected: &[(i64, f64)],
) {
    let Some(b) = graph.boxes.iter_mut().find(|b| b.id == box_id) else {
        return;
    };
    b.x = x;
    b.y = y;
    b.w = w;
    b.h = h;

    for &(pin_id, _) in connected {
        debug_assert!(
            b.pins.is_empty() || b.pins.iter().any(|p| p.id == pin_id),
            "pin {pin_id} 不属于 box#{} —— 端子配对错了",
            b.id
        );
        if !b.entry_points.iter().any(|e| e.pin_id == pin_id) {
            b.entry_points.push(EntryPoint {
                pin_id,
                pin_name: pin_id.to_string(),
                side: facing.clone(),
                offset: 0.5,
            });
        }
    }

    let pinned: Vec<(i64, f64)> = connected
        .iter()
        .map(|&(pin_id, ay)| (pin_id, ((ay - y) / h).clamp(0.02, 0.98)))
        .collect();

    distribute_terminal_pins(b, facing, &pinned);
    b.geom_locked = true;
}

/// ★ Unified chain-based layout.
///
/// Terminals are placed left-to-right along the x-axis in `order`.
/// Bands are grouped into gaps between adjacent terminals, stacked vertically
/// within each gap, and placed between their two terminals.
///
/// This naturally handles:
/// - 2 terminals → 1 gap (degenerate, same as old two-terminal layout)
/// - 3 terminals star → 2 gaps (degenerate, same as old centre-based layout)
/// - N terminals chain → N-1 gaps (the new supported shape)
fn apply_chain_layout(
    graph: &mut McVecGraph,
    bands: &[Band],
    order: &[i64],
    box_owned: &HashMap<i64, String>,
) -> bool {
    let n = order.len();
    if n < 2 {
        crate::vlog!("[islands] chain layout needs >= 2 terminals, got {n}");
        return false;
    }

    // ── Build gap → band mapping ───────────────────────────────────────────
    let pos: HashMap<i64, usize> = order.iter().enumerate().map(|(i, &t)| (t, i)).collect();
    let mut gaps: Vec<Vec<usize>> = vec![Vec::new(); n - 1];
    for (bi, band) in bands.iter().enumerate() {
        let (t0, t1) = band.terminal_boxes();
        let Some(&i0) = pos.get(&t0) else {
            crate::vlog!(
                "[islands] band {bi}: terminal {t0} not in chain order {:?} — skipping",
                order
            );
            continue;
        };
        let Some(&i1) = pos.get(&t1) else {
            crate::vlog!(
                "[islands] band {bi}: terminal {t1} not in chain order {:?} — skipping",
                order
            );
            continue;
        };
        let g = i0.min(i1);
        if (i1 as i64 - i0 as i64).abs() != 1 {
            crate::vlog!("[islands] band {bi}: 端子 ({t0}, {t1}) 在链上不相邻（gap={g}）— 跳过",);
            continue;
        }
        gaps[g].push(bi);
    }

    // ── Log gaps ───────────────────────────────────────────────────────────
    for (g, gap_bands) in gaps.iter().enumerate() {
        let l_name = box_owned
            .get(&order[g])
            .cloned()
            .unwrap_or_else(|| "?".into());
        let r_name = box_owned
            .get(&order[g + 1])
            .cloned()
            .unwrap_or_else(|| "?".into());
        let band_list: Vec<usize> = gap_bands.iter().copied().collect();
        let mut max_w = 0.0f64;
        let mut total_h = 0.0f64;
        for &bi in gap_bands {
            let (bw, bh) = bands[bi].extent_px(graph);
            max_w = max_w.max(bw);
            total_h += bh;
        }
        if !gap_bands.is_empty() {
            total_h += (gap_bands.len() - 1) as f64 * BAND_GAP;
        }
        crate::vlog!(
            "[islands]   gap{g} {l_name}~{r_name}: bands {band_list:?} w={max_w:.0} h={total_h:.0}"
        );
    }

    // ── Compute terminal widths ────────────────────────────────────────────
    let tw: Vec<f64> = order.iter().map(|&t| terminal_width(graph, t)).collect();

    // ── Compute gap widths (max band width in each gap) ────────────────────
    let gap_w: Vec<f64> = gaps
        .iter()
        .map(|gb| {
            if gb.is_empty() {
                0.0
            } else {
                gb.iter()
                    .map(|&bi| bands[bi].extent_px(graph).0)
                    .fold(0.0f64, f64::max)
            }
        })
        .collect();

    // ── Compute x positions ────────────────────────────────────────────────
    //    terminal[i] at x, then gap[i], then terminal[i+1], ...
    let mut term_x: Vec<f64> = vec![0.0; n];
    let mut gap_x0: Vec<f64> = vec![0.0; n - 1];
    let mut x = MARGIN;
    for i in 0..n {
        term_x[i] = x;
        x += tw[i];
        if i < n - 1 {
            x += TERM_GAP;
            gap_x0[i] = x;
            x += gap_w[i] + TERM_GAP;
        }
    }

    // ── Place passives: stack bands in each gap ────────────────────────────
    //    gap_pins: for each gap, BTreeMap<terminal_id, Vec<(pin_id, y_abs)>>
    let mut gap_pins: Vec<std::collections::BTreeMap<i64, Vec<(i64, f64)>>> =
        vec![std::collections::BTreeMap::new(); n - 1];
    let mut gap_h: Vec<f64> = vec![0.0; n - 1];

    for (g, gap_bands) in gaps.iter().enumerate() {
        if gap_bands.is_empty() {
            continue;
        }
        let mut y = MARGIN;
        for &bi in gap_bands {
            let band = &bands[bi];
            let (bw, bh) = band.extent_px(graph);

            // Place passives
            let origin = Point::new(gap_x0[g], y);
            band.place_passives(graph, origin, gap_x0[g] + bw);

            // Collect pin positions
            let row_h = band.row_h(graph);
            let (left_term, right_term) = band.terminal_boxes();
            let (lp, rp) = band.terminal_pins();

            for (k, p) in lp.iter().enumerate() {
                gap_pins[g]
                    .entry(left_term)
                    .or_default()
                    .push((*p, y + (k as f64 + 0.5) * row_h));
            }
            for (k, p) in rp.iter().enumerate() {
                gap_pins[g]
                    .entry(right_term)
                    .or_default()
                    .push((*p, y + (k as f64 + 0.5) * row_h));
            }

            crate::vlog!("[islands]   gap{g} band y0={:.0} h={:.0}", y, bh);
            y += bh + BAND_GAP;
        }
        gap_h[g] = y - BAND_GAP - MARGIN;
    }

    // ── Place terminals ────────────────────────────────────────────────────
    for (i, &tid) in order.iter().enumerate() {
        // Left gap pins (gap i-1, right terminal of that gap)
        let left_pins: Vec<(i64, f64)> = if i > 0 {
            gap_pins[i - 1].get(&tid).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        // Right gap pins (gap i, left terminal of that gap)
        let right_pins: Vec<(i64, f64)> = if i < n - 1 {
            gap_pins[i].get(&tid).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };

        let left_h = if i > 0 { gap_h[i - 1] } else { 0.0 };
        let right_h = if i < n - 1 { gap_h[i] } else { 0.0 };
        let th = terminal_height(graph, tid, left_h.max(right_h));

        let t_name = box_owned.get(&tid).cloned().unwrap_or_else(|| "?".into());

        if left_pins.is_empty() && right_pins.is_empty() {
            // Terminal with no band connections — just place it
            let Some(b) = graph.boxes.iter_mut().find(|b| b.id == tid) else {
                continue;
            };
            b.x = term_x[i];
            b.y = MARGIN;
            b.w = tw[i];
            b.h = th;
            b.geom_locked = true;
            crate::vlog!(
                "[islands]   placed terminal {} at x={:.0} (no bands)",
                t_name,
                term_x[i]
            );
            continue;
        }

        if left_pins.is_empty() {
            // Leftmost terminal: only right gap pins, face Right
            place_terminal_box(
                graph,
                tid,
                term_x[i],
                MARGIN,
                tw[i],
                th,
                EntrySide::Right,
                &right_pins,
            );
            crate::vlog!(
                "[islands]   placed leftmost terminal {} at x={:.0} h={:.0} ({} right pins)",
                t_name,
                term_x[i],
                th,
                right_pins.len()
            );
        } else if right_pins.is_empty() {
            // Rightmost terminal: only left gap pins, face Left
            place_terminal_box(
                graph,
                tid,
                term_x[i],
                MARGIN,
                tw[i],
                th,
                EntrySide::Left,
                &left_pins,
            );
            crate::vlog!(
                "[islands]   placed rightmost terminal {} at x={:.0} h={:.0} ({} left pins)",
                t_name,
                term_x[i],
                th,
                left_pins.len()
            );
        } else {
            // Middle terminal: pins on both sides
            let Some(b) = graph.boxes.iter_mut().find(|b| b.id == tid) else {
                continue;
            };
            b.x = term_x[i];
            b.y = MARGIN;
            b.w = tw[i];
            b.h = th;

            for &(pin_id, _) in left_pins.iter().chain(right_pins.iter()) {
                debug_assert!(
                    b.pins.is_empty() || b.pins.iter().any(|p| p.id == pin_id),
                    "pin {pin_id} 不属于 box#{} —— 端子配对错了",
                    b.id
                );
                if !b.entry_points.iter().any(|e| e.pin_id == pin_id) {
                    b.entry_points.push(EntryPoint {
                        pin_id,
                        pin_name: pin_id.to_string(),
                        side: EntrySide::Right,
                        offset: 0.5,
                    });
                }
            }

            // Left gap pins → EntrySide::Left
            for &(pin_id, ay) in &left_pins {
                if let Some(ep) = b.entry_points.iter_mut().find(|e| e.pin_id == pin_id) {
                    ep.side = EntrySide::Left;
                    ep.offset = ((ay - MARGIN) / th).clamp(0.02, 0.98);
                }
            }
            // Right gap pins → EntrySide::Right
            for &(pin_id, ay) in &right_pins {
                if let Some(ep) = b.entry_points.iter_mut().find(|e| e.pin_id == pin_id) {
                    ep.side = EntrySide::Right;
                    ep.offset = ((ay - MARGIN) / th).clamp(0.02, 0.98);
                }
            }

            // Unconnected pins → side with fewer connected pins
            let far_side = if left_pins.len() <= right_pins.len() {
                EntrySide::Left
            } else {
                EntrySide::Right
            };
            let connected_set: HashSet<i64> = left_pins
                .iter()
                .chain(right_pins.iter())
                .map(|(p, _)| *p)
                .collect();
            let mut far_idx: Vec<usize> = Vec::new();
            for (j, ep) in b.entry_points.iter_mut().enumerate() {
                if !connected_set.contains(&ep.pin_id) {
                    ep.side = far_side.clone();
                    far_idx.push(j);
                }
            }
            let nf = far_idx.len();
            for (k, &j) in far_idx.iter().enumerate() {
                b.entry_points[j].offset = (k as f64 + 1.0) / (nf as f64 + 1.0);
            }

            b.geom_locked = true;
            crate::vlog!(
                "[islands]   placed middle terminal {} at x={:.0} h={:.0} ({} left + {} right pins)",
                t_name,
                term_x[i],
                th,
                left_pins.len(),
                right_pins.len()
            );
        }
    }

    crate::vlog!(
        "[islands] ✓ chain layout: {} terminals, {} gaps, {} band(s)",
        n,
        n - 1,
        bands.len()
    );
    true
}

/// Log the decomposition for human review (Phase 1 — no geometry yet).
fn log_decomposition(
    graph: &McVecGraph,
    d: &Decomposition,
    box_by_id: &HashMap<i64, &crate::vector::graph::McVecBox>,
) {
    crate::vlog!(
        "[islands] {} island(s), {} direct band(s)",
        d.islands.len(),
        d.direct_bands.len()
    );

    for (i, isl) in d.islands.iter().enumerate() {
        let kind = classify(graph, isl, box_by_id);
        let labels: Vec<String> = isl
            .edges
            .iter()
            .map(|(_, label, _, _)| label.clone())
            .collect();
        let b_boxes = boundary_boxes(graph, isl, box_by_id);
        let b_labels: Vec<String> = b_boxes.iter().map(|&id| box_label(box_by_id, id)).collect();
        crate::vlog!(
            "[islands]   #{i} {:?} | edges={} | boundary_boxes={:?} nets={}",
            kind,
            labels.join(" "),
            b_labels,
            isl.boundaries.len()
        );
    }

    for (i, db) in d.direct_bands.iter().enumerate() {
        let left_name = box_label(box_by_id, db.left_box);
        let right_name = box_label(box_by_id, db.right_box);
        crate::vlog!(
            "[islands]   direct#{i} net[{}] '{}' : {} ~ {}",
            db.net,
            graph.nets[db.net].name,
            left_name,
            right_name
        );
    }
}

// ============================================================================
// DSU
// ============================================================================

struct Dsu {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden 6-element SP graph: one island with 2 boundaries.
    #[test]
    fn golden_sp_is_one_island() {
        let g = golden_sp_graph();
        let d = decompose(&g);
        // 6 passives, 5 nets → one island
        assert_eq!(d.islands.len(), 1, "golden SP should be one island");
        assert_eq!(d.islands[0].edges.len(), 6);
        assert_eq!(d.islands[0].boundaries.len(), 2);
        let box_by_id: HashMap<i64, &crate::vector::graph::McVecBox> =
            g.boxes.iter().map(|b| (b.id, b)).collect();
        assert_eq!(classify(&g, &d.islands[0], &box_by_id), IslandKind::Sp);
        // net[3] (u1.3~u2.3) is a direct band
        assert_eq!(d.direct_bands.len(), 1);
    }

    /// A graph with no passive components: 0 islands, all direct bands.
    #[test]
    fn no_passives_is_no_islands() {
        use crate::vector::graph::boxdef::IoSummary;
        use crate::vector::graph::netdef::{EndpointRef, VizNet};
        use crate::vector::graph::BoxKind;
        use crate::vector::graph::McVecBox;
        use crate::vector::graph::NetKind;
        use crate::vector::graph::Symbol;

        let mut g = McVecGraph::new(1, "main".into());
        for (id, name, outs) in [(101, "u1", 1), (102, "u2", 0)] {
            let mut io = IoSummary::new();
            io.outputs = outs;
            g.boxes.push(McVecBox::new_v2(
                id,
                name.into(),
                "".into(),
                BoxKind::TwoPin,
                Symbol::Ic,
                None,
                None,
                1,
                io,
            ));
        }
        g.nets.push(VizNet::new(
            0,
            "N1".into(),
            NetKind::Signal,
            vec![EndpointRef::new(101, 3, "3"), EndpointRef::new(102, 3, "3")],
        ));
        let d = decompose(&g);
        assert!(d.islands.is_empty());
        assert_eq!(d.direct_bands.len(), 1);
    }

    fn golden_sp_graph() -> McVecGraph {
        use crate::vector::graph::boxdef::IoSummary;
        use crate::vector::graph::netdef::{EndpointRef, VizNet};
        use crate::vector::graph::BoxKind;
        use crate::vector::graph::McVecBox;
        use crate::vector::graph::NetKind;
        use crate::vector::graph::Symbol;

        let mut g = McVecGraph::new(1, "main".into());
        for (id, name, sym) in [
            (1, "R1", Symbol::Resistor),
            (2, "C2", Symbol::Capacitor),
            (3, "R3", Symbol::Resistor),
            (4, "R4", Symbol::Resistor),
            (5, "C5", Symbol::Capacitor),
            (6, "R6", Symbol::Resistor),
        ] {
            g.boxes.push(McVecBox::new_v2(
                id,
                name.into(),
                "".into(),
                BoxKind::TwoPin,
                sym,
                Some(name.into()),
                None,
                2,
                IoSummary::new(),
            ));
        }
        for (id, name, outs) in [(101, "u1", 1), (102, "u2", 0)] {
            let mut io = IoSummary::new();
            io.outputs = outs;
            g.boxes.push(McVecBox::new_v2(
                id,
                name.into(),
                "".into(),
                BoxKind::TwoPin,
                Symbol::Ic,
                None,
                None,
                1,
                io,
            ));
        }
        g.nets.push(VizNet::new(
            0,
            "N1".into(),
            NetKind::Signal,
            vec![
                EndpointRef::new(101, 6, "6"),
                EndpointRef::new(1, 11, "11"),
                EndpointRef::new(3, 31, "31"),
            ],
        ));
        g.nets.push(VizNet::new(
            1,
            "N2".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 12, "12"), EndpointRef::new(2, 21, "21")],
        ));
        g.nets.push(VizNet::new(
            2,
            "N3".into(),
            NetKind::Signal,
            vec![
                EndpointRef::new(2, 22, "22"),
                EndpointRef::new(102, 6, "6"),
                EndpointRef::new(5, 52, "52"),
                EndpointRef::new(6, 62, "62"),
            ],
        ));
        g.nets.push(VizNet::new(
            3,
            "N4".into(),
            NetKind::Signal,
            vec![
                EndpointRef::new(3, 32, "32"),
                EndpointRef::new(4, 41, "41"),
                EndpointRef::new(6, 61, "61"),
            ],
        ));
        g.nets.push(VizNet::new(
            4,
            "N5".into(),
            NetKind::Signal,
            vec![EndpointRef::new(4, 42, "42"), EndpointRef::new(5, 51, "51")],
        ));
        // direct net: u1.3 ~ u2.3
        g.nets.push(VizNet::new(
            5,
            "N6".into(),
            NetKind::Signal,
            vec![EndpointRef::new(101, 3, "3"), EndpointRef::new(102, 3, "3")],
        ));
        g
    }
}
