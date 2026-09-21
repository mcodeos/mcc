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

/// Version of the layout contract: what a layouter promises about the
/// coordinates it hands back.
///
/// **Declared, not derived.** A token such as [`crate::stages::world_ver`] must
/// change whenever the world changes; this number changes only when someone
/// decides the contract itself changed — a box placed differently on purpose,
/// not because an input moved. Bumping it is a statement to consumers that
/// coordinates from before and after are not comparable.
pub const LAYOUT_VERSION: &str = "1";

pub mod audit_registry;
pub mod block_frame;
pub mod chain;
pub mod coalesce;
pub mod components;
pub mod edge_decide;
pub mod entry_points;
pub mod equi_audit;
pub mod equi_chain;
pub mod equi_column;
pub mod equi_place;
pub mod equipotential_tree;
pub mod facade;
pub mod flow;
pub mod islands;
pub mod ladder_model;
pub mod ladder_place;
pub mod module_frame;
pub mod normalize;
pub mod optimize;
pub mod overlap;
pub mod passive_inline;
pub mod pin_place;
pub mod radial;
pub mod rails;
pub mod select;
pub mod size;
pub mod sp_model;
pub mod sp_place;
pub mod supply_bundle;
pub mod two_lane_ladder;
pub use flow::FlowLayouter;
