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

use crate::vector::graph::McVecGraph;

// SvgRenderer (P4 assembly)

/// SVG renderer
///
/// Replaces the old `legacy::SvgRenderer`; the new version renders
/// `graph.nets` (★ VizNet multi-endpoint model).
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

            // ── ★ Module-port drawing: the module's own boundary ──
            // A module drawn as its own layer shows its ports as drawn objects on
            // a dashed frame, named by the port (`vin`) — never by the net the
            // wire carries (`V5V`), which stays on the wire. The frame geometry
            // comes from the `module_frame` layout pass; nothing is recomputed
            // here (see module-port-drawing-design.md).
            if let Some(mf) = &graph.module_frame {
                svg.push_str(&render_module_frame(mf));
            }

            // ── ★ P7-3: rail terminal decorations (pin render attributes, not boxes, discipline
            // 11) ──
            // ★ C1b: disabled — equipotential trees handle all power/ground symbols
            // (Power dots above the pin, ground symbols below the pin).
        }

        svg.push_str("</svg>\n");
        svg
    }
}

/// Draw a module's boundary frame: the dashed rect, its title, and the ports on
/// it.
///
/// A port is a drawn object — a stub tick plus a dot at the anchor the layout
/// pass chose, with the port's name just outside the frame. The name is the
/// **port's** (`vin`); the net it carries keeps its own name on the wire, so one
/// place carries one identity. Colour follows the structurally-carried supply
/// axis (never the port or net name): supply ports take the rail red the device
/// layer already paints power with, signals the signal blue.
fn render_module_frame(mf: &crate::vector::graph::ModuleFrame) -> String {
    use crate::vector::graph::EntrySide;

    let mut svg = format!(
        r##"  <g class="module-frame">
    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="8" ry="8"
          fill="none" stroke="#616161" stroke-width="1.5" stroke-dasharray="8,4"/>
    <text x="{tx:.1}" y="{ty:.1}" font-size="14" font-weight="600" fill="#616161"
          dominant-baseline="auto">{title}</text>
"##,
        x = mf.x,
        y = mf.y,
        w = mf.w,
        h = mf.h,
        tx = mf.x,
        ty = mf.y - 8.0,
        title = escape_xml(&mf.title),
    );

    for p in &mf.ports {
        let color = if p.is_supply { "#C0392B" } else { "#2980B9" };
        // The stub points from the frame inward, so the port reads as a terminal
        // ON the boundary rather than a floating label.
        const TICK: f64 = 9.0;
        let (sx, sy, ex, ey, ax, ay, anchor) = match p.side {
            EntrySide::Left => (p.x, p.y, p.x + TICK, p.y, p.x - 6.0, p.y, "end"),
            EntrySide::Right => (p.x, p.y, p.x - TICK, p.y, p.x + 6.0, p.y, "start"),
            EntrySide::Top => (p.x, p.y, p.x, p.y + TICK, p.x, p.y - 6.0, "middle"),
            EntrySide::Bottom => (p.x, p.y, p.x, p.y - TICK, p.x, p.y + 12.0, "middle"),
        };
        svg.push_str(&format!(
            r##"    <g class="port" data-port="{name}">
    <line x1="{sx:.1}" y1="{sy:.1}" x2="{ex:.1}" y2="{ey:.1}"
          stroke="{color}" stroke-width="2.0"/>
    <circle cx="{px:.1}" cy="{py:.1}" r="3.0" fill="{color}"/>
    <text x="{ax:.1}" y="{ay:.1}" text-anchor="{anchor}" font-size="11"
          font-weight="600" fill="{color}" dominant-baseline="central">{name}</text>
  </g>
"##,
            name = escape_xml(&p.name),
            sx = sx,
            sy = sy,
            ex = ex,
            ey = ey,
            px = p.x,
            py = p.y,
            ax = ax,
            ay = ay,
            anchor = anchor,
            color = color,
        ));
    }
    svg.push_str("  </g>\n");
    svg
}

