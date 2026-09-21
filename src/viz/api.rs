// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Top-level rendering API
//!
//! ## ★ PR-1 — single-layouter pipeline
//! One layouter, **circuit_flow** (`FlowLayouter`), runs at both top and sub level.
//! generate-and-rank is retired; `layout::select::layout_best` runs the single
//! layouter and applies a fidelity gate instead of ranking N candidates.
//!
//! ## ★ P03 (S1) changes
//! - Deleted `apply_route: bool` field, route now always executes (single pipeline)
//! - Deleted `RenderOpts::legacy_edges_only()` constructor (old binary edges rendering
//! discontinued)
//! - Simplified `render_layer_recursive` signature, no longer passes apply_route parameter
//!
//! ## ★ P10 (S6) — Channel-aware Routing
//! Routing runs through `scheduler::route_all_with_channels` (priority + ChannelMap to
//! coordinate multiple trunks), so parallel trunks do not stack on the same y.

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

// Rendering options

pub struct RenderOpts {
    pub top_layouter: Box<dyn Layouter>,
    pub sub_layouter: Box<dyn Layouter>,
    pub renderer: Box<dyn Renderer>,
    /// Whether to promote at top level (P1)
    pub apply_promote: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            top_layouter: Box::new(FlowLayouter::default()),
            sub_layouter: Box::new(FlowLayouter::sub()),
            renderer: Box::new(DefaultRenderer),
            apply_promote: true,
        }
    }
}

// Top-level API

pub fn render(graph: McVecGraph) -> VizDocument {
    render_with(graph, RenderOpts::default())
}

pub fn render_with(graph: McVecGraph, opts: RenderOpts) -> VizDocument {
    render_with_metrics(graph, opts).0
}

/// One layer exactly as the renderer received it.
///
/// [`render_layer_recursive`] consumes the graph it has just laid out — the
/// layer's SVG is all that survives the call — so a caller that wants to read
/// the laid-out *structure* has nothing left to read. This is that structure,
/// handed back at the point the renderer is given it: the same value, after
/// layout, routing, label placement and wire hops.
///
/// A stage view reads this rather than the SVG on purpose (design §11.1 / M5:
/// compare structure, not drawing — a segment's coordinates are a path between
/// two endpoints, so a change of drawing style would read as a change of
/// circuit).
#[derive(Debug, Clone)]
pub struct RenderedLayer {
    /// The post-layout graph. `graph.layer_style` says whether this layer was
    /// drawn as a block diagram or as a device schematic.
    pub graph: McVecGraph,
    /// The enclosing layer's `bid`; `None` for the root.
    pub parent: Option<i64>,
    /// The layer's canvas `(width, height)` — the space its coordinates are in.
    /// A position without it cannot be read.
    pub canvas: (f64, f64),
    /// Whether the pipeline *audited* this layer, i.e. whether it was one of the
    /// layers [`crate::viz::metrics::MetricsAccumulator::accumulate_layer`] ran
    /// on. A device layer is not audited (F2: it skips route and audit and wires
    /// itself through the equipotential trees instead), so a consumer reading the
    /// accumulated `fidelity` / `truth` / `visual` numbers has no way to know how
    /// much of the drawing they cover unless the layer says so itself.
    pub audited: bool,
}

/// Render and return metrics accumulator (build report not yet merged; dropped/partial
/// merged by caller at finish time).
pub fn render_with_metrics(
    graph: McVecGraph,
    opts: RenderOpts,
) -> (VizDocument, crate::viz::metrics::MetricsAccumulator) {
    render_with_metrics_and_sink(graph, opts, None)
}

