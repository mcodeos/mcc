// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Bus thick line + tap SVG output
//!
//! Cooperates with [`crate::viz::route::bus_bundle::BusBundleRouter`]:
//! the router computes the trunk (`route.segments[0]`) + each tap (`route.segments[1..]`),
//! and this file renders it as a thick trunk + thin tap lines + the bit-position label on
//! the trunk.
//!
//! ## Current status
//! P4's [`super::wire::render_viznet`] already correctly renders BusBundleRouter's output
//! (trunk + tap are both segments, unified into a single path). This file remains as a
//! **dedicated bus render** hook — fill this in when you want the trunk and taps to use
//! different stroke-widths later.
//!
//! Example: to make the trunk stroke-width=4, tap stroke-width=1.4, implement here:

use crate::vector::graph::{NetKind, VizNet};

use super::wire::render_label_with_bg;

/// Dedicated bus render (thick trunk + thin taps)
///
/// Assumes `route.segments[0]` is the trunk and the rest are taps.
pub fn render_bus_with_taps(net: &VizNet) -> String {
    let route = match &net.route {
        Some(r) => r,
        None => return String::new(),
    };
    if route.segments.is_empty() {
        return String::new();
    }

    let trunk = &route.segments[0];
    let taps = &route.segments[1..];

    let label = match &net.kind {
        NetKind::Bus(n) => format!("{} [{}]", net.name, n),
        _ => net.name.clone(),
    };

    let trunk_path = format!(
        "M{:.1},{:.1} L{:.1},{:.1}",
        trunk.from.x, trunk.from.y, trunk.to.x, trunk.to.y
    );

    let mut svg = format!(
        r##"  <g class="edge net bus" data-nid="{nid}">
    <!-- trunk -->
    <path d="{tp}" stroke="#854F0B" stroke-width="3.6" fill="none" stroke-linecap="square"/>
"##,
        nid = net.nid,
        tp = trunk_path,
    );

    // tap thin lines
    for tap in taps {
        svg.push_str(&format!(
            "    <path d=\"M{:.1},{:.1} L{:.1},{:.1}\" stroke=\"#854F0B\" stroke-width=\"1.4\" fill=\"none\"/>\n",
            tap.from.x, tap.from.y, tap.to.x, tap.to.y,
        ));
    }

    // tap junctions
    for j in &route.junctions {
        svg.push_str(&format!(
            "    <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.5\" fill=\"#854F0B\"/>\n",
            j.x, j.y,
        ));
    }

    // Label at the trunk's midpoint
    let mx = (trunk.from.x + trunk.to.x) / 2.0;
    let my = (trunk.from.y + trunk.to.y) / 2.0;
    svg.push_str("    ");
    svg.push_str(&render_label_with_bg(&label, mx, my, "#854F0B", 10.5));
    svg.push('\n');

    svg.push_str("  </g>\n");
    svg
}
