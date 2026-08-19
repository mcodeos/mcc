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

use std::collections::HashMap;
use std::fmt;

use super::boxdef::{McVecBox, PortDir, ZoneBorder};
use super::netdef::{McVecEdge, NetRole, VizNet};

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
    /// ★ M0-2: module port list (port name, direction, net role), from the module declaration
    pub module_ports: Vec<(String, PortDir, NetRole)>,
    /// ★ M2-3: zone border list (dashed rounded rect + title), filled by v2 layout
    pub zone_borders: Vec<ZoneBorder>,
    /// ★ M4-0: canvas hint (once set by v2 layout, normalize no longer recomputes from box coordinates)
    pub canvas_hint: Option<(f64, f64)>,
    /// ★ M4-1a: whether this is a sub-module graph (sub-modules use a smaller canvas minimum constraint)
    pub is_submodule: bool,
    /// ★ P7-3: rail terminal decorations (discipline 11: terminals are not boxes).
    ///
    /// Power/ground endpoints adjudicated by R-1/R-3 as "symbols placed in situ",
    /// existing as pin render attributes: zero layout cost, zero routing cost,
    /// never entering `boxes`; located at render time by the pin's entry_point,
    /// with the symbol reusing `PowerRailShape`.
    pub rail_decorations: Vec<RailDecoration>,
    /// ★ P7-4: this layer's geometry double-write diagnostics (collected by stage-boundary snapshot comparison, observe-only, no blocking).
    ///
    /// Dimension-ownership ruler (P7-4e): xy/wh belong to the Placement stage,
    /// pins to the PinPlace stage, Route is read-only. A write that crosses
    /// stages on an unauthorized dimension records one entry; intra-stage
    /// multi-function cooperation is free.
    pub geom_double_writes: Vec<GeomDoubleWrite>,
    /// ★ P7-9: pin_id → parent port group id (for collapsing member ports to port groups).
    /// Built by fromblock.rs from the InstTable. Only populated for port entries.
    pub pin_parent: HashMap<i64, i64>,
    /// ★ Wire/Label split: column pitch from the layouter (default 480.0 for top, 360.0 for sub).
    /// Set by FlowLayouter::layout. Used by wire_label_split pass for adaptive threshold.
    pub col_pitch: f64,
    /// ★ P9-B: whether this is the root (top-level) graph. Used instead of
    /// hardcoding `graph.name == "main"` so the pipeline works with any
    /// top-level module name.
    pub is_root: bool,
    /// ★ C1b: rendering style for this layer.
    pub layer_style: LayerStyle,
}

/// ★ C1b F0: rendering style — determines which pipeline a layer uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerStyle {
    Block,
    Device,
}

/// ★ P7-4e: geometry stages (the roadmap's three stages, refined for implementation)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeomStage {
    /// Decides x/y/w/h: prepare / size / placement / schematic_model / two_lane /
    /// idiom / post / renormalize / net_labels (initial net-label box placement)
    Placement,
    /// ★ PinFinal —— pin-driven final geometry: pin_place (entry_point
    /// allocation + hub enlargement) → islands/passives placed against pins.
    /// These writers relaying each other is a functional need (placing against
    /// pins depends on pin allocation results); all dimensions are free within
    /// the stage. Corresponds to the roadmap's PinPlace stage, named after
    /// real dependencies.
    PinFinal,
    /// Read-only geometry: route / feedback (reroute variant) / borders
    Route,
}

/// Writer label → logical stage.
pub fn stage_of(writer: &str) -> GeomStage {
    match writer {
        "7.pin_place" | "8.islands" | "8.sp_fallback" | "8.ladder_fallback"
        | "10.passive_inline" => GeomStage::PinFinal,
        "13.route" | "14.feedback" | "15.borders" | "16.net_labels_2" => GeomStage::Route,
        // 1.prepare 2.size 3.placement 4.schematic_model 5.two_lane 6.idiom
        // 9.post 11.renormalize 12.net_labels
        _ => GeomStage::Placement,
    }
}

/// ★ P7-4: structured diagnostic for one box getting geometry written by an out-of-authority stage
#[derive(Debug, Clone)]
pub struct GeomDoubleWrite {
    pub box_id: i64,
    pub box_name: String,
    pub prev_writer: &'static str,
    pub cur_writer: &'static str,
    /// Dimensions changed this time: xy / wh / pins ("new" for newly added boxes)
    pub dims: Vec<&'static str>,
}

