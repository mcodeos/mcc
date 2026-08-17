// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! SVG renderer for equipotential trees.
//!
//! Renders an `EquiTree` into SVG: trunk, taps, local trunks, junction dots,
//! and terminal symbols (GND bars, net labels, port labels).

use super::super::layout::equipotential_tree::{EquiTree, TreeSymbolKind};
use crate::vector::graph::NetKind;

/// Render an equipotential tree into SVG.
pub fn render_equi_tree(tree: &EquiTree) -> String {
    let (color, stroke_w) = style_for_kind(&tree.net_kind);

    if tree.horizontal_trunk {
        return render_horizontal_tree(tree, color, stroke_w);
    }

    let mut svg = String::new();

    // ── Main trunk ──
    svg.push_str(&format!(
        r##"  <line x1="{x:.1}" y1="{y1:.1}" x2="{x:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
        x = tree.trunk_x,
        y1 = tree.trunk_y_min,
        y2 = tree.trunk_y_max,
        color = color,
        sw = stroke_w,
    ));
    svg.push('\n');

    // ── Anchor local trunk ──
    svg.push_str(&format!(
        r##"  <line x1="{x:.1}" y1="{y1:.1}" x2="{x:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
        x = tree.anchor_local_trunk_x,
        y1 = tree.anchor_local_trunk_y_min,
        y2 = tree.anchor_local_trunk_y_max,
        color = color,
        sw = stroke_w,
    ));
    svg.push('\n');

    // Anchor-to-trunk horizontal line
    let anchor_attach_y = (tree.anchor_local_trunk_y_min + tree.anchor_local_trunk_y_max) / 2.0;
    svg.push_str(&format!(
        r##"  <line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
        x1 = tree.anchor_local_trunk_x,
        y = anchor_attach_y,
        x2 = tree.trunk_x,
        color = color,
        sw = stroke_w,
    ));
    svg.push('\n');

    // ── Tap branches ──
    for tap in &tree.taps {
        // Local trunk for this tap box
        svg.push_str(&format!(
            r##"  <line x1="{x:.1}" y1="{y1:.1}" x2="{x:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
            x = tap.local_trunk_x,
            y1 = tap.local_trunk_y_min,
            y2 = tap.local_trunk_y_max,
            color = color,
            sw = stroke_w,
        ));
        svg.push('\n');

        // Horizontal tap from local trunk to main trunk
        svg.push_str(&format!(
            r##"  <line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
            x1 = tap.local_trunk_x,
            y = tap.trunk_attach_y,
            x2 = tree.trunk_x,
            color = color,
            sw = stroke_w,
        ));
        svg.push('\n');
    }

    // ── Junction dots (≥3 line intersections) ──
    for &(jx, jy) in &tree.junction_dots {
        svg.push_str(&format!(
            r##"  <circle cx="{x:.1}" cy="{y:.1}" r="3.0" fill="{color}"/>"##,
            x = jx,
            y = jy,
            color = color,
        ));
        svg.push('\n');
    }

    // ── Terminal symbols ──
    for sym in &tree.symbols {
        svg.push_str(&render_symbol(sym, &tree.net_kind));
    }

    svg
}

/// Render a horizontal-trunk equipotential tree.
fn render_horizontal_tree(tree: &EquiTree, color: &str, stroke_w: f64) -> String {
    let mut svg = String::new();

    // ── Main trunk (horizontal) ──
    svg.push_str(&format!(
        r##"  <line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
        x1 = tree.trunk_x_min,
        y1 = tree.trunk_y,
        x2 = tree.trunk_x_max,
        y2 = tree.trunk_y,
        color = color,
        sw = stroke_w,
    ));
    svg.push('\n');

    // ── Anchor local trunk (vertical) ──
    svg.push_str(&format!(
        r##"  <line x1="{x:.1}" y1="{y1:.1}" x2="{x:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
        x = tree.anchor_local_trunk_x,
        y1 = tree.anchor_local_trunk_y_min,
        y2 = tree.anchor_local_trunk_y_max,
        color = color,
        sw = stroke_w,
    ));
    svg.push('\n');

    // Anchor-to-trunk: vertical line from anchor local trunk center to trunk_y
    let anchor_attach_y = (tree.anchor_local_trunk_y_min + tree.anchor_local_trunk_y_max) / 2.0;
    svg.push_str(&format!(
        r##"  <line x1="{x:.1}" y1="{y1:.1}" x2="{x:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
        x = tree.anchor_local_trunk_x,
        y1 = anchor_attach_y,
        y2 = tree.trunk_y,
        color = color,
        sw = stroke_w,
    ));
    svg.push('\n');

    // ── Tap branches ──
    for tap in &tree.taps {
        // Local trunk for this tap box (vertical)
        svg.push_str(&format!(
            r##"  <line x1="{x:.1}" y1="{y1:.1}" x2="{x:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
            x = tap.local_trunk_x,
            y1 = tap.local_trunk_y_min,
            y2 = tap.local_trunk_y_max,
            color = color,
            sw = stroke_w,
        ));
        svg.push('\n');

        // Vertical tap from local trunk to main trunk (horizontal)
        // The tap connects at trunk_attach_x on the trunk
        let tap_attach_y = (tap.local_trunk_y_min + tap.local_trunk_y_max) / 2.0;
        svg.push_str(&format!(
            r##"  <line x1="{x:.1}" y1="{y1:.1}" x2="{x:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
            x = tap.local_trunk_x,
            y1 = tap_attach_y,
            y2 = tree.trunk_y,
            color = color,
            sw = stroke_w,
        ));
        svg.push('\n');
    }

    // ── Junction dots (≥3 line intersections) ──
    for &(jx, jy) in &tree.junction_dots {
        svg.push_str(&format!(
            r##"  <circle cx="{x:.1}" cy="{y:.1}" r="3.0" fill="{color}"/>"##,
            x = jx,
            y = jy,
            color = color,
        ));
        svg.push('\n');
    }

    // ── Terminal symbols ──
    for sym in &tree.symbols {
        svg.push_str(&render_symbol(sym, &tree.net_kind));
    }

    svg
}

