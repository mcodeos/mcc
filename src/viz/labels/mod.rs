// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Milestone 8 — Label Placement Optimization
//!
//! Optimizes designator/value label positions to reduce overlaps with
//! other labels, boxes, wires, and canvas edges.
//!
//! ## Pipeline
//! ```text
//! layout + route done
//!   ↓
//! LabelPlacementModel::place(graph, canvas)
//!   ↓
//! write hints to McVecBox.label_placements
//!   ↓
//! render / metrics read placed labels
//! ```

use std::collections::BTreeMap;

use crate::vector::graph::box_def::LabelPlacementKind;
use crate::vector::graph::net_def::Segment;
use crate::vector::graph::{BoxLabelPlacement, McVecBox, McVecGraph, Symbol};

// ============================================================================
// Penalty constants
// ============================================================================

const OFF_CANVAS_PENALTY: f64 = 1_000_000.0;
const LABEL_LABEL_PENALTY: f64 = 10_000.0;
const LABEL_BOX_PENALTY: f64 = 8_000.0;
const LABEL_WIRE_PENALTY: f64 = 5_000.0;
const NON_DEFAULT_POSITION_PENALTY: f64 = 20.0;

const DESIGNATOR_FONT: f64 = 11.0;
const VALUE_FONT: f64 = 10.0;
const TEXT_WIDTH_FACTOR: f64 = 0.6;
const TEXT_HEIGHT_FACTOR: f64 = 1.2;

// ============================================================================
// LabelKey
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LabelKey {
    pub owner_box_id: i64,
    pub kind: LabelKind,
    pub index: usize,
}

// ============================================================================
// LabelKind
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LabelKind {
    Designator,
    Value,
}

// ============================================================================
// LabelPosition
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub enum LabelPosition {
    Above,
    Below,
    Left,
    Right,
    InsideTop,
    InsideCenter,
    InsideBottom,
    Hidden,
    Custom { x: f64, y: f64 },
}

impl PartialEq for LabelPosition {
    fn eq(&self, other: &Self) -> bool {
        self.discriminant() == other.discriminant()
            && match (self, other) {
                (
                    LabelPosition::Custom { x: x1, y: y1 },
                    LabelPosition::Custom { x: x2, y: y2 },
                ) => (x1 - x2).abs() < 1e-10 && (y1 - y2).abs() < 1e-10,
                _ => true,
            }
    }
}

impl Eq for LabelPosition {}

impl PartialOrd for LabelPosition {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LabelPosition {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.discriminant().cmp(&other.discriminant())
    }
}

impl LabelPosition {
    fn discriminant(&self) -> u8 {
        match self {
            LabelPosition::Above => 0,
            LabelPosition::Below => 1,
            LabelPosition::Left => 2,
            LabelPosition::Right => 3,
            LabelPosition::InsideTop => 4,
            LabelPosition::InsideCenter => 5,
            LabelPosition::InsideBottom => 6,
            LabelPosition::Hidden => 7,
            LabelPosition::Custom { .. } => 8,
        }
    }
}

// ============================================================================
// LabelRect
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl LabelRect {
    pub fn overlaps(&self, other: &LabelRect) -> bool {
        rects_overlap(
            self.x, self.y, self.w, self.h, other.x, other.y, other.w, other.h,
        )
    }

    pub fn overlaps_box(&self, b: &McVecBox) -> bool {
        rects_overlap(self.x, self.y, self.w, self.h, b.x, b.y, b.w, b.h)
    }

    pub fn overlaps_segment(&self, seg: &Segment) -> bool {
        segment_hits_rect(seg, self.x, self.y, self.w, self.h)
    }

    pub fn off_canvas(&self, canvas: (f64, f64)) -> bool {
        self.x < 0.0 || self.y < 0.0 || self.x + self.w > canvas.0 || self.y + self.h > canvas.1
    }
}

// ============================================================================
// LabelCandidate
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct LabelCandidate {
    pub key: LabelKey,
    pub position: LabelPosition,
    pub bounds: LabelRect,
    pub penalty: LabelPenalty,
    pub is_default: bool,
}

