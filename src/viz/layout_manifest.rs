// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The machine-readable layout manifest: positions, sizes, orientations, pin
//! placements, wire geometry and layer containment for every rendered layer —
//! the face an agent reads to locate problems, so it never has to parse the
//! SVG.
//!
//! Identity follows the stage keys: a layer is its `bid` plus its instance
//! path (`main.DCDC`), a box its `inst_path`, a pin its `(inst_path, number)`
//! pair, a net its per-layer name plus the endpoints it joins. Geometry is
//! layer coordinates, same numbers the SVG draws — one coordinate space per
//! layer, never a global one.

use serde_json::{json, Value};

use crate::vector::graph::{EntrySide, LayerStyle, McVecGraph};
use crate::viz::api::RenderedLayer;
use crate::viz::layout::equipotential_tree::build_all_trees;

/// Component parameters keyed by hierarchical instance path (`main.R1`):
/// value (first positional parameter as written), partno, package — the same
/// resolution chain the KiCad netlist export uses, so the drawing and the
/// netlist cannot disagree about what a part is.
pub fn collect_params(
    tree: &crate::McModuleInst,
    arena: &crate::instant::arena::NodeArena,
    store: &crate::instant::inststore::InstanceStore,
) -> std::collections::BTreeMap<String, Value> {
    use crate::instant::inststore::TreeView;
    use crate::instant::mc_comp::McComponentInst;
    let view = TreeView::new(arena, store);
    let mut out: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    fn attr_text(c: &McComponentInst, key: &str) -> Option<String> {
        use crate::semantic::component::mc_attr::attr_values_text;
        for attr in &c.resolved_attrs {
            if attr.id.segments.len() == 1 && attr.id.segments[0].to_string() == key {
                if let Some(t) = attr_values_text(attr.values.iter()) {
                    return Some(t);
                }
            }
        }
        None
    }
    fn walk(
        m: &crate::McModuleInst,
        view: &TreeView,
        path: &str,
        out: &mut std::collections::BTreeMap<String, Value>,
    ) {
        for c in view.components(m) {
            if c.name.starts_with("__") {
                continue;
            }
            let value = c
                .raw_params
                .iter()
                .map(|p| p.to_string())
                .find(|t| !t.is_empty() && t != "_" && t != "NC")
                .or_else(|| attr_text(c, "partno"))
                .unwrap_or_else(|| c.def.name.to_string());
            out.insert(
                format!("{path}.{}", c.name),
                json!({
                    "value": value,
                    "partno": attr_text(c, "partno"),
                    "package": attr_text(c, "package"),
                    "dnp": c.not_fitted(),
                }),
            );
        }
        for sub in view.sub_modules(m) {
            walk(sub, view, &format!("{path}.{}", sub.name), out);
        }
    }
    walk(tree, &view, &tree.name.clone(), &mut out);
    out
}

