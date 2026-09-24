// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `project-model` projection — the project model in one read.
//!
//! The third carried canonical `view-name` word (`schema/projection.cddl`
//! §2.1): the hierarchical module/instance tree — every circuit node with its
//! definition site, declared parameters, ports and their net binding, and its
//! children — the load MCP `read_project` / CLI `show project` hand to a
//! consumer that would otherwise reassemble the model from `show` pieces.
//!
//! The walk is the arena + instance-store [`TreeView`] over the *whole* world
//! (not the flat table): a node per Module / Device / Vector arena node, the
//! module's io ports as the node's `ports` rows (a port is a face of its
//! module, not a sub-circuit node), and children in the arena's deterministic
//! grouped order (vectors, devices, sub-modules). Nothing silently vanishes:
//! a vector grouping node reads as a node with an empty definition site (it
//! has none of its own), and every parameter that carries a value appears.
//!
//! A readout, not a verdict (law C): the item count never flips an exit code.

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

use super::read::Loaded;
use super::StageView;
use crate::export::netlist::{island_nets, PointNaming};
use crate::instant::arena::NodeKind;
use crate::instant::identity::NodeId;
use crate::instant::inststore::TreeView;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_mod::McModuleInst;

/// The canonical word this face publishes (CDDL `view-name`).
pub const PROJECT_MODEL_VIEW: &str = "project-model";

/// CDDL `typed = tstr / int / float / bool` — the value spellings the engine
/// can actually produce today. `bool` stays a CDDL-only alternative (the eval
/// engine has no bool value form); a quantity carries the author's notation
/// as a string, so `10kΩ` reads back as `10kΩ`, not as a bare number.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum TypedValue {
    Int(i64),
    Float(f64),
    Str(String),
}

/// CDDL `def-site = { kind, name, uri, span }` — where the node's definition
/// lives. `span` is the definition's byte-range length (the same spelling the
/// diagnostics view uses). A vector grouping node has no definition site of
/// its own: its `uri` is empty and its `span` 0, on purpose and not as a gap.
#[derive(Debug, Clone, Serialize)]
pub struct DefSite {
    pub kind: String,
    pub name: String,
    pub uri: String,
    pub span: u32,
}

/// CDDL `ports: [* { name, dir, ? net }]` — one row per port/pin. `net` is
/// the island name from the same builder the `netlist` view and the export
/// face use, so the three faces cannot disagree about where a pin lands.
#[derive(Debug, Clone, Serialize)]
pub struct PortItem {
    pub name: String,
    pub dir: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<String>,
}

/// CDDL `ratings-bound = { ?low, ?high }` — one entry of the node's class
/// `ratings` clause (ratings-param-constraint-design.md §8 ③: the def-side
/// bounds ride the model). The sides carry the author's notation verbatim
/// (`2500mV` reads back as `2500mV`); normalizing into base units is the
/// gate's job, not the wire's. A side the clause does not state is absent,
/// never null.
#[derive(Debug, Clone, Serialize)]
pub struct RatingsBound {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
}

/// CDDL `circuit-node` — one node of the project model. `children` is absent
/// on a leaf; the CDDL's `?` marks it optional (`slice.depth` controls — v1
/// always walks to full depth). So is `ratings`: present on a component node
/// whose class declares the clause, absent — not an empty map — otherwise.
#[derive(Debug, Clone, Serialize)]
pub struct CircuitNode {
    pub id: String,
    pub def: DefSite,
    pub params: BTreeMap<String, TypedValue>,
    pub ports: Vec<PortItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratings: Option<BTreeMap<String, RatingsBound>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<CircuitNode>>,
}

/// The island map inverted: point label → island name, keyed by exactly the
/// strings the `netlist` view publishes as `points`. One builder, one
/// spelling of connectivity.
fn island_index(table: &crate::InstTable) -> HashMap<String, String> {
    island_nets(table, PointNaming::Local)
        .into_iter()
        .flat_map(|(name, points)| {
            points.into_iter().map(move |p| (p, name.clone()))
        })
        .collect()
}

