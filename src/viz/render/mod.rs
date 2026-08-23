// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! SVG render layer
//!
//! ## Architecture after P4 completes
//!
//! ```text
//!   McVecGraph (already layouted, nets already routed)
//!         │
//!         ▼
//!     SvgRenderer::render(graph, canvas)
//!            │
//!            ├── shape::render_box ─────→ each box → SVG <g>
//!            │     ├── two_pin / multi_pin / sub_module / power_label
//!            │     └── BoxShape trait
//!            └── equipotential_tree_render ─→ each tree → SVG <g> (device layer)
//! ```
//!
//! ## Root layer (P9-B)
//! For the root layer, nets are not rendered. Instead, block edges (from
//! `edge_decide::decide_edges`) are drawn as straight lines with arrows and
//! labels. Sub-module boxes use solid-line block-diagram styling.
//!
//! ## Sub-modules
//! - [`shape`]       —— `BoxShape` trait + `render_box` dispatch
//! - [`two_pin`]     —— R / C / L / D etc.
//! - [`multi_pin`]   —— multi-pin IC
//! - [`sub_module`]  —— sub-module (with expand hint, extracted in P3)
//! - [`power_label`] —— power / ground
//!
//! ## legacy.rs has been removed
//! P4 extracted all features from legacy.rs; the file can be removed entirely.

pub mod capacitor;
pub mod diode;
pub mod equipotential_tree_render;
pub mod ic;
pub mod inductor;
pub mod label_render;
pub mod multi_pin;
pub mod pin_render;
pub mod power_label;
pub mod power_rail;
pub mod resistor;
pub mod shape;
pub mod sub_module;
pub mod two_pin;
pub use shape::{render_box, BoxShape};

use crate::vector::graph::{McVecBox, McVecGraph};

// ============================================================================
// SvgRenderer (P4 assembly)
// ============================================================================

/// SVG renderer
///
/// Replaces the old `legacy::SvgRenderer`; the new version supports both:
/// - `graph.edges` (old McVecEdge binary model, compatible)
/// - `graph.nets`  (★ VizNet multi-endpoint model, preferred)
///
/// When `graph.nets` is non-empty, prefer rendering nets; otherwise fall back to edges.
pub struct SvgRenderer;

impl SvgRenderer {
    pub fn render(
        graph: &McVecGraph,
        viewbox_x: f64,
        viewbox_y: f64,
        canvas_w: f64,
        canvas_h: f64,
    ) -> String {
        let mut svg = String::new();

        svg.push_str(&format!(
            r##"<svg viewBox="{vx:.0} {vy:.0} {vw:.0} {vh:.0}" xmlns="http://www.w3.org/2000/svg"
     font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
     style="background:transparent">"##,
            vx = viewbox_x,
            vy = viewbox_y,
            vw = canvas_w,
            vh = canvas_h
        ));
        svg.push('\n');

        svg.push_str(
            r##"  <defs>
    <marker id="dot" markerWidth="6" markerHeight="6" refX="3" refY="3">
      <circle cx="3" cy="3" r="2" fill="#888"/>
    </marker>
    <marker id="arrow" markerWidth="8" markerHeight="6" refX="8" refY="3" orient="auto">
      <path d="M0,0 L8,3 L0,6 Z" fill="#424242"/>
    </marker>
  </defs>
"##,
        );

        if graph.layer_style == crate::vector::graph::LayerStyle::Block {
            // ── ★ Block: root layer block diagram rendering ──
            // Render block edges instead of nets.
            svg.push_str(&render_block_edges(graph));

            // Boxes: root layer solid-line styling.
            for b in &graph.boxes {
                svg.push_str(&shape::render_box(b, true));
            }
        } else {
            // ── ★ Device: equipotential tree rendering for sub-layers ──
            // Each net is rendered as ONE connected orthogonal tree, not n-1 edges.
            let trees = crate::viz::layout::equipotential_tree::build_all_trees(graph);
            for tree in &trees {
                svg.push_str(&equipotential_tree_render::render_equi_tree(tree));
            }

            // ── Zone borders (M2-3) ──
            for zb in &graph.zone_borders {
                svg.push_str(&format!(
                    r##"  <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="none" stroke="#aaa" stroke-width="1.5" stroke-dasharray="8,4" rx="6" ry="6"/>
  <text x="{tx:.1}" y="{ty:.1}" font-size="14" font-weight="600" fill="#666">{title}</text>"##,
                    x = zb.x,
                    y = zb.y,
                    w = zb.w,
                    h = zb.h,
                    tx = zb.title_x,
                    ty = zb.title_y,
                    title = zb.title,
                ));
                svg.push('\n');
            }

            // ── Boxes (top layer) ──
            // ★ C1b: skip label-kind boxes — they are rendered as tree symbols
            // (PowerLabel / Dot / PortTerminal), not as physical component boxes.
            use crate::vector::graph::BoxKind;
            for b in &graph.boxes {
                if matches!(
                    b.kind,
                    BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
                ) {
                    continue;
                }
                svg.push_str(&shape::render_box(b, false));
            }

            // ── ★ P7-3: rail terminal decorations (pin render attributes, not boxes, discipline 11) ──
            // ★ C1b: disabled — equipotential trees handle all power/ground symbols
            // (Power dots above the pin, ground symbols below the pin).
        }

        svg.push_str("</svg>\n");
        svg
    }
}

