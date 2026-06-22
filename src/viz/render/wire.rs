// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Edge / net SVG output
//!
//! Input: a `VizNet` (hyperedge model) that has already been routed
//! Output: `<g class="edge ..."><path .../>...</g>` SVG fragment
//!
//! ## Boundary with [`crate::viz::route`]
//! - **route**: compute the geometric path (returns `Vec<(x,y)>` or `Route`)
//! - **render**: turn the path into an SVG string
//!
//! **How** the path is computed is route's job; this file only does SVG string concatenation.
//!
//! ## ★ P03 (S1) change
//! Removed the old `render_edge` (based on the `McVecEdge` binary model).
//! After P03 cut the dual track, `graph.edges` is no longer populated, and `render_edge` has
//! become dead code. The renderer (`SvgRenderer`) now uniformly only calls `render_viznet`.

use crate::vector::graph::{NetKind, Route, VizNet};

// ============================================================================
// ★ Render VizNet (multi-endpoint hyperedge, the only render path)
// ============================================================================

/// Render a `VizNet` (using its pre-computed `Route`)
///
/// Style by NetKind:
/// - Power     —— red, medium-thick
/// - Ground    —— blue, medium-thick
/// - Signal    —— black, thin
/// - Bus(n)    —— brown, thick + labeled width
/// - SubModuleIO —— purple, medium-thick (cross-module, emphasized)
pub fn render_viznet(net: &VizNet) -> String {
    let route = match &net.route {
        Some(r) => r,
        None => return String::new(),
    };
    if route.segments.is_empty() {
        return String::new();
    }

    let (color, width, dasharray) = style_for_kind(&net.kind);

    // Concatenate all segments into an SVG path
    let path_d = segments_to_svg_d(route);

    // ★ Stage D: decide whether to stamp the net-name label on the line (avoid duplicating
    //   the power flag / anonymous net)
    let anon = net.name.is_empty() || net.name.starts_with("__net");
    let show_label = match net.kind {
        // Power/ground: the flag (power_rail.rs) is already named + red/blue already conveys
        //   semantics → no need to repeat on the line
        NetKind::Power | NetKind::Ground => false,
        // Bus: keep (show bit width)
        NetKind::Bus(_) => true,
        // Signal / sub-module IO: only hide anonymous internal net names
        NetKind::Signal | NetKind::SubModuleIO => !anon,
    };

    let label_anchor = if show_label {
        pick_label_anchor(route)
    } else {
        None
    };
    let label_text = match &net.kind {
        NetKind::Bus(n) => format!("{} [{}]", net.name, n),
        _ => net.name.clone(),
    };

    let css_class = match &net.kind {
        NetKind::Bus(_) => "edge net bus",
        NetKind::Power => "edge net power",
        NetKind::Ground => "edge net ground",
        NetKind::SubModuleIO => "edge net sub-io",
        NetKind::Signal => "edge net signal",
    };

    let dash_attr = if dasharray.is_empty() {
        String::new()
    } else {
        format!(r##" stroke-dasharray="{dasharray}""##)
    };

    let mut svg = format!(
        r##"  <g class="{cls}" data-nid="{nid}" data-kind="{kind}">
    <path d="{p}" stroke="{c}" stroke-width="{w}" fill="none" stroke-linecap="square" stroke-linejoin="miter"{dash}/>
"##,
        cls = css_class,
        nid = net.nid,
        kind = net.kind,
        p = path_d,
        c = color,
        w = width,
        dash = dash_attr,
    );

    // junction dots (at T-junctions)
    for j in &route.junctions {
        svg.push_str(&format!(
            "    <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" fill=\"{}\"/>\n",
            j.x, j.y, color
        ));
    }

    // Label
    if let Some((mx, my)) = label_anchor {
        svg.push_str(&format!(
            "    {}\n",
            render_label_with_bg(&label_text, mx, my, color, 9.5)
        ));
    }

    svg.push_str("  </g>\n");
    svg
}

// ============================================================================
// Internal helpers
// ============================================================================

fn style_for_kind(kind: &NetKind) -> (&'static str, f64, &'static str) {
    match kind {
        NetKind::Power => ("#C0392B", 1.8, ""),
        NetKind::Ground => ("#2980B9", 1.8, ""),
        NetKind::Signal => ("#2c3e50", 1.4, ""),
        NetKind::Bus(_) => ("#854F0B", 3.2, ""),
        NetKind::SubModuleIO => ("#6A1B9A", 2.0, ""),
    }
}

/// Assemble the scattered segments of a route into continuous polylines and emit them (★ P3).
///
/// The old implementation emitted each segment as an independent `M-L` subpath → corners did
/// not miter, there were seams, and collinear midpoints were redundant. The new implementation:
/// join segments by shared endpoint into the longest polyline chain (break at endpoints/intersections),
/// merge collinear midpoints, drop overlapping / zero-length segments, and emit each chain as
/// a continuous `M L L…`. Jumper half-circles and T-intersections (junction dots drawn separately)
/// are both naturally preserved.
fn segments_to_svg_d(route: &Route) -> String {
    let mut out = String::new();
    for chain in build_polylines(route) {
        if chain.len() < 2 {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        for (i, &(x, y)) in chain.iter().enumerate() {
            if i == 0 {
                out.push_str(&format!("M{x:.1},{y:.1}"));
            } else {
                out.push_str(&format!(" L{x:.1},{y:.1}"));
            }
        }
    }
    out
}

/// Assemble `route.segments` into several continuous polylines (ordered point sequences, each ≥ 2 points).
///
/// Approach: quantize endpoints to 0.1px to build an undirected graph (removing floating-point
/// noise / duplicate edges / zero-length) → start from "non-degree-2 nodes" (endpoints/intersections)
/// and walk along degree-2 nodes to form the longest chain, breaking at intersections
/// (junction dots already drawn separately) → the remaining pure cycles are collected separately.
/// Finally, each chain merges collinear midpoints.
fn build_polylines(route: &Route) -> Vec<Vec<(f64, f64)>> {
    use std::collections::{HashMap, HashSet};
    type Node = (i64, i64);
    fn key(x: f64, y: f64) -> Node {
        ((x * 10.0).round() as i64, (y * 10.0).round() as i64)
    }
    fn unkey(n: Node) -> (f64, f64) {
        (n.0 as f64 / 10.0, n.1 as f64 / 10.0)
    }
    fn ekey(a: Node, b: Node) -> (Node, Node) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    // Undirected adjacency (deduplicate edges + drop zero-length)
    let mut adj: HashMap<Node, Vec<Node>> = HashMap::new();
    let mut seen: HashSet<(Node, Node)> = HashSet::new();
    for s in &route.segments {
        let a = key(s.from.x, s.from.y);
        let b = key(s.to.x, s.to.y);
        if a == b || !seen.insert(ekey(a, b)) {
            continue;
        }
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    if adj.is_empty() {
        return Vec::new();
    }

    let mut used: HashSet<(Node, Node)> = HashSet::new();
    let mut chains: Vec<Vec<Node>> = Vec::new();
    let nodes: Vec<Node> = adj.keys().copied().collect();

    // 1. Start from non-degree-2 nodes (endpoints/intersections) and walk the longest chain
    for &start in &nodes {
        if adj[&start].len() == 2 {
            continue;
        }
        for first in adj[&start].clone() {
            if used.contains(&ekey(start, first)) {
                continue;
            }
            let mut chain = vec![start];
            let (mut prev, mut cur) = (start, first);
            loop {
                used.insert(ekey(prev, cur));
                chain.push(cur);
                if adj.get(&cur).map(|v| v.len()).unwrap_or(0) != 2 {
                    break; // endpoint / intersection → break chain
                }
                let nxt = adj[&cur]
                    .iter()
                    .copied()
                    .find(|&x| x != prev && !used.contains(&ekey(cur, x)));
                match nxt {
                    Some(x) => {
                        prev = cur;
                        cur = x;
                    }
                    None => break,
                }
            }
            chains.push(chain);
        }
    }

    // 2. Remaining pure cycles (all nodes are degree 2, no endpoint to start from)
    for &start in &nodes {
        for first in adj[&start].clone() {
            if used.contains(&ekey(start, first)) {
                continue;
            }
            let mut chain = vec![start];
            let (mut prev, mut cur) = (start, first);
            loop {
                used.insert(ekey(prev, cur));
                chain.push(cur);
                if cur == start {
                    break; // cycle closed
                }
                let nxt = adj
                    .get(&cur)
                    .and_then(|v| v.iter().copied().find(|&x| !used.contains(&ekey(cur, x))));
                match nxt {
                    Some(x) => {
                        prev = cur;
                        cur = x;
                    }
                    None => break,
                }
            }
            chains.push(chain);
        }
    }

    chains
        .into_iter()
        .map(|ch| merge_collinear(&ch.into_iter().map(unkey).collect::<Vec<_>>()))
        .filter(|p| p.len() >= 2)
        .collect()
}

/// Remove collinear midpoints from an orthogonal polyline (three consecutive points sharing
/// x or y → the middle one is redundant). The arc points of a jumper half-circle differ in
/// both x and y → unaffected, the arc is preserved.
fn merge_collinear(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = *out.last().unwrap();
        let b = pts[i];
        let c = pts[i + 1];
        let coll_x = (a.0 - b.0).abs() < 0.2 && (b.0 - c.0).abs() < 0.2;
        let coll_y = (a.1 - b.1).abs() < 0.2 && (b.1 - c.1).abs() < 0.2;
        if coll_x || coll_y {
            continue; // b is collinear → drop
        }
        out.push(b);
    }
    out.push(pts[pts.len() - 1]);
    out
}

/// Label anchor: find the midpoint of the longest segment; None if there is no segment
fn pick_label_anchor(route: &Route) -> Option<(f64, f64)> {
    route
        .segments
        .iter()
        .max_by(|a, b| {
            let la = (a.to.x - a.from.x).hypot(a.to.y - a.from.y);
            let lb = (b.to.x - b.from.x).hypot(b.to.y - b.from.y);
            la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|seg| ((seg.from.x + seg.to.x) / 2.0, (seg.from.y + seg.to.y) / 2.0))
}

/// Label + semi-transparent white background rectangle (to avoid being obscured by lines/boxes)
pub fn render_label_with_bg(text: &str, mx: f64, my: f64, color: &str, size: f64) -> String {
    let w = (text.chars().count() as f64) * size * 0.6 + 6.0;
    let h = size + 4.0;
    format!(
        r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="2"
              fill="#ffffff" fill-opacity="0.85"/>
        <text x="{mx:.1}" y="{my:.1}" font-size="{s:.1}" fill="{c}"
              text-anchor="middle" dominant-baseline="central">{t}</text>"##,
        x = mx - w / 2.0,
        y = my - h / 2.0,
        w = w,
        h = h,
        mx = mx,
        my = my,
        s = size,
        c = color,
        t = escape_xml(text),
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
