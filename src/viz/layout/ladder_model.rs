// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Phase A — Ladder topology model (pure, geometry-free)
//!
//! ## What problem does this file solve
//! `two_lane_ladder` reconstructed the two lanes by walking nets, then spread each
//! lane's passives **evenly and independently** across the anchor span
//! (`step = inner_span / (n + 1)`). A 3-element lane and a 2-element lane therefore
//! land on **different column grids**, and the bridge caps — which must sit on a
//! column *shared* by both lanes — have nowhere clean to go. They end up on top of a
//! lane resistor, and their tap lands on the trunk's **endpoint** instead of its
//! interior, so the router draws an L instead of a T (visible as "zero junction dots
//! in the whole picture").
//!
//! But the column structure is not a geometric question. It is already fully
//! determined by the net list:
//!
//! ```text
//!   node  = net             (one vertical equipotential column)
//!   edge  = 2-pin passive   (touches exactly 2 nets)
//!     · series (RES)  -> consumes one column step:  rank(b) = rank(a) + 1
//!     · bridge (CAP') -> consumes none, and asserts: col(a) == col(b)
//!   anchors pin both ends.
//! ```
//!
//! So: union-find the bridges (this is the "closing bracket" — `net_1` and `net_4`
//! are *proven* to be the same column by `CAP2`), then longest-path rank the
//! resulting DAG. This is exactly graphviz's `rank` + `rank=same`, or Sugiyama's
//! layer assignment.
//!
//! ## `c07_pins` worked example
//! ```text
//! nets                                     element graph
//! ────────────────────────────────────     ─────────────────────────────────────
//! N0 : u1.3 ~ RES1.1                       N0 --RES1-- N1 --RES3-- N2 --RES6-- N3
//! N1 : RES1.2 ~ RES3.1 ~ CAP2.1            N4 --RES4-- N5 --RES7-- N6
//! N2 : RES3.2 ~ RES6.1 ~ CAP5.1            rungs: CAP2=(N1,N4)  CAP5=(N2,N5)
//! N3 : RES6.2 ~ u2.3                       anchors: u1 -> {N0,N4}  u2 -> {N3,N6}
//! N4 : u1.6 ~ RES4.1 ~ CAP2.2
//! N5 : RES4.2 ~ RES7.1 ~ CAP5.2       (1) union-find:  C1={N1,N4}  C2={N2,N5}
//! N6 : RES7.2 ~ u2.6                  (2) lanes:       u1.3->0     u1.6->1
//!                                     (3) DAG:  N0->C1->C2->N3   C1->C2->N6
//!                                     (4) rank: N0=0 C1=1 C2=2 N3=N6=3
//!
//! col:        0          1          2          3
//! lane0:  u1.3┼──RES1────┼──RES3────┼──RES6────┼─u2.3
//!             │        CAP2       CAP5         │
//! lane1:  u1.6┼──────────┼──RES4────┼──RES7────┼─u2.6
//!                 (`_`)
//! ```
//! `N4 ∈ C1` has rank 1 while `u1.6` sits at the left edge — i.e. lane 1 carries no
//! element before the first rung. The `_` placeholder needs no special case; it is
//! the natural consequence of the rank constraint.
//!
//! ## Scope / contract
//! This module is **pure**. It reads `nets` plus per-box metadata (`visual_role`,
//! `io_summary`, `is_two_pin_passive`) and returns a topology. It never reads or
//! writes `x/y/w/h` or `entry_points` — Phase B (`apply_ladder_model`) turns the
//! model into coordinates, and is then the *last* writer of that geometry.
//!
//! Anything that is not a clean ladder returns `Err(LadderBail)` and the caller falls
//! back to the generic flow layout: one specialised model plus one generic fallback,
//! rather than one heuristic patched everywhere. Every bail carries the offending id
//! so the log names it.
//!
//! ## Why `visual_role == BridgePassive` and not a topological test
//! A rung is a chord of the ladder graph; identifying it topologically means finding
//! faces — fragile, and unnecessary. `'` in the source *is* the authored intent, and
//! it already reaches the box via
//! `line.rs::bridge_passive_names → InstTable::bridge_passive_paths → from_block`.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::vector::graph::{McVecGraph, VisualRole};

use super::rails::is_rail_box;

