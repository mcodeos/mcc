// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `stage.viz` — the laid-out schematic as a stage view.
//!
//! The last segment on the chain, and the only one that knows **where** things
//! are. `stage.p2` and `stage.vec` read the circuit before layout; this reads
//! the graph the renderer was handed, so it is the one view that can answer
//! "which pin ended up on which side of which box" (design §3, §11.1).
//!
//! Three things it deliberately does **not** do:
//!
//! - **It does not read the SVG.** [`VizLayer`](crate::viz::layer::VizLayer)
//!   stores the rendered string; reading that would make a change of drawing
//!   style read as a change of circuit, and a segment's coordinates are a path
//!   between two endpoints rather than an object (M5: compare the structure, not
//!   the picture). The structure comes from
//!   [`RenderedLayer`](crate::viz::api::RenderedLayer), the same value the
//!   renderer consumed — captured through the pipeline's observation sink, which
//!   every other caller leaves at `None`.
//! - **It does not re-publish the trunks — it publishes statement groups.** A
//!   [`Trunk`](crate::vector::model::trunk::Trunk) is built before layout and
//!   carries lanes of pins and no coordinates, so listing it here would be a
//!   second readout of a `stage.vec` fact with nothing added; the trunk *list*
//!   lives there, once. Which statements reached **the drawing** is a different
//!   fact, and only this view can state it, because only this view knows what
//!   was drawn: a statement produces a `group` item here even when its nets
//!   were merged into one drawn net, and a statement whose nets were dropped on
//!   the way has no item — the difference between the two is exactly what a
//!   reader cannot get from the trunk list.
//! - **It does not invent keys.** §2.4: a layer and a box own an instance path, a
//!   pin owns a `PointId`, and a **segment owns nothing** — it is not an object
//!   but a path between two endpoints, so its handle is the endpoint pair, and
//!   the cells that would hold a key print `-`.
//!
//! ## What the graph does *not* store (measured)
//!
//! `segment` publishes the drawn structure **the graph carries**, which on a
//! project drawing is less than a reader might expect, and the shortfall is a
//! fact about the pipeline rather than about this view:
//!
//! - `VizNet::route` is **empty on every layer**. Routing runs in
//!   `layout_best`, which is reached only when a layer is neither `Device` nor a
//!   block diagram; `render_layer_recursive` forces `Device` on every sub-layer
//!   and forces it on a root without sub-boxes, so the only layer that reaches
//!   the router is the root *block* layer, which takes the `is_root` path that
//!   skips it (`layout/select.rs`: "skip for root — root uses block edges").
//!   Measured on `hbl`: 0 route segments across all 7 layers, and the metrics
//!   agree (`visual.route_segments_total = 0`).
//! - A **device** layer's wires are not on the graph at all: the renderer builds
//!   them at render time from the equipotential trees
//!   (`equipotential_tree::build_all_trees`, called from `render/mod.rs`). They
//!   are *drawn* but they are not *stored*, and re-running the tree builder here
//!   to publish them would make this view a second producer of the geometry
//!   rather than a reader of it.
//! - A **block** layer's edges are stored (`McVecGraph::block_edges`) and are
//!   published, without coordinates: the polyline between two boxes is computed
//!   inside the renderer, and inventing one here would be a new authority on
//!   where a wire runs.
//!
//! One more fact about block edges, because a reader will otherwise read the
//! published ends as "the pins of the two boxes" and find they are not: an edge is
//! decided at the **net** layer, against the whole world's endpoints, while the
//! block layer's box for a sub-module carries only the pins the *diagram* shows.
//! So an end is resolved through the `InstTable` — which knows every row in the
//! world — and not through the layer's boxes; the same §2.4 discipline applies as
//! everywhere else, that an endpoint is not necessarily a point.
//!
//! How far the two faces drift is a measurement, and it moved (U106). On the
//! `hbl` fixture they now agree exactly: **0 of the 38** ends name a row the
//! layer's boxes do not carry, where 2 did before — `main.MIC.MIC.N` / `.P`, the
//! two ports `main.MIC`'s box carries, reached the drawing spelled
//! `main.MIC.N` / `.P`, and neither face could then find the other. The `hs`
//! board is where the gap is still real: **13 of its 142** ends name a row no box
//! carries (its test points, `main.TP1` / `.TP4`-`.TP8`), and 6 more publish no
//! path at all. So the lookup stays world-scoped: a layer-scoped one would drop
//! those 19 ends without a word, and on `hbl` it would have dropped the MIC pair
//! the same way.
//!
//! So `segments` counts what the graph owns. Contrast §11.3 of the viz
//! requirement design, which sketches a wire with a point list — that is the
//! view model the viz domain's M2/M4 batches are meant to produce, and this view
//! is the readout of what exists *today*.
//!
//! ## What the reports cover (measured)
//!
//! The `metrics` items are M4's reports, read from the accumulator that the
//! render just filled, and they carry two different scopes — which is why the
//! view publishes `report.scope.*` beside them:
//!
//! - `fidelity` / `truth` / `visual` / `readability` are accumulated only for
//!   layers the pipeline *audits*. On `hbl` that is 1 of 7 layers, so
//!   `truth.boxes_total` is 7 while this view lists 63 boxes. Both numbers are
//!   right; the scope row is what stops the first from being read as the second.
//! - `determinism` is not accumulated but **overwritten** per layer
//!   (`MetricsAccumulator::accumulate_determinism` assigns), so it describes one
//!   layer — and it names none. Recorded here rather than papered over; giving it
//!   a layer would mean changing what the accumulator does.
//! - `connectivity` merges over every layer, so its scope is the whole drawing.
//! - `engineer_style` also merges over every layer — including the device
//!   sub-layers the audited four skip, since its axes read box placement, rails,
//!   labels and routes, all of which exist there too. It is the one **soft**
//!   family, and the one whose axes score `1.0` when they find nothing to
//!   measure: each score is therefore published with its own sample count.
//!
//! A count is not a reference, so each `layer` item also carries **`reports`** —
//! the families whose published rows cover it (design §6.1's "item carries no
//! report reference", the second of M2's two gaps). The two scopes above are why
//! the reference is per layer and not one answer: `connectivity` and
//! `engineer_style` are on every layer, the audited four are on the layers the
//! pipeline audited, and `determinism` is on the single layer
//! [`determinism_layer`] reads back off the report's own geometry hash. A family
//! named there always has a row to land on, which is what makes the entry point
//! a reference rather than a promise.

