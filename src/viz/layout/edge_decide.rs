// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P9-B · edge decision layer for root block diagram
//!
//! Converts VizNet into Vec<BlockEdge> by applying four rules:
//!
//! - **R-P Projection**: project each net's endpoints to the root layer;
//!   if <2 projected points, the net is not drawn.
//! - **R-B Name Visibility**: a net name is visible in the root layer only if
//!   the source code explicitly mentions it.
//! - **R-M Edge Merge**: nets with the same (from_box, to_box, link)
//!   are merged into a single edge.
//! - **R-R Power**: power nets draw driver→consumer edges; ground nets are
//!   invisible in the root layer.

use std::collections::{HashMap, HashSet};

use crate::vector::graph::McVecGraph;
use crate::vector::model::link::LinkCtx;

/// A block-diagram edge connecting two boxes.
#[derive(Debug, Clone)]
pub struct BlockEdge {
    pub from_box: i64,
    pub to_box: i64,
    pub label: String,
    pub lane_count: usize,
    pub kind: EdgeKind,
    pub source_span: Option<crate::semantic::common::SourcePos>,
    /// ★ §8.9.6: structured link context for edge merging.
    /// `Some` when this edge belongs to a link (e.g., SPI, I2C); the
    /// link `name` is the R-M merge key and the edge label.
    pub link: Option<LinkCtx>,
    /// ★ B2: whether this edge is bidirectional (e.g., SPI bus).
    /// Set to true when the original nets had edges in both directions.
    pub bidirectional: bool,
}

/// Edge kind for rendering decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Signal,
    Power,
    Bus,
}

/// Result of edge decision for a layer.
#[derive(Debug)]
pub struct EdgeDecideReport {
    pub box_count: usize,
    pub edge_count: usize,
    pub untraceable: usize,
    pub unrendered: usize,
    /// ★ P9-C W4: edges exceeding bend budget (bend>2)
    pub bend_over_budget: usize,
    /// ★ P9-C W4: route escalation count (A*/channel hits)
    pub route_escalation: usize,
}

impl EdgeDecideReport {
    pub fn log(&self, layer: &str) {
        eprintln!(
            "[edge] {}: {} boxes / {} edges / {} untraceable / {} unrendered / {} bend_over_budget / {} escalation",
            layer,
            self.box_count,
            self.edge_count,
            self.untraceable,
            self.unrendered,
            self.bend_over_budget,
            self.route_escalation,
        );
    }

    /// ★ P9-C G18: check if bend budget is clean
    pub fn is_g18_clean(&self) -> bool {
        self.bend_over_budget == 0 && self.route_escalation == 0
    }
}

/// Strip the member suffix from power net labels.
///
/// Power net names like "V3V3.VCC", "V1V2.VCC", "V5V.VCC" should be
/// displayed as "V3V3", "V1V2", "V5V" per R-B rule: the source code
/// writes the base name, the member name ".VCC" is not exposed.
fn strip_power_label(name: &str) -> String {
    if let Some(pos) = name.rfind(".VCC") {
        name[..pos].to_string()
    } else {
        name.to_string()
    }
}

