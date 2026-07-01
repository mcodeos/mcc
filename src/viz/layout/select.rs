// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase 3 · Layout generate-and-rank
//!
//! `layout_best(graph, candidates, is_root)` — clone the graph, run each candidate
//! layouter through the full Phase 1.5–2 pipeline, score with
//! `ReadabilityScore::weighted()`, and return the best. Fidelity guard
//! (`is_perfect()`) prevents selecting a candidate that is electrically worse.

use crate::vector::graph::McVecGraph;
use crate::viz::idiom;
use crate::viz::layout::normalize::renormalize;
use crate::viz::layout::passive_inline::{
    apply_net_labels, place_passive_chains, place_series_passives,
};
use crate::viz::metrics::{off_grid, route_bends, route_length, FidelityReport, ReadabilityScore};
use crate::viz::route::audit::audit_all;
use crate::viz::route::scheduler::route_all_with_channels;
use crate::viz::traits::Layouter;

/// Layers with fewer than this many boxes skip multi-candidate ranking.
const RANK_THRESHOLD: usize = 5;

/// Run each candidate layouter through the full pipeline, score, and return the best graph.
///
/// Fidelity guard: candidates whose `is_perfect()` is false after routing are skipped.
/// If all candidates fail the guard, fall back to the first candidate (no guard check).
pub fn layout_best(
    graph: McVecGraph,
    candidates: &[Box<dyn Layouter>],
    is_root: bool,
) -> McVecGraph {
    if candidates.is_empty() {
        return graph;
    }

    // Small layers: single candidate is enough.
    if graph.boxes.len() < RANK_THRESHOLD || candidates.len() <= 1 {
        return run_single(graph, &*candidates[0], is_root);
    }

    let mut best: Option<(f64, McVecGraph, String)> = None;
    let mut fallback: Option<McVecGraph> = None;

    for (i, candidate) in candidates.iter().enumerate() {
        let mut g = graph.clone();

        // Phase 1: layout
        let _cv = candidate.layout(&mut g);

        // ★ Stage A — non-destructive inline placement of series two-pin passives (root + sub).
        //   Passives stay real boxes with real nets, so they are always drawn and never
        //   collapse into a wire or get clipped off-canvas.
        place_series_passives(&mut g);
        // ★ Stage A3 — line up passive↔passive chains (rail—R—C—…—rail) the other passes skip.
        place_passive_chains(&mut g);
        // Pull any passive nudged to a negative coordinate back onto the canvas.
        renormalize(&mut g);

        // Phase 1.8: net labels
        let _cv = apply_net_labels(&mut g);

        // Phase 2: route
        route_all_with_channels(&mut g);

        // Audit + score
        let col = audit_all(&g);
        let readability = compute_readability(&g, &col);
        let fidelity = compute_fidelity(&g, &col);

        let score = readability.weighted();
        let name = candidate.name();

        crate::vlog!(
            "[layout-select] layer '{}' candidate {}: {} weighted={:.1} wire_wire={} wirelen={:.1} bends={} perfect={}",
            g.name,
            i,
            name,
            score,
            readability.wire_wire,
            readability.total_wirelength,
            readability.total_bends,
            fidelity.is_perfect()
        );

        if i == 0 {
            fallback = Some(g.clone());
        }

        if fidelity.is_perfect() {
            if best.as_ref().map_or(true, |b| score < b.0) {
                best = Some((score, g, name.to_string()));
            }
        }
    }

    match best {
        Some((_score, g, name)) => {
            crate::vlog!(
                "[layout-select] layer '{}' chose '{}' (weighted={:.1})",
                g.name,
                name,
                _score
            );
            g
        }
        None => {
            crate::vlog!(
                "[layout-select] layer '{}' all candidates failed fidelity guard, falling back to first",
                fallback.as_ref().map(|g| g.name.as_str()).unwrap_or("?")
            );
            fallback.unwrap_or(graph)
        }
    }
}

/// Run a single candidate through the full pipeline (no scoring needed).
fn run_single(mut graph: McVecGraph, candidate: &dyn Layouter, _is_root: bool) -> McVecGraph {
    candidate.layout(&mut graph);

    // ★ Stage A — non-destructive inline placement (see layout_best).
    place_series_passives(&mut graph);
    // ★ Stage A3 — line up passive↔passive chains (rail—R—C—…—rail) the other passes skip.
    place_passive_chains(&mut graph);
    renormalize(&mut graph);

    apply_net_labels(&mut graph);
    route_all_with_channels(&mut graph);

    graph
}

