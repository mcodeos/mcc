// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Quotient — quotient graph
//!
//! Compresses connections between devices in a zone into hyperedges for the searcher.
//!
//! ## Algorithm
//! 1. nodes = union of terminal ICs of all bands
//! 2. One edge per band (duplicate edges allowed)
//! 3. Direction priority: module ports > pin io_type > Neutral
//! 4. Rail nets produce no edges
//!
//! ## Acceptance (M3-1)
//! - t4_current.mc: nodes=5, edges=6
//! - All edges have non-zero w/h

use std::collections::BTreeMap;

use crate::vector::graph::netdef::IoDirection;
use crate::vector::graph::{BoxKind, McVecGraph, NetRole};

// ============================================================================
// Data structures
// ============================================================================

/// Quotient graph node ID (corresponds to box_id)
pub type NodeId = i64;

/// Edge direction preference (input to the searcher, not a hard constraint)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    /// Left to right
    LeftToRight,
    /// Right to left
    RightToLeft,
    /// Undetermined
    Neutral,
}

/// An edge in the quotient graph (a connection between two terminal ICs)
#[derive(Debug, Clone)]
pub struct QEdge {
    /// Source node
    pub src: NodeId,
    /// Destination node
    pub dst: NodeId,
    /// Direction preference
    pub prefer: Direction,
    /// Pixel width of the band (SP's COL_W=120, ladder's col_step=224)
    pub w: f64,
    /// Pixel height of the band (same terminal count)
    pub h: f64,
    /// Edge label (for debugging)
    pub label: String,
}

/// Quotient graph
#[derive(Debug, Clone)]
pub struct QuotientGraph {
    /// Node ID list (sorted)
    pub nodes: Vec<NodeId>,
    /// Edge list
    pub edges: Vec<QEdge>,
    /// Node ID → display label
    pub labels: BTreeMap<NodeId, String>,
}

// ============================================================================
// Constants
// ============================================================================

/// SP band column width (from islands.rs COL_W)
pub const SP_COL_W: f64 = 120.0;
/// Row height
pub const ROW_H: f64 = 60.0;

// ============================================================================
// Build
// ============================================================================

impl QuotientGraph {
    /// Build the quotient graph from a graph
    ///
    /// Algorithm:
    /// 1. Collect SubModule nodes as quotient graph nodes
    /// 2. For each signal net (NetRole ≠ Rail), find its IC endpoints
    /// 3. Produce one edge per pair of IC endpoints
    /// 4. Direction determined by io_type
    pub fn build(graph: &McVecGraph) -> Self {
        // ── 1. Collect IC nodes ──
        let mut nodes: Vec<NodeId> = Vec::new();
        let mut labels: BTreeMap<NodeId, String> = BTreeMap::new();

        for b in &graph.boxes {
            if is_ic_box(&b.kind) {
                nodes.push(b.id);
                labels.insert(b.id, b.display_label().to_string());
            }
        }
        nodes.sort();

        // ── 2. Build edges ──
        let mut edges: Vec<QEdge> = Vec::new();

        for net in &graph.nets {
            // Skip Rail nets
            if matches!(net.role, NetRole::Rail { .. }) {
                continue;
            }

            // Find the IC endpoints on this net
            let ic_eps: Vec<_> = net
                .endpoints
                .iter()
                .filter(|ep| nodes.contains(&ep.box_id))
                .collect();

            if ic_eps.len() < 2 {
                continue;
            }

            // One edge per pair of IC endpoints
            for i in 0..ic_eps.len() {
                for j in (i + 1)..ic_eps.len() {
                    let a = ic_eps[i];
                    let b = ic_eps[j];

                    let (src, dst, prefer) =
                        resolve_direction(a.box_id, b.box_id, a.io_type, b.io_type);

                    edges.push(QEdge {
                        src,
                        dst,
                        prefer,
                        w: SP_COL_W,
                        h: ROW_H,
                        label: net.name.clone(),
                    });
                }
            }
        }

        QuotientGraph {
            nodes,
            edges,
            labels,
        }
    }

