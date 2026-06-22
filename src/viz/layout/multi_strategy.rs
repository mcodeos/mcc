// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Multi-strategy scheduler: per connected component pick chain / radial layout
//!
//! This is the original `GridPlacer::layout` behavior, now wrapped as a [`Layouter`] trait impl.
//! It's the P2 "default Layouter", keeping the name `RadialLayouter` because for multi-component cases
//! the main circuit usually goes radial.
//!
//! ## Flow
//! ```text
//! 1. assign_default_sizes
//! 2. build_adjacency + build_degrees
//! 3. find_connected_components → multi / single-component groups
//! 4. Multi-box components sorted by box count desc, laid out one by one:
//!    a. try_linearize_chain → is chain → layout_chain_horizontal
//!    b. otherwise → find_hub + bfs_rings + place_ring + place_ring2 + place_unconnected
//! 5. Single-box components grouped into a row below
//! 6. Cross-component resolve_overlaps_iterative
//! 7. normalize_positions + compute_canvas
//! ```

use std::collections::HashSet;

use crate::vector::graph::McVecGraph;

use super::chain::{layout_chain_horizontal, try_linearize_chain};
use super::components::{
    build_adjacency, build_degrees, find_connected_components, partition_components,
};
use super::entry_points::assign_entry_points;
use super::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use super::overlap::resolve_overlaps_iterative;
use super::radial::{
    bfs_rings_in_subset, find_hub_in_subset, place_ring, place_ring2, place_unconnected,
    set_center, RING1_RADIUS, RING2_RADIUS,
};
use super::size::assign_default_sizes;
use crate::viz::traits::Layouter;

// ============================================================================
// RadialLayouter (multi-strategy scheduler)
// ============================================================================

/// Multi-strategy scheduler (default Layouter)
///
/// This is the default layout algorithm used by P2, replacing the old `LegacyLayouter` (which only wrapped GridPlacer).
pub struct RadialLayouter;

impl Layouter for RadialLayouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        // 1. Set box sizes
        assign_default_sizes(graph);
        assign_entry_points(graph);

        // Degenerate: single box centered directly
        if graph.boxes.len() == 1 {
            graph.boxes[0].x = 200.0;
            graph.boxes[0].y = 100.0;
            return (
                200.0 + graph.boxes[0].w + 100.0,
                200.0 + graph.boxes[0].h + 100.0,
            );
        }

        // 2. Adjacency list + degrees
        let adj = build_adjacency(graph);
        let degrees = build_degrees(graph, &adj);

        // 3. Connected component partition
        let all_components = find_connected_components(&graph.boxes, &adj);
        let (multi_comps, single_comps) = partition_components(all_components);

        eprintln!(
            "[layout::radial] components: multi={} (sizes={:?}), singleton={}",
            multi_comps.len(),
            multi_comps.iter().map(|c| c.len()).collect::<Vec<_>>(),
            single_comps.len()
        );

        // 4. Layout per component, stacked vertically
        const COMPONENT_ROW_GAP: f64 = 80.0;
        let canvas_left: f64 = 60.0;
        let mut cur_y: f64 = 60.0;
        let mut max_right: f64 = 0.0;

        for comp in &multi_comps {
            let comp_set: HashSet<i64> = comp.iter().copied().collect();

            // 4a. Try linearize as chain first
            if let Some(chain_order) = try_linearize_chain(comp, &adj) {
                eprintln!(
                    "[layout::radial]   chain detected: {} boxes → linear placement",
                    chain_order.len()
                );
                let (used_w, used_h) =
                    layout_chain_horizontal(graph, &chain_order, canvas_left, cur_y);
                max_right = max_right.max(canvas_left + used_w);
                cur_y += used_h + COMPONENT_ROW_GAP;
                continue;
            }

            // 4b. Non-chain component: go radial
            let hub_id = find_hub_in_subset(&graph.boxes, &degrees, &comp_set);
            let (ring1, ring2, unplaced) = bfs_rings_in_subset(hub_id, &adj, &comp_set);

            let max_ring = if !ring2.is_empty() || !unplaced.is_empty() {
                RING2_RADIUS
            } else {
                RING1_RADIUS
            };
            let local_cx = canvas_left + max_ring + 80.0;
            let local_cy = cur_y + max_ring + 40.0;

            set_center(graph, hub_id, local_cx, local_cy);
            place_ring(
                graph,
                &ring1,
                local_cx,
                local_cy,
                RING1_RADIUS,
                &adj,
                hub_id,
            );
            if !ring2.is_empty() {
                place_ring2(
                    graph,
                    &ring2,
                    &ring1,
                    local_cx,
                    local_cy,
                    RING2_RADIUS,
                    &adj,
                );
            }
            if !unplaced.is_empty() {
                place_unconnected(graph, &unplaced, local_cx, local_cy, max_ring + 80.0);
            }

            max_right = max_right.max(local_cx + max_ring + 40.0);
            cur_y = local_cy + max_ring + COMPONENT_ROW_GAP;
        }

        // 5. Isolated components grouped into a row below
        if !single_comps.is_empty() {
            let singletons: Vec<i64> = single_comps.into_iter().flatten().collect();
            let canvas_mid_x = (canvas_left + max_right) / 2.0;
            place_unconnected(graph, &singletons, canvas_mid_x, cur_y, 0.0);
        }

        // 6. Cross-component overlap removal
        resolve_overlaps_iterative(graph, 30);

        // 7. Normalize coordinates + compute canvas
        let _ = CANVAS_MARGIN; // (from normalize, here just to avoid dead_code warning on the use)
        normalize_positions(graph);
        compute_canvas(graph)
    }

    fn name(&self) -> &'static str {
        "radial_multi_strategy"
    }
}
