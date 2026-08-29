// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Topological analysis: `Vec<ConnPair>` → `McVecNet`
//!
//! Given a set of connection pairs (all belonging to the same net_name), analyze their topological structure:
//! - **Star**: A hub (appears >1 times) → hub vs leaves
//! - **Chain**: All points appear exactly once → linear connection
//! - **Degenerate**: Single pair → direct 1:1
//!
//! ## Typical Usage (Called by [`super::visit::McVecBuilder`])
//! ```ignore
//! let pairs = vec![
//!     ConnPair { left: 1, right: 2 },
//!     ConnPair { left: 1, right: 3 },
//! ];
//! let net = merge_pairs_to_vecnet(42, "VCC".into(), &pairs);
//! // → Star topology: McVec([1]) <-> McVec([2, 3])
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::super::model::netshape::{GroupRole, LaneRef, NetShape};
use super::super::model::{McVec, McVecNet};
use crate::semantic::common::{parallel_anchor, ConnDir, ConnOp};
use crate::vector::model::trunk::TrunkCtx;

// ============================================================================
// Internal Data Types
// ============================================================================

/// A single connection pair
#[derive(Debug, Clone)]
pub(crate) struct ConnPair {
    pub left: i64,
    pub right: i64,
    /// Which lane of the vector; None for scalar connections. From the `for k in 0..max_w` loop in visit.rs.
    pub lane: Option<LaneRef>,
    /// Arrow direction in the source
    pub dir: ConnDir,
    /// Connection operator that produced this pair (`Series` for `-`/`->`/`<-`,
    /// `Parallel` for `+`); `None` when unknown (projection trunks). Copied from
    /// `ConnectionInst.op` in visit.rs and surfaced on `NetShape.op`.
    pub op: Option<ConnOp>,
    /// Which two-terminal device this segment passes through
    pub via: Option<i64>,
    /// ★ P9-A2: source span for traceability
    pub source_span: Option<crate::semantic::common::SourcePos>,
    /// ★ §8.9.6: structured group context (group name, lane member, coarse
    /// kind) copied from `ConnectionInst.trunk`.
    pub trunk: Option<TrunkCtx>,
}

impl ConnPair {
    /// Construction with direction but no provenance
    pub(crate) fn plain_with_dir(left: i64, right: i64, dir: ConnDir) -> Self {
        Self {
            left,
            right,
            lane: None,
            dir,
            via: None,
            source_span: None,
            trunk: None,
            op: None,
        }
    }

    /// Construction with a lane
    pub(crate) fn laned(left: i64, right: i64, lane: LaneRef, dir: ConnDir) -> Self {
        Self {
            left,
            right,
            lane: Some(lane),
            dir,
            via: None,
            source_span: None,
            trunk: None,
            op: None,
        }
    }

    /// Constructor with lane and via (pass-through device ID)
    pub(crate) fn laned_with_via(
        left: i64,
        right: i64,
        lane: LaneRef,
        dir: ConnDir,
        via: Option<i64>,
    ) -> Self {
        Self {
            left,
            right,
            lane: Some(lane),
            dir,
            via,
            source_span: None,
            trunk: None,
            op: None,
        }
    }

    /// ★ §8.9.6: Set provenance metadata (source_span + structured group)
    pub(crate) fn with_meta(
        mut self,
        source_span: Option<crate::semantic::common::SourcePos>,
        trunk: Option<TrunkCtx>,
    ) -> Self {
        self.source_span = source_span;
        self.trunk = trunk;
        self
    }

    /// Set the connection operator carried from `ConnectionInst.op`.
    /// Production always passes the op through `plain`; this builder is
    /// exercised only by the `plain_with_dir(...).with_op(...)` tests.
    #[allow(dead_code)]
    pub(crate) fn with_op(mut self, op: ConnOp) -> Self {
        self.op = Some(op);
        self
    }
}

/// `net_name → connection pair list` grouping
pub(crate) type NetGroupMap = BTreeMap<String, Vec<ConnPair>>;

// ============================================================================
// Public API: merge_pairs_to_vecnet
// ============================================================================