use serde_json::{json, Value};

use crate::instant::insttab::InstTable;
use crate::vector::graph::boxdef::{BoxPin, EntryPoint, EntrySide, McVecBox};
use crate::vector::graph::graphdef::{LayerStyle, McVecGraph};
use crate::vector::graph::netdef::{Segment, VizNet};
use crate::viz::api::RenderedLayer;
use crate::viz::metrics::SchematicQualityReport;

use super::{
    canon_instance, loc_cell, loc_of, net_key, render_table, SourceText, StageSeg, StageView,
};

/// Build the `stage.viz` view from the layers the renderer saw, plus the
/// metrics it accumulated on the way.
///
/// The metrics arrive as the finished [`SchematicQualityReport`] rather than as
/// the accumulator: M4 asks for the *reports*, and finishing is the step that
/// merges per-layer readings into the numbers the design names. What the merged
/// numbers *cannot* say is how much of the drawing they cover, which is why the
/// scope rows are computed here, from the layers themselves.
pub fn build_viz(
    layers: &[RenderedLayer],
    quality: &SchematicQualityReport,
    table: &InstTable,
    top: &str,
    diagnostics: usize,
) -> StageView {
    let mut sources = SourceText::new();
    let mut items: Vec<Value> = Vec::new();

    // The statements this view can see, and the nets drawn for each — see
    // [`group_items`] for why the grouping is read from the flat table rather
    // than from the drawn net's own copies of its provenance.
    let statements = crate::stages::join::statement_refs(&mut sources);
    let mut drawn: std::collections::BTreeMap<usize, Vec<Value>> =
        std::collections::BTreeMap::new();

    // A sub-layer's canonical path is spelled from its parent's, and the sink
    // hands over the parent's `bid` rather than its path — so resolve them as we
    // walk. The sink is in pre-order, so a parent is always already resolved.
    let mut paths: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut audited = 0usize;
    // Resolved once, before the walk: the report is one value, so the layer it
    // belongs to cannot vary per layer.
    let det_bid = determinism_layer(layers, quality);
    for r in layers {
        let parent_path = r
            .parent
            .and_then(|p| paths.get(&p).cloned())
            .unwrap_or_default();
        let path = layer_path(&r.graph, table, &parent_path);
        paths.insert(r.graph.bid, path.clone());
        if r.audited {
            audited += 1;
        }

        items.push(layer_item(
            &r.graph,
            table,
            &path,
            &parent_path,
            r.canvas,
            r.audited,
            det_bid == Some(r.graph.bid),
        ));
        // Which net each pin is on, resolved once for the layer: a pin names its
        // net, so a consumer can group the ends of one net without a second
        // traversal of the graph.
        let nets = nets_by_pin(&r.graph, table);
        for b in &r.graph.boxes {
            items.push(box_item(b, table, &path, &mut sources));
            for p in &b.pins {
                items.push(pin_item(p, b, table, &path, &nets, &mut sources));
            }
        }
        // A block edge names its ends by pin id, and the id is run-local; the
        // endpoint's *handle* is its canonical path, so resolve them before
        // publishing (§2.4: a segment's handle is the endpoint pair).
        for (i, e) in r.graph.block_edges.iter().enumerate() {
            items.push(edge_item(e, table, &path, i, &mut sources));
        }
        for net in &r.graph.nets {
            if let Some(route) = &net.route {
                for (i, seg) in route.segments.iter().enumerate() {
                    items.push(segment_item(seg, net, &path, i));
                }
            }
            for s in net_statements(net, table, &statements) {
                let slot = drawn.entry(s).or_default();
                let already = slot.iter().any(|n| {
                    n["nid"] == json!(net.nid) && n["layer"].as_str() == Some(path.as_str())
                });
                if !already {
                    slot.push(json!({
                        "net": net_key(&net.name),
                        "nid": net.nid,
                        "name": net.name,
                        "layer": path,
                    }));
                }
            }
        }
    }

    items.extend(group_items(&statements, drawn, &mut sources));
    items.extend(metrics_items(quality, layers.len(), audited));

    StageView::new(StageSeg::Viz, top, items, diagnostics).carrying_drawing_contract()
}

/// One statement item per source statement this view drew nets for: the key the
/// hop readouts name it by, the nets it produced on this drawing, and how many.
///
/// The grouping is read from the **flat table**, through
/// [`RowAnchor`](crate::stages::join::RowAnchor) — never from the drawn net's
/// own `source_span` / `trunk_ref`. Those are copies taken at projection time,
/// and the projection merges and dedupes (that is what `vec.rs`'s
/// `ProjectionLog` records), so a merged net carries the name of one of the
/// statements that formed it and silently drops the rest. Grouping on it would
/// make a statement that produced a merged net look like it produced nothing —
/// the same failure this batch removes one layer up, which is why the table is
/// asked instead and why nothing falls back when a statement cannot be found.
///
/// A net no statement contains is not in any group: §5.2 hard constraint 2, no
/// fallback. That is a reading of the drawing, not a defect in it.
fn group_items(
    statements: &[crate::stages::join::StatementRef],
    drawn: std::collections::BTreeMap<usize, Vec<Value>>,
    sources: &mut SourceText,
) -> Vec<Value> {
    drawn
        .into_iter()
        .map(|(i, nets)| {
            let s = &statements[i];
            let count = nets.len();
            json!({
                "class": "group",
                "key": s.key,
                "point": Value::Null,
                "path": Value::Null,
                "canon_key": Value::Null,
                "text": s.text,
                "nets": nets,
                "count": count,
                "loc": loc_of(
                    Some(&crate::semantic::common::SourcePos::new(
                        s.uri.clone(),
                        s.start as u32,
                    )),
                    sources,
                ),
            })
        })
        .collect()
}