/// Render block edges for the root layer block diagram.
///
/// Draws orthogonal edges from the nearest box edges (not centers).
/// For boxes that overlap in y, draws horizontal lines between right/left edges.
/// For boxes that overlap in x, draws vertical lines between bottom/top edges.
/// Otherwise, draws from center to center.
/// For lane_count > 1 (bus edges), draws a thick line with slash marks
/// and lane count annotation (W3).
///
/// ★ P1-a: this is a pure formatter over [`supply_bundle::SupplyBundlePlan`].
/// The rail anchor that used to live here (the L2 "edge midpoint" fallback
/// shape) moved to `supply_bundle`, which is now the single authority for
/// where an edge attaches.
fn render_block_edges(graph: &McVecGraph) -> String {
    use crate::viz::layout::edge_decide::EdgeKind;
    use crate::viz::layout::supply_bundle;

    // ★ P1-a: the grouping, the rail x and every landing point are decided in
    // `supply_bundle::build_plan`; this function only turns that plan into SVG.
    // It used to compute all of it inline -- reading a `HashMap` for the
    // grouping order, and matching a net label against pin names for landings.
    // One authority for "where does an edge attach", drawn once.
    let plan = supply_bundle::build_plan(graph);
    let mut svg = String::new();

    // ── Bus trunks: one vertical rail per power label, one tap per consumer ──
    let stroke = "#E65100";
    for trunk in &plan.trunks {
        let label = &trunk.label;

        // Trunk rail
        svg.push_str(&format!(
            r##"  <line x1="{tx:.1}" y1="{y1:.1}" x2="{tx:.1}" y2="{y2:.1}"
       stroke="{stroke}" stroke-width="2.5"/>"##,
            tx = trunk.x,
            y1 = trunk.y_min,
            y2 = trunk.y_max,
            stroke = stroke,
        ));
        svg.push('\n');

        // Driver-to-trunk line
        if let Some((dx, dy)) = trunk.driver {
            let line_svg = render_ortho_path(dx, dy, trunk.x, dy, label, stroke, 2.5, false);
            svg.push_str(&line_svg);
        }

        // Trunk-to-consumer lines
        for (cx, cy) in &trunk.taps {
            let line_svg = render_ortho_path(trunk.x, *cy, *cx, *cy, label, stroke, 2.5, false);
            svg.push_str(&line_svg);
        }

        // Label at trunk midpoint
        let label_mid_y = (trunk.y_min + trunk.y_max) / 2.0;
        svg.push_str(&format!(
            r##"  <text x="{tx:.1}" y="{my:.1}" text-anchor="end"
       font-size="11" font-weight="600" fill="{stroke}"
       dominant-baseline="central">{label}</text>
"##,
            tx = trunk.x - 5.0,
            my = label_mid_y,
            stroke = stroke,
            label = escape_xml(label),
        ));
    }

    // ── Individual edges (non-power, or power below the trunk threshold) ──
    for draw in &plan.individual {
        let (x1, y1) = draw.from;
        let (x2, y2) = draw.to;

        let is_bus = draw.lane_count > 1;
        let stroke = match draw.kind {
            EdgeKind::Power => "#E65100",
            EdgeKind::Bus => "#1565C0",
            EdgeKind::Signal => "#424242",
        };
        let stroke_w = if is_bus {
            4.0
        } else {
            match draw.kind {
                EdgeKind::Power => 2.5,
                EdgeKind::Bus => 2.5,
                EdgeKind::Signal => 2.0,
            }
        };

        let label_text = if is_bus {
            format!("{} [{}]", draw.label, draw.lane_count)
        } else {
            draw.label.clone()
        };

        // Use orthogonal path for edges that need bends
        let needs_ortho = draw.ortho;
        if needs_ortho {
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
        if !draw.label.is_empty() {
            let (mx, my) = if needs_ortho {
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
