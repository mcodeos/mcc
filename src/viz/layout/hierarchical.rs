// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW —— Hierarchical layout (top-level simplest integration dedicated)
//!
//! User's words:
//! > The outermost layer depicts the simplest connection, a highest-level integration,
//! > it connects each module together.
//!
//! This layout works best after `apply_promote_recursive`:
//! top-level only has sub_module + a few cross-module nets,
//! `HierarchicalLayouter` layers them by "signal flow", rendering the common schematic visual effect
//! of "power on top, ground on bottom, signals flowing horizontally".
//!
//! ## Algorithm (simplified Sugiyama-style)
//! 1. Categorize boxes: power (top rail) / ground (bottom rail) / normal (middle layer)
//! 2. Normal boxes compute rank via BFS-from-power (layers of distance from power)
//! 3. Sort within same rank by "edge weight" (number of shared edges with left neighbor → reduce crossings)
//! 4. Each rank is one row, evenly spaced within row
//!
//! ## ★ P08 (S4) Changes
//! `categorize_boxes` uses three-level recursion to identify power/ground roles (Symbol → name → connected nets).
//!
//! ## ★ P06 (S5) Changes
//! `layout` is now **two rounds**:
//! 1. Compute size + coarse `assign_entry_points_coarse` (only looks at name/IoDirection)
//! 2. Compute coordinates (rank / row placement / overlap)
//! 3. **Refine** `assign_entry_points_refine` (use final coordinates to rearrange semantic-less pins)
//! 4. (Optional) `recompute_sizes_with_pin_count` + remove overlaps again
//!
//! Visual effect: U-shaped wire routing greatly reduced —— Generic pins exit towards neighbor direction, no longer "exit Left and route around to Right".
//!
//! ## Visual Effect (Expected)
//! ```text
//!  [V5V]                          [VBUS]              ← rank 0 (power)
//!    │                               │
//!  [LDO_5to3.3]                   [USB_HUB]           ← rank 1
//!    │           ╲                   │
//!  [V3V3]         ╲           [USB_PHY]               ← rank 2
//!    │             ╲                 │
//!  [MCU]──────────[Crystal]      [Speaker]            ← rank 3
//!    │
//!  [GND]                                              ← rank LAST (ground)
//! ```

use std::collections::{HashMap, VecDeque};

use crate::vector::graph::naming;
use crate::vector::graph::{BoxKind, McVecGraph, NetKind, Symbol};

use super::components::build_adjacency;
use super::entry_points::{assign_entry_points_coarse, assign_entry_points_refine};
use super::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use super::overlap::resolve_overlaps_iterative;
use super::size::{assign_default_sizes, recompute_sizes_with_pin_count, MIN_GAP};
use crate::viz::traits::Layouter;

// ============================================================================
// HierarchicalLayouter
// ============================================================================

pub struct HierarchicalLayouter {
    /// Row gap (vertical distance between ranks)
    pub row_gap: f64,
    /// Column gap (horizontal distance between boxes within same rank)
    pub col_gap: f64,
}

impl Default for HierarchicalLayouter {
    fn default() -> Self {
        Self {
            row_gap: 80.0,
            col_gap: 60.0,
        }
    }
}

impl Layouter for HierarchicalLayouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        // ── Round 1: coarse entry_points (only look at name / IoDirection) ──
        assign_default_sizes(graph);
        assign_entry_points_coarse(graph);

        if graph.boxes.len() == 1 {
            graph.boxes[0].x = CANVAS_MARGIN;
            graph.boxes[0].y = CANVAS_MARGIN;
            return compute_canvas(graph);
        }

        // 1. Categorize
        let categories = categorize_boxes(graph);

        // 2. Compute rank
        let adj = build_adjacency(graph);
        let ranks = assign_ranks(graph, &adj, &categories);

        // 3. Group by rank (rank → all box_ids in that row, sorted by adjacency)
        let rows = group_by_rank_sorted(graph, &ranks, &adj);

        // 4. Place (left to right per row, y = rank * row_height)
        let canvas = self.place_rows(graph, &rows);

        // 5. Cross-rank overlap removal (probably not needed, but for safety)
        resolve_overlaps_iterative(graph, 10);
        normalize_positions(graph);
        let _ = canvas;

        // ── Round 2: refine entry_points (rearrange semantic-less pins using final coordinates) ──
        // ★ P06 (S5): This step eliminates U-shaped routing where "neighbor is on right but pin faces left"
        assign_entry_points_refine(graph);

        // ── Round 2.5: after pin re-distribution to sides, side density may change → recompute size ──
        let resized = recompute_sizes_with_pin_count(graph);
        if resized {
            // size changed, may cause slight overlap, lightweight fix
            resolve_overlaps_iterative(graph, 5);
            normalize_positions(graph);
        }

        compute_canvas(graph)
    }

    fn name(&self) -> &'static str {
        "hierarchical"
    }
}