/// The statements a drawn net belongs to: for every endpoint, the row the pin
/// names in the flat table, and every position that row may be attributed
/// from — the same attribution the `join` hop uses, read through the same
/// [`RowAnchor`](crate::stages::join::RowAnchor), so the two faces cannot put
/// one row in different statements.
///
/// An endpoint whose pin id resolves to no row is skipped rather than guessed
/// at: a segment or a boundary end may name something the table does not carry.
fn net_statements(
    net: &VizNet,
    table: &InstTable,
    statements: &[crate::stages::join::StatementRef],
) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for e in &net.endpoints {
        let Some(entry) = (e.pin_id >= 0)
            .then(|| table.get_entry(e.pin_id as u32))
            .flatten()
        else {
            continue;
        };
        for at in crate::stages::join::RowAnchor::of(entry).attribution() {
            if let Some(i) =
                crate::stages::join::statement_ref_at(statements, &at.uri, at.offset as usize)
            {
                if !out.contains(&i) {
                    out.push(i);
                }
            }
        }
    }
    out
}

/// One end of a block edge: one entry per pin, ordered by canonical path so the
/// handle does not depend on which pin id happened to be lowest in this build.
///
/// Each end is resolved through the **`InstTable`**, not through this layer's
/// boxes, and the difference is not academic — see the module doc for the six
/// ends in `hbl` that only the table can resolve. An end may name a Pass2 row of
/// any class (the MIC edge's ends are `label` rows, not points), so this
/// publishes the path and the `PointId` when there is one, and does not claim the
/// row is a pin.
///
/// The id the edge carries is deliberately **not** published: it is a run-local
/// number, and a reader joins an end to a `pin` item by the path and the
/// `PointId` it already has.
fn edge_end(pins: &[i64], table: &InstTable) -> Value {
    let mut refs: Vec<(String, Option<String>)> = pins
        .iter()
        .map(|id| {
            let e = (*id >= 0).then(|| table.get_entry(*id as u32)).flatten();
            match e {
                Some(e) => (e.path.clone(), e.point.map(|pt| pt.to_string())),
                None => (String::new(), None),
            }
        })
        .collect();
    refs.sort();
    Value::Array(
        refs.into_iter()
            .map(|(path, point)| {
                json!({
                    "path": if path.is_empty() { Value::Null } else { Value::String(path) },
                    "point": point.map(Value::String).unwrap_or(Value::Null),
                })
            })
            .collect(),
    )
}

/// A layer's canonical path: its `InstTable` row when it has one, else the
/// enclosing path plus its own name. Same rule `stage.vec` applies to the same
/// graph, so the two views spell a layer identically.
fn layer_path(graph: &McVecGraph, table: &InstTable, parent: &str) -> String {
    if graph.bid >= 0 {
        if let Some(e) = table.get_entry(graph.bid as u32) {
            if !e.path.is_empty() {
                return e.path.clone();
            }
        }
    }
    if parent.is_empty() {
        graph.name.clone()
    } else {
        format!("{parent}.{}", graph.name)
    }
}

fn layer_item(
    g: &McVecGraph,
    table: &InstTable,
    path: &str,
    parent_path: &str,
    canvas: (f64, f64),
    audited: bool,
    determinism: bool,
) -> Value {
    let has_row = g.bid >= 0 && table.get_entry(g.bid as u32).is_some();
    let segments: usize = g
        .nets
        .iter()
        .filter_map(|n| n.route.as_ref())
        .map(|r| r.segments.len())
        .sum();
    json!({
        "class": "layer",
        "key": if has_row { Value::String(format!("D{}", g.bid)) } else { Value::Null },
        "point": Value::Null,
        "path": path,
        "canon_key": if has_row {
            canon_instance(table, g.bid as u32)
        } else {
            json!({ "path": path, "def": Value::Null })
        },
        "name": g.name,
        // `layer_style` is what the pipeline decided this layer *is*: a block
        // diagram (a module's boxes) or a device schematic (its internals).
        "style": style_str(g.layer_style),
        "parent": if parent_path.is_empty() { Value::Null } else { Value::String(parent_path.to_string()) },
        "boxes": g.boxes.len(),
        "nets": g.nets.len(),
        "edges": g.block_edges.len(),
        "segments": segments,
        "canvas": [canvas.0, canvas.1],
        // Whether the accumulated reports cover this layer. Printed per layer so
        // the scope row is checkable against the rows it summarises.
        "audited": audited,
        // Which report families they *are*, so a reader goes from the drawing to
        // the numbers by lookup instead of by inference. `audited` is the raw
        // flag the pipeline handed over; this is the reference built from it.
        "reports": reports_of(audited, determinism),
        "loc": Value::Null,
    })
}

/// The report families whose published rows cover one layer, in a fixed order.
///
/// `connectivity` and `engineer_style` are on every layer because their
/// accumulators merge over all of them; `determinism` is on one, because that
/// accumulator **assigns**; the four audited families are on the layers the
/// pipeline audited.
fn reports_of(audited: bool, determinism: bool) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    if audited {
        out.extend(["fidelity", "truth", "visual", "readability"]);
    }
    if determinism {
        out.push("determinism");
    }
    out.extend(["connectivity", "engineer_style"]);
    out
}

