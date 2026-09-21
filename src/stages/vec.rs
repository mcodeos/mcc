// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `stage.vec` — the vector graph as a stage view.
//!
//! The segment between Pass2 and the renderer. Two things make it worth reading
//! as data rather than as a dump:
//!
//! - **It is the first segment whose objects are not all instances.** A layer is
//!   a `bid`, a box is an instance, an endpoint is a pin, a net is a member set
//!   and a trunk has no id at all (design §2.4). Each class therefore carries
//!   exactly the key its kind owns, and nothing is invented for the kinds that
//!   own none.
//! - **It is where nets are projected, and the projection already keeps a
//!   record.** [`crate::viz::project::ProjectionLog`] holds one row per layer
//!   (net count before → after) and one row per action (merge / dedup /
//!   removal). Publishing that log is what lets a reader answer "why does
//!   `stage.p2` count 19 nets here and this view 14" from the *same*
//!   computation instead of from a second derivation that happens to agree.
//!
//! Everything is read from the graph, never from the rendered SVG: the design is
//! explicit that a stage view compares structure, not drawing (§11.1, M5).

use serde_json::{json, Value};
use std::collections::BTreeMap;

use crate::instant::insttab::InstTable;
use crate::vector::graph::boxdef::McVecBox;
use crate::vector::graph::graphdef::{LayerStyle, McVecGraph};
use crate::vector::graph::netdef::EndpointRef;
use crate::vector::model::trunk::Trunk;
use crate::viz::project::ProjectionLog;

use super::{
    canon_instance, loc_cell, loc_of, net_key, net_origin, render_table, SourceText, StageSeg,
    StageView,
};

/// Build the `stage.vec` view over a graph and the projection log that produced
/// it.
///
/// The log is passed in rather than recomputed: it is the projection's own
/// account of what it did, and re-deriving the same numbers here would make the
/// two agree by coincidence instead of by construction.
pub fn build_vec(
    graph: &McVecGraph,
    log: &ProjectionLog,
    table: &InstTable,
    top: &str,
    diagnostics: usize,
) -> StageView {
    let mut sources = SourceText::new();
    let mut items: Vec<Value> = Vec::new();
    let homes = endpoint_homes(graph, table, "");
    let mut seen_boxes = std::collections::HashSet::new();
    walk(graph, table, "", &mut sources, &mut items, &homes, &mut seen_boxes);

    // The projection's own account: one row per layer, then one per action.
    // `before` / `after` are the only place the *net count change* is visible —
    // a view that reported one number could never explain the other.
    for (layer, before, after) in &log.per_layer {
        items.push(json!({
            "class": "projection",
            "key": Value::Null,
            "point": Value::Null,
            "path": format!("{layer}/nets"),
            "canon_key": Value::Null,
            "layer": layer,
            "net": Value::Null,
            "endpoint": Value::Null,
            "rule": "-",
            "before": before,
            "after": after,
            "note": "-",
            "loc": Value::Null,
        }));
    }
    for r in &log.records {
        // `net` and `endpoint` are published as fields of their own, not only as
        // the two halves of `path`. A consumer comparing two readings has to
        // state which row is which, and the alternative -- splitting `path`
        // back apart -- would be recovering structure from a formatted string,
        // which this project does not do. The net component in particular
        // has to be readable on its own: a name the builder minted is not an
        // identity, and a row keyed on one would claim a stability it does not
        // have (`stages::net_origin`).
        items.push(json!({
            "class": "projection",
            "key": Value::Null,
            "point": Value::Null,
            "path": format!("{}/{}/{}", r.layer, r.net, r.endpoint),
            "canon_key": Value::Null,
            "layer": r.layer,
            "net": r.net,
            "endpoint": r.endpoint,
            "rule": r.rule,
            "before": Value::Null,
            "after": Value::Null,
            "note": r.note,
            "loc": Value::Null,
        }));
    }

    StageView::new(StageSeg::Vec, top, items, diagnostics)
}

