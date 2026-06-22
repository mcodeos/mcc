// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `BoxShape` trait + dispatch
//!
//! ## ★ P05 (S3) change
//! Dispatch logic changed from `b.kind` to **`b.symbol`** (the Symbol field is filled in after P01/S2):
//!
//! - `Symbol::Resistor`        → [`super::resistor::ResistorShape`] (zigzag)
//! - `Symbol::Capacitor`       → [`super::capacitor::CapacitorShape`] (parallel lines)
//! - `Symbol::PolarCapacitor`  → same as above, but the right plate becomes an arc + adds "+" polarity
//! - `Symbol::Inductor`        → [`super::inductor::InductorShape`] (half-circle wave)
//! - `Symbol::Diode`/`Led`/`Zener` → [`super::diode::DiodeShape`] (triangle + bar)
//! - `Symbol::Ic`              → [`super::ic::IcShape`] (P05 new pin labels)
//! - `Symbol::Module`          → [`super::sub_module::render_sub_module`] (existing)
//! - `Symbol::PowerRail{..}`   → [`super::power_rail::PowerRailShape`] (P05 real triangle / 3-bar GND)
//! - `Symbol::Unknown`         → fall back to old 4 shapes by `b.kind`
//!
//! Old shapes (`two_pin.rs` / `multi_pin.rs` / `power_label.rs`) are only used when falling
//! back for `Symbol::Unknown` — any properly recognized box goes through the new shapes.

use crate::vector::graph::{BoxKind, McVecBox, Symbol};

use super::capacitor::CapacitorShape;
use super::diode::DiodeShape;
use super::ic::IcShape;
use super::inductor::InductorShape;
use super::multi_pin::MultiPinShape;
use super::power_label::PowerLabelShape;
use super::power_rail::PowerRailShape;
use super::resistor::ResistorShape;
use super::sub_module::render_sub_module;
use super::two_pin::TwoPinShape;

// ============================================================================
// trait
// ============================================================================

/// Render strategy for a single box
pub trait BoxShape {
    /// Output the SVG `<g>` element (includes positioning / styling / text)
    fn render(&self, b: &McVecBox) -> String;
}

// ============================================================================
// Dispatch (★ P05: by Symbol)
// ============================================================================

/// Pick the corresponding Shape implementation by `b.symbol`
///
/// `Symbol::Unknown` falls back to old shapes by `b.kind` (compatible with historical fixtures).
pub fn render_box(b: &McVecBox) -> String {
    // ★ Reserved interface ②: if the user has uploaded a custom symbol for this part, use it;
    //   otherwise use the system one. Today the builder does not fill custom_symbol (always None)
    //   → always the system symbol, behavior unchanged.
    if let Some(cs) = &b.custom_symbol {
        return render_custom_symbol(b, cs);
    }
    match b.symbol {
        Symbol::Resistor => ResistorShape.render(b),
        Symbol::Capacitor | Symbol::PolarCapacitor => CapacitorShape.render(b),
        Symbol::Inductor => InductorShape.render(b),
        Symbol::Diode | Symbol::Led | Symbol::Zener => DiodeShape.render(b),
        Symbol::Ic => IcShape.render(b),
        Symbol::Module => render_sub_module(b),
        Symbol::PowerRail { .. } => PowerRailShape.render(b),
        Symbol::Unknown => {
            // ★ FIX: Unknown boxes with pin information (including Phase F.1 typed-chip placeholder
            //   pins / normal part real pins) now use IcShape to draw pin + number + name; only
            //   ones with no pins at all fall back to the old plain rectangle shape
            //   (TwoPin/MultiPin only draws the frame + name). PowerLabel / SubModule still
            //   use their own fallbacks, not IcShape.
            let has_pins = !b.entry_points.is_empty();
            let pin_kind = matches!(b.kind, BoxKind::TwoPin | BoxKind::MultiPin);
            if has_pins && pin_kind {
                IcShape.render(b)
            } else {
                render_box_legacy(b)
            }
        }
    }
}

/// ★ Reserved interface ② consumer: translate the user custom symbol `svg_body` to the box
/// position, then overlay the pins.
///
/// `svg_body` is drawn in the box's own reference frame (0,0 ~ b.w×b.h); this function does
/// the overall translate to (b.x, b.y). The pin marker (stub + number + function name + IO
/// arrow) is still drawn by `pin_render` based on entry_points, consistent with system
/// symbols — the custom symbol only changes the "part body" appearance, not the pins.
fn render_custom_symbol(b: &McVecBox, cs: &crate::vector::graph::box_def::CustomSymbol) -> String {
    use super::pin_render::{render_pin, PinRenderOpts};
    let pins: String = b
        .entry_points
        .iter()
        .map(|ep| render_pin(b, ep, PinRenderOpts::for_ic()))
        .collect();
    format!(
        r##"  <g class="comp custom" data-id="{id}" data-symbol-source="{src}">
    <g transform="translate({x:.1},{y:.1})">{body}</g>
{pins}  </g>
"##,
        id = b.id,
        src = escape_xml_attr(&cs.source),
        x = b.x,
        y = b.y,
        body = cs.svg_body,
        pins = pins,
    )
}

fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Pre-P05 dispatch logic (by BoxKind), now used as the `Symbol::Unknown` fallback
fn render_box_legacy(b: &McVecBox) -> String {
    match b.kind {
        BoxKind::TwoPin => TwoPinShape.render(b),
        BoxKind::MultiPin => MultiPinShape.render(b),
        BoxKind::SubModule => render_sub_module(b),
        BoxKind::PowerLabel => PowerLabelShape.render(b),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::CustomSymbol;
    use crate::vector::graph::IoSummary;

    fn mk(symbol: Symbol, kind: BoxKind) -> McVecBox {
        let mut b = McVecBox::new_v2(
            7,
            "u1".into(),
            "MyR".into(),
            kind,
            symbol,
            None,
            None,
            2,
            IoSummary::new(),
        );
        b.x = 10.0;
        b.y = 20.0;
        b.w = 40.0;
        b.h = 16.0;
        b
    }

    #[test]
    fn custom_symbol_overrides_system() {
        let mut b = mk(Symbol::Resistor, BoxKind::TwoPin);
        b.set_custom_symbol(CustomSymbol {
            source: "MyR".into(),
            svg_body: r#"<rect class="my-sym" width="40" height="16"/>"#.into(),
        });
        let svg = render_box(&b);
        // Goes through the custom symbol, no longer the system resistor zigzag
        assert!(svg.contains(r#"class="comp custom""#));
        assert!(svg.contains(r#"data-symbol-source="MyR""#));
        assert!(svg.contains(r#"class="my-sym""#));
        assert!(svg.contains("translate(10.0,20.0)"));
    }

    #[test]
    fn no_custom_symbol_uses_system() {
        let b = mk(Symbol::Resistor, BoxKind::TwoPin);
        let svg = render_box(&b);
        assert!(!svg.contains(r#"class="comp custom""#));
    }

    #[test]
    fn custom_symbol_source_is_escaped() {
        let mut b = mk(Symbol::Ic, BoxKind::MultiPin);
        b.set_custom_symbol(CustomSymbol {
            source: r#"a"<&>"#.into(),
            svg_body: "<g/>".into(),
        });
        let svg = render_box(&b);
        assert!(svg.contains("a&quot;&lt;&amp;&gt;"));
    }
}