/// ★ P7-4: stage-boundary geometry snapshot (the return value of `geom_snapshot`, aligned by box id)
#[derive(Debug, Clone)]
pub struct BoxGeomSnapshot {
    sigs: Vec<(i64, f64, f64, f64, f64, Vec<super::boxdef::EntryPoint>)>,
}

/// ★ P7-3: a power/ground terminal symbol attached to a pin
#[derive(Debug, Clone)]
pub struct RailDecoration {
    /// Owning box (a real box, not the symbol itself)
    pub box_id: i64,
    /// The decorated pin (InstTable entry id, same as EndpointRef.pin_id)
    pub pin_id: i64,
    /// true = ground symbol (pointing down, no text); false = rail terminal (pointing up, dot + net name)
    pub is_ground: bool,
    /// Display text (rail terminal = net name; unused for ground symbols)
    pub label: String,
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
            module_ports: vec![],
            zone_borders: vec![],
            canvas_hint: None,
            is_submodule: false,
            rail_decorations: vec![],
            geom_double_writes: vec![],
            pin_parent: HashMap::new(),
            col_pitch: 480.0,
            is_root: false,
            layer_style: LayerStyle::Block,
        }
    }

    // ─── ★ P7-4: geometry writer observation (observe-only, no blocking) ────────────────────────────

    /// Pre-stage snapshot: per box (id, x, y, w, h, entry_points), **aligned by id**
    /// (stages may add/remove boxes; index alignment would misalign).
    ///
    /// After the stage ends, hand the snapshot to [`McVecGraph::claim_geom_changes`];
    /// boxes whose geometry signature changed were written by that stage.
    /// Equal-value rewrites count as unwritten (no output effect, not listed).
    pub fn geom_snapshot(&self) -> BoxGeomSnapshot {
        BoxGeomSnapshot {
            sigs: self
                .boxes
                .iter()
                .map(|b| (b.id, b.x, b.y, b.w, b.h, b.entry_points.clone()))
                .collect(),
        }
    }

    /// Post-stage claim: record boxes whose geometry changed under `writer`,
    /// judging violations by dimension ownership:
    /// - xy/wh changed and writer not in the Placement stage → record diagnostic
    /// - pins changed and writer not in the PinPlace stage → record diagnostic
    /// - Boxes newly added by the stage (id not in the snapshot) are first writes,
    ///   not recorded; additions by Placement are legal, additions by other stages
    ///   (theoretically nonexistent) are still flagged by the "new" dimension.
    /// Cooperative writes within a stage (same `GeomStage`) are not recorded.
    /// Returns the number of boxes written by this stage.
    pub fn claim_geom_changes(&mut self, snap: &BoxGeomSnapshot, writer: &'static str) -> usize {
        let stage = stage_of(writer);
        let mut written = 0usize;
        for b in self.boxes.iter_mut() {
            let dims: Vec<&'static str> = match snap.sigs.iter().find(|(id, ..)| *id == b.id) {
                Some((_, x, y, w, h, eps)) => {
                    let mut d = Vec::new();
                    if b.x != *x || b.y != *y {
                        d.push("xy");
                    }
                    if b.w != *w || b.h != *h {
                        d.push("wh");
                    }
                    if &b.entry_points != eps {
                        d.push("pins");
                    }
                    d
                }
                None => vec!["new"],
            };
            if dims.is_empty() {
                continue;
            }
            // ★ Dashed-border exemption: the top-level module border with a
            // negative id (P7-3: negative id, empty name) is canvas decoration
            // that shrinks with its contents; the borders stage writing it is
            // not a geometry double-write.
            if b.id < 0 {
                continue;
            }
            let prev = b.geom_writer;
            b.geom_writer = Some(writer);
            written += 1;
            let violates = match stage {
                GeomStage::Placement => dims.contains(&"pins"),
                // PinFinal: pin allocation + hub enlargement + placement against pins, all dimensions free
                GeomStage::PinFinal => false,
                GeomStage::Route => true, // read-only stage; any geometry change is a violation
            };
            if violates {
                if let Some(p) = prev {
                    self.geom_double_writes.push(GeomDoubleWrite {
                        box_id: b.id,
                        box_name: b.name.clone(),
                        prev_writer: p,
                        cur_writer: writer,
                        dims,
                    });
                }
            }
        }
        written
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
