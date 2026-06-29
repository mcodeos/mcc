//! Iteration 01 · Acceptance yardstick: electrical fidelity (hard gate) + readability score (for ranking).
//!
//! - FidelityReport.is_perfect() = hard gate for all subsequent iterations.
//! - ReadabilityScore.weighted() = score for generate-and-rank (iteration 04).
//! - MetricsAccumulator passes through viz::api::render_layer_recursive, accumulating per layer.

use crate::vector::builder::builder_report::BuilderReport;
use crate::vector::graph::{McVecGraph, NetKind};
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
}

impl MetricsAccumulator {
    /// Accumulate **one layer** (graph.sub_graphs already taken by render, this layer only).
    /// `col` is the audit_all result for this layer.
    pub fn accumulate_layer(&mut self, graph: &McVecGraph, col: &CollisionReport) {
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
    }

    /// Merge build-phase dropped/partial, produce final two reports.
    pub fn finish(self, report: Option<&BuilderReport>) -> (FidelityReport, ReadabilityScore) {
        let (dropped, partial) = report
            .map(|r| (r.dropped_nets.len(), r.partial_nets.len()))
            .unwrap_or((0, 0));

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
            box_box: self.box_box,
            wire_box: self.wire_box,
        };
        let readability = ReadabilityScore {
            wire_wire: self.wire_wire,
            total_wirelength: self.total_wirelength,
            total_bends: self.total_bends,
            off_grid_penalty: self.off_grid_penalty,
            symmetry_penalty: 0.0, // [P1 placeholder] 06
            idiom_violation: 0,    // [P1 placeholder] 06
        };
        (fidelity, readability)
    }
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
    use crate::vector::graph::box_def::IoSummary;
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
        acc.accumulate_layer(&g, &CollisionReport::default());
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
            a.accumulate_layer(&g, &CollisionReport::default());
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
        acc.accumulate_layer(&g, &rep);
        let (fid, _) = acc.finish(None);
        assert!(fid.box_box >= 1);
        assert!(!fid.is_perfect());
    }
}
