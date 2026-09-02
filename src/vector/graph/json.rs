// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! JSON serialization (for frontend `viz/template/interact.js` to parse)
//!
//! Replaces the legacy `legacy::McVecGraph::to_json`, the new version also serializes:
//! - `boxes` (legacy field, for compatibility)
//! - `edges` (legacy field, for compatibility)
//! - **`nets`** ★ NEW multi-endpoint hyperedge
//! - `children` (sub-graphs, recursive)
//!
//! Frontend parsing order: prefer `nets` (new), fallback to `edges` (legacy).
//!
//! ## Note
//! Self-implemented to avoid introducing `serde` dependency (keeping consistent with the original
//! `legacy::write_json` style).

use super::graphdef::McVecGraph;

impl McVecGraph {
    /// Output compact JSON
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out, false, 0);
        out
    }

    /// Output pretty JSON (for debugging)
    pub fn to_json_pretty(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out, true, 0);
        out
    }

    fn write_json(&self, out: &mut String, pretty: bool, depth: usize) {
        let nl = if pretty { "\n" } else { "" };
        let i0 = if pretty {
            "  ".repeat(depth)
        } else {
            String::new()
        };
        let i1 = if pretty {
            "  ".repeat(depth + 1)
        } else {
            String::new()
        };
        let i2 = if pretty {
            "  ".repeat(depth + 2)
        } else {
            String::new()
        };
        let s = if pretty { ", " } else { "," };

        out.push_str(&format!("{{{nl}"));
        out.push_str(&format!(
            "{i1}\"bid\": {}{s}\"name\": \"{}\"{s}{nl}",
            self.bid,
            json_escape(&self.name)
        ));

        // ── boxes ─────────────────────────────────────────────────────────
        out.push_str(&format!("{i1}\"boxes\": ["));
        if !self.boxes.is_empty() {
            out.push_str(nl);
        }
        for (i, b) in self.boxes.iter().enumerate() {
            out.push_str(&format!(
                "{i2}{{\"id\": {}{s}\"name\": \"{}\"{s}\"class\": \"{}\"{s}\"kind\": \"{}\"{s}\"pins\": {}{s}",
                b.id,
                json_escape(&b.name),
                json_escape(&b.class_name),
                b.kind,
                b.pin_count
            ));
            out.push_str(&format!(
                "\"io\": {{\"in\": {}{s}\"out\": {}{s}\"pwr\": {}{s}\"other\": {}}}{s}",
                b.io_summary.inputs, b.io_summary.outputs, b.io_summary.power, b.io_summary.other
            ));
            out.push_str(&format!(
                "\"x\": {:.1}{s}\"y\": {:.1}{s}\"w\": {:.1}{s}\"h\": {:.1}}}",
                b.x, b.y, b.w, b.h
            ));
            if i + 1 < self.boxes.len() {
                out.push(',');
            }
            out.push_str(nl);
        }
        out.push_str(&format!("{i1}]{s}{nl}"));

        // ── edges (legacy binary model) ────────────────────────────────────────────
        out.push_str(&format!("{i1}\"edges\": ["));
        if !self.edges.is_empty() {
            out.push_str(nl);
        }
        for (i, e) in self.edges.iter().enumerate() {
            out.push_str(&format!(
                "{i2}{{\"src\": {}{s}\"dst\": {}{s}\"type\": \"{}\"{s}\"name\": \"{}\"{s}",
                e.src_box,
                e.dst_box,
                e.edge_type,
                json_escape(&e.net_name)
            ));
            out.push_str("\"wires\": [");
            for (j, w) in e.wires.iter().enumerate() {
                out.push_str(&format!(
                    "{{\"sp\":\"{}\"{s}\"sn\":\"{}\"{s}\"dp\":\"{}\"{s}\"dn\":\"{}\"}}",
                    w.src_pin_id,
                    json_escape(&w.src_pin_name),
                    w.dst_pin_id,
                    json_escape(&w.dst_pin_name)
                ));
                if j + 1 < e.wires.len() {
                    out.push(',');
                }
            }
            out.push_str("]}");
            if i + 1 < self.edges.len() {
                out.push(',');
            }
            out.push_str(nl);
        }
        out.push_str(&format!("{i1}]{s}{nl}"));

        // ── ★ NEW: nets (multi-endpoint hyperedge) ────────────────────────────────
        out.push_str(&format!("{i1}\"nets\": ["));
        if !self.nets.is_empty() {
            out.push_str(nl);
        }
        for (i, n) in self.nets.iter().enumerate() {
            out.push_str(&format!(
                "{i2}{{\"nid\": {}{s}\"name\": \"{}\"{s}\"kind\": \"{}\"",
                n.nid,
                json_escape(&n.name),
                n.kind
            ));
            // ★ §8.9.6: structured port group context (group name / member / kind)
            // carried through the whole pipeline; missing for free nets.
            if let Some(ref pg) = n.trunk {
                out.push_str(&format!(
                    "{s}\"trunk\": {{\"name\": {}{s}\"member\": {}{s}\"kind\": \"{}\"}}",
                    json_opt_str(&pg.name),
                    json_opt_str(&pg.member),
                    pg.kind.label()
                ));
            }
            // ★ §8.9.4: fine net -> coarse trunk back-reference
            if let Some(d) = n.trunk_ref {
                out.push_str(&format!(
                    "{s}\"trunk_ref\": {{\"id\": {}{s}\"lane\": {}}}",
                    d.id, d.lane
                ));
            }
            // ★ §8.9.2: topology shape (op / anchor / order), same semantics as
            // the coarse port_trunks so the frontend can draw fine nets alike.
            if let Some(shape) = &n.shape {
                let op = match shape.op {
                    Some(crate::semantic::common::ConnOp::Series) => "\"-\"",
                    Some(crate::semantic::common::ConnOp::Parallel) => "\"+\"",
                    None => "null",
                };
                let dir = match shape.dir {
                    crate::semantic::common::ConnDir::LtoR => "\"->\"",
                    crate::semantic::common::ConnDir::RtoL => "\"<-\"",
                    crate::semantic::common::ConnDir::Undirected => "null",
                };
                let order = shape
                    .order
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!(
                    "{s}\"shape\": {{\"op\": {op}{s}\"dir\": {dir}{s}\"anchor\": {}{s}\"order\": [{order}]}}",
                    json_opt_i64(&shape.anchor)
                ));
            }
            out.push_str(&format!("{s}\"endpoints\": ["));
            for (j, ep) in n.endpoints.iter().enumerate() {
                out.push_str(&format!(
                    "{{\"box\":{}{s}\"pin\":{}{s}\"name\":\"{}\"}}",
                    ep.box_id,
                    ep.pin_id,
                    json_escape(&ep.pin_name)
                ));
                if j + 1 < n.endpoints.len() {
                    out.push(',');
                }
            }
            out.push(']');
            // route field: output segments+junctions when routed, null when not routed
            if let Some(route) = &n.route {
                out.push_str(&format!("{s}\"route\":{{"));
                out.push_str("\"segments\":[");
                for (j, seg) in route.segments.iter().enumerate() {
                    out.push_str(&format!(
                        "{{\"from\":[{:.1}{s}{:.1}]{s}\"to\":[{:.1}{s}{:.1}]}}",
                        seg.from.x, seg.from.y, seg.to.x, seg.to.y
                    ));
                    if j + 1 < route.segments.len() {
                        out.push(',');
                    }
                }
                out.push(']');
                if !route.junctions.is_empty() {
                    out.push_str(&format!("{s}\"junctions\":["));
                    for (j, p) in route.junctions.iter().enumerate() {
                        out.push_str(&format!("[{:.1}{s}{:.1}]", p.x, p.y));
                        if j + 1 < route.junctions.len() {
                            out.push(',');
                        }
                    }
                    out.push(']');
                }
                out.push('}');
            } else {
                out.push_str(&format!("{s}\"route\":null"));
            }
            out.push('}');
            if i + 1 < self.nets.len() {
                out.push(',');
            }
            out.push_str(nl);
        }
        out.push_str(&format!("{i1}]{s}{nl}"));

        // ── ★ §8.9.4: port_trunks (coarse bus/interface trunks of this layer) ──────
        out.push_str(&format!("{i1}\"port_trunks\": ["));
        if !self.port_trunks.is_empty() {
            out.push_str(nl);
        }
        for (i, d) in self.port_trunks.iter().enumerate() {
            let dir = match d.dir {
                crate::semantic::common::ConnDir::LtoR => "\"->\"",
                crate::semantic::common::ConnDir::RtoL => "\"<-\"",
                crate::semantic::common::ConnDir::Undirected => "null",
            };
            let order = d
                .order
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!(
                "{i2}{{\"id\": {}{s}\"name\": \"{}\"{s}\"kind\": \"{}\"{s}\"op\": {}{s}\"dir\": {}{s}\"anchor\": {}{s}\"order\": [{}]{s}",
                d.id,
                json_escape(&d.name),
                d.kind,
                match d.op {
                    Some(crate::semantic::common::ConnOp::Series) => "\"-\"",
                    Some(crate::semantic::common::ConnOp::Parallel) => "\"+\"",
                    None => "null",
                },
                dir,
                json_opt_i64(&d.anchor),
                order,
            ));
            // left / right trunk ends
            out.push_str(&format!(
                "\"left\": {{\"box_id\": {}{s}\"instance\": {}{s}\"port\": \"{}\"{s}\"iface\": {}{s}\"io\": {}{s}\"path\": \"{}\"}}{s}",
                json_opt_i64(&d.left.box_id),
                json_opt_str(&d.left.instance),
                json_escape(&d.left.port),
                json_opt_str(&d.left.iface_class),
                d.left
                    .io
                    .as_ref()
                    .map(|t| format!("\"{t:?}\""))
                    .unwrap_or_else(|| "null".to_string()),
                path_display(&d.left.path),
            ));
            out.push_str(&format!(
                "\"right\": {{\"box_id\": {}{s}\"instance\": {}{s}\"port\": \"{}\"{s}\"iface\": {}{s}\"io\": {}{s}\"path\": \"{}\"}}{s}",
                json_opt_i64(&d.right.box_id),
                json_opt_str(&d.right.instance),
                json_escape(&d.right.port),
                json_opt_str(&d.right.iface_class),
                d.right.io.as_ref().map(|t| format!("\"{t:?}\"")).unwrap_or_else(|| "null".to_string()),
                path_display(&d.right.path),
            ));
            // fine layer: per-member pin2pin lanes
            out.push_str("\"members\": [");
            for (j, m) in d.members.iter().enumerate() {
                out.push_str(&format!(
                    "{{\"member\": \"{}\"{s}\"lane\": {}{s}\"dir\": {}{s}\"left_pin\": {}{s}\"right_pin\": {}{s}\"path\": \"{}\"{s}\"alias\": {}}}",
                    json_escape(&m.member),
                    m.lane,
                    match m.dir {
                        crate::semantic::common::ConnDir::LtoR => "\"->\"",
                        crate::semantic::common::ConnDir::RtoL => "\"<-\"",
                        crate::semantic::common::ConnDir::Undirected => "null",
                    },
                    m.left_pin,
                    m.right_pin,
                    path_display(&m.path),
                    json_opt_str(&m.alias),
                ));
                if j + 1 < d.members.len() {
                    out.push(',');
                }
            }
            out.push_str("]}");
            if i + 1 < self.port_trunks.len() {
                out.push(',');
            }
            out.push_str(nl);
        }
        out.push_str(&format!("{i1}]{s}{nl}"));

        // ── children (sub-graphs, recursive) ──────────────────────────────────────────
        out.push_str(&format!("{i1}\"children\": ["));
        if !self.sub_graphs.is_empty() {
            out.push_str(nl);
        }
        for (i, sg) in self.sub_graphs.iter().enumerate() {
            out.push_str(&i2);
            sg.write_json(out, pretty, depth + 2);
            if i + 1 < self.sub_graphs.len() {
                out.push(',');
            }
            out.push_str(nl);
        }
        out.push_str(&format!("{i1}]{nl}"));
        out.push_str(&format!("{i0}}}"));
    }
}

