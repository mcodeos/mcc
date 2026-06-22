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
    let cx = b.x + b.w / 2.0;
    let cy = b.y + b.h / 2.0;

    // ── All port pins (stub + function name) ──
    // When entry_points is empty (layout did not populate), fall back to old behavior: only the frame, no pins.
    let pins: String = b
        .entry_points
        .iter()
        .map(|ep| render_pin(b, ep, submodule_pin_opts()))
        .collect();

    // ── Class name (user requested we draw the classname too) ──
    // When there is a class name, the main name shifts up to make room; the class name goes below the name, and the "click to expand" hint moves further down.
    let has_class = !b.class_name.is_empty();
    let name_y = if has_class { cy - 12.0 } else { cy - 4.0 };
    let class_svg = if has_class {
        format!(
            r##"    <text x="{cx:.1}" y="{cy_:.1}" text-anchor="middle"
          font-size="10" fill="#5C6BC0">{cls}</text>
"##,
            cx = cx,
            cy_ = cy + 1.0,
            cls = escape_xml(&b.class_name)
        )
    } else {
        String::new()
    };
    let hint_y = if has_class { cy + 14.0 } else { cy + 12.0 };

    format!(
        r##"  <g class="comp sub-module" data-id="{id}" style="cursor:pointer"
       onclick="expandSubModule({id})">
    <title>Click to expand: {name}</title>
    <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="6"
          fill="#FAFAFA" stroke="#424242" stroke-width="1.5"/>
    <g transform="translate({corner_x:.1},{corner_y:.1})">
      <circle cx="0" cy="0" r="8" fill="#424242" />
      <text x="0" y="0.5" text-anchor="middle" dominant-baseline="central"
            font-size="10" font-weight="700" fill="#FAFAFA">＋</text>
    </g>
    <text x="{cx:.1}" y="{t1:.1}" text-anchor="middle"
          font-size="14" font-weight="700" fill="#212121">{name}</text>
{class_svg}    <text x="{cx:.1}" y="{t2:.1}" text-anchor="middle"
          font-size="9" fill="#888">▸ click to expand</text>
{pins}  </g>
"##,
        id = b.id,
        name = escape_xml(&b.name),
        x = b.x,
        y = b.y,
        w = b.w,
        h = b.h,
        cx = cx,
        t1 = name_y,
        t2 = hint_y,
        class_svg = class_svg,
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
