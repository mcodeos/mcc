// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Milestone 7 — Pin Anchor Model
//!
//! Unified pin side / offset / anchor model that replaces scattered
//! entry_points + pin_place logic with a single, testable, reportable
//! `PinAnchorModel`.
//!
//! ## Pipeline
//! ```text
//! McVecGraph + optional SemanticModel
//!   ↓
//! PinAnchorModel::build(graph, semantic, config)
//!   ↓
//! PinAnchorModel::apply_to_graph(graph)
//!   ↓
//! existing router / renderer (via EntryPoint)
//! ```
//!
//! ## Design
//! - EntryPoint is NOT removed — it is the compatibility output of the model.
//! - Default FlowLayouter is NOT forced to use the model in M7 first version.
//! - LayeredLayouter experimental path uses PinAnchorModel.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::vector::graph::box_def::EntryPoint;
use crate::vector::graph::net_def::IoDirection;
use crate::vector::graph::{EntrySide, McVecGraph};

use crate::viz::layout::entry_points::{
    collect_box_centers, collect_pin_io_types, collect_pin_neighbors, enforce_unique_offsets,
    promote_synthetic_pins, split_shared_pins,
};
use crate::viz::layout::flow::pin_abs;
use crate::viz::semantic::SemanticModel;

// ============================================================================
// PinKey
// ============================================================================

/// Unique key for a pin within a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PinKey {
    pub box_id: i64,
    pub pin_id: i64,
}

impl PinKey {
    pub fn new(box_id: i64, pin_id: i64) -> Self {
        Self { box_id, pin_id }
    }
}

// ============================================================================
// PinAbsPoint
// ============================================================================

/// Absolute (x, y) position of a pin anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinAbsPoint {
    pub x: f64,
    pub y: f64,
}

// ============================================================================
// PinAnchorSource
// ============================================================================

/// Source of a pin anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinAnchorSource {
    PhysicalPin,
    NetEndpoint,
    SyntheticEndpoint,
    SplitSharedPin,
    ReconciledMissingEndpoint,
}

// ============================================================================
// PinAnchorWarning
// ============================================================================

/// A warning produced during pin anchor model construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinAnchorWarning {
    pub key: Option<PinKey>,
    pub message: String,
}

// ============================================================================
// PinAnchor
// ============================================================================

/// A single pin anchor with side, offset, and absolute position.
#[derive(Debug, Clone, PartialEq)]
pub struct PinAnchor {
    pub key: PinKey,
    pub pin_name: String,
    pub io_direction: IoDirection,

    pub source: PinAnchorSource,
    pub intent_side: Option<EntrySide>,
    pub assigned_side: EntrySide,
    pub offset: f64,
    pub abs: Option<PinAbsPoint>,

    pub fixed_by_author: bool,
    pub synthetic: bool,
    pub split: bool,
}

// ============================================================================
// BoxAnchorSummary
// ============================================================================

/// Per-box summary of anchor state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoxAnchorSummary {
    pub box_id: i64,
    pub anchors_total: usize,
    pub left: usize,
    pub right: usize,
    pub top: usize,
    pub bottom: usize,
    pub duplicate_offsets: usize,
    pub missing_physical_pins: usize,
    pub missing_endpoint_pins: usize,
}

// ============================================================================
// PinAnchorReport
// ============================================================================

/// Aggregate report for the pin anchor model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PinAnchorReport {
    pub boxes_total: usize,
    pub anchors_total: usize,
    pub physical_pins_total: usize,
    pub endpoints_total: usize,

    pub endpoint_anchors_missing: usize,
    pub physical_pin_anchors_missing: usize,
    pub duplicate_side_offsets: usize,
    pub anchors_off_box: usize,

    pub authored_side_total: usize,
    pub authored_side_honored: usize,
    pub semantic_side_total: usize,
    pub semantic_side_honored: usize,
}

// ============================================================================
// PinAnchorConfig
// ============================================================================

