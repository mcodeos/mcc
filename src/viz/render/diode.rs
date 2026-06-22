// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Diode family symbol render
//!
//! One Shape handles three Symbols:
//! - `Diode` —— triangle + bar (anode → cathode)
//! - `Led`   —— diode + two outward arrows (light emission)
//! - `Zener` —— diode + bent hooks at both ends of the cathode line (voltage regulator)
//!
//! Output SVG (Diode):
//! ```text
//!     D1
//!  ────▷|────
//! ```
//! Vertical (h>w): use the concentric virtual horizontal box to draw the symbol, then rotate(90); the label does not rotate.

use crate::vector::graph::{McVecBox, Symbol};

use super::label_render::render_designator_and_value;
use super::shape::BoxShape;

/// Diode family (Diode / Led / Zener) shared render
pub struct DiodeShape;

impl BoxShape for DiodeShape {
    fn render(&self, b: &McVecBox) -> String {
        let stamp = render_designator_and_value(b);
        let (symbol, css_extra) = if b.h > b.w {
            let cx = b.x + b.w / 2.0;
            let cy = b.y + b.h / 2.0;
            let vb = vertical_virtual_box(b);
            let (sym, css_extra) = diode_symbol(&vb);
            (
                format!("<g transform=\"rotate(90 {cx:.1} {cy:.1})\">\n{sym}\n    </g>"),
                css_extra,
            )
        } else {
            diode_symbol(b)
        };
        format!(
            r##"  <g class="comp diode{css_extra}" data-id="{id}">
{symbol}
{stamp}  </g>
"##,
            css_extra = css_extra,
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

/// Diode symbol (lead + triangle + cathode bar + variant decoration, excluding label / outer g), drawn horizontally.
/// Returns (symbol SVG, css suffix).
fn diode_symbol(b: &McVecBox) -> (String, &'static str) {
    let cy = b.y + b.h / 2.0;
    let body_x_start = b.x + 6.0;
    let body_x_end = b.x + b.w - 6.0;
    let body_w = body_x_end - body_x_start;

    // Triangle center
    let tri_w = (body_w * 0.55).min(20.0);
    let tri_h = (b.h * 0.55).min(16.0);
    let tri_apex_x = body_x_start + (body_w - tri_w) / 2.0 + tri_w;
    let tri_left_x = tri_apex_x - tri_w;

    // Triangle (▷): two points on the left (top, bottom) → tip on the right
    let triangle = format!(
        r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}"
                fill="#222" stroke="#222" stroke-width="1.0"/>"##,
        tri_left_x,
        cy - tri_h / 2.0,
        tri_left_x,
        cy + tri_h / 2.0,
        tri_apex_x,
        cy
    );

    // Cathode bar (to the right of the triangle tip)
    let cathode_x = tri_apex_x;
    let cathode_h = tri_h;
    let cathode_line = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="2.0" stroke-linecap="square"/>"##,
        cathode_x,
        cy - cathode_h / 2.0,
        cathode_x,
        cy + cathode_h / 2.0
    );

    // Leads
    let lead_left = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="1.3"/>"##,
        b.x, cy, tri_left_x, cy
    );
    let lead_right = format!(
        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                stroke="#222" stroke-width="1.3"/>"##,
        cathode_x,
        cy,
        b.x + b.w,
        cy
    );

    // Variant decoration
    let (variant_decor, css_extra) = match b.symbol {
        Symbol::Led => {
            let arrow_start_x = tri_apex_x - tri_w * 0.3;
            let arrow_start_y = cy - tri_h / 2.0 - 2.0;
            let svg = format!(
                r##"<g stroke="#C0392B" stroke-width="1.2" fill="none">
        <line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"/>
        <polyline points="{ax:.1},{ay:.1} {bx:.1},{by:.1} {cx:.1},{cy:.1}"/>
        <line x1="{x3:.1}" y1="{y3:.1}" x2="{x4:.1}" y2="{y4:.1}"/>
        <polyline points="{ax2:.1},{ay2:.1} {bx2:.1},{by2:.1} {cx2:.1},{cy2:.1}"/>
    </g>"##,
                x1 = arrow_start_x,
                y1 = arrow_start_y,
                x2 = arrow_start_x + 6.0,
                y2 = arrow_start_y - 8.0,
                ax = arrow_start_x + 4.0,
                ay = arrow_start_y - 6.0,
                bx = arrow_start_x + 6.0,
                by = arrow_start_y - 8.0,
                cx = arrow_start_x + 4.5,
                cy = arrow_start_y - 4.5,
                x3 = arrow_start_x + 4.0,
                y3 = arrow_start_y,
                x4 = arrow_start_x + 10.0,
                y4 = arrow_start_y - 8.0,
                ax2 = arrow_start_x + 8.0,
                ay2 = arrow_start_y - 6.0,
                bx2 = arrow_start_x + 10.0,
                by2 = arrow_start_y - 8.0,
                cx2 = arrow_start_x + 8.5,
                cy2 = arrow_start_y - 4.5,
            );
            (svg, " led")
        }
        Symbol::Zener => {
            let hook_len = 4.0;
            let top = cy - cathode_h / 2.0;
            let bot = cy + cathode_h / 2.0;
            let svg = format!(
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                    stroke="#222" stroke-width="1.5"/>
    <line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"
                    stroke="#222" stroke-width="1.5"/>"##,
                cathode_x,
                top,
                cathode_x - hook_len,
                top - hook_len * 0.5,
                cathode_x,
                bot,
                cathode_x + hook_len,
                bot + hook_len * 0.5,
            );
            (svg, " zener")
        }
        _ => (String::new(), ""),
    };

    let sym = format!(
        "    {lead_left}\n    {lead_right}\n    {triangle}\n    {cathode_line}\n    {variant_decor}",
    );
    (sym, css_extra)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary};

    fn mk(symbol: Symbol) -> McVecBox {
        let mut b = McVecBox::new_v2(
            7,
            "D1".into(),
            "DIODE".into(),
            BoxKind::TwoPin,
            symbol,
            Some("D1".into()),
            None,
            2,
            IoSummary::new(),
        );
        b.x = 50.0;
        b.y = 50.0;
        b.w = 80.0;
        b.h = 30.0;
        b
    }

    #[test]
    fn diode_basic() {
        let svg = DiodeShape.render(&mk(Symbol::Diode));
        assert!(svg.contains("<polygon"));
        assert!(svg.contains(">D1</text>"));
        assert!(!svg.contains("class=\"comp diode led\""));
        assert!(!svg.contains("class=\"comp diode zener\""));
    }

    #[test]
    fn led_has_arrows() {
        let svg = DiodeShape.render(&mk(Symbol::Led));
        assert!(svg.matches("<polyline").count() >= 2);
        assert!(svg.contains(r#"class="comp diode led""#));
    }

    #[test]
    fn zener_has_hooks() {
        let svg = DiodeShape.render(&mk(Symbol::Zener));
        assert!(svg.contains(r#"class="comp diode zener""#));
    }
}
