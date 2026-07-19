// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW (P11, S4) — Tabulated router dispatch
//!
//! ## What problem does this file solve
//! Before S3, how `smart_route_all` dispatched a net to a router was a black box,
//! and multi-endpoint Signals were often wrongly routed via pairwise `OrthogonalRouter`
//! rather than `TrunkTapRouter`, resulting in a 5-endpoint net being drawn as 4
//! independent L-shaped wires (C(5,2)/shared mid ≈ 10 segments).
//!
//! P11 extracts the dispatch rules into a pure function [`pick_router`], tabulating
//! all cases so you can see at a glance "which router this kind of net should go to",
//! with unit tests for each rule.
//!
//! ## Dispatch rules summary
//!
//! | NetKind        | topology       | endpoints | router choice          |
//! |----------------|----------------|-----------|------------------------|
//! | (any)          | Isolated       | ≤ 1       | Noop                  |
//! | Bus(_)         | (any)          | (any)     | BusBundle             |
//! | Power/Ground   | TwoPoint       | 2         | Orthogonal            |
//! | Power/Ground   | StarOneDriver  | ≥ 3       | TrunkTap              |
//! | Power/Ground   | MultiDriver    | ≥ 3       | Star (centroid)       |
//! | Signal         | TwoPoint       | 2         | Orthogonal            |
//! | Signal         | StarOneDriver  | ≥ 3       | TrunkTap              |
//! | Signal         | MultiDriver    | ≥ 3       | TrunkTapWithWarning   |
//! | SubModuleIO    | TwoPoint       | 2         | Orthogonal            |
//! | SubModuleIO    | StarOneDriver  | ≥ 3       | TrunkTap              |
//! | SubModuleIO    | MultiDriver    | ≥ 3       | TrunkTap              |
//!
//! ## Key decisions
//! - **Bus always goes BusBundle**: regardless of endpoint count or topology, a bus
//!   is the thick-line + tap style
//! - **Power/Ground multi-driver** goes Star (radiating from geometric centroid,
//!   multiple sources like a power confluence)
//! - **Signal multi-driver** is a DRC warning (output-on-output short), but **still
//!   drawn**, using TrunkTap, and via [`RouterChoice::should_warn`] letting
//!   smart_route_all emit a stderr warning
//! - **SubModuleIO multi-driver** does not warn (cross-layer semantics ambiguous,
//!   may be legitimate)
//!
//! ## Collaboration with P08 / P09 / P10
//! This version of dispatch only looks at the **net's own semantics**, not aware
//! of obstacles (P09) or channels (P10). The Router trait signature is currently
//! `route(&graph, &mut net)`, with obstacle/channel parameters to be added in later
//! sprints. After P11 changes, it remains fully compatible with the status quo,
//! and the obstacle/channel interface can be added smoothly when P09/P10 land
//! (just append fields to `RouteIntent`).
//!
//! ## Usage (smart_route_all integration)
//! ```ignore
//! use crate::viz::route::dispatch::{pick_router, RouteIntent};
//!
//! pub fn smart_route_all(graph: &mut McVecGraph) {
//!     // 1. Compute intent + pick router for each net (this phase doesn't modify graph)
//!     let plans: Vec<(usize, RouterChoice, String)> = graph.nets.iter().enumerate()
//!         .map(|(i, n)| {
//!             let intent = RouteIntent::from_net(n, graph);
//!             let choice = pick_router(&intent);
//!             (i, choice, n.name.clone())
//!         })
//!         .collect();
//!
//!     // 2. Execute in order (mem::take resolves the borrow)
//!     for (i, choice, name) in plans {
//!         if choice.should_warn() {
//!             crate::vlog!("[route] WARN multi-driver net '{}'", name);
//!         }
//!         let router = choice.into_router();
//!         let mut net = std::mem::take(&mut graph.nets[i]);
//!         router.route(graph, &mut net);
//!         graph.nets[i] = net;
//!     }
//! }
//! ```
//!
//! Note: `mem::take` requires `VizNet: Default`. If VizNet doesn't implement Default,
//! use `std::mem::replace(&mut graph.nets[i], placeholder)` or take nets out of the
//! loop. Currently VizNet doesn't implement Default —— use the swap/replace pattern
//! when integrating (see INTEGRATION.md).
//!
//! ## Sub-layer recursion
//! Not handled in this module —— smart_route_all recurses into `graph.sub_graphs`
//! after dispatching the top layer.