    /// Build the quotient graph from a graph, considering only nodes within the given box_ids
    pub fn build_for_ids(graph: &McVecGraph, box_ids: &[i64]) -> Self {
        let id_set: std::collections::HashSet<i64> = box_ids.iter().copied().collect();

        // ── 1. Collect IC nodes ──
        let mut nodes: Vec<NodeId> = Vec::new();
        let mut labels: BTreeMap<NodeId, String> = BTreeMap::new();

        for b in &graph.boxes {
            if is_ic_box(&b.kind) && id_set.contains(&b.id) {
                nodes.push(b.id);
                labels.insert(b.id, b.display_label().to_string());
            }
        }
        nodes.sort();

        // ── 2. Build edges ──
        let mut edges: Vec<QEdge> = Vec::new();

        for net in &graph.nets {
            if matches!(net.role, NetRole::Rail { .. }) {
                continue;
            }

            let ic_eps: Vec<_> = net
                .endpoints
                .iter()
                .filter(|ep| nodes.contains(&ep.box_id))
                .collect();

            if ic_eps.len() < 2 {
                continue;
            }

            for i in 0..ic_eps.len() {
                for j in (i + 1)..ic_eps.len() {
                    let a = ic_eps[i];
                    let b = ic_eps[j];

                    let (src, dst, prefer) =
                        resolve_direction(a.box_id, b.box_id, a.io_type, b.io_type);

                    edges.push(QEdge {
                        src,
                        dst,
                        prefer,
                        w: SP_COL_W,
                        h: ROW_H,
                        label: net.name.clone(),
                    });
                }
            }
        }

        QuotientGraph {
            nodes,
            edges,
            labels,
        }
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Whether a box is an IC node (SubModule or MultiPin)
fn is_ic_box(kind: &BoxKind) -> bool {
    matches!(kind, BoxKind::SubModule | BoxKind::MultiPin)
}

/// Determine direction preference from two endpoints' io_type
///
/// Priority:
/// 1. Output → Input → LeftToRight
/// 2. Input → Output → RightToLeft
/// 3. Otherwise → Neutral (sorted by id for determinism)
fn resolve_direction(
    a_id: i64,
    b_id: i64,
    a_io: IoDirection,
    b_io: IoDirection,
) -> (i64, i64, Direction) {
    let a_is_output = matches!(a_io, IoDirection::Output);
    let b_is_output = matches!(b_io, IoDirection::Output);
    let a_is_input = matches!(a_io, IoDirection::Input);
    let b_is_input = matches!(b_io, IoDirection::Input);

    // Output → Input
    if a_is_output && b_is_input {
        return (a_id, b_id, Direction::LeftToRight);
    }
    if b_is_output && a_is_input {
        return (b_id, a_id, Direction::LeftToRight);
    }

    // Both Output or both Input → Neutral
    // Determinism: sort by id
    if a_id < b_id {
        (a_id, b_id, Direction::Neutral)
    } else {
        (b_id, a_id, Direction::Neutral)
    }
}

// ============================================================================
// Test helpers
// ============================================================================

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;
    use crate::vector::graph::boxdef::McVecBox;
    use crate::vector::graph::netdef::IoDirection;
    use crate::vector::graph::{BoxKind, EndpointRef, IoSummary, NetKind, VizNet};

    /// Build a multi-terminal IC box
    pub fn mk_ic(id: i64, name: &str, pin_count: usize) -> McVecBox {
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

    /// Build a passive device (R/C/L)
    pub fn mk_passive(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::TwoPin,
            crate::vector::graph::Symbol::Unknown,
            None,
            None,
            2,
            IoSummary::new(),
            format!("main.{}", name),
            Vec::new(),
        )
    }

    /// Build a signal net (with given endpoints)
    pub fn mk_signal_net(id: i64, name: &str, endpoints: Vec<EndpointRef>) -> VizNet {
        VizNet::new(id, name.into(), NetKind::Signal, NetRole::Signal, endpoints)
    }

    /// Build an endpoint (with direction)
    pub fn mk_ep(box_id: i64, pin_id: i64, name: &str, io: IoDirection) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, name, io)
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;
    use crate::vector::graph::netdef::IoDirection;
    use crate::vector::graph::{EndpointRef, NetKind, NetRole, VizNet};

