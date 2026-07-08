// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `McVecBlock` -> `McVecGraph` converter
//! `McVecBlock` -> `McVecGraph` converter
//!
//! ## Architecture after P1 completion
//!
//! ### Type definition layer
//! - [`kinds`]      -- `BoxKind` / `EdgeType` / **`NetKind`** ★
//! - [`box_def`]    -- `IoSummary` / `Wire` / `McVecBox` / **`EntryPoint`** ★
//! - [`net_def`]    -- **`VizNet`** (multi-endpoint hyperedge) ★ + compatible `McVecEdge`
//! - [`graph_def`]  -- `McVecGraph` (with `nets: Vec<VizNet>` field)
//!
//! ### Algorithm layer
//! - [`detect`]     -- duck typing recognition + naming / IO helpers
//! - [`from_table`] -- fallback: build from flat `InstTable` (legacy behavior)
//! - [`from_block`] -- ★ Main flow: build from `McVecBlock` + simultaneously generate `VizNet`
//! - [`promote`]    -- ★ Cross-layer net promotion (core of top-level simplest integration)
//!
//! ### Output layer
//! - [`json`]       -- `to_json` / `to_json_pretty` (including VizNet serialization)
//!
//! ### Legacy path
//! After P1 completes, legacy.rs **is no longer needed**. Can be deleted entirely, or just
//! keep `#[cfg(test)] mod tests;` for regression testing.
//!
//! ## Call flow
//! ```ignore
//! use crate::vector::graph::*;
//!
//! let graph = build_mc_vec_graph(&block, &table);
//!
//! // Top-level simplest integration: keep only inter-box nets
//! let mut g = graph;
//! apply_promote_recursive(&mut g);
//!
//! // Serialize for frontend
//! let json = g.to_json();
//! ```

// ── Type definition layer ──
pub mod box_def;
pub mod graph_def;
pub mod json;
pub mod kinds;
pub mod naming;
pub mod net_def;
pub mod symbol;
// ── Algorithm layer ──
pub mod detect;
pub mod from_block;
pub mod from_table;
pub mod net_probe;
pub mod promote;
// ============================================================================
// Top-level re-exports
// ============================================================================

pub use box_def::{
    BoxLabelPlacement, EntryPoint, EntrySide, IoSummary, LabelPlacementKind, McVecBox, VisualRole,
    Wire,
};
pub use graph_def::McVecGraph;
pub use json::json_escape;
pub use kinds::{BoxKind, EdgeType, NetKind};
pub use net_def::{EndpointRef, McVecEdge, Point, Route, Segment, VizNet};
pub use symbol::Symbol;

pub use detect::{
    compute_io, detect_kind, extract_last_segment, is_power_label, is_signal_like, DetectedKind,
};
pub use from_block::{build_graph_smart, build_mc_vec_graph};
pub use from_table::build_graph_from_table;
pub use promote::{
    apply_promote_in_place, apply_promote_recursive, lift_endpoints_to_layer_boxes,
    promote_to_inter_box_only, PromoteResult,
};