/// JSON string escape (exposed because builder debug logs also use it)
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// Serialize an `Option<String>` as a JSON string literal or `null`
fn json_opt_str(s: &Option<String>) -> String {
    match s {
        Some(v) => format!("\"{}\"", json_escape(v)),
        None => "null".to_string(),
    }
}

/// Serialize an `Option<i64>` as a JSON number literal or `null`
fn json_opt_i64(v: &Option<i64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

/// Serialize a structured [`PathSegment`] chain as a dotted path string
/// (uC.I2C0.SCL), mirroring its `Display` form.
fn path_display(path: &[crate::vector::model::trunk::PathSegment]) -> String {
    path.iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::common::{ConnDir, ConnOp};
    use crate::vector::graph::{EndpointRef, NetKind, NetRole, VizNet};
    use crate::vector::model::netshape::NetShape;

    /// Serialize a single-net graph whose net shape carries `dir`, returning the JSON.
    fn shape_json(dir: ConnDir) -> String {
        let mut graph = McVecGraph::new(1, "t".into());
        let mut net = VizNet::new(
            1,
            "N1".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![EndpointRef::new(9, 9, "B"), EndpointRef::new(2, 2, "A")],
        );
        net.shape = Some(NetShape {
            dir,
            op: Some(ConnOp::Series),
            order: vec![9, 2],
            ..Default::default()
        });
        graph.nets.push(net);
        graph.to_json()
    }

    /// §8.9.2 fine-net shape now serializes `dir`, aligned with coarse
    /// port_trunks and member lanes (which already emit it).
    #[test]
    fn fine_net_shape_serializes_dir() {
        let rtl = shape_json(ConnDir::RtoL);
        assert!(
            rtl.contains("\"dir\": \"<-\""),
            "RtoL shape must serialize its dir: {rtl}"
        );
        let ltr = shape_json(ConnDir::LtoR);
        assert!(
            ltr.contains("\"dir\": \"->\""),
            "LtoR shape must serialize its dir: {ltr}"
        );
        let und = shape_json(ConnDir::Undirected);
        assert!(
            und.contains("\"dir\": null"),
            "Undirected shape dir is null: {und}"
        );
    }
}
