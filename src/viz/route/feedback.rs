// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Milestone 9 — Route Feedback Loop
//!
//! Establishes a feedback loop that audits conflicts, scores route quality,
//! and accepts or rolls back reroute attempts.
//!
//! ## Pipeline
//! ```text
//! initial route from scheduler
//!   ↓
//! run_route_feedback(graph, grid, acfg, config)
//!   ↓
//! for iter in 0..max_iters:
//!     audit current route
//!     collect per-net conflicts
//!     rank nets by severity/span
//!     for each bad net:
//!         rip-up candidate
//!         reroute
//!         score before/after
//!         accept if better and truth preserved
//!         else rollback
//!     stop if no improvement
//!   ↓
//! final audit → report
//! ```

use crate::vector::graph::{EntryPoint, EntrySide, McVecBox, McVecGraph, Segment, VizNet};

use super::audit::{self, CollisionReport};
use super::grid_router::{self, AStarCfg, Grid, GRID_GAP};

// ============================================================================
// RouteFeedbackConfig
// ============================================================================

#[derive(Debug, Clone)]
pub struct RouteFeedbackConfig {
    pub max_iters: usize,
    pub max_reroutes_per_iter: usize,
    pub allow_multi_point_reroute: bool,
    pub allow_label_avoidance: bool,
    pub accept_equal_if_shorter: bool,
    pub max_length_increase_ratio: f64,
    pub max_bend_increase: usize,
    pub hist_inc: i64,
}

impl Default for RouteFeedbackConfig {
    fn default() -> Self {
        Self {
            max_iters: 8,
            max_reroutes_per_iter: 64,
            allow_multi_point_reroute: true,
            allow_label_avoidance: true,
            accept_equal_if_shorter: true,
            max_length_increase_ratio: 0.25,
            max_bend_increase: 4,
            hist_inc: 60,
        }
    }
}

// ============================================================================
// RouteFeedbackReport
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct RouteFeedbackReport {
    pub iterations: usize,
    pub nets_considered: usize,
    pub nets_rerouted: usize,
    pub reroutes_accepted: usize,
    pub reroutes_rejected: usize,
    pub conflicts_before: RouteConflictSummary,
    pub conflicts_after: RouteConflictSummary,
    pub quality_before: RouteQualityScore,
    pub quality_after: RouteQualityScore,
    pub unresolved: Vec<RouteConflict>,
}

// ============================================================================
// RouteConflictSummary
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteConflictSummary {
    pub wire_box: usize,
    pub wire_wire: usize,
    pub label_wire: usize,
    pub route_unreached: usize,
    pub empty_route: usize,
}

impl RouteConflictSummary {
    pub fn from_collision_report(rep: &CollisionReport, graph: &McVecGraph) -> Self {
        let empty_route = graph
            .nets
            .iter()
            .filter(|n| {
                n.route.is_none() || n.route.as_ref().map_or(true, |r| r.segments.is_empty())
            })
            .count();
        let route_unreached = graph
            .nets
            .iter()
            .filter(|n| {
                n.route.is_some()
                    && !n.route.as_ref().map_or(true, |r| r.segments.is_empty())
                    && !check_endpoint_reachability(graph, n)
            })
            .count();
        Self {
            wire_box: rep.wire_box,
            wire_wire: rep.wire_wire,
            label_wire: 0,
            route_unreached,
            empty_route,
        }
    }

    pub fn total(&self) -> usize {
        self.wire_box + self.wire_wire + self.label_wire + self.route_unreached + self.empty_route
    }
}

