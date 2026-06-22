// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared pin render logic
//!
//! Draws SVG for an `EntryPoint`: pin marker (or stub line) + pin number + pin name.
//! Various Symbol renderers (`IcShape` / future `ModuleShape` etc.) all draw pins
//! through this module, avoiding duplicate logic for "which edge the pin is on, which
//! side the label goes on".
//!
//! ## Output form
//! ```xml
//! <g class="pin" data-pin-id="42">
//!   <circle cx="120" cy="80" r="2.5" fill="#222"/>   <!-- pin dot -->
//!   <text x="116" y="74" font-size="8" ...>1</text>    <!-- pin number (physical) -->
//!   <text x="124" y="80" font-size="10" ...>VCC</text> <!-- pin name (function) -->
//! </g>
//! ```
//!
//! ## Design points
//! - `pin_number` is the physical pin number (`1`/`2`/`14`), placed **outside** the box close to the pin dot, small font
//! - `pin_name` is the function name (`VCC`/`RX`), placed **inside** the box, medium font
//! - The position / text alignment of the two is computed by `label_positions` based on `EntrySide`

use crate::vector::graph::net_def::IoDirection;
use crate::vector::graph::{EntryPoint, EntrySide, McVecBox};

// ============================================================================
// Options
// ============================================================================

/// Pin marker style
#[derive(Debug, Clone, Copy)]
pub enum PinStyle {
    /// A small black dot (concise, suitable for dense ICs)
    Dot,
    /// Short line (traditional schematic style, extending 8px outward from box edge)
    Stub,
}

/// Pin render options
#[derive(Debug, Clone, Copy)]
pub struct PinRenderOpts {
    pub style: PinStyle,
    pub show_number: bool,
    pub show_name: bool,
}

impl PinRenderOpts {
    /// IC default: stub + number + name
    pub fn for_ic() -> Self {
        Self {
            style: PinStyle::Stub,
            show_number: true,
            show_name: true,
        }
    }

    /// Concise: only draw the pin dot, no label (used for two-pin parts like R/C/L/D
    /// where the endpoints are directly the part's lead tails and don't need text)
    pub fn marker_only() -> Self {
        Self {
            style: PinStyle::Dot,
            show_number: false,
            show_name: false,
        }
    }
}

// ============================================================================
// Main API: render_pin
// ============================================================================

/// Output SVG for an `EntryPoint`
///
/// Parameter `b` is used to look up the box's (x, y, w, h), and based on ep.side + ep.offset
/// compute the pin's absolute coordinates (cx, cy).
pub fn render_pin(b: &McVecBox, ep: &EntryPoint, opts: PinRenderOpts) -> String {
    let (cx, cy) = pin_position(b, ep);

    // The physical pin for this lead (BoxPin: pin_id=number/common name, description=function name, io=direction)
    let pin = b.find_pin(ep.pin_id);
    let io_dir = pin.map(|p| p.io).unwrap_or(IoDirection::Unknown);

    // Marker (dot or stub). On a stub, overlay the IO direction arrow (in points into box / out points out of box / io diamond).
    let marker = match opts.style {
        PinStyle::Dot => format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="2.5" fill="#222"/>"##),
        PinStyle::Stub => {
            let (ex, ey) = stub_outward(b, ep, 8.0);
            let line = format!(
                r##"<line x1="{cx:.1}" y1="{cy:.1}" x2="{ex:.1}" y2="{ey:.1}"
                        stroke="#222" stroke-width="1.2"/>"##
            );
            format!("{}{}", line, io_marker(b, ep, io_dir))
        }
    };

    let (num_pos, name_pos, num_anchor, name_anchor) = label_positions(b, ep);

    // ── A single lead draws at most two things: number (outside) + function name (inside) ──
    //
    // Source `io B = Base`: the `B` on the left of `=` is number/common name → drawn **on the outside stub** (small);
    //                      the `Base` on the right of `=` is the function name   → drawn **inside**, prominent (medium).
    // Source of truth is BoxPin (find_pin):
    //   - `pin.pin_id`      = number/common name (B / 1 / A1) → outside;
    //   - `pin.description` = function name     (Base / TX / 1) → inside.
    // When there is a function name, draw both: outside number + inside function name — **even if the two are identical** (a pure numeric lead `1=1`
    // must also print both 1s). When there is no function name (placeholder lead / unnamed lead), draw the number only once, placed inside.
    // When find_pin returns nothing (synthetic endpoint / rail flag), fall back to ep.pin_name as the inside name, skipping `(rail)`.
    let (outside_number, inside_name): (Option<String>, Option<String>) = match pin {
        Some(p) => {
            let id = p.pin_id.trim();
            let desc = p.description.trim();
            if !desc.is_empty() {
                let out = if id.is_empty() {
                    None
                } else {
                    Some(id.to_string())
                };
                (out, Some(desc.to_string()))
            } else {
                let single = if !id.is_empty() {
                    Some(id.to_string())
                } else {
                    None
                };
                (None, single)
            }
        }
        None => {
            if !ep.pin_name.is_empty() && !is_synthetic_pin_name(&ep.pin_name) {
                (None, Some(ep.pin_name.clone()))
            } else {
                (None, None)
            }
        }
    };

    // Outside number (small, on the stub). Only boxes (parts) with `show_number` draw it; sub-module ports do not draw the number.
    let number_svg = match (opts.show_number, &outside_number) {
        (true, Some(num)) => format!(
            r##"<text x="{:.1}" y="{:.1}" font-size="8" fill="#666"
                    font-family="JetBrains Mono, Menlo, monospace"
                    text-anchor="{}" dominant-baseline="central">{}</text>"##,
            num_pos.0,
            num_pos.1,
            num_anchor,
            escape_xml(num)
        ),
        _ => String::new(),
    };

    // Inside name (function name, medium). All boxes draw it (this is how "solid-line frames also fully show pinname").
    let name_svg = match (opts.show_name, &inside_name) {
        (true, Some(txt)) if !is_synthetic_pin_name(txt) => format!(
            r##"<text x="{:.1}" y="{:.1}" font-size="10" fill="#222"
                    text-anchor="{}" dominant-baseline="central">{}</text>"##,
            name_pos.0,
            name_pos.1,
            name_anchor,
            escape_xml(txt)
        ),
        _ => String::new(),
    };

    format!(
        r##"    <g class="pin" data-pin-id="{}">{}{}{}
    </g>
"##,
        ep.pin_id, marker, number_svg, name_svg
    )
}