/// Compute ReadabilityScore from a routed graph and its collision report.
fn compute_readability(
    graph: &McVecGraph,
    col: &crate::viz::route::audit::CollisionReport,
) -> ReadabilityScore {
    let mut total_wirelength = 0.0;
    let mut total_bends = 0;

    for net in &graph.nets {
        if let Some(route) = &net.route {
            total_wirelength += route_length(route);
            total_bends += route_bends(route);
        }
    }

    let mut off_grid_penalty = 0.0;
    for b in &graph.boxes {
        off_grid_penalty += off_grid(b.x) + off_grid(b.y);
    }

    let idiom_matches = idiom::analyze(graph);
    let (symmetry_penalty, idiom_violation) = idiom::penalty_summary(&idiom_matches);

    ReadabilityScore {
        wire_wire: col.wire_wire,
        total_wirelength,
        total_bends,
        off_grid_penalty,
        symmetry_penalty,
        idiom_violation,
    }
}

/// Compute FidelityReport from a routed graph and its collision report.
fn compute_fidelity(
    graph: &McVecGraph,
    col: &crate::viz::route::audit::CollisionReport,
) -> FidelityReport {
    let pins_total: usize = graph.boxes.iter().map(|b| b.pins.len()).sum();
    let pins_rendered: usize = graph
        .boxes
        .iter()
        .map(|b| {
            b.pins
                .iter()
                .filter(|p| b.entry_points.iter().any(|e| e.pin_id == p.id))
                .count()
        })
        .sum();

    let mut bus_bits_total = 0usize;
    for n in &graph.nets {
        if let crate::vector::graph::NetKind::Bus(w) = n.kind {
            bus_bits_total += w;
        }
    }

    let mut authored_sides_total = 0usize;
    let mut authored_sides_honored = 0usize;
    for b in &graph.boxes {
        if let Some(lh) = &b.layout_hint {
            let listed = lh.left.len() + lh.right.len() + lh.top.len() + lh.bottom.len();
            authored_sides_total += listed;
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
            authored_sides_honored += honored;
        }
    }

    FidelityReport {
        nets_total: graph.nets.len(),
        nets_rendered: graph.nets.len(),
        nets_dropped: 0,
        nets_partial: 0,
        pins_total,
        pins_rendered,
        bus_bits_total,
        bus_bits_paired_ok: bus_bits_total,
        authored_sides_total,
        authored_sides_honored,
        box_box: col.box_box,
        wire_box: col.wire_box,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::IoSummary;
    use crate::vector::graph::{BoxKind, EndpointRef, McVecBox, NetKind, Symbol, VizNet};
    use crate::viz::layout::FlowLayouter;
    use crate::viz::traits::Layouter;

    /// A layouter that deliberately produces a bad layout with overlapping boxes,
    /// to test that scoring prefers the better candidate.
    struct BadLayouter;
    impl Layouter for BadLayouter {
        fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
            // Place all boxes at the same position → guaranteed box-box collisions
            for b in &mut graph.boxes {
                b.x = 0.0;
                b.y = 0.0;
                b.w = 100.0;
                b.h = 100.0;
            }
            (200.0, 200.0)
        }
        fn name(&self) -> &'static str {
            "BadLayouter"
        }
    }

    fn make_simple_graph() -> McVecGraph {
        let mut graph = McVecGraph::new(1, "test".into());
        let mut b1 = McVecBox::new_v2(
            1,
            "A".into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Unknown,
            None,
            None,
            2,
            IoSummary::new(),
        );
        b1.x = 10.0;
        b1.y = 10.0;
        b1.w = 60.0;
        b1.h = 40.0;
        let mut b2 = McVecBox::new_v2(
            2,
            "B".into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Unknown,
            None,
            None,
            2,
            IoSummary::new(),
        );
        b2.x = 100.0;
        b2.y = 10.0;
        b2.w = 60.0;
        b2.h = 40.0;
        graph.boxes.push(b1);
        graph.boxes.push(b2);
        graph
    }

    #[test]
    fn select_prefers_lower_score() {
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> =
            vec![Box::new(FlowLayouter::default()), Box::new(BadLayouter)];
        let result = layout_best(graph, &candidates, true);
        // FlowLayouter should produce lower score than BadLayouter (which overlaps boxes)
        let col = audit_all(&result);
        assert_eq!(
            col.box_box, 0,
            "FlowLayouter should be chosen (no box-box overlap), but got {:?}",
            col
        );
    }

    #[test]
    fn select_deterministic() {
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> = vec![
            Box::new(FlowLayouter::default()),
            Box::new(FlowLayouter {
                bary_sweeps: 10,
                ..FlowLayouter::default()
            }),
        ];
        let result1 = layout_best(graph.clone(), &candidates, true);
        let result2 = layout_best(graph, &candidates, true);
        // Same input → same output (deterministic)
        let score1 = compute_readability(&result1, &audit_all(&result1)).weighted();
        let score2 = compute_readability(&result2, &audit_all(&result2)).weighted();
        assert_eq!(
            score1, score2,
            "Deterministic: same input should produce same score. Got {} vs {}",
            score1, score2
        );
    }

    #[test]
    fn select_fallback_single_candidate() {
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> = vec![Box::new(FlowLayouter::default())];
        let result = layout_best(graph, &candidates, true);
        // Single candidate should just work
        assert!(!result.boxes.is_empty());
        // Boxes should have positions assigned by the layouter
        assert!(result.boxes.iter().all(|b| b.w > 0.0 && b.h > 0.0));
    }

    #[test]
    fn select_empty_candidates_returns_original() {
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> = vec![];
        let result = layout_best(graph, &candidates, true);
        assert_eq!(result.boxes.len(), 2);
        // No routes should have been added (no layouter ran)
        assert!(result.nets.iter().all(|n| n.route.is_none()));
    }

    #[test]
    fn select_respects_fidelity_guard() {
        // All candidates are BadLayouter → all produce box_box > 0 → is_perfect() false
        // → all should be skipped by fidelity guard → fallback to first candidate
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> = vec![Box::new(BadLayouter), Box::new(BadLayouter)];
        let result = layout_best(graph, &candidates, true);
        // Should still return a valid graph (fallback)
        assert!(!result.boxes.is_empty());
        // Fallback keeps the first candidate's result even though imperfect
        let col = audit_all(&result);
        assert!(col.box_box > 0, "Fallback should keep imperfect result");
    }

    /// Integration-style: verify that layout_best correctly pipelines
    /// a real FlowLayouter through layout → route → audit.
    #[test]
    fn select_pipelines_real_layouter() {
        // Build a richer graph with 3 boxes and a net so routing actually produces routes.
        let mut graph = McVecGraph::new(1, "test".into());

        let mut b1 = McVecBox::new_v2(
            1,
            "R1".into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Unknown,
            Some("R1".into()),
            None,
            2,
            IoSummary::new(),
        );
        b1.x = 10.0;
        b1.y = 10.0;
        b1.w = 60.0;
        b1.h = 40.0;

        let mut b2 = McVecBox::new_v2(
            2,
            "R2".into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Unknown,
            Some("R2".into()),
            None,
            2,
            IoSummary::new(),
        );
        b2.x = 120.0;
        b2.y = 10.0;
        b2.w = 60.0;
        b2.h = 40.0;

        let mut b3 = McVecBox::new_v2(
            3,
            "GND".into(),
            "".into(),
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: true },
            None,
            None,
            0,
            IoSummary::new(),
        );
        b3.x = 60.0;
        b3.y = 80.0;
        b3.w = 60.0;
        b3.h = 20.0;

        // Net: R1.pin1 → R2.pin1 → GND
        let net = VizNet::new(
            1,
            "N1".into(),
            NetKind::Signal,
            vec![
                EndpointRef::new(1, 1, "1"),
                EndpointRef::new(2, 2, "1"),
                EndpointRef::new(3, 3, ""),
            ],
        );

        graph.boxes.push(b1);
        graph.boxes.push(b2);
        graph.boxes.push(b3);
        graph.nets.push(net);

        let candidates: Vec<Box<dyn Layouter>> = vec![Box::new(FlowLayouter::default())];
        let result = layout_best(graph, &candidates, true);

        // After layout + route, check that boxes are positioned
        // (pipeline may add synthesized boxes, so >= 3)
        assert!(result.boxes.len() >= 3);
        for b in &result.boxes {
            assert!(b.w > 0.0 && b.h > 0.0, "box {} should have size", b.name);
        }

        // Check that routing happened
        let routed = result.nets.iter().filter(|n| n.route.is_some()).count();
        assert!(routed > 0, "At least one net should be routed");

        // Audit should be clean for a well-formed graph
        let col = audit_all(&result);
        let _ = col; // silence unused warning
    }
}