// ============================================================================
// Model
// ============================================================================

/// Lane index. `0` is the lane seeded by the left anchor's first pin (pin order is
/// the tie-break, so Phase B can map lane -> y by walking the anchor's pins in the
/// same order).
pub type Lane = usize;

/// Column index (`0 ..= n_cols-1`). A column is one vertical equipotential line.
pub type Col = usize;

/// A series element (RES/L/...): sits on **one** lane, spanning the gap between two
/// columns. `to_col` is normally `from_col + 1`; it can be larger when the sink
/// right-align (see [`build_ladder_model`] step 8) stretched the last column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesSlot {
    pub box_id: i64,
    pub lane: Lane,
    pub from_col: Col,
    pub to_col: Col,
}

/// A bridge element (`CAP()'`): sits **on** a column, spanning two lanes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSlot {
    pub box_id: i64,
    pub col: Col,
    /// Always `lane_a < lane_b`.
    pub lane_a: Lane,
    pub lane_b: Lane,
}

/// The ladder topology. Pure combinatorics — no coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct LadderModel {
    /// Source-side anchor (more outputs wins; ties break on lower box id).
    pub left: i64,
    /// Sink-side anchor.
    pub right: i64,
    pub n_lanes: usize,
    pub n_cols: usize,
    /// `lane_pin[l]` = the left anchor's pin_id that seeds lane `l`, ascending.
    /// Phase B uses this to decide which lane gets which y.
    pub lane_pin: Vec<i64>,
    /// Sorted by `(lane, from_col, box_id)`.
    pub series: Vec<SeriesSlot>,
    /// Sorted by `(col, lane_a, box_id)`.
    pub bridges: Vec<BridgeSlot>,
    /// `nid -> (lane, col)` — the net's tap column. For the probe and the router.
    pub net_col: HashMap<i64, (Lane, Col)>,
}

impl LadderModel {
    /// All boxes the model places (anchors excluded).
    pub fn placed_boxes(&self) -> impl Iterator<Item = i64> + '_ {
        self.series
            .iter()
            .map(|s| s.box_id)
            .chain(self.bridges.iter().map(|b| b.box_id))
    }
}

/// Why a graph is not a ladder. Every variant names the offender so the log is
/// actionable and tests can assert on the reason rather than on `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LadderBail {
    /// Not exactly two anchor boxes (non-rail, non-passive, touching >= 2 nets).
    AnchorCount(usize),
    /// No `CAP()'` anywhere — nothing constrains the lanes to a shared grid, so the
    /// generic layout is as good as this model. (Flip this if unbridged multi-lane
    /// buses should also be ranked.)
    NoBridge,
    /// A 2-pin passive that does not touch exactly 2 nets (bypass cap, dangling, ...).
    PassiveNetCount { box_id: i64, nets: usize },
    /// A series element joins two different lanes — that is not a ladder.
    SeriesCrossesLane { box_id: i64 },
    /// A bridge whose two nets are on the same lane — mis-marked, or not a rung.
    BridgeSameLane { box_id: i64, lane: Lane },
    /// A net no lane BFS could reach from the left anchor.
    UnreachableNet { nid: i64 },
    /// The class DAG has a cycle (feedback / backward bridge) — no longest path.
    Cycle,
    /// A series element whose two nets are the same column (parallel with a bridge).
    SelfLoop { box_id: i64 },
    /// Two elements want the same `(lane, column-gap)` or the same rung — v1 has no
    /// sub-columns.
    SlotConflict { a: i64, b: i64 },
    /// Anchors disagree on the lane count, or two of an anchor's pins share a lane.
    AnchorLaneMismatch {
        box_id: i64,
        nets: usize,
        lanes: usize,
    },
    /// A box sits on a lane net but is neither an anchor nor a ladder element
    /// (rail flag, third IC, ...).
    ForeignBox { box_id: i64 },
}