/// The layer the surviving determinism report was taken from, or `None` when
/// that cannot be told.
///
/// `MetricsAccumulator::accumulate_determinism` assigns rather than merges, so
/// the published report describes exactly one layer — and names none (module
/// doc, and `view-model-design.md` §6.2). But it carries the box-geometry hash
/// it was taken with, and that hash is computed over the same five fields of the
/// same boxes this view walks, so the layer can be **read back** off the report
/// rather than guessed from the order the pipeline happened to run in.
///
/// Built on the report's own hash function and on nothing else: if the pipeline
/// stops stamping that field, or two layers carry identical geometry, the answer
/// is `None` and no layer claims the family — which is the truth, and is
/// checkable in the output.
fn determinism_layer(layers: &[RenderedLayer], q: &SchematicQualityReport) -> Option<i64> {
    let want = q.determinism.as_ref()?.graph_input_hash.as_str();
    if want.is_empty() {
        return None;
    }
    let mut hit: Option<i64> = None;
    for r in layers {
        if crate::viz::stability::hash::hash_box_geometry(&r.graph) == want {
            if hit.is_some() {
                return None;
            }
            hit = Some(r.graph.bid);
        }
    }
    hit
}

/// One block-diagram edge: a wire the renderer will draw between two boxes.
///
/// It is a `segment` because that is the class for "a connection that is drawn",
/// and it carries `kind: "edge"` to say where it came from — the graph's own
/// edge list, not a routed polyline. Its ends are pins, spelled canonically; its
/// `net` is the edge's **label**, which is a display name and is published as
/// one (§0.2: a label never carries identity, so it never becomes the handle).
fn edge_item(
    e: &crate::viz::layout::edge_decide::BlockEdge,
    table: &InstTable,
    layer: &str,
    index: usize,
    sources: &mut SourceText,
) -> Value {
    json!({
        "class": "segment",
        "key": Value::Null,
        "point": Value::Null,
        "path": Value::Null,
        "canon_key": Value::Null,
        "kind": "edge",
        "net": e.label,
        "nid": Value::Null,
        "index": index,
        "edge_kind": format!("{:?}", e.kind).to_lowercase(),
        "lanes": e.lane_count,
        // An anonymous trunk group has no name; the row then prints `-`.
        "trunk": e.trunk.as_ref().and_then(|t| t.name.clone()).map(Value::String).unwrap_or(Value::Null),
        // The paired return face of a power edge's hot net, as declared. A
        // declaration, not a drawing: it says which net returns, not where the
        // wire runs.
        "ret": e.ret.as_ref().map(|r| Value::String(r.clone())).unwrap_or(Value::Null),
        "from": edge_end(&e.from_pins, table),
        "to": edge_end(&e.to_pins, table),
        // Same key set as a wire segment, so one segment row has one shape: an
        // edge has ends but no coordinates (see the module doc), a wire has
        // coordinates but no ends.
        "from_at": Value::Null,
        "to_at": Value::Null,
        "length": Value::Null,
        "layer": layer,
        "loc": loc_of(e.source_span.as_ref(), sources),
    })
}

fn style_str(s: LayerStyle) -> &'static str {
    match s {
        LayerStyle::Block => "block",
        LayerStyle::Device => "device",
    }
}

fn box_item(b: &McVecBox, table: &InstTable, layer: &str, sources: &mut SourceText) -> Value {
    let has_row = b.id >= 0 && table.get_entry(b.id as u32).is_some();
    json!({
        "class": "box",
        "key": if has_row { Value::String(format!("D{}", b.id)) } else { Value::Null },
        "point": Value::Null,
        "path": b.inst_path,
        "canon_key": if has_row {
            canon_instance(table, b.id as u32)
        } else {
            json!({ "path": b.inst_path, "def": Value::Null })
        },
        "name": b.name,
        "class_name": b.class_name,
        "kind": b.kind.to_string(),
        // The two numbers layout exists to produce. A box with no key is still
        // at a place, which is why they are published for every box.
        "at": [b.x, b.y],
        "size": [b.w, b.h],
        "pins": b.pins.len(),
        "anchors": b.entry_points.len(),
        "layer": layer,
        "loc": loc_of(b.source_span.as_ref(), sources),
    })
}

/// One physical pin of a box, with the anchor layout gave it.
///
/// `side` / `offset` are the layout's own decision (which edge, where along it)
/// and are the reason this view exists; `at` is those two plus the box's
/// position, and is repeated here because the arithmetic is otherwise spread
/// over four fields a reader would have to join — see [`anchor_x`] for the rule
/// and for why this view spells it a third time.
fn pin_item(
    p: &BoxPin,
    b: &McVecBox,
    table: &InstTable,
    layer: &str,
    nets: &NetsByPin,
    sources: &mut SourceText,
) -> Value {
    let mut anchors = b.entry_points.iter().filter(|e| e.pin_id == p.id);
    let anchor = anchors.next();
    let path = pin_path(table, p.id);
    let net = path.as_ref().and_then(|p| nets.get(p.as_str()));
    json!({
        "class": "pin",
        // The stage key: the same `PointId` the net layer derived and the pin
        // element publishes as `data-point`. A placeholder pin never gets one.
        "key": p.point.map(|pt| Value::String(pt.to_string())).unwrap_or(Value::Null),
        "point": p.point.map(|pt| pt.to_string()),
        "path": path.clone().map(Value::String).unwrap_or(Value::Null),
        "canon_key": match &path {
            Some(_) => canon_instance(table, p.id as u32),
            None => Value::Null,
        },
        // Which net this pin is on — see [`nets_by_pin`] for the two halves and
        // for why the second one is not a key.
        "net": net.and_then(|n| n.key.clone()).map(Value::String).unwrap_or(Value::Null),
        "nid": net.map(|n| json!(n.nid)).unwrap_or(Value::Null),
        // `pin_id` is the number written on the stub ("1", "B", "A1");
        // `description` is the function name drawn inside the box.
        "num": p.pin_id,
        "name": p.description,
        "io": format!("{:?}", p.io).to_lowercase(),
        "side": anchor.map(|a| Value::String(side_str(a.side).to_string())).unwrap_or(Value::Null),
        "offset": anchor.map(|a| json!(a.offset)).unwrap_or(Value::Null),
        "at": anchor.map(|a| json!([anchor_x(a, b), anchor_y(a, b)])).unwrap_or(Value::Null),
        // Several anchors may name one pin (the pin's identity is not its
        // placement). The first in the layer's own anchor list is the one
        // reported above; the count says whether that was the whole story.
        "anchors": b.entry_points.iter().filter(|e| e.pin_id == p.id).count(),
        "box": b.inst_path,
        "layer": layer,
        "loc": loc_of(p.src_span.as_ref(), sources),
    })
}

