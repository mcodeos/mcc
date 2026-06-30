// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Top-level rendering API
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
use super::layer::VizLayer;
use super::layout::select::layout_best;
use super::layout::{FlowLayouter, HierarchicalLayouter, RadialLayouter, SchematicRadialLayouter};
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
    /// Phase 3: top-level candidate layouters for generate-and-rank.
    /// Default = single candidate (top_layouter), backward compatible.
    pub top_candidates: Vec<Box<dyn Layouter>>,
    /// Phase 3: sub-level candidate layouters for generate-and-rank.
    /// Default = single candidate (sub_layouter), backward compatible.
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
            // Phase 3: multi-candidate generate-and-rank
            top_candidates: vec![
                Box::new(FlowLayouter::default()),
                Box::new(FlowLayouter {
                    hub_keep_semantic: true,
                    ..FlowLayouter::default()
                }),
                Box::new(FlowLayouter {
                    bary_sweeps: 10,
                    ..FlowLayouter::default()
                }),
                Box::new(FlowLayouter {
                    hub_min_degree: 3,
                    ..FlowLayouter::default()
                }),
                Box::new(FlowLayouter {
                    fanout_star: true,
                    ..FlowLayouter::default()
                }),
                Box::new(SchematicRadialLayouter::default()),
                Box::new(HierarchicalLayouter::default()),
            ],
            sub_candidates: vec![
                Box::new(FlowLayouter::sub()),
                Box::new(FlowLayouter {
                    hub_keep_semantic: true,
                    ..FlowLayouter::sub()
                }),
                Box::new(FlowLayouter {
                    bary_sweeps: 10,
                    ..FlowLayouter::sub()
                }),
                Box::new(FlowLayouter {
                    hub_min_degree: 3,
                    ..FlowLayouter::sub()
                }),
                Box::new(FlowLayouter {
                    fanout_star: true,
                    ..FlowLayouter::sub()
                }),
            ],
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

    /// ★ Stage A — previous default: center IC + radiation (for comparison)
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

    // ── Phase 1–2: layout + route via generate-and-rank ──
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

    metrics.accumulate_layer(&graph, &rep);

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
