// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power / ground label render (real electrical symbols) —— ★ Stage F: four-way orientable
//!
//! The flag's "connect edge" (`entry_points[0].side`, set by layout's place_flags) decides the
//! symbol's orientation: the symbol always points **outward, away from the consumer**, with
//! the lead running from the connect edge to the symbol.
//!
//! - connect edge = Bottom → symbol points up (classic VCC↑ / old behavior)
//! - connect edge = Top    → symbol points down
//! - connect edge = Right  → symbol points left (consumer on the right)
//! - connect edge = Left   → symbol points right
//!
//! This way, power flags for top-layer left-column modules point left, right-column modules
//! point right, and middle-column modules point up or down, all pointing at empty space and
//! not colliding with the signal lines in the middle.

use crate::vector::graph::{EntrySide, McVecBox, Symbol};

use super::shape::BoxShape;

const POWER_COLOR: &str = "#C0392B";
const GROUND_COLOR: &str = "#2980B9";

pub struct PowerRailShape;

impl BoxShape for PowerRailShape {
    fn render(&self, b: &McVecBox) -> String {
        let is_ground = matches!(b.symbol, Symbol::PowerRail { is_ground: true });
        // Connect edge (the edge the flag's single lead faces toward the consumer); if absent, fall back to classic "up"
        let connect = b
            .entry_points
            .first()
            .map(|e| e.side.clone())
            .unwrap_or(if is_ground {
                EntrySide::Top
            } else {
                EntrySide::Bottom
            });
        if is_ground {
            render_ground(b, &connect)
        } else {
            render_power(b, &connect)
        }
    }
}

// ============================================================================
// Orientation helpers
// ============================================================================

/// Symbol outward direction = away from consumer = opposite of the connect edge
fn glyph_outward(connect: &EntrySide) -> (f64, f64) {
    match connect {
        EntrySide::Top => (0.0, 1.0),     // connect on top → symbol points down
        EntrySide::Bottom => (0.0, -1.0), // connect on bottom → symbol points up (classic)
        EntrySide::Left => (1.0, 0.0),    // connect on left → symbol points right
        EntrySide::Right => (-1.0, 0.0),  // connect on right → symbol points left
    }
}

