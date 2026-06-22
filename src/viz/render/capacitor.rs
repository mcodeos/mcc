// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Capacitor symbol render — two parallel short vertical lines
//!
//! Output SVG:
//! ```text
//!     C5
//!  ──┤├──
//!    100nF
//! ```
//!
//! Normal capacitor: two equal-length vertical lines
//! Polar capacitor: one straight + one arc (this phase uses the same two lines first, refine later at end of P05)

use crate::vector::graph::{McVecBox, Symbol};

use super::label_render::render_designator_and_value;
use super::shape::BoxShape;

pub struct CapacitorShape;

impl BoxShape for CapacitorShape {
    fn render(&self, b: &McVecBox) -> String {
        let stamp = render_designator_and_value(b);
        let css_class = if matches!(b.symbol, Symbol::PolarCapacitor) {
            "comp capacitor polar"
        } else {
            "comp capacitor"
        };
        // ★ Vertical (h>w): draw symbol with virtual horizontal box + rotate(90); label does not rotate.
        let symbol = if b.h > b.w {
            let cx = b.x + b.w / 2.0;
            let cy = b.y + b.h / 2.0;
            let vb = vertical_virtual_box(b);
            format!(
                "<g transform=\"rotate(90 {cx:.1} {cy:.1})\">\n{sym}\n    </g>",
                cx = cx,
                cy = cy,
                sym = capacitor_symbol(&vb)
            )
        } else {
            capacitor_symbol(b)
        };
        format!(
            r##"  <g class="{cls}" data-id="{id}">
{symbol}
{stamp}  </g>
"##,
            cls = css_class,
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

/// Capacitor symbol (two leads + two plates + polarity mark, excluding label / outer g), drawn horizontally.
fn capacitor_symbol(b: &McVecBox) -> String {
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;

    // Gap between the two short vertical lines
    let plate_gap = 6.0;
    let plate_h = (b.h * 0.55).min(16.0);

    let left_plate_x = cx - plate_gap / 2.0;
    let right_plate_x = cx + plate_gap / 2.0;

    // Left plate
    let left_plate = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="2.2" stroke-linecap="square"/>"##,
        left_plate_x,
        cy - plate_h / 2.0,
        left_plate_x,
        cy + plate_h / 2.0
    );

    // Right plate: polar capacitor uses arc, normal capacitor uses line
    let right_plate = if matches!(b.symbol, Symbol::PolarCapacitor) {
        let r = plate_h * 0.8;
        format!(
            r##"<path d="M {:.1} {:.1} A {:.1} {:.1} 0 0 1 {:.1} {:.1}"
                    stroke="#222" stroke-width="2.0" fill="none"/>"##,
            right_plate_x,
            cy - plate_h / 2.0,
            r,
            r,
            right_plate_x,
            cy + plate_h / 2.0
        )
    } else {
        format!(
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                    stroke="#222" stroke-width="2.2" stroke-linecap="square"/>"##,
            right_plate_x,
            cy - plate_h / 2.0,
            right_plate_x,
            cy + plate_h / 2.0
        )
    };

    // Leads on both sides
    let lead_left = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="1.3"/>"##,
        b.x, cy, left_plate_x, cy
    );
    let lead_right = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="1.3"/>"##,
        right_plate_x,
        cy,
        b.x + b.w,
        cy
    );

    // Polar capacitor: draw a "+" above the top of the left plate (marks anode)
    let polarity_mark = if matches!(b.symbol, Symbol::PolarCapacitor) {
        format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle"
                    font-size="10" font-weight="700" fill="#222">+</text>"##,
            left_plate_x - 4.0,
            cy - plate_h / 2.0 - 4.0
        )
    } else {
        String::new()
    };

    format!(
        "    {lead_left}\n    {lead_right}\n    {left_plate}\n    {right_plate}\n    {polarity_mark}",
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary};

    fn mk_cap(symbol: Symbol, designator: Option<&str>, value: Option<&str>) -> McVecBox {
        let mut b = McVecBox::new_v2(
            100,
            "C1".into(),
            "CAP".into(),
            BoxKind::TwoPin,
            symbol,
            designator.map(String::from),
            value.map(String::from),
            2,
            IoSummary::new(),
        );
        b.x = 50.0;
        b.y = 60.0;
        b.w = 80.0;
        b.h = 30.0;
        b
    }

    #[test]
    fn normal_cap_two_lines() {
        let b = mk_cap(Symbol::Capacitor, Some("C1"), Some("100nF"));
        let svg = CapacitorShape.render(&b);
        // Two plates + two leads = 4 <line> elements
        assert_eq!(svg.matches("<line").count(), 4);
        assert!(svg.contains(">C1</text>"));
        assert!(svg.contains(">100nF</text>"));
        // Normal capacitor should not have an arc
        assert!(!svg.contains("<path"));
        assert!(!svg.contains(">+<"));
    }

    #[test]
    fn polar_cap_has_arc_and_plus() {
        let b = mk_cap(Symbol::PolarCapacitor, Some("C2"), Some("47uF"));
        let svg = CapacitorShape.render(&b);
        // One left plate + two leads = 3 <line> elements, the arc is <path>
        assert!(svg.contains("<path d=\"M"));
        // Positive marker "+"
        assert!(svg.contains(">+</text>"));
        // CSS marker
        assert!(svg.contains(r#"class="comp capacitor polar""#));
    }

    #[test]
    fn no_rect_body() {
        let b = mk_cap(Symbol::Capacitor, Some("C1"), None);
        let svg = CapacitorShape.render(&b);
        assert!(!svg.contains("<rect"));
    }
}