/// Configuration for building the pin anchor model.
#[derive(Debug, Clone)]
pub struct PinAnchorConfig {
    pub lr_only: bool,
    pub hub_id: Option<i64>,
    pub hub_keep_semantic: bool,
    pub allow_power_ground_top_bottom: bool,
    pub align_hub_to_spokes: bool,
    pub straighten_facing_pairs: bool,
}

impl Default for PinAnchorConfig {
    fn default() -> Self {
        Self {
            lr_only: true,
            hub_id: None,
            hub_keep_semantic: false,
            allow_power_ground_top_bottom: true,
            align_hub_to_spokes: true,
            straighten_facing_pairs: true,
        }
    }
}

// ============================================================================
// PinAnchorModel
// ============================================================================

/// The unified pin anchor model.
#[derive(Debug, Clone, PartialEq)]
pub struct PinAnchorModel {
    pub anchors: BTreeMap<PinKey, PinAnchor>,
    pub boxes: BTreeMap<i64, BoxAnchorSummary>,
    pub warnings: Vec<PinAnchorWarning>,
    pub report: PinAnchorReport,
}

impl PinAnchorModel {
    /// Build the pin anchor model from a graph and optional semantic model.
    pub fn build(
        graph: &McVecGraph,
        _semantic: Option<&SemanticModel>,
        config: &PinAnchorConfig,
    ) -> Self {
        let warnings: Vec<PinAnchorWarning> = Vec::new();
        let mut anchors: BTreeMap<PinKey, PinAnchor> = BTreeMap::new();

        // ── Phase 1: Collect all physical pins ──
        let mut physical_count = 0usize;
        for b in &graph.boxes {
            for pin in &b.pins {
                let key = PinKey::new(b.id, pin.id);
                let io = pin.io;
                let intent = intent_side_from_io(io, config.allow_power_ground_top_bottom);
                let assigned = if config.lr_only {
                    project_to_lr(intent.clone().unwrap_or(EntrySide::Left))
                } else {
                    intent.clone().unwrap_or(EntrySide::Left)
                };
                anchors.insert(
                    key,
                    PinAnchor {
                        key,
                        pin_name: pin.pin_id.clone(),
                        io_direction: io,
                        source: PinAnchorSource::PhysicalPin,
                        intent_side: intent,
                        assigned_side: assigned,
                        offset: 0.5,
                        abs: None,
                        fixed_by_author: false,
                        synthetic: false,
                        split: false,
                    },
                );
                physical_count += 1;
            }
        }

        // ── Phase 2: Collect endpoint pins (from nets) ──
        let mut endpoint_count = 0usize;
        let mut endpoint_keys: HashSet<PinKey> = HashSet::new();
        for net in &graph.nets {
            for ep in &net.endpoints {
                let key = PinKey::new(ep.box_id, ep.pin_id);
                endpoint_keys.insert(key);
                if !anchors.contains_key(&key) {
                    let io = ep.io_type;
                    let intent = intent_side_from_io(io, config.allow_power_ground_top_bottom);
                    let assigned = if config.lr_only {
                        project_to_lr(intent.clone().unwrap_or(EntrySide::Left))
                    } else {
                        intent.clone().unwrap_or(EntrySide::Left)
                    };
                    anchors.insert(
                        key,
                        PinAnchor {
                            key,
                            pin_name: ep.pin_name.clone(),
                            io_direction: io,
                            source: PinAnchorSource::NetEndpoint,
                            intent_side: intent,
                            assigned_side: assigned,
                            offset: 0.5,
                            abs: None,
                            fixed_by_author: false,
                            synthetic: false,
                            split: false,
                        },
                    );
                }
                endpoint_count += 1;
            }
        }

        // ── Phase 3: Collect existing entry_points as fallback anchors ──
        for b in &graph.boxes {
            for ep in &b.entry_points {
                let key = PinKey::new(b.id, ep.pin_id);
                if !anchors.contains_key(&key) {
                    anchors.insert(
                        key,
                        PinAnchor {
                            key,
                            pin_name: ep.pin_name.clone(),
                            io_direction: IoDirection::Unknown,
                            source: PinAnchorSource::ReconciledMissingEndpoint,
                            intent_side: None,
                            assigned_side: ep.side.clone(),
                            offset: ep.offset,
                            abs: None,
                            fixed_by_author: false,
                            synthetic: false,
                            split: false,
                        },
                    );
                }
            }
        }

        // ── Phase 4: Connectivity-driven side assignment ──
        let box_centers = collect_box_centers(graph);
        let _io_types = collect_pin_io_types(graph);
        let neighbors = collect_pin_neighbors(graph);

        for (key, anchor) in anchors.iter_mut() {
            if anchor.fixed_by_author {
                continue;
            }
            // Hub pins: if hub_keep_semantic and this is hub, skip connectivity
            if config.hub_keep_semantic {
                if let Some(hub) = config.hub_id {
                    if key.box_id == hub {
                        continue;
                    }
                }
            }
            // Try connectivity-based side
            let neighbor_key = (key.box_id, key.pin_id);
            if let Some(nbr_ids) = neighbors.get(&neighbor_key) {
                if let Some(&nbr_id) = nbr_ids.first() {
                    if let (Some(center), Some(nbr_center)) =
                        (box_centers.get(&key.box_id), box_centers.get(&nbr_id))
                    {
                        let dx = nbr_center.0 - center.0;
                        let dy = nbr_center.1 - center.1;
                        let conn_side = pick_side_by_direction(dx, dy);
                        if config.lr_only {
                            anchor.assigned_side = project_to_lr(conn_side);
                        } else {
                            anchor.assigned_side = conn_side;
                        }
                    }
                }
            }
        }

        // ── Phase 5: Assign offsets within each box side ──
        assign_offsets_per_box_side(&mut anchors, graph);

        // ── Phase 6: Compute absolute points ──
        for (key, anchor) in anchors.iter_mut() {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == key.box_id) {
                let (x, y) = pin_abs(b, &anchor.assigned_side, anchor.offset);
                anchor.abs = Some(PinAbsPoint { x, y });
            }
        }