/// The class's `ratings` clause, read once per node off its definition. An
/// empty clause is no member at all: the map carries bounds, not absences.
/// (The reader is `mc_ratings::read_ratings`; sides the clause wrote but the
/// reader could not parse are the gate's E5360 business, not the model's.)
fn ratings_of(
    def: &crate::semantic::component::McComponent,
) -> Option<BTreeMap<String, RatingsBound>> {
    let entries = crate::semantic::component::mc_ratings::read_ratings(&def.attrs);
    if entries.is_empty() {
        return None;
    }
    use crate::semantic::component::mc_ratings::RatingsSide;
    let mut out = BTreeMap::new();
    for e in entries {
        let param = e.param.clone();
        out.insert(
            param,
            RatingsBound {
                low: e.side_text(RatingsSide::Low),
                high: e.side_text(RatingsSide::High),
            },
        );
    }
    Some(out)
}

/// The declared parameters that carry a value — bound or declaration default
/// (the same fallback the eval path takes). A formal with neither contributes
/// no entry: the map carries values, not absences.
fn param_map(bindings: &crate::McParamBindings) -> BTreeMap<String, TypedValue> {
    bindings
        .to_params_for_eval()
        .into_iter()
        .filter_map(|(name, text)| {
            let t = text.trim();
            if t.is_empty() {
                return None;
            }
            let typed = match crate::eval::Value::from_text(t) {
                crate::eval::Value::Int(i) => TypedValue::Int(i),
                crate::eval::Value::Float(f) => TypedValue::Float(f),
                other => TypedValue::Str(other.text()),
            };
            Some((name.to_string(), typed))
        })
        .collect()
}

fn module_ports(inst: &McModuleInst, islands: &HashMap<String, String>) -> Vec<PortItem> {
    inst.ports
        .iter()
        .map(|p| PortItem {
            name: p.name.clone(),
            dir: crate::rpc::handlers::iotype_str(&p.iotype),
            net: islands.get(p.net_point.path.as_str()).cloned(),
        })
        .collect()
}

fn component_ports(inst: &McComponentInst, islands: &HashMap<String, String>) -> Vec<PortItem> {
    inst.sorted_pin_ids()
        .into_iter()
        .filter_map(|pid| {
            let point = inst.pins.get(pid)?;
            let io = inst.def.pins.get_pin_io(pid);
            Some(PortItem {
                name: pid.clone(),
                dir: crate::rpc::handlers::iotype_str(&io.unwrap_or(crate::IOType::None)),
                net: islands.get(point.path.as_str()).cloned(),
            })
        })
        .collect()
}

/// A module node's def-site / params / ports, read off its instance.
fn module_fields(inst: &McModuleInst, islands: &HashMap<String, String>) -> (DefSite, BTreeMap<String, TypedValue>, Vec<PortItem>) {
    (
        DefSite {
            kind: "module".to_string(),
            name: inst.def.name.to_string(),
            uri: inst.def_uri.as_str().to_string(),
            span: inst.def.span.len() as u32,
        },
        param_map(&inst.params),
        module_ports(inst, islands),
    )
}

/// The children of `id`, each threaded under `path` by the identity ledger's
/// `{parent}.{name}` rule; a `Port` child contributes nothing (it is a face
/// of its module — the `ports` member).
fn child_nodes(
    view: &TreeView,
    id: NodeId,
    path: &str,
    islands: &HashMap<String, String>,
) -> Vec<CircuitNode> {
    view.children(id)
        .unwrap_or_default()
        .iter()
        .filter_map(|cid| {
            let child = view.node(*cid)?;
            let child_path = format!("{path}.{}", child.name);
            build_node(view, *cid, &child_path, islands)
        })
        .collect()
}

/// One walk step below the root: `path` is the canonical identity path
/// threaded from the top. `Port` arena nodes return `None` — they are faces
/// of their module (the `ports` member), not sub-circuit nodes.
fn build_node(
    view: &TreeView,
    id: NodeId,
    path: &str,
    islands: &HashMap<String, String>,
) -> Option<CircuitNode> {
    let node = view.node(id)?;
    let (def, params, ports, ratings) = match node.kind {
        NodeKind::Module => {
            let inst = view.store().module(id)?;
            let (def, params, ports) = module_fields(inst, islands);
            // `ratings` is a component-head clause; a module carries none.
            (def, params, ports, None)
        }
        NodeKind::Device => {
            let inst = view.store().component(id)?;
            (
                DefSite {
                    kind: "component".to_string(),
                    name: inst.def.name.to_string(),
                    uri: inst.def.uri.as_str().to_string(),
                    span: inst.def.span.len() as u32,
                },
                param_map(&inst.params),
                component_ports(inst, islands),
                ratings_of(&inst.def),
            )
        }
        // A grouping header (`c[1:2]`): real identity, no definition of its
        // own — the empty def-site says so instead of dropping the node.
        NodeKind::Vector => (
            DefSite {
                kind: "vector".to_string(),
                name: node.name.clone(),
                uri: String::new(),
                span: 0,
            },
            BTreeMap::new(),
            Vec::new(),
            None,
        ),
        NodeKind::Port => return None,
    };

    let children = child_nodes(view, id, path, islands);
    Some(CircuitNode {
        id: path.to_string(),
        def,
        params,
        ports,
        ratings,
        children: (!children.is_empty()).then_some(children),
    })
}