// ============================================================================
// LabelPenalty
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelPenalty {
    pub total: f64,
    pub label_overlap: usize,
    pub box_overlap: usize,
    pub wire_overlap: usize,
    pub off_canvas: bool,
    pub non_default_position: bool,
}

// ============================================================================
// PlacedLabel
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct PlacedLabel {
    pub key: LabelKey,
    pub text: String,
    pub font_size: f64,
    pub position: LabelPosition,
    pub bounds: LabelRect,
    pub inside_owner_box: bool,
}

// ============================================================================
// LabelPlacementReport
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LabelPlacementReport {
    pub labels_total: usize,
    pub labels_placed: usize,
    pub labels_hidden: usize,
    pub labels_optimized: usize,
    pub labels_kept_default: usize,
}

// ============================================================================
// LabelPlacementModel
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct LabelPlacementModel {
    pub placed: BTreeMap<LabelKey, PlacedLabel>,
    pub report: LabelPlacementReport,
}

impl LabelPlacementModel {
    /// One-shot: collect labels → generate candidates → greedy place → write hints.
    pub fn place(graph: &mut McVecGraph, canvas: (f64, f64)) -> LabelPlacementReport {
        let model = Self::run_placement(graph, canvas);
        model.write_hints(graph);
        model.report
    }

    /// Run placement without writing hints (for testing).
    pub fn run_placement(graph: &McVecGraph, canvas: (f64, f64)) -> Self {
        // ── Collect all label keys ──
        let all_labels = collect_labels(graph);
        if all_labels.is_empty() {
            return Self {
                placed: BTreeMap::new(),
                report: LabelPlacementReport::default(),
            };
        }

        let mut placed: BTreeMap<LabelKey, PlacedLabel> = BTreeMap::new();
        let mut placed_rects: Vec<(LabelKey, LabelRect, bool)> = Vec::new();
        let mut labels_optimized = 0usize;
        let mut labels_kept_default = 0usize;

        // ── Greedy placement ──
        for (key, text, font_size, is_designator) in &all_labels {
            let owner = match graph.boxes.iter().find(|b| b.id == key.owner_box_id) {
                Some(b) => b,
                None => continue,
            };

            let candidates = generate_candidates(
                key,
                text,
                *font_size,
                owner,
                *is_designator,
                canvas,
                &placed_rects,
                graph,
            );

            let best = candidates
                .into_iter()
                .min_by(|a, b| {
                    a.penalty
                        .total
                        .partial_cmp(&b.penalty.total)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.position.cmp(&b.position))
                        .then_with(|| a.key.cmp(&b.key))
                })
                .unwrap();

            let inside = matches!(
                best.position,
                LabelPosition::InsideTop
                    | LabelPosition::InsideCenter
                    | LabelPosition::InsideBottom
            );

            if best.position == LabelPosition::Hidden {
                continue;
            }

            if best.is_default {
                labels_kept_default += 1;
            } else {
                labels_optimized += 1;
            }

            let placed_label = PlacedLabel {
                key: *key,
                text: text.clone(),
                font_size: *font_size,
                position: best.position,
                bounds: best.bounds,
                inside_owner_box: inside,
            };

            placed_rects.push((*key, best.bounds, inside));
            placed.insert(*key, placed_label);
        }

        let report = LabelPlacementReport {
            labels_total: all_labels.len(),
            labels_placed: placed.len(),
            labels_hidden: all_labels.len() - placed.len(),
            labels_optimized,
            labels_kept_default,
        };

        Self { placed, report }
    }

    /// Write placement hints back to McVecBox.label_placements.
    pub fn write_hints(&self, graph: &mut McVecGraph) {
        for b in &mut graph.boxes {
            b.label_placements.clear();
            for (key, label) in &self.placed {
                if key.owner_box_id != b.id {
                    continue;
                }
                let kind = match key.kind {
                    LabelKind::Designator => LabelPlacementKind::Designator,
                    LabelKind::Value => LabelPlacementKind::Value,
                };
                let text_anchor = match label.position {
                    LabelPosition::Left | LabelPosition::Right => "start",
                    _ => "middle",
                };
                b.label_placements.push(BoxLabelPlacement {
                    text: label.text.clone(),
                    kind,
                    x: label.bounds.x,
                    y: label.bounds.y,
                    w: label.bounds.w,
                    h: label.bounds.h,
                    inside_owner_box: label.inside_owner_box,
                    font_size: label.font_size,
                    text_anchor,
                    dominant_baseline: "auto",
                });
            }
        }
    }
}