impl HierarchicalLayouter {
    fn place_rows(&self, graph: &mut McVecGraph, rows: &[Vec<i64>]) -> (f64, f64) {
        let mut cur_y = CANVAS_MARGIN;
        let mut max_right: f64 = 0.0;

        for row in rows {
            // Compute max height of this row
            let row_h: f64 = row
                .iter()
                .filter_map(|id| graph.boxes.iter().find(|b| b.id == *id))
                .map(|b| b.h)
                .fold(0.0f64, f64::max);

            // Lay out horizontally
            let mut cur_x = CANVAS_MARGIN;
            for &id in row {
                if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
                    b.x = cur_x;
                    b.y = cur_y + (row_h - b.h) / 2.0; // vertically centered within row
                    cur_x += b.w + self.col_gap;
                }
            }

            max_right = max_right.max(cur_x);
            cur_y += row_h + self.row_gap;
        }

        (max_right, cur_y)
    }
}

// ============================================================================
// Categorize: power / ground / normal
// ============================================================================

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Category {
    Power,
    Ground,
    Normal,
}

/// ★ P08 (S4) rewrite: three-level recursion to identify box roles
///
/// Priority (high to low):
/// 1. **`box.symbol`** — `Symbol::PowerRail { is_ground }` filled in by P01 anchors directly
/// 2. **`box.name`** heuristic — `naming::is_power` / `naming::is_ground` (compatible with old data)
/// 3. **NetKind of connected nets** — a box that **only** connects to Power net, treat as power source;
///    Same for Ground
/// 4. Otherwise Normal
///
/// ## Design Tradeoff
/// Rule 3 is deliberately conservative: must have **all** connected nets of the same semantic kind to classify,
/// to avoid misclassifying MCU (connects VCC + GND + lots of signals) as power source. In other words, rule 3 mainly
/// catches "transit boxes" (e.g. an LDO output only connects to one V3V3 net, or pure power distribution box).
///
/// ## Why Rule 3 is Needed
/// After `synthesize_rail_nets` + promote, top-level might not have `PowerLabel` box,
/// all are `SubModule`, but some of those boxes were LDO at sub-layer, after promotion can only be inferred
/// "it's power" by the Power nets they connect to.
fn categorize_boxes(graph: &McVecGraph) -> HashMap<i64, Category> {
    let mut out = HashMap::new();
    for b in &graph.boxes {
        out.insert(b.id, classify_one_box(graph, b));
    }
    out
}

/// Classify a single box (P08 rewrite)
fn classify_one_box(graph: &McVecGraph, b: &crate::vector::graph::McVecBox) -> Category {
    // ── Rule 1: Symbol::PowerRail anchors directly ──
    if let Symbol::PowerRail { is_ground } = b.symbol {
        return if is_ground {
            Category::Ground
        } else {
            Category::Power
        };
    }

    // ── Rule 2: name heuristic (compatible with projects where P01 didn't run / old fixtures) ──
    if naming::is_ground(&b.name) {
        return Category::Ground;
    }
    if naming::is_power(&b.name) {
        return Category::Power;
    }

    // ── Rule 2b: BoxKind::PowerLabel but no symbol/name match (legacy fallback) ──
    // This barely triggers after P01, but kept to prevent old fixtures from degrading.
    if b.kind == BoxKind::PowerLabel {
        // PowerLabel defaults to Power, unless name is ground
        let u = b.name.to_uppercase();
        if u.starts_with("GND") || u == "VSS" {
            return Category::Ground;
        }
        return Category::Power;
    }

    // ── Rule 3: Look at NetKind of connected nets ──
    // (Note: if a box has no net connections at all, returns empty list here, classified as Normal)
    let net_kinds = collect_net_kinds_of_box(graph, b.id);
    if !net_kinds.is_empty() {
        let all_power = net_kinds.iter().all(|k| matches!(k, NetKind::Power));
        let all_ground = net_kinds.iter().all(|k| matches!(k, NetKind::Ground));
        if all_power {
            return Category::Power;
        }
        if all_ground {
            return Category::Ground;
        }
    }

    Category::Normal
}