fn side_str(s: EntrySide) -> &'static str {
    match s {
        EntrySide::Top => "top",
        EntrySide::Right => "right",
        EntrySide::Bottom => "bottom",
        EntrySide::Left => "left",
    }
}

/// The net one pin is on, as the layer's graph records it.
#[derive(Debug, Clone)]
struct PinNet {
    /// The net's canonical key, `None` when the name it carries is not the
    /// source's ([`net_key`]).
    key: Option<String>,
    /// The net's id **within this build**, which is an index and not a key: it
    /// is an allocation ordinal, so it is not comparable across two builds and
    /// is published beside the key rather than instead of it (§3.5). It is what
    /// still groups the pins of a net that owns no key.
    nid: i64,
}

/// A layer's nets, indexed by the canonical path of each pin they join.
///
/// A pin names its own net, which is the one reference the SVM skeleton spells
/// on every pin (§4: `"pins": [ { "id": "pin:…", "net": "net:V3V3", … } ]`), and
/// this is where the answer is found: the graph keeps a net as a *member set*
/// ([`VizNet::endpoints`]), so the pin's path — not its row id, not its
/// `PointId` — is what joins them. Both sides spell a path through the same
/// `InstTable` row, so the join cannot be off by a numbering.
///
/// A pin on no net at all gets **no entry**, and so prints `null`. That is not
/// the same as a net whose key is `null`: the `nid` beside it still tells the two
/// apart. The family itself has two members on a real project and both are
/// readings rather than accidents: a pin nothing connects to (the ERC's
/// unconnected-pin reports), and a pin whose net the render **promoted away** —
/// promotion replaces `graph.nets` with the nets that reach at least one box of
/// the layer, and this view reads the graph the renderer consumed.
///
/// ⚠ This is deliberately **not** a net item. A figure element carries a
/// *reference* to its net, and the connectivity is looked up in the projection
/// (design D1), so the net's own row — its name, kind, role and members — is a
/// `stage.vec` fact that stays there once. What this view adds is which pin is on
/// which net, which is a fact about the drawing.
fn nets_by_pin(g: &McVecGraph, table: &InstTable) -> NetsByPin {
    let mut out: NetsByPin = std::collections::HashMap::new();
    for net in &g.nets {
        let key = net_key(&net.name);
        for e in &net.endpoints {
            if let Some(p) = pin_path(table, e.pin_id) {
                out.insert(
                    p,
                    PinNet {
                        key: key.clone(),
                        nid: net.nid,
                    },
                );
            }
        }
    }
    out
}

/// One layer's nets by the canonical path of each pin they join. Scoped to a
/// layer on purpose: a net is built per layer, and two layers number their nets
/// independently, so a path lookup that crossed layers would answer with
/// another layer's net.
type NetsByPin = std::collections::HashMap<String, PinNet>;

/// One routed segment of a net: a straight run between two points.
///
/// It has no `key` on purpose (§2.4): a segment is not an object, it is the
/// path between two endpoints, so its handle is the endpoint pair and its owner
/// is named by `net`. Its `index` is a within-net position, printed for a human
/// to read in order and never used as an identity.
///
/// Geometry lives in `from_at` / `to_at` rather than in `from` / `to`, so that
/// the handle fields mean the same thing on every `segment`: the endpoint pair,
/// which a routed segment cannot name (it records where the run starts and ends,
/// not which pins those are) and which is therefore `null` here.
fn segment_item(seg: &Segment, net: &VizNet, layer: &str, index: usize) -> Value {
    json!({
        "class": "segment",
        "key": Value::Null,
        "point": Value::Null,
        "path": Value::Null,
        "canon_key": Value::Null,
        "kind": "wire",
        "net": net.name,
        "nid": net.nid,
        "index": index,
        "edge_kind": Value::Null,
        "lanes": Value::Null,
        "trunk": Value::Null,
        "ret": Value::Null,
        "from": Value::Null,
        "to": Value::Null,
        "from_at": [seg.from.x, seg.from.y],
        "to_at": [seg.to.x, seg.to.y],
        "length": seg_length(seg),
        "layer": layer,
        "loc": Value::Null,
    })
}

fn seg_length(seg: &Segment) -> f64 {
    let dx = seg.to.x - seg.from.x;
    let dy = seg.to.y - seg.from.y;
    (dx * dx + dy * dy).sqrt()
}

