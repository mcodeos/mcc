// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `McVecBlock` -> `McVecGraph` converter
//!
//! ## Architecture
//!
//! ### Type definition layer
//! - [`kinds`]      -- `BoxKind` / **`NetKind`** ★
//! - [`boxdef`]     -- `IoSummary` / `McVecBox` / **`EntryPoint`** ★
//! - [`netdef`]     -- **`VizNet`** (multi-endpoint hyperedge) ★
//! - [`graphdef`]   -- `McVecGraph` (with `nets: Vec<VizNet>` field)
//!
//! ### Algorithm layer
//! - [`detect`]     -- duck typing recognition + naming / IO helpers
//! - [`fromblock`]  -- ★ Main flow: build from `McVecBlock` + simultaneously generate `VizNet`
//! - [`promote`]    -- ★ Cross-layer net promotion (core of top-level simplest integration)
//!
//! ### Output layer
//! - [`json`]       -- `to_json` / `to_json_pretty` (including VizNet serialization)
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
pub mod boxdef;
pub mod graphdef;
pub mod json;
pub mod kinds;
pub mod naming;
pub mod netdef;
pub(crate) mod psymbol;
pub mod symbol;
// ── Algorithm layer ──
pub mod detect;
pub mod fromblock;
pub mod promote;
// ============================================================================
// Top-level re-exports
// ============================================================================

pub use boxdef::{
    AnchorHint, BoundaryPort, BoxLabelPlacement, EntryPoint, EntrySide, FramePort, IoSummary,
    LabelPlacementKind, McVecBox, ModuleFrame, PinConstraint, PinSlot, PortDir, VisualRole,
    ZoneBorder,
};
pub use graphdef::{LayerStyle, McVecGraph};
pub use json::json_escape;
pub use kinds::{BoxKind, NetKind};
pub use netdef::{EndpointRef, NetRole, Point, Route, Segment, VizNet};
pub use symbol::Symbol;

pub use detect::{
    compute_io, compute_scope_chain, detect_kind, extract_last_segment, is_power_label,
    is_signal_like, DetectedKind,
};
pub use fromblock::{build_graph_smart, build_mc_vec_graph};
pub use promote::{
    apply_promote_in_place, apply_promote_recursive, lift_endpoints_to_layer_boxes,
    promote_to_inter_box_only, PromoteResult,
};
