// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Sub-module rectangle render — with "click to expand" visual hint
//!
//! Improvements (vs legacy::render_sub_module):
//! - hover tooltip
//! - top-right `＋` circle corner marker
//! - cursor:pointer

use crate::vector::graph::McVecBox;

use super::pin_render::{render_pin, PinRenderOpts, PinStyle};

/// Pin render options for sub-module ports.
///
/// ★ FIX (pin distribution): Previously the sub-module only drew a dashed frame, **not drawing pins at all** —
/// wires went straight to a bare point on the box edge, with multiple lines crammed together, which doesn't
/// match the schematic style of "each pin connects to a part then continues out". Now, just like
/// [`super::ic::IcShape`]: for each `entry_point` draw a stub (8px outward = where the wire enters)
/// + the function name (inside the box).
///
/// Difference from IC: **physical pin numbers are not shown** — the `pin_id` of sub-module ports is
/// mostly a high-bit id synthesized in layout (see `promote_synthetic_pins`), and showing it would be meaningless.
fn submodule_pin_opts() -> PinRenderOpts {
    PinRenderOpts {
        style: PinStyle::Stub,
        show_number: false,
        show_name: true,
    }
}

pub fn render_sub_module(b: &McVecBox) -> String {
    // ── All port pins (stub + function name) ──
    // When entry_points is empty (layout did not populate), fall back to old behavior: only the frame, no pins.
    let pins: String = b
        .entry_points
        .iter()
        .map(|ep| render_pin(b, ep, submodule_pin_opts()))
        .collect();

    // ── Instance name + class name (top-left, outside the box) ──
    let label_x = b.x;
    let name_y = b.y - 14.0;
    let name_svg = format!(
        r##"    <text x="{:.1}" y="{:.1}" text-anchor="start"
          font-size="14" font-weight="700" fill="#212121">{}</text>
"##,
        label_x,
        name_y,
        escape_xml(&b.name)
    );
    let class_svg = if !b.class_name.is_empty() {
        format!(
            r##"    <text x="{:.1}" y="{:.1}" text-anchor="start"
          font-size="10" fill="#5C6BC0">{}</text>
"##,
            label_x,
            b.y - 2.0,
            escape_xml(&b.class_name)
        )
    } else {
        String::new()
    };

    format!(
        r##"  <g class="comp sub-module" data-id="{id}" style="cursor:pointer"
       onclick="expandSubModule({id})">
    <title>Click to expand: {name}</title>
{name_svg}{class_svg}    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="6"
          fill="#FAFAFA" stroke="#424242" stroke-width="1.5"/>
    <g transform="translate({corner_x:.1},{corner_y:.1})">
      <circle cx="0" cy="0" r="8" fill="#424242" />
      <text x="0" y="0.5" text-anchor="middle" dominant-baseline="central"
            font-size="10" font-weight="700" fill="#FAFAFA">＋</text>
    </g>
{pins}  </g>
"##,
        id = b.id,
        name = escape_xml(&b.name),
        name_svg = name_svg,
        class_svg = class_svg,
        x = b.x,
        y = b.y,
        w = b.w,
        h = b.h,
        corner_x = b.x + b.w - 10.0,
        corner_y = b.y + 10.0,
        pins = pins,
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