/// Merge all connection pairs for a given net_name into a single `McVecNet`
///
/// ## Topology Types
/// - **Star**: A hub (appears >1 times) → hub vs leaves
/// - **Chain**: All points appear exactly once → linear connection
/// - **Degenerate**: Single pair → direct 1:1
pub(crate) fn merge_pairs_to_vecnet(nid: i64, net_name: String, pairs: &[ConnPair]) -> McVecNet {
    // ── Extract provenance from first pair ──
    let source_span = pairs.first().and_then(|p| p.source_span.clone());
    let trunk = pairs.first().and_then(|p| p.trunk.clone());

    // ── With lane info, build groups directly from the source shape instead of guessing by frequency ──
    if pairs.iter().any(|p| p.lane.is_some()) {
        if let Some(mut net) = build_from_lanes(nid, &net_name, pairs) {
            net.source_span = source_span;
            net.trunk = trunk;
            return net;
        }
        // Couldn't build (lane incomplete) → fall back to the legacy logic below, no panic
    }

    // ── ★ B4: source-operator-driven parallel topology ──────────────────
    // A parallel `+` net is a flat set of scalar operands all at one level
    // (an equipotential merge). The old frequency guess (`max_freq > 1 → star`)
    // collapsed it into `[hub, leaves]` — misreading the parallel merge as a
    // 1→N broadcast and giving the whole net a single direction
    // (connection_type() misjudging equipotential points). Build it directly
    // from the source operator instead: one scalar McVec per distinct endpoint
    // in source order, all-Scalar groups, left-main anchor.
    if pairs.iter().any(|p| p.op == Some(ConnOp::Parallel)) {
        let order = collect_unique_ordered(pairs);
        let vecs: Vec<McVec> = order.iter().map(|&id| McVec::single(id)).collect();
        let shape = NetShape {
            groups: vec![GroupRole::Scalar; vecs.len()],
            dir: majority_dir(pairs),
            lane: None,
            series_chain: pairs.iter().filter_map(|p| p.via).collect(),
            op: Some(ConnOp::Parallel),
            anchor: parallel_anchor(&order),
            order,
        };
        let mut net = McVecNet::with_shape(nid, net_name, vecs, shape);
        net.source_span = source_span;
        net.trunk = trunk;
        return net;
    }

    let dir = majority_dir(pairs);

    // Only one connection pair: Degenerate to 1:1
    if pairs.len() == 1 {
        let mut net = McVecNet::new(
            nid,
            net_name,
            vec![McVec::single(pairs[0].left), McVec::single(pairs[0].right)],
        );
        net.shape = Some(build_net_shape(dir, pairs, &net.nets));
        net.source_span = source_span;
        net.trunk = trunk;
        return net;
    }

    // Count frequency of each ID
    let mut freq: HashMap<i64, usize> = HashMap::new();
    for pair in pairs {
        *freq.entry(pair.left).or_insert(0) += 1;
        *freq.entry(pair.right).or_insert(0) += 1;
    }

    let max_freq = freq.values().cloned().max().unwrap_or(0);

    let mut net = if max_freq > 1 {
        build_star_topology(nid, net_name, pairs, &freq, max_freq)
    } else {
        build_chain_topology(nid, net_name, pairs)
    };

    // ★ Fill NetShape for all non-lane branches
    if net.shape.is_none() {
        net.shape = Some(build_net_shape(dir, pairs, &net.nets));
    }

    net.source_span = source_span;
    net.trunk = trunk;

    net
}