/// One endpoint row per pin, and the layer that emits it.
///
/// §2.4: an endpoint *is* the pin, so the segment holds one endpoint object per
/// pin. A conductor that crosses a module boundary is claimed by both scopes'
/// nets — the boundary anchors on the pin itself (CIMP §1 U127: one pin one id,
/// the junction runs both segments) — and emitting a row per claim would key
/// two objects on one pin. So the row is emitted in the pin's own module layer
/// (`endpoint_homes`), and a foreign scope carries the pin through its net
/// row's member list, which is the member set that keys the net anyway. A pin
/// claimed twice at the same layer is left alone: two same-key rows there is a
/// real defect, and the diff's duplicate-key mark is how it surfaces.
fn endpoint_homes(graph: &McVecGraph, table: &InstTable, parent: &str) -> BTreeMap<i64, String> {
    let mut claims: BTreeMap<i64, (Option<String>, Vec<String>)> = BTreeMap::new();
    collect_claims(graph, table, parent, &mut claims);
    claims
        .into_iter()
        .map(|(pin, (owner, layers))| {
            // The pin's own scope wins when it claims the pin; otherwise the
            // smallest layer path keeps the pick deterministic.
            let home = match owner {
                Some(o) if layers.iter().any(|l| *l == o) => o,
                _ => layers
                    .into_iter()
                    .min()
                    .expect("a claim list is never empty"),
            };
            (pin, home)
        })
        .collect()
}

/// The layer paths that claim each pin — with the pin's own module scope, read
/// off its canonical path (`scope.pin` minus the pin and its component) — walked
/// in the same order `walk` uses.
fn collect_claims(
    graph: &McVecGraph,
    table: &InstTable,
    parent: &str,
    claims: &mut BTreeMap<i64, (Option<String>, Vec<String>)>,
) {
    let path = layer_path(graph, table, parent);
    for net in &graph.nets {
        for e in &net.endpoints {
            if e.pin_id < 0 {
                continue;
            }
            let entry = claims.entry(e.pin_id).or_default();
            if entry.0.is_none() {
                entry.0 = endpoint_path(e, table)
                    .as_deref()
                    .and_then(pin_owner_scope)
                    .map(str::to_string);
            }
            entry.1.push(path.clone());
        }
    }
    for sub in &graph.sub_graphs {
        collect_claims(sub, table, &path, claims);
    }
}

/// The scope a pin lives in: its canonical path minus the pin and the
/// component that owns it — `main.MCU513.UC.8` lives in `main.MCU513`.
fn pin_owner_scope(path: &str) -> Option<&str> {
    let without_pin = path.rsplit_once('.')?.0;
    without_pin.rsplit_once('.').map(|(scope, _)| scope)
}