    /// ── t4_current: 5 nodes 6 edges, optimal solution [u4][u2][u1][u3,u5] ──
    ///
    /// Topology:
    ///   u1(mcu) ←→ u2(ldo_in) ←→ u4(ldo_out) ←→ u3(spk) ←→ u5(flash)
    ///   All edges are Neutral (IN↔IN, OUT↔OUT), relying purely on orientation anchors + lexicographic order.
    fn make_t4_current() -> McVecGraph {
        let mut g = McVecGraph::new(0, "main".into());
        // 5 ICs
        g.boxes.push(mk_ic(1, "u1_mcu", 4));
        g.boxes.push(mk_ic(2, "u2_ldo_in", 3));
        g.boxes.push(mk_ic(3, "u3_spk", 3));
        g.boxes.push(mk_ic(4, "u4_ldo_out", 3));
        g.boxes.push(mk_ic(5, "u5_flash", 3));

        // 6 edges (all Neutral direction)
        g.nets.push(mk_signal_net(
            10,
            "u1_u2",
            vec![
                mk_ep(1, 11, "OUT", IoDirection::Output),
                mk_ep(2, 21, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            11,
            "u2_u4",
            vec![
                mk_ep(2, 22, "OUT", IoDirection::Output),
                mk_ep(4, 41, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            12,
            "u4_u3",
            vec![
                mk_ep(4, 42, "OUT", IoDirection::Output),
                mk_ep(3, 31, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            13,
            "u3_u5",
            vec![
                mk_ep(3, 32, "OUT", IoDirection::Output),
                mk_ep(5, 51, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            14,
            "u1_u4",
            vec![
                mk_ep(1, 12, "CTRL", IoDirection::Output),
                mk_ep(4, 43, "CTRL", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            15,
            "u1_u5",
            vec![
                mk_ep(1, 13, "CLK", IoDirection::Output),
                mk_ep(5, 52, "CLK", IoDirection::Input),
            ],
        ));
        g
    }

    #[test]
    fn quotient_t4_current() {
        let g = make_t4_current();
        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 5, "t4_current: 5 nodes");
        assert_eq!(q.edges.len(), 6, "t4_current: 6 edges");
        // No edge should have zero width/height
        for e in &q.edges {
            assert!(e.w > 0.0, "edge {:?} has zero width", e.label);
            assert!(e.h > 0.0, "edge {:?} has zero height", e.label);
        }
    }

    /// ── t2_cycle: 2-node cycle → backward=1 ──
    #[test]
    fn quotient_t2_cycle() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        // Bidirectional connection forms a cycle
        g.nets.push(mk_signal_net(
            10,
            "u1_u2",
            vec![
                mk_ep(1, 11, "OUT", IoDirection::Output),
                mk_ep(2, 21, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            11,
            "u2_u1",
            vec![
                mk_ep(2, 22, "OUT", IoDirection::Output),
                mk_ep(1, 12, "IN", IoDirection::Input),
            ],
        ));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 2);
        assert_eq!(q.edges.len(), 2);
    }

    /// ── t3_cycle: 3-node cycle → backward=1 ──
    #[test]
    fn quotient_t3_cycle() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        // u1→u2→u3→u1 cycle
        g.nets.push(mk_signal_net(
            10,
            "u1_u2",
            vec![
                mk_ep(1, 11, "OUT", IoDirection::Output),
                mk_ep(2, 21, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            11,
            "u2_u3",
            vec![
                mk_ep(2, 22, "OUT", IoDirection::Output),
                mk_ep(3, 31, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            12,
            "u3_u1",
            vec![
                mk_ep(3, 32, "OUT", IoDirection::Output),
                mk_ep(1, 12, "IN", IoDirection::Input),
            ],
        ));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 3);
        assert_eq!(q.edges.len(), 3);
    }

    /// ── t1_chain: single edge → [u1][u2] ──
    #[test]
    fn quotient_t1_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(
            10,
            "u1_u2",
            vec![
                mk_ep(1, 11, "OUT", IoDirection::Output),
                mk_ep(2, 21, "IN", IoDirection::Input),
            ],
        ));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 2);
        assert_eq!(q.edges.len(), 1);
    }

    /// ── t2_chain: linear chain → [u3][u1][u2] ──
    #[test]
    fn quotient_t2_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.nets.push(mk_signal_net(
            10,
            "u3_u1",
            vec![
                mk_ep(3, 31, "OUT", IoDirection::Output),
                mk_ep(1, 11, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            11,
            "u1_u2",
            vec![
                mk_ep(1, 12, "OUT", IoDirection::Output),
                mk_ep(2, 21, "IN", IoDirection::Input),
            ],
        ));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 3);
        assert_eq!(q.edges.len(), 2);
    }

    /// ── t3_chain: 4-node linear chain → [u3][u1][u2][u4] ──
    #[test]
    fn quotient_t3_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.boxes.push(mk_ic(4, "u4", 3));
        g.nets.push(mk_signal_net(
            10,
            "u3_u1",
            vec![
                mk_ep(3, 31, "OUT", IoDirection::Output),
                mk_ep(1, 11, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            11,
            "u1_u2",
            vec![
                mk_ep(1, 12, "OUT", IoDirection::Output),
                mk_ep(2, 21, "IN", IoDirection::Input),
            ],
        ));
        g.nets.push(mk_signal_net(
            12,
            "u2_u4",
            vec![
                mk_ep(2, 22, "OUT", IoDirection::Output),
                mk_ep(4, 41, "IN", IoDirection::Input),
            ],
        ));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 4);
        assert_eq!(q.edges.len(), 3);
    }

    /// ── Rail nets produce no edges ──
    #[test]
    fn quotient_rail_nets_ignored() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(100, "V3V3", 1));
        // Signal net
        g.nets.push(mk_signal_net(
            10,
            "u1_u2",
            vec![
                mk_ep(1, 11, "OUT", IoDirection::Output),
                mk_ep(2, 21, "IN", IoDirection::Input),
            ],
        ));
        // Power net (should be ignored)
        g.nets.push(VizNet::new(
            11,
            "V3V3".into(),
            NetKind::Power,
            NetRole::Rail { volt: None },
            vec![
                EndpointRef::with_io(100, 1001, "V3V3", IoDirection::Power),
                EndpointRef::with_io(1, 12, "VDD", IoDirection::Power),
                EndpointRef::with_io(2, 22, "VDD", IoDirection::Power),
            ],
        ));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 3, "all IC boxes should be nodes");
        assert_eq!(q.edges.len(), 1, "rail nets should not produce edges");
    }
}
