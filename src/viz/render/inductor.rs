// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Inductor symbol render —— half-circle wave (IEEE style)
//!
//! Output SVG: a series of upward convex half-circle arcs (traditional "⌒⌒⌒⌒" inductor symbol)
//!
//! ```text
//!      L1
//!  ───⌒⌒⌒⌒───
//!     4.7uH
//! ```

use crate::vector::graph::McVecBox;

use super::label_render::render_designator_and_value;
use super::shape::BoxShape;

pub struct InductorShape;

impl BoxShape for InductorShape {
    fn render(&self, b: &McVecBox) -> String {
        let stamp = render_designator_and_value(b);
        let symbol = if b.h > b.w {
            let cx = b.x + b.w / 2.0;
            let cy = b.y + b.h / 2.0;
            let vb = vertical_virtual_box(b);
            format!(
                "<g transform=\"rotate(90 {cx:.1} {cy:.1})\">\n{sym}\n    </g>",
                cx = cx,
                cy = cy,
                sym = inductor_symbol(&vb)
            )
        } else {
            inductor_symbol(b)
        };
        format!(
            r##"  <g class="comp inductor" data-id="{id}">
{symbol}
{stamp}  </g>
"##,
            id = b.id,
            symbol = symbol,
            stamp = stamp,
        )
    }
}

/// Concentric "virtual horizontal box": swap width and height, center unchanged. Vertical parts are drawn horizontally first then rotated 90°.
fn vertical_virtual_box(b: &McVecBox) -> McVecBox {
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;
    let mut vb = b.clone();
    vb.w = b.h;
    vb.h = b.w;
    vb.x = cx - vb.w / 2.0;
    vb.y = cy - vb.h / 2.0;
    vb
}

/// Inductor symbol (two leads + half-circle wave, excluding label / outer g), drawn horizontally.
fn inductor_symbol(b: &McVecBox) -> String {
    let cy = b.y + b.h / 2.0;
    let body_x_start = b.x + 6.0;
    let body_x_end = b.x + b.w - 6.0;
    let body_w = body_x_end - body_x_start;

    // 4 half circles
    let n_arcs = 4;
    let arc_w = body_w / n_arcs as f64;
    let arc_r = arc_w / 2.0;

    let mut d = format!("M {body_x_start:.1} {cy:.1}");
    for _ in 0..n_arcs {
        d.push_str(&format!(" a {arc_r:.1} {arc_r:.1} 0 0 1 {arc_w:.1} 0"));
    }

    let arcs = format!(r##"<path d="{d}" stroke="#222" stroke-width="1.4" fill="none"/>"##);

    let lead_left = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="1.3"/>"##,
        b.x, cy, body_x_start, cy
    );
    let lead_right = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="1.3"/>"##,
        body_x_end,
        cy,
        b.x + b.w,
        cy
    );

    format!("    {lead_left}\n    {lead_right}\n    {arcs}")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary, Symbol};

    fn mk_inductor() -> McVecBox {
        let mut b = McVecBox::new_v2(
            55,
            "L1".into(),
            "IND".into(),
            BoxKind::TwoPin,
            Symbol::Inductor,
            Some("L1".into()),
            Some("4.7uH".into()),
            2,
            IoSummary::new(),
        );
        b.x = 100.0;
        b.y = 50.0;
        b.w = 100.0;
        b.h = 30.0;
        b
    }

    #[test]
    fn has_arc_path() {
        let svg = InductorShape.render(&mk_inductor());
        // 4 "a" commands (SVG relative arcs)
        let a_count = svg.matches(" a ").count();
        assert!(a_count >= 4, "expected ≥4 arc commands, got {}", a_count);
    }

    #[test]
    fn has_labels() {
        let svg = InductorShape.render(&mk_inductor());
        assert!(svg.contains(">L1</text>"));
        assert!(svg.contains(">4.7uH</text>"));
    }

    #[test]
    fn no_rect_body() {
        let svg = InductorShape.render(&mk_inductor());
        assert!(!svg.contains("<rect"));
    }
}