/// Walk one layer: emit its row and its objects, then recurse into its
/// sub-graphs. `parent` is the enclosing layer's canonical path, used only as
/// the fallback when a layer's `bid` names no `InstTable` row. `homes` decides
/// which layer emits each pin's endpoint row (see [`endpoint_homes`]).
fn walk(
    graph: &McVecGraph,
    table: &InstTable,
    parent: &str,
    sources: &mut SourceText,
    items: &mut Vec<Value>,
    homes: &BTreeMap<i64, String>,
    seen_boxes: &mut std::collections::HashSet<i64>,
) {
    let path = layer_path(graph, table, parent);
    let has_row = graph.bid >= 0 && table.get_entry(graph.bid as u32).is_some();

    items.push(json!({
        "class": "layer",
        "key": if has_row { Value::String(format!("D{}", graph.bid)) } else { Value::Null },
        "point": Value::Null,
        "path": path,
        "canon_key": if has_row {
            canon_instance(table, graph.bid as u32)
        } else {
            json!({ "path": path, "def": Value::Null })
        },
        "name": graph.name,
        "style": match graph.layer_style {
            LayerStyle::Block => "block",
            LayerStyle::Device => "device",
        },
        "boxes": graph.boxes.len(),
        "nets": graph.nets.len(),
        "root": graph.is_root,
        "loc": Value::Null,
    }));

    for b in &graph.boxes {
        // §2.4: one instance, one row. A comp-boundary instance (P8-6 inner
        // layer) is a box in its parent layer AND the boundary face inside its
        // own layer — the same InstTable id, hence the same `D<id>` key. The
        // walk is in pre-order, so the first (outermost) row is the identity's
        // home; the deeper copy is the layer-tree face and is not re-published.
        if b.id >= 0 && !seen_boxes.insert(b.id) {
            continue;
        }
        items.push(box_item(b, table, &path, sources));
    }
    for t in &graph.port_trunks {
        items.push(trunk_item(t, table, &path));
    }
    for net in &graph.nets {
        // A net is keyed by its name only when that name is the *source's* — the
        // thing a reader can compare across builds. A name the compiler minted
        // belongs to this build's segmentation, not to the circuit, so the net
        // keeps its member set as its handle instead (§2.4).
        let origin = net_origin(&net.name);
        let key = net_key(&net.name);
        let mut members: Vec<String> = net
            .endpoints
            .iter()
            .filter_map(|e| endpoint_path(e, table))
            .collect();
        members.sort();
        items.push(json!({
            "class": "net",
            "key": key.map(Value::String).unwrap_or(Value::Null),
            "point": Value::Null,
            "path": Value::Null,
            "canon_key": Value::Null,
            "name": net.name,
            "origin": origin.as_str(),
            "kind": net.kind.to_string(),
            "role": format!("{:?}", net.role).to_lowercase(),
            "nid": net.nid,
            "members": members,
            "endpoints": net.endpoints.len(),
            "layer": path,
            "loc": loc_of(net.source_span.as_ref(), sources),
        }));

        for e in &net.endpoints {
            // A foreign scope's claim on this pin yields to the pin's own
            // layer's row (see `endpoint_homes`); its net row still carries
            // the pin in `members`.
            if e.pin_id >= 0 && homes.get(&e.pin_id).is_some_and(|h| *h != path) {
                continue;
            }
            items.push(endpoint_item(e, table, &path, &net.name));
        }
    }

    for sub in &graph.sub_graphs {
        walk(sub, table, &path, sources, items, homes, seen_boxes);
    }
}

/// A layer's canonical path: its `InstTable` row when it has one, else the
/// enclosing path plus its own name.
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

fn box_item(b: &McVecBox, table: &InstTable, layer: &str, sources: &mut SourceText) -> Value {
    let has_row = b.id >= 0 && table.get_entry(b.id as u32).is_some();
    json!({
        "class": "box",
        // A synthesized box (a `PowerLabel` the builder invented, a port
        // terminal) has no instance, so it gets no key rather than a made-up
        // one; `inst_path` is still what a human recognizes it by.
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
        "pins": b.pins.len(),
        "layer": layer,
        "loc": loc_of(b.source_span.as_ref(), sources),
    })
}

fn endpoint_item(e: &EndpointRef, table: &InstTable, layer: &str, net: &str) -> Value {
    let canon = endpoint_canon(e, table);
    json!({
        "class": "endpoint",
        // The identity half. `pin_id` is an `InstTable` row number — an index
        // inside this build, never a key — so it is not published as one.
        "key": e.point.map(|p| Value::String(p.to_string())).unwrap_or(Value::Null),
        "point": e.point.map(|p| p.to_string()),
        "path": endpoint_path(e, table).map(Value::String).unwrap_or(Value::Null),
        "canon_key": canon,
        "pin": e.pin_name,
        "io": format!("{:?}", e.io_type).to_lowercase(),
        "net": net,
        "layer": layer,
        "loc": Value::Null,
    })
}

