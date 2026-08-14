// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # v2 · strangler pattern
//!
//! New layout pipeline, enabled via the `MC_LAYOUT_V2=1` environment variable.
//! The default takes the old path, with not one line of the old path changed,
//! until the align metrics are all green.
//!
//! ## Architecture
//!
//! ```text
//! solve(graph) → Plan
//!   ├── zone::build_zone_tree(graph)     → ZoneTree    (M2-2)
//!   ├── {zone rough placement}           → ZonePlan[]  (M2-3)
//!   ├── quotient::build(graph, zones)    → quotient graph (M3)
//!   ├── arrange::layers(quotient)        → Arrangement (M3)
//!   └── cutset::decide(graph, zones)     → CutDecision (M4)
//!
//! geom::apply(graph, &plan)  ← the sole geometry writer
//! ```

pub mod arrange;
pub mod cutset;
pub mod geom;
pub mod plan;
pub mod quotient;
pub mod zone;
pub mod zone_placement;

use crate::vector::graph::McVecGraph;
use plan::Plan;

/// Search entry: analyze graph → produce a Plan.
pub fn solve(graph: &McVecGraph) -> Plan {
    let tree = zone::ZoneTree::build(graph);
    let zone_plans = zone_placement::place_zones(graph, &tree, graph.is_submodule);
    let canvas = zone_placement::compute_canvas(&zone_plans, graph.is_submodule);

    // ── M3: build the quotient graph and layer-arrange each zone ──
    let mut arrangements: Vec<plan::Arrangement> = Vec::new();
    for zp in &zone_plans {
        if zp.box_ids.is_empty() {
            continue;
        }
        let q = quotient::QuotientGraph::build_for_ids(graph, &zp.box_ids);
        if q.nodes.is_empty() {
            continue;
        }
        // Exact search (N ≤ 7); skip beyond the limit
        if q.nodes.len() <= arrange::EXACT_SEARCH_LIMIT {
            let candidates = arrange::solve(&q);
            if let Some((_cost, best)) = candidates.into_iter().next() {
                arrangements.push(plan::Arrangement {
                    zone: zp.zone,
                    layers: best.layers,
                });
            }
        }
    }

    Plan {
        zones: zone_plans,
        cuts: Vec::new(),
        arrangements,
        canvas,
    }
}