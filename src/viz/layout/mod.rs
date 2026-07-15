// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Layout algorithms
//!
//! ## Architecture after P3 completion
//!
//! ### Utility layer (single-responsibility small functions)
//! - [`size`]       —— box size calculation + spacing constants
//! - [`components`] —— adjacency list + connected component partition
//! - [`overlap`]    —— overlap removal (force-directed push apart)
//! - [`normalize`]  —— coordinate normalization + canvas size calculation
//!
//! ### Single-strategy (used inside each box subset)
//! - [`chain`]      —— chain topology detection + horizontal layout
//! - [`radial`]     —— hub-and-spoke radial (inside subset)
//!
//! ### Whole-graph Layouter (impl trait)
//! - [`multi_strategy::RadialLayouter`] —— multi-strategy scheduler (default, replaces old GridPlacer)
//! - [`hierarchical::HierarchicalLayouter`] —— ★ top-level simplest integration dedicated hierarchical layout
//! - [`grid::GridLayouter`] —— simple grid (debug / alternative)
//!
//! ### legacy.rs deleted
//! P3 extracted all functionality from legacy.rs, **legacy.rs file can be deleted entirely**.
//! Old call `crate::viz::layout::GridPlacer` should use [`multi_strategy::RadialLayouter`].

pub mod chain;
pub mod components;
pub mod entry_points;
pub mod flow;
pub mod grid;
pub mod hierarchical;
pub mod ladder_model;
pub mod ladder_place;
pub mod layered;
pub mod multi_strategy;
pub mod normalize;
pub mod optimize;
pub mod overlap;
pub mod passive_inline;
pub mod pin_place;
pub mod radial;
pub mod rails;
pub mod schematic_layout;
pub mod schematic_radial;
pub mod select;
pub mod size;
pub mod two_lane_ladder;
pub use flow::FlowLayouter;
pub use layered::LayeredLayouter;
pub use schematic_layout::SchematicSubLayouter;
pub use schematic_radial::SchematicRadialLayouter;
// ============================================================================
// Top-level re-exports
// ============================================================================

// Utilities
pub use components::{
    build_adjacency, build_degrees, find_connected_components, partition_components,
};
pub use entry_points::assign_entry_points;
pub use normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN, CANVAS_PADDING};
pub use overlap::{resolve_overlaps, resolve_overlaps_iterative};
pub use size::{assign_default_sizes, box_size, MIN_GAP};

// Single-strategy
pub use chain::{layout_chain_horizontal, try_linearize_chain};
pub use radial::{
    bfs_rings_in_subset, find_hub_in_subset, place_ring, place_ring2, place_unconnected,
    set_center, RING1_RADIUS, RING2_RADIUS,
};

// Layouter implementations
pub use grid::GridLayouter;
pub use hierarchical::HierarchicalLayouter;
pub use multi_strategy::RadialLayouter;

// ============================================================================
// Compatibility: old `GridPlacer` callers (kept for 1 release cycle)
// ============================================================================

/// Compatibility for old `viz::layout::GridPlacer::layout(&mut graph)` calls
///
/// **deprecated**: new code should construct `RadialLayouter` and call `Layouter::layout(&mut graph)` directly
pub struct GridPlacer;

impl GridPlacer {
    pub fn layout(graph: &mut crate::vector::graph::McVecGraph) -> (f64, f64) {
        use crate::viz::traits::Layouter;
        RadialLayouter.layout(graph)
    }
}