// ============================================================================
// RouteConflict
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteConflict {
    pub net_id: i64,
    pub net_name: String,
    pub kind: RouteConflictKind,
    pub severity: RouteConflictSeverity,
    pub segment_index: Option<usize>,
    pub other_net_id: Option<i64>,
    pub box_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteConflictKind {
    WireBox,
    WireWire,
    LabelWire,
    EmptyRoute,
    EndpointUnreached,
    ExcessiveBends,
    ExcessiveLength,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteConflictSeverity {
    Hard,
    Soft,
}

// ============================================================================
// RouteQualityScore
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq, PartialOrd)]
pub struct RouteQualityScore {
    pub wire_box: usize,
    pub wire_wire: usize,
    pub label_wire: usize,
    pub endpoint_unreached: usize,
    pub bends: usize,
    pub length: f64,
    pub segments: usize,
    pub weighted: f64,
}

impl RouteQualityScore {
    const WIRE_BOX_WEIGHT: f64 = 100_000.0;
    const ENDPOINT_UNREACHED_WEIGHT: f64 = 100_000.0;
    const LABEL_WIRE_WEIGHT: f64 = 5_000.0;
    const WIRE_WIRE_WEIGHT: f64 = 3_000.0;
    const BEND_WEIGHT: f64 = 20.0;

    pub fn compute(conflicts: &[RouteConflict], graph: &McVecGraph) -> Self {
        let mut score = Self::default();
        for c in conflicts {
            match c.kind {
                RouteConflictKind::WireBox => score.wire_box += 1,
                RouteConflictKind::WireWire => score.wire_wire += 1,
                RouteConflictKind::LabelWire => score.label_wire += 1,
                RouteConflictKind::EmptyRoute => {}
                RouteConflictKind::EndpointUnreached => score.endpoint_unreached += 1,
                RouteConflictKind::ExcessiveBends => score.bends += 1,
                RouteConflictKind::ExcessiveLength => {}
            }
        }
        // Count bends and length
        for net in &graph.nets {
            if let Some(ref route) = net.route {
                score.bends += route.segments.len().saturating_sub(1);
                score.segments = route.segments.len();
                score.length += route_length(&route.segments);
            }
        }
        score.weighted = score.compute_weighted();
        score
    }

    pub fn compute_weighted(&self) -> f64 {
        self.wire_box as f64 * Self::WIRE_BOX_WEIGHT
            + self.endpoint_unreached as f64 * Self::ENDPOINT_UNREACHED_WEIGHT
            + self.label_wire as f64 * Self::LABEL_WIRE_WEIGHT
            + self.wire_wire as f64 * Self::WIRE_WIRE_WEIGHT
            + self.bends as f64 * Self::BEND_WEIGHT
            + self.length
    }

    pub fn has_hard_conflict(&self) -> bool {
        self.wire_box > 0 || self.endpoint_unreached > 0
    }
}

// ============================================================================
// Per-net conflict collection
// ============================================================================

pub fn collect_net_conflicts(graph: &McVecGraph, net_index: usize) -> Vec<RouteConflict> {
    let mut conflicts = Vec::new();
    let net = &graph.nets[net_index];

    if net.route.is_none() || net.route.as_ref().map_or(true, |r| r.segments.is_empty()) {
        conflicts.push(RouteConflict {
            net_id: net.nid,
            net_name: net.name.clone(),
            kind: RouteConflictKind::EmptyRoute,
            severity: RouteConflictSeverity::Hard,
            segment_index: None,
            other_net_id: None,
            box_id: None,
        });
        return conflicts;
    }

    let route = net.route.as_ref().unwrap();
    let ep_ids: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();

    // wire-box
    for (si, seg) in route.segments.iter().enumerate() {
        for b in &graph.boxes {
            if ep_ids.contains(&b.id) {
                continue;
            }
            if segment_hits_box(seg, b) {
                conflicts.push(RouteConflict {
                    net_id: net.nid,
                    net_name: net.name.clone(),
                    kind: RouteConflictKind::WireBox,
                    severity: RouteConflictSeverity::Hard,
                    segment_index: Some(si),
                    other_net_id: None,
                    box_id: Some(b.id),
                });
            }
        }
    }

    // wire-wire
    for (ni, other) in graph.nets.iter().enumerate() {
        if ni == net_index {
            continue;
        }
        if let Some(ref other_route) = other.route {
            for seg in &route.segments {
                for oseg in &other_route.segments {
                    if audit::seg_cross_point(seg, oseg).is_some() {
                        conflicts.push(RouteConflict {
                            net_id: net.nid,
                            net_name: net.name.clone(),
                            kind: RouteConflictKind::WireWire,
                            severity: RouteConflictSeverity::Soft,
                            segment_index: None,
                            other_net_id: Some(other.nid),
                            box_id: None,
                        });
                        break;
                    }
                }
            }
        }
    }

    // endpoint unreached
    if !check_endpoint_reachability(graph, net) {
        conflicts.push(RouteConflict {
            net_id: net.nid,
            net_name: net.name.clone(),
            kind: RouteConflictKind::EndpointUnreached,
            severity: RouteConflictSeverity::Hard,
            segment_index: None,
            other_net_id: None,
            box_id: None,
        });
    }

    conflicts
}

