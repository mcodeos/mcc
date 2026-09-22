//! Iteration 01 · Acceptance yardstick: electrical fidelity (hard gate) + readability score (for
//! ranking).
//!
//! - FidelityReport.is_perfect() = hard gate for all subsequent iterations.
//! - ReadabilityScore.weighted() = score for generate-and-rank (iteration 04).
//! - MetricsAccumulator passes through viz::api::render_layer_recursive, accumulating per layer.
//! - AlignMetricsReport (align.rs) provides M1-4 alignment metrics (self-test version).

pub mod align;
pub mod renderdiff;

use serde::{Deserialize, Serialize};

use crate::vector::builder::report::BuilderReport;
use crate::vector::graph::boxdef::{EntryPoint, EntrySide, McVecBox};
use crate::vector::graph::netdef::{Point, Route, Segment};
use crate::semantic::common::ConnOp;
use crate::vector::graph::{McVecGraph, NetKind};
use crate::vector::model::AttrRole;
use crate::viz::render::label_render::{designator_value_label_bounds, LabelBounds};
use crate::viz::route::audit::CollisionReport;
use crate::viz::semantic::SemanticSummary;

/// Alignment grid for off-grid penalty (no coordinate snapping in this codebase; soft alignment
/// signal, tunable).
pub const GRID: f64 = 10.0;

/// Version of the metrics schema: the shape of [`SchematicMetricsSnapshot`].
///
/// **Declared, not derived** — see [`crate::viz::layout::LAYOUT_VERSION`] for
/// why. It is one number consumed in two places, the snapshot that carries it
/// and the projection envelope that reports it
/// ([`crate::stages::StageView::metric_schema_version`]); spelling it twice is
/// how the two would come to disagree.
pub const METRIC_SCHEMA_VERSION: u32 = 1;

// Electrical fidelity — hard gate, must be all green
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
    /// ★ Layout model coverage: islands claimed / total.
    /// Set from graph.islands_claimed / graph.islands_total.
    pub islands_claimed: usize,
    pub islands_total: usize,
}

impl FidelityReport {
    /// Tier 1 · CORRECTNESS — hard gate: electrical correctness only.
    /// Nets, pins, bus bits must be perfect. Collisions and coverage are
    /// Tier 2 (quality ratchet), NOT Tier 1.
    pub fn is_correct(&self) -> bool {
        self.nets_dropped == 0
            && self.nets_partial == 0
            && self.pins_rendered == self.pins_total
            && self.bus_bits_paired_ok == self.bus_bits_total
    }

    /// ★ Legacy: Tier 1 + Tier 2 combined. Kept for backward compat;
    ///    new code should use `is_correct()` for the hard gate and
    ///    `quality_ok()` for the ratchet.
    pub fn is_perfect(&self) -> bool {
        self.is_correct() && self.box_box == 0 && self.wire_box == 0
    }

    /// Tier 2 · QUALITY — ratchet gate: collisions + coverage.
    /// Returns false if any quality metric is non-zero.
    pub fn has_quality_issues(&self) -> bool {
        self.box_box > 0 || self.wire_box > 0
    }

    /// ★ Layout coverage percentage (0–100).
    pub fn coverage_pct(&self) -> f64 {
        if self.islands_total > 0 {
            self.islands_claimed as f64 / self.islands_total as f64 * 100.0
        } else {
            0.0
        }
    }

    pub fn report_line(&self) -> String {
        format!(
            "[metrics] FIDELITY: nets {}/{} rendered ({} dropped, {} partial), \
             pins {}/{}, bus-bits {}/{}, authored-sides {}/{}, box_box={}, wire_box={}, \
             islands={}/{} coverage={:.0}% -> CORRECT? {}",
            self.nets_rendered,
            self.nets_total,
            self.nets_dropped,
            self.nets_partial,
            self.pins_rendered,
            self.pins_total,
            self.bus_bits_paired_ok,
            self.bus_bits_total,
            self.authored_sides_honored,
            self.authored_sides_total,
            self.box_box,
            self.wire_box,
            self.islands_claimed,
            self.islands_total,
            self.coverage_pct(),
            self.is_correct()
        )
    }
}

// Readability score — lower is better, for ranking/comparison
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
    /// Single scalar for generate-and-rank (dimensions copied from obstacles::score_path: collision
    /// 1000 >> length).
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

// Phase F — Engineer style soft metrics

/// One engineer-style axis, kept as raw counts rather than a ratio.
///
/// `num / den` is the axis' score and `samples` is how many objects it was
/// measured over. Raw because a render has several layers and the accumulator
/// merges them: adding numerators and denominators gives a **ratio of sums**,
/// where adding finished ratios would give the average of averages. The same
/// shape the accumulator already uses for pins and authored sides.
///
/// `samples` is also why this is not a bare `f64`. Every axis below scores `1.0`
/// when it has nothing to measure — the historical convention of these helpers —
/// so a layer with no power nets would otherwise publish the same number as a
/// layer whose power nets are all placed right. The count is what tells those
/// two apart, and it is published beside the score.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Axis {
    pub num: f64,
    pub den: f64,
    pub samples: usize,
}

impl Axis {
    /// `den` objects measured, `num` of them good. `den` doubles as `samples`.
    fn new(num: usize, den: usize) -> Self {
        Self {
            num: num as f64,
            den: den as f64,
            samples: den,
        }
    }

    /// For the axes whose two halves are areas rather than object counts.
    fn area(num: f64, den: f64, samples: usize) -> Self {
        Self { num, den, samples }
    }

    /// The score, with the vacuous case left at `1.0` and recorded by `samples`.
    fn ratio(self) -> f64 {
        if self.den == 0.0 {
            1.0
        } else {
            self.num / self.den
        }
    }

    fn add(&mut self, o: Self) {
        self.num += o.num;
        self.den += o.den;
        self.samples += o.samples;
    }
}

/// Every engineer-style axis for one layer, in raw counts.
///
/// Public because it is the unit [`MetricsAccumulator`] merges and the unit the
/// tests exercise; the published shape is [`EngineerStyleMetrics`], which folds
/// these into scores.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EngineerStyleCells {
    pub signal_flow: Axis,
    pub rail_align: Axis,
    pub ground_align: Axis,
    pub bus_order: Axis,
    pub idiom_prox: Axis,
    pub pin_side: Axis,
    /// `(sum of box areas, bounding-box area of the boxes, boxes)`. Merged by
    /// summing both areas across layers. The score is their ratio capped at
    /// `1.0`, since boxes may overlap.
    pub block_compact: Axis,
    pub route_channel: Axis,
    /// `(label pairs that overlap, labels)`. The score is the complement of the
    /// overlap rate against the most overlaps a set of that size can hold — see
    /// [`label_readability_score`].
    pub label_readable: Axis,
}

impl EngineerStyleCells {
    /// Measure one layer.
    pub fn measure(graph: &McVecGraph) -> Self {
        let (rail_align, ground_align) = measure_rail_alignment(graph);
        Self {
            signal_flow: measure_signal_flow(graph),
            rail_align,
            ground_align,
            bus_order: measure_bus_order(graph),
            idiom_prox: measure_idiom_proximity(graph),
            pin_side: measure_pin_side_honor(graph),
            block_compact: measure_block_compactness(graph),
            route_channel: measure_route_channel_clarity(graph),
            label_readable: measure_label_readability(graph),
        }
    }