/// [`render_with_metrics`] with an optional observation sink.
///
/// The sink receives every layer's post-layout graph in pre-order (a layer
/// before its sub-layers). Passing `None` is the normal path and costs nothing:
/// the graph is simply dropped where it always was, so no existing call site
/// changes.
pub fn render_with_metrics_and_sink(
    mut graph: McVecGraph,
    opts: RenderOpts,
    sink: Option<&mut Vec<RenderedLayer>>,
) -> (VizDocument, crate::viz::metrics::MetricsAccumulator) {
    crate::vlog!(
        "[DEBUG api] render_with_metrics: graph.is_root={} name={} boxes={} nets={}",
        graph.is_root,
        graph.name,
        graph.boxes.len(),
        graph.nets.len()
    );
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
        &*opts.top_layouter,
        &*opts.sub_layouter,
        &*opts.renderer,
        &mut metrics,
        sink,
    );

    crate::vlog!(
        "[viz::api] render done: {} layers, {} bytes total SVG",
        doc.layer_count(),
        doc.total_svg_bytes()
    );

    // ── ★ P7-1: renderdiff (baseline generation, or readings vs baseline) ──
    // Large-scale red mid-way is the expected shape; reported here without
    // blocking —— the Tier 1 electrical gate (RENDER_GATE_FAILED) is the hard failure.
    // The report is vlog-only, so skip the full diff when MC_VIZ_DUMP is off —— but
    // not the write: generating a baseline is `MC_RENDER_GOLDEN_SAVE` alone, because
    // a switch that only exists behind another switch is one nobody finds.
    if super::debug::dump_enabled()
        || crate::viz::metrics::renderdiff::RenderGolden::save_requested()
    {
        let _ = renderdiff_report(&metrics);
    }

    debug::dump_document(&doc);
    (doc, metrics)
}

/// ★ P7-1: compare renderdiff readings against golden, reporting layer by layer.
///
/// golden path: `MC_RENDER_GOLDEN` env var > `./build/baseline/render_golden.toml`.
/// When golden is not found, prints a SKIP (a visible skip, not a false green).
///
/// With `MC_RENDER_GOLDEN_SAVE` set the same path is **written** instead of read
/// (see [`crate::viz::metrics::renderdiff::RenderGolden::save`]), and the report
/// is skipped in that run — generating a baseline and comparing against it in one
/// run would make every criterion green by construction.
pub fn renderdiff_report(
    metrics: &crate::viz::metrics::MetricsAccumulator,
) -> Option<Vec<crate::viz::metrics::renderdiff::LayerDiff>> {
    let path = std::env::var("MC_RENDER_GOLDEN").unwrap_or_else(|_| {
        std::path::PathBuf::from("build/baseline/render_golden.toml")
            .to_string_lossy()
            .into_owned()
    });
    use crate::viz::metrics::renderdiff::RenderGolden;
    if RenderGolden::save_requested() {
        let golden = RenderGolden::from_readings(&metrics.renderdiff_layers);
        // Printed rather than vlogged: this line reports a file the run changed,
        // so it has to be visible without MC_VIZ_DUMP (the same reason regress.sh
        // echoes "UPDATED:").
        match golden.save(std::path::Path::new(&path)) {
            Ok(()) => eprintln!(
                "[renderdiff] WROTE baseline {path} ({} layers) — this run judged nothing",
                golden.layer.len()
            ),
            // Loud on purpose: a silent failure here would leave the caller
            // believing a baseline now exists when it does not.
            Err(e) => eprintln!("[renderdiff] FAILED to write {path} ({e})"),
        }
        for v in RenderGolden::invariant_violations(&metrics.renderdiff_layers) {
            eprintln!("[renderdiff]   invariant: {v}");
        }
        return None;
    }
    let golden =
        match crate::viz::metrics::renderdiff::RenderGolden::load(std::path::Path::new(&path)) {
            Ok(g) => g,
            Err(e) => {
                crate::vlog!("[renderdiff] · SKIP golden not loaded ({path}: {e})");
                return None;
            }
        };

    let mut diffs = Vec::new();
    let (mut red, mut green, mut skip) = (0usize, 0usize, 0usize);
    for r in &metrics.renderdiff_layers {
        let d = golden.diff_layer(r);
        crate::vlog!("{}", d.report_line());
        red += d.red;
        green += d.green;
        skip += d.skipped;
        diffs.push(d);
    }
    crate::vlog!(
        "[renderdiff] TOTAL: {} red / {} green / {} skip (large-scale red is the correct shape at the P7-1 stage)",
        red,
        green,
        skip
    );
    Some(diffs)
}