// ============================================================================
// Geometry helpers
// ============================================================================

/// Compute the pin's absolute coordinates in SVG
pub fn pin_position(b: &McVecBox, ep: &EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Top => (b.x + b.w * ep.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * ep.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * ep.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * ep.offset),
    }
}

/// Extend outward `length` px from the pin dot, return the endpoint coordinates (used for the stub line)
fn stub_outward(b: &McVecBox, ep: &EntryPoint, length: f64) -> (f64, f64) {
    let (cx, cy) = pin_position(b, ep);
    match ep.side {
        EntrySide::Top => (cx, cy - length),
        EntrySide::Bottom => (cx, cy + length),
        EntrySide::Left => (cx - length, cy),
        EntrySide::Right => (cx + length, cy),
    }
}

/// IO direction marker: small arrow / diamond drawn on the stub line.
///
/// - `Input`  → solid triangle, tip **points into the box** (signal flows in);
/// - `Output` → solid triangle, tip **points out of the box** (signal flows out);
/// - `Bidir`  → hollow diamond (mc `io`, bidirectional);
/// - `Power` / `Ground` / `Passive` / `Unknown` → not drawn (return empty).
///
/// Geometry: `d` = distance along the outward direction from the box edge (px), `w` = perpendicular offset (px). The triangle occupies the 2~7px segment of the stub.
fn io_marker(b: &McVecBox, ep: &EntryPoint, io: IoDirection) -> String {
    // Unit vector along the outward lead direction (from box edge outward)
    let (ox, oy) = match ep.side {
        EntrySide::Top => (0.0, -1.0),
        EntrySide::Bottom => (0.0, 1.0),
        EntrySide::Left => (-1.0, 0.0),
        EntrySide::Right => (1.0, 0.0),
    };
    // Perpendicular direction (used to widen the arrow base)
    let (px, py) = (-oy, ox);
    let (cx, cy) = pin_position(b, ep);
    let pt = |d: f64, w: f64| (cx + ox * d + px * w, cy + oy * d + py * w);

    let tri = |apex: (f64, f64), b1: (f64, f64), b2: (f64, f64)| {
        format!(
            r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="#1A237E"/>"##,
            apex.0, apex.1, b1.0, b1.1, b2.0, b2.1
        )
    };

    match io {
        // Tip on the inside (near box, d=2), base outside (d=7) → points into the box
        IoDirection::Input => tri(pt(2.0, 0.0), pt(7.0, 3.0), pt(7.0, -3.0)),
        // Tip on the outside (d=7), base inside (d=2) → points out of the box
        IoDirection::Output => tri(pt(7.0, 0.0), pt(2.0, 3.0), pt(2.0, -3.0)),
        // Bidirectional: hollow diamond
        IoDirection::Bidir => {
            let n = pt(2.0, 0.0);
            let l = pt(4.5, 2.5);
            let f = pt(7.0, 0.0);
            let r = pt(4.5, -2.5);
            format!(
                r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="#FFF" stroke="#1A237E" stroke-width="1"/>"##,
                n.0, n.1, l.0, l.1, f.0, f.1, r.0, r.1
            )
        }
        IoDirection::Power | IoDirection::Ground | IoDirection::Passive | IoDirection::Unknown => {
            String::new()
        }
    }
}