/// The M4 reports, one item per field.
///
/// Per field rather than one item per report because a reader comparing two
/// runs wants to see *which number moved*, and a diff of a nested object makes
/// that the reader's job. Each row's `path` is `<family>.<field>`, so the sort
/// groups the families and the text face prints a value per line.
///
/// `renderdiff` (P7-1) is not here, and the reason is not that it is a gate: it
/// reports a **comparison against a baseline**, and its answer depends on
/// `MC_RENDER_GOLDEN`/`MC_RENDER_GOLDEN_SAVE` besides. This envelope is a
/// function of the world alone — every other row is the same bytes whatever the
/// environment says — and a family that changed with an env var would be the one
/// exception to that. It stays where it is, behind `MC_VIZ_DUMP`.
///
/// The `scope` family is published first and is not a report — it is the answer
/// to "how much of the drawing do these numbers cover". See the module doc for
/// which families share which scope; the two counts here are what make the
/// difference readable instead of inferable.
fn metrics_items(q: &SchematicQualityReport, layers: usize, audited: usize) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    push_fields(
        &mut out,
        "scope",
        &[
            ("layers", json!(layers)),
            ("audited_layers", json!(audited)),
        ],
    );

    let f = &q.fidelity;
    push_fields(
        &mut out,
        "fidelity",
        &[
            ("nets_total", json!(f.nets_total)),
            ("nets_rendered", json!(f.nets_rendered)),
            ("nets_dropped", json!(f.nets_dropped)),
            ("nets_partial", json!(f.nets_partial)),
            ("pins_total", json!(f.pins_total)),
            ("pins_rendered", json!(f.pins_rendered)),
            ("bus_bits_total", json!(f.bus_bits_total)),
            ("bus_bits_paired_ok", json!(f.bus_bits_paired_ok)),
            ("authored_sides_total", json!(f.authored_sides_total)),
            ("authored_sides_honored", json!(f.authored_sides_honored)),
            ("box_box", json!(f.box_box)),
            ("wire_box", json!(f.wire_box)),
            ("islands_claimed", json!(f.islands_claimed)),
            ("islands_total", json!(f.islands_total)),
        ],
    );

    let t = &q.truth;
    push_fields(
        &mut out,
        "truth",
        &[
            ("layers_total", json!(t.layers_total)),
            ("nets_total", json!(t.nets_total)),
            ("drawable_nets_total", json!(t.drawable_nets_total)),
            ("routed_nets_total", json!(t.routed_nets_total)),
            ("nets_missing_route", json!(t.nets_missing_route)),
            ("nets_empty_route", json!(t.nets_empty_route)),
            ("endpoints_total", json!(t.endpoints_total)),
            (
                "drawable_endpoints_total",
                json!(t.drawable_endpoints_total),
            ),
            ("endpoints_box_missing", json!(t.endpoints_box_missing)),
            ("endpoints_pin_missing", json!(t.endpoints_pin_missing)),
            ("endpoints_entry_missing", json!(t.endpoints_entry_missing)),
            (
                "endpoints_route_unreached",
                json!(t.endpoints_route_unreached),
            ),
            ("boxes_total", json!(t.boxes_total)),
            ("physical_pins_total", json!(t.physical_pins_total)),
            (
                "physical_pins_with_entry",
                json!(t.physical_pins_with_entry),
            ),
            (
                "physical_pins_missing_entry",
                json!(t.physical_pins_missing_entry),
            ),
        ],
    );

    let v = &q.visual;
    push_fields(
        &mut out,
        "visual",
        &[
            ("canvas_width", json!(v.canvas_width)),
            ("canvas_height", json!(v.canvas_height)),
            ("boxes_total", json!(v.boxes_total)),
            ("box_area_total", json!(v.box_area_total)),
            ("box_density", json!(v.box_density)),
            ("labels_total", json!(v.labels_total)),
            ("label_label_overlaps", json!(v.label_label_overlaps)),
            ("label_box_overlaps", json!(v.label_box_overlaps)),
            ("label_wire_overlaps", json!(v.label_wire_overlaps)),
            ("labels_off_canvas", json!(v.labels_off_canvas)),
            ("routed_nets", json!(v.routed_nets)),
            ("route_segments_total", json!(v.route_segments_total)),
            ("route_bends_total", json!(v.route_bends_total)),
            ("route_length_total", json!(v.route_length_total)),
            ("symmetry_penalty", json!(v.symmetry_penalty)),
            ("idiom_violations", json!(v.idiom_violations)),
        ],
    );

    let r = &q.readability;
    push_fields(
        &mut out,
        "readability",
        &[
            ("wire_wire", json!(r.wire_wire)),
            ("total_wirelength", json!(r.total_wirelength)),
            ("total_bends", json!(r.total_bends)),
            ("off_grid_penalty", json!(r.off_grid_penalty)),
            ("weighted", json!(r.weighted())),
        ],
    );

    // M12 and M13 are per-run accumulations and may be absent (a render with no
    // layer reports neither); they still get a row each, so the row set does not
    // change shape between runs.
    match &q.determinism {
        Some(d) => push_fields(
            &mut out,
            "determinism",
            &[
                ("graph_input_hash", json!(d.graph_input_hash)),
                ("box_order_hash", json!(d.box_order_hash)),
                ("net_order_hash", json!(d.net_order_hash)),
                ("pin_anchor_hash", json!(d.pin_anchor_hash)),
                ("route_schedule_hash", json!(d.route_schedule_hash)),
                ("route_geometry_hash", json!(d.route_geometry_hash)),
                ("metrics_hash", json!(d.metrics_hash)),
                (
                    "placement_constraint_hash",
                    json!(d.placement_constraint_hash),
                ),
                ("idiom_instance_hash", json!(d.idiom_instance_hash)),
                ("unstable_decisions", json!(d.unstable_decisions)),
            ],
        ),
        None => out.push(absent_family("determinism")),
    }
    match &q.rendered_connectivity {
        Some(c) => push_fields(
            &mut out,
            "connectivity",
            &[
                ("is_perfect", json!(c.is_perfect)),
                ("pins_total", json!(c.pins_total)),
                ("pins_reachable", json!(c.pins_reachable)),
                ("pins_unreachable", json!(c.pins_unreachable)),
                ("nets_total", json!(c.nets_total)),
                ("nets_perfect", json!(c.nets_perfect)),
                (
                    "nets_with_render_mismatch",
                    json!(c.nets_with_render_mismatch),
                ),
                ("false_connections", json!(c.false_connections)),
                ("missing_connections", json!(c.missing_connections)),
                ("false_junctions", json!(c.false_junctions)),
                ("missing_junctions", json!(c.missing_junctions)),
                ("different_net_crossings", json!(c.different_net_crossings)),
                (
                    "different_net_crossings_with_hop",
                    json!(c.different_net_crossings_with_hop),
                ),
                (
                    "different_net_crossings_without_hop",
                    json!(c.different_net_crossings_without_hop),
                ),
                ("ambiguous_near_misses", json!(c.ambiguous_near_misses)),
                ("connectivity_hash", json!(c.connectivity_hash)),
            ],
        ),
        None => out.push(absent_family("connectivity")),
    }

    // Engineer style is the one **soft** family: informational, never a gate,
    // and the only one whose axes score `1.0` when they have nothing to measure.
    // So every score is published with the count it was measured over — read as
    // a pair. `1.0` with a zero count is an axis that found nothing, not one
    // that found everything in order; without the count the two are the same
    // row, which is the defect this family was wired up to avoid.
    let e = &q.engineer_style;
    push_fields(
        &mut out,
        "engineer_style",
        &[
            (
                "signal_flow_monotonicity",
                json!(e.signal_flow_monotonicity),
            ),
            ("signal_flow_samples", json!(e.signal_flow_samples)),
            ("rail_alignment_score", json!(e.rail_alignment_score)),
            ("rail_alignment_samples", json!(e.rail_alignment_samples)),
            ("ground_alignment_score", json!(e.ground_alignment_score)),
            (
                "ground_alignment_samples",
                json!(e.ground_alignment_samples),
            ),
            ("bus_order_score", json!(e.bus_order_score)),
            ("bus_order_samples", json!(e.bus_order_samples)),
            ("idiom_proximity_score", json!(e.idiom_proximity_score)),
            ("idiom_proximity_samples", json!(e.idiom_proximity_samples)),
            (
                "pin_side_intent_honor_rate",
                json!(e.pin_side_intent_honor_rate),
            ),
            ("pin_side_intent_samples", json!(e.pin_side_intent_samples)),
            (
                "functional_block_compactness",
                json!(e.functional_block_compactness),
            ),
            (
                "functional_block_samples",
                json!(e.functional_block_samples),
            ),
            ("route_channel_clarity", json!(e.route_channel_clarity)),
            ("route_channel_samples", json!(e.route_channel_samples)),
            ("label_readability_score", json!(e.label_readability_score)),
            (
                "label_readability_samples",
                json!(e.label_readability_samples),
            ),
        ],
    );
    out
}