use crate::vector::graph::netdef::{IoDirection, NetTopology};
use crate::vector::graph::{McVecGraph, NetKind, VizNet};

use super::bus_bundle::BusBundleRouter;
use super::orthogonal::OrthogonalRouter;
use super::star::StarRouter;
use super::trunk_tap::TrunkTapRouter;
use crate::viz::traits::{NoopRouter, Router};

// ============================================================================
// RouteIntent — pure-data input for dispatch decision
// ============================================================================

/// A net's "dispatch intent": all info needed to pick a router
///
/// Deliberately a pure-data struct (holds no references), so [`pick_router`] is a
/// pure function and unit tests only need to construct a RouteIntent to verify rules,
/// without building an McVecGraph.
#[derive(Debug, Clone)]
pub struct RouteIntent {
    /// Net's semantic type (Power / Ground / Signal / Bus / SubModuleIO)
    pub kind: NetKind,
    /// Net's topology shape (TwoPoint / StarOneDriver / MultiDriver / Isolated)
    pub topology: NetTopology,
    /// Total endpoint count (≤ 1 = Isolated)
    pub endpoint_count: usize,
    /// Driver endpoint count (IoDirection ∈ Output / Bidir)
    ///
    /// Used for `MultiDriver` warning judgement + future layouter driver-direction inference.
    pub driver_count: usize,
    /// Endpoint horizontal span (max_x - min_x), used for P10 priority sort (larger span reserves trunk first)
    pub span_x: f64,
    /// Endpoint vertical span
    pub span_y: f64,
}

impl RouteIntent {
    /// Build intent from [`VizNet`] + [`McVecGraph`]
    ///
    /// graph is used to look up box coordinates (for span_x / span_y); does not modify graph.
    pub fn from_net(net: &VizNet, graph: &McVecGraph) -> Self {
        let driver_count = net
            .endpoints
            .iter()
            .filter(|e| matches!(e.io_type, IoDirection::Output | IoDirection::Bidir))
            .count();

        // Compute span: use box centers
        let positions: Vec<(f64, f64)> = net
            .endpoints
            .iter()
            .filter_map(|e| {
                graph
                    .boxes
                    .iter()
                    .find(|b| b.id == e.box_id)
                    .map(|b| (b.x + b.w / 2.0, b.y + b.h / 2.0))
            })
            .collect();

        let (span_x, span_y) = if positions.is_empty() {
            (0.0, 0.0)
        } else {
            let min_x = positions.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
            let max_x = positions
                .iter()
                .map(|p| p.0)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = positions.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
            let max_y = positions
                .iter()
                .map(|p| p.1)
                .fold(f64::NEG_INFINITY, f64::max);
            (max_x - min_x, max_y - min_y)
        };

        Self {
            kind: net.kind.clone(),
            topology: net.topology(),
            endpoint_count: net.endpoints.len(),
            driver_count,
            span_x,
            span_y,
        }
    }
}

// ============================================================================
// RouterChoice — dispatch result
// ============================================================================

/// Output of `pick_router`: which specific router to use (as enum)
///
/// Doesn't directly return `Box<dyn Router>` because enums are comparable and easier
/// to test. Callers needing a concrete router call [`RouterChoice::into_router`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterChoice {
    /// Endpoints ≤ 1, no routing needed
    Noop,
    /// Two-point / simple single wire — Manhattan polyline
    Orthogonal,
    /// Multi-endpoint — trunk + tap (Steiner simplification)
    TrunkTap,
    /// Same as TrunkTap, but with DRC warning (Signal multi-driver)
    TrunkTapWithWarning,
    /// Multi-endpoint Power/Ground multi-driver — geometric centroid star
    Star,
    /// Bus (NetKind::Bus(_)) — thick line + tap, trunk extends at both ends
    BusBundle,
}