impl std::fmt::Display for LadderBail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LadderBail::AnchorCount(n) => write!(f, "anchor count = {n} (need exactly 2)"),
            LadderBail::NoBridge => write!(f, "no bridge passive (CAP') present"),
            LadderBail::PassiveNetCount { box_id, nets } => {
                write!(f, "passive #{box_id} touches {nets} net(s), need 2")
            }
            LadderBail::SeriesCrossesLane { box_id } => {
                write!(f, "series #{box_id} joins two lanes")
            }
            LadderBail::BridgeSameLane { box_id, lane } => {
                write!(f, "bridge #{box_id} has both ends on lane {lane}")
            }
            LadderBail::UnreachableNet { nid } => {
                write!(f, "net {nid} unreachable from left anchor")
            }
            LadderBail::Cycle => write!(f, "column DAG has a cycle"),
            LadderBail::SelfLoop { box_id } => {
                write!(f, "series #{box_id} has both ends in one column")
            }
            LadderBail::SlotConflict { a, b } => write!(f, "#{a} and #{b} want the same slot"),
            LadderBail::AnchorLaneMismatch {
                box_id,
                nets,
                lanes,
            } => {
                write!(f, "anchor #{box_id} has {nets} net(s) for {lanes} lane(s)")
            }
            LadderBail::ForeignBox { box_id } => {
                write!(
                    f,
                    "#{box_id} sits on a lane but is not an anchor or a 2-pin passive"
                )
            }
        }
    }
}

// ============================================================================
// Public entry
// ============================================================================

/// Logging wrapper: dump the model on success, name the bail reason on failure.
/// This is what the layouter calls.
pub fn try_build_ladder_model(graph: &McVecGraph) -> Option<LadderModel> {
    match build_ladder_model(graph) {
        Ok(m) => {
            crate::vlog!("{}", dump(&m, graph));
            Some(m)
        }
        Err(reason) => {
            crate::vlog!("[ladder-model] bail: {reason}");
            None
        }
    }
}