fn render_layer_recursive(
    doc: &mut VizDocument,
    mut graph: McVecGraph,
    parent: Option<i64>,
    is_root: bool,
    top_layouter: &dyn Layouter,
    sub_layouter: &dyn Layouter,
    renderer: &dyn Renderer,
    metrics: &mut crate::viz::metrics::MetricsAccumulator,
    mut sink: Option<&mut Vec<RenderedLayer>>,
) {
    let bid = graph.bid;
    let name = graph.name.clone();

    let sub_graphs = std::mem::take(&mut graph.sub_graphs);
    let clickable_subs: Vec<i64> = sub_graphs.iter().map(|sg| sg.bid).collect();
    // ★ U87: the same list the document gets as `layer.clickable_subs` (and whose
    // keys are the `layers` of the exported `DOC`) travels on the graph too, so the
    // renderer can tell a box that really has a layer from one that only looks like
    // a block. One list, two readers — they cannot drift.
    graph.clickable_subs = clickable_subs.clone();

    // ★ A root layer is a *block diagram* only when it actually contains sub-module
    // boxes. Those sub-modules are the blocks; the block-diagram rules — C5 top-level
    // passive drop (rails::drop_top_passives), R-B ground hide, radial supply
    // fan-out — exist to keep a module's internals from drowning that block graph.
    //
    // A root layer with **no** sub-module box is the *schematic of a single module*:
    // every box is a real component and there is nothing to fold internals into.
    //
    // That module is not a special case — inside a project it is exactly one of the
    // sub-layers, and the recursion below paints every sub-layer with the device
    // (equipotential tree) pipeline. A file-scope preview of the same module must be
    // the *same drawing*, so it takes that same pipeline here. One strategy: the
    // picture of a module never depends on whether it was opened on its own or
    // expanded inside its project.
    //
    // Structural — no module or file names are consulted.
    let has_sub_boxes = graph
        .boxes
        .iter()
        .any(|b| b.kind == crate::vector::graph::BoxKind::SubModule);
    let is_block_diagram = is_root && has_sub_boxes;

    if is_root && !has_sub_boxes {
        graph.layer_style = crate::vector::graph::LayerStyle::Device;
    }

    let layouter = if is_block_diagram {
        top_layouter
    } else {
        sub_layouter
    };
    // flow / radial / facade read the graph field (not the parameter) to decide
    // block-diagram vs schematic behaviour; keep the two in step.
    graph.is_root = is_block_diagram;

    // ── Phase 1–2: layout + route via the single-layouter pipeline ──
    // `canvas` is the SVG viewBox SIZE `(w, h)` (consumed by label placement,
    // metrics and `layer.canvas`); `viewbox_origin` is the viewBox top-left
    // `(x, y)` — M10's content fit starts it at the true content top, which is
    // negative when a vertical label reads upward off a high row.
    let (canvas, viewbox_origin) = if graph.boxes.is_empty() {
        crate::vlog!(
            "[viz::api] layer {} '{}' is empty, skipping layout",
            bid,
            name
        );
        ((200.0, 100.0), (0.0, 0.0))
    } else if graph.layer_style == crate::vector::graph::LayerStyle::Device {
        // ── ★ F2: Device pipeline — equipotential tree layout only ──
        crate::viz::layout::equipotential_tree::layout_device_layer(&mut graph);
        // ★ Content-adaptive canvas: fit every box + tree segment + symbol
        // (including negative-x West trunks and upward-reading vertical labels)
        // into the SVG viewBox, starting at the TRUE content top.
        let cv = crate::viz::layout::equipotential_tree::fit_content_to_canvas(&mut graph);
        // ★ Module-port drawing: a module's own layer gets its boundary drawn —
        // a dashed frame with the module's ports on it. Drawn from `BoundaryInfo`,
        // named by the port. No-op for layers that are not a module's schematic.
        let (vx, vy, vw, vh) = crate::viz::layout::module_frame::layout_module_frame(
            &mut graph,
            (cv.0, cv.1, cv.2, cv.3),
        );
        crate::vlog!(
            "[viz::api] layer {} '{}' device canvas={}x{} origin=({},{})",
            bid,
            name,
            vw as i32,
            vh as i32,
            vx as i32,
            vy as i32
        );
        ((vw, vh), (vx, vy))
    } else {
        let layouter_name = layouter.name();

        // ── Phase D: build SchematicLayoutModel before layout for low-risk intent ──
        // Semantic and special analysis are read-only and don't need positions.
        let _td = std::time::Instant::now();
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
        tracing::info!(target: "mcc::perf", step = "schematic_model", ms = _td.elapsed().as_millis() as u64, boxes = graph.boxes.len(), nets = graph.nets.len(), "render step");

        let _tl = std::time::Instant::now();
        graph = layout_best(graph, layouter, is_block_diagram, Some(schematic_model));
        tracing::info!(target: "mcc::perf", step = "layout_best", ms = _tl.elapsed().as_millis() as u64, "render step");

        // ── Phase 1.46b: Adjust Virtual Top Module Border position/size ──
        // After layout positions all boxes, adjust the dashed border boxes to surround internal
        // components.
        let g_snap = graph.geom_snapshot();
        crate::vector::graph::fromblock::layout_post_adjust_borders(&mut graph);
        graph.claim_geom_changes(&g_snap, "15.borders");

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

        // ── ★ inventory dump: box provenance + isolated + single-endpoint nets ──
        {
            let mut component = 0usize;
            let mut sub_module = 0usize;
            let mut power_label = 0usize;
            let mut _dot = 0usize;
            let mut phase_145 = 0usize;
            let mut port_terminal = 0usize;
            for b in &graph.boxes {
                match b.kind {
                    crate::vector::graph::BoxKind::TwoPin
                    | crate::vector::graph::BoxKind::MultiPin => component += 1,
                    crate::vector::graph::BoxKind::SubModule => {
                        sub_module += 1;
                        if b.id == bid {
                            phase_145 += 1;
                        }
                    }
                    crate::vector::graph::BoxKind::PowerLabel => power_label += 1,
                    crate::vector::graph::BoxKind::Dot => _dot += 1,
                    crate::vector::graph::BoxKind::PortTerminal => port_terminal += 1,
                }
            }
            let declared = graph.boxes.len() - power_label - phase_145 - port_terminal;
            let synth_endpoint = power_label; // PowerLabel boxes = Phase E.1 boundary labels

            // degree: number of unique net connections per box
            let mut box_conn: std::collections::HashMap<i64, usize> =
                std::collections::HashMap::new();
            for net in &graph.nets {
                for bid in net.box_ids() {
                    *box_conn.entry(bid).or_default() += 1;
                }
            }
            let hub_id = box_conn
                .iter()
                .max_by_key(|(_, &c)| c)
                .map(|(&id, _)| id)
                .unwrap_or(graph.boxes.first().map(|b| b.id).unwrap_or(0));

            let isolated = crate::viz::layout::flow::compute_isolated_ids(&graph, hub_id);
            let single_endpoint_nets = graph.nets.iter().filter(|n| n.box_ids().len() <= 1).count();

            crate::vlog!(
                "[inventory] layer '{}' boxes={} (Component={}, SubModule={}, PowerLabel={}) \
                 provenance: Declared={}, SynthesizedFromEndpoint={}, Phase1_45={} \
                 isolated={:?} nets_with_single_endpoint={}",
                name,
                graph.boxes.len(),
                component,
                sub_module,
                power_label,
                declared,
                synth_endpoint,
                phase_145,
                isolated,
                single_endpoint_nets,
            );

            // ── isolated box details: id:name:kind:degree ──
            if !isolated.is_empty() {
                let box_lookup: std::collections::HashMap<
                    i64,
                    (&crate::vector::graph::McVecBox, usize),
                > = graph
                    .boxes
                    .iter()
                    .map(|b| (b.id, (b, box_conn.get(&b.id).copied().unwrap_or(0))))
                    .collect();
                let mut iso_ids: Vec<i64> = isolated.iter().copied().collect();
                iso_ids.sort_unstable();
                for &id in &iso_ids {
                    if let Some(&(b, deg)) = box_lookup.get(&id) {
                        crate::vlog!(
                            "[inventory]   isolated: id={} name='{}' kind={} degree={} pins={}",
                            b.id,
                            b.name,
                            b.kind,
                            deg,
                            b.pin_count,
                        );
                    }
                }
            }

            // ── hub candidates: top-3 boxes by degree ──
            let mut degs: Vec<(i64, usize)> = box_conn.iter().map(|(&id, &d)| (id, d)).collect();
            degs.sort_by_key(|&(_, d)| std::cmp::Reverse(d));
            let box_lookup2: std::collections::HashMap<i64, &crate::vector::graph::McVecBox> =
                graph.boxes.iter().map(|b| (b.id, b)).collect();
            for (id, deg) in degs.iter().take(3) {
                if let Some(b) = box_lookup2.get(id) {
                    crate::vlog!(
                        "[inventory]   hub-candidate: id={} name='{}' kind={} degree={} pins={}",
                        b.id,
                        b.name,
                        b.kind,
                        deg,
                        b.pin_count,
                    );
                }
            }

            // ── box 1010 degree (if present) ──
            if let Some(b) = box_lookup2.get(&1010) {
                let deg = box_conn.get(&1010).copied().unwrap_or(0);
                crate::vlog!(
                    "[inventory]   box-1010: id=1010 name='{}' kind={} degree={} pins={}",
                    b.name,
                    b.kind,
                    deg,
                    b.pin_count,
                );
            }
        }

        // Non-device layouts live at positive coordinates already, so their
        // viewBox keeps the `0 0` origin.
        (cv, (0.0, 0.0))
    };

    // ★ P7-4f: apply_net_labels is called only once in select.rs (before route).
    // The former second call here measured zero geometry writes across all 7 example
    // layers (label idempotence guard: nets already carrying a label are skipped);
    // its only role was a canvas fallback, which is already computed above.
    // ★ F2: Device layer skips route/audit (the equipotential-tree pipeline
    // draws its own per-net geometry). Label placement still runs on Device
    // layers: the S8 NC-prefix contract and the S7 overlap guard apply to
    // sub-layers too — before F2, sub-layers ran the FlowLayouter and received
    // placed labels, so dropping the pipeline here silently un-marked NC parts.
    let mut audit = None;
    if graph.layer_style != crate::vector::graph::LayerStyle::Device {
        let rep = super::route::audit::audit_all(&graph);
        crate::vlog!(
            "[viz::audit] box-box={} wire-box={} wire-wire={} (total={})",
            rep.box_box,
            rep.wire_box,
            rep.wire_wire,
            rep.total()
        );
        for d in &rep.details {
            crate::vlog!("[viz::audit] detail: {d}");
        }
        audit = Some(rep);
    }

    // ── M8: Label placement optimization (after route, before metrics) ──
    let label_report = label_placement_pipeline(&mut graph, canvas);
    crate::vlog!(
        "[viz::labels] placed={} total={} hidden={}",
        label_report.labels_placed,
        label_report.labels_total,
        label_report.labels_hidden,
    );

    // Recorded before the report is consumed: the sink hands the layer back with
    // this flag, so a reader of the accumulated numbers can see their scope.
    let audited = audit.is_some();
    if let Some(rep) = audit {
        metrics.accumulate_layer(&graph, &rep, canvas);
    }

    // ── M12: Determinism report (after route, before render) ──
    // ★ P0.5-3c: fill in idiom_hash / placement_hash —— after layout_best,
    // re-detect idiom instances and constraints (read-only operations), and
    // populate them into the determinism report.
    let _tdr = std::time::Instant::now();
    let det_report = {
        let mut r = crate::viz::stability::report::DeterminismReport::from_graph(&graph);
        r.graph_input_hash = crate::viz::stability::hash::hash_box_geometry(&graph);
        r.route_schedule_hash = crate::viz::stability::hash::canonical_hash(&graph.nets.len());
        // Collect protected boxes (those with geom_locked)
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
        // placement_decision_hash approximates with the constraints hash (the real
        // decisions are computed inside flow.rs, unreachable here; but the top-level
        // report needs a non-empty value)
        r.placement_decision_hash = r.placement_constraint_hash.clone();
        r
    };
    metrics.accumulate_determinism(&det_report);
    tracing::info!(target: "mcc::perf", step = "det_report", ms = _tdr.elapsed().as_millis() as u64, "render step");

    // ── Semantic analysis (read-only, soft signal) ──
    let semantic = SemanticModel::analyze(&graph);
    metrics.accumulate_semantic(&semantic.summary);

    // ── M10: Special power/ground/bus analysis (read-only) ──
    let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
    special.vlog_long_stubs(&name);
    metrics.accumulate_special(&special.report);

    // ── Phase F: engineer style soft metrics (read-only) ──
    // Every layer, deliberately: unlike the four audited families this one is not
    // gated on `audit.is_some()`, because its axes read placement, rails, labels
    // and routes — all of which a device sub-layer has as well.
    metrics.accumulate_engineer_style(&graph);

    debug::dump_route(&graph);

    super::route::wire_hops::apply_wire_hops(&mut graph);

    // ── ★ P1-c: decide the block edges once, here, after layout ──
    // The root block diagram used to re-run `decide_edges` inside
    // `render_block_edges`; the projection is prep work now, carried on the
    // graph so the renderer is a pure consumer.
    if graph.layer_style == crate::vector::graph::LayerStyle::Block {
        graph.block_edges = crate::viz::layout::edge_decide::decide_edges(&graph).0;
    }

    // ── Phase 3: render ──
    let svg = renderer.render(&graph, canvas, viewbox_origin);
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

        // ── ★ P7-1: renderdiff measurement (final graph after route, before render) ──
        let col = crate::viz::route::audit::audit_all(&graph);
        let reading = crate::viz::metrics::renderdiff::LayerReading::measure(
            &graph,
            &col,
            Some((conn_report.pins_total, conn_report.pins_unreachable)),
        );
        crate::vlog!(
            "[renderdiff] layer '{}' measured: boxes={} (declared={} synth={} flags={}) gnd_edges={} power_edges={} passives={} s6={} box_box={} wire_box={}",
            reading.layer,
            reading.total_boxes,
            reading.declared_boxes,
            reading.synth_endpoint_boxes,
            reading.rail_flag_boxes,
            reading.gnd_edges,
            reading.power_edges,
            reading.two_pin_passives,
            reading.s6_violations,
            reading.box_box,
            reading.wire_box
        );
        metrics.accumulate_renderdiff(reading);
    }

    let mut layer = VizLayer::new(bid, name, parent);
    layer.canvas = canvas;
    layer.svg = svg;
    layer.clickable_subs = clickable_subs;
    // P3 (opt-in): attach the supply-bundle model of a Block layer to the JSON
    // document. The default output stays byte-identical — the model is computed
    // and exported only under the gate.
    if graph.layer_style == crate::vector::graph::LayerStyle::Block
        && std::env::var("MCC_VIZ_SUPPLY_BUNDLES").map_or(false, |v| v == "1")
    {
        layer.supply_bundles = Some(crate::viz::layout::supply_bundle::export_json(&graph));
    }
    doc.add_layer(layer);

    // Last use of `graph` is behind us (render, connectivity, renderdiff all
    // borrow it), so the sink takes it by move: observing a run costs no clone.
    if let Some(sink) = sink.as_deref_mut() {
        sink.push(RenderedLayer {
            graph,
            parent,
            canvas,
            audited,
        });
    }

    for mut sub in sub_graphs {
        // ★ F2: sub-layers use Device pipeline
        sub.layer_style = crate::vector::graph::LayerStyle::Device;
        render_layer_recursive(
            doc,
            sub,
            Some(bid),
            false,
            top_layouter,
            sub_layouter,
            renderer,
            metrics,
            sink.as_deref_mut(),
        );
    }
}

// One-stop: graph → HTML

pub fn render_to_html(graph: McVecGraph) -> String {
    let doc = render(graph);
    super::template::wrap_document(&doc)
}