// ============================================================================
// Accept / rollback
// ============================================================================

pub fn should_accept_reroute(
    old: &RouteQualityScore,
    new: &RouteQualityScore,
    config: &RouteFeedbackConfig,
) -> bool {
    // Must not introduce or increase hard conflicts
    if new.wire_box > old.wire_box || new.endpoint_unreached > old.endpoint_unreached {
        return false;
    }

    // If old has hard conflict, new can be longer/more bends
    if old.has_hard_conflict() {
        if new.wire_box < old.wire_box || new.endpoint_unreached < old.endpoint_unreached {
            return true; // Hard conflict reduced → accept even if longer
        }
    }

    // Length increase constraint
    if old.length > 0.0 {
        let length_ratio = new.length / old.length;
        if length_ratio > 1.0 + config.max_length_increase_ratio {
            return false;
        }
    }

    // Bend increase constraint
    if new.bends > old.bends + config.max_bend_increase {
        return false;
    }

    // Weighted comparison
    if new.weighted < old.weighted {
        return true;
    }

    if config.accept_equal_if_shorter && new.weighted == old.weighted && new.length < old.length {
        return true;
    }

    false
}

// ============================================================================
// Endpoint reachability
// ============================================================================

pub fn check_endpoint_reachability(graph: &McVecGraph, net: &VizNet) -> bool {
    let route = match &net.route {
        Some(r) => r,
        None => return false,
    };
    if route.segments.is_empty() {
        return false;
    }

    for ep in &net.endpoints {
        let b = match graph.boxes.iter().find(|x| x.id == ep.box_id) {
            Some(b) => b,
            None => return false,
        };
        let entry = match b.find_entry(ep.pin_id) {
            Some(e) => e,
            None => return false,
        };
        let (ax, ay) = pin_position_from_entry(b, entry);
        // Check that at least one segment endpoint is close to the anchor
        let mut touched = false;
        for seg in &route.segments {
            let d1 = ((seg.from.x - ax).powi(2) + (seg.from.y - ay).powi(2)).sqrt();
            let d2 = ((seg.to.x - ax).powi(2) + (seg.to.y - ay).powi(2)).sqrt();
            if d1 < 2.0 || d2 < 2.0 {
                touched = true;
                break;
            }
        }
        if !touched {
            return false;
        }
    }
    true
}

// ============================================================================
// Score a single net's route
// ============================================================================

pub fn score_net_route(graph: &McVecGraph, net_index: usize) -> RouteQualityScore {
    let conflicts = collect_net_conflicts(graph, net_index);
    let mut score = RouteQualityScore::default();
    for c in &conflicts {
        match c.kind {
            RouteConflictKind::WireBox => score.wire_box += 1,
            RouteConflictKind::WireWire => score.wire_wire += 1,
            RouteConflictKind::LabelWire => score.label_wire += 1,
            RouteConflictKind::EmptyRoute => {}
            RouteConflictKind::EndpointUnreached => score.endpoint_unreached += 1,
            RouteConflictKind::ExcessiveBends => {}
            RouteConflictKind::ExcessiveLength => {}
        }
    }
    let net = &graph.nets[net_index];
    if let Some(ref route) = net.route {
        score.bends = route.segments.len().saturating_sub(1);
        score.segments = route.segments.len();
        score.length = route_length(&route.segments);
    }
    score.weighted = score.compute_weighted();
    score
}