impl RouterChoice {
    /// Whether to emit a DRC warning to stderr
    pub fn should_warn(&self) -> bool {
        matches!(self, RouterChoice::TrunkTapWithWarning)
    }

    /// Short name (for dump / log)
    pub fn name(&self) -> &'static str {
        match self {
            RouterChoice::Noop => "noop",
            RouterChoice::Orthogonal => "orthogonal",
            RouterChoice::TrunkTap => "trunk_tap",
            RouterChoice::TrunkTapWithWarning => "trunk_tap_warn",
            RouterChoice::Star => "star",
            RouterChoice::BusBundle => "bus_bundle",
        }
    }

    /// Convert to a concrete Router instance (heap-allocated, used for router.route calls)
    pub fn into_router(self) -> Box<dyn Router> {
        match self {
            RouterChoice::Noop => Box::new(NoopRouter),
            RouterChoice::Orthogonal => Box::new(OrthogonalRouter),
            RouterChoice::TrunkTap | RouterChoice::TrunkTapWithWarning => Box::new(TrunkTapRouter),
            RouterChoice::Star => Box::new(StarRouter),
            RouterChoice::BusBundle => Box::new(BusBundleRouter),
        }
    }
}

// ============================================================================
// pick_router — dispatch table (pure function, unit-testable)
// ============================================================================

/// Pick a router based on RouteIntent (P11 dispatch rules)
///
/// Tabulated dispatch logic, with a corresponding unit test for each rule. Bus always
/// goes to BusBundle (regardless of topology), endpoints ≤ 1 always go to Noop,
/// others are dispatched by (kind, topology) combination.
///
/// ## Rule priority (matched top-to-bottom)
/// 1. endpoint_count ≤ 1 → Noop (Isolated)
/// 2. NetKind::Bus(_) → BusBundle (regardless of topology)
/// 3. Dispatch by (kind, topology) table (see table at top of module)
pub fn pick_router(intent: &RouteIntent) -> RouterChoice {
    // ── Rule 1: too few endpoints ──
    if intent.endpoint_count <= 1 || matches!(intent.topology, NetTopology::Isolated) {
        return RouterChoice::Noop;
    }

    // ── Rule 2: Bus always goes BusBundle ──
    if matches!(intent.kind, NetKind::Bus(_)) {
        return RouterChoice::BusBundle;
    }

    // ── Rule 3: (kind, topology) dispatch table ──
    use NetKind::*;
    use NetTopology::*;

    match (&intent.kind, intent.topology) {
        // Power / Ground
        (Power | Ground, TwoPoint) => RouterChoice::Orthogonal,
        (Power | Ground, StarOneDriver) => RouterChoice::TrunkTap,
        (Power | Ground, MultiDriver) => RouterChoice::Star,

        // Ordinary Signal
        (Signal, TwoPoint) => RouterChoice::Orthogonal,
        (Signal, StarOneDriver) => RouterChoice::TrunkTap,
        (Signal, MultiDriver) => RouterChoice::TrunkTapWithWarning,

        // Cross-layer SubModuleIO (after promotion)
        (SubModuleIO, TwoPoint) => RouterChoice::Orthogonal,
        (SubModuleIO, StarOneDriver) => RouterChoice::TrunkTap,
        (SubModuleIO, MultiDriver) => RouterChoice::TrunkTap,

        // Fallback: should not reach (Bus already returned in Rule 2; Isolated in Rule 1)
        (Bus(_), _) => RouterChoice::BusBundle, // Defensive: in case Rule 2 misses
        (_, Isolated) => RouterChoice::Noop,    // Defensive
    }
}

// ============================================================================
// Integration helper — end-to-end function callable directly by smart_route_all
// ============================================================================

