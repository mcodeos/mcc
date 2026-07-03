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

use std::collections::{BTreeMap, HashMap};

use super::super::model::{McVec, McVecNet};

// ============================================================================
// Internal Data Types
// ============================================================================

/// A single connection pair
#[derive(Debug, Clone)]
pub(crate) struct ConnPair {
    pub left: i64,
    pub right: i64,
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
    // Only one connection pair: Degenerate to 1:1
    if pairs.len() == 1 {
        return McVecNet::new(
            nid,
            net_name,
            vec![McVec::single(pairs[0].left), McVec::single(pairs[0].right)],
        );
    }

    // Count frequency of each ID
    let mut freq: HashMap<i64, usize> = HashMap::new();
    for pair in pairs {
        *freq.entry(pair.left).or_insert(0) += 1;
        *freq.entry(pair.right).or_insert(0) += 1;
    }

    let max_freq = freq.values().cloned().max().unwrap_or(0);

    if max_freq > 1 {
        build_star_topology(nid, net_name, pairs, &freq, max_freq)
    } else {
        build_chain_topology(nid, net_name, pairs)
    }
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
