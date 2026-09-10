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
//! - **R-M Edge Merge**: nets with the same (from_box, to_box, trunk)
//!   are merged into a single edge.
//! - **R-R Power**: power nets draw driver→consumer edges; ground nets are
//!   invisible in the root layer.

use std::collections::{HashMap, HashSet};

use crate::vector::graph::McVecGraph;
use crate::vector::model::trunk::TrunkCtx;

/// A block-diagram edge connecting two boxes.
#[derive(Debug, Clone)]
pub struct BlockEdge {
    pub from_box: i64,
    pub to_box: i64,
    /// ★ L1/§5: endpoint identity -- the pin ids this edge actually attaches to at
    /// each end, ascending and deduped. **Index 0 is the primary pin** (the
    /// deterministic choice: for a power edge's driver end, `rail.driver_pin`;
    /// otherwise the lowest pin id on that box for this net).
    ///
    /// A `Vec` rather than a scalar because R-M merges several edges that may span
    /// several pins per end. Empty means "no known real pin at this end" -- note
    /// that a synthesized endpoint's `pin_id` is `-1` before
    /// `promote_synthetic_pins` and a `3e9`-based id after.
    ///
    /// Consumers must match on **these ids**, never on `label` against a pin name:
    /// net labels and pin names are different namespaces (see design R5).
    pub from_pins: Vec<i64>,
    pub to_pins: Vec<i64>,
    /// ★ B5: the driver end **as the model layer declares it** -- the box owning
    /// `RailSpec.driver_pin`. `Some` for power edges, `None` for signal/bus ones.
    ///
    /// Stamped here because `decide_edges` is the only place that reads the model
    /// layer; consumers must read **this** rather than re-deriving a driver by
    /// voting on `from_box` frequency. That vote lived in `render/mod.rs` and
    /// picked its winner with `HashMap::max_by_key`, i.e. the tie was broken by
    /// HashMap iteration order -- nondeterministic across runs, same defect class
    /// as R0 in `radial.rs`. The vote only ever reproduced what is already uniform
    /// here (`from_box == driver_box` on every power edge), so deleting it is
    /// behaviour-neutral today and removes the nondeterminism for good.
    ///
    /// If an R-M merge ever joins two power edges that disagree on this, the group
    /// spans two different nets and the driver is genuinely ambiguous -- that is
    /// logged, never silently resolved.
    pub driver_box: Option<i64>,
    pub label: String,
    pub lane_count: usize,
    pub kind: EdgeKind,
    pub source_span: Option<crate::semantic::common::SourcePos>,
    /// ★ §8.9.6: structured trunk context for edge merging.
    /// `Some` when this edge belongs to a trunk (e.g., SPI, I2C); the
    /// trunk `name` is the R-M merge key and the edge label.
    pub trunk: Option<TrunkCtx>,
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
    /// ★ B1/§5: endpoint identities that name no real net endpoint. Must be 0 --
    /// anything else means pin identity went stale and consumers would silently
    /// match nothing.
    pub identity_dangling: usize,
}