// ============================================================================
// collect_labels
// ============================================================================

fn collect_labels(graph: &McVecGraph) -> Vec<(LabelKey, String, f64, bool)> {
    let mut out = Vec::new();
    let mut index_counter: BTreeMap<i64, usize> = BTreeMap::new();

    for b in &graph.boxes {
        let mut idx = index_counter.get(&b.id).copied().unwrap_or(0);

        match b.symbol {
            Symbol::Resistor
            | Symbol::Capacitor
            | Symbol::PolarCapacitor
            | Symbol::Inductor
            | Symbol::Diode
            | Symbol::Led
            | Symbol::Zener => {
                if let Some(ref d) = b.designator {
                    if !d.is_empty() {
                        out.push((
                            LabelKey {
                                owner_box_id: b.id,
                                kind: LabelKind::Designator,
                                index: idx,
                            },
                            d.clone(),
                            DESIGNATOR_FONT,
                            true,
                        ));
                        idx += 1;
                    }
                }
                if let Some(ref v) = b.value {
                    if !v.is_empty() {
                        out.push((
                            LabelKey {
                                owner_box_id: b.id,
                                kind: LabelKind::Value,
                                index: idx,
                            },
                            v.clone(),
                            VALUE_FONT,
                            false,
                        ));
                        idx += 1;
                    }
                }
            }
            Symbol::Ic | Symbol::Module => {
                if let Some(ref d) = b.designator {
                    if !d.is_empty() {
                        out.push((
                            LabelKey {
                                owner_box_id: b.id,
                                kind: LabelKind::Designator,
                                index: idx,
                            },
                            d.clone(),
                            VALUE_FONT,
                            true,
                        ));
                        idx += 1;
                    }
                }
            }
            Symbol::PowerRail { .. } | Symbol::Dot | Symbol::Unknown => {}
        }
        index_counter.insert(b.id, idx);
    }

    out
}

// ============================================================================
// generate_candidates
// ============================================================================

fn generate_candidates(
    key: &LabelKey,
    text: &str,
    font_size: f64,
    owner: &McVecBox,
    is_designator: bool,
    canvas: (f64, f64),
    placed_rects: &[(LabelKey, LabelRect, bool)],
    graph: &McVecGraph,
) -> Vec<LabelCandidate> {
    let (w, h) = label_size(text, font_size);

    let positions = match owner.symbol {
        Symbol::Resistor
        | Symbol::Capacitor
        | Symbol::PolarCapacitor
        | Symbol::Inductor
        | Symbol::Diode
        | Symbol::Led
        | Symbol::Zener => {
            if is_designator {
                vec![
                    LabelPosition::Above,
                    LabelPosition::Below,
                    LabelPosition::Left,
                    LabelPosition::Right,
                ]
            } else {
                vec![
                    LabelPosition::Below,
                    LabelPosition::Above,
                    LabelPosition::Left,
                    LabelPosition::Right,
                ]
            }
        }
        Symbol::Ic | Symbol::Module => {
            vec![
                LabelPosition::InsideCenter,
                LabelPosition::InsideBottom,
                LabelPosition::InsideTop,
                LabelPosition::Above,
                LabelPosition::Below,
            ]
        }
        _ => return Vec::new(),
    };

    let default_pos = if is_designator {
        match owner.symbol {
            Symbol::Ic | Symbol::Module => LabelPosition::InsideCenter,
            _ => LabelPosition::Above,
        }
    } else {
        LabelPosition::Below
    };

    positions
        .into_iter()
        .map(|pos| {
            let rect = compute_rect(pos, owner, w, h, font_size);
            let penalty =
                score_candidate(&rect, pos, owner, default_pos, canvas, placed_rects, graph);
            LabelCandidate {
                key: *key,
                position: pos,
                bounds: rect,
                penalty,
                is_default: pos == default_pos,
            }
        })
        .collect()
}