// ============================================================================
// Route feedback loop
// ============================================================================

pub fn run_route_feedback(
    graph: &mut McVecGraph,
    grid: &mut Grid,
    acfg: &AStarCfg,
    config: &RouteFeedbackConfig,
) -> RouteFeedbackReport {
    let before_audit = audit::audit_collisions(graph);
    let before_summary = RouteConflictSummary::from_collision_report(&before_audit, graph);
    let before_quality = RouteQualityScore::compute(&[], graph);

    let mut report = RouteFeedbackReport {
        conflicts_before: before_summary,
        quality_before: before_quality,
        ..Default::default()
    };

    for iter in 0..config.max_iters {
        bump_crossings(grid, graph, config.hist_inc);

        // Collect conflict nets
        let mut conflict: Vec<(f64, usize)> = (0..graph.nets.len())
            .filter(|&i| audit::net_has_conflict(graph, i))
            .map(|i| (net_span(graph, &graph.nets[i]), i))
            .collect();
        if conflict.is_empty() {
            break;
        }
        conflict.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        report.nets_considered += conflict.len();
        let mut rerouted_this_iter = 0usize;
        let mut accepted_this_iter = 0usize;
        let mut rejected_this_iter = 0usize;

        for (_, net_idx) in conflict {
            if rerouted_this_iter >= config.max_reroutes_per_iter {
                break;
            }

            if !audit::net_has_conflict(graph, net_idx) {
                continue;
            }

            let nid = graph.nets[net_idx].nid;
            let n_eps = graph.nets[net_idx].endpoints.len();

            let old_score = score_net_route(graph, net_idx);

            // Rip-up
            grid.unreserve_net(nid);

            let placeholder = VizNet::new(
                0,
                String::new(),
                crate::vector::graph::NetKind::Signal,
                vec![],
            );
            let mut tmp = std::mem::replace(&mut graph.nets[net_idx], placeholder);

            let new_route = if n_eps == 2 {
                grid_router::reroute_two_point(grid, graph, &tmp, acfg)
            } else if n_eps > 2 && config.allow_multi_point_reroute {
                grid_router::reroute_multi_point(grid, graph, &tmp, acfg)
            } else {
                None
            };

            rerouted_this_iter += 1;

            if let Some(r2) = new_route {
                let old_route = tmp.route.clone();
                tmp.route = Some(r2);
                graph.nets[net_idx] = std::mem::replace(
                    &mut tmp,
                    VizNet::new(
                        0,
                        String::new(),
                        crate::vector::graph::NetKind::Signal,
                        vec![],
                    ),
                );

                let new_score = score_net_route(graph, net_idx);

                if should_accept_reroute(&old_score, &new_score, config) {
                    accepted_this_iter += 1;
                    crate::vlog!(
                        "[route::feedback] iter={} net='{}' nid={} ACCEPT (old weighted={:.0} → new weighted={:.0})",
                        iter,
                        graph.nets[net_idx].name,
                        nid,
                        old_score.weighted,
                        new_score.weighted
                    );
                } else {
                    rejected_this_iter += 1;
                    // Rollback
                    graph.nets[net_idx].route = old_route;
                    crate::vlog!(
                        "[route::feedback] iter={} net='{}' nid={} REJECT (old weighted={:.0} → new weighted={:.0})",
                        iter,
                        graph.nets[net_idx].name,
                        nid,
                        old_score.weighted,
                        new_score.weighted
                    );
                }
            } else {
                rejected_this_iter += 1;
                graph.nets[net_idx] = std::mem::replace(
                    &mut tmp,
                    VizNet::new(
                        0,
                        String::new(),
                        crate::vector::graph::NetKind::Signal,
                        vec![],
                    ),
                );
            }

            // Re-reserve
            if let Some(ref r) = graph.nets[net_idx].route {
                grid.reserve_segments(&r.segments, nid, GRID_GAP);
            }
        }

        report.iterations = iter + 1;
        report.nets_rerouted += rerouted_this_iter;
        report.reroutes_accepted += accepted_this_iter;
        report.reroutes_rejected += rejected_this_iter;

        if accepted_this_iter == 0 {
            break;
        }
    }

    // Final audit
    let after_audit = audit::audit_collisions(graph);
    report.conflicts_after = RouteConflictSummary::from_collision_report(&after_audit, graph);
    report.quality_after = RouteQualityScore::compute(&[], graph);

    // Collect unresolved conflicts
    report.unresolved = (0..graph.nets.len())
        .filter(|&i| audit::net_has_conflict(graph, i))
        .flat_map(|i| collect_net_conflicts(graph, i))
        .collect();

    crate::vlog!(
        "[route::feedback] iters={} considered={} rerouted={} accepted={} rejected={} before wb={} ww={} after wb={} ww={}",
        report.iterations,
        report.nets_considered,
        report.nets_rerouted,
        report.reroutes_accepted,
        report.reroutes_rejected,
        report.conflicts_before.wire_box,
        report.conflicts_before.wire_wire,
        report.conflicts_after.wire_box,
        report.conflicts_after.wire_wire,
    );

    report
}

