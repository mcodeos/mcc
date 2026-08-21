// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Algorithm slots: `Layouter` / `Router` / `Renderer` trait
//!
//! ## P4 Changes
//! - `LegacyRenderer` is removed, replaced with [`crate::viz::render::SvgRenderer`]
//! - `LegacyLayouter` is now a compatibility alias for `RadialLayouter` (P3 changed)
//! - `NoopRouter` is still used (for debugging)

use crate::vector::graph::{McVecGraph, VizNet};

// ============================================================================
// Trait definitions
// ============================================================================

pub trait Layouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64);

    /// ★ P7-0: attach a SchematicLayoutModel **onto the candidate itself**,
    /// preserving the candidate's own parameters (sub() vs default()).
    /// Returns `None` for layouters that don't consume a model — caller then
    /// just runs the candidate as-is.
    fn with_model(
        &self,
        _model: crate::viz::layout_model::SchematicLayoutModel,
    ) -> Option<Box<dyn Layouter>> {
        None
    }

    fn name(&self) -> &'static str {
        "unnamed_layouter"
    }
}

pub trait Router {
    fn route(&self, graph: &McVecGraph, net: &mut VizNet);

    fn name(&self) -> &'static str {
        "unnamed_router"
    }
}

pub trait Renderer {
    /// Render the graph to an SVG string. `canvas` is the viewBox SIZE
    /// `(w, h)`; `origin` is the viewBox top-left `(x, y)` — the M10 content-fit
    /// starts the viewBox at the true content top (which may be negative for
    /// upward-reading vertical labels), so the origin is not always `(0, 0)`.
    fn render(&self, graph: &McVecGraph, canvas: (f64, f64), origin: (f64, f64)) -> String;

    fn name(&self) -> &'static str {
        "unnamed_renderer"
    }
}

// ============================================================================
// SvgRenderer (new, P4) wrapped as Renderer trait
// ============================================================================

/// Default renderer: wraps [`crate::viz::render::SvgRenderer`]
///
/// Replaces P2 / P3's `LegacyRenderer` (which references `viz::render::legacy`,
/// P4 removed legacy.rs)
pub struct DefaultRenderer;

impl Renderer for DefaultRenderer {
    fn render(&self, graph: &McVecGraph, canvas: (f64, f64), origin: (f64, f64)) -> String {
        crate::viz::render::SvgRenderer::render(graph, origin.0, origin.1, canvas.0, canvas.1)
    }

    fn name(&self) -> &'static str {
        "default_svg"
    }
}

/// **deprecated** —— `LegacyRenderer` now equals `DefaultRenderer`
///
/// (References to old `LegacyRenderer` continue to work, but behavior is updated)
pub use DefaultRenderer as LegacyRenderer;

// ============================================================================
// NoopRouter
// ============================================================================

pub struct NoopRouter;

impl Router for NoopRouter {
    fn route(&self, _graph: &McVecGraph, _net: &mut VizNet) {}

    fn name(&self) -> &'static str {
        "noop_router"
    }
}
