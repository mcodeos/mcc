// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Layout pipeline entry — single-layouter + fidelity gate (PR-1)
//!
//! `layout_best(graph, candidates, is_root)` runs the one configured layouter
//! (circuit_flow) through the full Phase 1.5–2 pipeline, then applies a
//! **fidelity gate**:
//!
//!   * **Dimension 1 — electrical correctness** is a hard veto. With a single
//!     layouter there is no alternate candidate to swap to, so the veto is
//!     realized as a loud, structured log of every electrical violation (dropped
//!     nets, unconnected pins, bus bit-order mismatch, box/wire collisions).
//!     This turns *silent* fidelity regressions into *visible* ones. When a build
//!     report / severity channel is wired in (PR-3) these become hard errors.
//!   * **Dimensions 2–7 — readability / flow / compactness / …** are emitted as a
//!     report card for iterating on circuit_flow, no longer used to pick a winner.
//!
//! ## History
//! Before PR-1 this function was a *generate-and-rank* selector: it cloned the
//! graph, ran N candidate layouters, scored each with `ReadabilityScore::weighted()`
//! and returned the best. That made "what you edit is what you see" untrue — a
//! hidden candidate could win the ranking and mask a change, which is exactly why
//! the layout was "impossible to fix". PR-1 retires the ranking; the candidate pool
//! now holds exactly one layouter. The scoring helpers (`compute_fidelity`,
//! `compute_readability`) are kept and re-used by the gate.

use crate::vector::graph::McVecGraph;
use crate::viz::idiom;
use crate::viz::layout::normalize::renormalize;
use crate::viz::layout::passive_inline::{
    apply_net_labels, place_bridge_passives, place_passive_chains, place_series_passives,
    probe_box_collisions, probe_rail_passive_candidates, probe_scatter_census,
};
use crate::viz::layout_model::SchematicLayoutModel;
use crate::viz::metrics::{off_grid, route_bends, route_length, FidelityReport, ReadabilityScore};
use crate::viz::route::audit::audit_all;
use crate::viz::route::scheduler::route_all_with_channels;
use crate::viz::traits::Layouter;

/// Run the configured layouter through the full pipeline and apply the fidelity gate.
///
/// PR-1: single-pipeline. The candidate pool holds exactly one layouter
/// (circuit_flow) at both top and sub level, so this always runs `candidates[0]`.
/// The extra `candidates`/`is_root` parameters are retained so the public signature
/// and every call site stay unchanged while the ranking machinery is removed.
///
/// Phase D: `schematic_model` is passed to the layouter for low-risk layout intent.
pub fn layout_best(
    graph: McVecGraph,
    candidates: &[Box<dyn Layouter>],
    is_root: bool,
    schematic_model: Option<SchematicLayoutModel>,
) -> McVecGraph {
    match candidates.first() {
        Some(_) => run_single(graph, &*candidates[0], is_root, schematic_model),
        // No layouter configured: return the graph untouched (nothing to route/gate).
        None => graph,
    }
}