// ============================================================================
// Helpers
// ============================================================================

fn pin_position_from_entry(b: &McVecBox, ep: &EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Top => (b.x + b.w * ep.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * ep.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * ep.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * ep.offset),
    }
}

fn segment_hits_box(seg: &Segment, b: &McVecBox) -> bool {
    let inflate = 2.0;
    let (rx, ry, rw, rh) = (
        b.x - inflate,
        b.y - inflate,
        b.w + 2.0 * inflate,
        b.h + 2.0 * inflate,
    );
    let (x1, y1) = (seg.from.x, seg.from.y);
    let (x2, y2) = (seg.to.x, seg.to.y);

    if (x1 >= rx && x1 <= rx + rw && y1 >= ry && y1 <= ry + rh)
        || (x2 >= rx && x2 <= rx + rw && y2 >= ry && y2 <= ry + rh)
    {
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

fn route_length(segs: &[Segment]) -> f64 {
    segs.iter()
        .map(|s| ((s.to.x - s.from.x).powi(2) + (s.to.y - s.from.y).powi(2)).sqrt())
        .sum()
}

fn net_span(graph: &McVecGraph, net: &VizNet) -> f64 {
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut any = false;
    for e in &net.endpoints {
        if let Some(b) = graph.boxes.iter().find(|x| x.id == e.box_id) {
            let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            minx = minx.min(cx);
            miny = miny.min(cy);
            maxx = maxx.max(cx);
            maxy = maxy.max(cy);
            any = true;
        }
    }
    if !any {
        0.0
    } else {
        ((maxx - minx).powi(2) + (maxy - miny).powi(2)).sqrt()
    }
}

fn bump_crossings(grid: &mut Grid, graph: &McVecGraph, amount: i64) {
    let mut segs: Vec<(i64, &Segment)> = Vec::new();
    for net in &graph.nets {
        if let Some(r) = &net.route {
            for s in &r.segments {
                segs.push((net.nid, s));
            }
        }
    }
    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            if segs[i].0 == segs[j].0 {
                continue;
            }
            if let Some((x, y)) = audit::seg_cross_point(segs[i].1, segs[j].1) {
                grid.bump_history_at(x, y, amount);
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::net_def::{EndpointRef, IoDirection, Route};
    use crate::vector::graph::{
        BoxKind, EntryPoint, EntrySide, IoSummary, NetKind, Point, Segment, Symbol, VizNet,
    };

    fn mk_box(
        id: i64,
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> crate::vector::graph::McVecBox {
        let mut b = crate::vector::graph::McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        b.entry_points.push(EntryPoint {
            pin_id: 1,
            side: EntrySide::Right,
            offset: 0.0,
            pin_name: "1".into(),
        });
        b
    }

    fn mk_net(nid: i64, name: &str, segments: Vec<(f64, f64, f64, f64)>) -> VizNet {
        let mut net = VizNet::new(nid, name.into(), NetKind::Signal, vec![]);
        net.route = Some(Route {
            segments: segments
                .into_iter()
                .map(|(x1, y1, x2, y2)| Segment {
                    from: Point { x: x1, y: y1 },
                    to: Point { x: x2, y: y2 },
                })
                .collect(),
            junctions: vec![],
        });
        net
    }

    fn mk_ep(box_id: i64, pin_id: i64) -> EndpointRef {
        EndpointRef {
            box_id,
            pin_id,
            pin_name: String::new(),
            io_type: IoDirection::Unknown,
            pin_number: None,
        }
    }

    // ── Score: wire_box weight > wire_wire weight ──

    #[test]
    fn wire_box_heavier_than_wire_wire() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(1, "R1", 50.0, 50.0, 30.0, 30.0));
        graph.boxes.push(mk_box(2, "R2", 100.0, 80.0, 30.0, 30.0));
        let mut net = mk_net(1, "SIG", vec![(40.0, 60.0, 120.0, 60.0)]);
        net.endpoints.push(mk_ep(1, 1));
        net.endpoints.push(mk_ep(2, 1));
        graph.nets.push(net);

        let conflicts = collect_net_conflicts(&graph, 0);
        let score = RouteQualityScore::compute(&conflicts, &graph);
        // wire_box weight 100K, wire_wire weight 3K
        assert!(score.weighted >= 100_000.0);
    }

    // ── Score: no conflicts → low score ──

    #[test]
    fn no_conflicts_low_score() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(1, "R1", 50.0, 50.0, 30.0, 30.0));
        graph.boxes.push(mk_box(2, "R2", 100.0, 50.0, 30.0, 30.0));
        // Wire that goes outside both boxes, no endpoints so no reachability check
        let net = mk_net(1, "SIG", vec![(200.0, 65.0, 250.0, 65.0)]);
        graph.nets.push(net);

        let conflicts = collect_net_conflicts(&graph, 0);
        let score = RouteQualityScore::compute(&conflicts, &graph);
        assert!(score.weighted < 100_000.0);
    }

    // ── Accept: wire_box reduced → accept ──

    #[test]
    fn wire_box_reduced_accepts() {
        let config = RouteFeedbackConfig::default();
        let old = RouteQualityScore {
            wire_box: 1,
            weighted: 100_000.0,
            ..Default::default()
        };
        let new = RouteQualityScore {
            wire_box: 0,
            weighted: 200.0,
            ..Default::default()
        };
        assert!(should_accept_reroute(&old, &new, &config));
    }

    // ── Accept: candidate longer without reducing conflict → reject ──

    #[test]
    fn longer_without_conflict_reduction_rejects() {
        let config = RouteFeedbackConfig {
            max_length_increase_ratio: 0.25,
            ..Default::default()
        };
        let old = RouteQualityScore {
            wire_box: 0,
            wire_wire: 0,
            length: 100.0,
            weighted: 100.0,
            ..Default::default()
        };
        let new = RouteQualityScore {
            wire_box: 0,
            wire_wire: 0,
            length: 200.0,
            weighted: 200.0,
            ..Default::default()
        };
        assert!(!should_accept_reroute(&old, &new, &config));
    }

    // ── Accept: hard conflict allows longer route ──

    #[test]
    fn hard_conflict_allows_longer_route() {
        let config = RouteFeedbackConfig::default();
        let old = RouteQualityScore {
            wire_box: 1,
            length: 100.0,
            weighted: 100_100.0,
            ..Default::default()
        };
        let new = RouteQualityScore {
            wire_box: 0,
            length: 300.0,
            weighted: 300.0,
            ..Default::default()
        };
        assert!(should_accept_reroute(&old, &new, &config));
    }

    // ── Accept: endpoint unreached → reject ──

    #[test]
    fn endpoint_unreached_rejects() {
        let config = RouteFeedbackConfig::default();
        let old = RouteQualityScore {
            endpoint_unreached: 0,
            weighted: 100.0,
            ..Default::default()
        };
        let new = RouteQualityScore {
            endpoint_unreached: 1,
            weighted: 100_000.0,
            ..Default::default()
        };
        assert!(!should_accept_reroute(&old, &new, &config));
    }

    // ── Accept: no improvement → reject ──

    #[test]
    fn no_improvement_rejects() {
        let config = RouteFeedbackConfig::default();
        let old = RouteQualityScore {
            wire_wire: 1,
            length: 100.0,
            weighted: 3_100.0,
            ..Default::default()
        };
        let new = RouteQualityScore {
            wire_wire: 1,
            length: 100.0,
            weighted: 3_100.0,
            ..Default::default()
        };
        assert!(!should_accept_reroute(&old, &new, &config));
    }

    // ── Reachability: all endpoints touched → true ──

    #[test]
    fn all_endpoints_touched() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut b = mk_box(1, "R1", 50.0, 50.0, 30.0, 30.0);
        b.entry_points[0].side = EntrySide::Right;
        b.entry_points[0].offset = 0.0;
        graph.boxes.push(b.clone());

        let mut net = mk_net(1, "SIG", vec![(80.0, 50.0, 100.0, 50.0)]);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        assert!(check_endpoint_reachability(&graph, &graph.nets[0]));
    }

    // ── Reachability: empty route → false ──

    #[test]
    fn empty_route_not_reachable() {
        let graph = McVecGraph::new(0, "test".into());
        let net = VizNet::new(1, "SIG".into(), NetKind::Signal, vec![]);
        assert!(!check_endpoint_reachability(&graph, &net));
    }

    // ── ConflictSummary from CollisionReport ──

    #[test]
    fn conflict_summary_from_collision_report() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut net = mk_net(1, "SIG", vec![(0.0, 0.0, 100.0, 0.0)]);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let rep = CollisionReport {
            wire_box: 2,
            wire_wire: 3,
            ..Default::default()
        };
        let summary = RouteConflictSummary::from_collision_report(&rep, &graph);
        assert_eq!(summary.wire_box, 2);
        assert_eq!(summary.wire_wire, 3);
    }

    // ── RouteQualityScore has_hard_conflict ──

    #[test]
    fn has_hard_conflict_wire_box() {
        let score = RouteQualityScore {
            wire_box: 1,
            ..Default::default()
        };
        assert!(score.has_hard_conflict());
    }

    #[test]
    fn no_hard_conflict() {
        let score = RouteQualityScore {
            wire_wire: 1,
            ..Default::default()
        };
        assert!(!score.has_hard_conflict());
    }

    // ── Score: weighted computed correctly ──

    #[test]
    fn weighted_score_computation() {
        let score = RouteQualityScore {
            wire_box: 1,
            wire_wire: 2,
            bends: 3,
            length: 50.0,
            ..Default::default()
        };
        let expected = 100_000.0 + 2.0 * 3_000.0 + 3.0 * 20.0 + 50.0;
        assert!((score.compute_weighted() - expected).abs() < 0.01);
    }

    // ── RouteFeedbackConfig defaults ──

    #[test]
    fn config_defaults_sane() {
        let c = RouteFeedbackConfig::default();
        assert_eq!(c.max_iters, 8);
        assert!(c.max_length_increase_ratio > 0.0);
    }
}