/// The items, one per circuit node, root first, depth-first in the arena's
/// deterministic child order. The root module's instance content is the tree
/// itself — the store holds only the descendants (the construction-time
/// builder inserts children; the root stays in the tree hand).
pub fn project_model_items(loaded: &Loaded) -> Vec<Value> {
    let view = TreeView::new(&loaded.arena, &loaded.store);
    let islands = island_index(&loaded.table);
    let root_id = view.root();
    let Some(node) = view.node(root_id) else {
        return Vec::new();
    };
    if !matches!(node.kind, NodeKind::Module) {
        return Vec::new();
    }
    let (def, params, ports) = module_fields(&loaded.tree, &islands);
    let children = child_nodes(&view, root_id, &loaded.top, &islands);
    let root = CircuitNode {
        id: loaded.top.clone(),
        def,
        params,
        ports,
        // The top module is a module, and `ratings` is a component clause.
        ratings: None,
        children: (!children.is_empty()).then_some(children),
    };
    vec![serde_json::to_value(&root).unwrap_or(Value::Null)]
}

/// Per-face counts: the nodes and their ports, every word printed even at
/// zero — an absent line reads as "not implemented" rather than "none".
pub fn project_model_counts(items: &[Value]) -> Value {
    let mut nodes = 0usize;
    let mut ports = 0usize;
    let mut stack: Vec<&Value> = items.iter().collect();
    while let Some(item) = stack.pop() {
        nodes += 1;
        ports += item["ports"].as_array().map(|a| a.len()).unwrap_or(0);
        if let Some(children) = item["children"].as_array() {
            stack.extend(children);
        }
    }
    serde_json::json!({
        "nodes": nodes,
        "ports": ports,
    })
}

/// Assemble the projection. Goes through [`StageView::with_view`] — this view
/// publishes its own vocabulary and its own count words, not a pipeline
/// segment's.
pub fn project_model_view(loaded: &Loaded) -> StageView {
    let items = project_model_items(loaded);
    let counts = project_model_counts(&items);
    StageView::with_view(PROJECT_MODEL_VIEW, &loaded.top, items, counts)
}

/// The text face, rendered from the **same** items the envelope carries:
/// header, counts, then the tree — one line per node and per port row.
pub fn render_project_model_text(view: &StageView) -> String {
    let mut lines = vec![view.header_line()];
    let words: Vec<String> = view
        .counts
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k} {}", v.as_u64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    lines.push(format!("# {}", words.join("  ")));

    fn render_node(node: &Value, depth: usize, out: &mut Vec<String>) {
        let pad = "  ".repeat(depth);
        let def = &node["def"];
        let params = node["params"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| format!("{k}={}", spell(v)))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let mut line = format!(
            "{pad}{} {} {}",
            node["id"].as_str().unwrap_or("-"),
            def["kind"].as_str().unwrap_or("-"),
            def["name"].as_str().unwrap_or("-"),
        );
        if !params.is_empty() {
            line.push(' ');
            line.push_str(&params);
        }
        out.push(line);
        let port_pad = "  ".repeat(depth + 1);
        for p in node["ports"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            let net = p["net"].as_str().unwrap_or("-");
            out.push(format!(
                "{port_pad}{} {} -> {}",
                p["name"].as_str().unwrap_or("-"),
                p["dir"].as_str().unwrap_or("-"),
                net,
            ));
        }
        for child in node["children"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            render_node(child, depth + 1, out);
        }
    }

    let mut out: Vec<String> = Vec::new();
    for item in &view.items {
        render_node(item, 0, &mut out);
    }
    lines.extend(out);
    lines.join("\n")
}

/// The text spelling of one parameter value (the JSON face spells it typed;
/// the text face shows the value itself).
fn spell(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}
