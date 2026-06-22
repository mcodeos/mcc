// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Updated (Step 2 + P09 + P10) —— Bus router (reuses trunk_tap helper)
//!
//! ## Motivation
//! For an 8-wire `data[7:0]` bus, the old router drew 8 independent thin wires,
//! taking up space and looking messy. Schematic industry convention:
//! - **One thick line** represents the whole bus (trunk)
//! - A short tap at each endpoint
//! - The bit number `[3]` is labeled at the tap end
//!
//! ## Step 2 changes
//! Bus routing is essentially a "thick-line" variant of trunk-tap. The whole algorithm
//! is moved to `trunk_tap`. This file only keeps:
//! - Preparing `(exit_pt, exit_side)` data
//! - Calling `build_trunk_tap_route` with `trunk_overhang = 12.0` (trunk extension)
//!
//! Also automatically gains the **pin stub** improvement introduced in Step 2
//! (the pin exit walks a short distance before turning).
//!
//! ## ★ P09 (S5) changes
//! Pass `ObstacleMap` through to `build_trunk_tap_route`; the bus trunk now also
//! avoids obstacles.
//!
//! ## ★ P10 (S6) changes
//! New `route_bus_bundle_with_channels`: scheduler passes in the built ChannelMap,
//! multiple Buses naturally stagger on different channel slots (Bus has the highest
//! priority and claims positions first).
//!
//! ## Difference from `trunk_tap`
//! The only difference is `trunk_overhang: 12.0` —— the Bus trunk extends 12px past
//! both ends, visually making the "thick line extend past the tap", looking more like
//! a real bus trunk. Signal uses 0.0 (no need to extend).
//!
//! ## Render side
//! Thick line / color / bit-number width (`name [n]`) is all handled automatically by
//! `viz::render::wire::render_viznet` based on `NetKind::Bus(n)`; this router doesn't
//! touch them.

use crate::vector::graph::{McVecGraph, Route, VizNet};

use super::channels::ChannelMap;
use super::obstacles::ObstacleMap;
use super::side::{compute_exit_for_pin, ExitSide};
use super::trunk_tap::{build_trunk_tap_route, BuildOptions};
use crate::viz::traits::Router;

/// Bus trunk extension length on both ends (thick line extends past outermost tap,
/// visually like a real bus trunk)
const BUS_TRUNK_OVERHANG: f64 = 12.0;

pub struct BusBundleRouter;

impl Router for BusBundleRouter {
    fn route(&self, graph: &McVecGraph, net: &mut VizNet) {
        if net.endpoints.len() < 2 {
            net.route = Some(Route::new());
            return;
        }

        let exits: Vec<((f64, f64), ExitSide)> = net
            .endpoints
            .iter()
            .filter_map(|e| {
                graph
                    .boxes
                    .iter()
                    .find(|b| b.id == e.box_id)
                    .map(|b| compute_exit_for_pin(b, e.pin_id, None))
            })
            .collect();

        if exits.len() < 2 {
            net.route = Some(Route::new());
            return;
        }

        // P09 only (no channels in stand-alone trait call)
        let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
        let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);

        let route = build_trunk_tap_route(
            &exits,
            BuildOptions {
                trunk_overhang: BUS_TRUNK_OVERHANG,
                obstacles: Some(&obstacles),
                net_id: net.nid,
                ..Default::default()
            },
        );

        net.route = Some(route);
    }

    fn name(&self) -> &'static str {
        "bus_bundle"
    }
}

// ============================================================================
// ★ P10 (S6) channel-aware end-to-end entry
// ============================================================================

/// P10 main entry: bus routing with ChannelMap
pub fn route_bus_bundle_with_channels(
    graph: &McVecGraph,
    net: &mut VizNet,
    channels: &mut ChannelMap,
) {
    if net.endpoints.len() < 2 {
        net.route = Some(Route::new());
        return;
    }

    let exits: Vec<((f64, f64), ExitSide)> = net
        .endpoints
        .iter()
        .filter_map(|e| {
            graph
                .boxes
                .iter()
                .find(|b| b.id == e.box_id)
                .map(|b| compute_exit_for_pin(b, e.pin_id, None))
        })
        .collect();

    if exits.len() < 2 {
        net.route = Some(Route::new());
        return;
    }

    let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
    let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);
    let net_id = net.nid;

    let route = build_trunk_tap_route(
        &exits,
        BuildOptions {
            trunk_overhang: BUS_TRUNK_OVERHANG,
            obstacles: Some(&obstacles),
            channels: Some(channels),
            net_id,
        },
    );

    net.route = Some(route);
}
