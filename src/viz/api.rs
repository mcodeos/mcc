// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Top-level rendering API
//!
//! ## ★ PR-1 — single-layouter pipeline
//! The default candidate pool is collapsed to a single layouter, **circuit_flow**
//! (`FlowLayouter`), at both top and sub level. generate-and-rank is retired in
//! `layout::select::layout_best`, which now runs one layouter and applies a
//! fidelity gate instead of ranking N candidates. "What you edit is what you see."
//!
//! ## ★ P03 (S1) changes
//! - Deleted `apply_route: bool` field, route now always executes (single pipeline)
//! - Deleted `RenderOpts::legacy_edges_only()` constructor (old binary edges rendering discontinued)
//! - Simplified `render_layer_recursive` signature, no longer passes apply_route parameter
//!
//! ## ★ P10 (S6) changes — Channel-aware Routing
//! `smart_route_all` internally upgraded from `dispatch::route_all_with_dispatch` to
//! `scheduler::route_all_with_channels` (priority + ChannelMap to coordinate multiple trunks).
//! Visually multiple parallel trunks no longer stack on the same y.

use std::collections::HashSet;

use crate::vector::graph::{apply_promote_recursive, McVecGraph};

use super::debug;
use super::doc::VizDocument;
use super::labels::label_placement_pipeline;
use super::layer::VizLayer;
use super::layout::select::layout_best;
use super::layout::FlowLayouter;
use super::semantic::SemanticModel;
use super::special::PowerGroundBusModel;
use super::traits::{DefaultRenderer, Layouter, Renderer};

// ============================================================================
// Rendering options
// ============================================================================

pub struct RenderOpts {
    pub top_layouter: Box<dyn Layouter>,
    pub sub_layouter: Box<dyn Layouter>,
    pub renderer: Box<dyn Renderer>,
    /// Whether to promote at top level (P1)
    pub apply_promote: bool,
    /// Top-level candidate layouters for the layout pipeline.
    /// PR-1: single candidate (circuit_flow).
    pub top_candidates: Vec<Box<dyn Layouter>>,
    /// Sub-level candidate layouters for the layout pipeline.
    /// PR-1: single candidate (circuit_flow / FlowLayouter::sub()).
    pub sub_candidates: Vec<Box<dyn Layouter>>,
}

impl Default for RenderOpts {
    fn default() -> Self {
        let top = FlowLayouter::default();
        let sub = FlowLayouter::sub();
        Self {
            top_layouter: Box::new(top),
            sub_layouter: Box::new(sub),
            renderer: Box::new(DefaultRenderer),
            apply_promote: true,
            // ★ PR-1: single-layouter pipeline. circuit_flow (FlowLayouter) is the
            //   only candidate at both levels. generate-and-rank is retired — see
            //   layout::select::layout_best. The alternate layouters are kept in the
            //   tree and reachable via the explicit constructors below.
            top_candidates: vec![Box::new(FlowLayouter::default())],
            sub_candidates: vec![Box::new(FlowLayouter::sub())],
        }
    }
}

impl RenderOpts {
    // Only FlowLayouter is retained after M1-1 dead code removal.
    // All alternative layouters (Radial, Hierarchical, SchematicRadial, Layered)
    // have been removed along with their implementations.
}

// ============================================================================
// Top-level API
// ============================================================================

pub fn render(graph: McVecGraph) -> VizDocument {
    render_with(graph, RenderOpts::default())
}

pub fn render_with(graph: McVecGraph, opts: RenderOpts) -> VizDocument {
    render_with_metrics(graph, opts).0
}

/// Render and return metrics accumulator (build report not yet merged; dropped/partial
/// merged by caller at finish time).
pub fn render_with_metrics(
    mut graph: McVecGraph,
    opts: RenderOpts,
) -> (VizDocument, crate::viz::metrics::MetricsAccumulator) {
    // Reset R15 counter for this render
    crate::viz::SYNTHETIC_PIN_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);

    let root_bid = graph.bid;
    let root_name = graph.name.clone();

    // ── Phase 0: promote (P1) ──
    if opts.apply_promote {
        if super::debug::dump_enabled() {
            crate::vlog!("[viz::api] applying promote_recursive (top-level simplest integration)");
        }
        apply_promote_recursive(&mut graph);
    }

    let mut doc = VizDocument::new(root_bid, root_name);
    let mut metrics = crate::viz::metrics::MetricsAccumulator::default();

    render_layer_recursive(
        &mut doc,
        graph,
        None,
        true,
        &opts.top_candidates,
        &opts.sub_candidates,
        &*opts.renderer,
        &mut metrics,
    );

    crate::vlog!(
        "[viz::api] render done: {} layers, {} bytes total SVG",
        doc.layer_count(),
        doc.total_svg_bytes()
    );

    debug::dump_document(&doc);
    (doc, metrics)
}