/// Build a complete NetShape from pairs and the already-computed vecs.
fn build_net_shape(dir: ConnDir, pairs: &[ConnPair], nets: &[McVec]) -> NetShape {
    // ── ★ B3: groups from the source shape, not the vec length ───────────
    // The old `v.len()==1 → Scalar else Broadcast(v.len())` was a projection
    // of the *result* — the post-topology-merge vecs — so any multi-point vec
    // read as a 1→N broadcast even when the source was a parallel merge or a
    // lane net. The source shape is the lane the connection belongs to: a lane
    // net carries its width in the lane index, so every group is one scalar
    // point. Mirroring `build_from_lanes` (all-Scalar groups) keeps the
    // mixed-lane fallback straight instead of re-deriving a width off the
    // merged vec (impl-plan.md §3.3.3 item 3 / §3.4 step ④).
    let is_lane_net = pairs.iter().any(|p| p.lane.is_some());
    let groups: Vec<GroupRole> = if is_lane_net {
        vec![GroupRole::Scalar; nets.len()]
    } else {
        nets.iter()
            .map(|v| {
                if v.len() == 1 {
                    GroupRole::Scalar
                } else {
                    GroupRole::Broadcast(v.len())
                }
            })
            .collect()
    };

    // series_chain: collect all pass-through device IDs from pairs
    let series_chain: Vec<i64> = pairs.iter().filter_map(|p| p.via).collect();

    // lane: take the first lane info from any pair
    let mut lane: Option<LaneRef> = pairs.iter().find_map(|p| p.lane.clone());
    // ── §8.9.6.7: align LaneRef.name with the AST-layer TrunkCtx.member ──
    // The lane member name is a shape derivation (bracket split in visit.rs);
    // the group member is the connection identity decided at the AST layer.
    // When the lane carries no name, take the structured member as the
    // authority so the two sources never disagree.
    if let Some(ref mut l) = lane {
        if l.name.is_none() {
            l.name = pairs
                .first()
                .and_then(|p| p.trunk.as_ref())
                .and_then(|g| g.member.clone());
        }
    }

    // op: the source operator (series `-`/`->` or parallel `+`), taken from
    // the first pair that carries one
    let op: Option<ConnOp> = pairs.iter().find_map(|p| p.op);

    // order: source-order endpoint sequence (deduplicated)
    let order = collect_unique_ordered(pairs);

    // anchor: §8.9.4 step 4 — the left main of a parallel `+` net is its first
    // ordered endpoint (shared `parallel_anchor` rule; None for series nets)
    let anchor = if op == Some(ConnOp::Parallel) {
        parallel_anchor(&order)
    } else {
        None
    };

    NetShape {
        groups,
        dir,
        lane,
        series_chain,
        op,
        anchor,
        order,
    }
}

// ============================================================================
// Lane-aware construction (patch 3)
// ============================================================================

/// Compute the majority direction from pairs
fn majority_dir(pairs: &[ConnPair]) -> ConnDir {
    let ltr = pairs.iter().filter(|p| p.dir == ConnDir::LtoR).count();
    let rtl = pairs.iter().filter(|p| p.dir == ConnDir::RtoL).count();
    if ltr > rtl {
        ConnDir::LtoR
    } else if rtl > ltr {
        ConnDir::RtoL
    } else {
        ConnDir::Undirected
    }
}

/// With lane info, build groups directly from the source shape instead of guessing by frequency.
/// Returns `None` to silently fall back to the legacy logic, no panic.
fn build_from_lanes(nid: i64, name: &str, pairs: &[ConnPair]) -> Option<McVecNet> {
    // Pairs in the same net should belong to the same lane; different lanes were already
    // split into different sub_net_names in visit.rs, so the lane is expected to be unique here.
    let mut lanes: BTreeSet<u16> = BTreeSet::new();
    for p in pairs {
        if let Some(l) = &p.lane {
            lanes.insert(l.index);
        }
    }
    if lanes.len() != 1 {
        return None; // mixed lanes → hand over to legacy logic
    }

    let lane = pairs.iter().find_map(|p| p.lane.clone())?;

    // Direction: majority vote
    let ltr = pairs.iter().filter(|p| p.dir == ConnDir::LtoR).count();
    let rtl = pairs.iter().filter(|p| p.dir == ConnDir::RtoL).count();
    let dir = if ltr > rtl {
        ConnDir::LtoR
    } else if rtl > ltr {
        ConnDir::RtoL
    } else {
        ConnDir::Undirected
    };

    // Endpoint order: walk the chain along each pair's left→right, no order_chain start guessing
    let chain = order_by_direction(pairs, dir)?;
    let vecs: Vec<McVec> = chain.into_iter().map(McVec::single).collect();

    let order = collect_unique_ordered(pairs);
    let op = pairs.iter().find_map(|p| p.op);
    let shape = NetShape {
        groups: vec![super::super::model::netshape::GroupRole::Scalar; vecs.len()],
        dir,
        lane: Some(lane),
        series_chain: pairs.iter().filter_map(|p| p.via).collect(),
        op,
        // §8.9.4 step 4: parallel left main = first ordered endpoint
        anchor: if op == Some(ConnOp::Parallel) {
            parallel_anchor(&order)
        } else {
            None
        },
        order,
    };
    Some(McVecNet::with_shape(nid, name.to_string(), vecs, shape))
}