// ============================================================================
// compute_rect
// ============================================================================

fn compute_rect(pos: LabelPosition, owner: &McVecBox, w: f64, h: f64, font_size: f64) -> LabelRect {
    let cx = owner.x + owner.w / 2.0;
    let cy = owner.y + owner.h / 2.0;

    match pos {
        LabelPosition::Above => LabelRect {
            x: cx - w / 2.0,
            y: owner.y - 4.0 - h,
            w,
            h,
        },
        LabelPosition::Below => LabelRect {
            x: cx - w / 2.0,
            y: owner.y + owner.h + 12.0,
            w,
            h,
        },
        LabelPosition::Left => LabelRect {
            x: owner.x - 6.0 - w,
            y: cy - h / 2.0,
            w,
            h,
        },
        LabelPosition::Right => LabelRect {
            x: owner.x + owner.w + 6.0,
            y: cy - h / 2.0,
            w,
            h,
        },
        LabelPosition::InsideTop => LabelRect {
            x: cx - w / 2.0,
            y: owner.y + 2.0,
            w,
            h,
        },
        LabelPosition::InsideCenter => {
            let baseline_y = owner.y + owner.h / 2.0 + font_size * 0.4;
            LabelRect {
                x: cx - w / 2.0,
                y: baseline_y - h,
                w,
                h,
            }
        }
        LabelPosition::InsideBottom => LabelRect {
            x: cx - w / 2.0,
            y: owner.y + owner.h - 4.0 - h,
            w,
            h,
        },
        LabelPosition::Hidden => LabelRect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        },
        LabelPosition::Custom { x, y } => LabelRect { x, y, w, h },
    }
}

// ============================================================================
// score_candidate
// ============================================================================

fn score_candidate(
    rect: &LabelRect,
    pos: LabelPosition,
    owner: &McVecBox,
    default_pos: LabelPosition,
    canvas: (f64, f64),
    placed_rects: &[(LabelKey, LabelRect, bool)],
    graph: &McVecGraph,
) -> LabelPenalty {
    let off_canvas = rect.off_canvas(canvas);
    let mut penalty = if off_canvas { OFF_CANVAS_PENALTY } else { 0.0 };

    let mut label_overlap = 0usize;
    let mut box_overlap = 0usize;
    let mut wire_overlap = 0usize;

    // Check against already placed labels
    for (_, placed, _) in placed_rects {
        if rect.overlaps(placed) {
            label_overlap += 1;
        }
    }

    // Check against all boxes
    for b in &graph.boxes {
        if b.id == owner.id {
            // IC inside labels are allowed to overlap owner
            if matches!(
                pos,
                LabelPosition::InsideTop
                    | LabelPosition::InsideCenter
                    | LabelPosition::InsideBottom
            ) {
                continue;
            }
        }
        if rect.overlaps_box(b) {
            box_overlap += 1;
        }
    }

    // Check against wire segments
    for net in &graph.nets {
        if let Some(ref route) = net.route {
            for seg in &route.segments {
                if rect.overlaps_segment(seg) {
                    wire_overlap += 1;
                    break;
                }
            }
        }
    }

    penalty += label_overlap as f64 * LABEL_LABEL_PENALTY;
    penalty += box_overlap as f64 * LABEL_BOX_PENALTY;
    penalty += wire_overlap as f64 * LABEL_WIRE_PENALTY;

    if pos != default_pos {
        penalty += NON_DEFAULT_POSITION_PENALTY;
    }

    LabelPenalty {
        total: penalty,
        label_overlap,
        box_overlap,
        wire_overlap,
        off_canvas,
        non_default_position: pos != default_pos,
    }
}

// ============================================================================
// Geometry helpers
// ============================================================================

fn label_size(text: &str, font_size: f64) -> (f64, f64) {
    let w = text.chars().count() as f64 * font_size * TEXT_WIDTH_FACTOR;
    let h = font_size * TEXT_HEIGHT_FACTOR;
    (w, h)
}

