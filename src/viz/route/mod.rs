// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Routing algorithm slots (P4 + Step 2)
//!
//! ## Sub-modules
//! - [`side`]        —— `ExitSide` exit direction + EntryPoint-aware exit point computation
//! - [`orthogonal`]  —— Manhattan polyline + `OrthogonalRouter` (mainstay for 2-endpoint nets)
//! - [`bus_bundle`]  —— bus thick line + tap (★ Step 2: reuses trunk_tap helper)
//! - [`star`]        —— star multi-endpoint + `StarRouter` (Power/Ground/SubModuleIO)
//! - [`trunk_tap`]   —— ★ Step 2 NEW: trunk-tap + pin stub (multi-endpoint Signal)
//!
//! ## Step 2 scheduling change
//! ```ignore
//! // Step 1 (old):                          // Step 2 (new):
//! Signal multi → StarRouter        ──→     Signal multi → TrunkTapRouter
//! ```

pub mod audit;
pub mod bus_bundle;
pub mod channels;
pub mod dispatch;
pub mod feedback;
pub mod grid_router;
pub mod obstacles;
pub mod orthogonal;
pub mod scheduler;
pub mod side;
pub mod star;
pub mod trunk_tap;
pub mod wire_hops;
pub mod wire_label_split;
pub use orthogonal::{label_anchor, orthogonal_path, points_to_svg_d};
pub use side::{compute_exit_for_pin, compute_exit_to, ExitSide};
pub use star::StarRouter;
pub use trunk_tap::{build_trunk_tap_route, BuildOptions, PIN_STUB_LEN};
