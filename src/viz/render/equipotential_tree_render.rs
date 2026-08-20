// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! SVG renderer for equipotential trees.
//!
//! Renders an `EquiTree` into SVG: segments, junction dots, and terminal symbols.

use super::super::layout::equipotential_tree::{EquiTree, TreeSymbol, TreeSymbolKind};
use crate::vector::graph::NetKind;

/// Render an equipotential tree into SVG.
pub fn render_equi_tree(tree: &EquiTree) -> String {
    let (color, stroke_w) = style_for_kind(&tree.net_kind);

    let mut svg = String::new();

    // ── Segments ──
    for seg in &tree.segments {
        svg.push_str(&format!(
            r##"  <line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"
       stroke="{color}" stroke-width="{sw:.1}"/>"##,
            x1 = seg.x1,
            y1 = seg.y1,
            x2 = seg.x2,
            y2 = seg.y2,
            color = color,
            sw = stroke_w,
        ));
        svg.push('\n');
    }

    // ── Junction dots (>=3 degree) ──
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
fn render_symbol(sym: &TreeSymbol, net_kind: &NetKind) -> String {
    let (color, _) = style_for_kind(net_kind);

    match sym.kind {
        TreeSymbolKind::Ground => {
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
        TreeSymbolKind::NetLabel | TreeSymbolKind::Power => {
            // M3.5 (R1): text side comes from `text_side` (net region), NOT from
            // `dir` — after M1 flipped the trunks horizontal, `pick_stub_dir`
            // prefers down/up so `dir.0` was always 0.0 and labels always wrote
            // right (a West symbol's text ran back along the trunk into the IC).
            let (tx, anchor) = if sym.text_side < 0.0 {
                (sym.x - 4.0, "end")
            } else {
                (sym.x + 4.0, "start")
            };
            format!(
                r##"  <text x="{x:.1}" y="{y:.1}" text-anchor="{anchor}"
       font-size="10" font-weight="600" fill="{color}"
       dominant-baseline="central">{label}</text>
"##,
                x = tx,
                y = sym.y,
                anchor = anchor,
                color = color,
                label = escape_xml(&sym.label),
            )
        }
        TreeSymbolKind::PortLabel => {
            // M3.5 (R1): honour `text_side` like the other labels, so A17's
            // bbox estimate matches what is actually drawn (a centered "middle"
            // anchor would extend half a label width to both sides and let A17
            // go false-green for a PortLabel pressed against another glyph).
            let (tx, anchor) = if sym.text_side < 0.0 {
                (sym.x - 4.0, "end")
            } else {
                (sym.x + 4.0, "start")
            };
            format!(
                r##"  <text x="{x:.1}" y="{y:.1}" text-anchor="{anchor}"
       font-size="10" font-weight="600" fill="#7B1FA2"
       dominant-baseline="central">{label}</text>
"##,
                x = tx,
                y = sym.y,
                anchor = anchor,
                label = escape_xml(&sym.label),
            )
        }
        TreeSymbolKind::BusLabel => {
            // M3.5 (R1): text side from `text_side` (net region), not `dir.0`
            // (see the NetLabel branch above).
            let r = 6.0;
            let (tx, anchor) = if sym.text_side < 0.0 {
                (sym.x - r - 4.0, "end")
            } else {
                (sym.x + r + 4.0, "start")
            };
            format!(
                r##"  <circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" fill="none" stroke="{color}" stroke-width="1.5"/>
  <text x="{tx:.1}" y="{y:.1}" text-anchor="{anchor}"
       font-size="10" font-weight="600" fill="{color}"
       dominant-baseline="central">{label}</text>
"##,
                x = sym.x,
                y = sym.y,
                r = r,
                tx = tx,
                anchor = anchor,
                color = color,
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