/// Compute the position + text alignment for the pin number / pin name
///
/// Returns `(num_pos, name_pos, num_anchor, name_anchor)`:
/// - `num_pos`: (x, y) of the pin number, small, placed **outside** the box near the pin
/// - `name_pos`: (x, y) of the pin function name, medium, placed **inside** the box
/// - `*_anchor`: SVG text-anchor (`"start"` / `"middle"` / `"end"`)
fn label_positions(
    b: &McVecBox,
    ep: &EntryPoint,
) -> ((f64, f64), (f64, f64), &'static str, &'static str) {
    let (cx, cy) = pin_position(b, ep);
    match ep.side {
        EntrySide::Left => (
            // pin number outside (slightly raised on the left), function name inside (right)
            (cx - 4.0, cy - 7.0),
            (cx + 6.0, cy),
            "end",
            "start",
        ),
        EntrySide::Right => (
            // pin number outside (raised on the right), function name inside (left)
            (cx + 4.0, cy - 7.0),
            (cx - 6.0, cy),
            "start",
            "end",
        ),
        EntrySide::Top => (
            // pin number outside (above), function name inside (below)
            (cx + 5.0, cy - 4.0),
            (cx, cy + 12.0),
            "start",
            "middle",
        ),
        EntrySide::Bottom => (
            // pin number outside (below), function name inside (above)
            (cx + 5.0, cy + 10.0),
            (cx, cy - 6.0),
            "start",
            "middle",
        ),
    }
}

/// Whether this is a "real" physical pin number (displayable).
///
/// `promote_synthetic_pins` (3e9), `split_shared_pins` (4e9), the flags in `rails.rs`
/// (9e9), and other internally assigned pin_ids use the high base, are not physical pin
/// numbers, and should not be displayed as pin numbers.
///
/// Note: render_pin now uses `b.pins` to look up physical pin numbers and no longer
/// relies on this function; kept for future use.
#[allow(dead_code)]
fn is_real_pin_number(pin_id: i64) -> bool {
    pin_id > 0 && pin_id < 1_000_000_000
}

/// Whether this is a placeholder pin name produced by rail-synth synthetic endpoints (`"(rail)"`, `"(test)"`)
fn is_synthetic_pin_name(name: &str) -> bool {
    name.starts_with('(') && name.ends_with(')')
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
    use crate::vector::graph::{BoxKind, IoSummary, Symbol};

    fn mk_box() -> McVecBox {
        let mut b = McVecBox::new_v2(
            1,
            "U1".into(),
            "".into(),
            BoxKind::MultiPin,
            Symbol::Ic,
            None,
            None,
            4,
            IoSummary::new(),
        );
        b.x = 100.0;
        b.y = 50.0;
        b.w = 80.0;
        b.h = 60.0;
        b
    }

    #[test]
    fn pin_position_correct() {
        let b = mk_box();
        // Left at offset 0.5 → (b.x, b.y + b.h/2)
        let ep = EntryPoint {
            pin_id: 10,
            pin_name: "RX".into(),
            side: EntrySide::Left,
            offset: 0.5,
        };
        let (x, y) = pin_position(&b, &ep);
        assert!((x - 100.0).abs() < 1e-6);
        assert!((y - 80.0).abs() < 1e-6);
    }

    #[test]
    fn render_pin_includes_name_and_number() {
        let b = mk_box();
        let ep = EntryPoint {
            pin_id: 10,
            pin_name: "RX".into(),
            side: EntrySide::Left,
            offset: 0.5,
        };
        // Simulating EntryPoint that doesn't store pin_number, so the number segment is empty
        // We call render_pin directly here to verify the name
        let svg = render_pin(&b, &ep, PinRenderOpts::for_ic());
        assert!(svg.contains(">RX</text>"));
        assert!(svg.contains(r#"data-pin-id="10""#));
        assert!(svg.contains("<line")); // Stub line
    }

    #[test]
    fn render_pin_hides_synthetic_names() {
        let b = mk_box();
        let ep = EntryPoint {
            pin_id: -1,
            pin_name: "(rail)".into(),
            side: EntrySide::Top,
            offset: 0.5,
        };
        let svg = render_pin(&b, &ep, PinRenderOpts::for_ic());
        // Synthetic pin name `(rail)` should not appear on the diagram
        assert!(!svg.contains(">(rail)</text>"));
    }

    #[test]
    fn marker_only_no_text() {
        let b = mk_box();
        let ep = EntryPoint {
            pin_id: 1,
            pin_name: "1".into(),
            side: EntrySide::Right,
            offset: 0.5,
        };
        let svg = render_pin(&b, &ep, PinRenderOpts::marker_only());
        assert!(svg.contains("<circle"));
        assert!(!svg.contains("<text"));
    }
}