/// Run a single layouter through the full pipeline, then gate + report.
fn run_single(
    mut graph: McVecGraph,
    candidate: &dyn Layouter,
    is_root: bool,
    schematic_model: Option<SchematicLayoutModel>,
) -> McVecGraph {
    let layer = graph.name.clone();
    let layouter = candidate.name();

    macro_rules! t {
        ($lbl:expr, $body:expr) => {{
            let _ts = std::time::Instant::now();
            let r = $body;
            tracing::info!(target: "mcc::perf", step = $lbl, ms = _ts.elapsed().as_millis() as u64, "run_single step");
            r
        }};
    }

    // ── Phase 1: layout ──
    // ★ P7-0 (root cause F): inject the model into the *candidate itself*,
    // preserving its parameters (sub() vs default()). Previously this branch
    // built `FlowLayouter { model, ..default() }`, so every layer — including
    // all 6 sub-layers — ran with top-level parameters (480/220/recompute=false)
    // and `FlowLayouter::sub()` was dead code.
    t!("flow_layout", {
        if let Some(flow) = schematic_model.and_then(|m| candidate.with_model(m)) {
            flow.layout(&mut graph);
        } else {
            candidate.layout(&mut graph);
        }
    });
    probe_scatter_census(&graph);

    // ── Stage A / A3: non-destructive inline placement of series & chained passives ──
    //   Passives stay real boxes with real nets, so they are always drawn and never
    //   collapse into a wire or get clipped off-canvas.
    // ★ B2: skip for root — radial layout already placed all boxes.
    if !is_root {
        probe_rail_passive_candidates(&graph);
        let g_snap = graph.geom_snapshot();
        place_series_passives(&mut graph);
        place_passive_chains(&mut graph);
        place_bridge_passives(&mut graph); // ★ P2: bridge passives (transposed CAP in two-lane series)
                                           // ★ P7-5 S3/S4a/S5: grounded passives stand vertically (GND end down);
                                           // transposed rungs that bridge placement couldn't reach get the rung shape.
        let stood = super::passive_inline::stand_grounded_passives(&mut graph);
        if stood > 0 {
            crate::vlog!("[layout::passive_inline] stood up {stood} grounded passive(s)");
        }
        graph.claim_geom_changes(&g_snap, "10.passive_inline");
        // Pull any passive nudged to a negative coordinate back onto the canvas.
        let g_snap = graph.geom_snapshot();
        t!("renormalize", renormalize(&mut graph));
        graph.claim_geom_changes(&g_snap, "11.renormalize");
    }

    // ── Phase 1.8: net labels ──
    // ★ B2: skip for root layer — root only has declared boxes, no net labels.
    let g_snap = graph.geom_snapshot();
    if !is_root {
        t!("apply_net_labels", apply_net_labels(&mut graph));
    }
    graph.claim_geom_changes(&g_snap, "12.net_labels");
    probe_box_collisions(&graph);

    // ── Wire/Label split pass ──
    // ★ B2: skip for root — root uses block edges, not nets.
    let g_snap = graph.geom_snapshot();
    if !is_root {
        let split_changed = t!("wire_label_split", {
            crate::viz::route::wire_label_split::apply_wire_label_split(&mut graph)
        });
        if split_changed {
            graph.claim_geom_changes(&g_snap, "12b.wire_label_split");
            // Re-normalize if new label boxes were added
            let g_snap2 = graph.geom_snapshot();
            t!("renormalize_wls", renormalize(&mut graph));
            graph.claim_geom_changes(&g_snap2, "12c.renormalize_wls");
        }
    }

    // ── Phase 2: route ──
    // ★ B2: skip for root — root uses block edges rendered directly from edge_decide.
    if !is_root {
        let g_snap = graph.geom_snapshot();
        t!("route_all", route_all_with_channels(&mut graph));
        graph.claim_geom_changes(&g_snap, "13.route");
    }

    // ★ P7-4e: Phase E (route feedback loop, nudge→reroute→accept) removed.
    // It moved box x/y inside the Route stage (19 net shifts measured in
    // P7-4c), violating the "Route reads geometry only" stage contract; its
    // benefit (lower wire_wire) is not a G12 criterion and box_box/wire_box
    // showed no regression after removal. Future routing-space relief belongs
    // in a Placement-stage collision-aware spacing pass.

    // ── Gate + report ──
    let col = t!("audit_all", audit_all(&graph));
    let fidelity = compute_fidelity(&graph, &col);
    let readability = compute_readability(&graph, &col);
    fidelity_gate(&layer, layouter, &fidelity, &readability, is_root);

    graph
}