/// End-to-end: route all nets of one graph layer following the P11 dispatch rules
///
/// This function encapsulates the complete flow of "compute intent → pick router →
/// call route", using `mem::replace` to resolve the borrow conflict between
/// Router::route's simultaneous need for `&graph` and `&mut net`.
/// (Prerequisite: all existing Router impls only read graph.boxes, not graph.nets ——
/// this invariant currently holds, and after P09/P10 introduces obstacle avoidance
/// it still only reads boxes.)
///
/// **Does not recurse** ── sub-layers (`graph.sub_graphs`) are recursed by
/// [`route_all_with_dispatch`].
pub fn route_layer_with_dispatch(graph: &mut McVecGraph) {
    // First pass: compute each net's intent + choice (immutable borrow)
    let plans: Vec<(usize, RouterChoice, String)> = graph
        .nets
        .iter()
        .enumerate()
        .map(|(i, net)| {
            let intent = RouteIntent::from_net(net, graph);
            let choice = pick_router(&intent);
            (i, choice, net.name.clone())
        })
        .collect();

    crate::vlog!(
        "[route::dispatch] layer '{}' bid={} planned {} nets",
        graph.name,
        graph.bid,
        plans.len()
    );

    // Second pass: execute according to the plan
    for (i, choice, name) in plans {
        // DRC warning
        if choice.should_warn() {
            crate::vlog!(
                "[route::dispatch] WARN net '{name}' has multi-driver Signal topology — \
                 likely DRC violation (output-on-output), routing as trunk_tap anyway"
            );
        }

        if crate::viz::debug::dump_enabled() {
            crate::vlog!("[route::dispatch] net='{}' → {}", name, choice.name());
        }

        // Borrow trick: extract the net so we can have both &graph + &mut net
        // (Router::route doesn't modify graph, so this swap is safe)
        let mut tmp = std::mem::replace(
            &mut graph.nets[i],
            VizNet::new(0, String::new(), NetKind::Signal, Vec::new()),
        );
        let router = choice.into_router();
        router.route(graph, &mut tmp);
        graph.nets[i] = tmp;
    }
}