/// Build the manifest for a whole rendered hierarchy (pre-order, root first).
pub fn build_manifest(
    layers: &[RenderedLayer],
    params: &std::collections::BTreeMap<String, Value>,
    top: &str,
) -> Value {
    // bid -> instance path, built from the parent chain.
    let mut paths: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for l in layers {
        let parent_path = l
            .parent
            .and_then(|p| paths.get(&p).cloned())
            .unwrap_or_default();
        let path = if parent_path.is_empty() {
            l.graph.name.clone()
        } else {
            format!("{parent_path}.{}", l.graph.name)
        };
        paths.insert(l.graph.bid, path);
    }

    let mut layer_rows: Vec<Value> = Vec::new();
    let mut box_rows: Vec<Value> = Vec::new();
    let mut net_rows: Vec<Value> = Vec::new();

    for l in layers {
        let graph: &McVecGraph = &l.graph;
        let path = paths.get(&graph.bid).cloned().unwrap_or_default();
        layer_rows.push(json!({
            "bid": graph.bid,
            "path": path,
            "name": graph.name,
            "parent": l.parent,
            "style": if graph.layer_style == LayerStyle::Device { "device" } else { "block" },
            "canvas": [l.canvas.0, l.canvas.1],
        }));
        for b in &graph.boxes {
            let pins: Vec<Value> = b
                .pins
                .iter()
                .map(|p| {
                    let entry = b.find_entry(p.id);
                    let (px, py) = match entry.map(|e| e.side) {
                        Some(EntrySide::Top) => (b.x + b.w * entry.unwrap().offset, b.y),
                        Some(EntrySide::Bottom) => (
                            b.x + b.w * entry.unwrap().offset,
                            b.y + b.h,
                        ),
                        Some(EntrySide::Left) => (b.x, b.y + b.h * entry.unwrap().offset),
                        Some(EntrySide::Right) => (b.x + b.w, b.y + b.h * entry.unwrap().offset),
                        None => (b.x + b.w / 2.0, b.y + b.h / 2.0),
                    };
                    let src = p.src_span.as_ref();
                    json!({
                        "num": p.pin_id,
                        "name": p.description,
                        "io": format!("{:?}", p.io).to_lowercase(),
                        "side": entry.map(|e| side_str(e.side)),
                        "offset": entry.map(|e| e.offset),
                        "at": [px, py],
                        "point": p.point.map(|pt| pt.to_string()),
                        "src": src.map(|sp| json!({"uri": sp.uri, "offset": sp.offset})),
                    })
                })
                .collect();
            box_rows.push(json!({
                "layer": graph.bid,
                "layer_path": path,
                "path": crate::viz::render::label_render::visible_path(b),
                "name": crate::viz::render::label_render::visible_name(b),
                "class": b.class_name,
                "kind": format!("{:?}", b.kind).to_lowercase(),
                "symbol": format!("{:?}", b.symbol).to_lowercase(),
                "at": [b.x, b.y],
                "size": [b.w, b.h],
                "orientation": orientation_of(b),
                "dnp": b.not_fitted,
                "params": params.get(&b.inst_path).cloned().unwrap_or(Value::Null),
                "pins": pins,
            }));
        }
        // Wire geometry: block layers route through `net.route`, device layers
        // draw through the equipotential trees — publish both so every layer
        // style exposes the same segment face.
        let tree_segments: std::collections::HashMap<String, Vec<(f64, f64, f64, f64)>> =
            if graph.layer_style == LayerStyle::Device {
                build_all_trees(graph)
                    .iter()
                    .map(|t| {
                        (
                            t.net_name.clone(),
                            t.segments
                                .iter()
                                .map(|s| (s.x1, s.y1, s.x2, s.y2))
                                .collect(),
                        )
                    })
                    .collect()
            } else {
                std::collections::HashMap::new()
            };
        for net in &graph.nets {
            let mut segs: Vec<Value> = Vec::new();
            if let Some(route) = &net.route {
                for s in &route.segments {
                    segs.push(json!([[s.from.x, s.from.y], [s.to.x, s.to.y]]));
                }
            }
            if let Some(s) = tree_segments.get(net.name.as_str()) {
                for (x1, y1, x2, y2) in s {
                    segs.push(json!([[x1, y1], [x2, y2]]));
                }
            }
            let ends: Vec<String> = net
                .endpoints
                .iter()
                .filter_map(|e| {
                    let b = graph.boxes.iter().find(|b| b.id == e.box_id)?;
                    let pin = b
                        .pins
                        .iter()
                        .find(|p| p.id == e.pin_id)
                        .map(|p| p.pin_id.clone())
                        .unwrap_or_else(|| e.pin_name.clone());
                    Some(format!("{}.{}", b.inst_path, pin))
                })
                .collect();
            net_rows.push(json!({
                "layer": graph.bid,
                "layer_path": path,
                "name": net.name,
                "kind": format!("{:?}", net.kind).to_lowercase(),
                "role": format!("{:?}", net.role).to_lowercase(),
                "endpoints": ends,
                "segments": segs,
            }));
        }
    }

    json!({
        "schema": "viz.layout.1",
        "top": top,
        "layers": layer_rows,
        "boxes": box_rows,
        "nets": net_rows,
    })
}