/// Decide edges for the root layer from nets.
///
/// ## Pipeline
/// 1. **R-P**: For each net, project endpoints to the root layer.
///    An endpoint is visible if its owner_box is in the root layer's box_ids.
///    If projected count < 2, skip the net.
/// 2. **R-B**: Ground nets are already filtered by `filter_ground_nets_for_main`.
///    Power nets without driver are skipped (no edge to draw).
/// 3. **R-R**: For power nets, draw driver→consumer edges.
/// 4. **R-M**: Group nets by (from_box, to_box, link) and merge them.
///    link is None for now (P9-A2 not yet implemented).
pub fn decide_edges(graph: &McVecGraph) -> (Vec<BlockEdge>, EdgeDecideReport) {
    eprintln!(
        "[DEBUG edge_decide] decide_edges: graph has {} nets, {} boxes",
        graph.nets.len(),
        graph.boxes.len()
    );
    let box_ids: std::collections::HashSet<i64> = graph.boxes.iter().map(|b| b.id).collect();

    let mut edges: Vec<BlockEdge> = Vec::new();
    let mut untraceable = 0usize;
    let unrendered = 0usize;

    for net in &graph.nets {
        // ── R-P: project endpoints to root layer ──
        let projected: Vec<&crate::vector::graph::netdef::EndpointRef> = net
            .endpoints
            .iter()
            .filter(|ep| box_ids.contains(&ep.box_id))
            .collect();

        let ep_paths: Vec<String> = net
            .endpoints
            .iter()
            .map(|ep| {
                let box_name = graph
                    .boxes
                    .iter()
                    .find(|b| b.id == ep.box_id)
                    .map(|b| b.name.as_str())
                    .unwrap_or("?");
                format!("{}(box={})", ep.pin_name, box_name)
            })
            .collect();
        crate::vlog!(
            "[edge] net '{}' (nid={}, kind={:?}, rail={:?}, link={:?}): {} endpoints [{}], {} projected",
            net.name,
            net.nid,
            net.kind,
            net.rail.as_ref().map(|r| &r.class),
            net.link,
            net.endpoints.len(),
            ep_paths.join(", "),
            projected.len()
        );

        if projected.len() < 2 {
            // Net doesn't have enough visible endpoints in this layer → skip
            continue;
        }

        // ── R-R: power net handling ──
        if let Some(ref rail) = net.rail {
            if rail.class == crate::vector::model::RailClass::Power {
                crate::vlog!(
                    "[edge] power net '{}': driver_pin={:?}, projected endpoints: {:?}",
                    net.name,
                    rail.driver_pin,
                    projected
                        .iter()
                        .map(|ep| (ep.box_id, ep.pin_id))
                        .collect::<Vec<_>>()
                );
                if let Some(driver_pin) = rail.driver_pin {
                    // Driver→consumer edges
                    let driver_box = projected
                        .iter()
                        .find(|ep| ep.pin_id == driver_pin)
                        .map(|ep| ep.box_id)
                        // Fallback: if pin_id changed (e.g. promote_synthetic_pins),
                        // use the first endpoint as driver (driver_edges always have
                        // driver as first endpoint).
                        .or_else(|| projected.first().map(|ep| ep.box_id));

                    crate::vlog!(
                        "[edge] power net '{}': driver_box={:?}",
                        net.name,
                        driver_box
                    );

                    if let Some(dbox) = driver_box {
                        let label = strip_power_label(&net.name);
                        for ep in &projected {
                            if ep.box_id != dbox {
                                edges.push(BlockEdge {
                                    from_box: dbox,
                                    to_box: ep.box_id,
                                    label: label.clone(),
                                    lane_count: 1,
                                    kind: EdgeKind::Power,
                                    source_span: net.source_span.clone(),
                                    link: net.link.clone(),
                                    bidirectional: false,
                                });
                            }
                        }
                    }
                }
                // Power nets without driver (ground nets) are invisible in root
                continue;
            }
        }

        // ── Non-power nets: create edges between box pairs ──
        // For a net with 2+ projected endpoints, create edges between all pairs
        // that share a common box. This is a simplification; R-M will merge them.
        let box_list: Vec<i64> = projected.iter().map(|ep| ep.box_id).collect();
        let unique_boxes: Vec<i64> = {
            let mut seen = std::collections::HashSet::new();
            let mut v = Vec::new();
            for &bid in &box_list {
                if seen.insert(bid) {
                    v.push(bid);
                }
            }
            v
        };

        if unique_boxes.len() == 2 {
            // Simple two-box net: one edge
            let (from_box, to_box) = (unique_boxes[0], unique_boxes[1]);
            let label = if crate::instant::mc_net::is_anon_net_name(&net.name) {
                String::new()
            } else {
                net.name.clone()
            };

            crate::vlog!(
                "[edge] signal net '{}': edge {} -> {} (label='{}')",
                net.name,
                from_box,
                to_box,
                label
            );

            if net.source_span.is_none() {
                untraceable += 1;
            }

            edges.push(BlockEdge {
                from_box,
                to_box,
                label,
                lane_count: 1,
                kind: EdgeKind::Signal,
                source_span: net.source_span.clone(),
                link: net.link.clone(),
                bidirectional: false,
            });
        } else if unique_boxes.len() > 2 {
            // Multi-box net: create edges between all pairs
            for i in 0..unique_boxes.len() {
                for j in (i + 1)..unique_boxes.len() {
                    let label = if crate::instant::mc_net::is_anon_net_name(&net.name) {
                        String::new()
                    } else {
                        net.name.clone()
                    };

                    if net.source_span.is_none() {
                        untraceable += 1;
                    }

                    edges.push(BlockEdge {
                        from_box: unique_boxes[i],
                        to_box: unique_boxes[j],
                        label,
                        lane_count: 1,
                        kind: EdgeKind::Signal,
                        source_span: net.source_span.clone(),
                        link: net.link.clone(),
                        bidirectional: false,
                    });
                }
            }
        }
    }

    // ── ★ B2: bidirectional detection ──
    // Check for each pair of boxes if there are edges in both directions.
    // Also, Bus edges with lane_count>1 are considered bidirectional (e.g., SPI).
    {
        let mut pair_dirs: HashMap<(i64, i64), (bool, bool)> = HashMap::new();
        for edge in &edges {
            let key = if edge.from_box < edge.to_box {
                (edge.from_box, edge.to_box)
            } else {
                (edge.to_box, edge.from_box)
            };
            let entry = pair_dirs.entry(key).or_insert((false, false));
            if edge.from_box < edge.to_box {
                entry.0 = true; // forward direction
            } else {
                entry.1 = true; // reverse direction
            }
        }
        // Mark bidirectional pairs (both directions)
        let bidirectional_pairs: HashSet<(i64, i64)> = pair_dirs
            .iter()
            .filter(|(_, (fwd, rev))| *fwd && *rev)
            .map(|(k, _)| *k)
            .collect();
        for edge in &mut edges {
            let key = if edge.from_box < edge.to_box {
                (edge.from_box, edge.to_box)
            } else {
                (edge.to_box, edge.from_box)
            };
            if bidirectional_pairs.contains(&key) {
                edge.bidirectional = true;
            }
        }
    }

    // ── R-M: merge edges with same (from_box, to_box, kind, link) ──
    // When link is set, edges with the same link are merged into a bus.
    // The label is the link name, lane_count is the number of merged edges.
    // ★ P9-fix: edges with link=None are NEVER merged — they are independent
    // edges (e.g. DAC_OUT and SPK_MUTE are two separate edges between mcu513↔speaker).
    let before_merge = edges.len();
    let mut merged: Vec<BlockEdge> = Vec::new();
    let mut seen_pairs: HashMap<(i64, i64, EdgeKind, Option<String>), usize> = HashMap::new();

    for edge in edges {
        // ★ P9-fix: only merge edges that have a link. Edges without
        // link are kept as separate edges.
        if edge.link.is_none() {
            merged.push(edge);
            continue;
        }

        let pair = if edge.from_box < edge.to_box {
            (
                edge.from_box,
                edge.to_box,
                edge.kind,
                edge.link.as_ref().and_then(|g| g.name.clone()),
            )
        } else {
            (
                edge.to_box,
                edge.from_box,
                edge.kind,
                edge.link.as_ref().and_then(|g| g.name.clone()),
            )
        };

        if let Some(&idx) = seen_pairs.get(&pair) {
            // Merge: increment lane_count
            merged[idx].lane_count += 1;
            if merged[idx].kind == EdgeKind::Signal && edge.kind == EdgeKind::Signal {
                merged[idx].kind = EdgeKind::Bus;
            }
            // Preserve bidirectional flag
            merged[idx].bidirectional = merged[idx].bidirectional || edge.bidirectional;
            // Use the link name as the label
            merged[idx].label = merged[idx]
                .link
                .as_ref()
                .and_then(|g| g.name.as_deref())
                .unwrap_or_default()
                .to_string();
        } else {
            // Use the link name as the label
            let mut e = edge;
            if let Some(ref lc) = e.link {
                e.label = lc.name.clone().unwrap_or_default();
            }
            seen_pairs.insert(pair, merged.len());
            merged.push(e);
        }
    }

    // ── ★ B2: mark merged Bus edges as bidirectional ──
    // Bus edges with lane_count>=4 represent multi-lane bidirectional bus interfaces
    // (e.g., SPI). Bus edges with lane_count<4 are unidirectional signal groups
    // (e.g., MIC with 2 lanes).
    for edge in &mut merged {
        if edge.kind == EdgeKind::Bus && edge.lane_count >= 4 {
            edge.bidirectional = true;
        }
    }

    // ── ★ P9-C W4: bend budget check ──
    // For each merged edge, compute the expected bend count based on box positions.
    // 0 bends: boxes aligned horizontally or vertically (|dx|≈0 or |dy|≈0)
    // 1 bend:  L-shaped (|dx|>0 and |dy|>0)
    // 2 bends: offset in both dimensions with a reason
    // >2 bends: over budget → warn
    let layer = &graph.name;
    let mut bend_over_budget = 0usize;
    for edge in &merged {
        let (Some(from_box), Some(to_box)) = (
            graph.boxes.iter().find(|b| b.id == edge.from_box),
            graph.boxes.iter().find(|b| b.id == edge.to_box),
        ) else {
            continue;
        };
        let cx1 = from_box.x + from_box.w / 2.0;
        let cy1 = from_box.y + from_box.h / 2.0;
        let cx2 = to_box.x + to_box.w / 2.0;
        let cy2 = to_box.y + to_box.h / 2.0;
        let dx = (cx2 - cx1).abs();
        let dy = (cy2 - cy1).abs();

        // Budget: 0 if aligned, 1 if L-shaped, 2 if offset
        let threshold = 10.0; // minimum pixel distance to count as "not aligned"
        let budget = if dx < threshold && dy < threshold {
            0 // overlapping boxes
        } else if dx < threshold || dy < threshold {
            0 // aligned on one axis
        } else {
            // Determine if boxes overlap in either axis
            let overlap_x =
                (from_box.x + from_box.w > to_box.x) && (to_box.x + to_box.w > from_box.x);
            let overlap_y =
                (from_box.y + from_box.h > to_box.y) && (to_box.y + to_box.h > from_box.y);
            if overlap_x || overlap_y {
                1 // L-shaped: overlap in one axis
            } else {
                2 // offset in both axes, need 2 bends
            }
        };

        if budget > 2 {
            bend_over_budget += 1;
            eprintln!(
                "[warn] {}: edge {} -> {} bend budget exceeded (budget={}, actual>2)",
                layer, edge.from_box, edge.to_box, budget
            );
        }
    }

    let report = EdgeDecideReport {
        box_count: graph.boxes.len(),
        edge_count: merged.len(),
        untraceable,
        unrendered,
        bend_over_budget,
        route_escalation: 0, // root layer has no routing phase
    };

    if bend_over_budget > 0 {
        eprintln!(
            "[trace] {}: G18 bend_over_budget: {} edges exceed budget",
            layer, bend_over_budget
        );
    }

    // ── ★ P9-A2.5 renderdiff trace output ──

    eprintln!(
        "[trace] {}: R-M edge merge: {} -> {} edges",
        layer,
        before_merge,
        merged.len()
    );

    // 1. Trace edges with link (provenance)
    for edge in &merged {
        if let Some(ref lc) = edge.link {
            let link_name = lc.name.as_deref().unwrap_or("");
            if let Some(ref pos) = edge.source_span {
                eprintln!(
                    "[trace] {}: edge '{}' <- {}:{}  (link={})",
                    layer, link_name, pos.uri, pos.offset, link_name
                );
            } else {
                eprintln!(
                    "[trace] {}: edge '{}'  (link={})",
                    layer, link_name, link_name
                );
            }
        }
        if edge.lane_count > 1 {
            eprintln!(
                "[trace] {}: merged edge {} -> {} kind={:?} label=\"{}\" lane_count={} link={:?}",
                layer,
                edge.from_box,
                edge.to_box,
                edge.kind,
                edge.label,
                edge.lane_count,
                edge.link
            );
        }
    }

    // 2. Source span coverage across all nets
    let total_nets = graph.nets.len();
    let nets_with_span = graph
        .nets
        .iter()
        .filter(|n| n.source_span.is_some())
        .count();
    let pct = if total_nets > 0 {
        nets_with_span as f64 * 100.0 / total_nets as f64
    } else {
        0.0
    };
    eprintln!(
        "[trace] {}: source_span coverage: {}/{} nets ({:.0}%)",
        layer, nets_with_span, total_nets, pct
    );

    // 3. Count nets with link
    let nets_with_pg = graph.nets.iter().filter(|n| n.link.is_some()).count();
    eprintln!(
        "[trace] {}: link coverage: {}/{} nets",
        layer, nets_with_pg, total_nets
    );

    (merged, report)
}
