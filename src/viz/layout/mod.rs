// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Layout algorithms
//!
//! ## Architecture
//!
//! ### Utility layer (single-responsibility small functions)
//! - [`size`]       —— box size calculation + spacing constants
//! - [`components`] —— adjacency list + connected component partition
//! - [`overlap`]    —— overlap removal (force-directed push apart)
//! - [`normalize`]  —— coordinate normalization + canvas size calculation
//!
//! ### Single-strategy (used inside each box subset)
//! - [`chain`]      —— chain topology detection + horizontal layout
//!
//! ### Whole-graph Layouter (impl trait)
//! - [`flow::FlowLayouter`] —— default layout engine

pub mod chain;
pub mod coalesce;
pub mod components;
pub mod entry_points;
pub mod facade;
pub mod flow;
pub mod islands;
pub mod ladder_model;
pub mod ladder_place;
pub mod normalize;
pub mod optimize;
pub mod overlap;
pub mod passive_inline;
pub mod pin_place;
pub mod rails;
pub mod select;
pub mod size;
pub mod sp_model;
pub mod sp_place;
pub mod two_lane_ladder;
pub mod v2;
pub use flow::FlowLayouter;
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
