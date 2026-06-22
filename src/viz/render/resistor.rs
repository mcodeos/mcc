// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Resistor symbol render —— zigzag (IEEE style)
//!
//! Output SVG:
//! ```text
//!     R1
//!  ───/\/\/\/\───
//!     10k
//! ```
//!
//! Part is drawn **horizontally** (assumes both leads are at the left/right ends; that is the
//! layout's responsibility and not handled here). The two end leads meet the physical position
//! of entry_points (if any), so the router's wires connect seamlessly.

use crate::vector::graph::McVecBox;

use super::label_render::render_designator_and_value;
use super::shape::BoxShape;

pub struct ResistorShape;

impl BoxShape for ResistorShape {
    fn render(&self, b: &McVecBox) -> String {
        let stamp = render_designator_and_value(b);
        // ★ Vertical (h>w): use concentric "virtual horizontal box" to draw the symbol (reusing horizontal shape math), then rotate the whole thing 90° around the center to convert to vertical; label is drawn separately and not rotated (otherwise text would lie on its side).
        let symbol = if b.h > b.w {
            let cx = b.x + b.w / 2.0;
            let cy = b.y + b.h / 2.0;
            let vb = vertical_virtual_box(b);
            format!(
                "<g transform=\"rotate(90 {cx:.1} {cy:.1})\">\n{sym}\n    </g>",
                cx = cx,
                cy = cy,
                sym = resistor_symbol(&vb)
            )
        } else {
            resistor_symbol(b)
        };
        format!(
            r##"  <g class="comp resistor" data-id="{id}">
{symbol}
{stamp}  </g>
"##,
            id = b.id,
            symbol = symbol,
            stamp = stamp,
        )
    }
}

/// Concentric "virtual horizontal box": swap width and height (long edge to x), center unchanged. Vertical parts are drawn horizontally first then rotated 90°.
pub(crate) fn vertical_virtual_box(b: &McVecBox) -> McVecBox {
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;
    let mut vb = b.clone();
    vb.w = b.h;
    vb.h = b.w;
    vb.x = cx - vb.w / 2.0;
    vb.y = cy - vb.h / 2.0;
    vb
}

/// Resistor symbol (two leads + zigzag, excluding label / outer g), drawn horizontally.
fn resistor_symbol(b: &McVecBox) -> String {
    let cy = b.y + b.h / 2.0;
    let body_x_start = b.x + 6.0;
    let body_x_end = b.x + b.w - 6.0;

    // Zigzag: 6 segments, alternating up/down
    let zig_count = 6;
    let zig_w = (body_x_end - body_x_start) / zig_count as f64;
    let amp = (b.h * 0.32).min(8.0); // Zigzag amplitude, not exceeding 8px

    let mut d = format!("M {body_x_start:.1} {cy:.1}");
    for i in 0..zig_count {
        let x = body_x_start + (i as f64 + 0.5) * zig_w;
        let y = if i % 2 == 0 { cy - amp } else { cy + amp };
        d.push_str(&format!(" L {x:.1} {y:.1}"));
    }
    d.push_str(&format!(" L {body_x_end:.1} {cy:.1}"));

    // Short leads at both ends (for the router to attach to)
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

    let zigzag = format!(
        r##"<path d="{d}" stroke="#222" stroke-width="1.3" fill="none"
                stroke-linejoin="miter"/>"##
    );

    format!("    {lead_left}\n    {lead_right}\n    {zigzag}")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary, Symbol};

    fn mk_resistor() -> McVecBox {
        let mut b = McVecBox::new_v2(
            42,
            "R1".into(),
            "RES".into(),
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some("R1".into()),
            Some("10k".into()),
            2,
            IoSummary::new(),
        );
        b.x = 100.0;
        b.y = 50.0;
        b.w = 90.0;
        b.h = 30.0;
        b
    }

    #[test]
    fn renders_zigzag_path() {
        let svg = ResistorShape.render(&mk_resistor());
        // The zigzag is a <path>
        assert!(svg.contains("<path d=\"M"));
        // Zigzag should contain multiple L (line-to) commands
        let l_count = svg.matches(" L ").count();
        assert!(l_count >= 6, "expected ≥6 L commands, got {}", l_count);
    }

    #[test]
    fn shows_designator_and_value() {
        let svg = ResistorShape.render(&mk_resistor());
        assert!(svg.contains(">R1</text>"));
        assert!(svg.contains(">10k</text>"));
    }

    #[test]
    fn has_data_id() {
        let svg = ResistorShape.render(&mk_resistor());
        assert!(svg.contains(r#"data-id="42""#));
        assert!(svg.contains(r#"class="comp resistor""#));
    }

    #[test]
    fn no_rect_body() {
        // Previously was <rect>; after the change there should be none
        let svg = ResistorShape.render(&mk_resistor());
        assert!(
            !svg.contains("<rect"),
            "resistor body should be path, not rect"
        );
    }
}
