// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `McModuleInst` → `McVecBlock` Converter
//!
//! ## P1: Vector Builder
//!
//! Convert `McModuleInst` to `McVecBlock` main driver.
//!
//! ## Architecture after P1
//!
//! - [`visit`]      —— Main driver: `McVecBuilder` recursive traversal + `build_mc_vec` entry
//! - [`resolve`]    —— NetPoint resolution: string path → InstTable ID + `np_warn` counter
//! - [`connection`] —— Topology analysis: connection pairs → `McVecNet` (star / chain / 1:1)
//! - [`debug`]      —— ★ NEW: `MC_VEC_DUMP=1` debug logging
//!
//! ## ★ P02 (S1) additions
//! - [`builder_report`] —— Structured diagnostics: `BuilderReport` / `BuildMode` / `BuilderError`
//! - [`visit::McVecBuilder::with_mode`] / [`visit::McVecBuilder::try_build`] use the above types
//!
//! ### Legacy path
//! After P1, legacy.rs is **no longer needed** and can be removed entirely.

pub mod connection;
pub mod debug;
pub mod report; // ★ P02 (S1)
pub mod resolve;
pub mod visit;

// ============================================================================
// Top-level re-exports
// ============================================================================

// Main entry + data structures
pub use visit::{build_mc_vec, build_mc_vec_strict, build_mc_vec_with_report, McVecBuilder};

// ★ P02 additions
pub use report::{
    BuildMode, BuilderError, BuilderReport, DroppedNet, PartialNet, ResolutionOutcome,
    ResolutionRecord,
};

// NetPoint resolution + counter (used by mcviz.rs: np_warn_count / reset_np_warn_count)
pub use resolve::{
    expand_bracket_list, np_warn_count, reset_np_warn_count, resolve_id, resolve_netpoint,
    resolve_netpoint_v2, resolve_path, try_resolve_path, ResolveOutcome,
};

// Debug API
pub use debug::{dump_diff, dump_enabled, dump_input, dump_output};