/// Build the ladder topology from the net list. Pure; never touches geometry.
pub fn build_ladder_model(graph: &McVecGraph) -> Result<LadderModel, LadderBail> {
    let n_nets = graph.nets.len();
    if n_nets == 0 {
        return Err(LadderBail::AnchorCount(0));
    }
    let box_nets = box_net_index(graph);

    // ── 1. Anchors: non-rail, non-passive, touching >= 2 nets ────────────────
    //    `main` (the SubModule boundary) touches 0 nets, so it never qualifies.
    let mut anchors: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.id >= 0 && !is_rail_box(b) && !b.is_two_pin_passive())
        .filter(|b| box_nets.get(&b.id).map(|v| v.len()).unwrap_or(0) >= 2)
        .map(|b| b.id)
        .collect();
    anchors.sort_unstable();
    if anchors.len() != 2 {
        return Err(LadderBail::AnchorCount(anchors.len()));
    }
    let out_of = |id: i64| -> usize {
        graph
            .boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.io_summary.outputs)
            .unwrap_or(0)
    };
    // Source drives the lanes left-to-right. Ties break on the lower id, so the
    // model is deterministic even when neither anchor carries direction.
    let (left, right) = if out_of(anchors[0]) >= out_of(anchors[1]) {
        (anchors[0], anchors[1])
    } else {
        (anchors[1], anchors[0])
    };

    // ── 2. Elements: every 2-pin passive is an edge between exactly 2 nets ───
    let mut series_edges: Vec<(i64, usize, usize)> = Vec::new(); // (box, net_a, net_b)
    let mut bridge_edges: Vec<(i64, usize, usize)> = Vec::new();
    for b in &graph.boxes {
        if !b.is_two_pin_passive() {
            continue;
        }
        let nets = box_nets.get(&b.id).cloned().unwrap_or_default();
        if nets.len() != 2 {
            return Err(LadderBail::PassiveNetCount {
                box_id: b.id,
                nets: nets.len(),
            });
        }
        if b.visual_role == Some(VisualRole::BridgePassive) {
            bridge_edges.push((b.id, nets[0], nets[1]));
        } else {
            series_edges.push((b.id, nets[0], nets[1]));
        }
    }
    if bridge_edges.is_empty() {
        return Err(LadderBail::NoBridge);
    }

    // ── 3. Union-find: each bridge proves its two nets are one column ────────
    //    (this is the "closing bracket": CAP2 proves net_1 ≡ net_4)
    let mut dsu = Dsu::new(n_nets);
    for &(_, a, b) in &bridge_edges {
        dsu.union(a, b);
    }
    let class_of: Vec<usize> = (0..n_nets).map(|i| dsu.find(i)).collect();

    // ── 4. Lanes + BFS distance, walking SERIES edges only ──────────────────
    //    Seeds = the left anchor's nets, ordered by pin id -> lane index.
    let mut seeds: Vec<(i64, usize)> = box_nets
        .get(&left)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|ni| {
            graph.nets[ni]
                .endpoints
                .iter()
                .find(|e| e.box_id == left)
                .map(|e| (e.pin_id, ni))
        })
        .collect();
    seeds.sort_unstable();
    seeds.dedup_by_key(|(_, ni)| *ni);
    let n_lanes = seeds.len();

    let mut series_of_net: HashMap<usize, Vec<(i64, usize)>> = HashMap::new(); // net -> [(box, other)]
    for &(bid, a, b) in &series_edges {
        series_of_net.entry(a).or_default().push((bid, b));
        series_of_net.entry(b).or_default().push((bid, a));
    }

    let mut lane_of: HashMap<usize, Lane> = HashMap::new();
    let mut dist_of: HashMap<usize, usize> = HashMap::new();
    for (lane, &(_, seed)) in seeds.iter().enumerate() {
        lane_of.insert(seed, lane);
        dist_of.insert(seed, 0);
        let mut q: VecDeque<usize> = VecDeque::from([seed]);
        while let Some(n) = q.pop_front() {
            let d = dist_of[&n];
            for &(bid, other) in series_of_net.get(&n).into_iter().flatten() {
                match lane_of.get(&other).copied() {
                    // Reached a net another lane already owns -> a series element
                    // ties two lanes together. Not a ladder.
                    Some(l) if l != lane => {
                        return Err(LadderBail::SeriesCrossesLane { box_id: bid })
                    }
                    Some(_) => continue, // the net we came from, or a loop (caught by the DAG check)
                    None => {
                        lane_of.insert(other, lane);
                        dist_of.insert(other, d + 1);
                        q.push_back(other);
                    }
                }
            }
        }
    }
    for ni in 0..n_nets {
        if !lane_of.contains_key(&ni) {
            return Err(LadderBail::UnreachableNet {
                nid: graph.nets[ni].nid,
            });
        }
    }

    // ── 5. A rung must actually cross lanes ─────────────────────────────────
    for &(bid, a, b) in &bridge_edges {
        let (la, lb) = (lane_of[&a], lane_of[&b]);
        if la == lb {
            return Err(LadderBail::BridgeSameLane {
                box_id: bid,
                lane: la,
            });
        }
    }

    // ── 6. Orient series edges by BFS distance -> class DAG ─────────────────
    let mut roots: Vec<usize> = class_of.clone();
    roots.sort_unstable();
    roots.dedup();
    let idx_of: HashMap<usize, usize> = roots.iter().enumerate().map(|(i, &r)| (r, i)).collect();
    let k = roots.len();

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); k];
    let mut indeg: Vec<usize> = vec![0; k];
    // (box, from_net, to_net, from_class_idx, to_class_idx)
    let mut oriented: Vec<(i64, usize, usize, usize, usize)> = Vec::new();
    for &(bid, a, b) in &series_edges {
        let (from, to) = match dist_of[&a].cmp(&dist_of[&b]) {
            Ordering::Less => (a, b),
            Ordering::Greater => (b, a),
            // Equidistant from the source -> the lane is not a simple chain.
            Ordering::Equal => return Err(LadderBail::Cycle),
        };
        let (cu, cv) = (idx_of[&class_of[from]], idx_of[&class_of[to]]);
        if cu == cv {
            return Err(LadderBail::SelfLoop { box_id: bid });
        }
        adj[cu].push(cv);
        indeg[cv] += 1;
        oriented.push((bid, from, to, cu, cv));
    }

    // ── 7. Longest-path rank (Kahn). Parallel edges are fine: C1->C2 appears
    //       twice (RES3 and RES4) and both decrements land. ──────────────────
    let mut rank: Vec<usize> = vec![0; k];
    let mut indeg_work = indeg.clone();
    let mut q: VecDeque<usize> = (0..k).filter(|&i| indeg_work[i] == 0).collect();
    let mut settled = 0usize;
    while let Some(u) = q.pop_front() {
        settled += 1;
        let succ = adj[u].clone();
        for v in succ {
            rank[v] = rank[v].max(rank[u] + 1);
            indeg_work[v] -= 1;
            if indeg_work[v] == 0 {
                q.push_back(v);
            }
        }
    }
    if settled != k {
        return Err(LadderBail::Cycle);
    }

    // ── 8. Right-align the sinks so both lanes end on the same column ───────
    //    Raising a sink is always safe: it has no outgoing edge to violate.
    let right_nets = box_nets.get(&right).cloned().unwrap_or_default();
    {
        let mut lanes_seen: HashSet<Lane> = HashSet::new();
        for &ni in &right_nets {
            if !lanes_seen.insert(lane_of[&ni]) {
                return Err(LadderBail::AnchorLaneMismatch {
                    box_id: right,
                    nets: right_nets.len(),
                    lanes: n_lanes,
                });
            }
        }
        if right_nets.len() != n_lanes {
            return Err(LadderBail::AnchorLaneMismatch {
                box_id: right,
                nets: right_nets.len(),
                lanes: n_lanes,
            });
        }
    }
    let max_rank = rank.iter().copied().max().unwrap_or(0);
    for &ni in &right_nets {
        let ci = idx_of[&class_of[ni]];
        if adj[ci].is_empty() && rank[ci] < max_rank {
            crate::vlog!(
                "[ladder-model] right-align: net {} col {} -> {max_rank}",
                graph.nets[ni].nid,
                rank[ci]
            );
            rank[ci] = max_rank;
        }
    }
    let n_cols = max_rank + 1;

    // ── 9. Emit slots ───────────────────────────────────────────────────────
    let mut series: Vec<SeriesSlot> = oriented
        .iter()
        .map(|&(bid, from, _to, cu, cv)| SeriesSlot {
            box_id: bid,
            lane: lane_of[&from],
            from_col: rank[cu],
            to_col: rank[cv],
        })
        .collect();
    series.sort_by_key(|s| (s.lane, s.from_col, s.box_id));

    let mut bridges: Vec<BridgeSlot> = bridge_edges
        .iter()
        .map(|&(bid, a, b)| {
            let (la, lb) = (lane_of[&a], lane_of[&b]);
            BridgeSlot {
                box_id: bid,
                col: rank[idx_of[&class_of[a]]],
                lane_a: la.min(lb),
                lane_b: la.max(lb),
            }
        })
        .collect();
    bridges.sort_by_key(|b| (b.col, b.lane_a, b.box_id));

    // one element per (lane, gap); one rung per (column, lane pair)
    let mut occ: HashMap<(Lane, Col), i64> = HashMap::new();
    for s in &series {
        if let Some(&other) = occ.get(&(s.lane, s.from_col)) {
            return Err(LadderBail::SlotConflict {
                a: other,
                b: s.box_id,
            });
        }
        occ.insert((s.lane, s.from_col), s.box_id);
    }
    let mut bocc: HashMap<(Col, Lane, Lane), i64> = HashMap::new();
    for b in &bridges {
        if let Some(&other) = bocc.get(&(b.col, b.lane_a, b.lane_b)) {
            return Err(LadderBail::SlotConflict {
                a: other,
                b: b.box_id,
            });
        }
        bocc.insert((b.col, b.lane_a, b.lane_b), b.box_id);
    }

    // ── 10. Nothing foreign on the lanes ────────────────────────────────────
    let mut known: HashSet<i64> = HashSet::new();
    known.insert(left);
    known.insert(right);
    known.extend(series.iter().map(|s| s.box_id));
    known.extend(bridges.iter().map(|b| b.box_id));
    for net in &graph.nets {
        for e in &net.endpoints {
            if !known.contains(&e.box_id) {
                return Err(LadderBail::ForeignBox { box_id: e.box_id });
            }
        }
    }

    let net_col: HashMap<i64, (Lane, Col)> = (0..n_nets)
        .map(|ni| {
            (
                graph.nets[ni].nid,
                (lane_of[&ni], rank[idx_of[&class_of[ni]]]),
            )
        })
        .collect();

    Ok(LadderModel {
        left,
        right,
        n_lanes,
        n_cols,
        lane_pin: seeds.iter().map(|&(p, _)| p).collect(),
        series,
        bridges,
        net_col,
    })
}