/// Label's text-anchor + baseline preference (by outward direction)
fn label_anchor_attrs(ox: f64, oy: f64) -> (&'static str, &'static str) {
    if ox > 0.3 {
        ("start", "central")
    } else if ox < -0.3 {
        ("end", "central")
    } else if oy < 0.0 {
        ("middle", "auto") // points up, label above
    } else {
        ("middle", "hanging") // points down, label below
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ============================================================================
// VCC / VDD style (triangle arrow points outward)
// ============================================================================

fn render_power(b: &McVecBox, connect: &EntrySide) -> String {
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;
    let (ox, oy) = glyph_outward(connect);
    let (tx, ty) = (-oy, ox); // tangent

    // Half-length along the outward direction / half-width perpendicular to it
    let along = if ox.abs() > 0.5 { b.w } else { b.h };
    let perp = if ox.abs() > 0.5 { b.h } else { b.w };
    let half = along / 2.0;

    // Connect point (lead start, at midpoint of the connect edge) / outer apex
    let cpx = cx - ox * half;
    let cpy = cy - oy * half;
    let apex_x = cx + ox * half;
    let apex_y = cy + oy * half;

    // Triangle base (from apex inward by tri_h)
    let tri_h = (along * 0.55).clamp(10.0, 18.0);
    let bcx = apex_x - ox * tri_h;
    let bcy = apex_y - oy * tri_h;
    let tw = (perp * 0.35).clamp(7.0, 11.0);
    let (b1x, b1y) = (bcx + tx * tw, bcy + ty * tw);
    let (b2x, b2y) = (bcx - tx * tw, bcy - ty * tw);

    let triangle = format!(
        r##"<polygon points="{apex_x:.1},{apex_y:.1} {b1x:.1},{b1y:.1} {b2x:.1},{b2y:.1}"
            fill="{POWER_COLOR}" stroke="{POWER_COLOR}" stroke-width="1.0"/>"##
    );

    // Lead: connect point → triangle base midpoint
    let lead = format!(
        r##"<line x1="{cpx:.1}" y1="{cpy:.1}" x2="{bcx:.1}" y2="{bcy:.1}"
            stroke="{POWER_COLOR}" stroke-width="1.5"/>"##
    );

    // Label: outside the apex
    let lx = apex_x + ox * 7.0;
    let ly = apex_y + oy * 9.0;
    let (anchor, baseline) = label_anchor_attrs(ox, oy);
    let label = format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="{a}" dominant-baseline="{bl}"
            font-size="11" font-weight="700" fill="{c}">{name}</text>"##,
        lx,
        ly,
        a = anchor,
        bl = baseline,
        c = POWER_COLOR,
        name = escape_xml(&b.name)
    );

    format!(
        r##"  <g class="comp power-rail" data-id="{id}">
    {triangle}
    {lead}
    {label}
  </g>
"##,
        id = b.id,
        triangle = triangle,
        lead = lead,
        label = label
    )
}

// ============================================================================
// GND / VSS style (3 decreasing horizontal bars on the outside)
// ============================================================================

fn render_ground(b: &McVecBox, connect: &EntrySide) -> String {
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;
    let (ox, oy) = glyph_outward(connect);
    let (tx, ty) = (-oy, ox);

    let along = if ox.abs() > 0.5 { b.w } else { b.h };
    let perp = if ox.abs() > 0.5 { b.h } else { b.w };
    let half = along / 2.0;

    // Connect point (lead start)
    let cpx = cx - ox * half;
    let cpy = cy - oy * half;

    // 3 horizontal bars: laid out from center toward outside; half-width decreasing
    let d1 = along * 0.05;
    let d2 = along * 0.22;
    let d3 = along * 0.39;
    let w1 = (perp * 0.35).clamp(10.0, 16.0);
    let w2 = w1 * 0.70;
    let w3 = w1 * 0.40;

    let bar = |d: f64, hw: f64| -> (f64, f64, f64, f64) {
        let bx = cx + ox * d;
        let by = cy + oy * d;
        (bx + tx * hw, by + ty * hw, bx - tx * hw, by - ty * hw)
    };
    let (l1x1, l1y1, l1x2, l1y2) = bar(d1, w1);
    let (l2x1, l2y1, l2x2, l2y2) = bar(d2, w2);
    let (l3x1, l3y1, l3x2, l3y2) = bar(d3, w3);

    // Lead: connect point → center of first bar
    let lead = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
            stroke="{c}" stroke-width="1.5"/>"##,
        cpx,
        cpy,
        cx + ox * d1,
        cy + oy * d1,
        c = GROUND_COLOR
    );
    let mk_line = |x1: f64, y1: f64, x2: f64, y2: f64| {
        format!(
            r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"
            stroke="{GROUND_COLOR}" stroke-width="2.0" stroke-linecap="square"/>"##
        )
    };
    let line1 = mk_line(l1x1, l1y1, l1x2, l1y2);
    let line2 = mk_line(l2x1, l2y1, l2x2, l2y2);
    let line3 = mk_line(l3x1, l3y1, l3x2, l3y2);

    // Label: further out from the outermost bar
    let lx = cx + ox * (d3 + 9.0);
    let ly = cy + oy * (d3 + 11.0);
    let (anchor, baseline) = label_anchor_attrs(ox, oy);
    let label = format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="{a}" dominant-baseline="{bl}"
            font-size="11" font-weight="700" fill="{c}">{name}</text>"##,
        lx,
        ly,
        a = anchor,
        bl = baseline,
        c = GROUND_COLOR,
        name = escape_xml(&b.name)
    );

    format!(
        r##"  <g class="comp power-rail ground" data-id="{id}">
    {lead}
    {line1}
    {line2}
    {line3}
    {label}
  </g>
"##,
        id = b.id,
        lead = lead,
        line1 = line1,
        line2 = line2,
        line3 = line3,
        label = label
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, EntryPoint, IoSummary};

    fn mk(is_ground: bool, name: &str, connect: EntrySide) -> McVecBox {
        let mut b = McVecBox::new_v2(
            9,
            name.into(),
            "".into(),
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground },
            None,
            None,
            1,
            IoSummary::new(),
        );
        b.x = 100.0;
        b.y = 30.0;
        b.w = 40.0;
        b.h = 40.0;
        b.entry_points.push(EntryPoint {
            pin_id: 9,
            pin_name: name.into(),
            side: connect,
            offset: 0.5,
        });
        b
    }

    #[test]
    fn power_up_when_connect_bottom() {
        let svg = PowerRailShape.render(&mk(false, "VCC", EntrySide::Bottom));
        assert!(svg.contains("<polygon"));
        assert!(svg.contains(">VCC</text>"));
    }

    #[test]
    fn power_side_when_connect_right() {
        // Connect on right → symbol points left, still renders (no panic, contains triangle + name)
        let svg = PowerRailShape.render(&mk(false, "V5V", EntrySide::Right));
        assert!(svg.contains("<polygon"));
        assert!(svg.contains(">V5V</text>"));
        assert!(svg.contains(r#"text-anchor="end""#)); // points left → label right-aligned
    }

    #[test]
    fn ground_has_three_bars_any_orientation() {
        for side in [EntrySide::Top, EntrySide::Left, EntrySide::Right] {
            let svg = PowerRailShape.render(&mk(true, "GND", side));
            assert!(svg.matches("<line").count() >= 4); // lead + 3 bars
            assert!(!svg.contains("<polygon"));
            assert!(svg.contains(">GND</text>"));
        }
    }

    #[test]
    fn fallback_no_entry_point() {
        // No entry_point → fall back to classic "points up", no panic
        let mut b = mk(false, "VDD", EntrySide::Bottom);
        b.entry_points.clear();
        let svg = PowerRailShape.render(&b);
        assert!(svg.contains(">VDD</text>"));
    }
}