impl EdgeDecideReport {
    pub fn log(&self, layer: &str) {
        crate::vlog!(
            "[edge] {}: {} boxes / {} edges / {} untraceable / {} unrendered / {} bend_over_budget / {} escalation / {} dangling_identity",
            layer,
            self.box_count,
            self.edge_count,
            self.untraceable,
            self.unrendered,
            self.bend_over_budget,
            self.route_escalation,
            self.identity_dangling,
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

/// Every pin id `box_id` has among a net's projected endpoints, ascending and
/// deduped. **Index 0 is the primary pin** -- L1's deterministic rule (§5.2): the
/// lowest pin id, never "whichever endpoint was listed first".
///
/// Empty when the box has no endpoint on this net.
fn pins_of_box(projected: &[&crate::vector::graph::netdef::EndpointRef], box_id: i64) -> Vec<i64> {
    let mut v: Vec<i64> = projected
        .iter()
        .filter(|ep| ep.box_id == box_id)
        .map(|ep| ep.pin_id)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// ★ B2: a stable fingerprint of everything [`decide_edges`] reads.
///
/// The function runs **three times per render** (facade in the prepare phase,
/// `place_radial` for the anchors, `render_block_edges` for the lines) and its
/// consumers must agree on the answer -- the whole point of the anchor law is
/// that the line drawn is the line the anchor was computed for. Nothing forces
/// those three calls to see the same graph, though: each reads whatever state
/// its own phase left behind.
///
/// Measured 2026-09-10: they *do* agree today (the per-net `[edge]` logs of all
/// three calls are byte-identical on both hbl and pwrint), so this is latent, not
/// a live bug. But if a future pass ever moved net/pin identity between two of
/// them, the divergence would be **silent** -- anchors would stop matching lines
/// and nothing would say so. Printing this fingerprint gives the three calls two
/// comparable values in the dump (§3.1: make the divergence visible).
///
/// Deliberately structural: only ids and counts, never a name heuristic.
fn input_fingerprint(graph: &McVecGraph) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    fn mix(h: &mut u64, v: i64) {
        // Sign-extended, not `as u64`: the value must hash as an i64 so that
        // `-1` (a pre-promotion synthesized pin) is distinct from `u64::MAX`.
        for b in v.to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(FNV_PRIME);
        }
    }

    let mut h: u64 = FNV_OFFSET;
    let mut ids: Vec<i64> = graph.boxes.iter().map(|b| b.id).collect();
    ids.sort_unstable();
    mix(&mut h, ids.len() as i64);
    for id in ids {
        mix(&mut h, id);
    }
    mix(&mut h, graph.nets.len() as i64);
    for net in &graph.nets {
        mix(&mut h, net.nid);
        mix(
            &mut h,
            net.rail
                .as_ref()
                .and_then(|r| r.driver_pin)
                .unwrap_or(i64::MIN),
        );
        // Endpoint order in the net is not part of the identity: sort it out.
        let mut eps: Vec<(i64, i64)> = net
            .endpoints
            .iter()
            .map(|ep| (ep.box_id, ep.pin_id))
            .collect();
        eps.sort_unstable();
        for (box_id, pin_id) in eps {
            mix(&mut h, box_id);
            mix(&mut h, pin_id);
        }
        for b in net
            .trunk
            .as_ref()
            .and_then(|t| t.name.as_deref())
            .unwrap_or("")
            .bytes()
        {
            mix(&mut h, b as i64);
        }
    }
    h
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
/// 4. **R-M**: Group nets by (from_box, to_box, trunk) and merge them.
///    trunk is None for now (P9-A2 not yet implemented).
pub fn decide_edges(graph: &McVecGraph) -> (Vec<BlockEdge>, EdgeDecideReport) {
    crate::vlog!(
        "[DEBUG edge_decide] decide_edges: graph has {} nets, {} boxes, input#{:016x}",
        graph.nets.len(),
        graph.boxes.len(),
        input_fingerprint(graph)
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
            "[edge] net '{}' (nid={}, kind={:?}, rail={:?}, trunk={:?}): {} endpoints [{}], {} projected",
            net.name,
            net.nid,
            net.kind,
            net.rail.as_ref().map(|r| &r.class),
            net.trunk,
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
                    // Driver→consumer edges. Resolve the driver *endpoint*, not just
                    // its box: the endpoint carries the pin id that is the driver
                    // end's identity.
                    let driver_ep = projected.iter().find(|ep| ep.pin_id == driver_pin);
                    if driver_ep.is_none() {
                        // §3.1: a fallback must be visible, never silent. The model
                        // layer named a driver pin (B5: it is the authority) that is
                        // not among this layer's projected endpoints -- identity is
                        // degraded here, so say so instead of quietly drawing from
                        // whatever endpoint happened to be first.
                        crate::vlog!(
                            "[edge] power net '{}': driver_pin={:?} NOT among projected {:?}; \
                             falling back to first endpoint (identity degraded)",
                            net.name,
                            driver_pin,
                            projected
                                .iter()
                                .map(|ep| (ep.box_id, ep.pin_id))
                                .collect::<Vec<_>>()
                        );
                    }
                    let driver_ep = driver_ep.or_else(|| projected.first());

                    crate::vlog!(
                        "[edge] power net '{}': driver_box={:?}",
                        net.name,
                        driver_ep.map(|ep| ep.box_id)
                    );

                    if let Some(dep) = driver_ep {
                        let dbox = dep.box_id;
                        let label = strip_power_label(&net.name);
                        for ep in &projected {
                            if ep.box_id != dbox {
                                edges.push(BlockEdge {
                                    from_box: dbox,
                                    to_box: ep.box_id,
                                    from_pins: vec![dep.pin_id],
                                    to_pins: vec![ep.pin_id],
                                    driver_box: Some(dbox),
                                    label: label.clone(),
                                    lane_count: 1,
                                    kind: EdgeKind::Power,
                                    source_span: net.source_span.clone(),
                                    trunk: net.trunk.clone(),
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
                from_pins: pins_of_box(&projected, from_box),
                to_pins: pins_of_box(&projected, to_box),
                driver_box: None,
                label,
                lane_count: 1,
                kind: EdgeKind::Signal,
                source_span: net.source_span.clone(),
                trunk: net.trunk.clone(),
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
                        from_pins: pins_of_box(&projected, unique_boxes[i]),
                        to_pins: pins_of_box(&projected, unique_boxes[j]),
                        driver_box: None,
                        label,
                        lane_count: 1,
                        kind: EdgeKind::Signal,
                        source_span: net.source_span.clone(),
                        trunk: net.trunk.clone(),
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

    // ── R-M: merge edges with same (from_box, to_box, kind, trunk) ──
    // When trunk is set, edges with the same trunk are merged into a bus.
    // The label is the trunk name, lane_count is the number of merged edges.
    // ★ P9-fix: edges with trunk=None are NEVER merged — they are independent
    // edges (e.g. DAC_OUT and SPK_MUTE are two separate edges between mcu513↔speaker).
    let before_merge = edges.len();
    let mut merged: Vec<BlockEdge> = Vec::new();
    let mut seen_pairs: HashMap<(i64, i64, EdgeKind, Option<String>), usize> = HashMap::new();

    for edge in edges {
        // ★ P9-fix: only merge edges that have a trunk. Edges without
        // trunk are kept as separate edges.
        if edge.trunk.is_none() {
            merged.push(edge);
            continue;
        }

        let pair = if edge.from_box < edge.to_box {
            (
                edge.from_box,
                edge.to_box,
                edge.kind,
                edge.trunk.as_ref().and_then(|g| g.name.clone()),
            )
        } else {
            (
                edge.to_box,
                edge.from_box,
                edge.kind,
                edge.trunk.as_ref().and_then(|g| g.name.clone()),
            )
        };

        if let Some(&idx) = seen_pairs.get(&pair) {
            // Merge: increment lane_count
            merged[idx].lane_count += 1;
            // Union the endpoint identity. The merge key is normalized to
            // (min_box, max_box), so an edge travelling the other way must have its
            // pin lists swapped before joining -- otherwise a reversed lane would
            // donate its `from_pins` to this edge's `to_pins`.
            let same_dir =
                merged[idx].from_box == edge.from_box && merged[idx].to_box == edge.to_box;
            let (fp, tp): (&[i64], &[i64]) = if same_dir {
                (&edge.from_pins, &edge.to_pins)
            } else {
                (&edge.to_pins, &edge.from_pins)
            };
            {
                let dst = &mut merged[idx].from_pins;
                dst.extend_from_slice(fp);
                dst.sort_unstable();
                dst.dedup();
            }
            {
                let dst = &mut merged[idx].to_pins;
                dst.extend_from_slice(tp);
                dst.sort_unstable();
                dst.dedup();
            }
            if merged[idx].kind == EdgeKind::Signal && edge.kind == EdgeKind::Signal {
                merged[idx].kind = EdgeKind::Bus;
            }
            // ★ B5: reconcile the declared driver. Equal or one-sided is the
            // normal case (the merge key already carries the trunk name, so a
            // differing driver means two distinct power nets landed in one group).
            // Keep the first and say so -- never let a tie be broken silently by
            // iteration order, which is exactly what the deleted `from` vote did.
            match (merged[idx].driver_box, edge.driver_box) {
                (Some(a), Some(b)) if a != b => {
                    crate::vlog!(
                        "[edge] merge: group ({}, {:?}) spans two drivers ({} vs {}); \
                         keeping {} -- the grouping key is the label, not the net",
                        pair.0,
                        pair.1,
                        a,
                        b,
                        a
                    );
                }
                (None, Some(b)) => merged[idx].driver_box = Some(b),
                _ => {}
            }
            // Preserve bidirectional flag
            merged[idx].bidirectional = merged[idx].bidirectional || edge.bidirectional;
            // Use the trunk name as the label
            merged[idx].label = merged[idx]
                .trunk
                .as_ref()
                .and_then(|g| g.name.as_deref())
                .unwrap_or_default()
                .to_string();
        } else {
            // Use the trunk name as the label
            let mut e = edge;
            if let Some(ref lc) = e.trunk {
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

    // ── ★ B1/§5: endpoint-identity invariant ──
    // Every pin id an edge claims must be a real endpoint of some net on that same
    // box. This is the invariant that makes `from_pins`/`to_pins` trustworthy; if it
    // ever breaks, the identity has gone stale (e.g. a pass renumbered pins after
    // `decide_edges`) and every consumer would silently match nothing -- the R5
    // failure mode. Checked against the live graph so it runs on real projects.
    let mut identity_dangling = 0usize;
    {
        let mut real: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
        for net in &graph.nets {
            for ep in &net.endpoints {
                real.insert((ep.box_id, ep.pin_id));
            }
        }
        for edge in &merged {
            for (box_id, pins) in [
                (&edge.from_box, &edge.from_pins),
                (&edge.to_box, &edge.to_pins),
            ] {
                for &pin in pins.iter() {
                    if !real.contains(&(*box_id, pin)) {
                        identity_dangling += 1;
                        crate::vlog!(
                            "[edge] {}: DANGLING endpoint identity: edge {}->{} claims pin {} on box {} \
                             which is no net endpoint",
                            layer,
                            edge.from_box,
                            edge.to_box,
                            pin,
                            box_id
                        );
                    }
                }
            }
        }
    }
    debug_assert_eq!(
        identity_dangling, 0,
        "BlockEdge endpoint identity is dangling (see [edge] DANGLING log lines)"
    );

    let report = EdgeDecideReport {
        box_count: graph.boxes.len(),
        edge_count: merged.len(),
        untraceable,
        unrendered,
        bend_over_budget,
        route_escalation: 0, // root layer has no routing phase
        identity_dangling,
    };

    if bend_over_budget > 0 {
        crate::vlog!(
            "[trace] {}: G18 bend_over_budget: {} edges exceed budget",
            layer,
            bend_over_budget
        );
    }

    // ── ★ P9-A2.5 renderdiff trace output ──

    crate::vlog!(
        "[trace] {}: R-M edge merge: {} -> {} edges",
        layer,
        before_merge,
        merged.len()
    );

    // 1. Trace edges with trunk (provenance)
    for edge in &merged {
        if let Some(ref lc) = edge.trunk {
            let trunk_name = lc.name.as_deref().unwrap_or("");
            if let Some(ref pos) = edge.source_span {
                crate::vlog!(
                    "[trace] {}: edge '{}' <- {}:{}  (trunk={})",
                    layer,
                    trunk_name,
                    pos.uri,
                    pos.offset,
                    trunk_name
                );
            } else {
                crate::vlog!(
                    "[trace] {}: edge '{}'  (trunk={})",
                    layer,
                    trunk_name,
                    trunk_name
                );
            }
        }
        if edge.lane_count > 1 {
            crate::vlog!(
                "[trace] {}: merged edge {} -> {} kind={:?} label=\"{}\" lane_count={} trunk={:?}",
                layer,
                edge.from_box,
                edge.to_box,
                edge.kind,
                edge.label,
                edge.lane_count,
                edge.trunk
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
    crate::vlog!(
        "[trace] {}: source_span coverage: {}/{} nets ({:.0}%)",
        layer,
        nets_with_span,
        total_nets,
        pct
    );

    // 3. Count nets with trunk
    let nets_with_pg = graph.nets.iter().filter(|n| n.trunk.is_some()).count();
    crate::vlog!(
        "[trace] {}: trunk coverage: {}/{} nets",
        layer,
        nets_with_pg,
        total_nets
    );

    (merged, report)
}
