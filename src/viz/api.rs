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
//! The alternate layouters (`SchematicRadialLayouter`, `HierarchicalLayouter`,
//! `RadialLayouter`, and the `FlowLayouter` parameter variants) are **kept in the
//! tree** and remain reachable for comparison via the explicit `RenderOpts`
//! constructors below (`all_radial`, `sugiyama`, `legacy_radial`, `schematic_radial`).
//! They are simply no longer in the default pool.
//!
//! ## ★ P03 (S1) changes
//! - Deleted `apply_route: bool` field, route now always executes (single pipeline)
//! - Deleted `RenderOpts::legacy_edges_only()` constructor (old binary edges rendering discontinued)
//! - Simplified `render_layer_recursive` signature, no longer passes apply_route parameter
//!
//! For "no routing, direct rendering" debug mode, now use `NoopRouter` wrapped in RenderOpts:
//! ```ignore
//! // Debug without routing
//! let opts = RenderOpts { ..Default::default() };
//! // (After P11, RenderOpts will expose router_choice field)
//! ```
//!
//! ## ★ P07 (S6) changes — Schematic Radial becomes default top-level
//! `top_layouter` default changed from `HierarchicalLayouter` to `SchematicRadialLayouter`,
//! visually from "layered strips" to "center IC + peripheral radiation" (more like real schematics).
//! (Superseded by PR-1 above: default is now circuit_flow.)
//!
//! Old behavior still available:
//! - `RenderOpts::sugiyama()` — top-level uses old HierarchicalLayouter
//! - `RenderOpts::legacy_radial()` — top-level uses old RadialLayouter (2 rings + mutual push)
//! - `RenderOpts::all_radial()` — all use old RadialLayouter (backward compatibility)
//!
//! ## ★ P10 (S6) changes — Channel-aware Routing
//! `smart_route_all` internally upgraded from `dispatch::route_all_with_dispatch` to
//! `scheduler::route_all_with_channels` (priority + ChannelMap to coordinate multiple trunks).
//! Visually multiple parallel trunks no longer stack on the same y.

use crate::vector::graph::{apply_promote_recursive, McVecGraph};

use super::debug;
use super::doc::VizDocument;
use super::labels::label_placement_pipeline;
use super::layer::VizLayer;
use super::layout::select::layout_best;
use super::layout::{
    FlowLayouter, HierarchicalLayouter, LayeredLayouter, RadialLayouter, SchematicRadialLayouter,
};
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
    /// All use the old Radial (compatible with old debugging / old tests)
    pub fn all_radial() -> Self {
        Self {
            top_layouter: Box::new(RadialLayouter),
            sub_layouter: Box::new(RadialLayouter),
            renderer: Box::new(DefaultRenderer),
            apply_promote: true,
            top_candidates: vec![Box::new(RadialLayouter)],
            sub_candidates: vec![Box::new(RadialLayouter)],
        }
    }

    /// ★ P07 — top-level uses Sugiyama layering (default before S5)
    pub fn sugiyama() -> Self {
        Self {
            top_layouter: Box::new(HierarchicalLayouter::default()),
            sub_layouter: Box::new(RadialLayouter),
            renderer: Box::new(DefaultRenderer),
            apply_promote: true,
            top_candidates: vec![Box::new(HierarchicalLayouter::default())],
            sub_candidates: vec![Box::new(RadialLayouter)],
        }
    }

    /// ★ P07 — top-level uses old RadialLayouter (2 rings, mutual push)
    pub fn legacy_radial() -> Self {
        Self {
            top_layouter: Box::new(RadialLayouter),
            sub_layouter: Box::new(RadialLayouter),
            renderer: Box::new(DefaultRenderer),
            apply_promote: true,
            top_candidates: vec![Box::new(RadialLayouter)],
            sub_candidates: vec![Box::new(RadialLayouter)],
        }
    }

    /// ★ Stage A — center IC + radiation (kept for comparison against circuit_flow)
    pub fn schematic_radial() -> Self {
        Self {
            top_layouter: Box::new(SchematicRadialLayouter::default()),
            sub_layouter: Box::new(RadialLayouter),
            renderer: Box::new(DefaultRenderer),
            apply_promote: true,
            top_candidates: vec![Box::new(SchematicRadialLayouter::default())],
            sub_candidates: vec![Box::new(RadialLayouter)],
        }
    }

    /// ★ M6 — Semantic-driven layered placement prototype (experimental).
    pub fn layered() -> Self {
        Self {
            top_layouter: Box::new(LayeredLayouter::default()),
            sub_layouter: Box::new(LayeredLayouter::sub()),
            renderer: Box::new(DefaultRenderer),
            apply_promote: true,
            top_candidates: vec![Box::new(LayeredLayouter::default())],
            sub_candidates: vec![Box::new(LayeredLayouter::sub())],
        }
    }
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
        graph = layout_best(graph, candidates, is_root);

        // ── Phase 1.46b: Adjust Virtual Top Module Border position/size ──
        // After layout positions all boxes, adjust the dashed border boxes to surround internal components.
        crate::vector::graph::from_block::layout_post_adjust_borders(&mut graph);

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

    crate::vector::graph::net_probe::probe_route(&graph); // ★ NEW

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

    // ── Semantic analysis (read-only, soft signal) ──
    let semantic = SemanticModel::analyze(&graph);
    metrics.accumulate_semantic(&semantic.summary);

    // ── M10: Special power/ground/bus analysis (read-only) ──
    let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
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
