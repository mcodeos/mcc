// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! SVG render layer
//!
//! ## Architecture after P4 completes
//!
//! ```text
//!   McVecGraph (already layouted, nets already routed)
//!         │
//!         ▼
//!     SvgRenderer::render(graph, canvas)
//!            │
//!            ├── shape::render_box ─────→ each box → SVG <g>
//!            │     ├── two_pin / multi_pin / sub_module / power_label
//!            │     └── BoxShape trait
//!            ├── wire::render_edge   ────→ each McVecEdge → SVG <g> (compat with old binary)
//!            └── wire::render_viznet ────→ each VizNet → SVG <g> (new hyperedge)
//! ```
//!
//! ## Sub-modules
//! - [`shape`]       —— `BoxShape` trait + `render_box` dispatch
//! - [`two_pin`]     —— R / C / L / D etc.
//! - [`multi_pin`]   —— multi-pin IC
//! - [`sub_module`]  —— sub-module (with expand hint, extracted in P3)
//! - [`power_label`] —— power / ground
//! - [`wire`]        —— old `McVecEdge` + new `VizNet` SVG output
//! - [`bus`]         —— dedicated bus render (thick trunk + thin taps)
//!
//! ## legacy.rs has been removed
//! P4 extracted all features from legacy.rs; the file can be removed entirely.

pub mod bus;
pub mod capacitor;
pub mod diode;
pub mod ic;
pub mod inductor;
pub mod label_render;
pub mod multi_pin;
pub mod pin_render;
pub mod power_label;
pub mod power_rail;
pub mod resistor;
pub mod shape;
pub mod sub_module;
pub mod two_pin;
pub mod wire;
pub use bus::render_bus_with_taps;
pub use shape::{render_box, BoxShape};
pub use wire::render_viznet;

use crate::vector::graph::McVecGraph;

// ============================================================================
// SvgRenderer (P4 assembly)
// ============================================================================

/// SVG renderer
///
/// Replaces the old `legacy::SvgRenderer`; the new version supports both:
/// - `graph.edges` (old McVecEdge binary model, compatible)
/// - `graph.nets`  (★ VizNet multi-endpoint model, preferred)
///
/// When `graph.nets` is non-empty, prefer rendering nets; otherwise fall back to edges.
pub struct SvgRenderer;

impl SvgRenderer {
    pub fn render(graph: &McVecGraph, canvas_w: f64, canvas_h: f64) -> String {
        let mut svg = String::new();

        svg.push_str(&format!(
            r##"<svg viewBox="0 0 {canvas_w:.0} {canvas_h:.0}" xmlns="http://www.w3.org/2000/svg"
     font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
     style="background:transparent">"##
        ));
        svg.push('\n');

        svg.push_str(
            r##"  <defs>
    <marker id="dot" markerWidth="6" markerHeight="6" refX="3" refY="3">
      <circle cx="3" cy="3" r="2" fill="#888"/>
    </marker>
  </defs>
"##,
        );

        // ── Edges (bottom layer) ──
        // Use VizNet (multi-endpoint hyperedge) render
        for net in &graph.nets {
            if net.route.is_some() {
                svg.push_str(&wire::render_viznet(net));
            }
        }

        // ── Boxes (top layer) ──
        for b in &graph.boxes {
            svg.push_str(&shape::render_box(b));
        }

        svg.push_str("</svg>\n");
        svg
    }
}
