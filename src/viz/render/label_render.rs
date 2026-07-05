// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Designator + value label layout
//!
//! Outputs the SVG for a `McVecBox`'s designator (e.g. `R1`) and nominal value
//! (e.g. `10k`). The label position is decided based on the `Symbol` type:
//!
//! - **Two-pin passive parts** (R/C/L/D/Led/Zener): designator **above** the part, value **below**
//! - **IC**: designator inside the box, directly below the part name
//! - **Module**: same as IC
//! - **PowerRail / Unknown**: designator/value not shown (a PowerRail's own name serves as the label)

use crate::vector::graph::{McVecBox, Symbol};

const DESIGNATOR_FONT: f64 = 11.0;
const VALUE_FONT: f64 = 10.0;
const TEXT_WIDTH_FACTOR: f64 = 0.6;
const TEXT_HEIGHT_FACTOR: f64 = 1.2;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LabelBounds {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub inside_owner_box: bool,
}

// ============================================================================
// Main API
// ============================================================================

/// Output the SVG fragment for the designator + value
///
/// When both designator and value are empty, returns an empty string (draws nothing).
pub fn render_designator_and_value(b: &McVecBox) -> String {
    let designator = b.designator.as_deref().unwrap_or("");
    let value = b.value.as_deref().unwrap_or("");

    // Both empty → draw nothing
    if designator.is_empty() && value.is_empty() {
        return String::new();
    }

    let cx = b.x + b.w / 2.0;

    match b.symbol {
        Symbol::Resistor
        | Symbol::Capacitor
        | Symbol::PolarCapacitor
        | Symbol::Inductor
        | Symbol::Diode
        | Symbol::Led
        | Symbol::Zener => {
            // Two-pin part: designator above, value below
            let designator_y = b.y - 4.0;
            let value_y = b.y + b.h + 12.0;
            let mut out = String::new();
            if !designator.is_empty() {
                out.push_str(&format!(
                    r##"    <text class="designator" x="{cx:.1}" y="{dy:.1}"
            text-anchor="middle" font-size="11" font-weight="600"
            fill="#333">{d}</text>
"##,
                    cx = cx,
                    dy = designator_y,
                    d = escape_xml(designator)
                ));
            }
            if !value.is_empty() {
                out.push_str(&format!(
                    r##"    <text class="value" x="{cx:.1}" y="{vy:.1}"
            text-anchor="middle" font-size="10" fill="#666">{v}</text>
"##,
                    cx = cx,
                    vy = value_y,
                    v = escape_xml(value)
                ));
            }
            out
        }
        Symbol::Ic | Symbol::Module => {
            // IC / Module: designator inside the box, below the part name
            // (value is usually not drawn on an IC — IC has no "nominal value" concept)
            if designator.is_empty() {
                return String::new();
            }
            let dy = b.y + b.h / 2.0 + 14.0;
            format!(
                r##"    <text class="designator" x="{cx:.1}" y="{dy:.1}"
            text-anchor="middle" font-size="10" fill="#666"
            font-style="italic">{d}</text>
"##,
                cx = cx,
                dy = dy,
                d = escape_xml(designator)
            )
        }
        Symbol::PowerRail { .. } | Symbol::Dot | Symbol::Unknown => String::new(),
    }
}