/// ★ P7-4 [DET]: pick the chain start among degree-1 nodes.
///
/// A group's pairs may form several disconnected chains (same-name net merge,
/// e.g. a `GND` group holding both the flash decoupling chain and the mic
/// supply chain), so degree-1 nodes are not unique. The old
/// `adj.iter().find(...)` followed HashMap iteration order, so which
/// component got picked as the start was random: only one component was
/// walked and the other points were dropped entirely (root cause of
/// GND/VCC group members flipping across renders). Taking the **smallest
/// id** among degree-1 nodes keeps both the start and the content stable.
fn pick_chain_start(adj: &HashMap<i64, Vec<i64>>) -> Option<i64> {
    adj.iter()
        .filter(|(_, neighbors)| neighbors.len() == 1)
        .map(|(&id, _)| id)
        .min()
}

/// Order the chain along directed edges: start from the first left, walk left→right.
fn order_by_direction(pairs: &[ConnPair], _dir: ConnDir) -> Option<Vec<i64>> {
    if pairs.is_empty() {
        return Some(vec![]);
    }
    if pairs.len() == 1 {
        return Some(vec![pairs[0].left, pairs[0].right]);
    }

    // Build an adjacency list
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for pair in pairs {
        adj.entry(pair.left).or_default().push(pair.right);
        adj.entry(pair.right).or_default().push(pair.left);
    }

    // Find a degree-1 node to use as the start (smallest id; see pick_chain_start)
    let start = pick_chain_start(&adj)?;

    let mut chain = vec![start];
    let mut visited: std::collections::HashSet<i64> = std::collections::HashSet::new();
    visited.insert(start);

    let mut current = start;
    loop {
        let neighbors = adj.get(&current)?;
        match neighbors.iter().find(|&&n| !visited.contains(&n)) {
            Some(&n) => {
                chain.push(n);
                visited.insert(n);
                current = n;
            }
            None => break,
        }
    }

    // ★ P7-4 [DET]: a group may contain more than one connected component (or a
    // star) —— a single chain cannot cover it. Append all unvisited nodes in
    // first-appearance order, guaranteeing **no dropped points** (previously
    // the VCC star kept only 3/6 points and the GND double chain kept only
    // one component).
    let mut remaining = collect_unique_ordered(pairs);
    remaining.retain(|id| !visited.contains(id));
    chain.extend(remaining);

    Some(chain)
}

// ============================================================================
// Topology Construction
// ============================================================================

/// Build star topology
///
/// ```text
///       ┌── leaf1
/// hub ──┼── leaf2
///       └── leaf3
/// ```
/// → `McVecNet { nets: [McVec([hub]), McVec([leaf1, leaf2, leaf3])] }`
fn build_star_topology(
    nid: i64,
    net_name: String,
    pairs: &[ConnPair],
    freq: &HashMap<i64, usize>,
    max_freq: usize,
) -> McVecNet {
    let mut hubs: Vec<i64> = freq
        .iter()
        .filter(|(_, &f)| f == max_freq)
        .map(|(&id, _)| id)
        .collect();
    hubs.sort();

    let mut leaves: Vec<i64> = Vec::new();
    for pair in pairs {
        for &id in &[pair.left, pair.right] {
            if !hubs.contains(&id) && !leaves.contains(&id) {
                leaves.push(id);
            }
        }
    }

    // ★ FIX (star leaf-drop): single hub + N leaves is a legitimate 1:N star.
    //   The old `hubs.len()==1` / `leaves.len()==1` branches returned only the hub,
    //   dropping every leaf → rail/divider nets collapsed, passives orphaned.
    //   Only "no leaves at all" degenerates to a hub chain; everything else is a star.
    if leaves.is_empty() {
        let vecs: Vec<McVec> = hubs.into_iter().map(McVec::single).collect();
        return McVecNet::new(nid, net_name, vecs);
    }

    McVecNet::new(nid, net_name, vec![McVec::new(hubs), McVec::new(leaves)])
}