        // ── Phase 7: Build box summaries ──
        let mut box_summaries: BTreeMap<i64, BoxAnchorSummary> = BTreeMap::new();
        for b in &graph.boxes {
            let mut summary = BoxAnchorSummary {
                box_id: b.id,
                ..Default::default()
            };
            let box_anchors: Vec<&PinAnchor> =
                anchors.values().filter(|a| a.key.box_id == b.id).collect();
            summary.anchors_total = box_anchors.len();
            for a in &box_anchors {
                match a.assigned_side {
                    EntrySide::Left => summary.left += 1,
                    EntrySide::Right => summary.right += 1,
                    EntrySide::Top => summary.top += 1,
                    EntrySide::Bottom => summary.bottom += 1,
                }
            }
            // Check missing physical pins
            summary.missing_physical_pins = b
                .pins
                .iter()
                .filter(|p| !anchors.contains_key(&PinKey::new(b.id, p.id)))
                .count();
            // Check missing endpoint pins
            let box_ep_keys: HashSet<PinKey> = graph
                .nets
                .iter()
                .flat_map(|n| n.endpoints.iter())
                .filter(|e| e.box_id == b.id)
                .map(|e| PinKey::new(e.box_id, e.pin_id))
                .collect();
            summary.missing_endpoint_pins = box_ep_keys
                .iter()
                .filter(|k| !anchors.contains_key(k))
                .count();
            // Check duplicate offsets
            let mut seen: HashMap<EntrySide, HashSet<u64>> = HashMap::new();
            let mut dupes = 0usize;
            for a in &box_anchors {
                let bucket = (a.offset * 1_000_000.0) as u64;
                if seen
                    .entry(a.assigned_side.clone())
                    .or_default()
                    .insert(bucket)
                    == false
                {
                    dupes += 1;
                }
            }
            summary.duplicate_offsets = dupes;
            box_summaries.insert(b.id, summary);
        }