/// Return approximate bounding boxes for labels produced by [`render_designator_and_value`].
/// This mirrors the SVG positions without changing rendering; metrics use it as a
/// deterministic readability proxy, not pixel-perfect text measurement.
pub(crate) fn designator_value_label_bounds(b: &McVecBox) -> Vec<LabelBounds> {
    let designator = b.designator.as_deref().unwrap_or("");
    let value = b.value.as_deref().unwrap_or("");
    if designator.is_empty() && value.is_empty() {
        return Vec::new();
    }

    let cx = b.x + b.w / 2.0;
    let mut out = Vec::new();
    match b.symbol {
        Symbol::Resistor
        | Symbol::Capacitor
        | Symbol::PolarCapacitor
        | Symbol::Inductor
        | Symbol::Diode
        | Symbol::Led
        | Symbol::Zener => {
            if !designator.is_empty() {
                out.push(label_bounds(
                    designator,
                    cx,
                    b.y - 4.0,
                    DESIGNATOR_FONT,
                    false,
                ));
            }
            if !value.is_empty() {
                out.push(label_bounds(value, cx, b.y + b.h + 12.0, VALUE_FONT, false));
            }
        }
        Symbol::Ic | Symbol::Module => {
            if !designator.is_empty() {
                out.push(label_bounds(
                    designator,
                    cx,
                    b.y + b.h / 2.0 + 14.0,
                    VALUE_FONT,
                    true,
                ));
            }
        }
        Symbol::PowerRail { .. } | Symbol::Dot | Symbol::Unknown => {}
    }
    out
}

fn label_bounds(
    text: &str,
    center_x: f64,
    baseline_y: f64,
    font_size: f64,
    inside_owner_box: bool,
) -> LabelBounds {
    let w = text.chars().count() as f64 * font_size * TEXT_WIDTH_FACTOR;
    let h = font_size * TEXT_HEIGHT_FACTOR;
    LabelBounds {
        text: text.to_string(),
        x: center_x - w / 2.0,
        y: baseline_y - h,
        w,
        h,
        inside_owner_box,
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary};

    fn mk_box(symbol: Symbol, designator: Option<&str>, value: Option<&str>) -> McVecBox {
        let mut b = McVecBox::new_v2(
            1,
            "X".into(),
            "".into(),
            BoxKind::TwoPin,
            symbol,
            designator.map(String::from),
            value.map(String::from),
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
    fn resistor_shows_both() {
        let b = mk_box(Symbol::Resistor, Some("R1"), Some("10k"));
        let svg = render_designator_and_value(&b);
        assert!(svg.contains(">R1</text>"));
        assert!(svg.contains(">10k</text>"));
    }

    #[test]
    fn capacitor_no_value_skipped() {
        let b = mk_box(Symbol::Capacitor, Some("C5"), None);
        let svg = render_designator_and_value(&b);
        assert!(svg.contains(">C5</text>"));
        assert!(!svg.contains("class=\"value\""));
    }

    #[test]
    fn ic_uses_one_line() {
        let b = mk_box(Symbol::Ic, Some("U3"), None);
        let svg = render_designator_and_value(&b);
        assert!(svg.contains(">U3</text>"));
    }

    #[test]
    fn powerrail_empty() {
        let b = mk_box(Symbol::PowerRail { is_ground: false }, None, None);
        assert_eq!(render_designator_and_value(&b), "");
    }

    #[test]
    fn nothing_filled_empty() {
        let b = mk_box(Symbol::Resistor, None, None);
        assert_eq!(render_designator_and_value(&b), "");
    }

    #[test]
    fn label_bounds_for_passive_designator_and_value() {
        let b = mk_box(Symbol::Resistor, Some("R1"), Some("10k"));
        let labels = designator_value_label_bounds(&b);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].text, "R1");
        assert!(labels[0].y < b.y);
        assert_eq!(labels[1].text, "10k");
        assert!(labels[1].y + labels[1].h > b.y + b.h);
        assert!(!labels[0].inside_owner_box);
    }

    #[test]
    fn label_bounds_for_ic_inside_box() {
        let b = mk_box(Symbol::Ic, Some("U3"), None);
        let labels = designator_value_label_bounds(&b);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "U3");
        assert!(labels[0].inside_owner_box);
        assert!(labels[0].y >= b.y);
        assert!(labels[0].y + labels[0].h <= b.y + b.h + 1.0);
    }

    #[test]
    fn xml_escaping() {
        let b = mk_box(Symbol::Resistor, Some("R<1>"), Some("100Ω"));
        let svg = render_designator_and_value(&b);
        assert!(svg.contains(">R&lt;1&gt;</text>"));
        assert!(svg.contains(">100Ω</text>")); // non-ASCII does not need escaping
    }
}