/// Build chain topology
///
/// ```text
/// A ── B ── C
/// ```
/// → `McVecNet { nets: [McVec([A]), McVec([B]), McVec([C])] }`
fn build_chain_topology(nid: i64, net_name: String, pairs: &[ConnPair]) -> McVecNet {
    let chain = order_chain(pairs);
    let vecs: Vec<McVec> = chain.into_iter().map(McVec::single).collect();
    McVecNet::new(nid, net_name, vecs)
}

/// Order connection pairs into a sorted chain
///
/// Input: `[(A,B), (B,C)]` (may be out of order) → Output: `[A, B, C]` (sorted)
fn order_chain(pairs: &[ConnPair]) -> Vec<i64> {
    if pairs.is_empty() {
        return vec![];
    }
    if pairs.len() == 1 {
        return vec![pairs[0].left, pairs[0].right];
    }

    // Build adjacency list from pairs
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for pair in pairs {
        adj.entry(pair.left).or_default().push(pair.right);
        adj.entry(pair.right).or_default().push(pair.left);
    }

    // Find degree-1 node (start of chain) — ★ P7-4 [DET]: the smallest id, so
    // HashMap iteration order cannot decide chain direction (direction affects
    // McVec.nets order → render order).
    let start = pick_chain_start(&adj);

    let start = match start {
        Some(s) => s,
        None => {
            // No degree-1 node (ring or complex graph): fallback to ordered collection
            return collect_unique_ordered(pairs);
        }
    };

    // Traverse from start node
    let mut chain = vec![start];
    let mut visited = std::collections::HashSet::new();
    visited.insert(start);

    let mut current = start;
    loop {
        let neighbors = match adj.get(&current) {
            Some(n) => n,
            None => break,
        };
        match neighbors.iter().find(|&&n| !visited.contains(&n)) {
            Some(&n) => {
                chain.push(n);
                visited.insert(n);
                current = n;
            }
            None => break,
        }
    }

    // Add remaining nodes to chain
    let mut remaining = collect_unique_ordered(pairs);
    remaining.retain(|id| !visited.contains(id));
    chain.extend(remaining);

    chain
}