/// Collect NetKind of all nets connected to a box
///
/// Note: NetKind doesn't implement Hash, so use Vec instead of HashSet.
fn collect_net_kinds_of_box(graph: &McVecGraph, box_id: i64) -> Vec<NetKind> {
    graph
        .nets
        .iter()
        .filter(|n| n.endpoints.iter().any(|e| e.box_id == box_id))
        .map(|n| n.kind.clone())
        .collect()
}

// ============================================================================
// Rank assignment (BFS-from-power)
// ============================================================================

/// Compute a rank for each box
///
/// - power: rank 0
/// - normal: BFS distance (from any power) + 1
/// - unreachable normal: place at max rank separately
/// - ground: always the last rank (after all normal)
fn assign_ranks(
    graph: &McVecGraph,
    adj: &HashMap<i64, Vec<i64>>,
    categories: &HashMap<i64, Category>,
) -> HashMap<i64, usize> {
    let mut rank: HashMap<i64, usize> = HashMap::new();
    let mut queue: VecDeque<(i64, usize)> = VecDeque::new();

    // Step 1: power → rank 0
    for b in &graph.boxes {
        if categories.get(&b.id) == Some(&Category::Power) {
            rank.insert(b.id, 0);
            queue.push_back((b.id, 0));
        }
    }

    // No power: degenerate, pick highest-degree non-ground as rank 0
    if rank.is_empty() {
        // Find a non-ground seed
        let seed = graph
            .boxes
            .iter()
            .filter(|b| categories.get(&b.id) != Some(&Category::Ground))
            .max_by_key(|b| adj.get(&b.id).map(|v| v.len()).unwrap_or(0))
            .map(|b| b.id);
        if let Some(s) = seed {
            rank.insert(s, 0);
            queue.push_back((s, 0));
        }
    }

    // Step 2: BFS spread
    while let Some((cur, cur_rank)) = queue.pop_front() {
        if let Some(neighbors) = adj.get(&cur) {
            for &n in neighbors {
                // ground doesn't participate in BFS, they're placed last
                if categories.get(&n) == Some(&Category::Ground) {
                    continue;
                }
                if rank.contains_key(&n) {
                    continue;
                }
                rank.insert(n, cur_rank + 1);
                queue.push_back((n, cur_rank + 1));
            }
        }
    }

    // Step 3: unreachable normal → place at current max rank + 1
    let mut max_normal_rank = rank.values().max().copied().unwrap_or(0);
    for b in &graph.boxes {
        if categories.get(&b.id) == Some(&Category::Normal) && !rank.contains_key(&b.id) {
            rank.insert(b.id, max_normal_rank + 1);
        }
    }

    // Recompute max
    max_normal_rank = rank.values().max().copied().unwrap_or(0);

    // Step 4: ground → max rank + 1
    for b in &graph.boxes {
        if categories.get(&b.id) == Some(&Category::Ground) {
            rank.insert(b.id, max_normal_rank + 1);
        }
    }

    rank
}

// ============================================================================
// Sort within same rank (reduce crossings)
// ============================================================================

