//! Iteration 01 · Acceptance yardstick: electrical fidelity (hard gate) + readability score (for ranking).
//!
//! - FidelityReport.is_perfect() = hard gate for all subsequent iterations.
//! - ReadabilityScore.weighted() = score for generate-and-rank (iteration 04).
//! - MetricsAccumulator passes through viz::api::render_layer_recursive, accumulating per layer.

use crate::vector::builder::builder_report::BuilderReport;
use crate::vector::graph::box_def::{EntryPoint, EntrySide, McVecBox};
use crate::vector::graph::net_def::{Point, Route, Segment};
use crate::vector::graph::{McVecGraph, NetKind};
use crate::viz::render::label_render::{designator_value_label_bounds, LabelBounds};
use crate::viz::route::audit::CollisionReport;

/// Alignment grid for off-grid penalty (no coordinate snapping in this codebase; soft alignment signal, tunable).
pub const GRID: f64 = 10.0;

// ============================================================================
// Electrical fidelity — hard gate, must be all green
// ============================================================================
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FidelityReport {
    pub nets_total: usize,
    pub nets_rendered: usize,
    pub nets_dropped: usize,
    pub nets_partial: usize,
    pub pins_total: usize,
    pub pins_rendered: usize,
    pub bus_bits_total: usize,
    pub bus_bits_paired_ok: usize,
    pub authored_sides_total: usize,
    pub authored_sides_honored: usize,
    pub box_box: usize,
    pub wire_box: usize,
}

impl FidelityReport {
    /// Hard gate: all electrical dimensions green.
    pub fn is_perfect(&self) -> bool {
        self.nets_dropped == 0
            && self.nets_partial == 0
            && self.pins_rendered == self.pins_total
            && self.bus_bits_paired_ok == self.bus_bits_total
            && self.authored_sides_honored == self.authored_sides_total
            && self.box_box == 0
            && self.wire_box == 0
    }

    pub fn report_line(&self) -> String {
        format!(
            "[metrics] FIDELITY: nets {}/{} rendered ({} dropped, {} partial), \
             pins {}/{}, bus-bits {}/{}, authored-sides {}/{}, box_box={}, wire_box={}  -> PERFECT? {}",
            self.nets_rendered, self.nets_total, self.nets_dropped, self.nets_partial,
            self.pins_rendered, self.pins_total,
            self.bus_bits_paired_ok, self.bus_bits_total,
            self.authored_sides_honored, self.authored_sides_total,
            self.box_box, self.wire_box,
            self.is_perfect()
        )
    }
}

// ============================================================================
// Readability score — lower is better, for ranking/comparison
// ============================================================================
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadabilityScore {
    pub wire_wire: usize,
    pub total_wirelength: f64,
    pub total_bends: usize,
    pub off_grid_penalty: f64,
    pub symmetry_penalty: f64, // [P1 placeholder] wired in 06, always 0 for now
    pub idiom_violation: usize, // [P1 placeholder] wired in 06, always 0 for now
}

impl ReadabilityScore {
    /// Single scalar for generate-and-rank (dimensions copied from obstacles::score_path: collision 1000 >> length).
    pub fn weighted(&self) -> f64 {
        self.wire_wire as f64 * 1000.0
            + self.total_wirelength
            + self.total_bends as f64 * 20.0
            + self.off_grid_penalty * 5.0
            + self.symmetry_penalty * 50.0
            + self.idiom_violation as f64 * 200.0
    }

    pub fn report_line(&self) -> String {
        format!(
            "[metrics] READABILITY: wire_wire={}, wirelen={:.1}, bends={}, off_grid={:.1} -> weighted={:.1}",
            self.wire_wire, self.total_wirelength, self.total_bends, self.off_grid_penalty, self.weighted()
        )
    }
}

// ============================================================================
// Unified schematic quality report — Milestone 1 acceptance report
// ============================================================================
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BuilderQualitySummary {
    pub resolutions_total: usize,
    pub resolution_success_rate: f64,
    pub dropped_nets: usize,
    pub partial_nets: usize,
    pub unresolved_modules: usize,
    pub warnings: usize,
}

impl BuilderQualitySummary {
    pub fn from_report(report: Option<&BuilderReport>) -> Self {
        match report {
            Some(r) => Self {
                resolutions_total: r.resolutions.len(),
                resolution_success_rate: r.success_rate(),
                dropped_nets: r.dropped_nets.len(),
                partial_nets: r.partial_nets.len(),
                unresolved_modules: r.unresolved_modules.len(),
                warnings: r.warn_count(),
            },
            None => Self {
                resolution_success_rate: 1.0,
                ..Self::default()
            },
        }
    }

