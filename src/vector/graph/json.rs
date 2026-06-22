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

use super::graph_def::McVecGraph;

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
                "{i2}{{\"nid\": {}{s}\"name\": \"{}\"{s}\"kind\": \"{}\"{s}",
                n.nid,
                json_escape(&n.name),
                n.kind
            ));
            out.push_str("\"endpoints\": [");
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