    /// Fold another layer's cells in.
    pub fn merge(&mut self, other: Self) {
        self.signal_flow.add(other.signal_flow);
        self.rail_align.add(other.rail_align);
        self.ground_align.add(other.ground_align);
        self.bus_order.add(other.bus_order);
        self.idiom_prox.add(other.idiom_prox);
        self.pin_side.add(other.pin_side);
        self.block_compact.add(other.block_compact);
        self.route_channel.add(other.route_channel);
        self.label_readable.add(other.label_readable);
    }
}

/// Soft metrics that measure how "engineer-like" the schematic looks.
/// These are informational only — they do NOT affect the hard gate.
///
/// Each score carries the count it was measured over. A score of `1.0` with
/// `*_samples == 0` means the axis found nothing to measure, not that it found
/// everything in order; read the pair, never the score alone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineerStyleMetrics {
    /// Signal flow monotonicity: fraction of signal chains drawn left-to-right.
    pub signal_flow_monotonicity: f64,
    /// Chains that fraction was taken over.
    pub signal_flow_samples: usize,
    /// Power rail alignment: fraction of power placements above their consumer.
    pub rail_alignment_score: f64,
    pub rail_alignment_samples: usize,
    /// Ground alignment: fraction of ground placements below their consumer.
    pub ground_alignment_score: f64,
    pub ground_alignment_samples: usize,
    /// Bus order: fraction of buses whose bit order comes out ascending.
    pub bus_order_score: f64,
    pub bus_order_samples: usize,
    /// Idiom proximity: fraction of idiom instances that are not violations.
    pub idiom_proximity_score: f64,
    pub idiom_proximity_samples: usize,
    /// Pin side intent honor rate: fraction of authored pin sides honored.
    pub pin_side_intent_honor_rate: f64,
    pub pin_side_intent_samples: usize,
    /// Functional block compactness: box area over bounding-box area, capped.
    pub functional_block_compactness: f64,
    pub functional_block_samples: usize,
    /// Route channel clarity: fraction of routes with few bends per segment.
    pub route_channel_clarity: f64,
    pub route_channel_samples: usize,
    /// Label readability: how far the label overlap count is from the worst a
    /// label set that size allows.
    pub label_readability_score: f64,
    pub label_readability_samples: usize,
}

impl EngineerStyleMetrics {
    /// Fold measured cells into the published scores.
    pub fn from_cells(c: EngineerStyleCells) -> Self {
        Self {
            signal_flow_monotonicity: c.signal_flow.ratio(),
            signal_flow_samples: c.signal_flow.samples,
            rail_alignment_score: c.rail_align.ratio(),
            rail_alignment_samples: c.rail_align.samples,
            ground_alignment_score: c.ground_align.ratio(),
            ground_alignment_samples: c.ground_align.samples,
            bus_order_score: c.bus_order.ratio(),
            bus_order_samples: c.bus_order.samples,
            idiom_proximity_score: c.idiom_prox.ratio(),
            idiom_proximity_samples: c.idiom_prox.samples,
            pin_side_intent_honor_rate: c.pin_side.ratio(),
            pin_side_intent_samples: c.pin_side.samples,
            functional_block_compactness: c.block_compact.ratio().min(1.0),
            functional_block_samples: c.block_compact.samples,
            route_channel_clarity: c.route_channel.ratio(),
            route_channel_samples: c.route_channel.samples,
            label_readability_score: label_readability_score(&c.label_readable),
            label_readability_samples: c.label_readable.samples,
        }
    }

    /// Measure a single graph.
    ///
    /// The accumulator path is [`EngineerStyleCells::measure`] into
    /// [`EngineerStyleMetrics::from_cells`], because a render has more than one
    /// layer and its axes merge; this is the one-graph convenience over it.
    pub fn compute(graph: &McVecGraph) -> Self {
        Self::from_cells(EngineerStyleCells::measure(graph))
    }

    pub fn report_line(&self) -> String {
        format!(
            "[metrics] ENGINEER-STYLE: signal_flow={:.2}({}) rail_align={:.2}({}) \
             ground_align={:.2}({}) bus_order={:.2}({}) idiom_prox={:.2}({}) \
             pin_side={:.2}({}) block_compact={:.2}({}) route_channel={:.2}({}) \
             label_readable={:.2}({})",
            self.signal_flow_monotonicity,
            self.signal_flow_samples,
            self.rail_alignment_score,
            self.rail_alignment_samples,
            self.ground_alignment_score,
            self.ground_alignment_samples,
            self.bus_order_score,
            self.bus_order_samples,
            self.idiom_proximity_score,
            self.idiom_proximity_samples,
            self.pin_side_intent_honor_rate,
            self.pin_side_intent_samples,
            self.functional_block_compactness,
            self.functional_block_samples,
            self.route_channel_clarity,
            self.route_channel_samples,
            self.label_readability_score,
            self.label_readability_samples,
        )
    }
}

// ── Engineer style metric helpers ──
//
// Each returns the raw counts, not a finished ratio, so the accumulator can add
// them over layers. The empty-input case yields `den == 0` and is left for
// `Axis::ratio` to turn into the conventional `1.0`.

fn measure_signal_flow(graph: &McVecGraph) -> Axis {
    // Check if signal chains flow left-to-right
    let mut chains = 0usize;
    let mut monotonic = 0usize;
    // U162(b) (signal-flow-metric-design P1/P3): a net is judged by its own
    // drawing law, not by one flat rule. Declared supply nets draw as trunks
    // and are already placed by the rail/ground axes; a declared parallel net
    // fans out from one driver and carries no chain order. Both leave the
    // axis — counting them here judged misordering no layout pass promised.
    let mut rails_out = 0usize;
    let mut fanout_out = 0usize;
    for net in &graph.nets {
        if net.endpoints.len() < 2 {
            continue;
        }
        if net
            .attr
            .as_ref()
            .is_some_and(|a| matches!(a.role, AttrRole::Hot | AttrRole::Ret | AttrRole::Reference))
        {
            rails_out += 1;
            continue;
        }
        if net.shape.as_ref().and_then(|s| s.op) == Some(ConnOp::Parallel) {
            fanout_out += 1;
            continue;
        }
        chains += 1;
        let mut all_ltr = true;
        let mut prev_x = f64::NEG_INFINITY;
        // Debug face: the monotone prefix of the chain, `box@center-x`, so a
        // failing chain can be read straight off the dump. The failing
        // endpoint itself is not in the sequence — it broke the order.
        let dump = crate::viz::log::enabled();
        let mut seq = String::new();
        let mut full = String::new();
        for ep in &net.endpoints {
            if let Some(b) = graph.boxes.iter().find(|bx| bx.id == ep.box_id) {
                let cx = b.x + b.w / 2.0;
                if dump {
                    let name = b.designator.clone().unwrap_or_else(|| b.name.clone());
                    full.push_str(&format!("{}/{}@{:.0} ", name, ep.box_id, cx));
                }
                if cx < prev_x {
                    all_ltr = false;
                    break;
                }
                prev_x = cx;
            }
            if dump {
                let name = graph
                    .boxes
                    .iter()
                    .find(|bx| bx.id == ep.box_id)
                    .map(|bx| {
                        bx.designator
                            .clone()
                            .unwrap_or_else(|| bx.name.clone())
                    })
                    .unwrap_or_default();
                seq.push_str(&format!("{}/{}@{:.0} ", name, ep.box_id, prev_x));
            }
        }
        if !all_ltr {
            crate::vlog!(
                "[sigflow] layer={} net={}({}) NONLTR seq={} FULL={}",
                graph.name,
                net.name,
                net.nid,
                seq,
                full
            );
        } else {
            monotonic += 1;
        }
    }
    crate::vlog!(
        "[sigflow] layer={} summary monotonic={}/{} rails-out={} fanout-out={}",
        graph.name,
        monotonic,
        chains,
        rails_out,
        fanout_out
    );
    Axis::new(monotonic, chains)
}

