// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Routing algorithm slots (P4 + Step 2)
//!
//! ## Sub-modules
//! - [`side`]        —— `ExitSide` exit direction + EntryPoint-aware exit point computation
//! - [`orthogonal`]  —— Manhattan polyline + `OrthogonalRouter` (mainstay for 2-endpoint nets)
//! - [`straight`]    —— straight line (debug) + `StraightRouter`
//! - [`bus_bundle`]  —— bus thick line + tap (★ Step 2: reuses trunk_tap helper)
//! - [`star`]        —— star multi-endpoint + `StarRouter` (Power/Ground/SubModuleIO)
//! - [`trunk_tap`]   —— ★ Step 2 NEW: trunk-tap + pin stub (multi-endpoint Signal)
//!
//! ## Step 2 scheduling change
//! ```ignore
//! // Step 1 (old):                          // Step 2 (new):
//! Signal multi → StarRouter        ──→     Signal multi → TrunkTapRouter
//! ```
//!
//! See [`smart_route_all`] implementation.

pub mod bus_bundle;
pub mod channels;
pub mod dispatch;
pub mod obstacles;
pub mod orthogonal;
pub mod scheduler;
pub mod side;
pub mod star;
pub mod straight;
pub mod trunk_tap; // ★ Step 2
pub use bus_bundle::BusBundleRouter;
pub use orthogonal::{label_anchor, orthogonal_path, points_to_svg_d, OrthogonalRouter};
pub use side::{compute_exit_for_pin, compute_exit_to, ExitSide};
pub use star::StarRouter;
pub use straight::StraightRouter;
pub use trunk_tap::{build_trunk_tap_route, BuildOptions, TrunkTapRouter, PIN_STUB_LEN}; // ★ Step 2
pub mod audit;
pub mod grid_router;
pub mod wire_hops;
// ============================================================================
// Smart scheduling: pick router by NetKind
// ============================================================================

use crate::vector::graph::McVecGraph;

/// Route all nets in graph by picking a router according to NetKind
///
/// Routing result is written into each `net.route`.
///
/// ## Step 2 scheduling rules
/// | NetKind                            | endpoints | Router               |
/// |------------------------------------|-----------|----------------------|
/// | `Bus(_)`                           | any       | `BusBundleRouter`    |
/// | `Power` / `Ground` / `SubModuleIO` | any       | `StarRouter`         |
/// | `Signal`                           | ≤ 2       | `OrthogonalRouter`   |
/// | `Signal`                           | ≥ 3       | `TrunkTapRouter` ★  |
pub fn smart_route_all(graph: &mut McVecGraph) {
    crate::viz::route::dispatch::route_all_with_dispatch(graph);
}
