// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`McVecGraph`] -- graph container
//!
//! Holds boxes / edges (legacy, deprecated) / nets / sub-graphs of one layer.
//!
//! ## ★ P03 (S1) Changes
//! - `edges` field **kept but no longer populated**:
//!   - `from_block.rs::build_mc_vec_graph` stopped writing to `graph.edges`
//!   - `components.rs::build_adjacency` now reads only `graph.nets`
//!   - `entry_points.rs::collect_pins_per_box` same as above
//!   - `wire.rs::render_edge` removed
//! - `nets: Vec<VizNet>` is the **only network representation**
//! - `total_edges()` / `total_wires()` still compile, but always return 0 under the production path
//!
//! ## Field evolution
//! - `boxes`      -- always present
//! - `edges`      -- **deprecated**, kept only for from_table.rs (legacy builder)
//! - `nets`       -- multi-endpoint hyperedge ([`VizNet`]), the only network model
//! - `sub_graphs` -- recursive sub-graphs

use std::fmt;

use super::boxdef::McVecBox;
use super::netdef::{McVecEdge, VizNet};

// ============================================================================
// McVecGraph
// ============================================================================

#[derive(Debug, Clone)]
pub struct McVecGraph {
    /// ID of this layer's block (corresponds to InstTable)
    pub bid: i64,
    /// Name of this layer's block (module instance name)
    pub name: String,
    /// Boxes of this layer
    pub boxes: Vec<McVecBox>,
    /// Edges of this layer (★ P03: deprecated, only from_table.rs legacy builder still populates)
    ///
    /// New code cannot read any edge (because from_block no longer writes). Please use `nets`.
    pub edges: Vec<McVecEdge>,
    /// Nets of this layer (the only network representation after P03)
    ///
    /// One `VizNet` per net, no limit on endpoint count. Router uses this to compute paths.
    pub nets: Vec<VizNet>,
    /// Sub-graphs (recursive sub-modules, implementable as expandable)
    pub sub_graphs: Vec<McVecGraph>,
    /// ★ FIX (sub-graph): whether multi-endpoint single-driver nets in this layer use
    /// hub-star routing (with the main device pin as hub, multiple wires fanning out from
    /// the device) instead of TrunkTap (shared trunk). Set by the layouter:
    /// sub-layer = true, top layer = false (top-layer routing behavior unchanged).
    pub fanout_star: bool,
    /// ★ Layout coverage tracking: number of islands claimed by islands decomposition.
    /// Set by `islands::apply_islands`, read by `compute_fidelity` for the gate.
    pub islands_claimed: usize,
    pub islands_total: usize,
}

impl McVecGraph {
    /// Create an empty graph
    pub fn new(bid: i64, name: String) -> Self {
        Self {
            bid,
            name,
            boxes: vec![],
            edges: vec![],
            nets: vec![],
            sub_graphs: vec![],
            fanout_star: false,
            islands_claimed: 0,
            islands_total: 0,
        }
    }

    // ─── Statistics ─────────────────────────────────────────────────────────

    /// Recursive total box count
    pub fn total_boxes(&self) -> usize {
        self.boxes.len()
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_boxes())
                .sum::<usize>()
    }

    /// Recursive total edge count (legacy binary edges)
    pub fn total_edges(&self) -> usize {
        self.edges.len()
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_edges())
                .sum::<usize>()
    }

    /// Recursive total wire count (wires inside legacy binary edges)
    pub fn total_wires(&self) -> usize {
        let local: usize = self.edges.iter().map(|e| e.wires.len()).sum();
        local
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_wires())
                .sum::<usize>()
    }

    /// ★ NEW: Recursive total net count (new hyperedge)
    pub fn total_nets(&self) -> usize {
        self.nets.len()
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_nets())
                .sum::<usize>()
    }

    /// ★ NEW: Recursive total endpoint count
    pub fn total_endpoints(&self) -> usize {
        let local: usize = self.nets.iter().map(|n| n.endpoint_count()).sum();
        local
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_endpoints())
                .sum::<usize>()
    }

    // ─── Sub-graph query ─────────────────────────────────────────────────────

    /// Find a sub-graph by bid (used by frontend to locate during expand)
    pub fn find_subgraph(&self, bid: i64) -> Option<&McVecGraph> {
        if self.bid == bid {
            return Some(self);
        }
        for sub in &self.sub_graphs {
            if let Some(found) = sub.find_subgraph(bid) {
                return Some(found);
            }
        }
        None
    }

    // ─── Display (for debugging, with recursive indentation) ──────────────────

    fn fmt_with_indent(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let ind = "  ".repeat(depth);
        writeln!(
            f,
            "{}Graph(bid={}, name=\"{}\", boxes={}, edges={}, nets={})",
            ind,
            self.bid,
            self.name,
            self.boxes.len(),
            self.edges.len(),
            self.nets.len()
        )?;
        for b in &self.boxes {
            writeln!(
                f,
                "{}  Box(id={}, \"{}\" [{}], kind={}, pins={})",
                ind, b.id, b.name, b.class_name, b.kind, b.pin_count
            )?;
        }
        for e in &self.edges {
            writeln!(
                f,
                "{}  Edge({}->{}, {}, \"{}\")",
                ind, e.src_box, e.dst_box, e.edge_type, e.net_name
            )?;
        }
        for n in &self.nets {
            writeln!(
                f,
                "{}  Net(#{}, \"{}\", {}, endpoints={})",
                ind,
                n.nid,
                n.name,
                n.kind,
                n.endpoints.len()
            )?;
        }
        for sub in &self.sub_graphs {
            sub.fmt_with_indent(f, depth + 1)?;
        }
        Ok(())
    }
}

impl fmt::Display for McVecGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}