fn render_layer_recursive(
    doc: &mut VizDocument,
    mut graph: McVecGraph,
    parent: Option<i64>,
    is_root: bool,
    top_candidates: &[Box<dyn Layouter>],
    sub_candidates: &[Box<dyn Layouter>],
    renderer: &dyn Renderer,
    metrics: &mut crate::viz::metrics::MetricsAccumulator,
) {
    let bid = graph.bid;
    let name = graph.name.clone();

    let sub_graphs = std::mem::take(&mut graph.sub_graphs);
    let clickable_subs: Vec<i64> = sub_graphs.iter().map(|sg| sg.bid).collect();

    let candidates = if is_root {
        top_candidates
    } else {
        sub_candidates
    };

    // ── Phase 1–2: layout + route via the single-layouter pipeline ──
    let mut canvas = if graph.boxes.is_empty() {
        crate::vlog!(
            "[viz::api] layer {} '{}' is empty, skipping layout",
            bid,
            name
        );
        (200.0, 100.0)
    } else {
        let layouter_name = candidates.first().map(|c| c.name()).unwrap_or("none");

        // ── Phase D: build SchematicLayoutModel before layout for low-risk intent ──
        // Semantic and special analysis are read-only and don't need positions.
        let schematic_model = {
            let semantic = SemanticModel::analyze(&graph);
            let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
            let idioms = crate::viz::idiom::detect_placement_instances(&graph, &HashSet::new());
            let model = crate::viz::layout_model::SchematicLayoutModel::build(
                &graph, &semantic, &special, &idioms,
            );
            for line in model.report_lines() {
                crate::vlog!("{}", line);
            }
            model
        };

        graph = layout_best(graph, candidates, is_root, Some(schematic_model));

        // ── Phase 1.46b: Adjust Virtual Top Module Border position/size ──
        // After layout positions all boxes, adjust the dashed border boxes to surround internal components.
        crate::vector::graph::fromblock::layout_post_adjust_borders(&mut graph);

        // Compute canvas from laid-out boxes
        let cv = super::layout::normalize::compute_canvas(&graph);
        crate::vlog!(
            "[viz::api] layer {} '{}' layout done: canvas={}x{} (algo={})",
            bid,
            name,
            cv.0 as i32,
            cv.1 as i32,
            layouter_name
        );
        debug::dump_layout(&graph, layouter_name, cv);
        cv
    };

    // Phase 1.8: net labels (may update canvas; layout_best already ran it, but
    // we need the final canvas value for rendering)
    if let Some(cv) = super::layout::passive_inline::apply_net_labels(&mut graph) {
        canvas = cv;
    }

    crate::vector::graph::netprobe::probe_route(&graph); // ★ NEW

    let rep = super::route::audit::audit_all(&graph);
    crate::vlog!(
        "[viz::audit] box-box={} wire-box={} wire-wire={} (total={})",
        rep.box_box,
        rep.wire_box,
        rep.wire_wire,
        rep.total()
    );

    // ── M8: Label placement optimization (after route, before metrics) ──
    let label_report = label_placement_pipeline(&mut graph, canvas);
    crate::vlog!(
        "[viz::labels] placed={} total={} hidden={}",
        label_report.labels_placed,
        label_report.labels_total,
        label_report.labels_hidden,
    );

    metrics.accumulate_layer(&graph, &rep, canvas);

    // ── M12: Determinism report (after route, before render) ──
    // ★ P0.5-3c: 补上 idiom_hash / placement_hash —— 在 layout_best 之后
    // 重新检测 idiom 实例和约束（只读操作），填充到 determinism 报告中。
    let det_report = {
        let mut r = crate::viz::stability::report::DeterminismReport::from_graph(&graph);
        r.graph_input_hash = crate::viz::stability::hash::hash_box_geometry(&graph);
        r.route_schedule_hash = crate::viz::stability::hash::canonical_hash(&graph.nets.len());
        // 收集 protected boxes（geom_locked 的）
        let protected: HashSet<i64> = graph
            .boxes
            .iter()
            .filter(|b| b.geom_locked)
            .map(|b| b.id)
            .collect();
        let idiom_instances = crate::viz::idiom::detect_placement_instances(&graph, &protected);
        let constraints = crate::viz::idiom::generate_constraints(&idiom_instances);
        let prefix = &r.box_order_hash;
        r.idiom_instance_hash =
            crate::viz::stability::hash::hash_idiom_instances(&idiom_instances, prefix);
        r.placement_constraint_hash =
            crate::viz::stability::hash::hash_placement_constraints(&constraints, prefix);
        // placement_decision_hash 用 constraints hash 作为近似（实际 decisions
        // 在 flow.rs 内计算，此处不可达；但顶层报告需要非空值）
        r.placement_decision_hash = r.placement_constraint_hash.clone();
        r
    };
    metrics.accumulate_determinism(&det_report);

    // ── Semantic analysis (read-only, soft signal) ──
    let semantic = SemanticModel::analyze(&graph);
    metrics.accumulate_semantic(&semantic.summary);

    // ── M10: Special power/ground/bus analysis (read-only) ──
    let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
    special.vlog_long_stubs(&name);
    metrics.accumulate_special(&special.report);

    debug::dump_route(&graph);

    super::route::wire_hops::apply_wire_hops(&mut graph);

    // ── Phase 3: render ──
    let svg = renderer.render(&graph, canvas);
    crate::vlog!(
        "[viz::api] layer {} '{}' render done: {} bytes (algo={})",
        bid,
        name,
        svg.len(),
        renderer.name()
    );

    // ── M13: Rendered connectivity extraction (after render, per-layer) ──
    {
        let conn = crate::viz::connectivity::model::RenderedConnectivity::extract(&graph);
        let mut conn_report =
            crate::viz::connectivity::report::RenderedConnectivityReport::from_connectivity(&conn);
        conn_report.connectivity_hash =
            crate::viz::stability::hash::canonical_hash(&conn_report.pins_reachable);
        metrics.accumulate_connectivity(&conn_report);
    }

    let mut layer = VizLayer::new(bid, name, parent);
    layer.canvas = canvas;
    layer.svg = svg;
    layer.clickable_subs = clickable_subs;
    doc.add_layer(layer);

    for sub in sub_graphs {
        render_layer_recursive(
            doc,
            sub,
            Some(bid),
            false,
            top_candidates,
            sub_candidates,
            renderer,
            metrics,
        );
    }
}

// ============================================================================
// One-stop: graph → HTML
// ============================================================================

pub fn render_to_html(graph: McVecGraph) -> String {
    let doc = render(graph);
    super::template::wrap_document(&doc)
}