fn push_fields(out: &mut Vec<Value>, family: &str, fields: &[(&str, Value)]) {
    for (name, value) in fields {
        out.push(json!({
            "class": "metrics",
            "key": Value::Null,
            "point": Value::Null,
            "path": format!("{family}.{name}"),
            "canon_key": Value::Null,
            "family": family,
            "field": name,
            "value": value,
            "layer": Value::Null,
            "loc": Value::Null,
        }));
    }
}

/// A family whose report is absent still prints its name, with the value left
/// `null` so the text face shows `-` (§5.3: a missing value is `-`, not a row
/// that quietly disappears).
fn absent_family(family: &str) -> Value {
    json!({
        "class": "metrics",
        "key": Value::Null,
        "point": Value::Null,
        "path": family,
        "canon_key": Value::Null,
        "family": family,
        "field": Value::Null,
        "value": Value::Null,
        "layer": Value::Null,
        "loc": Value::Null,
    })
}

/// The pin's canonical path, when it has an `InstTable` row. `id < 0` marks the
/// pins the viz layer invents, which own no path.
fn pin_path(table: &InstTable, pin_id: i64) -> Option<String> {
    if pin_id < 0 {
        return None;
    }
    let p = table.get_entry(pin_id as u32)?.path.clone();
    if p.is_empty() {
        None
    } else {
        Some(p)
    }
}

/// The absolute point of one anchor, from the box it hangs on: an edge plus how
/// far along it, in the box's own canvas.
///
/// The rule, all four sides:
///
/// ```text
/// Top    -> (x + w * offset, y)
/// Bottom -> (x + w * offset, y + h)
/// Left   -> (x,             y + h * offset)
/// Right  -> (x + w,         y + h * offset)
/// ```
///
/// Spelled here rather than called from the pipeline because the pipeline has no
/// shared function for it — the same four-arm match is written out in seven
/// places (`route/side.rs`, `route/feedback.rs`, `route/wire_label_split.rs`,
/// `metrics/mod.rs`, `metrics/renderdiff.rs`, `connectivity/model.rs`,
/// `layout/flow.rs`). A third copy is therefore no worse than the status quo, and
/// the alternative — reaching into another stage's private helper — is not
/// available. What *is* required is that this copy be right, which is what the
/// acceptance test checks against the box's own geometry.
fn anchor_x(a: &EntryPoint, b: &McVecBox) -> f64 {
    match a.side {
        EntrySide::Top | EntrySide::Bottom => b.x + b.w * a.offset,
        EntrySide::Left => b.x,
        EntrySide::Right => b.x + b.w,
    }
}

fn anchor_y(a: &EntryPoint, b: &McVecBox) -> f64 {
    match a.side {
        EntrySide::Top => b.y,
        EntrySide::Bottom => b.y + b.h,
        EntrySide::Left | EntrySide::Right => b.y + b.h * a.offset,
    }
}

