// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Render side: layout + route + render + template
//!
//! ## Full-stack architecture after P4 completes
//! ```text
//!   McVecGraph (from vector::graph, contains nets: Vec<VizNet>)
//!         │
//!         ├── apply_promote_recursive   (P1) ── top-level minimal integration
//!         ▼
//!     viz::api::render(graph) ──→ VizDocument
//!         │   │
//!         │   ├── Top-level layout: HierarchicalLayouter (P3)
//!         │   ├── Sub-layer layout: RadialLayouter    (P3)
//!         │   ├── route:       smart_route_all       (P4) ─ pick router by NetKind
//!         │   │     ├── Power/Ground/SubModuleIO → StarRouter
//!         │   │     ├── Bus(n)                    → BusBundleRouter
//!         │   │     └── Signal                    → OrthogonalRouter
//!         │   └── render:      DefaultRenderer (new SvgRenderer) (P4)
//!         │
//!         ▼
//!  template::wrap_document(doc) ──→ HTML  (P2: real expand JS)
//! ```
//!
//! ## Sub-modules
//! - [`api`]      —— top-level `render(graph) → VizDocument`
//! - [`doc`]      —— `VizDocument` multi-layer document
//! - [`layer`]    —— `VizLayer` single layer
//! - [`traits`]   —— `Layouter` / `Router` / `Renderer` trait
//! - [`debug`]    —— ★ P4: `MC_VIZ_DUMP=1` triggered layout/route/doc three-stage reconciliation
//! - [`layout`]   —— layout algorithms (P3 fully split)
//! - [`route`]    —— ★ P4: routing algorithms (Orthogonal/Star/BusBundle/Straight)
//! - [`render`]   —— ★ P4: SVG output (BoxShape trait + wire/bus separation)
//! - [`template`] —— HTML wrapper

pub mod api;
pub mod debug;
pub mod doc;
pub mod idiom;
pub mod layer;
pub mod layout;
pub mod log;
pub mod metrics;
pub mod pins;
pub mod render;
pub mod route;
pub mod semantic;
pub mod template;
pub mod traits;

// ============================================================================
// Top-level re-exports
// ============================================================================

pub use api::{render, render_to_html, render_with, render_with_metrics, RenderOpts};
pub use doc::VizDocument;
pub use layer::VizLayer;

// trait + default implementations
pub use traits::{
    DefaultRenderer, Layouter, LegacyLayouter, LegacyRenderer, NoopRouter, Renderer, Router,
};

// Layout algorithms
pub use layout::{GridLayouter, HierarchicalLayouter, LayeredLayouter, RadialLayouter};

// Routing algorithms (P4 new)
pub use route::{smart_route_all, BusBundleRouter, OrthogonalRouter, StarRouter, StraightRouter};

// Render (P4 new)
pub use render::SvgRenderer;
