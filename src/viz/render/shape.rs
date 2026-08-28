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
use super::label_render::display_name;
use super::multi_pin::MultiPinShape;
use super::power_label::PowerLabelShape;
use super::power_rail::PowerRailShape;
use super::resistor::ResistorShape;
use super::sub_module::{render_sub_module, render_sub_module_root};
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
///
/// When `is_root` is true, sub-module boxes use root layer block-diagram styling
/// (solid lines, centered name, no + corner).
pub fn render_box(b: &McVecBox, is_root: bool) -> String {
    // Root (block) layer: user-provided custom symbols take priority over the
    // P9-B block-diagram fallback (fixture: svg_symbol_project expects J1/J2 on
    // the root).
    if is_root {
        if let Some(cs) = &b.custom_symbol {
            return render_custom_symbol(b, cs);
        }
        return render_sub_module_root(b);
    }
    // Device layers: the symbol is determined by device category, not by a
    // custom SVG from the manifest (R-S discipline 26) — usbsock & co. render as
    // their rectangular multi-pin device, not as a pictographic icon.
    match b.symbol {
        Symbol::Resistor => ResistorShape.render(b),
        Symbol::Capacitor | Symbol::PolarCapacitor => CapacitorShape.render(b),
        Symbol::Inductor => InductorShape.render(b),
        Symbol::Diode | Symbol::Led | Symbol::Zener => DiodeShape.render(b),
        Symbol::Ic => IcShape.render(b),
        Symbol::Module => {
            if is_root {
                render_sub_module_root(b)
            } else {
                render_sub_module(b)
            }
        }
        Symbol::PowerRail { .. } => PowerRailShape.render(b),
        Symbol::Dot => render_dot_symbol(b),
        Symbol::TestPoint => render_test_point(b),
        Symbol::PortTerminal { .. } => {
            // PortTerminal: small terminal at canvas edge, similar to PowerLabel but with port name
            let cx = b.x + b.w / 2.0;
            let cy = b.y + b.h / 2.0;
            format!(
                r##"    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}"
            fill="none" stroke="#888" stroke-width="1.5" rx="2" ry="2"/>
    <text x="{cx:.1}" y="{cy:.1}" text-anchor="middle"
            font-size="9" fill="#666" dominant-baseline="central">{name}</text>
"##,
                x = b.x,
                y = b.y,
                w = b.w,
                h = b.h,
                cx = cx,
                cy = cy,
                name = escape_xml_attr(box_name_label(b)),
            )
        }
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
                render_box_legacy(b, is_root)
            }
        }
    }
}

/// ★ Reserved interface ② consumer: scale the validated custom symbol into the box, then
/// overlay the pins.
///
/// The source `viewBox` is mapped into the component body using SVG's `xMidYMid meet` behavior.
/// The pin marker (stub + number + function name + IO arrow) is still drawn by `pin_render`
/// based on entry_points, consistent with system symbols. The custom symbol only changes the
/// component body, not electrical anchors.
fn render_custom_symbol(b: &McVecBox, cs: &crate::vector::graph::boxdef::CustomSymbol) -> String {
    use super::pin_render::{render_pin, PinRenderOpts};
    let pins: String = b
        .entry_points
        .iter()
        .map(|ep| render_pin(b, ep, PinRenderOpts::for_ic()))
        .collect();
    format!(
        r##"  <g class="comp custom" data-id="{id}" data-symbol-source="{src}">
    <svg x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}"
         viewBox="{vx:.3} {vy:.3} {vw:.3} {vh:.3}"
         preserveAspectRatio="xMidYMid meet" overflow="hidden">{body}</svg>
{pins}  </g>
"##,
        id = b.id,
        src = escape_xml_attr(&cs.source),
        x = b.x,
        y = b.y,
        w = b.w,
        h = b.h,
        vx = cs.view_box.min_x,
        vy = cs.view_box.min_y,
        vw = cs.view_box.width,
        vh = cs.view_box.height,
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

/// Pick the visible label for a box's name slot (mcd docs-mc 16-export-viz §6).
/// Virtual instantiation views must not leak the fabricated instance name
/// (`u_1`) into the schematic — the box then shows its class name instead.
/// Mirrors the `suppress_instance_name` convention in ic.rs / multi_pin.rs /
/// two_pin.rs. Non-virtual boxes keep their instance name verbatim.
fn box_name_label(b: &McVecBox) -> &str {
    if b.suppress_instance_name {
        if b.class_name.is_empty() {
            ""
        } else {
            display_name(&b.class_name)
        }
    } else {
        &b.name
    }
}

/// Render a Dot label box — a small filled circle with the label text
/// ★ C1b: Test point symbol — small circle pad with designator label
fn render_test_point(b: &McVecBox) -> String {
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;
    // Virtual instantiation view suppresses the fabricated `u_1` name; the box
    // shows its class name (e.g. `TP`) instead.
    let label = box_name_label(b);
    format!(
        r##"  <g class="comp testpoint" data-id="{id}">
    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}"
          fill="#fafafa" stroke="#222" stroke-width="1.5" rx="2" ry="2"/>
    <text x="{cx:.1}" y="{cy:.1}" text-anchor="middle"
          font-size="10" fill="#222" dominant-baseline="central">{name}</text>
  </g>
"##,
        id = b.id,
        x = b.x,
        y = b.y,
        w = b.w,
        h = b.h,
        cx = cx,
        cy = cy,
        name = escape_xml_attr(label),
    )
}