    pub fn report_line(&self) -> String {
        format!(
            "[metrics] BUILDER: resolutions={} success={:.1}%, dropped={} partial={} unresolved_modules={} warnings={}",
            self.resolutions_total,
            self.resolution_success_rate * 100.0,
            self.dropped_nets,
            self.partial_nets,
            self.unresolved_modules,
            self.warnings
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SchematicQualityReport {
    pub fidelity: FidelityReport,
    pub readability: ReadabilityScore,
    pub collisions: CollisionReport,
    pub builder: BuilderQualitySummary,
    pub truth: TruthSnapshotReport,
    pub visual: VisualQualityReport,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisualQualityReport {
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub canvas_area: f64,

    pub boxes_total: usize,
    pub box_area_total: f64,
    pub box_density: f64,

    pub labels_total: usize,
    pub label_label_overlaps: usize,
    pub label_box_overlaps: usize,
    pub label_wire_overlaps: usize,
    pub labels_off_canvas: usize,

    pub routed_nets: usize,
    pub route_segments_total: usize,
    pub route_bends_total: usize,
    pub route_length_total: f64,
    pub avg_segments_per_routed_net: f64,
    pub avg_bends_per_routed_net: f64,
    pub avg_route_length_per_routed_net: f64,

    pub symmetry_penalty: f64,
    pub idiom_violations: usize,
}

impl VisualQualityReport {
    pub fn merge(&mut self, other: VisualQualityReport) {
        self.canvas_width = self.canvas_width.max(other.canvas_width);
        self.canvas_height = self.canvas_height.max(other.canvas_height);
        self.canvas_area += other.canvas_area;
        self.boxes_total += other.boxes_total;
        self.box_area_total += other.box_area_total;
        self.labels_total += other.labels_total;
        self.label_label_overlaps += other.label_label_overlaps;
        self.label_box_overlaps += other.label_box_overlaps;
        self.label_wire_overlaps += other.label_wire_overlaps;
        self.labels_off_canvas += other.labels_off_canvas;
        self.routed_nets += other.routed_nets;
        self.route_segments_total += other.route_segments_total;
        self.route_bends_total += other.route_bends_total;
        self.route_length_total += other.route_length_total;
        self.symmetry_penalty += other.symmetry_penalty;
        self.idiom_violations += other.idiom_violations;
        self.recompute_derived();
    }

    fn recompute_derived(&mut self) {
        self.box_density = if self.canvas_area > 0.0 {
            self.box_area_total / self.canvas_area
        } else {
            0.0
        };
        if self.routed_nets > 0 {
            let n = self.routed_nets as f64;
            self.avg_segments_per_routed_net = self.route_segments_total as f64 / n;
            self.avg_bends_per_routed_net = self.route_bends_total as f64 / n;
            self.avg_route_length_per_routed_net = self.route_length_total / n;
        } else {
            self.avg_segments_per_routed_net = 0.0;
            self.avg_bends_per_routed_net = 0.0;
            self.avg_route_length_per_routed_net = 0.0;
        }
    }

    pub fn report_lines(&self) -> Vec<String> {
        vec![
            format!(
                "[metrics] VISUAL: canvas={:.1}x{:.1} area={:.1} boxes={} box_area={:.1} density={:.3} labels={} label_label={} label_box={} label_wire={} off_canvas={}",
                self.canvas_width,
                self.canvas_height,
                self.canvas_area,
                self.boxes_total,
                self.box_area_total,
                self.box_density,
                self.labels_total,
                self.label_label_overlaps,
                self.label_box_overlaps,
                self.label_wire_overlaps,
                self.labels_off_canvas
            ),
            format!(
                "[metrics] ROUTE-QUALITY: routed_nets={} segments={} bends={} wirelen={:.1} avg_segments={:.2} avg_bends={:.2} avg_len={:.1}",
                self.routed_nets,
                self.route_segments_total,
                self.route_bends_total,
                self.route_length_total,
                self.avg_segments_per_routed_net,
                self.avg_bends_per_routed_net,
                self.avg_route_length_per_routed_net
            ),
            format!(
                "[metrics] IDIOM: symmetry={:.1} violations={}",
                self.symmetry_penalty, self.idiom_violations
            ),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TruthSnapshotReport {
    pub layers_total: usize,

    pub nets_total: usize,
    pub drawable_nets_total: usize,
    pub routed_nets_total: usize,
    pub nets_missing_route: usize,
    pub nets_empty_route: usize,

    pub endpoints_total: usize,
    pub drawable_endpoints_total: usize,
    pub endpoints_box_missing: usize,
    pub endpoints_pin_missing: usize,
    pub endpoints_entry_missing: usize,
    pub endpoints_route_unreached: usize,

    pub boxes_total: usize,
    pub physical_pins_total: usize,
    pub physical_pins_with_entry: usize,
    pub physical_pins_missing_entry: usize,
}

impl TruthSnapshotReport {
    pub fn is_perfect(&self) -> bool {
        self.nets_missing_route == 0
            && self.nets_empty_route == 0
            && self.endpoints_box_missing == 0
            && self.endpoints_pin_missing == 0
            && self.endpoints_entry_missing == 0
            && self.endpoints_route_unreached == 0
            && self.physical_pins_missing_entry == 0
    }

    pub fn merge(&mut self, other: TruthSnapshotReport) {
        self.layers_total += other.layers_total;
        self.nets_total += other.nets_total;
        self.drawable_nets_total += other.drawable_nets_total;
        self.routed_nets_total += other.routed_nets_total;
        self.nets_missing_route += other.nets_missing_route;
        self.nets_empty_route += other.nets_empty_route;
        self.endpoints_total += other.endpoints_total;
        self.drawable_endpoints_total += other.drawable_endpoints_total;
        self.endpoints_box_missing += other.endpoints_box_missing;
        self.endpoints_pin_missing += other.endpoints_pin_missing;
        self.endpoints_entry_missing += other.endpoints_entry_missing;
        self.endpoints_route_unreached += other.endpoints_route_unreached;
        self.boxes_total += other.boxes_total;
        self.physical_pins_total += other.physical_pins_total;
        self.physical_pins_with_entry += other.physical_pins_with_entry;
        self.physical_pins_missing_entry += other.physical_pins_missing_entry;
    }

    pub fn report_line(&self) -> String {
        format!(
            "[metrics] TRUTH: layers={} nets drawable={}/{} routed={} missing_route={} empty_route={}, \
             endpoints drawable={}/{} box_missing={} pin_missing={} entry_missing={} route_unreached={}, \
             physical-pins entries={}/{} missing_entry={} -> PERFECT? {}",
            self.layers_total,
            self.drawable_nets_total,
            self.nets_total,
            self.routed_nets_total,
            self.nets_missing_route,
            self.nets_empty_route,
            self.drawable_endpoints_total,
            self.endpoints_total,
            self.endpoints_box_missing,
            self.endpoints_pin_missing,
            self.endpoints_entry_missing,
            self.endpoints_route_unreached,
            self.physical_pins_with_entry,
            self.physical_pins_total,
            self.physical_pins_missing_entry,
            self.is_perfect()
        )
    }
}

impl SchematicQualityReport {
    /// Milestone 2 keeps fidelity as the base gate and adds graph-truth completeness.
    pub fn is_perfect(&self) -> bool {
        self.fidelity.is_perfect() && self.truth.is_perfect()
    }

    pub fn report_lines(&self) -> Vec<String> {
        let mut lines = vec![
            self.fidelity.report_line(),
            self.readability.report_line(),
            format!(
                "[metrics] COLLISIONS: box_box={} wire_box={} wire_wire={} total={}",
                self.collisions.box_box,
                self.collisions.wire_box,
                self.collisions.wire_wire,
                self.collisions.total()
            ),
            self.builder.report_line(),
            self.truth.report_line(),
        ];
        lines.extend(self.visual.report_lines());
        lines.push(format!(
            "[metrics] QUALITY: perfect={} weighted={:.1}",
            self.is_perfect(),
            self.readability.weighted()
        ));
        lines
    }

    pub fn report_line(&self) -> String {
        self.report_lines().join("\n")
    }
}

// ============================================================================
// Accumulator — passes through render_layer_recursive, accumulating per layer
// ============================================================================
#[derive(Debug, Clone, Default)]
pub struct MetricsAccumulator {
    // fidelity (graph side)
    nets_rendered: usize,
    pins_total: usize,
    pins_rendered: usize,
    bus_bits_total: usize,
    authored_sides_total: usize,
    authored_sides_honored: usize,
    box_box: usize,
    wire_box: usize,
    // readability
    wire_wire: usize,
    total_wirelength: f64,
    total_bends: usize,
    off_grid_penalty: f64,
    truth: TruthSnapshotReport,
    visual: VisualQualityReport,
}

impl MetricsAccumulator {
    /// Accumulate **one layer** (graph.sub_graphs already taken by render, this layer only).
    /// `col` is the audit_all result for this layer.
    pub fn accumulate_layer(
        &mut self,
        graph: &McVecGraph,
        col: &CollisionReport,
        canvas: (f64, f64),
    ) {
        self.box_box += col.box_box;
        self.wire_box += col.wire_box;
        self.wire_wire += col.wire_wire;

        self.nets_rendered += graph.nets.len();

        for n in &graph.nets {
            if let NetKind::Bus(w) = n.kind {
                self.bus_bits_total += w;
            }
            if let Some(route) = &n.route {
                self.total_wirelength += route_length(route);
                self.total_bends += route_bends(route);
            }
        }

        for b in &graph.boxes {
            self.pins_total += b.pins.len();
            // Count physical pins that actually got an entry_point (matched by id).
            // Flag / synthetic / split entry_points have no corresponding BoxPin and
            // must not inflate pins_rendered. Placeholder pins (id ≥ 8e9) are dropped
            // by merge_box_pins and will be excluded from total in 02/03.
            self.pins_rendered += b
                .pins
                .iter()
                .filter(|p| b.entry_points.iter().any(|e| e.pin_id == p.id))
                .count();
            self.off_grid_penalty += off_grid(b.x) + off_grid(b.y);

            if let Some(lh) = &b.layout_hint {
                let listed = lh.left.len() + lh.right.len() + lh.top.len() + lh.bottom.len();
                self.authored_sides_total += listed;
                // Count honored: for each entry point, check if its actual side matches the layout-specified side
                let honored = b
                    .entry_points
                    .iter()
                    .filter(|ep| {
                        b.find_pin(ep.pin_id).is_some_and(|p| {
                            lh.side_of(&p.pin_id) == Some(ep.side.clone())
                                || lh.side_of(&p.description) == Some(ep.side.clone())
                        })
                    })
                    .count();
                self.authored_sides_honored += honored;
            }
        }

        self.truth.merge(snapshot_layer_truth(graph));
        self.visual.merge(visual_quality_for_layer(graph, canvas));
    }

    /// Merge build-phase dropped/partial, produce final two reports.
    pub fn finish(self, report: Option<&BuilderReport>) -> (FidelityReport, ReadabilityScore) {
        let (fidelity, readability, _, _, _, _) = self.finish_parts(report);
        (fidelity, readability)
    }

    /// Merge build-phase diagnostics and produce the unified schematic quality report.
    pub fn finish_quality(self, report: Option<&BuilderReport>) -> SchematicQualityReport {
        let (fidelity, readability, collisions, builder, truth, visual) = self.finish_parts(report);
        SchematicQualityReport {
            fidelity,
            readability,
            collisions,
            builder,
            truth,
            visual,
        }
    }

    fn finish_parts(
        self,
        report: Option<&BuilderReport>,
    ) -> (
        FidelityReport,
        ReadabilityScore,
        CollisionReport,
        BuilderQualitySummary,
        TruthSnapshotReport,
        VisualQualityReport,
    ) {
        let builder = BuilderQualitySummary::from_report(report);
        let dropped = builder.dropped_nets;
        let partial = builder.partial_nets;

        let collisions = CollisionReport {
            box_box: self.box_box,
            wire_box: self.wire_box,
            wire_wire: self.wire_wire,
            details: Vec::new(),
        };

        let fidelity = FidelityReport {
            nets_total: self.nets_rendered + dropped,
            nets_rendered: self.nets_rendered,
            nets_dropped: dropped,
            nets_partial: partial,
            pins_total: self.pins_total,
            pins_rendered: self.pins_rendered,
            bus_bits_total: self.bus_bits_total,
            bus_bits_paired_ok: self.bus_bits_total.saturating_sub(
                crate::instant::mc_mod::group::BUS_BITS_MISMATCHED
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            authored_sides_total: self.authored_sides_total,
            authored_sides_honored: self.authored_sides_honored,
            box_box: collisions.box_box,
            wire_box: collisions.wire_box,
        };
        let readability = ReadabilityScore {
            wire_wire: collisions.wire_wire,
            total_wirelength: self.total_wirelength,
            total_bends: self.total_bends,
            off_grid_penalty: self.off_grid_penalty,
            symmetry_penalty: self.visual.symmetry_penalty,
            idiom_violation: self.visual.idiom_violations,
        };
        (
            fidelity,
            readability,
            collisions,
            builder,
            self.truth,
            self.visual,
        )
    }
}

// ============================================================================
// Visual quality helpers — objective soft signals for schematic readability
// ============================================================================
fn visual_quality_for_layer(graph: &McVecGraph, canvas: (f64, f64)) -> VisualQualityReport {
    let mut rep = VisualQualityReport {
        canvas_width: canvas.0,
        canvas_height: canvas.1,
        canvas_area: (canvas.0 * canvas.1).max(0.0),
        boxes_total: graph.boxes.len(),
        box_area_total: graph.boxes.iter().map(|b| (b.w * b.h).max(0.0)).sum(),
        ..VisualQualityReport::default()
    };

    let labels: Vec<(i64, LabelBounds)> = graph
        .boxes
        .iter()
        .flat_map(|b| {
            designator_value_label_bounds(b)
                .into_iter()
                .map(move |label| (b.id, label))
        })
        .collect();
    rep.labels_total = labels.len();

    for (_, label) in &labels {
        if rect_off_canvas(label.x, label.y, label.w, label.h, canvas) {
            rep.labels_off_canvas += 1;
        }
    }

    for (i, (_, a)) in labels.iter().enumerate() {
        for (_, b) in labels.iter().skip(i + 1) {
            if rects_overlap_simple(a.x, a.y, a.w, a.h, b.x, b.y, b.w, b.h) {
                rep.label_label_overlaps += 1;
            }
        }
    }

    for (owner_id, label) in &labels {
        for b in &graph.boxes {
            if b.id == *owner_id && label.inside_owner_box {
                continue;
            }
            if rects_overlap_simple(label.x, label.y, label.w, label.h, b.x, b.y, b.w, b.h) {
                rep.label_box_overlaps += 1;
            }
        }
    }

    for (_, label) in &labels {
        for net in &graph.nets {
            if let Some(route) = &net.route {
                for seg in &route.segments {
                    if segment_hits_rect_simple(seg, label.x, label.y, label.w, label.h) {
                        rep.label_wire_overlaps += 1;
                    }
                }
            }
        }
    }

    for net in &graph.nets {
        if let Some(route) = &net.route {
            rep.routed_nets += 1;
            rep.route_segments_total += route.segments.len();
            rep.route_bends_total += route_bends(route);
            rep.route_length_total += route_length(route);
        }
    }

    let (symmetry, idioms) = crate::viz::idiom::penalty_summary(&crate::viz::idiom::analyze(graph));
    rep.symmetry_penalty = symmetry;
    rep.idiom_violations = idioms;
    rep.recompute_derived();
    rep
}

fn rects_overlap_simple(
    ax: f64,
    ay: f64,
    aw: f64,
    ah: f64,
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
) -> bool {
    ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah
}

fn rect_off_canvas(x: f64, y: f64, w: f64, h: f64, canvas: (f64, f64)) -> bool {
    x < 0.0 || y < 0.0 || x + w > canvas.0 || y + h > canvas.1
}

fn segment_hits_rect_simple(s: &Segment, rx: f64, ry: f64, rw: f64, rh: f64) -> bool {
    let (sx0, sx1) = (s.from.x.min(s.to.x), s.from.x.max(s.to.x));
    let (sy0, sy1) = (s.from.y.min(s.to.y), s.from.y.max(s.to.y));
    sx1 >= rx && sx0 <= rx + rw && sy1 >= ry && sy0 <= ry + rh
}

// ============================================================================
// Truth snapshot helpers — verifies routed graph still covers declared endpoints
// ============================================================================
const ROUTE_ENDPOINT_EPS: f64 = 1.0;

fn snapshot_layer_truth(graph: &McVecGraph) -> TruthSnapshotReport {
    let mut rep = TruthSnapshotReport {
        layers_total: 1,
        boxes_total: graph.boxes.len(),
        ..TruthSnapshotReport::default()
    };

    for b in &graph.boxes {
        rep.physical_pins_total += b.pins.len();
        for p in &b.pins {
            if b.entry_points.iter().any(|e| e.pin_id == p.id) {
                rep.physical_pins_with_entry += 1;
            } else {
                rep.physical_pins_missing_entry += 1;
            }
        }
    }

    for net in &graph.nets {
        rep.nets_total += 1;
        rep.endpoints_total += net.endpoints.len();

        if net.endpoint_count() < 2 {
            continue;
        }

        rep.drawable_nets_total += 1;
        rep.drawable_endpoints_total += net.endpoints.len();

        let route = match &net.route {
            Some(route) if route.segments.is_empty() => {
                rep.nets_empty_route += 1;
                None
            }
            Some(route) => {
                rep.routed_nets_total += 1;
                Some(route)
            }
            None => {
                rep.nets_missing_route += 1;
                None
            }
        };

        for endpoint in &net.endpoints {
            let Some(b) = graph.boxes.iter().find(|b| b.id == endpoint.box_id) else {
                rep.endpoints_box_missing += 1;
                continue;
            };

            if endpoint.is_synthetic() {
                continue;
            }

            if b.find_pin(endpoint.pin_id).is_none() {
                rep.endpoints_pin_missing += 1;
                continue;
            }

            let Some(entry) = b.find_entry(endpoint.pin_id) else {
                rep.endpoints_entry_missing += 1;
                continue;
            };

            if let Some(route) = route {
                let p = entry_point_abs(b, entry);
                if !route_touches_point(route, p, ROUTE_ENDPOINT_EPS) {
                    rep.endpoints_route_unreached += 1;
                }
            }
        }
    }

    rep
}

fn entry_point_abs(b: &McVecBox, ep: &EntryPoint) -> Point {
    match ep.side {
        EntrySide::Top => Point::new(b.x + ep.offset * b.w, b.y),
        EntrySide::Right => Point::new(b.x + b.w, b.y + ep.offset * b.h),
        EntrySide::Bottom => Point::new(b.x + ep.offset * b.w, b.y + b.h),
        EntrySide::Left => Point::new(b.x, b.y + ep.offset * b.h),
    }
}

fn route_touches_point(route: &Route, p: Point, eps: f64) -> bool {
    route.segments.iter().any(|s| point_on_segment(p, s, eps))
}

fn point_on_segment(p: Point, s: &Segment, eps: f64) -> bool {
    let dx = s.to.x - s.from.x;
    let dy = s.to.y - s.from.y;

    if dx.abs() <= eps {
        return (p.x - s.from.x).abs() <= eps && between(p.y, s.from.y, s.to.y, eps);
    }
    if dy.abs() <= eps {
        return (p.y - s.from.y).abs() <= eps && between(p.x, s.from.x, s.to.x, eps);
    }

    let len2 = dx * dx + dy * dy;
    if len2 <= eps * eps {
        return ((p.x - s.from.x).powi(2) + (p.y - s.from.y).powi(2)).sqrt() <= eps;
    }

    let t = ((p.x - s.from.x) * dx + (p.y - s.from.y) * dy) / len2;
    if t < -eps || t > 1.0 + eps {
        return false;
    }
    let t = t.clamp(0.0, 1.0);
    let proj = Point::new(s.from.x + t * dx, s.from.y + t * dy);
    ((p.x - proj.x).powi(2) + (p.y - proj.y).powi(2)).sqrt() <= eps
}

fn between(v: f64, a: f64, b: f64, eps: f64) -> bool {
    v >= a.min(b) - eps && v <= a.max(b) + eps
}

// ============================================================================
// Geometry helpers
// ============================================================================
pub(crate) fn route_length(route: &crate::vector::graph::net_def::Route) -> f64 {
    route
        .segments
        .iter()
        .map(|s| (s.to.x - s.from.x).abs() + (s.to.y - s.from.y).abs()) // Manhattan
        .sum()
}

/// Bend count ≈ number of axis changes between adjacent segments (orthogonal routing: each H↔V switch = one bend).
pub(crate) fn route_bends(route: &crate::vector::graph::net_def::Route) -> usize {
    #[derive(PartialEq)]
    enum Axis {
        H,
        V,
        Z,
    }
    let axis = |s: &crate::vector::graph::net_def::Segment| -> Axis {
        let dx = (s.to.x - s.from.x).abs();
        let dy = (s.to.y - s.from.y).abs();
        if dx > 0.0 && dy == 0.0 {
            Axis::H
        } else if dy > 0.0 && dx == 0.0 {
            Axis::V
        } else {
            Axis::Z
        }
    };
    let mut bends = 0;
    let mut prev: Option<Axis> = None;
    for s in &route.segments {
        let a = axis(s);
        if let Some(p) = &prev {
            if *p != a && a != Axis::Z && *p != Axis::Z {
                bends += 1;
            }
        }
        prev = Some(a);
    }
    bends
}

/// Distance to nearest grid line ([0, GRID/2]).
pub(crate) fn off_grid(v: f64) -> f64 {
    let m = v.rem_euclid(GRID);
    m.min(GRID - m)
}

// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::builder::builder_report::{
        BuilderReport, DroppedNet, PartialNet, ResolutionOutcome, ResolutionRecord,
    };
    use crate::vector::graph::box_def::{BoxPin, IoSummary};
    use crate::vector::graph::net_def::{Point, Route, Segment};
    use crate::vector::graph::{BoxKind, EndpointRef, McVecBox, Symbol, VizNet};

    fn mk_box(id: i64, x: f64, y: f64) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            format!("B{id}"),
            String::new(),
            BoxKind::TwoPin,
            Symbol::Resistor,
            None,
            None,
            1,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 40.0;
        b.h = 20.0;
        b
    }

    fn net_with_route(nid: i64, a: i64, b: i64, segs: Vec<(f64, f64, f64, f64)>) -> VizNet {
        let mut n = VizNet::new(
            nid,
            format!("n{nid}"),
            NetKind::Signal,
            vec![
                EndpointRef::new(a, -1, "(t)"),
                EndpointRef::new(b, -1, "(t)"),
            ],
        );
        let mut r = Route::new();
        for (x0, y0, x1, y1) in segs {
            r.segments.push(Segment {
                from: Point::new(x0, y0),
                to: Point::new(x1, y1),
            });
        }
        n.route = Some(r);
        n
    }

    fn add_pin_and_entry(b: &mut McVecBox, pin_id: i64, side: EntrySide, offset: f64) {
        add_pin(b, pin_id);
        b.entry_points.push(EntryPoint {
            pin_id,
            pin_name: format!("P{pin_id}"),
            side,
            offset,
        });
    }

    fn add_pin(b: &mut McVecBox, pin_id: i64) {
        b.pins.push(BoxPin {
            id: pin_id,
            pin_id: format!("P{pin_id}"),
            description: format!("P{pin_id}"),
            io: crate::vector::graph::net_def::IoDirection::Unknown,
        });
    }

    fn real_net(nid: i64, a_box: i64, a_pin: i64, b_box: i64, b_pin: i64) -> VizNet {
        VizNet::new(
            nid,
            format!("n{nid}"),
            NetKind::Signal,
            vec![
                EndpointRef::new(a_box, a_pin, format!("P{a_pin}")),
                EndpointRef::new(b_box, b_pin, format!("P{b_pin}")),
            ],
        )
    }

    fn complete_two_point_graph() -> McVecGraph {
        let mut g = McVecGraph::new(0, "t".into());
        let mut b1 = mk_box(1, 0.0, 0.0);
        let mut b2 = mk_box(2, 100.0, 0.0);
        add_pin_and_entry(&mut b1, 11, EntrySide::Right, 0.5);
        add_pin_and_entry(&mut b2, 22, EntrySide::Left, 0.5);
        g.boxes.push(b1);
        g.boxes.push(b2);
        let mut net = real_net(1, 1, 11, 2, 22);
        let mut route = Route::new();
        route.segments.push(Segment {
            from: Point::new(40.0, 10.0),
            to: Point::new(100.0, 10.0),
        });
        net.route = Some(route);
        g.nets.push(net);
        g
    }

    /// Counts match hand calculation: 1 net, wirelen=100+50, 1 bend.
    #[test]
    fn metrics_counts_match_netlist() {
        let mut g = McVecGraph::new(0, "t".into());
        g.boxes.push(mk_box(1, 0.0, 0.0));
        g.boxes.push(mk_box(2, 100.0, 50.0));
        // L-shape: horizontal 100 + vertical 50 → 1 bend
        g.nets.push(net_with_route(
            0,
            1,
            2,
            vec![(0.0, 0.0, 100.0, 0.0), (100.0, 0.0, 100.0, 50.0)],
        ));

        let mut acc = MetricsAccumulator::default();
        acc.accumulate_layer(&g, &CollisionReport::default(), (200.0, 120.0));
        let (fid, read) = acc.finish(None);

        assert_eq!(fid.nets_rendered, 1);
        assert_eq!(fid.nets_total, 1); // no dropped
        assert!((read.total_wirelength - 150.0).abs() < 1e-9);
        assert_eq!(read.total_bends, 1);
    }

    /// Determinism: same graph twice yields equal weighted() (depends on Phase 0 determinism fix).
    #[test]
    fn metrics_deterministic() {
        let build = || {
            let mut g = McVecGraph::new(0, "t".into());
            g.boxes.push(mk_box(1, 0.0, 0.0));
            g.boxes.push(mk_box(2, 80.0, 0.0));
            g.nets
                .push(net_with_route(0, 1, 2, vec![(0.0, 0.0, 80.0, 0.0)]));
            let mut a = MetricsAccumulator::default();
            a.accumulate_layer(&g, &CollisionReport::default(), (200.0, 120.0));
            a.finish(None).1.weighted()
        };
        assert_eq!(build(), build());
    }

    /// Manual overlap of two boxes → audit_all reports box_box≥1 → is_perfect()==false.
    #[test]
    fn metrics_detects_box_overlap() {
        let mut g = McVecGraph::new(0, "t".into());
        g.boxes.push(mk_box(1, 0.0, 0.0));
        g.boxes.push(mk_box(2, 5.0, 5.0)); // overlaps box 1
        let rep = crate::viz::route::audit::audit_all(&g);
        let mut acc = MetricsAccumulator::default();
        acc.accumulate_layer(&g, &rep, (200.0, 120.0));
        let (fid, _) = acc.finish(None);
        assert!(fid.box_box >= 1);
        assert!(!fid.is_perfect());
    }

    #[test]
    fn finish_quality_matches_finish() {
        let mut g = McVecGraph::new(0, "t".into());
        g.boxes.push(mk_box(1, 0.0, 0.0));
        g.boxes.push(mk_box(2, 80.0, 0.0));
        g.nets
            .push(net_with_route(0, 1, 2, vec![(0.0, 0.0, 80.0, 0.0)]));

        let mut acc = MetricsAccumulator::default();
        acc.accumulate_layer(&g, &CollisionReport::default(), (200.0, 120.0));

        let (fidelity, readability) = acc.clone().finish(None);
        let quality = acc.finish_quality(None);

        assert_eq!(quality.fidelity, fidelity);
        assert_eq!(quality.readability, readability);
    }

    #[test]
    fn finish_quality_preserves_collision_counts() {
        let mut g = McVecGraph::new(0, "t".into());
        g.boxes.push(mk_box(1, 0.0, 0.0));
        let collisions = CollisionReport {
            box_box: 1,
            wire_box: 2,
            wire_wire: 3,
            details: vec!["detail is intentionally not accumulated".into()],
        };

        let mut acc = MetricsAccumulator::default();
        acc.accumulate_layer(&g, &collisions, (200.0, 120.0));
        let quality = acc.finish_quality(None);

        assert_eq!(quality.collisions.box_box, 1);
        assert_eq!(quality.collisions.wire_box, 2);
        assert_eq!(quality.collisions.wire_wire, 3);
        assert_eq!(quality.collisions.total(), 6);
        assert!(quality.collisions.details.is_empty());
    }

    #[test]
    fn builder_quality_summary_maps_report_counts() {
        let mut report = BuilderReport::new();
        report.resolutions.push(ResolutionRecord {
            module_path: "top".into(),
            net_name: "N1".into(),
            point_path: "U1.A".into(),
            outcome: ResolutionOutcome::Direct,
        });
        report.resolutions.push(ResolutionRecord {
            module_path: "top".into(),
            net_name: "N2".into(),
            point_path: "U2.A".into(),
            outcome: ResolutionOutcome::Failed,
        });
        report.dropped_nets.push(DroppedNet {
            module_path: "top".into(),
            net_name: "DROP".into(),
            original_point_count: 2,
            resolved_point_count: 0,
        });
        report.partial_nets.push(PartialNet {
            module_path: "top".into(),
            net_name: "PARTIAL".into(),
            failed_points: vec!["U3.A".into(), "U4.A".into()],
            resolved_point_count: 1,
        });
        report.unresolved_modules.push("missing".into());

        let summary = BuilderQualitySummary::from_report(Some(&report));

        assert_eq!(summary.resolutions_total, 2);
        assert!((summary.resolution_success_rate - 0.5).abs() < 1e-9);
        assert_eq!(summary.dropped_nets, 1);
        assert_eq!(summary.partial_nets, 1);
        assert_eq!(summary.unresolved_modules, 1);
        assert_eq!(summary.warnings, 3);
    }

    #[test]
    fn truth_detects_missing_route_for_drawable_net() {
        let mut g = complete_two_point_graph();
        g.nets[0].route = None;

        let truth = snapshot_layer_truth(&g);

        assert_eq!(truth.drawable_nets_total, 1);
        assert_eq!(truth.nets_missing_route, 1);
        assert!(!truth.is_perfect());
    }

    #[test]
    fn truth_detects_empty_route() {
        let mut g = complete_two_point_graph();
        g.nets[0].route = Some(Route::new());

        let truth = snapshot_layer_truth(&g);

        assert_eq!(truth.nets_empty_route, 1);
        assert!(!truth.is_perfect());
    }

    #[test]
    fn truth_detects_missing_endpoint_box() {
        let mut g = complete_two_point_graph();
        g.nets[0].endpoints[1].box_id = 999;

        let truth = snapshot_layer_truth(&g);

        assert_eq!(truth.endpoints_box_missing, 1);
        assert!(!truth.is_perfect());
    }

    #[test]
    fn truth_detects_missing_physical_pin() {
        let mut g = complete_two_point_graph();
        g.boxes[1].pins.clear();

        let truth = snapshot_layer_truth(&g);

        assert_eq!(truth.endpoints_pin_missing, 1);
        assert!(!truth.is_perfect());
    }

    #[test]
    fn truth_detects_missing_entry_point() {
        let mut g = complete_two_point_graph();
        g.boxes[1].entry_points.clear();

        let truth = snapshot_layer_truth(&g);

        assert_eq!(truth.endpoints_entry_missing, 1);
        assert!(truth.physical_pins_missing_entry >= 1);
        assert!(!truth.is_perfect());
    }

    #[test]
    fn truth_detects_route_unreached_endpoint() {
        let mut g = complete_two_point_graph();
        if let Some(route) = &mut g.nets[0].route {
            route.segments[0] = Segment {
                from: Point::new(50.0, 50.0),
                to: Point::new(60.0, 50.0),
            };
        }

        let truth = snapshot_layer_truth(&g);

        assert_eq!(truth.endpoints_route_unreached, 2);
        assert!(!truth.is_perfect());
    }

    #[test]
    fn truth_accepts_complete_two_point_route() {
        let g = complete_two_point_graph();
        let mut acc = MetricsAccumulator::default();
        acc.accumulate_layer(&g, &CollisionReport::default(), (200.0, 120.0));
        let quality = acc.finish_quality(None);

        assert!(quality.truth.is_perfect());
        assert!(quality.is_perfect());
    }

    #[test]
    fn truth_skips_synthetic_pin_physical_checks() {
        let mut g = complete_two_point_graph();
        g.nets[0].endpoints[1] = EndpointRef::new(2, -1, "SYNTH");
        g.boxes[1].pins.clear();
        g.boxes[1].entry_points.clear();

        let truth = snapshot_layer_truth(&g);

        assert_eq!(truth.endpoints_pin_missing, 0);
        assert_eq!(truth.endpoints_entry_missing, 0);
        assert_eq!(truth.physical_pins_missing_entry, 0);
    }

    #[test]
    fn visual_detects_label_label_overlap() {
        let mut g = McVecGraph::new(0, "t".into());
        let mut a = mk_box(1, 50.0, 50.0);
        a.designator = Some("R_LONG_A".into());
        let mut b = mk_box(2, 55.0, 50.0);
        b.designator = Some("R_LONG_B".into());
        g.boxes.push(a);
        g.boxes.push(b);

        let visual = visual_quality_for_layer(&g, (200.0, 120.0));

        assert_eq!(visual.labels_total, 2);
        assert!(visual.label_label_overlaps > 0);
    }

    #[test]
    fn visual_detects_label_box_overlap() {
        let mut g = McVecGraph::new(0, "t".into());
        let mut labeled = mk_box(1, 50.0, 50.0);
        labeled.designator = Some("R1".into());
        let blocker = mk_box(2, 50.0, 30.0);
        g.boxes.push(labeled);
        g.boxes.push(blocker);

        let visual = visual_quality_for_layer(&g, (200.0, 120.0));

        assert!(visual.label_box_overlaps > 0);
    }

    #[test]
    fn visual_detects_label_wire_overlap() {
        let mut g = McVecGraph::new(0, "t".into());
        let mut b = mk_box(1, 50.0, 50.0);
        b.designator = Some("R1".into());
        g.boxes.push(b);
        g.nets
            .push(net_with_route(1, 1, 1, vec![(40.0, 40.0, 80.0, 40.0)]));

        let visual = visual_quality_for_layer(&g, (200.0, 120.0));

        assert!(visual.label_wire_overlaps > 0);
    }

    #[test]
    fn visual_detects_label_off_canvas() {
        let mut g = McVecGraph::new(0, "t".into());
        let mut b = mk_box(1, 50.0, 5.0);
        b.designator = Some("R1".into());
        g.boxes.push(b);

        let visual = visual_quality_for_layer(&g, (200.0, 120.0));

        assert_eq!(visual.labels_off_canvas, 1);
    }

    #[test]
    fn visual_computes_canvas_density() {
        let mut g = McVecGraph::new(0, "t".into());
        g.boxes.push(mk_box(1, 0.0, 0.0));
        g.boxes.push(mk_box(2, 50.0, 0.0));

        let visual = visual_quality_for_layer(&g, (100.0, 100.0));

        assert_eq!(visual.boxes_total, 2);
        assert!((visual.box_area_total - 1600.0).abs() < 1e-9);
        assert!((visual.box_density - 0.16).abs() < 1e-9);
    }

    #[test]
    fn visual_computes_route_averages() {
        let mut g = McVecGraph::new(0, "t".into());
        g.boxes.push(mk_box(1, 0.0, 0.0));
        g.boxes.push(mk_box(2, 100.0, 50.0));
        g.nets.push(net_with_route(
            1,
            1,
            2,
            vec![(0.0, 0.0, 100.0, 0.0), (100.0, 0.0, 100.0, 50.0)],
        ));

        let visual = visual_quality_for_layer(&g, (200.0, 120.0));

        assert_eq!(visual.routed_nets, 1);
        assert_eq!(visual.route_segments_total, 2);
        assert_eq!(visual.route_bends_total, 1);
        assert!((visual.route_length_total - 150.0).abs() < 1e-9);
        assert!((visual.avg_segments_per_routed_net - 2.0).abs() < 1e-9);
        assert!((visual.avg_bends_per_routed_net - 1.0).abs() < 1e-9);
        assert!((visual.avg_route_length_per_routed_net - 150.0).abs() < 1e-9);
    }

    #[test]
    fn visual_metrics_do_not_affect_hard_gate() {
        let mut g = complete_two_point_graph();
        g.boxes[0].designator = Some("R_LONG_A".into());
        g.boxes[1].x = 45.0;
        g.boxes[1].designator = Some("R_LONG_B".into());
        g.nets[0].route = Some(Route {
            segments: vec![Segment {
                from: Point::new(40.0, 10.0),
                to: Point::new(45.0, 10.0),
            }],
            junctions: Vec::new(),
        });

        let mut acc = MetricsAccumulator::default();
        acc.accumulate_layer(&g, &CollisionReport::default(), (200.0, 120.0));
        let quality = acc.finish_quality(None);

        assert!(quality.visual.label_label_overlaps > 0);
        assert!(quality.is_perfect());
    }

    #[test]
    fn report_lines_include_visual() {
        let quality = SchematicQualityReport::default();
        let lines = quality.report_lines();
        assert!(lines.iter().any(|line| line.contains("[metrics] VISUAL:")));
        assert!(lines
            .iter()
            .any(|line| line.contains("[metrics] ROUTE-QUALITY:")));
        assert!(lines.iter().any(|line| line.contains("[metrics] IDIOM:")));
    }

    #[test]
    fn report_lines_include_truth() {
        let quality = SchematicQualityReport::default();
        assert!(quality
            .report_lines()
            .iter()
            .any(|line| line.contains("[metrics] TRUTH:")));
    }
}
