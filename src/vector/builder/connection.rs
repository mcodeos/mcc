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

use super::super::model::netshape::{GroupRole, LaneRef, NetShape, PairDir};
use super::super::model::{McVec, McVecNet};

// ============================================================================
// Internal Data Types
// ============================================================================

/// A single connection pair
#[derive(Debug, Clone)]
pub(crate) struct ConnPair {
    pub left: i64,
    pub right: i64,
    /// 向量的第几道；标量连接为 None。来自 visit.rs 的 `for k in 0..max_w`。
    pub lane: Option<LaneRef>,
    /// 源码里的箭头方向
    pub dir: PairDir,
    /// 这一段是穿过哪个二端器件产生的
    pub via: Option<i64>,
}

impl ConnPair {
    /// 无 provenance 的构造（等价于改造前的行为）
    pub(crate) fn plain(left: i64, right: i64) -> Self {
        Self {
            left,
            right,
            lane: None,
            dir: PairDir::Undirected,
            via: None,
        }
    }

    /// 带方向的无 provenance 构造
    pub(crate) fn plain_with_dir(left: i64, right: i64, dir: PairDir) -> Self {
        Self {
            left,
            right,
            lane: None,
            dir,
            via: None,
        }
    }

    /// 带 lane 的构造
    pub(crate) fn laned(left: i64, right: i64, lane: LaneRef, dir: PairDir) -> Self {
        Self {
            left,
            right,
            lane: Some(lane),
            dir,
            via: None,
        }
    }

    /// Constructor with lane and via (pass-through device ID)
    pub(crate) fn laned_with_via(
        left: i64,
        right: i64,
        lane: LaneRef,
        dir: PairDir,
        via: Option<i64>,
    ) -> Self {
        Self {
            left,
            right,
            lane: Some(lane),
            dir,
            via,
        }
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
    // ── 有 lane 信息时，直接按源码形状建组，不再靠频次猜 ──
    if pairs.iter().any(|p| p.lane.is_some()) {
        if let Some(net) = build_from_lanes(nid, &net_name, pairs) {
            return net;
        }
        // 建不出来（lane 不完整）→ 落回下面的旧逻辑，不 panic
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

    net
}

/// Build a complete NetShape from pairs and the already-computed vecs.
fn build_net_shape(dir: PairDir, pairs: &[ConnPair], nets: &[McVec]) -> NetShape {
    // groups: one entry per McVec, derived from vec length
    let groups: Vec<GroupRole> = nets
        .iter()
        .map(|v| {
            if v.len() == 1 {
                GroupRole::Scalar
            } else {
                GroupRole::Broadcast(v.len())
            }
        })
        .collect();

    // series_chain: collect all pass-through device IDs from pairs
    let series_chain: Vec<i64> = pairs.iter().filter_map(|p| p.via).collect();

    // lane: take the first lane info from any pair
    let lane: Option<LaneRef> = pairs.iter().find_map(|p| p.lane.clone());

    NetShape {
        groups,
        dir,
        lane,
        series_chain,
        src_pos: None,
    }
}

// ============================================================================
// Lane-aware construction (补丁 3)
// ============================================================================

/// 从 pairs 中计算多数方向
fn majority_dir(pairs: &[ConnPair]) -> PairDir {
    let ltr = pairs.iter().filter(|p| p.dir == PairDir::LtoR).count();
    let rtl = pairs.iter().filter(|p| p.dir == PairDir::RtoL).count();
    if ltr > rtl {
        PairDir::LtoR
    } else if rtl > ltr {
        PairDir::RtoL
    } else {
        PairDir::Undirected
    }
}

/// 有 lane 信息时，直接按源码形状建组，不再靠频次猜。
/// 返回 `None` 时安静地落回旧逻辑，不 panic。
fn build_from_lanes(nid: i64, name: &str, pairs: &[ConnPair]) -> Option<McVecNet> {
    // 同一个 net 里的 pair 应该属于同一道；不同道在 visit.rs 就已经分到
    // 不同的 sub_net_name 了，所以这里期望 lane 唯一。
    let mut lanes: BTreeSet<u16> = BTreeSet::new();
    for p in pairs {
        if let Some(l) = &p.lane {
            lanes.insert(l.index);
        }
    }
    if lanes.len() != 1 {
        return None; // 混了多道 → 交给旧逻辑
    }

    let lane = pairs.iter().find_map(|p| p.lane.clone())?;

    // 方向：多数决
    let ltr = pairs.iter().filter(|p| p.dir == PairDir::LtoR).count();
    let rtl = pairs.iter().filter(|p| p.dir == PairDir::RtoL).count();
    let dir = if ltr > rtl {
        PairDir::LtoR
    } else if rtl > ltr {
        PairDir::RtoL
    } else {
        PairDir::Undirected
    };

    // 端点顺序：沿 pair 的 left→right 走链，不用 order_chain 猜起点
    let chain = order_by_direction(pairs, dir)?;
    let vecs: Vec<McVec> = chain.into_iter().map(McVec::single).collect();

    let shape = NetShape {
        groups: vec![super::super::model::netshape::GroupRole::Scalar; vecs.len()],
        dir,
        lane: Some(lane),
        series_chain: pairs.iter().filter_map(|p| p.via).collect(),
        src_pos: None,
    };
    Some(McVecNet::with_shape(nid, name.to_string(), vecs, shape))
}

/// 沿有向边排链：从第一个 left 开始，按 left→right 走链。
fn order_by_direction(pairs: &[ConnPair], _dir: PairDir) -> Option<Vec<i64>> {
    if pairs.is_empty() {
        return Some(vec![]);
    }
    if pairs.len() == 1 {
        return Some(vec![pairs[0].left, pairs[0].right]);
    }

    // 构建邻接表
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for pair in pairs {
        adj.entry(pair.left).or_default().push(pair.right);
        adj.entry(pair.right).or_default().push(pair.left);
    }

    // 找度数为 1 的节点作为起点
    let start = adj
        .iter()
        .find(|(_, neighbors)| neighbors.len() == 1)
        .map(|(&id, _)| id)?;

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

    // Find degree-1 node (start of chain)
    let start = adj
        .iter()
        .find(|(_, neighbors)| neighbors.len() == 1)
        .map(|(&id, _)| id);

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