fn render_dot_symbol(b: &McVecBox) -> String {
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;
    let r = b.w.min(b.h) / 2.0 * 0.7;
    let label_x = cx + r + 6.0;
    format!(
        r##"  <g class="comp dot" data-id="{id}">
    <circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="#333" stroke="none"/>
    <text x="{lx:.1}" y="{cy:.1}" font-size="10" fill="#333"
          dominant-baseline="central">{name}</text>
  </g>
"##,
        id = b.id,
        cx = cx,
        cy = cy,
        r = r,
        lx = label_x,
        name = escape_xml_attr(box_name_label(b))
    )
}

/// Pre-P05 dispatch logic (by BoxKind), now used as the `Symbol::Unknown` fallback
fn render_box_legacy(b: &McVecBox, is_root: bool) -> String {
    match b.kind {
        BoxKind::TwoPin => TwoPinShape.render(b),
        BoxKind::MultiPin => MultiPinShape.render(b),
        BoxKind::SubModule => {
            if is_root {
                render_sub_module_root(b)
            } else {
                render_sub_module(b)
            }
        }
        BoxKind::PowerLabel => PowerLabelShape.render(b),
        BoxKind::Dot => render_dot_symbol(b),
        BoxKind::PortTerminal => {
            // PortTerminal fallback: small rectangle with port name
            let cx = b.x + b.w / 2.0;
            let cy = b.y + b.h / 2.0;
            format!(
                r##"    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}"
            fill="none" stroke="#888" stroke-width="1.5" rx="2" ry="2"/>
    <text x="{cx:.1}" y="{cy:.1}" text-anchor="middle"
            font-size="9" fill="#666" dominant-baseline="central">{name}</text>
"##,
                x = b.x,
                y = b.y,
                w = b.w,
                h = b.h,
                cx = cx,
                cy = cy,
                name = escape_xml_attr(box_name_label(b)),
            )
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::CustomSymbol;
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
            "u1".to_string(),
            Vec::new(),
        );
        b.x = 10.0;
        b.y = 20.0;
        b.w = 40.0;
        b.h = 16.0;
        b
    }

    /// ★ R-S (discipline 26): custom_symbol does NOT override system symbol.
    /// A resistor with a custom SVG still renders as a resistor zigzag.
    #[test]
    fn custom_symbol_does_not_override_system_symbol() {
        let mut b = mk(Symbol::Resistor, BoxKind::TwoPin);
        b.set_custom_symbol(CustomSymbol {
            source: "MyR".into(),
            svg_body: r#"<rect class="my-sym" width="40" height="16"/>"#.into(),
            view_box: crate::vector::graph::boxdef::SvgViewBox {
                min_x: 0.0,
                min_y: 0.0,
                width: 40.0,
                height: 16.0,
            },
        });
        let svg = render_box(&b, false);
        // R-S: system symbol (resistor zigzag) is used, not custom SVG
        assert!(!svg.contains(r#"class="comp custom""#));
        assert!(!svg.contains(r#"data-symbol-source="MyR""#));
        assert!(!svg.contains(r#"class="my-sym""#));
    }

    #[test]
    fn no_custom_symbol_uses_system() {
        let b = mk(Symbol::Resistor, BoxKind::TwoPin);
        let svg = render_box(&b, false);
        assert!(!svg.contains(r#"class="comp custom""#));
    }

    /// mcd docs-mc 16-export-viz §6: a virtually instantiated test point
    /// (wrapper `u_1`) must not leak its fabricated instance name — the box
    /// shows its class name (e.g. `TP`) instead.
    #[test]
    fn virtual_test_point_hides_instance_name() {
        let mut b = mk(Symbol::TestPoint, BoxKind::Dot);
        b.name = "u_1".into();
        b.class_name = "TP".into();
        b.suppress_instance_name = true;
        let svg = render_box(&b, false);
        assert!(!svg.contains("u_1"), "instance name must not render: {svg}");
        assert!(
            svg.contains(">TP</text>"),
            "class name should render: {svg}"
        );
    }

    /// Non-virtual test points keep their instance name (TP1, TP2, …).
    #[test]
    fn real_test_point_keeps_instance_name() {
        let mut b = mk(Symbol::TestPoint, BoxKind::Dot);
        b.name = "TP3".into();
        let svg = render_box(&b, false);
        assert!(svg.contains(">TP3</text>"), "{svg}");
        assert!(!svg.contains("u_1"), "{svg}");
    }
}