// ============================================================================
// Dump
// ============================================================================

pub fn dump(m: &LadderModel, graph: &McVecGraph) -> String {
    let name = |id: i64| -> String {
        graph
            .boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| format!("#{id}"))
    };
    let mut s = format!(
        "[ladder-model] lanes={} n_cols={} left={} right={}\n",
        m.n_lanes,
        m.n_cols,
        name(m.left),
        name(m.right)
    );
    for lane in 0..m.n_lanes {
        let items: Vec<String> = m
            .series
            .iter()
            .filter(|x| x.lane == lane)
            .map(|x| format!("{} c={}..{}", name(x.box_id), x.from_col, x.to_col))
            .collect();
        s.push_str(&format!(
            "[ladder-model]   series lane={lane} (pin {}): {}\n",
            m.lane_pin.get(lane).copied().unwrap_or(-1),
            if items.is_empty() {
                "(none)".into()
            } else {
                items.join(" | ")
            }
        ));
    }
    let br: Vec<String> = m
        .bridges
        .iter()
        .map(|b| {
            format!(
                "{} col={} (lane{}<->lane{})",
                name(b.box_id),
                b.col,
                b.lane_a,
                b.lane_b
            )
        })
        .collect();
    s.push_str(&format!("[ladder-model]   bridges: {}", br.join(" | ")));
    s
}

