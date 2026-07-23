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

use crate::vector::graph::McVecGraph;

use super::rails::is_rail_box;
use super::sp_model::{build_sp_tree, SpModel, SubNet};
use super::sp_place::apply_sp_model;

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

/// Try to apply island-based layout. Returns `true` if **all** islands were claimed
/// and placed. Returns `false` if any island could not be claimed — the caller should
/// fall back to the old whole-graph SP/ladder dispatch.
///
/// ## Algorithm
/// 1. For each Sp island: find terminal boxes → build SubNet → try `build_sp_tree`
/// 2. For each Ladder island: find terminal boxes → build SubNet → try `build_ladder_model_on`
/// 3. Direct bands are logged but not placed (routing is handled by trunk_tap)
/// 4. Stubs are logged only (pendant branches, decoupling caps)
/// 5. MultiPort islands → not claimed → causes fallback
///
/// Every early return and every placement failure logs exactly what went wrong.
pub fn apply_islands(graph: &mut McVecGraph, d: &Decomposition) -> bool {
    if d.islands.is_empty() && d.direct_bands.is_empty() {
        crate::vlog!("[islands] no islands and no direct bands — nothing to claim");
        return false;
    }

    let mut all_claimed = true;
    let mut sp_models: Vec<(SpModel, usize)> = Vec::new();
    let mut ladder_models: Vec<(super::ladder_model::LadderModel, usize)> = Vec::new();

    // ── Phase A: try to build a model for every Sp / Ladder island ─────────
    //    Build box_by_id inside a block so it's dropped before Phase B's
    //    mutable borrow of graph.
    let box_owned: HashMap<i64, String> = {
        let box_by_id: HashMap<i64, &crate::vector::graph::McVecBox> =
            graph.boxes.iter().map(|b| (b.id, b)).collect();

        for (i, isl) in d.islands.iter().enumerate() {
            let kind = classify(graph, isl, &box_by_id);
            match kind {
                IslandKind::Stub => {
                    crate::vlog!(
                        "[islands] island#{i} is a stub ({} edges, boundary_nets={:?}) — log only, not placed",
                        isl.edges.len(),
                        isl.boundaries
                    );
                }
                IslandKind::MultiPort => {
                    let b_boxes = boundary_boxes(graph, isl, &box_by_id);
                    let b_labels: Vec<String> = isl
                        .boundaries
                        .iter()
                        .map(|&ni| graph.nets[ni].name.clone())
                        .collect();
                    crate::vlog!(
                        "[islands] island#{i} is MultiPort ({} boundary_boxes: {:?}, {} boundary_nets: {:?}) — not claimed, falling back",
                        b_boxes.len(),
                        b_boxes,
                        b_labels.len(),
                        b_labels
                    );
                    all_claimed = false;
                }
                IslandKind::Sp => {
                    let (left_box, right_box) = match find_terminals(graph, isl, &box_by_id) {
                        Some(pair) => pair,
                        None => {
                            crate::vlog!(
                                    "[islands] island#{i} (Sp): cannot find 2 terminal boxes — not claimed"
                                );
                            all_claimed = false;
                            continue;
                        }
                    };

                    let passive_boxes: Vec<i64> =
                        isl.edges.iter().map(|(id, _, _, _)| *id).collect();
                    let sub = SubNet {
                        nodes: isl.nodes.clone(),
                        passive_boxes,
                        left_box,
                        right_box,
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
                        }
                        Err(e) => {
                            crate::vlog!(
                                "[islands] island#{i}: SP bail — {e} — not claimed, falling back"
                            );
                            all_claimed = false;
                        }
                    }
                }
                IslandKind::Ladder { lanes } => {
                    let (left_box, right_box) = match find_terminals(graph, isl, &box_by_id) {
                        Some(pair) => pair,
                        None => {
                            crate::vlog!(
                                    "[islands] island#{i} (Ladder): cannot find 2 terminal boxes — not claimed"
                                );
                            all_claimed = false;
                            continue;
                        }
                    };

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
                        }
                        Err(e) => {
                            crate::vlog!(
                                "[islands] island#{i}: ladder bail — {e} — not claimed, falling back"
                            );
                            all_claimed = false;
                        }
                    }
                }
            }
        }

        // Build a label map before dropping box_by_id
        graph
            .boxes
            .iter()
            .map(|b| (b.id, b.display_label().to_string()))
            .collect()
    }; // ★ box_by_id dropped here — mutable borrow is now safe

    if !all_claimed {
        crate::vlog!(
            "[islands] not all islands claimed ({}/{} succeeded) — falling back to whole-graph models",
            sp_models.len() + ladder_models.len(),
            d.islands.len()
        );
        return false;
    }

    // ── Phase B: apply all models ──────────────────────────────────────────
    for (model, i) in &sp_models {
        crate::vlog!("[islands] applying SP model for island#{i}");
        apply_sp_model(graph, model);
    }
    for (model, i) in &ladder_models {
        crate::vlog!("[islands] applying ladder model for island#{i}");
        super::ladder_place::apply_ladder_model(graph, model);
    }

    // ── Direct bands: log only, routing is handled by trunk_tap ────────────
    for db in &d.direct_bands {
        let left_name = box_owned
            .get(&db.left_box)
            .cloned()
            .unwrap_or_else(|| "?".into());
        let right_name = box_owned
            .get(&db.right_box)
            .cloned()
            .unwrap_or_else(|| "?".into());
        crate::vlog!(
            "[islands] direct band net[{}] '{}' : {} ~ {} — log only, routing by trunk_tap",
            db.net,
            graph.nets[db.net].name,
            left_name,
            right_name
        );
    }

    crate::vlog!(
        "[islands] ✓ all {} island(s) claimed and applied, {} direct band(s) logged",
        d.islands.len(),
        d.direct_bands.len()
    );
    true
}

/// Collect the boundary boxes for an island — the non-passive, non-rail boxes
/// that touch the island's boundary nets.
fn boundary_boxes(
    graph: &McVecGraph,
    isl: &Island,
    box_by_id: &HashMap<i64, &crate::vector::graph::McVecBox>,
) -> HashSet<i64> {
    let mut boxes: HashSet<i64> = HashSet::new();
    for &ni in &isl.boundaries {
        for ep in &graph.nets[ni].endpoints {
            if let Some(b) = box_by_id.get(&ep.box_id) {
                if b.id >= 0 && !is_rail_box(b) && !b.is_two_pin_passive() {
                    boxes.insert(b.id);
                }
            }
        }
    }
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