/// Recursive version: top layer + all sub-layers dispatched together
pub fn route_all_with_dispatch(graph: &mut McVecGraph) {
    route_layer_with_dispatch(graph);
    for sub in &mut graph.sub_graphs {
        route_all_with_dispatch(sub);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::EndpointRef;
    use crate::viz::route::dispatch::IoDirection;

    // ────────────────────────────────────────────────────────────────────────
    // intent builder helper (independent of graph, hand-crafted directly)
    // ────────────────────────────────────────────────────────────────────────

    fn intent(
        kind: NetKind,
        topology: NetTopology,
        endpoint_count: usize,
        driver_count: usize,
    ) -> RouteIntent {
        RouteIntent {
            kind,
            topology,
            endpoint_count,
            driver_count,
            span_x: 100.0,
            span_y: 50.0,
        }
    }

    // ========================================================================
    // Rule 1: Isolated / ≤ 1 endpoint → Noop
    // ========================================================================

    #[test]
    fn dispatch_isolated_zero_endpoint() {
        let i = intent(NetKind::Signal, NetTopology::Isolated, 0, 0);
        assert_eq!(pick_router(&i), RouterChoice::Noop);
    }

    #[test]
    fn dispatch_isolated_one_endpoint() {
        let i = intent(NetKind::Signal, NetTopology::Isolated, 1, 0);
        assert_eq!(pick_router(&i), RouterChoice::Noop);
    }

    #[test]
    fn dispatch_isolated_even_if_power() {
        // Power kind but Isolated topology → still Noop (nothing to draw)
        let i = intent(NetKind::Power, NetTopology::Isolated, 1, 0);
        assert_eq!(pick_router(&i), RouterChoice::Noop);
    }

    // ========================================================================
    // Rule 2: Bus → BusBundle (regardless of topology)
    // ========================================================================

    #[test]
    fn dispatch_bus_twopoint() {
        let i = intent(NetKind::Bus(4), NetTopology::TwoPoint, 2, 1);
        assert_eq!(pick_router(&i), RouterChoice::BusBundle);
    }

    #[test]
    fn dispatch_bus_star() {
        let i = intent(NetKind::Bus(8), NetTopology::StarOneDriver, 5, 1);
        assert_eq!(pick_router(&i), RouterChoice::BusBundle);
    }

    #[test]
    fn dispatch_bus_multi_driver() {
        // Even multi-driver Bus goes BusBundle (common for open-drain bus)
        let i = intent(NetKind::Bus(2), NetTopology::MultiDriver, 4, 2);
        assert_eq!(pick_router(&i), RouterChoice::BusBundle);
    }

    // ========================================================================
    // Rule 3: Power / Ground
    // ========================================================================

    #[test]
    fn dispatch_power_twopoint() {
        let i = intent(NetKind::Power, NetTopology::TwoPoint, 2, 0);
        assert_eq!(pick_router(&i), RouterChoice::Orthogonal);
    }

    #[test]
    fn dispatch_power_star_one_driver() {
        let i = intent(NetKind::Power, NetTopology::StarOneDriver, 5, 1);
        assert_eq!(pick_router(&i), RouterChoice::TrunkTap);
    }

    #[test]
    fn dispatch_power_multi_driver() {
        // Multiple power sources (dual-rail supply confluence) → Star centroid
        let i = intent(NetKind::Power, NetTopology::MultiDriver, 4, 2);
        assert_eq!(pick_router(&i), RouterChoice::Star);
    }

    #[test]
    fn dispatch_ground_twopoint() {
        let i = intent(NetKind::Ground, NetTopology::TwoPoint, 2, 0);
        assert_eq!(pick_router(&i), RouterChoice::Orthogonal);
    }

    #[test]
    fn dispatch_ground_star_one_driver() {
        let i = intent(NetKind::Ground, NetTopology::StarOneDriver, 6, 1);
        assert_eq!(pick_router(&i), RouterChoice::TrunkTap);
    }

    #[test]
    fn dispatch_ground_multi_driver() {
        let i = intent(NetKind::Ground, NetTopology::MultiDriver, 5, 2);
        assert_eq!(pick_router(&i), RouterChoice::Star);
    }

    // ========================================================================
    // Rule 3: Signal
    // ========================================================================

    #[test]
    fn dispatch_signal_twopoint() {
        let i = intent(NetKind::Signal, NetTopology::TwoPoint, 2, 1);
        assert_eq!(pick_router(&i), RouterChoice::Orthogonal);
    }

    #[test]
    fn dispatch_signal_star_one_driver() {
        // 5-endpoint Signal, single driver → TrunkTap (trunk + 4 taps)
        let i = intent(NetKind::Signal, NetTopology::StarOneDriver, 5, 1);
        assert_eq!(pick_router(&i), RouterChoice::TrunkTap);
    }

    #[test]
    fn dispatch_signal_multi_driver_warns() {
        // Signal multi-driver = output-on-output DRC violation → TrunkTap + warning
        let i = intent(NetKind::Signal, NetTopology::MultiDriver, 4, 2);
        let choice = pick_router(&i);
        assert_eq!(choice, RouterChoice::TrunkTapWithWarning);
        assert!(choice.should_warn());
    }

    // ========================================================================
    // Rule 3: SubModuleIO
    // ========================================================================

    #[test]
    fn dispatch_submodule_io_twopoint() {
        let i = intent(NetKind::SubModuleIO, NetTopology::TwoPoint, 2, 0);
        assert_eq!(pick_router(&i), RouterChoice::Orthogonal);
    }

    #[test]
    fn dispatch_submodule_io_star_one_driver() {
        let i = intent(NetKind::SubModuleIO, NetTopology::StarOneDriver, 4, 1);
        assert_eq!(pick_router(&i), RouterChoice::TrunkTap);
    }

    #[test]
    fn dispatch_submodule_io_multi_driver_no_warn() {
        // SubModuleIO multi-driver is legitimate (cross-layer semantics ambiguous), no warning
        let i = intent(NetKind::SubModuleIO, NetTopology::MultiDriver, 4, 2);
        let choice = pick_router(&i);
        assert_eq!(choice, RouterChoice::TrunkTap);
        assert!(!choice.should_warn());
    }

    // ========================================================================
    // should_warn behavior
    // ========================================================================

    #[test]
    fn should_warn_only_for_trunk_tap_with_warning() {
        assert!(!RouterChoice::Noop.should_warn());
        assert!(!RouterChoice::Orthogonal.should_warn());
        assert!(!RouterChoice::TrunkTap.should_warn());
        assert!(RouterChoice::TrunkTapWithWarning.should_warn());
        assert!(!RouterChoice::Star.should_warn());
        assert!(!RouterChoice::BusBundle.should_warn());
    }

    #[test]
    fn router_choice_names() {
        assert_eq!(RouterChoice::Noop.name(), "noop");
        assert_eq!(RouterChoice::Orthogonal.name(), "orthogonal");
        assert_eq!(RouterChoice::TrunkTap.name(), "trunk_tap");
        assert_eq!(RouterChoice::TrunkTapWithWarning.name(), "trunk_tap_warn");
        assert_eq!(RouterChoice::Star.name(), "star");
        assert_eq!(RouterChoice::BusBundle.name(), "bus_bundle");
    }

    // ========================================================================
    // RouteIntent::from_net behavior
    // ========================================================================

    fn ep(box_id: i64, io: IoDirection) -> EndpointRef {
        EndpointRef {
            box_id,
            pin_id: box_id * 10, // any id
            pin_name: String::new(),
            io_type: io,
            pin_number: None,
        }
    }

    #[test]
    fn intent_from_net_counts_drivers() {
        let net = VizNet::new(
            0,
            "x".into(),
            NetKind::Signal,
            vec![
                ep(1, IoDirection::Output),
                ep(2, IoDirection::Input),
                ep(3, IoDirection::Output), // second driver → MultiDriver
                ep(4, IoDirection::Input),
            ],
        );
        let graph = McVecGraph::new(0, "test".into());
        let intent = RouteIntent::from_net(&net, &graph);
        assert_eq!(intent.endpoint_count, 4);
        assert_eq!(intent.driver_count, 2);
        assert_eq!(intent.topology, NetTopology::MultiDriver);
        // Signal multi-driver → warning
        assert_eq!(pick_router(&intent), RouterChoice::TrunkTapWithWarning);
    }

    #[test]
    fn intent_from_net_bidir_counts_as_driver() {
        let net = VizNet::new(
            0,
            "x".into(),
            NetKind::Signal,
            vec![
                ep(1, IoDirection::Bidir),
                ep(2, IoDirection::Bidir),
                ep(3, IoDirection::Input),
            ],
        );
        let graph = McVecGraph::new(0, "test".into());
        let intent = RouteIntent::from_net(&net, &graph);
        assert_eq!(intent.driver_count, 2);
        assert_eq!(intent.topology, NetTopology::MultiDriver);
    }

    #[test]
    fn intent_from_net_unknown_io_not_a_driver() {
        // Before P01 all endpoints had io_type=Unknown, should be treated as non-driver → StarOneDriver
        let net = VizNet::new(
            0,
            "x".into(),
            NetKind::Signal,
            vec![
                ep(1, IoDirection::Unknown),
                ep(2, IoDirection::Unknown),
                ep(3, IoDirection::Unknown),
            ],
        );
        let graph = McVecGraph::new(0, "test".into());
        let intent = RouteIntent::from_net(&net, &graph);
        assert_eq!(intent.driver_count, 0);
        assert_eq!(intent.topology, NetTopology::StarOneDriver);
        // Signal star → TrunkTap (no warning)
        let choice = pick_router(&intent);
        assert_eq!(choice, RouterChoice::TrunkTap);
        assert!(!choice.should_warn());
    }

    #[test]
    fn intent_from_net_twopoint_topology() {
        let net = VizNet::new(
            0,
            "x".into(),
            NetKind::Signal,
            vec![ep(1, IoDirection::Output), ep(2, IoDirection::Input)],
        );
        let graph = McVecGraph::new(0, "test".into());
        let intent = RouteIntent::from_net(&net, &graph);
        assert_eq!(intent.endpoint_count, 2);
        assert_eq!(intent.topology, NetTopology::TwoPoint);
        assert_eq!(pick_router(&intent), RouterChoice::Orthogonal);
    }
}