/// `(power, ground)`. Both count endpoint pairs rather than nets: a net with
/// three placements offers every ordered pair to the comparison.
fn measure_rail_alignment(graph: &McVecGraph) -> (Axis, Axis) {
    let mut power_total = 0usize;
    let mut power_above = 0usize;
    let mut ground_total = 0usize;
    let mut ground_below = 0usize;

    for net in &graph.nets {
        let is_power = matches!(net.kind, NetKind::Power);
        let is_ground = matches!(net.kind, NetKind::Ground);
        if !is_power && !is_ground {
            continue;
        }
        for ep in &net.endpoints {
            if let Some(b) = graph.boxes.iter().find(|bx| bx.id == ep.box_id) {
                let cy = b.y + b.h / 2.0;
                // Find the consumer (non-flag endpoint)
                for other_ep in &net.endpoints {
                    if other_ep.box_id == ep.box_id {
                        continue;
                    }
                    if let Some(other_b) = graph.boxes.iter().find(|bx| bx.id == other_ep.box_id) {
                        let other_cy = other_b.y + other_b.h / 2.0;
                        if is_power {
                            power_total += 1;
                            if cy < other_cy {
                                power_above += 1;
                            }
                        }
                        if is_ground {
                            ground_total += 1;
                            if cy > other_cy {
                                ground_below += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    (
        Axis::new(power_above, power_total),
        Axis::new(ground_below, ground_total),
    )
}

fn measure_bus_order(graph: &McVecGraph) -> Axis {
    let mut total_buses = 0usize;
    let mut ordered = 0usize;
    for net in &graph.nets {
        if !matches!(net.kind, NetKind::Bus(_)) {
            continue;
        }
        total_buses += 1;
        let mut all_ordered = true;
        let mut prev_bit: Option<usize> = None;
        let mut eps: Vec<_> = net.endpoints.iter().collect();
        eps.sort_by_key(|ep| ep.pin_name.clone());
        for ep in &eps {
            if let Some(bit) = ep.pin_name.parse::<usize>().ok() {
                if let Some(prev) = prev_bit {
                    if bit != prev + 1 {
                        all_ordered = false;
                        break;
                    }
                }
                prev_bit = Some(bit);
            }
        }
        if all_ordered {
            ordered += 1;
        }
    }
    Axis::new(ordered, total_buses)
}

fn measure_idiom_proximity(graph: &McVecGraph) -> Axis {
    let idioms = crate::viz::idiom::analyze(graph);
    let total = idioms.len();
    let violations = idioms.iter().filter(|i| i.idiom_violation).count();
    Axis::new(total - violations, total)
}

fn measure_pin_side_honor(graph: &McVecGraph) -> Axis {
    let mut total = 0usize;
    let mut honored = 0usize;
    for b in &graph.boxes {
        if let Some(lh) = &b.layout_hint {
            let listed = lh.left.len() + lh.right.len() + lh.top.len() + lh.bottom.len();
            total += listed;
            let h = b
                .entry_points
                .iter()
                .filter(|ep| {
                    b.find_pin(ep.pin_id).is_some_and(|p| {
                        lh.side_of(&p.pin_id) == Some(ep.side.clone())
                            || lh.side_of(&p.description) == Some(ep.side.clone())
                    })
                })
                .count();
            honored += h;
        }
    }
    Axis::new(honored, total)
}

fn measure_block_compactness(graph: &McVecGraph) -> Axis {
    // Measure how compactly boxes are packed
    let boxes = graph.boxes.len();
    if boxes < 2 {
        return Axis::area(0.0, 0.0, 0);
    }
    let total_box_area: f64 = graph.boxes.iter().map(|b| b.w * b.h).sum();
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for b in &graph.boxes {
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
        max_x = max_x.max(b.x + b.w);
        max_y = max_y.max(b.y + b.h);
    }
    let bbox_area = (max_x - min_x) * (max_y - min_y);
    Axis::area(total_box_area, bbox_area.max(0.0), boxes)
}

fn measure_route_channel_clarity(graph: &McVecGraph) -> Axis {
    // Measure how many routes are orthogonal and clear
    let mut total_routes = 0usize;
    let mut clear_routes = 0usize;
    for net in &graph.nets {
        if let Some(route) = &net.route {
            if route.segments.is_empty() {
                continue;
            }
            total_routes += 1;
            // Check if route is mostly orthogonal (few bends)
            let bends = crate::viz::metrics::route_bends(route);
            let segments = route.segments.len();
            if bends <= segments / 2 + 1 {
                clear_routes += 1;
            }
        }
    }
    Axis::new(clear_routes, total_routes)
}

fn measure_label_readability(graph: &McVecGraph) -> Axis {
    let labels: Vec<LabelBounds> = graph
        .boxes
        .iter()
        .flat_map(|b| designator_value_label_bounds(b))
        .collect();
    let mut overlaps = 0usize;
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            if rects_overlap_simple(
                labels[i].x,
                labels[i].y,
                labels[i].w,
                labels[i].h,
                labels[j].x,
                labels[j].y,
                labels[j].w,
                labels[j].h,
            ) {
                overlaps += 1;
            }
        }
    }
    Axis::new(overlaps, labels.len())
}

/// The label axis is the one whose score is not `num / den`: it compares the
/// overlap count against the most a set that size can hold, which is half its
/// size (each label sharing with one other). A set of one can hold no overlap at
/// all, so it scores `1.0` and reports `samples == 1` — not the same thing as an
/// axis with nothing to measure, but the two are told apart the same way.
fn label_readability_score(a: &Axis) -> f64 {
    let worst = (a.den / 2.0).floor();
    if worst <= 0.0 {
        1.0
    } else {
        1.0 - (a.num / worst).min(1.0)
    }
}

// Unified schematic quality report — Milestone 1 acceptance report
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
    pub semantic: Option<SemanticSummary>,
    pub special: Option<super::special::PowerGroundBusReport>,
    pub determinism: Option<super::stability::report::DeterminismReport>,
    pub stability: Option<super::stability::report::StabilityReport>,
    pub rendered_connectivity: Option<super::connectivity::report::RenderedConnectivityReport>,
    /// Phase F — Engineer style soft metrics (informational, not hard gate)
    pub engineer_style: EngineerStyleMetrics,
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
        if let Some(ref s) = self.semantic {
            lines.push(format!(
                "[metrics] SEMANTIC: hubs={} signal_chains={} passive_chains={} \
                 component_groups={} bus_groups={} rail_intents={} idioms={} \
                 pins_preferred_side={} pins_actual_side={} side_mismatches={} \
                 warnings={}",
                s.hubs_detected,
                s.signal_chains_detected,
                s.passive_chains_detected,
                s.component_groups_detected,
                s.bus_groups_detected,
                s.rail_intents_detected,
                s.idioms_detected,
                s.pins_with_preferred_side,
                s.pins_with_actual_side,
                s.preferred_actual_side_mismatches,
                s.warnings,
            ));
        }
        if let Some(ref s) = self.special {
            lines.push(s.report_line());
        }
        if let Some(ref d) = self.determinism {
            lines.push(d.report_line());
        }
        if let Some(ref s) = self.stability {
            lines.push(s.report_line());
        }
        if let Some(ref c) = self.rendered_connectivity {
            lines.push(c.report_line());
        }
        lines.push(self.engineer_style.report_line());
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

// Accumulator — passes through render_layer_recursive, accumulating per layer
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
    semantic: Option<SemanticSummary>,
    special: Option<super::special::PowerGroundBusReport>,
    determinism: Option<super::stability::report::DeterminismReport>,
    stability: Option<super::stability::report::StabilityReport>,
    connectivity: Option<super::connectivity::report::RenderedConnectivityReport>,
    /// Phase F — engineer style axes, merged over every layer.
    engineer_style: EngineerStyleCells,
    /// ★ P7-1: renderdiff per-layer readings (measured after route, before render)
    pub renderdiff_layers: Vec<renderdiff::LayerReading>,
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
                // ★ Model-claimed boxes (geom_locked) have their pin sides determined
                //   by topology — count as honoured, same as select.rs.
                let honored = if b.geom_locked {
                    listed
                } else {
                    b.entry_points
                        .iter()
                        .filter(|ep| {
                            b.find_pin(ep.pin_id).is_some_and(|p| {
                                lh.side_of(&p.pin_id) == Some(ep.side.clone())
                                    || lh.side_of(&p.description) == Some(ep.side.clone())
                            })
                        })
                        .count()
                };
                self.authored_sides_honored += honored;
            }
        }

        self.truth.merge(snapshot_layer_truth(graph));
        self.visual.merge(visual_quality_for_layer(graph, canvas));
    }

    /// Accumulate semantic analysis result for this layer (merge across layers).
    pub fn accumulate_semantic(&mut self, summary: &SemanticSummary) {
        match &mut self.semantic {
            Some(existing) => existing.merge(summary),
            None => self.semantic = Some(summary.clone()),
        }
    }

    /// Accumulate M10 special (power/ground/bus) report across layers.
    pub fn accumulate_special(&mut self, report: &super::special::PowerGroundBusReport) {
        match &mut self.special {
            Some(existing) => existing.merge(report),
            None => self.special = Some(report.clone()),
        }
    }

    /// Accumulate M12 determinism report across layers.
    pub fn accumulate_determinism(&mut self, report: &super::stability::report::DeterminismReport) {
        self.determinism = Some(report.clone());
    }

    /// Accumulate M12 stability report across layers.
    pub fn accumulate_stability(&mut self, report: &super::stability::report::StabilityReport) {
        match &mut self.stability {
            Some(existing) => {
                existing.unchanged_boxes_total += report.unchanged_boxes_total;
                existing.unchanged_boxes_moved += report.unchanged_boxes_moved;
                existing.max_unchanged_box_delta = existing
                    .max_unchanged_box_delta
                    .max(report.max_unchanged_box_delta);
                existing.route_hashes_changed += report.route_hashes_changed;
                existing.locality_warning = existing.locality_warning || report.locality_warning;
            }
            None => self.stability = Some(report.clone()),
        }
    }

    /// Accumulate M13 rendered connectivity report across layers.
    pub fn accumulate_connectivity(
        &mut self,
        report: &super::connectivity::report::RenderedConnectivityReport,
    ) {
        match &mut self.connectivity {
            Some(existing) => existing.merge(report),
            None => self.connectivity = Some(report.clone()),
        }
    }

    /// Accumulate engineer style metrics for one layer.
    ///
    /// Runs for **every** layer, unlike the four audited families: these axes
    /// read box placement, rails, labels and routes, and a device sub-layer has
    /// all four. Merged by addition, so the published score is a ratio of sums
    /// over the whole drawing rather than the last layer's ratio.
    pub fn accumulate_engineer_style(&mut self, graph: &McVecGraph) {
        self.engineer_style
            .merge(EngineerStyleCells::measure(graph));
    }

    /// ★ P7-1: accumulate one renderdiff layer reading (measured after route, before render).
    pub fn accumulate_renderdiff(&mut self, reading: renderdiff::LayerReading) {
        self.renderdiff_layers.push(reading);
    }

    /// ★ M5 (opt-in): serialize the accumulated reports as the machine-side
    /// report JSON (fidelity / M12 determinism / M13 connectivity snapshots).
    /// The builder summary is empty here: the builder report is attached by
    /// the CLI at finish time, not at the viz layer.
    pub fn report_json(&self) -> String {
        let quality = self.clone().finish_quality(None);
        let semantic = match &quality.semantic {
            Some(s) => SemanticSnapshot::from_summary(s),
            None => SemanticSnapshot::default(),
        };
        let snap = SchematicMetricsSnapshot::from_quality(
            &quality,
            semantic,
            "viz",
            "",
            "render_with_metrics",
        );
        snap.to_json()
    }

    /// Merge build-phase dropped/partial, produce final two reports.
    pub fn finish(self, report: Option<&BuilderReport>) -> (FidelityReport, ReadabilityScore) {
        let (fidelity, readability, _, _, _, _, _, _) = self.finish_parts(report);
        (fidelity, readability)
    }

    /// Merge build-phase diagnostics and produce the unified schematic quality report.
    pub fn finish_quality(self, report: Option<&BuilderReport>) -> SchematicQualityReport {
        let determinism = self.determinism.clone();
        let stability = self.stability.clone();
        let connectivity = self.connectivity.clone();
        // `EngineerStyleCells` is `Copy`, so this is read here, before
        // `finish_parts` consumes `self`, without cloning.
        let engineer_style = EngineerStyleMetrics::from_cells(self.engineer_style);
        let (fidelity, readability, collisions, builder, truth, visual, semantic, special) =
            self.finish_parts(report);
        SchematicQualityReport {
            fidelity,
            readability,
            collisions,
            builder,
            truth,
            visual,
            semantic,
            special,
            determinism,
            stability,
            rendered_connectivity: connectivity,
            engineer_style,
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
        Option<SemanticSummary>,
        Option<super::special::PowerGroundBusReport>,
    ) {
        let builder = BuilderQualitySummary::from_report(report);
        let dropped = builder.dropped_nets;
        let partial = builder.partial_nets;

        let collisions = CollisionReport {
            box_box: self.box_box,
            wire_box: self.wire_box,
            wire_wire: self.wire_wire,
            diagonal_segments: 0,
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
            // E4052 retired (design doc section 6.1 D3): member names are
            // never a pairing criterion, so there is no name-mismatch signal
            // to subtract - every bus bit pairs by position.
            bus_bits_paired_ok: self.bus_bits_total,
            authored_sides_total: self.authored_sides_total,
            authored_sides_honored: self.authored_sides_honored,
            box_box: collisions.box_box,
            wire_box: collisions.wire_box,
            islands_claimed: 0,
            islands_total: 0,
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
            self.semantic,
            self.special,
        )
    }
}

// Visual quality helpers — objective soft signals for schematic readability
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

// Truth snapshot helpers — verifies routed graph still covers declared endpoints
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

// Geometry helpers
pub(crate) fn route_length(route: &crate::vector::graph::netdef::Route) -> f64 {
    route
        .segments
        .iter()
        .map(|s| (s.to.x - s.from.x).abs() + (s.to.y - s.from.y).abs()) // Manhattan
        .sum()
}

/// Bend count ≈ number of axis changes between adjacent segments (orthogonal routing: each H↔V
/// switch = one bend).
pub(crate) fn route_bends(route: &crate::vector::graph::netdef::Route) -> usize {
    #[derive(PartialEq)]
    enum Axis {
        H,
        V,
        Z,
    }
    let axis = |s: &crate::vector::graph::netdef::Segment| -> Axis {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::builder::report::{
        BuilderReport, DroppedNet, PartialNet, ResolutionOutcome, ResolutionRecord,
    };
    use crate::vector::graph::boxdef::{BoxPin, IoSummary};
    use crate::vector::graph::netdef::{Point, Route, Segment};
    use crate::vector::graph::{BoxKind, EndpointRef, McVecBox, NetRole, Symbol, VizNet};

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
            format!("B{id}"),
            Vec::new(),
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
            NetRole::Signal,
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
            io: crate::vector::graph::netdef::IoDirection::Unknown,
            port_dir: crate::vector::graph::boxdef::PortDir::None,
            src_span: None,
            point: None,
        });
    }

    fn real_net(nid: i64, a_box: i64, a_pin: i64, b_box: i64, b_pin: i64) -> VizNet {
        VizNet::new(
            nid,
            format!("n{nid}"),
            NetKind::Signal,
            NetRole::Signal,
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
            diagonal_segments: 0,
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
            escalated: false,
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
    // ── Engineer style (M4): the axes are counted, merged, and their vacuous
    // case is visible. These three carry the whole reason the axes are raw
    // counts rather than finished ratios.

    /// One signal chain of two boxes. `left_to_right` picks which box comes
    /// first, which is the whole of what the `signal_flow` axis measures.
    fn chain(g: &mut McVecGraph, nid: i64, left_to_right: bool) {
        let (a, b) = if left_to_right {
            (0.0, 100.0)
        } else {
            (100.0, 0.0)
        };
        let (ida, idb) = (nid * 2, nid * 2 + 1);
        g.boxes.push(mk_box(ida, a, 0.0));
        g.boxes.push(mk_box(idb, b, 0.0));
        g.nets.push(real_net(nid, ida, 10, idb, 20));
    }

    /// The merge is a **ratio of sums**, not the mean of the layers' ratios —
    /// the reason [`EngineerStyleCells`] holds counts rather than scores.
    ///
    /// One layer is 1/1 and the other 0/3, so the ratio of sums is 1/4 while the
    /// mean of the two ratios is 1/2. Pinning the first rules out the second.
    #[test]
    fn engineer_style_merges_as_a_ratio_of_sums() {
        let mut one = McVecGraph::new(0, "one".into());
        chain(&mut one, 1, true);
        let mut other = McVecGraph::new(1, "other".into());
        for nid in 1..=3 {
            chain(&mut other, nid, false);
        }

        let a = EngineerStyleMetrics::compute(&one);
        let b = EngineerStyleMetrics::compute(&other);
        assert_eq!(
            (a.signal_flow_monotonicity, a.signal_flow_samples),
            (1.0, 1),
            "one chain, drawn left to right"
        );
        assert_eq!(
            (b.signal_flow_monotonicity, b.signal_flow_samples),
            (0.0, 3),
            "three chains, none of them"
        );

        let mut cells = EngineerStyleCells::measure(&one);
        cells.merge(EngineerStyleCells::measure(&other));
        let merged = EngineerStyleMetrics::from_cells(cells);
        assert_eq!(
            (merged.signal_flow_monotonicity, merged.signal_flow_samples),
            (0.25, 4),
            "1 of 4 chains across both layers"
        );
        let mean_of_ratios = (a.signal_flow_monotonicity + b.signal_flow_monotonicity) / 2.0;
        assert_ne!(
            merged.signal_flow_monotonicity, mean_of_ratios,
            "the merge must not collapse into the average of the two layers"
        );
    }

    /// The score alone cannot tell "nothing to measure" from "everything in
    /// order" — both are `1.0`. The count is what separates them, and that is
    /// why every score is published with one.
    #[test]
    fn a_vacuous_axis_scores_the_same_as_a_perfect_one() {
        let empty = McVecGraph::new(0, "empty".into());
        let mut perfect = McVecGraph::new(0, "perfect".into());
        chain(&mut perfect, 1, true);

        let e = EngineerStyleMetrics::compute(&empty);
        let p = EngineerStyleMetrics::compute(&perfect);
        assert_eq!(
            e.signal_flow_monotonicity, p.signal_flow_monotonicity,
            "the two agree on the score, which is exactly the hazard"
        );
        assert_eq!(e.signal_flow_monotonicity, 1.0);
        assert_eq!(e.signal_flow_samples, 0, "nothing to measure");
        assert_eq!(p.signal_flow_samples, 1, "one chain, drawn left to right");
    }

    /// A graph with nothing on it must not read as the all-zero default: the
    /// scores are `1.0` and only the counts are zero. The default struct is what
    /// an unwired family would have published, and it is the opposite claim.
    #[test]
    fn an_empty_graph_is_not_the_unwired_default() {
        let empty = McVecGraph::new(0, "empty".into());
        let m = EngineerStyleMetrics::compute(&empty);
        assert_ne!(m, EngineerStyleMetrics::default());
        assert_eq!(m.signal_flow_monotonicity, 1.0);
        assert_eq!(m.route_channel_clarity, 1.0);
        for (name, samples) in [
            ("signal_flow", m.signal_flow_samples),
            ("rail_alignment", m.rail_alignment_samples),
            ("ground_alignment", m.ground_alignment_samples),
            ("bus_order", m.bus_order_samples),
            ("idiom_proximity", m.idiom_proximity_samples),
            ("pin_side_intent", m.pin_side_intent_samples),
            ("functional_block", m.functional_block_samples),
            ("route_channel", m.route_channel_samples),
            ("label_readability", m.label_readability_samples),
        ] {
            assert_eq!(
                samples, 0,
                "`{name}` has nothing to measure on an empty graph"
            );
        }
    }
}

// Milestone 4 — Stable snapshot types for regression baseline

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateSnapshot {
    pub quality_perfect: bool,
    pub fidelity_perfect: bool,
    pub truth_perfect: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FidelitySnapshot {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TruthSnapshot {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollisionSnapshot {
    pub box_box: usize,
    pub wire_box: usize,
    pub wire_wire: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualSnapshot {
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
    pub symmetry_penalty: f64,
    pub idiom_violations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteQualitySnapshot {
    pub routed_nets: usize,
    pub route_segments_total: usize,
    pub route_bends_total: usize,
    pub route_length_total: f64,
    pub avg_segments_per_routed_net: f64,
    pub avg_bends_per_routed_net: f64,
    pub avg_route_length_per_routed_net: f64,
    pub wire_wire: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuilderSnapshot {
    pub resolutions_total: usize,
    pub resolution_success_rate: f64,
    pub dropped_nets: usize,
    pub partial_nets: usize,
    pub unresolved_modules: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SemanticSnapshot {
    pub boxes_total: usize,
    pub nets_total: usize,
    pub pins_total: usize,
    pub hubs_detected: usize,
    pub signal_chains_detected: usize,
    pub passive_chains_detected: usize,
    pub component_groups_detected: usize,
    pub bus_groups_detected: usize,
    pub rail_intents_detected: usize,
    pub idioms_detected: usize,
    pub pins_with_preferred_side: usize,
    pub pins_with_actual_side: usize,
    pub preferred_actual_side_mismatches: usize,
    pub warnings: usize,
}

impl SemanticSnapshot {
    pub fn from_summary(s: &SemanticSummary) -> Self {
        Self {
            boxes_total: s.boxes_total,
            nets_total: s.nets_total,
            pins_total: s.pins_total,
            hubs_detected: s.hubs_detected,
            signal_chains_detected: s.signal_chains_detected,
            passive_chains_detected: s.passive_chains_detected,
            component_groups_detected: s.component_groups_detected,
            bus_groups_detected: s.bus_groups_detected,
            rail_intents_detected: s.rail_intents_detected,
            idioms_detected: s.idioms_detected,
            pins_with_preferred_side: s.pins_with_preferred_side,
            pins_with_actual_side: s.pins_with_actual_side,
            preferred_actual_side_mismatches: s.preferred_actual_side_mismatches,
            warnings: s.warnings,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadabilitySnapshot {
    pub wire_wire: usize,
    pub total_wirelength: f64,
    pub total_bends: usize,
    pub off_grid_penalty: f64,
    pub symmetry_penalty: f64,
    pub idiom_violation: usize,
    pub weighted: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeterminismSnapshot {
    pub box_order_hash: String,
    pub net_order_hash: String,
    pub pin_anchor_hash: String,
    pub route_geometry_hash: String,
    pub metrics_hash: String,
    pub unstable_decisions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StabilitySnapshot {
    pub unchanged_boxes_total: usize,
    pub unchanged_boxes_moved: usize,
    pub max_unchanged_box_delta: f64,
    pub route_hashes_changed: usize,
    pub locality_warning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConnectivitySnapshot {
    pub is_perfect: bool,
    pub pins_total: usize,
    pub pins_reachable: usize,
    pub pins_unreachable: usize,
    pub nets_total: usize,
    pub nets_perfect: usize,
    pub nets_with_render_mismatch: usize,
    pub false_connections: usize,
    pub missing_connections: usize,
    pub false_junctions: usize,
    pub missing_junctions: usize,
    pub different_net_crossings: usize,
    pub different_net_crossings_with_hop: usize,
    pub different_net_crossings_without_hop: usize,
    pub connectivity_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsThresholds {
    pub float_tolerance_abs: f64,
    pub float_tolerance_ratio: f64,
    pub max_label_label_overlaps_delta: usize,
    pub max_label_box_overlaps_delta: usize,
    pub max_label_wire_overlaps_delta: usize,
    pub max_labels_off_canvas_delta: usize,
    pub max_wire_wire_delta: usize,
    pub max_avg_bends_ratio_increase: f64,
    pub max_canvas_area_ratio_increase: f64,
    pub max_weighted_readability_ratio_increase: f64,
}

impl Default for MetricsThresholds {
    fn default() -> Self {
        Self {
            float_tolerance_abs: 0.1,
            float_tolerance_ratio: 0.01,
            max_label_label_overlaps_delta: 0,
            max_label_box_overlaps_delta: 0,
            max_label_wire_overlaps_delta: 0,
            max_labels_off_canvas_delta: 0,
            max_wire_wire_delta: 0,
            max_avg_bends_ratio_increase: 0.05,
            max_canvas_area_ratio_increase: 0.05,
            max_weighted_readability_ratio_increase: 0.05,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchematicMetricsSnapshot {
    pub schema_version: u32,
    pub project: String,
    pub entry: String,
    pub command: String,
    pub gate: GateSnapshot,
    pub fidelity: FidelitySnapshot,
    pub truth: TruthSnapshot,
    pub collisions: CollisionSnapshot,
    pub visual: VisualSnapshot,
    pub route_quality: RouteQualitySnapshot,
    pub builder: BuilderSnapshot,
    pub semantic: SemanticSnapshot,
    pub readability: ReadabilitySnapshot,
    pub determinism: DeterminismSnapshot,
    pub stability: StabilitySnapshot,
    pub connectivity: ConnectivitySnapshot,
    pub thresholds: MetricsThresholds,
}

impl SchematicMetricsSnapshot {
    pub fn from_quality(
        quality: &SchematicQualityReport,
        semantic: SemanticSnapshot,
        project: impl Into<String>,
        entry: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        let gate = GateSnapshot {
            quality_perfect: quality.is_perfect(),
            fidelity_perfect: quality.fidelity.is_perfect(),
            truth_perfect: quality.truth.is_perfect(),
        };

        let fidelity = FidelitySnapshot {
            nets_total: quality.fidelity.nets_total,
            nets_rendered: quality.fidelity.nets_rendered,
            nets_dropped: quality.fidelity.nets_dropped,
            nets_partial: quality.fidelity.nets_partial,
            pins_total: quality.fidelity.pins_total,
            pins_rendered: quality.fidelity.pins_rendered,
            bus_bits_total: quality.fidelity.bus_bits_total,
            bus_bits_paired_ok: quality.fidelity.bus_bits_paired_ok,
            authored_sides_total: quality.fidelity.authored_sides_total,
            authored_sides_honored: quality.fidelity.authored_sides_honored,
            box_box: quality.fidelity.box_box,
            wire_box: quality.fidelity.wire_box,
        };

        let truth = TruthSnapshot {
            layers_total: quality.truth.layers_total,
            nets_total: quality.truth.nets_total,
            drawable_nets_total: quality.truth.drawable_nets_total,
            routed_nets_total: quality.truth.routed_nets_total,
            nets_missing_route: quality.truth.nets_missing_route,
            nets_empty_route: quality.truth.nets_empty_route,
            endpoints_total: quality.truth.endpoints_total,
            drawable_endpoints_total: quality.truth.drawable_endpoints_total,
            endpoints_box_missing: quality.truth.endpoints_box_missing,
            endpoints_pin_missing: quality.truth.endpoints_pin_missing,
            endpoints_entry_missing: quality.truth.endpoints_entry_missing,
            endpoints_route_unreached: quality.truth.endpoints_route_unreached,
            boxes_total: quality.truth.boxes_total,
            physical_pins_total: quality.truth.physical_pins_total,
            physical_pins_with_entry: quality.truth.physical_pins_with_entry,
            physical_pins_missing_entry: quality.truth.physical_pins_missing_entry,
        };

        let collisions = CollisionSnapshot {
            box_box: quality.collisions.box_box,
            wire_box: quality.collisions.wire_box,
            wire_wire: quality.collisions.wire_wire,
            total: quality.collisions.total(),
        };

        let visual = VisualSnapshot {
            canvas_width: quality.visual.canvas_width,
            canvas_height: quality.visual.canvas_height,
            canvas_area: quality.visual.canvas_area,
            boxes_total: quality.visual.boxes_total,
            box_area_total: quality.visual.box_area_total,
            box_density: quality.visual.box_density,
            labels_total: quality.visual.labels_total,
            label_label_overlaps: quality.visual.label_label_overlaps,
            label_box_overlaps: quality.visual.label_box_overlaps,
            label_wire_overlaps: quality.visual.label_wire_overlaps,
            labels_off_canvas: quality.visual.labels_off_canvas,
            symmetry_penalty: quality.visual.symmetry_penalty,
            idiom_violations: quality.visual.idiom_violations,
        };

        let route_quality = RouteQualitySnapshot {
            routed_nets: quality.visual.routed_nets,
            route_segments_total: quality.visual.route_segments_total,
            route_bends_total: quality.visual.route_bends_total,
            route_length_total: quality.visual.route_length_total,
            avg_segments_per_routed_net: quality.visual.avg_segments_per_routed_net,
            avg_bends_per_routed_net: quality.visual.avg_bends_per_routed_net,
            avg_route_length_per_routed_net: quality.visual.avg_route_length_per_routed_net,
            wire_wire: quality.collisions.wire_wire,
        };

        let builder = BuilderSnapshot {
            resolutions_total: quality.builder.resolutions_total,
            resolution_success_rate: quality.builder.resolution_success_rate,
            dropped_nets: quality.builder.dropped_nets,
            partial_nets: quality.builder.partial_nets,
            unresolved_modules: quality.builder.unresolved_modules,
            warnings: quality.builder.warnings,
        };

        let readability = ReadabilitySnapshot {
            wire_wire: quality.readability.wire_wire,
            total_wirelength: quality.readability.total_wirelength,
            total_bends: quality.readability.total_bends,
            off_grid_penalty: quality.readability.off_grid_penalty,
            symmetry_penalty: quality.readability.symmetry_penalty,
            idiom_violation: quality.readability.idiom_violation,
            weighted: quality.readability.weighted(),
        };

        let determinism = match &quality.determinism {
            Some(d) => DeterminismSnapshot {
                box_order_hash: d.box_order_hash.clone(),
                net_order_hash: d.net_order_hash.clone(),
                pin_anchor_hash: d.pin_anchor_hash.clone(),
                route_geometry_hash: d.route_geometry_hash.clone(),
                metrics_hash: d.metrics_hash.clone(),
                unstable_decisions: d.unstable_decisions,
            },
            None => DeterminismSnapshot::default(),
        };

        let stability = match &quality.stability {
            Some(s) => StabilitySnapshot {
                unchanged_boxes_total: s.unchanged_boxes_total,
                unchanged_boxes_moved: s.unchanged_boxes_moved,
                max_unchanged_box_delta: s.max_unchanged_box_delta,
                route_hashes_changed: s.route_hashes_changed,
                locality_warning: s.locality_warning,
            },
            None => StabilitySnapshot::default(),
        };

        let connectivity = match &quality.rendered_connectivity {
            Some(c) => ConnectivitySnapshot {
                is_perfect: c.is_perfect,
                pins_total: c.pins_total,
                pins_reachable: c.pins_reachable,
                pins_unreachable: c.pins_unreachable,
                nets_total: c.nets_total,
                nets_perfect: c.nets_perfect,
                nets_with_render_mismatch: c.nets_with_render_mismatch,
                false_connections: c.false_connections,
                missing_connections: c.missing_connections,
                false_junctions: c.false_junctions,
                missing_junctions: c.missing_junctions,
                different_net_crossings: c.different_net_crossings,
                different_net_crossings_with_hop: c.different_net_crossings_with_hop,
                different_net_crossings_without_hop: c.different_net_crossings_without_hop,
                connectivity_hash: c.connectivity_hash.clone(),
            },
            None => ConnectivitySnapshot::default(),
        };

        Self {
            schema_version: METRIC_SCHEMA_VERSION,
            project: project.into(),
            entry: entry.into(),
            command: command.into(),
            gate,
            fidelity,
            truth,
            collisions,
            visual,
            route_quality,
            builder,
            semantic,
            readability,
            determinism,
            stability,
            connectivity,
            thresholds: MetricsThresholds::default(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {}\"}}", e))
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

// Milestone 4 — Comparison report

#[derive(Debug, Clone, PartialEq)]
pub struct MetricRegression {
    pub path: String,
    pub baseline: String,
    pub current: String,
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricsComparisonReport {
    pub passed: bool,
    pub failures: Vec<MetricRegression>,
    pub warnings: Vec<MetricRegression>,
}

impl MetricsComparisonReport {
    pub fn report_text(&self) -> String {
        let mut lines = vec![format!(
            "[metrics-golden] {} hbl1 metrics regression",
            if self.passed { "PASS" } else { "FAIL" }
        )];

        if !self.failures.is_empty() {
            lines.push(String::new());
            lines.push("Failures:".into());
            for f in &self.failures {
                lines.push(format!(
                    "  - {}: baseline={} current={} rule={}",
                    f.path, f.baseline, f.current, f.rule
                ));
            }
        }

        if !self.warnings.is_empty() {
            lines.push(String::new());
            lines.push("Warnings:".into());
            for w in &self.warnings {
                lines.push(format!(
                    "  - {}: baseline={} current={} rule={}",
                    w.path, w.baseline, w.current, w.rule
                ));
            }
        }

        lines.join("\n")
    }
}

pub fn compare_metrics_snapshot(
    current: &SchematicMetricsSnapshot,
    baseline: &SchematicMetricsSnapshot,
) -> MetricsComparisonReport {
    let t = &baseline.thresholds;
    let mut failures: Vec<MetricRegression> = Vec::new();
    let mut warnings: Vec<MetricRegression> = Vec::new();

    let mut add_failure = |path: &str, base: String, cur: String, rule: String| {
        failures.push(MetricRegression {
            path: path.into(),
            baseline: base,
            current: cur,
            rule,
        });
    };

    let _add_warning = |path: &str, base: String, cur: String, rule: String| {
        warnings.push(MetricRegression {
            path: path.into(),
            baseline: base,
            current: cur,
            rule,
        });
    };

    // ── Hard gate: true — current must not be worse than baseline ──
    for (path, cur, base) in [
        (
            "gate.quality_perfect",
            current.gate.quality_perfect,
            baseline.gate.quality_perfect,
        ),
        (
            "gate.fidelity_perfect",
            current.gate.fidelity_perfect,
            baseline.gate.fidelity_perfect,
        ),
        (
            "gate.truth_perfect",
            current.gate.truth_perfect,
            baseline.gate.truth_perfect,
        ),
    ] {
        if base && !cur {
            add_failure(
                path,
                base.to_string(),
                cur.to_string(),
                "must be true (was true in baseline)".into(),
            );
        }
    }

    // ── Hard gate: zero — current must not be worse than baseline ──
    for (path, cur, base) in [
        (
            "fidelity.nets_dropped",
            current.fidelity.nets_dropped,
            baseline.fidelity.nets_dropped,
        ),
        (
            "fidelity.nets_partial",
            current.fidelity.nets_partial,
            baseline.fidelity.nets_partial,
        ),
        (
            "truth.nets_missing_route",
            current.truth.nets_missing_route,
            baseline.truth.nets_missing_route,
        ),
        (
            "truth.nets_empty_route",
            current.truth.nets_empty_route,
            baseline.truth.nets_empty_route,
        ),
        (
            "truth.endpoints_box_missing",
            current.truth.endpoints_box_missing,
            baseline.truth.endpoints_box_missing,
        ),
        (
            "truth.endpoints_pin_missing",
            current.truth.endpoints_pin_missing,
            baseline.truth.endpoints_pin_missing,
        ),
        (
            "truth.endpoints_entry_missing",
            current.truth.endpoints_entry_missing,
            baseline.truth.endpoints_entry_missing,
        ),
        (
            "truth.endpoints_route_unreached",
            current.truth.endpoints_route_unreached,
            baseline.truth.endpoints_route_unreached,
        ),
        (
            "truth.physical_pins_missing_entry",
            current.truth.physical_pins_missing_entry,
            baseline.truth.physical_pins_missing_entry,
        ),
        (
            "collisions.box_box",
            current.collisions.box_box,
            baseline.collisions.box_box,
        ),
        (
            "collisions.wire_box",
            current.collisions.wire_box,
            baseline.collisions.wire_box,
        ),
    ] {
        if cur > base {
            add_failure(
                path,
                base.to_string(),
                cur.to_string(),
                "current <= baseline".into(),
            );
        }
    }

    // ── Integer non-regression: current <= baseline + delta ──
    let int_rules: &[(&str, usize, usize, usize)] = &[
        (
            "collisions.wire_wire",
            current.collisions.wire_wire,
            baseline.collisions.wire_wire,
            t.max_wire_wire_delta,
        ),
        (
            "visual.label_label_overlaps",
            current.visual.label_label_overlaps,
            baseline.visual.label_label_overlaps,
            t.max_label_label_overlaps_delta,
        ),
        (
            "visual.label_box_overlaps",
            current.visual.label_box_overlaps,
            baseline.visual.label_box_overlaps,
            t.max_label_box_overlaps_delta,
        ),
        (
            "visual.label_wire_overlaps",
            current.visual.label_wire_overlaps,
            baseline.visual.label_wire_overlaps,
            t.max_label_wire_overlaps_delta,
        ),
        (
            "visual.labels_off_canvas",
            current.visual.labels_off_canvas,
            baseline.visual.labels_off_canvas,
            t.max_labels_off_canvas_delta,
        ),
    ];

    for (path, cur, base, delta) in int_rules {
        if *cur > base.saturating_add(*delta) {
            add_failure(
                path,
                base.to_string(),
                cur.to_string(),
                format!("current <= baseline + {}", delta),
            );
        }
    }

    // ── Float non-regression: current <= baseline * (1 + ratio) ──
    let float_ratio_rules: &[(&str, f64, f64, f64)] = &[
        (
            "route_quality.avg_bends_per_routed_net",
            current.route_quality.avg_bends_per_routed_net,
            baseline.route_quality.avg_bends_per_routed_net,
            t.max_avg_bends_ratio_increase,
        ),
        (
            "visual.canvas_area",
            current.visual.canvas_area,
            baseline.visual.canvas_area,
            t.max_canvas_area_ratio_increase,
        ),
        (
            "readability.weighted",
            current.readability.weighted,
            baseline.readability.weighted,
            t.max_weighted_readability_ratio_increase,
        ),
    ];

    for (path, cur, base, ratio) in float_ratio_rules {
        let threshold = base * (1.0 + ratio) + t.float_tolerance_abs;
        if *cur > threshold {
            add_failure(
                path,
                format!("{:.2}", base),
                format!("{:.2}", cur),
                format!("current <= baseline * {:.2}", 1.0 + ratio),
            );
        }
    }

    MetricsComparisonReport {
        passed: failures.is_empty(),
        failures,
        warnings,
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip_json() {
        let quality = SchematicQualityReport::default();
        let snap = SchematicMetricsSnapshot::from_quality(
            &quality,
            SemanticSnapshot::default(),
            "test",
            "test.mc",
            "cargo test",
        );
        let json = snap.to_json();
        let parsed: SchematicMetricsSnapshot = SchematicMetricsSnapshot::from_json(&json).unwrap();
        assert_eq!(snap, parsed);
    }

    #[test]
    fn gate_hard_fail_detected() {
        let mut baseline = SchematicMetricsSnapshot::from_quality(
            &SchematicQualityReport::default(),
            SemanticSnapshot::default(),
            "test",
            "test.mc",
            "",
        );
        baseline.gate.quality_perfect = true;
        let mut current = baseline.clone();
        current.gate.quality_perfect = false;
        let report = compare_metrics_snapshot(&current, &baseline);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|f| f.path == "gate.quality_perfect"));
    }

    #[test]
    fn wire_wire_regression_detected() {
        let mut baseline = SchematicMetricsSnapshot::from_quality(
            &SchematicQualityReport::default(),
            SemanticSnapshot::default(),
            "test",
            "test.mc",
            "",
        );
        baseline.collisions.wire_wire = 10;
        let mut current = baseline.clone();
        current.collisions.wire_wire = 15;
        let report = compare_metrics_snapshot(&current, &baseline);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|f| f.path == "collisions.wire_wire"));
    }

    #[test]
    fn wire_wire_improvement_passes() {
        let mut baseline = SchematicMetricsSnapshot::from_quality(
            &SchematicQualityReport::default(),
            SemanticSnapshot::default(),
            "test",
            "test.mc",
            "",
        );
        baseline.collisions.wire_wire = 10;
        let mut current = baseline.clone();
        current.collisions.wire_wire = 5;
        let report = compare_metrics_snapshot(&current, &baseline);
        assert!(report.passed);
    }

    #[test]
    fn canvas_area_regression_detected() {
        let mut baseline = SchematicMetricsSnapshot::from_quality(
            &SchematicQualityReport::default(),
            SemanticSnapshot::default(),
            "test",
            "test.mc",
            "",
        );
        baseline.visual.canvas_area = 1000.0;
        let mut current = baseline.clone();
        current.visual.canvas_area = 1200.0; // 20% increase, above 5% threshold
        let report = compare_metrics_snapshot(&current, &baseline);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|f| f.path == "visual.canvas_area"));
    }

    #[test]
    fn snapshot_has_schema_version() {
        let snap = SchematicMetricsSnapshot::from_quality(
            &SchematicQualityReport::default(),
            SemanticSnapshot::default(),
            "test",
            "test.mc",
            "",
        );
        assert_eq!(snap.schema_version, 1);
    }

    #[test]
    fn hard_zero_checks_all_fields() {
        let baseline = SchematicMetricsSnapshot::from_quality(
            &SchematicQualityReport::default(),
            SemanticSnapshot::default(),
            "test",
            "test.mc",
            "",
        );
        let mut current = baseline.clone();
        current.fidelity.nets_dropped = 3;
        let report = compare_metrics_snapshot(&current, &baseline);
        assert!(!report.passed);
        assert!(report
            .failures
            .iter()
            .any(|f| f.path == "fidelity.nets_dropped"));
    }
}
