// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! IC (multi-pin chip) render —— P05 redo, shows every pin
//!
//! Replaces the old `multi_pin.rs` (which only drew a rectangle + name).
//! Now every `entry_point` draws a pin marker + pin number + pin name:
//!
//! ```text
//!       1┌──────────────┐
//!  ──────┤VCC      RX  ├──── 2
//!       3├GND      TX  ├──── 4
//!       5├RST    MOSI  ├──── 6
//!         │   mcu513    │
//!         │     U3      │
//!         └──────────────┘
//! ```
//!
//! ## Coupling with layout
//! Render requires `entry_points` to be populated (filled in by layout after computing
//! coordinates). When empty, falls back: only draws the rectangle + name (equivalent to
//! the pre-P05 MultiPinShape).
//!
//! ## Pin 1 marker
//! Industry convention: an IC has a small dot / notch at the top-left, marking the
//! physical pin 1 location. P05 simplified handling: draw a small dot at the top-left
//! of the box regardless of which side pin 1 is actually on.

use crate::vector::graph::McVecBox;

use super::label_render::render_designator_and_value;
use super::pin_render::{render_pin, PinRenderOpts};
use super::shape::BoxShape;

pub struct IcShape;

impl BoxShape for IcShape {
    fn render(&self, b: &McVecBox) -> String {
        // ── Main body frame ──
        let body = format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3"
            fill="#FAFAFA" stroke="#1A237E" stroke-width="1.5"/>"##,
            x = b.x,
            y = b.y,
            w = b.w,
            h = b.h
        );

        // ── Pin 1 corner marker (small dot at top-left, industry convention) ──
        let pin1_marker = format!(
            r##"<circle cx="{:.1}" cy="{:.1}" r="2.5" fill="#1A237E"/>"##,
            b.x + 7.0,
            b.y + 7.0
        );

        // ── Instance name + class name (top-left, outside the box) ──
        let label_x = b.x;
        let name_y = b.y - 14.0;
        let name = format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="start" dominant-baseline="auto"
            font-size="12" font-weight="700" fill="#1A237E">{}</text>"##,
            label_x,
            name_y,
            escape_xml(&b.name)
        );

        let class_svg = if !b.class_name.is_empty() {
            format!(
                r##"    <text class="class-name" x="{:.1}" y="{:.1}" text-anchor="start"
            dominant-baseline="auto" font-size="9" fill="#5C6BC0">{}</text>
"##,
                label_x,
                b.y - 2.0,
                escape_xml(&b.class_name)
            )
        } else {
            String::new()
        };

        // ── Designator (below the part, small, italic) ──
        let stamp = render_designator_and_value(b);

        // ── All pin labels ──
        // When there are no entry_points, fall back: do not draw pin labels (back to the old MultiPin look)
        let pins: String = if b.entry_points.is_empty() {
            String::new()
        } else {
            b.entry_points
                .iter()
                .map(|ep| render_pin(b, ep, PinRenderOpts::for_ic()))
                .collect()
        };

        format!(
            r##"  <g class="comp ic" data-id="{id}">
    {body}
    {pin1_marker}
    {name}
{class_svg}{stamp}{pins}  </g>
"##,
            id = b.id,
            body = body,
            pin1_marker = pin1_marker,
            name = name,
            class_svg = class_svg,
            stamp = stamp,
            pins = pins,
        )
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
    use crate::vector::graph::{BoxKind, EntryPoint, EntrySide, IoSummary, Symbol};

    fn mk_ic(pins: &[(&str, EntrySide)]) -> McVecBox {
        let mut b = McVecBox::new_v2(
            42,
            "MCU".into(),
            "STM32F4".into(),
            BoxKind::MultiPin,
            Symbol::Ic,
            Some("U1".into()),
            None,
            pins.len(),
            IoSummary::new(),
        );
        b.x = 100.0;
        b.y = 50.0;
        b.w = 140.0;
        b.h = 100.0;
        b.entry_points = pins
            .iter()
            .enumerate()
            .map(|(i, (name, side))| EntryPoint {
                pin_id: (i as i64) + 1,
                pin_name: (*name).into(),
                side: side.clone(),
                offset: 0.5,
            })
            .collect();
        b
    }

    #[test]
    fn renders_all_pins() {
        let b = mk_ic(&[
            ("VCC", EntrySide::Top),
            ("GND", EntrySide::Bottom),
            ("RX", EntrySide::Left),
            ("TX", EntrySide::Right),
        ]);
        let svg = IcShape.render(&b);
        // Each pin has one <g class="pin">
        assert_eq!(
            svg.matches(r#"<g class="pin""#).count(),
            4,
            "expected 4 pins, svg was:\n{}",
            svg
        );
        // Every pin name is displayed
        assert!(svg.contains(">VCC</text>"));
        assert!(svg.contains(">GND</text>"));
        assert!(svg.contains(">RX</text>"));
        assert!(svg.contains(">TX</text>"));
    }

    #[test]
    fn shows_designator() {
        let b = mk_ic(&[("VCC", EntrySide::Top)]);
        let svg = IcShape.render(&b);
        assert!(
            svg.contains(">U1</text>"),
            "expected U1 designator: {}",
            svg
        );
    }

    #[test]
    fn shows_pin1_marker() {
        let b = mk_ic(&[("1", EntrySide::Left)]);
        let svg = IcShape.render(&b);
        // Top-left should have a circle (pin 1 marker)
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn renders_main_name() {
        let b = mk_ic(&[("VCC", EntrySide::Top)]);
        let svg = IcShape.render(&b);
        assert!(svg.contains(">MCU</text>"));
    }

    #[test]
    fn no_entry_points_still_renders() {
        let mut b = McVecBox::new_v2(
            1,
            "X".into(),
            "".into(),
            BoxKind::MultiPin,
            Symbol::Ic,
            None,
            None,
            0,
            IoSummary::new(),
        );
        b.x = 0.0;
        b.y = 0.0;
        b.w = 100.0;
        b.h = 60.0;
        // entry_points is empty
        let svg = IcShape.render(&b);
        // No pin labels
        assert!(!svg.contains(r#"class="pin""#));
        // But the body is still there
        assert!(svg.contains("<rect"));
        assert!(svg.contains(">X</text>"));
    }
}