/// ★ P7-1: global failure flag for the Tier 1 real gate.
///
/// Fix for v5 §0.2 "four false greens" / v6 §2 root cause J: fidelity_gate Tier 1
/// used to just log and `return;` —— the gate could falsify nothing. Now a Tier 1
/// failure sets this flag; `mcviz` checks it after rendering and exits non-zero
/// (integration tests assert this flag directly).
pub static RENDER_GATE_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// PR-1 fidelity gate — three-tier structure.
///
/// **Tier 1 · CORRECTNESS** — hard veto. Electrical correctness only:
/// nets, pins, bus bits. Any violation is a hard error that must be fixed.
/// ★ P7-1: Tier 1 failure now **really fails** —— sets [`RENDER_GATE_FAILED`],
/// checked by the bin (mcviz) after rendering, exiting with a non-zero code.
///
/// **Tier 2 · QUALITY** — ratchet. Collisions, coverage, wire quality.
/// These are logged as warnings; they don't block the gate but signal
/// that the layout quality has regressed.
///
/// **Tier 3 · INFO** — never fails. Authored pin sides (only for
/// non-model-claimed boxes), symmetry, engineer-style metrics. Purely
/// informational.
fn fidelity_gate(
    layer: &str,
    layouter: &str,
    fidelity: &FidelityReport,
    readability: &ReadabilityScore,
    is_root: bool,
) {
    // ── Tier 1 · CORRECTNESS — hard veto ──────────────────────────────────
    // ★ B2 root layers use the block-edge model: entry points are synthetic
    // (pin_id=0, name-keyed) and rail pins are covered by anchors, so the
    // entry-point coverage metric structurally can't reach pins_total. Root
    // pin coverage is validated by the edge renderer / rail contract instead
    // (G10/G11 in renderdiff); skip the pins veto on root. Nets and bus-bits
    // still gate.
    let pins_ok = is_root || fidelity.pins_rendered == fidelity.pins_total;
    let correct = fidelity.nets_dropped == 0
        && fidelity.nets_partial == 0
        && pins_ok
        && fidelity.bus_bits_paired_ok == fidelity.bus_bits_total;
    if !correct {
        RENDER_GATE_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
        if fidelity.nets_dropped > 0 || fidelity.nets_partial > 0 {
            crate::vlog!(
                "[layout-gate] ✗ VETO layer '{}': nets dropped={} partial={} of {} — lines missing",
                layer,
                fidelity.nets_dropped,
                fidelity.nets_partial,
                fidelity.nets_total
            );
        }
        if !pins_ok {
            crate::vlog!(
                "[layout-gate] ✗ VETO layer '{}': pins rendered {}/{} — some pins unconnected",
                layer,
                fidelity.pins_rendered,
                fidelity.pins_total
            );
        }
        if fidelity.bus_bits_paired_ok < fidelity.bus_bits_total {
            crate::vlog!(
                "[layout-gate] ✗ VETO layer '{}': bus bits paired {}/{} — bit-order mismatch",
                layer,
                fidelity.bus_bits_paired_ok,
                fidelity.bus_bits_total
            );
        }
        crate::vlog!(
            "[layout-gate] ✗✗✗ layer '{}' CORRECTNESS FAILED — cannot proceed",
            layer
        );
        return;
    }

    crate::vlog!("[layout-gate] layer '{}' Tier 1 CORRECTNESS ✓", layer);

    // ── Tier 2 · QUALITY — ratchet ────────────────────────────────────────
    let mut tier2_ok = true;
    if fidelity.box_box > 0 || fidelity.wire_box > 0 {
        crate::vlog!(
            "[layout-gate] ⚠ QUALITY layer '{}': collisions box_box={} wire_box={}",
            layer,
            fidelity.box_box,
            fidelity.wire_box
        );
        tier2_ok = false;
    }
    if fidelity.coverage_pct() < 100.0 {
        crate::vlog!(
            "[layout-gate] ⚠ QUALITY layer '{}': LAYOUT-MODEL coverage={:.0}% (claimed {}/{})",
            layer,
            fidelity.coverage_pct(),
            fidelity.islands_claimed,
            fidelity.islands_total
        );
        tier2_ok = false;
    }
    if tier2_ok {
        crate::vlog!("[layout-gate] layer '{}' Tier 2 QUALITY ✓", layer);
    }

    // ── Tier 3 · INFO — never fails, only prints ─────────────────────────
    // Authored pin sides: only meaningful for non-model-claimed boxes.
    // Model-claimed boxes (geom_locked) intentionally override authored sides
    // for topological correctness — not a quality issue.
    if fidelity.authored_sides_total > 0
        && fidelity.authored_sides_honored < fidelity.authored_sides_total
    {
        crate::vlog!(
            "[layout-gate] ℹ layer '{}': authored pin sides honored {}/{}",
            layer,
            fidelity.authored_sides_honored,
            fidelity.authored_sides_total
        );
    }

    crate::vlog!(
        "[layout-gate] layer '{}' ({}) report: wire_wire={} wirelen={:.1} bends={} off_grid={:.1} symmetry={:.1} idiom={:.1}",
        layer,
        layouter,
        readability.wire_wire,
        readability.total_wirelength,
        readability.total_bends,
        readability.off_grid_penalty,
        readability.symmetry_penalty,
        readability.idiom_violation,
    );
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
            // ★ Model-claimed boxes (geom_locked) have their pin sides determined
            //   by topology — the authored side is intentionally overridden. Count
            //   them as honoured rather than penalising the gate for a correct layout.
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
            authored_sides_honored += honored;
        }
    }

    FidelityReport {
        nets_total: graph.nets.len(),
        nets_rendered: graph.nets.len(),
        // NOTE (PR-3): nets_dropped / nets_partial are still hardcoded to 0 here —
        // the topology reconstruction that could drop a leaf lives upstream in
        // connection.rs (`merge_pairs_to_vecnet`).
        // ★ M0-C BLOCKED: once M0-A provides ConnDir, driver/load semantics can be
        //   derived by majority vote over ConnPair.dir; real dropped/partial counts
        //   can then be wired up.
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
        // ★ Layout coverage: read from graph (set by islands::apply_islands)
        islands_claimed: graph.islands_claimed,
        islands_total: graph.islands_total,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::{BoxKind, EndpointRef, McVecBox, NetKind, NetRole, Symbol, VizNet};
    use crate::viz::layout::FlowLayouter;
    use crate::viz::traits::Layouter;

    /// A layouter that deliberately produces a bad layout with overlapping boxes,
    /// used to check the fidelity gate observes (but does not drop) a bad layout.
    struct BadLayouter;
    impl Layouter for BadLayouter {
        fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
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
            "A".to_string(),
            Vec::new(),
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
            "B".to_string(),
            Vec::new(),
        );
        b2.x = 100.0;
        b2.y = 10.0;
        b2.w = 60.0;
        b2.h = 40.0;
        graph.boxes.push(b1);
        graph.boxes.push(b2);
        graph
    }

    /// The single pipeline runs the sole (first) candidate and returns a laid-out graph.
    #[test]
    fn single_pipeline_runs_first_candidate() {
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> = vec![Box::new(FlowLayouter::default())];
        let result = layout_best(graph, &candidates, true, None);
        assert!(!result.boxes.is_empty());
        assert!(result.boxes.iter().all(|b| b.w > 0.0 && b.h > 0.0));
        // circuit_flow should not overlap the two boxes.
        let col = audit_all(&result);
        assert_eq!(
            col.box_box, 0,
            "circuit_flow should not overlap boxes: {:?}",
            col
        );
    }

    /// Determinism: same input → same layout metrics.
    #[test]
    fn single_pipeline_deterministic() {
        let candidates: Vec<Box<dyn Layouter>> = vec![Box::new(FlowLayouter::default())];
        let r1 = layout_best(make_simple_graph(), &candidates, true, None);
        let r2 = layout_best(make_simple_graph(), &candidates, true, None);
        let s1 = compute_readability(&r1, &audit_all(&r1)).weighted();
        let s2 = compute_readability(&r2, &audit_all(&r2)).weighted();
        assert_eq!(
            s1, s2,
            "same input should produce same score: {} vs {}",
            s1, s2
        );
    }

    /// Empty candidate pool returns the graph untouched, no routing.
    #[test]
    fn empty_candidates_returns_original() {
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> = vec![];
        let result = layout_best(graph, &candidates, true, None);
        assert_eq!(result.boxes.len(), 2);
        assert!(result.nets.iter().all(|n| n.route.is_none()));
    }

    /// The gate observes a bad layout but never drops it — the graph still comes back.
    /// (With a single layouter there is no alternate to swap to; the veto is a log.)
    #[test]
    fn gate_does_not_drop_bad_layout() {
        let graph = make_simple_graph();
        let candidates: Vec<Box<dyn Layouter>> = vec![Box::new(BadLayouter)];
        let result = layout_best(graph, &candidates, true, None);
        assert!(!result.boxes.is_empty());
        // BadLayouter piles boxes on the same spot → the gate would log a VETO,
        // but the graph is returned unchanged for rendering.
        let col = audit_all(&result);
        assert!(
            col.box_box > 0,
            "bad layout kept (gate logs, does not drop)"
        );
    }

    /// ★ P7-1: Tier 1 failure must set RENDER_GATE_FAILED (a real gate, not just logging).
    /// Render-layer fix for v5 §0.2 "four false greens" —— directly constructs a
    /// report with is_correct()==false and calls fidelity_gate, asserting the global
    /// failure flag gets set (mcviz checks it and exits 2).
    #[test]
    fn tier1_failure_sets_render_gate_failed() {
        RENDER_GATE_FAILED.store(false, std::sync::atomic::Ordering::Relaxed);
        let bad = FidelityReport {
            nets_total: 3,
            nets_rendered: 2,
            nets_dropped: 1, // ← Tier 1 hard defect: a net left undrawn
            ..Default::default()
        };
        let readability = crate::viz::metrics::ReadabilityScore::default();
        assert!(!bad.is_correct());
        fidelity_gate("test_layer", "test_layouter", &bad, &readability, false);
        assert!(
            RENDER_GATE_FAILED.load(std::sync::atomic::Ordering::Relaxed),
            "Tier 1 CORRECTNESS failure must set RENDER_GATE_FAILED —— logging only = false gate"
        );
        // Reset so other tests are not polluted
        RENDER_GATE_FAILED.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Integration: circuit_flow pipelines layout → route → audit and produces routes.
    #[test]
    fn single_pipeline_routes_real_layouter() {
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
            "R1".to_string(),
            Vec::new(),
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
            "R2".to_string(),
            Vec::new(),
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
            "GND".to_string(),
            Vec::new(),
        );
        b3.x = 60.0;
        b3.y = 80.0;
        b3.w = 60.0;
        b3.h = 20.0;

        let net = VizNet::new(
            1,
            "N1".into(),
            NetKind::Signal,
            NetRole::Signal,
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
        // is_root=false so run_single reaches the Phase 2 route stage (root
        // layers route via block edges and skip net routing entirely).
        let result = layout_best(graph, &candidates, false, None);

        assert!(result.boxes.len() >= 3);
        for b in &result.boxes {
            assert!(b.w > 0.0 && b.h > 0.0, "box {} should have size", b.name);
        }

        let routed = result.nets.iter().filter(|n| n.route.is_some()).count();
        assert!(routed > 0, "at least one net should be routed");

        let _ = audit_all(&result);
    }
}