/// Render a terminal symbol.
fn render_symbol(
    sym: &super::super::layout::equipotential_tree::TreeSymbol,
    net_kind: &NetKind,
) -> String {
    let (color, _) = style_for_kind(net_kind);

    match sym.kind {
        TreeSymbolKind::Ground => {
            // GND symbol: 3 horizontal bars decreasing in width
            let bar_w1 = 20.0;
            let bar_w2 = 14.0;
            let bar_w3 = 8.0;
            let bar_gap = 4.0;
            let x = sym.x;
            let y = sym.y;

            format!(
                r##"  <line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}" stroke="{color}" stroke-width="1.5"/>
  <line x1="{x3:.1}" y1="{y2:.1}" x2="{x4:.1}" y2="{y2:.1}" stroke="{color}" stroke-width="1.5"/>
  <line x1="{x5:.1}" y1="{y3:.1}" x2="{x6:.1}" y2="{y3:.1}" stroke="{color}" stroke-width="1.5"/>
  <line x1="{x:.1}" y1="{y0:.1}" x2="{x:.1}" y2="{y:.1}" stroke="{color}" stroke-width="1.5"/>"##,
                x = x,
                x1 = x - bar_w1 / 2.0,
                x2 = x + bar_w1 / 2.0,
                x3 = x - bar_w2 / 2.0,
                x4 = x + bar_w2 / 2.0,
                x5 = x - bar_w3 / 2.0,
                x6 = x + bar_w3 / 2.0,
                y0 = y - bar_gap,
                y = y,
                y2 = y + bar_gap,
                y3 = y + 2.0 * bar_gap,
                color = color,
            )
        }
        TreeSymbolKind::Power => {
            format!(
                r##"  <text x="{x:.1}" y="{y:.1}" text-anchor="middle"
       font-size="10" font-weight="600" fill="{color}"
       dominant-baseline="central">{label}</text>
"##,
                x = sym.x,
                y = sym.y,
                color = color,
                label = escape_xml(&sym.label),
            )
        }
        TreeSymbolKind::NetLabel => {
            format!(
                r##"  <text x="{x:.1}" y="{y:.1}" text-anchor="middle"
       font-size="10" font-weight="600" fill="{color}"
       dominant-baseline="central">{label}</text>
"##,
                x = sym.x,
                y = sym.y,
                color = color,
                label = escape_xml(&sym.label),
            )
        }
        TreeSymbolKind::PortLabel => {
            format!(
                r##"  <text x="{x:.1}" y="{y:.1}" text-anchor="middle"
       font-size="10" font-weight="600" fill="#7B1FA2"
       dominant-baseline="central">{label}</text>
"##,
                x = sym.x,
                y = sym.y,
                label = escape_xml(&sym.label),
            )
        }
    }
}

/// Color and stroke-width by NetKind.
fn style_for_kind(kind: &NetKind) -> (&'static str, f64) {
    match kind {
        NetKind::Power => ("#C0392B", 2.0),
        NetKind::Ground => ("#2980B9", 2.0),
        NetKind::Signal => ("#222222", 1.5),
        NetKind::SubModuleIO => ("#7B1FA2", 2.0),
        NetKind::Bus(_) => ("#1565C0", 2.5),
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