/// Group by rank and sort within each group
///
/// Sort strategy (simplified barycenter):
/// First row: by ID ascending (stable starting point)
/// Row r: by "average x index of previous row's neighbors" ascending ── make connections more vertical, reduce crossings
fn group_by_rank_sorted(
    _graph: &McVecGraph,
    ranks: &HashMap<i64, usize>,
    adj: &HashMap<i64, Vec<i64>>,
) -> Vec<Vec<i64>> {
    let max_rank = ranks.values().max().copied().unwrap_or(0);

    // First bucket by rank
    let mut rows: Vec<Vec<i64>> = vec![Vec::new(); max_rank + 1];
    for (&id, &r) in ranks {
        rows[r].push(id);
    }

    // rank 0: ID ascending (stable starting point, used as barycenter reference later)
    if !rows.is_empty() {
        rows[0].sort();
    }

    // Subsequent ranks: compute each box's "average x index of previous row's neighbors", sort ascending
    for r in 1..=max_rank {
        // Get prev row's IDs → their position in prev row (index)
        let prev_index: HashMap<i64, usize> = rows[r - 1]
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, i))
            .collect();

        let mut sorted_row = std::mem::take(&mut rows[r]);
        sorted_row.sort_by_key(|&id| {
            let neighbors_in_prev: Vec<usize> = adj
                .get(&id)
                .map(|nbs| {
                    nbs.iter()
                        .filter_map(|n| prev_index.get(n).copied())
                        .collect()
                })
                .unwrap_or_default();
            if neighbors_in_prev.is_empty() {
                // Nodes with no connection to previous row go to the end
                usize::MAX
            } else {
                neighbors_in_prev.iter().sum::<usize>() / neighbors_in_prev.len()
            }
        });
        rows[r] = sorted_row;
    }

    // Fault tolerance: filter empty ranks (empty layers left by BFS jumps)
    rows.retain(|r| !r.is_empty());

    eprintln!(
        "[layout::hierarchical] {} rank rows, sizes={:?}",
        rows.len(),
        rows.iter().map(|r| r.len()).collect::<Vec<_>>(),
    );

    // Hint: use MIN_GAP to prevent dead_code
    let _ = MIN_GAP;
    rows
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{EndpointRef, IoSummary, McVecBox, NetKind, VizNet};

    fn mk_box(id: i64, name: &str, kind: BoxKind) -> McVecBox {
        McVecBox::new(id, name.into(), String::new(), kind, 1, IoSummary::new())
    }

    /// ★ P03 refactor: test fixtures push VizNet instead of McVecEdge
    ///
    /// One net has two endpoints, using synthetic endpoint (pin_id=-1) to represent the whole box.
    /// `build_adjacency` looks at nets, so fixtures work normally.
    fn mk_net(nid: i64, name: &str, a: i64, b: i64) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            NetKind::Signal,
            vec![
                EndpointRef::new(a, -1, "(test)"),
                EndpointRef::new(b, -1, "(test)"),
            ],
        )
    }

    #[test]
    fn test_categorize() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, "VCC", BoxKind::PowerLabel));
        g.boxes.push(mk_box(2, "GND", BoxKind::PowerLabel));
        g.boxes.push(mk_box(3, "MCU", BoxKind::SubModule));

        let cats = categorize_boxes(&g);
        assert_eq!(cats[&1], Category::Power);
        assert_eq!(cats[&2], Category::Ground);
        assert_eq!(cats[&3], Category::Normal);
    }

    #[test]
    fn test_assign_ranks_basic() {
        // VCC → MCU → GND  (chain)
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, "VCC", BoxKind::PowerLabel));
        g.boxes.push(mk_box(2, "MCU", BoxKind::SubModule));
        g.boxes.push(mk_box(3, "GND", BoxKind::PowerLabel));
        g.nets.push(mk_net(0, "n1", 1, 2));
        g.nets.push(mk_net(1, "n2", 2, 3));

        let cats = categorize_boxes(&g);
        let adj = build_adjacency(&g);
        let ranks = assign_ranks(&g, &adj, &cats);
        assert_eq!(ranks[&1], 0); // VCC top
        assert_eq!(ranks[&2], 1); // MCU middle
        assert_eq!(ranks[&3], 2); // GND last (= max_normal + 1)
    }

    #[test]
    fn test_full_layout_runs() {
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, "VCC", BoxKind::PowerLabel));
        g.boxes.push(mk_box(2, "MCU", BoxKind::SubModule));
        g.boxes.push(mk_box(3, "GND", BoxKind::PowerLabel));
        g.nets.push(mk_net(0, "n1", 1, 2));
        g.nets.push(mk_net(1, "n2", 2, 3));

        let layouter = HierarchicalLayouter::default();
        let (cw, ch) = layouter.layout(&mut g);
        assert!(cw > 0.0 && ch > 0.0);
        // VCC y < MCU y < GND y
        let vcc_y = g.boxes.iter().find(|b| b.id == 1).unwrap().y;
        let mcu_y = g.boxes.iter().find(|b| b.id == 2).unwrap().y;
        let gnd_y = g.boxes.iter().find(|b| b.id == 3).unwrap().y;
        assert!(vcc_y < mcu_y, "VCC should be above MCU");
        assert!(mcu_y < gnd_y, "MCU should be above GND");
    }

    // ========================================================================
    // ★ P08 (S4) categorize_boxes new rules tests
    // ========================================================================

    /// Helper: construct box with Symbol
    fn mk_box_with_symbol(id: i64, name: &str, kind: BoxKind, symbol: Symbol) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            symbol,
            None,
            None,
            1,
            IoSummary::new(),
        )
    }

    /// Helper: construct net of specified kind
    fn mk_net_kind(nid: i64, name: &str, kind: NetKind, a: i64, b: i64) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            kind,
            vec![
                EndpointRef::new(a, -1, "(test)"),
                EndpointRef::new(b, -1, "(test)"),
            ],
        )
    }

    #[test]
    fn p08_categorize_via_symbol_power_rail() {
        // ★ P08 Rule 1: Symbol::PowerRail beats BoxKind / name
        let mut g = McVecGraph::new(0, "test".into());
        // Intentionally use SubModule kind + "MCU" name, but symbol says PowerRail
        g.boxes.push(mk_box_with_symbol(
            1,
            "MCU", // misleading name
            BoxKind::SubModule,
            Symbol::PowerRail { is_ground: false },
        ));
        g.boxes.push(mk_box_with_symbol(
            2,
            "X",
            BoxKind::SubModule,
            Symbol::PowerRail { is_ground: true },
        ));

        let cats = categorize_boxes(&g);
        assert_eq!(
            cats[&1],
            Category::Power,
            "Symbol::PowerRail beats name+kind"
        );
        assert_eq!(cats[&2], Category::Ground);
    }

    #[test]
    fn p08_categorize_via_name_when_no_symbol() {
        // ★ P08 Rule 2: name heuristic (old fixture / Symbol::Unknown fallback)
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, "VCC", BoxKind::SubModule)); // name=VCC, no symbol
        g.boxes.push(mk_box(2, "GND", BoxKind::SubModule));
        g.boxes.push(mk_box(3, "MCU", BoxKind::SubModule));
        // VCC/GND should be recognized by naming::is_power / naming::is_ground
        let cats = categorize_boxes(&g);
        assert_eq!(cats[&1], Category::Power);
        assert_eq!(cats[&2], Category::Ground);
        assert_eq!(cats[&3], Category::Normal);
    }

    #[test]
    fn p08_categorize_via_connected_nets_power_only() {
        // ★ P08 Rule 3: box only connects to Power net → Power
        let mut g = McVecGraph::new(0, "test".into());
        // "ldo_out" box, not a power label, no power name, but **only** connects to Power net
        g.boxes.push(mk_box(1, "ldo_out", BoxKind::SubModule));
        g.boxes.push(mk_box(2, "consumer1", BoxKind::SubModule));
        g.boxes.push(mk_box(3, "consumer2", BoxKind::SubModule));

        // ldo_out connects to two Power kind nets
        g.nets.push(mk_net_kind(0, "V3V3", NetKind::Power, 1, 2));
        g.nets.push(mk_net_kind(1, "V3V3", NetKind::Power, 1, 3));

        let cats = categorize_boxes(&g);
        // ldo_out only connects to Power net → classified as Power
        assert_eq!(cats[&1], Category::Power);
    }

    #[test]
    fn p08_categorize_mixed_nets_not_power() {
        // ★ P08 Rule 3 conservatism: connecting Power + Signal → not Power
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, "mcu", BoxKind::SubModule));
        g.boxes.push(mk_box(2, "rail", BoxKind::SubModule));
        g.boxes.push(mk_box(3, "device", BoxKind::SubModule));

        // mcu connects both Power + Signal net
        g.nets.push(mk_net_kind(0, "V3V3", NetKind::Power, 1, 2));
        g.nets
            .push(mk_net_kind(1, "SPI_CLK", NetKind::Signal, 1, 3));

        let cats = categorize_boxes(&g);
        // mcu connects mixed nets → Normal, not Power
        assert_eq!(
            cats[&1],
            Category::Normal,
            "MCU connecting both Power and Signal should NOT be classified Power"
        );
    }

    #[test]
    fn p08_categorize_connected_only_ground() {
        // ★ P08 Rule 3 symmetric: only connects Ground → Ground
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, "gnd_plane", BoxKind::SubModule));
        g.boxes.push(mk_box(2, "consumer1", BoxKind::SubModule));
        g.boxes.push(mk_box(3, "consumer2", BoxKind::SubModule));

        g.nets.push(mk_net_kind(0, "GND", NetKind::Ground, 1, 2));
        g.nets.push(mk_net_kind(1, "GND", NetKind::Ground, 1, 3));

        let cats = categorize_boxes(&g);
        assert_eq!(cats[&1], Category::Ground);
    }

    #[test]
    fn p08_categorize_no_connection_is_normal() {
        // ★ P08 Rule 3 degenerate: isolated box with no nets → Normal (don't misclassify as Power/Ground)
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, "orphan", BoxKind::SubModule));
        let cats = categorize_boxes(&g);
        assert_eq!(cats[&1], Category::Normal);
    }

    #[test]
    fn p08_categorize_priority_symbol_wins_over_connected_nets() {
        // ★ P08 rule priority: Symbol::PowerRail beats Rule 3 net analysis
        let mut g = McVecGraph::new(0, "test".into());
        // A Symbol::PowerRail (is_ground=true), but it **happens** to connect to a Signal net
        g.boxes.push(mk_box_with_symbol(
            1,
            "GND",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: true },
        ));
        g.boxes.push(mk_box(2, "x", BoxKind::SubModule));
        // Intentionally add a Signal net for interference
        g.nets.push(mk_net_kind(0, "weird", NetKind::Signal, 1, 2));

        let cats = categorize_boxes(&g);
        assert_eq!(
            cats[&1],
            Category::Ground,
            "Symbol::PowerRail should override connected-net analysis"
        );
    }

    #[test]
    fn p08_promoted_power_net_drives_power_layout() {
        // ★ P08 end-to-end: even without PowerLabel box at top, as long as net.kind=Power remains,
        // "pure power-only boxes connected to power nets" can be classified to Power layer
        let mut g = McVecGraph::new(0, "top".into());
        // No PowerLabel, only SubModule
        g.boxes.push(mk_box(1, "vreg", BoxKind::SubModule));
        g.boxes.push(mk_box(2, "mcu513", BoxKind::SubModule));
        g.boxes.push(mk_box(3, "gnd_sink", BoxKind::SubModule));

        // Key: after promote, VCC net kind is still Power (P08 modified promote to preserve)
        g.nets.push(mk_net_kind(0, "VCC", NetKind::Power, 1, 2));
        g.nets.push(mk_net_kind(1, "GND", NetKind::Ground, 2, 3));

        let cats = categorize_boxes(&g);
        // vreg only connects to Power net → Power layer
        assert_eq!(cats[&1], Category::Power);
        // mcu513 connects Power + Ground → Normal (conservative)
        assert_eq!(cats[&2], Category::Normal);
        // gnd_sink only connects Ground net → Ground layer
        assert_eq!(cats[&3], Category::Ground);

        // Run full layout, verify y ordering
        let layouter = HierarchicalLayouter::default();
        layouter.layout(&mut g);
        let y1 = g.boxes.iter().find(|b| b.id == 1).unwrap().y;
        let y2 = g.boxes.iter().find(|b| b.id == 2).unwrap().y;
        let y3 = g.boxes.iter().find(|b| b.id == 3).unwrap().y;
        assert!(y1 < y2, "vreg (Power) should be above mcu513");
        assert!(y2 < y3, "mcu513 should be above gnd_sink (Ground)");
    }
}