/// Render block edges for the root layer block diagram.
///
/// Draws orthogonal edges from the nearest box edges (not centers).
/// For boxes that overlap in y, draws horizontal lines between right/left edges.
/// For boxes that overlap in x, draws vertical lines between bottom/top edges.
/// Otherwise, draws from center to center.
/// For lane_count > 1 (bus edges), draws a thick line with slash marks
/// and lane count annotation (W3).
/// Compute the rail anchor position on a box edge facing a target point (P-2).
///
/// Rail anchors are centered on the edge facing the target, not at pin positions.
/// If multiple rail anchors share the same edge, they are evenly distributed.
fn rail_anchor(b: &McVecBox, target_x: f64, target_y: f64, idx: usize, total: usize) -> (f64, f64) {
    let offset = if total <= 1 {
        0.5
    } else {
        (idx + 1) as f64 / (total + 1) as f64
    };

    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let dx = target_x - bx;
    let dy = target_y - by;

    if dx.abs() >= dy.abs() {
        // Horizontal: left or right edge
        if dx > 0.0 {
            (b.x + b.w, b.y + offset * b.h)
        } else {
            (b.x, b.y + offset * b.h)
        }
    } else {
        // Vertical: top or bottom edge
        if dy > 0.0 {
            (b.x + offset * b.w, b.y + b.h)
        } else {
            (b.x + offset * b.w, b.y)
        }
    }
}