fn side_str(s: EntrySide) -> &'static str {
    match s {
        EntrySide::Top => "top",
        EntrySide::Bottom => "bottom",
        EntrySide::Left => "left",
        EntrySide::Right => "right",
    }
}

/// Two-pin bodies draw along the axis their pins sit on; everything else is
/// orientation-free.
fn orientation_of(b: &crate::vector::graph::McVecBox) -> &'static str {
    if !b.is_two_pin_passive() {
        return "none";
    }
    let has = |side: EntrySide| {
        b.pins
            .iter()
            .any(|p| b.find_entry(p.id).map(|e| e.side) == Some(side))
    };
    let horizontal = has(EntrySide::Left) && has(EntrySide::Right);
    let vertical = has(EntrySide::Top) && has(EntrySide::Bottom);
    if horizontal {
        "horizontal"
    } else if vertical {
        "vertical"
    } else {
        "none"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{BoxPin, IoSummary};
    use crate::vector::graph::{BoxKind, EntryPoint, McVecBox, NetKind, Symbol, VizNet, NetRole};

    #[test]
    fn manifest_carries_geometry_and_containment() {
        let mut root = McVecGraph::new(1, "main".into());
        root.is_root = true;
        root.layer_style = LayerStyle::Block;
        let mut r = McVecBox::new_v2(
            10,
            "R1".into(),
            "RES".into(),
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1".into()),
            None,
            2,
            IoSummary::new(),
            "main.R1".into(),
            Vec::new(),
        );
        r.x = 100.0;
        r.y = 100.0;
        r.w = 40.0;
        r.h = 20.0;
        r.pins.push(BoxPin {
            id: 100,
            pin_id: "1".into(),
            description: String::new(),
            io: crate::vector::graph::netdef::IoDirection::Passive,
            port_dir: crate::vector::graph::PortDir::None,
            src_span: None,
            point: None,
        });
        r.entry_points.push(EntryPoint {
            pin_id: 100,
            pin_name: "V3V3".into(),
            side: EntrySide::Left,
            offset: 0.5,
        });
        root.boxes.push(r);
        let mut net = VizNet::new(
            1,
            "V3V3".into(),
            NetKind::Power,
            NetRole::Signal,
            vec![crate::vector::graph::EndpointRef::new(10, 100, "V3V3")],
        );
        let mut route = crate::vector::graph::Route::new();
        route.segments.push(crate::vector::graph::Segment {
            from: crate::vector::graph::Point::new(100.0, 110.0),
            to: crate::vector::graph::Point::new(140.0, 110.0),
        });
        net.route = Some(route);
        root.nets.push(net);

        let layers = vec![RenderedLayer {
            graph: root,
            parent: None,
            canvas: (500.0, 300.0),
            audited: true,
        }];
        let params = std::collections::BTreeMap::new();
        let m = build_manifest(&layers, &params, "main");
        assert_eq!(m["schema"], "viz.layout.1");
        assert_eq!(m["layers"][0]["path"], "main");
        let b = &m["boxes"][0];
        assert_eq!(b["path"], "main.R1");
        assert_eq!(b["at"], json!([100.0, 100.0]));
        assert_eq!(b["size"], json!([40.0, 20.0]));
        assert_eq!(b["pins"][0]["side"], "left");
        assert_eq!(b["pins"][0]["at"], json!([100.0, 110.0]));
        let n = &m["nets"][0];
        assert_eq!(n["name"], "V3V3");
        assert_eq!(n["endpoints"][0], "main.R1.1");
        assert_eq!(n["segments"][0], json!([[100.0, 110.0], [140.0, 110.0]]));
    }
}