/// Collect all unique IDs in pairs in order of first appearance
fn collect_unique_ordered(pairs: &[ConnPair]) -> Vec<i64> {
    let mut result = Vec::new();
    for pair in pairs {
        if !result.contains(&pair.left) {
            result.push(pair.left);
        }
        if !result.contains(&pair.right) {
            result.push(pair.right);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §8.9.4: the series operator written in the source (`-`/`->`) must be
    /// carried through ConnPair → NetShape.op, and the source-order endpoint
    /// sequence must land in NetShape.order.
    #[test]
    fn series_shape_carries_op_and_order() {
        let pairs = vec![
            ConnPair::plain_with_dir(1, 2, ConnDir::LtoR).with_op(ConnOp::Series),
            ConnPair::plain_with_dir(2, 3, ConnDir::LtoR).with_op(ConnOp::Series),
        ];
        let nets = vec![McVec::single(1), McVec::single(2), McVec::single(3)];
        let shape = build_net_shape(ConnDir::LtoR, &pairs, &nets);
        assert_eq!(shape.op, Some(ConnOp::Series));
        assert_eq!(shape.order, vec![1, 2, 3]);
        assert!(shape.is_informative());
        assert_eq!(shape.anchor, None);
    }

    /// §8.9.4: a parallel `+` net keeps `Parallel`, the left-aligned merge
    /// order (the anchor `10` appears once, deduplicated at the front) and the
    /// left-main anchor (`10`).
    #[test]
    fn parallel_shape_keeps_op() {
        let pairs = vec![
            ConnPair::plain_with_dir(10, 11, ConnDir::Undirected).with_op(ConnOp::Parallel),
            ConnPair::plain_with_dir(10, 12, ConnDir::Undirected).with_op(ConnOp::Parallel),
        ];
        let nets = vec![McVec::single(10), McVec::single(11), McVec::single(12)];
        let shape = build_net_shape(ConnDir::Undirected, &pairs, &nets);
        assert_eq!(shape.op, Some(ConnOp::Parallel));
        assert_eq!(shape.order, vec![10, 11, 12]);
        assert_eq!(shape.anchor, Some(10));
        assert!(shape.is_informative());
    }

    /// B4: a parallel `+` net merges into a flat set of scalar operands, not a
    /// freq-guessed `[hub, leaves]` star. `A + B + C` = (A,B),(B,C) has `B` at
    /// degree 2 (the old guess → star); the source operator must win: one
    /// scalar McVec per distinct endpoint, all-Scalar groups, `Parallel` op.
    #[test]
    fn parallel_net_builds_flat_from_source_op() {
        let pairs = vec![
            ConnPair::plain_with_dir(1, 2, ConnDir::Undirected).with_op(ConnOp::Parallel),
            ConnPair::plain_with_dir(2, 3, ConnDir::Undirected).with_op(ConnOp::Parallel),
        ];
        let net = merge_pairs_to_vecnet(42, "PAR".to_string(), &pairs);
        assert_eq!(net.nets.len(), 3, "one McVec per operand, not a star pair");
        assert!(net.nets.iter().all(|v| v.len() == 1), "all operands scalar");
        let shape = net.shape.as_ref().expect("shape filled");
        assert_eq!(shape.op, Some(ConnOp::Parallel));
        assert_eq!(shape.order, vec![1, 2, 3]);
        assert_eq!(shape.anchor, Some(1));
        assert!(
            shape.groups.iter().all(|g| *g == GroupRole::Scalar),
            "parallel merge must not read as a Broadcast; got {:?}",
            shape.groups
        );
    }

    /// B4: a genuine series broadcast (hub at degree >1, no `+` operator) still
    /// builds a star — the source shape is a 1→N broadcast, not a parallel
    /// merge, so the freq path is the correct topology here.
    #[test]
    fn series_broadcast_still_builds_star() {
        let pairs = vec![
            ConnPair::plain_with_dir(7, 8, ConnDir::LtoR).with_op(ConnOp::Series),
            ConnPair::plain_with_dir(7, 9, ConnDir::LtoR).with_op(ConnOp::Series),
        ];
        let net = merge_pairs_to_vecnet(43, "GND".to_string(), &pairs);
        assert_eq!(net.nets.len(), 2, "hub + leaves star");
        let shape = net.shape.as_ref().expect("shape filled");
        assert_eq!(shape.op, Some(ConnOp::Series));
        assert_eq!(
            shape.groups,
            vec![GroupRole::Scalar, GroupRole::Broadcast(2)]
        );
    }

    /// B3: a lane net's groups are all Scalar (width lives in the lane index),
    /// even when the merged vec happens to be multi-point in the mixed-lane
    /// fallback — never re-derive a Broadcast width off the vec length.
    #[test]
    fn lane_net_groups_are_scalar_not_vec_length_projection() {
        let pairs = vec![ConnPair::laned(
            5,
            6,
            LaneRef::new(0, Some("TX".into())),
            ConnDir::LtoR,
        )];
        let nets = vec![McVec::single(5), McVec::single(6)];
        let shape = build_net_shape(ConnDir::LtoR, &pairs, &nets);
        assert_eq!(shape.groups, vec![GroupRole::Scalar, GroupRole::Scalar]);
        assert!(shape.is_bus_lane());
    }
}