        // ── Phase 8: Build report ──
        let report = PinAnchorReport {
            boxes_total: graph.boxes.len(),
            anchors_total: anchors.len(),
            physical_pins_total: physical_count,
            endpoints_total: endpoint_count,
            endpoint_anchors_missing: endpoint_keys
                .iter()
                .filter(|k| !anchors.contains_key(k))
                .count(),
            physical_pin_anchors_missing: graph
                .boxes
                .iter()
                .flat_map(|b| b.pins.iter().map(move |p| (b.id, p.id)))
                .filter(|(bid, pid)| !anchors.contains_key(&PinKey::new(*bid, *pid)))
                .count(),
            duplicate_side_offsets: box_summaries.values().map(|s| s.duplicate_offsets).sum(),
            anchors_off_box: 0,
            authored_side_total: 0,
            authored_side_honored: 0,
            semantic_side_total: 0,
            semantic_side_honored: 0,
        };

        PinAnchorModel {
            anchors,
            boxes: box_summaries,
            warnings,
            report,
        }
    }

    /// Apply the model back to graph entry_points.
    /// This is the compatibility layer: router and renderer still use EntryPoint.
    pub fn apply_to_graph(&self, graph: &mut McVecGraph) {
        for b in &mut graph.boxes {
            let mut new_eps: Vec<EntryPoint> = Vec::new();
            for anchor in self.anchors.values() {
                if anchor.key.box_id != b.id {
                    continue;
                }
                new_eps.push(EntryPoint {
                    pin_id: anchor.key.pin_id,
                    pin_name: anchor.pin_name.clone(),
                    side: anchor.assigned_side.clone(),
                    offset: anchor.offset,
                });
            }
            // Sort by stable key: side then offset
            new_eps.sort_by(|a, b| {
                side_order(a.side.clone())
                    .cmp(&side_order(b.side.clone()))
                    .then_with(|| {
                        a.offset
                            .partial_cmp(&b.offset)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
            b.entry_points = new_eps;
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Pick intent side from IO direction.
fn intent_side_from_io(io: IoDirection, allow_top_bottom: bool) -> Option<EntrySide> {
    match io {
        IoDirection::Power => {
            if allow_top_bottom {
                Some(EntrySide::Top)
            } else {
                None
            }
        }
        IoDirection::Ground => {
            if allow_top_bottom {
                Some(EntrySide::Bottom)
            } else {
                None
            }
        }
        IoDirection::Input => Some(EntrySide::Left),
        IoDirection::Output => Some(EntrySide::Right),
        _ => None,
    }
}

/// Project top/bottom to left/right for lr_only mode.
fn project_to_lr(side: EntrySide) -> EntrySide {
    match side {
        EntrySide::Top | EntrySide::Bottom => EntrySide::Left,
        other => other,
    }
}

/// Pick side by direction vector.
fn pick_side_by_direction(dx: f64, dy: f64) -> EntrySide {
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            EntrySide::Right
        } else {
            EntrySide::Left
        }
    } else {
        if dy >= 0.0 {
            EntrySide::Bottom
        } else {
            EntrySide::Top
        }
    }
}

/// Stable ordering for EntrySide.
fn side_order(side: EntrySide) -> u8 {
    match side {
        EntrySide::Top => 0,
        EntrySide::Right => 1,
        EntrySide::Bottom => 2,
        EntrySide::Left => 3,
    }
}

/// Assign offsets evenly within each box×side group.
fn assign_offsets_per_box_side(anchors: &mut BTreeMap<PinKey, PinAnchor>, _graph: &McVecGraph) {
    let mut groups: HashMap<(i64, EntrySide), Vec<PinKey>> = HashMap::new();
    for (key, anchor) in anchors.iter() {
        groups
            .entry((key.box_id, anchor.assigned_side.clone()))
            .or_default()
            .push(*key);
    }
    for (_, keys) in groups.iter_mut() {
        keys.sort();
        let n = keys.len() as f64;
        for (i, key) in keys.iter().enumerate() {
            if let Some(anchor) = anchors.get_mut(key) {
                anchor.offset = (i as f64 + 0.5) / n;
            }
        }
    }
}

// ============================================================================
// One-shot convenience
// ============================================================================

/// One-shot pin anchor pipeline: repair → build → apply → report.
pub fn pin_anchor_pipeline(
    graph: &mut McVecGraph,
    semantic: Option<&SemanticModel>,
    config: PinAnchorConfig,
) -> PinAnchorReport {
    // Identity repair
    promote_synthetic_pins(graph);
    split_shared_pins(graph);

    // Build model
    let model = PinAnchorModel::build(graph, semantic, &config);

    // Apply to graph
    model.apply_to_graph(graph);

    // Enforce uniqueness
    enforce_unique_offsets(graph);

    model.report
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::BoxPin;
    use crate::vector::graph::net_def::EndpointRef;
    use crate::vector::graph::{BoxKind, McVecBox, McVecGraph, NetKind, Symbol, VizNet};

    fn mk_box(id: i64, name: &str, kind: BoxKind, symbol: Symbol, pin_count: usize) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            symbol,
            None,
            None,
            pin_count,
            crate::vector::graph::box_def::IoSummary::new(),
        );
        b.x = 100.0 + id as f64 * 200.0;
        b.y = 100.0;
        b.w = 60.0;
        b.h = 40.0;
        b
    }

    fn add_pin(b: &mut McVecBox, pin_id: i64, name: &str, io: IoDirection) {
        b.pins.push(BoxPin {
            id: pin_id,
            pin_id: name.into(),
            description: name.into(),
            io,
        });
    }

    fn ep(box_id: i64, pin_id: i64, pin_name: &str, io: IoDirection) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, pin_name, io)
    }

    // ── Test: power pin intent top ──

    #[test]
    fn power_pin_intent_top() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 2);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        graph.boxes.push(ic);

        let config = PinAnchorConfig::default();
        let model = PinAnchorModel::build(&graph, None, &config);

        let vcc = &model.anchors[&PinKey::new(1, 1)];
        assert_eq!(vcc.intent_side, Some(EntrySide::Top));
        let gnd = &model.anchors[&PinKey::new(1, 2)];
        assert_eq!(gnd.intent_side, Some(EntrySide::Bottom));
    }

    // ── Test: input left, output right ──

    #[test]
    fn input_left_output_right() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let config = PinAnchorConfig::default();
        let model = PinAnchorModel::build(&graph, None, &config);

        let inp = &model.anchors[&PinKey::new(1, 3)];
        assert_eq!(inp.intent_side, Some(EntrySide::Left));
        let out = &model.anchors[&PinKey::new(1, 4)];
        assert_eq!(out.intent_side, Some(EntrySide::Right));
    }

    // ── Test: lr_only projects top/bottom to left/right ──

    #[test]
    fn lr_only_projects_top_bottom() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 2);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        graph.boxes.push(ic);

        let config = PinAnchorConfig {
            lr_only: true,
            ..Default::default()
        };
        let model = PinAnchorModel::build(&graph, None, &config);

        let vcc = &model.anchors[&PinKey::new(1, 1)];
        assert_eq!(vcc.assigned_side, EntrySide::Left);
        let gnd = &model.anchors[&PinKey::new(1, 2)];
        assert_eq!(gnd.assigned_side, EntrySide::Left);
    }

    // ── Test: offsets unique per side ──

    #[test]
    fn offsets_unique_per_side() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4);
        add_pin(&mut ic, 1, "A", IoDirection::Input);
        add_pin(&mut ic, 2, "B", IoDirection::Input);
        add_pin(&mut ic, 3, "C", IoDirection::Input);
        add_pin(&mut ic, 4, "D", IoDirection::Input);
        graph.boxes.push(ic);

        let config = PinAnchorConfig::default();
        let model = PinAnchorModel::build(&graph, None, &config);

        // All input pins should be on left side
        let offsets: Vec<f64> = model
            .anchors
            .values()
            .filter(|a| a.key.box_id == 1 && a.assigned_side == EntrySide::Left)
            .map(|a| a.offset)
            .collect();

        let mut sorted = offsets.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        assert_eq!(offsets.len(), sorted.len(), "duplicate offsets detected");
        for &o in &offsets {
            assert!(o > 0.0 && o < 1.0, "offset {} out of (0,1)", o);
        }
    }

    // ── Test: deterministic ──

    #[test]
    fn model_deterministic() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let config = PinAnchorConfig::default();
        let a = PinAnchorModel::build(&graph, None, &config);
        let b = PinAnchorModel::build(&graph, None, &config);
        assert_eq!(a, b);
    }

    // ── Test: geometry left anchor x == box.x ──

    #[test]
    fn left_anchor_at_box_left() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 1);
        add_pin(&mut ic, 1, "IN", IoDirection::Input);
        let ic_x = ic.x;
        graph.boxes.push(ic);

        let config = PinAnchorConfig::default();
        let model = PinAnchorModel::build(&graph, None, &config);

        let anchor = &model.anchors[&PinKey::new(1, 1)];
        assert_eq!(anchor.assigned_side, EntrySide::Left);
        let abs = anchor.abs.unwrap();
        assert!(
            (abs.x - ic_x).abs() < 0.001,
            "x={} != box.x={}",
            abs.x,
            ic_x
        );
    }

    // ── Test: apply_to_graph writes entry_points ──

    #[test]
    fn apply_to_graph_writes_entry_points() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let config = PinAnchorConfig::default();
        let model = PinAnchorModel::build(&graph, None, &config);
        model.apply_to_graph(&mut graph);

        assert_eq!(graph.boxes[0].entry_points.len(), 4);
        for ep in &graph.boxes[0].entry_points {
            assert!(ep.offset > 0.0 && ep.offset < 1.0);
        }
    }

    // ── Test: all endpoints have anchors ──

    #[test]
    fn all_endpoints_have_anchors() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let mut r = mk_box(2, "R1", BoxKind::TwoPin, Symbol::Resistor, 2);
        add_pin(&mut r, 1, "1", IoDirection::Passive);
        add_pin(&mut r, 2, "2", IoDirection::Passive);
        graph.boxes.push(r);

        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(1, 4, "OUT", IoDirection::Output),
                ep(2, 1, "1", IoDirection::Input),
            ],
        ));

        let config = PinAnchorConfig::default();
        let model = PinAnchorModel::build(&graph, None, &config);

        assert_eq!(model.report.endpoint_anchors_missing, 0);
    }

    // ── Test: pin anchor pipeline one-shot ──

    #[test]
    fn pin_anchor_pipeline_smoke() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let report = pin_anchor_pipeline(&mut graph, None, PinAnchorConfig::default());
        assert!(report.anchors_total >= 4);
        assert!(report.duplicate_side_offsets == 0);
        assert!(!graph.boxes[0].entry_points.is_empty());
    }

    // ── Test: box anchor summary ──

    #[test]
    fn box_anchor_summary_counts() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let config = PinAnchorConfig::default();
        let model = PinAnchorModel::build(&graph, None, &config);

        let summary = &model.boxes[&1];
        assert_eq!(summary.anchors_total, 4);
        assert_eq!(summary.missing_physical_pins, 0);

        // With lr_only, power/ground project to left
        assert!(summary.left >= 2);
        assert!(summary.right >= 1);
    }
}