fn render_block_edges(graph: &McVecGraph) -> String {
    use crate::viz::layout::edge_decide;
    use crate::viz::layout::edge_decide::EdgeKind;

    let (edges, _report) = edge_decide::decide_edges(graph);
    let mut svg = String::new();

    // Helper to find the pin position on a box for a given edge label.
    let pin_pos = |b: &crate::vector::graph::McVecBox, label: &str| -> (f64, f64) {
        for ep in &b.entry_points {
            if ep.pin_name == label {
                let (px, py) = match ep.side {
                    crate::vector::graph::EntrySide::Left => (b.x, b.y + ep.offset * b.h),
                    crate::vector::graph::EntrySide::Right => (b.x + b.w, b.y + ep.offset * b.h),
                    crate::vector::graph::EntrySide::Top => (b.x + ep.offset * b.w, b.y),
                    crate::vector::graph::EntrySide::Bottom => (b.x + ep.offset * b.w, b.y + b.h),
                };
                return (px, py);
            }
        }
        let base_label = label.split(' ').next().unwrap_or(label);
        for ep in &b.entry_points {
            if ep.pin_name == base_label {
                let (px, py) = match ep.side {
                    crate::vector::graph::EntrySide::Left => (b.x, b.y + ep.offset * b.h),
                    crate::vector::graph::EntrySide::Right => (b.x + b.w, b.y + ep.offset * b.h),
                    crate::vector::graph::EntrySide::Top => (b.x + ep.offset * b.w, b.y),
                    crate::vector::graph::EntrySide::Bottom => (b.x + ep.offset * b.w, b.y + b.h),
                };
                return (px, py);
            }
        }
        (b.x + b.w / 2.0, b.y + b.h / 2.0)
    };

    // ── ★ Bus trunk: group power edges with same label ──
    // Separate edges into bus groups and individual edges.
    let mut bus_groups: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    let mut individual_indices: Vec<usize> = Vec::new();

    for (i, edge) in edges.iter().enumerate() {
        if edge.kind == EdgeKind::Power && !edge.label.is_empty() {
            bus_groups.entry(edge.label.clone()).or_default().push(i);
        } else {
            individual_indices.push(i);
        }
    }

    // Process bus groups (power edges with same label)
    for (label, indices) in &bus_groups {
        if indices.len() >= 3 {
            // Identify the driver: the box that appears most frequently as `from` in the group.
            let mut from_counts: std::collections::HashMap<i64, usize> =
                std::collections::HashMap::new();
            for &idx in indices {
                let edge = &edges[idx];
                *from_counts.entry(edge.from_box).or_default() += 1;
            }
            let driver_box_id = from_counts
                .iter()
                .max_by_key(|(_, count)| **count)
                .map(|(id, _)| *id);

            // Compute trunk_x: midpoint between the driver's right edge and the
            // rightmost consumer's left edge. If no clear driver, use midpoint of all boxes.
            let mut driver_anchor: Option<(f64, f64)> = None;
            let mut all_box_xs: Vec<f64> = Vec::new();

            for &idx in indices {
                let edge = &edges[idx];
                let from_box = graph.boxes.iter().find(|b| b.id == edge.from_box);
                let to_box = graph.boxes.iter().find(|b| b.id == edge.to_box);
                let (Some(from), Some(to)) = (from_box, to_box) else {
                    continue;
                };
                all_box_xs.push(from.x + from.w);
                all_box_xs.push(to.x);

                let is_driver = Some(from.id) == driver_box_id;
                let is_driver_to = Some(to.id) == driver_box_id;
                if is_driver {
                    let (ax, ay) = rail_anchor(from, to.x + to.w / 2.0, to.y + to.h / 2.0, 0, 1);
                    driver_anchor = Some((ax, ay));
                } else if is_driver_to {
                    let (ax, ay) =
                        rail_anchor(to, from.x + from.w / 2.0, from.y + from.h / 2.0, 0, 1);
                    driver_anchor = Some((ax, ay));
                }
            }

            // Compute trunk_x dynamically: midpoint between driver right edge and
            // rightmost consumer left edge, with a minimum gap.
            all_box_xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let trunk_x = if all_box_xs.len() >= 2 {
                let leftmost = all_box_xs[0];
                let rightmost = all_box_xs[all_box_xs.len() - 1];
                (leftmost + rightmost) / 2.0
            } else {
                580.0
            };

            // Recompute consumer anchors with the dynamic trunk_x
            let mut consumer_anchors_final: Vec<((f64, f64), &edge_decide::BlockEdge)> = Vec::new();
            for &idx in indices {
                let edge = &edges[idx];
                let to_box = graph.boxes.iter().find(|b| b.id == edge.to_box);
                let Some(to) = to_box else {
                    continue;
                };
                let is_driver_to = Some(to.id) == driver_box_id;
                // ★ Rail trunk-tap fix: `decide_edges` emits power edges as
                // driver→consumer, so `is_driver` is true on every edge of a
                // single-driver star. The old `!is_driver && !is_driver_to`
                // guard skipped all of them → empty consumer anchors → the trunk
                // collapsed to a zero-length stub at the driver. Take every edge
                // whose *target* is a consumer (secondary consumer→consumer edges
                // in a multi-driver mesh still qualify; edges pointing back at the
                // driver are correctly excluded).
                if !is_driver_to {
                    let (ax, ay) = rail_anchor(to, trunk_x, to.y + to.h / 2.0, 0, 1);
                    consumer_anchors_final.push(((ax, ay), edge));
                }
            }

            // Collect all y values for trunk range
            let mut all_ys: Vec<f64> = consumer_anchors_final
                .iter()
                .map(|((_, y), _)| *y)
                .collect();
            if let Some((_, dy)) = driver_anchor {
                all_ys.push(dy);
            }
            all_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let trunk_y_min = all_ys.first().copied().unwrap_or(100.0);
            let trunk_y_max = all_ys.last().copied().unwrap_or(740.0);

            // Draw trunk line
            let stroke = "#E65100";
            svg.push_str(&format!(
                r##"  <line x1="{tx:.1}" y1="{y1:.1}" x2="{tx:.1}" y2="{y2:.1}"
       stroke="{stroke}" stroke-width="2.5"/>"##,
                tx = trunk_x,
                y1 = trunk_y_min,
                y2 = trunk_y_max,
                stroke = stroke,
            ));
            svg.push('\n');

            // Draw driver-to-trunk line
            if let Some((dx, dy)) = driver_anchor {
                let line_svg = render_ortho_path(dx, dy, trunk_x, dy, label, stroke, 2.5, false);
                svg.push_str(&line_svg);
            }

            // Draw trunk-to-consumer lines
            for ((cx, cy), _edge) in &consumer_anchors_final {
                let line_svg = render_ortho_path(trunk_x, *cy, *cx, *cy, label, stroke, 2.5, false);
                svg.push_str(&line_svg);
            }

            // Label at trunk midpoint
            let label_mid_y = (trunk_y_min + trunk_y_max) / 2.0;
            svg.push_str(&format!(
                r##"  <text x="{tx:.1}" y="{my:.1}" text-anchor="end"
       font-size="11" font-weight="600" fill="{stroke}"
       dominant-baseline="central">{label}</text>
"##,
                tx = trunk_x - 5.0,
                my = label_mid_y,
                stroke = stroke,
                label = escape_xml(label),
            ));
        } else {
            // Power edges with <3 consumers: draw as direct lines
            for &idx in indices {
                individual_indices.push(idx);
            }
        }
    }

    // Process individual edges (non-power, or power with <3 consumers)
    for &idx in &individual_indices {
        let edge = &edges[idx];

        let from_box = graph.boxes.iter().find(|b| b.id == edge.from_box);
        let to_box = graph.boxes.iter().find(|b| b.id == edge.to_box);
        let (Some(from), Some(to)) = (from_box, to_box) else {
            continue;
        };

        let (x1, y1) = if edge.kind == EdgeKind::Power {
            rail_anchor(from, to.x + to.w / 2.0, to.y + to.h / 2.0, 0, 1)
        } else {
            pin_pos(from, &edge.label)
        };
        let (x2, y2) = if edge.kind == EdgeKind::Power {
            // Keep same coordinate as source for the axis where boxes are aligned
            let (tx, ty) = rail_anchor(to, from.x + from.w / 2.0, from.y + from.h / 2.0, 0, 1);
            // If boxes share the same x column, use same x; otherwise use same y
            if (x1 - tx).abs() < 1.0 {
                (x1, ty)
            } else {
                (tx, y1)
            }
        } else {
            pin_pos(to, &edge.label)
        };

        let is_bus = edge.lane_count > 1;
        let stroke = match edge.kind {
            edge_decide::EdgeKind::Power => "#E65100",
            edge_decide::EdgeKind::Bus => "#1565C0",
            edge_decide::EdgeKind::Signal => "#424242",
        };
        let stroke_w = if is_bus {
            4.0
        } else {
            match edge.kind {
                edge_decide::EdgeKind::Power => 2.5,
                edge_decide::EdgeKind::Bus => 2.5,
                edge_decide::EdgeKind::Signal => 2.0,
            }
        };

        let label_text = if is_bus {
            format!("{} [{}]", edge.label, edge.lane_count)
        } else {
            edge.label.clone()
        };

        // Use orthogonal path for edges that need bends
        let needs_ortho = (x1 - x2).abs() > 1.0 && (y1 - y2).abs() > 1.0;
        if needs_ortho && edge.kind == EdgeKind::Power {
            // Power edge with offset: use L-shaped path
            svg.push_str(&format!(
                r##"  <polyline points="{x1:.1},{y1:.1} {x2:.1},{y1:.1} {x2:.1},{y2:.1}"
       fill="none" stroke="{stroke}" stroke-width="{sw:.1}" marker-end="url(#arrow)"/>"##,
                x1 = x1,
                y1 = y1,
                x2 = x2,
                y2 = y2,
                stroke = stroke,
                sw = stroke_w,
            ));
            svg.push('\n');
        } else {
            // Direct line
            svg.push_str(&format!(
                r##"  <line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"
       stroke="{stroke}" stroke-width="{sw:.1}" marker-end="url(#arrow)"/>"##,
                x1 = x1,
                y1 = y1,
                x2 = x2,
                y2 = y2,
                stroke = stroke,
                sw = stroke_w,
            ));
            svg.push('\n');
        }

        // ★ W3: bus slash marks for lane_count>1 edges
        if is_bus {
            let dx = x2 - x1;
            let dy = y2 - y1;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.0 {
                let ux = dx / len;
                let uy = dy / len;
                let px = -uy;
                let py = ux;
                let slash_len = 12.0;
                for &t in &[1.0 / 3.0, 2.0 / 3.0] {
                    let cx = x1 + dx * t;
                    let cy = y1 + dy * t;
                    let sx1 = cx - px * slash_len / 2.0;
                    let sy1 = cy - py * slash_len / 2.0;
                    let sx2 = cx + px * slash_len / 2.0;
                    let sy2 = cy + py * slash_len / 2.0;
                    svg.push_str(&format!(
                        r##"  <line x1="{sx1:.1}" y1="{sy1:.1}" x2="{sx2:.1}" y2="{sy2:.1}"
       stroke="{stroke}" stroke-width="1.5"/>"##,
                        sx1 = sx1,
                        sy1 = sy1,
                        sx2 = sx2,
                        sy2 = sy2,
                        stroke = stroke,
                    ));
                    svg.push('\n');
                }
            }
        }

        // Label at midpoint
        if !edge.label.is_empty() {
            let (mx, my) = if needs_ortho && edge.kind == EdgeKind::Power {
                // L-shaped path: label on the horizontal segment
                ((x1 + x2) / 2.0, y1 - 10.0)
            } else {
                ((x1 + x2) / 2.0, (y1 + y2) / 2.0 - 10.0)
            };
            svg.push_str(&format!(
                r##"  <text x="{mx:.1}" y="{my:.1}" text-anchor="middle"
       font-size="11" font-weight="600" fill="{stroke}"
       dominant-baseline="central">{label}</text>
"##,
                mx = mx,
                my = my,
                stroke = stroke,
                label = escape_xml(&label_text),
            ));
        }
    }

    svg
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render an orthogonal path from (x1,y1) to (x2,y2).
///
/// The path goes horizontal first, then vertical to reach the target.
/// If the points share the same x or y, a single line is drawn.
fn render_ortho_path(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    label: &str,
    stroke: &str,
    stroke_w: f64,
    with_arrow: bool,
) -> String {
    let mut svg = String::new();
    let arrow = if with_arrow {
        r#" marker-end="url(#arrow)""#
    } else {
        ""
    };

    if (x1 - x2).abs() < 1.0 || (y1 - y2).abs() < 1.0 {
        // Single segment
        svg.push_str(&format!(
            r##"  <line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"
       stroke="{stroke}" stroke-width="{sw:.1}"{arrow}/>"##,
            x1 = x1,
            y1 = y1,
            x2 = x2,
            y2 = y2,
            stroke = stroke,
            sw = stroke_w,
            arrow = arrow,
        ));
        svg.push('\n');
    } else {
        // L-shaped: horizontal then vertical
        svg.push_str(&format!(
            r##"  <polyline points="{x1:.1},{y1:.1} {x2:.1},{y1:.1} {x2:.1},{y2:.1}"
       fill="none" stroke="{stroke}" stroke-width="{sw:.1}"{arrow}/>"##,
            x1 = x1,
            y1 = y1,
            x2 = x2,
            y2 = y2,
            stroke = stroke,
            sw = stroke_w,
            arrow = arrow,
        ));
        svg.push('\n');
    }

    if !label.is_empty() {
        let mx = (x1 + x2) / 2.0;
        let my = (y1 + y2) / 2.0;
        svg.push_str(&format!(
            r##"  <text x="{mx:.1}" y="{my:.1}" text-anchor="middle"
       font-size="11" font-weight="600" fill="{stroke}"
       dominant-baseline="central">{label}</text>
"##,
            mx = mx,
            my = my - 10.0,
            stroke = stroke,
            label = escape_xml(label),
        ));
    }

    svg
}