// ============================================================================
// Union-Find (iterative; no recursion depth risk)
// ============================================================================

struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Dsu {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Keep the lower root so class ids are deterministic.
            let (hi, lo) = if ra > rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi] = lo;
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// box_id -> net indices it touches (deduped, ascending).
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
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::netdef::{EndpointRef, VizNet};
    use crate::vector::graph::{BoxKind, McVecBox, NetKind, Symbol};

    // ---- builders ---------------------------------------------------------

    fn anchor(id: i64, name: &str, outputs: usize, inputs: usize) -> McVecBox {
        let mut io = IoSummary::new();
        io.outputs = outputs;
        io.inputs = inputs;
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Ic,
            None,
            None,
            2,
            io,
        )
    }

    fn res(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            "RES".into(),
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
        )
    }

    fn cap_bridge(id: i64, name: &str) -> McVecBox {
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
        );
        b.visual_role = Some(VisualRole::BridgePassive);
        b
    }

    /// `eps`: (box_id, pin_id)
    fn net(nid: i64, name: &str, eps: &[(i64, i64)]) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            NetKind::Signal,
            eps.iter()
                .map(|&(b, p)| EndpointRef::new(b, p, &p.to_string()))
                .collect(),
        )
    }

    fn slot<'a>(m: &'a LadderModel, box_id: i64) -> &'a SeriesSlot {
        m.series.iter().find(|s| s.box_id == box_id).unwrap()
    }

    fn rung<'a>(m: &'a LadderModel, box_id: i64) -> &'a BridgeSlot {
        m.bridges.iter().find(|b| b.box_id == box_id).unwrap()
    }

    // ---- the real case ----------------------------------------------------

    /// `[u1.3,u1.6] -> [RES,_] -> CAP' -> [RES,RES] -> CAP' -> [RES,RES] -> [u2.3,u2.6]`
    /// Ids and pin numbers mirror the c07_pins InstTable dump.
    fn c07() -> McVecGraph {
        let mut g = McVecGraph::new(1000, "main".into());
        g.boxes.push(cap_bridge(1001, "@CAP2"));
        g.boxes.push(cap_bridge(1004, "@CAP5"));
        g.boxes.push(res(1007, "@RES1"));
        g.boxes.push(res(1010, "@RES3"));
        g.boxes.push(res(1013, "@RES4"));
        g.boxes.push(res(1016, "@RES6"));
        g.boxes.push(res(1019, "@RES7"));
        g.boxes.push(anchor(1022, "u1", 2, 0));
        g.boxes.push(anchor(1025, "u2", 0, 2));

        g.nets
            .push(net(0, "__net_0", &[(1022, 1023), (1007, 1008)]));
        g.nets.push(net(
            1,
            "__net_1",
            &[(1007, 1009), (1010, 1011), (1001, 1002)],
        ));
        g.nets.push(net(
            2,
            "__net_2",
            &[(1010, 1012), (1016, 1017), (1004, 1005)],
        ));
        g.nets
            .push(net(3, "__net_3", &[(1016, 1018), (1025, 1026)]));
        g.nets.push(net(
            4,
            "__net_4",
            &[(1022, 1024), (1013, 1014), (1001, 1003)],
        ));
        g.nets.push(net(
            5,
            "__net_5",
            &[(1013, 1015), (1019, 1020), (1004, 1006)],
        ));
        g.nets
            .push(net(6, "__net_6", &[(1019, 1021), (1025, 1027)]));
        g
    }

    #[test]
    fn c07_ranks_match_the_hand_drawn_ladder() {
        let m = build_ladder_model(&c07()).expect("c07 is a ladder");

        assert_eq!(m.left, 1022, "u1 drives (outputs=2) -> left anchor");
        assert_eq!(m.right, 1025);
        assert_eq!(m.n_lanes, 2);
        assert_eq!(m.n_cols, 4, "N0=0 C1=1 C2=2 N3=N6=3");
        assert_eq!(m.lane_pin, vec![1023, 1024], "lane order = u1's pin order");

        // lane 0: RES1 | RES3 | RES6 at gaps 0-1, 1-2, 2-3
        assert_eq!(
            slot(&m, 1007),
            &SeriesSlot {
                box_id: 1007,
                lane: 0,
                from_col: 0,
                to_col: 1
            }
        );
        assert_eq!(
            slot(&m, 1010),
            &SeriesSlot {
                box_id: 1010,
                lane: 0,
                from_col: 1,
                to_col: 2
            }
        );
        assert_eq!(
            slot(&m, 1016),
            &SeriesSlot {
                box_id: 1016,
                lane: 0,
                from_col: 2,
                to_col: 3
            }
        );

        // lane 1: `_` then RES4 | RES7 — note RES4 starts at col 1, not col 0.
        assert_eq!(
            slot(&m, 1013),
            &SeriesSlot {
                box_id: 1013,
                lane: 1,
                from_col: 1,
                to_col: 2
            }
        );
        assert_eq!(
            slot(&m, 1019),
            &SeriesSlot {
                box_id: 1019,
                lane: 1,
                from_col: 2,
                to_col: 3
            }
        );

        // ★ the whole point: RES3 and RES4 share a column gap, so the lanes align.
        assert_eq!(slot(&m, 1010).from_col, slot(&m, 1013).from_col);
        assert_eq!(slot(&m, 1016).from_col, slot(&m, 1019).from_col);

        assert_eq!(
            rung(&m, 1001),
            &BridgeSlot {
                box_id: 1001,
                col: 1,
                lane_a: 0,
                lane_b: 1
            }
        );
        assert_eq!(
            rung(&m, 1004),
            &BridgeSlot {
                box_id: 1004,
                col: 2,
                lane_a: 0,
                lane_b: 1
            }
        );

        // `_`: u1.6's net lives at column 1 -> nothing between u1 and the first rung.
        assert_eq!(m.net_col[&4], (1, 1));
        assert_eq!(m.net_col[&0], (0, 0));
        assert_eq!(m.net_col[&3], (0, 3));
        assert_eq!(m.net_col[&6], (1, 3));
    }

    #[test]
    fn c07_is_deterministic() {
        let a = build_ladder_model(&c07()).unwrap();
        let b = build_ladder_model(&c07()).unwrap();
        assert_eq!(a, b);
    }

    // ---- `_` on the other lane -------------------------------------------

    /// ```text
    ///  u1.3 ──────── nA ──────────────── u2.3      (lane 0: no element at all)
    ///                │ CAP
    ///  u1.6 ──RESb── nB ──RESc──── n2 ── u2.6
    /// ```
    /// The rung pins nA to nB's column, so lane 0's wire simply runs long.
    fn underscore_on_lane0() -> McVecGraph {
        let mut g = McVecGraph::new(1, "t".into());
        g.boxes.push(cap_bridge(10, "CAP"));
        g.boxes.push(res(20, "RESb"));
        g.boxes.push(res(21, "RESc"));
        g.boxes.push(anchor(30, "u1", 2, 0));
        g.boxes.push(anchor(31, "u2", 0, 2));

        g.nets
            .push(net(0, "nA", &[(30, 301), (31, 311), (10, 101)])); // lane0, whole width
        g.nets.push(net(1, "n1", &[(30, 302), (20, 201)]));
        g.nets
            .push(net(2, "nB", &[(20, 202), (10, 102), (21, 211)]));
        g.nets.push(net(3, "n2", &[(21, 212), (31, 312)]));
        g
    }

    #[test]
    fn underscore_lane_gets_no_element() {
        let m = build_ladder_model(&underscore_on_lane0()).expect("still a ladder");
        assert_eq!(m.n_lanes, 2);
        assert_eq!(m.n_cols, 3);
        assert!(
            m.series.iter().all(|s| s.lane == 1),
            "lane 0 carries no element"
        );
        assert_eq!(
            slot(&m, 20),
            &SeriesSlot {
                box_id: 20,
                lane: 1,
                from_col: 0,
                to_col: 1
            }
        );
        assert_eq!(
            slot(&m, 21),
            &SeriesSlot {
                box_id: 21,
                lane: 1,
                from_col: 1,
                to_col: 2
            }
        );
        assert_eq!(
            rung(&m, 10),
            &BridgeSlot {
                box_id: 10,
                col: 1,
                lane_a: 0,
                lane_b: 1
            }
        );
        // nA is lane 0's only net: pinned to the rung's column, spans u1 -> u2.
        assert_eq!(m.net_col[&0], (0, 1));
    }

    // ---- bails ------------------------------------------------------------

    #[test]
    fn no_bridge_bails() {
        let mut g = c07();
        for b in &mut g.boxes {
            if b.visual_role == Some(VisualRole::BridgePassive) {
                b.visual_role = None;
            }
        }
        // Without the rungs, RES4's net_4 and RES1's net_1 are no longer tied, and
        // the caps become plain series -> the shape stops being a ladder well before
        // NoBridge would fire; either way it must not silently produce a model.
        assert!(build_ladder_model(&g).is_err());
    }

    #[test]
    fn no_bridge_on_clean_two_lane_bails_with_nobridge() {
        // u1 =2 lanes= u2, one resistor per lane, no cap anywhere.
        let mut g = McVecGraph::new(1, "t".into());
        g.boxes.push(res(20, "Ra"));
        g.boxes.push(res(21, "Rb"));
        g.boxes.push(anchor(30, "u1", 2, 0));
        g.boxes.push(anchor(31, "u2", 0, 2));
        g.nets.push(net(0, "n0", &[(30, 301), (20, 201)]));
        g.nets.push(net(1, "n1", &[(20, 202), (31, 311)]));
        g.nets.push(net(2, "n2", &[(30, 302), (21, 211)]));
        g.nets.push(net(3, "n3", &[(21, 212), (31, 312)]));
        assert_eq!(build_ladder_model(&g), Err(LadderBail::NoBridge));
    }

    #[test]
    fn series_joining_two_lanes_bails() {
        // Same as c07 but @CAP2 lost its bridge mark -> it becomes a series element
        // that ties lane 0 to lane 1.
        let mut g = c07();
        for b in &mut g.boxes {
            if b.id == 1001 {
                b.visual_role = None;
            }
        }
        assert_eq!(
            build_ladder_model(&g),
            Err(LadderBail::SeriesCrossesLane { box_id: 1001 })
        );
    }

    #[test]
    fn bridge_within_one_lane_bails() {
        // Mark @RES3 (lane 0, between net_1 and net_2) as a bridge.
        let mut g = c07();
        for b in &mut g.boxes {
            if b.id == 1010 {
                b.visual_role = Some(VisualRole::BridgePassive);
            }
        }
        match build_ladder_model(&g) {
            Err(LadderBail::BridgeSameLane { box_id, .. }) => assert_eq!(box_id, 1010),
            other => panic!("expected BridgeSameLane, got {other:?}"),
        }
    }

    #[test]
    fn third_anchor_bails() {
        let mut g = c07();
        let mut u3 = anchor(1030, "u3", 1, 1);
        u3.id = 1030;
        g.boxes.push(u3);
        g.nets.push(net(7, "n7", &[(1030, 1031), (1022, 1023)]));
        g.nets.push(net(8, "n8", &[(1030, 1032), (1025, 1026)]));
        assert_eq!(build_ladder_model(&g), Err(LadderBail::AnchorCount(3)));
    }
}