fn rects_overlap(ax: f64, ay: f64, aw: f64, ah: f64, bx: f64, by: f64, bw: f64, bh: f64) -> bool {
    ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by
}

fn segment_hits_rect(seg: &Segment, rx: f64, ry: f64, rw: f64, rh: f64) -> bool {
    let (x1, y1) = (seg.from.x, seg.from.y);
    let (x2, y2) = (seg.to.x, seg.to.y);

    if rect_contains_point(rx, ry, rw, rh, x1, y1) || rect_contains_point(rx, ry, rw, rh, x2, y2) {
        return true;
    }

    if x1 == x2 {
        if x1 >= rx && x1 <= rx + rw {
            let ymin = y1.min(y2);
            let ymax = y1.max(y2);
            return ymax >= ry && ymin <= ry + rh;
        }
    }
    if y1 == y2 {
        if y1 >= ry && y1 <= ry + rh {
            let xmin = x1.min(x2);
            let xmax = x1.max(x2);
            return xmax >= rx && xmin <= rx + rw;
        }
    }
    false
}

fn rect_contains_point(rx: f64, ry: f64, rw: f64, rh: f64, px: f64, py: f64) -> bool {
    px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
}

// ============================================================================
// One-shot convenience
// ============================================================================

/// One-shot label placement pipeline: collect → place → write hints → report.
pub fn label_placement_pipeline(
    graph: &mut McVecGraph,
    canvas: (f64, f64),
) -> LabelPlacementReport {
    LabelPlacementModel::place(graph, canvas)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::net_def::Route;
    use crate::vector::graph::{BoxKind, IoSummary, NetKind, Point, VizNet};

    fn mk_box(
        id: i64,
        name: &str,
        kind: BoxKind,
        symbol: Symbol,
        designator: Option<&str>,
        value: Option<&str>,
    ) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            symbol,
            designator.map(String::from),
            value.map(String::from),
            2,
            IoSummary::new(),
        );
        b.x = 100.0 + id as f64 * 200.0;
        b.y = 100.0;
        b.w = 60.0;
        b.h = 30.0;
        b
    }

    fn mk_ic(id: i64, name: &str, designator: Option<&str>) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::MultiPin,
            Symbol::Ic,
            designator.map(String::from),
            None,
            4,
            IoSummary::new(),
        );
        b.x = 100.0 + id as f64 * 300.0;
        b.y = 100.0;
        b.w = 120.0;
        b.h = 80.0;
        b
    }

    // ── Test: passive generates above/below candidates ──

    #[test]
    fn passive_designator_candidates() {
        let graph = McVecGraph::new(0, "test".into());
        let b = mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1"),
            Some("10k"),
        );
        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        assert!(model.placed.is_empty()); // no graph boxes

        // With graph containing the box
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(b);
        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        assert_eq!(model.placed.len(), 2);
        assert!(model.placed.values().any(|l| l.text == "R1"));
        assert!(model.placed.values().any(|l| l.text == "10k"));
    }

    // ── Test: all candidates within canvas ──

    #[test]
    fn all_candidates_within_canvas() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1"),
            Some("10k"),
        ));
        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        for label in model.placed.values() {
            assert!(
                !label.bounds.off_canvas((800.0, 600.0)),
                "label '{}' at ({}, {}) off canvas",
                label.text,
                label.bounds.x,
                label.bounds.y
            );
        }
    }

    // ── Test: two adjacent passive labels don't overlap ──

    #[test]
    fn adjacent_passive_labels_no_overlap() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut b1 = mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1"),
            Some("10k"),
        );
        b1.x = 100.0;
        b1.y = 100.0;
        let mut b2 = mk_box(
            2,
            "R2",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R2"),
            Some("22k"),
        );
        b2.x = 120.0;
        b2.y = 100.0;
        graph.boxes.push(b1);
        graph.boxes.push(b2);

        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        let labels: Vec<&PlacedLabel> = model.placed.values().collect();
        for i in 0..labels.len() {
            for j in (i + 1)..labels.len() {
                assert!(
                    !labels[i].bounds.overlaps(&labels[j].bounds),
                    "labels '{}' and '{}' overlap",
                    labels[i].text,
                    labels[j].text
                );
            }
        }
    }

    // ── Test: label avoids wire ──

    #[test]
    fn label_avoids_wire() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut b = mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1"),
            Some("10k"),
        );
        b.x = 100.0;
        b.y = 100.0;
        graph.boxes.push(b);

        // Add a wire running right through the default designator position (above)
        let mut net = VizNet::new(1, "SIG".into(), NetKind::Signal, vec![]);
        net.route = Some(Route {
            segments: vec![Segment {
                from: Point { x: 80.0, y: 85.0 },
                to: Point { x: 180.0, y: 85.0 },
            }],
            junctions: vec![],
        });
        graph.nets.push(net);

        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        let designator = model.placed.values().find(|l| l.text == "R1").unwrap();
        // Should not overlap the wire
        let wire_rect = LabelRect {
            x: 80.0,
            y: 83.0,
            w: 100.0,
            h: 4.0,
        };
        assert!(
            !designator.bounds.overlaps(&wire_rect),
            "designator should avoid wire"
        );
    }

    // ── Test: IC designator inside box ──

    #[test]
    fn ic_designator_inside_box() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_ic(1, "U1", Some("U1")));
        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        let label = &model.placed.values().next().unwrap();
        assert!(label.bounds.x >= 100.0);
        assert!(label.bounds.y >= 100.0);
    }

    // ── Test: write_hints stores on McVecBox ──

    #[test]
    fn write_hints_stores_on_box() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1"),
            Some("10k"),
        ));
        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        model.write_hints(&mut graph);

        assert!(!graph.boxes[0].label_placements.is_empty());
        let hint = &graph.boxes[0].label_placements[0];
        assert!(hint.w > 0.0);
        assert!(hint.h > 0.0);
    }

    // ── Test: deterministic ──

    #[test]
    fn placement_deterministic() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1"),
            Some("10k"),
        ));
        let a = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        let b = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        assert_eq!(a, b);
    }

    // ── Test: empty graph ──

    #[test]
    fn empty_graph_no_labels() {
        let graph = McVecGraph::new(0, "test".into());
        let model = LabelPlacementModel::run_placement(&graph, (800.0, 600.0));
        assert!(model.placed.is_empty());
    }

    // ── Test: one-shot pipeline ──

    #[test]
    fn label_placement_pipeline_smoke() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1"),
            Some("10k"),
        ));
        let report = label_placement_pipeline(&mut graph, (800.0, 600.0));
        assert!(report.labels_placed == 2);
        assert!(!graph.boxes[0].label_placements.is_empty());
    }

    // ── Test: rects_overlap correct ──

    #[test]
    fn rects_overlap_correct() {
        let a = LabelRect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let b = LabelRect {
            x: 5.0,
            y: 5.0,
            w: 10.0,
            h: 10.0,
        };
        let c = LabelRect {
            x: 20.0,
            y: 20.0,
            w: 10.0,
            h: 10.0,
        };
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }

    // ── Test: segment_hits_rect ──

    #[test]
    fn segment_hits_rect_correct() {
        let rect = LabelRect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let hit = Segment {
            from: Point { x: 5.0, y: -5.0 },
            to: Point { x: 5.0, y: 15.0 },
        };
        let miss = Segment {
            from: Point { x: 20.0, y: 20.0 },
            to: Point { x: 30.0, y: 30.0 },
        };
        assert!(rect.overlaps_segment(&hit));
        assert!(!rect.overlaps_segment(&miss));
    }

    // ── Test: off_canvas ──

    #[test]
    fn off_canvas_detection() {
        let r = LabelRect {
            x: -1.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        assert!(r.off_canvas((800.0, 600.0)));
        let r2 = LabelRect {
            x: 100.0,
            y: 100.0,
            w: 10.0,
            h: 10.0,
        };
        assert!(!r2.off_canvas((800.0, 600.0)));
    }
}