/// Render the `stage.viz` text face from the *same* items the JSON face uses
/// (design §5.3 ruling ③). Columns: key, path-or-name, detail, loc — first
/// column always the key, last always `loc` (§5.3 ①).
pub fn render_viz_text(view: &StageView) -> String {
    let rows: Vec<Vec<String>> = view
        .items
        .iter()
        .map(|item| {
            let class = item["class"].as_str().unwrap_or("");
            let key = item["key"].as_str().unwrap_or("-").to_string();
            let second = match class {
                "segment" => item["net"].as_str().unwrap_or("-").to_string(),
                // A statement group's own text, not a path: the group is a
                // source statement, and the second column is where a reader
                // reads what was written.
                "group" => item["text"].as_str().unwrap_or("-").to_string(),
                _ => item["path"].as_str().unwrap_or("-").to_string(),
            };
            let third = match class {
                // How many nets this statement produced on the drawing. The
                // nets themselves are on the JSON face, where the `net`/`nid`
                // pair joins them to the `pin` items; a count is what a column
                // can say without becoming a list.
                "group" => format!("nets={}", item["count"].as_u64().unwrap_or(0)),
                // A layer's own numbers, plus the space its coordinates are in:
                // a position without a canvas is not a reading. `audited` is
                // spelled rather than tabulated because it is the scope of the
                // report rows at the bottom of the same output.
                "layer" => format!(
                    "{} boxes={} nets={} edges={} segments={} canvas={}x{} audited={} reports={}",
                    item["style"].as_str().unwrap_or("-"),
                    item["boxes"].as_u64().unwrap_or(0),
                    item["nets"].as_u64().unwrap_or(0),
                    item["edges"].as_u64().unwrap_or(0),
                    item["segments"].as_u64().unwrap_or(0),
                    num(&item["canvas"][0]),
                    num(&item["canvas"][1]),
                    item["audited"].as_bool().unwrap_or(false),
                    // The families this layer's row is the entry point to, on the
                    // face a reader reads by default — a report row at the bottom
                    // of the same output is then reachable from the layer it is
                    // about. `-` when none, the same glyph as any missing value.
                    family_list(&item["reports"]),
                ),
                "box" => {
                    let c = item["class_name"].as_str().filter(|s| !s.is_empty());
                    format!(
                        "{} at=({},{}) {}x{}",
                        c.unwrap_or_else(|| item["kind"].as_str().unwrap_or("-")),
                        num(&item["at"][0]),
                        num(&item["at"][1]),
                        num(&item["size"][0]),
                        num(&item["size"][1]),
                    )
                }
                "pin" => format!(
                    "num={} name={} {} side={}@{} at=({},{}) net={}",
                    item["num"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("-"),
                    item["name"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("-"),
                    item["io"].as_str().unwrap_or("-"),
                    item["side"].as_str().unwrap_or("-"),
                    num(&item["offset"]),
                    num(&item["at"][0]),
                    num(&item["at"][1]),
                    // The net's canonical key, or `-` — the same glyph §5.3
                    // fixes for a value that is not there. A pin on a net that
                    // owns no key prints `-` here too: the key is what this
                    // column carries, and the `nid` behind it is an index, not
                    // a second spelling of the same thing.
                    item["net"].as_str().unwrap_or("-"),
                ),
                "segment" => match item["kind"].as_str().unwrap_or("") {
                    // An edge has ends (canonical paths) and no coordinates.
                    "edge" => format!(
                        "#{} {} {} lanes={} -> {}",
                        item["index"].as_u64().unwrap_or(0),
                        item["edge_kind"].as_str().unwrap_or("-"),
                        end_list(&item["from"]),
                        item["lanes"].as_u64().unwrap_or(0),
                        end_list(&item["to"]),
                    ),
                    // A wire has coordinates and no ends.
                    _ => format!(
                        "#{} ({},{})->({},{}) len={}",
                        item["index"].as_u64().unwrap_or(0),
                        num(&item["from_at"][0]),
                        num(&item["from_at"][1]),
                        num(&item["to_at"][0]),
                        num(&item["to_at"][1]),
                        num(&item["length"]),
                    ),
                },
                "metrics" => match &item["value"] {
                    Value::Null => "-".to_string(),
                    Value::String(s) => s.clone(),
                    v => v.to_string(),
                },
                _ => "-".to_string(),
            };
            vec![key, second, third, loc_cell(&item["loc"]).to_string()]
        })
        .collect();

    let mut out = vec![view.header_line(), view.counts_line(StageSeg::Viz)];
    render_table(&rows, &mut out);
    out.join("\n")
}

/// A list of report family names as the text face prints it, comma separated.
/// An empty list prints `-`, the glyph §5.3 fixes for a value that is not there.
fn family_list(v: &Value) -> String {
    let Some(arr) = v.as_array() else {
        return "-".to_string();
    };
    if arr.is_empty() {
        return "-".to_string();
    }
    arr.iter()
        .map(|e| e.as_str().unwrap_or("-").to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// One end of a block edge as the text face prints it: the canonical paths of
/// the pins it attaches to, comma separated.
///
/// An end with no pins prints `-`; one pin that resolves to no path prints `-`
/// too, so `-,-` reads as "two pins, neither placeable" while `-` reads as
/// "nothing here". Both use the missing-value glyph §5.3 fixes rather than a
/// third symbol of this view's own.
fn end_list(v: &Value) -> String {
    let Some(arr) = v.as_array() else {
        return "-".to_string();
    };
    if arr.is_empty() {
        return "-".to_string();
    }
    arr.iter()
        .map(|e| e["path"].as_str().unwrap_or("-").to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// A coordinate as the text face prints it. An absent coordinate prints `-`,
/// the same as any other missing value; a present one prints its shortest
/// round-tripping form so that "did this move?" is answerable by reading.
fn num(v: &Value) -> String {
    if v.is_null() {
        return "-".to_string();
    }
    match v.as_f64() {
        Some(f) if f.fract() == 0.0 => format!("{}", f as i64),
        Some(f) => format!("{f}"),
        None => "-".to_string(),
    }
}