fn trunk_item(t: &Trunk, table: &InstTable, layer: &str) -> Value {
    let mut lanes: Vec<Value> = t
        .members
        .iter()
        .map(|m| {
            json!({
                "member": m.member,
                "lane": m.lane,
                "left": pin_path(table, m.left_pin),
                "right": pin_path(table, m.right_pin),
            })
        })
        .collect();
    lanes.sort_by_key(|l| l["lane"].as_u64());
    json!({
        "class": "trunk",
        // §2.4: a trunk owns no id. Its name and its lanes are what it is.
        "key": Value::Null,
        "point": Value::Null,
        "path": format!("{layer}.{}", t.name),
        "canon_key": Value::Null,
        "name": t.name,
        "kind": t.kind.label(),
        "op": match t.op {
            Some(crate::semantic::common::ConnOp::Series) => "series",
            Some(crate::semantic::common::ConnOp::Parallel) => "parallel",
            None => "-",
        },
        "dir": t.dir.to_string(),
        "lanes": lanes,
        "loc": Value::Null,
    })
}

/// An endpoint's canonical key: the pin's `InstTable` path plus its owner's def
/// — the same spelling `stage.p2` gives the same pin, which is what makes the
/// two views joinable at all.
///
/// `pin_id < 0` marks the endpoints the viz layer invents (rail-synth, the
/// synthetic `PowerLabel` box), and those get `null`: they are not physical
/// points, so a key would be a fabricated one.
fn endpoint_canon(e: &EndpointRef, table: &InstTable) -> Value {
    if endpoint_path(e, table).is_none() {
        return Value::Null;
    }
    canon_instance(table, e.pin_id as u32)
}

/// The pin's canonical path, when it has an `InstTable` row.
fn endpoint_path(e: &EndpointRef, table: &InstTable) -> Option<String> {
    pin_path(table, e.pin_id)
}

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

/// Render the `stage.vec` text face from the *same* items the JSON face uses
/// (design §5.3 ruling ③). Columns: key, path-or-name, detail, loc — first
/// column always the key, last always `loc` (§5.3 ①).
pub fn render_vec_text(view: &StageView) -> String {
    let rows: Vec<Vec<String>> = view
        .items
        .iter()
        .map(|item| {
            let key = item["key"].as_str().unwrap_or("-").to_string();
            let class = item["class"].as_str().unwrap_or("");
            let second = match class {
                "net" => item["name"].as_str().unwrap_or("-").to_string(),
                "trunk" => item["path"].as_str().unwrap_or("-").to_string(),
                _ => item["path"].as_str().unwrap_or("-").to_string(),
            };
            let third = match class {
                "layer" => format!(
                    "boxes={} nets={}",
                    item["boxes"].as_u64().unwrap_or(0),
                    item["nets"].as_u64().unwrap_or(0)
                ),
                "box" => item["class_name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| item["kind"].as_str().unwrap_or("-"))
                    .to_string(),
                // The origin is in the row on purpose: a keyless net's cell then
                // says *why* it is keyless, on the face a reader reads by
                // default, instead of leaving `-` to look like missing data.
                "net" => format!(
                    "{}/{}/{}",
                    item["kind"].as_str().unwrap_or("-"),
                    item["role"].as_str().unwrap_or("-"),
                    item["origin"].as_str().unwrap_or("-")
                ),
                "endpoint" => item["net"].as_str().unwrap_or("-").to_string(),
                "trunk" => format!(
                    "trunk:{} lanes={}",
                    item["kind"].as_str().unwrap_or("-"),
                    item["lanes"].as_array().map(|l| l.len()).unwrap_or(0)
                ),
                // One shape for both projection rows: a per-layer net count, or
                // one action rule.
                "projection" => match item["before"].as_u64() {
                    Some(b) => format!("nets {b}->{}", item["after"].as_u64().unwrap_or(0)),
                    None => format!("rule {}", item["rule"].as_str().unwrap_or("-")),
                },
                _ => "-".to_string(),
            };
            vec![key, second, third, loc_cell(&item["loc"]).to_string()]
        })
        .collect();

    let mut out = vec![view.header_line(), view.counts_line(StageSeg::Vec)];
    render_table(&rows, &mut out);
    out.join("\n")
}
