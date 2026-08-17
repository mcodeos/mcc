// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-9 · pin facade pass (root cause L)
//!
//! Filters SubModule box pins to only those that appear in this layer's nets.
//! Collapses member ports (SCL/SDA → I2C0) to port groups, and removes pins
//! whose nets are R-3 (consumer-side power, no edge drawn).
//!
//! ## Algorithm
//! 1. For each SubModule box, collect pin_ids that appear in any net's endpoints
//! 2. Collapse member ports to their parent port groups (via `graph.pin_parent`)
//! 3. Remove pins whose nets are R-3 (power rail without driver, no edge)
//!
//! ## Root layer (P9-B Step 4)
//! For the root layer, the facade uses edges (from `edge_decide::decide_edges`)
//! instead of raw nets. Each edge represents a merged connection between two
//! boxes; the facade resolves the edge's pin-level nets to port groups.
//! Raw pin numbers (8/9/10/11) are already folded into groups by R-M.
//!
//! ## Integration point
//! Called from `phase_prepare` in flow.rs, after `classify_rails` and before
//! `assign_default_sizes` / `assign_entry_points_coarse`.

use std::collections::HashSet;

use crate::vector::graph::{BoxKind, McVecGraph};

/// Apply pin facade to all SubModule boxes in the graph.
///
/// For the root layer, uses edges to decide which pins to show.
/// For sub-layers, uses nets directly.
///
/// Prints a [facade] line per box that was trimmed.
pub fn pin_facade(graph: &mut McVecGraph) {
    if graph.is_root {
        pin_facade_root(graph);
    } else {
        pin_facade_sub(graph);
    }
}

/// Root-layer facade: use edges (from edge_decide) to decide which pins to show.
///
/// Algorithm:
/// 1. Call `decide_edges` to get merged edges between boxes.
/// 2. For each edge, find the pin-level nets connecting the two boxes.
/// 3. Collapse to port groups via `pin_parent`.
/// 4. Raw pin numbers (8/9/10/11) are already merged into port groups by R-M.
fn pin_facade_root(graph: &mut McVecGraph) {
    let (edges, _report) = super::edge_decide::decide_edges(graph);

    // Build set of connected box pairs from edges.
    let edge_pairs: HashSet<(i64, i64)> = edges
        .iter()
        .map(|e| {
            if e.from_box < e.to_box {
                (e.from_box, e.to_box)
            } else {
                (e.to_box, e.from_box)
            }
        })
        .collect();

    // Collect used pins: for each net, check if its endpoints span a box pair
    // that appears in the edge list. Only then are the pins "used".
    let mut used_pins: HashSet<(i64, i64)> = HashSet::new();
    for net in &graph.nets {
        // Find all unique box_ids in this net's endpoints.
        let net_boxes: Vec<i64> = {
            let mut seen = HashSet::new();
            let mut v = Vec::new();
            for ep in &net.endpoints {
                if ep.pin_id > 0 && seen.insert(ep.box_id) {
                    v.push(ep.box_id);
                }
            }
            v
        };

        // Check if any pair of boxes in this net matches an edge pair.
        let mut net_matches_edge = false;
        for i in 0..net_boxes.len() {
            for j in (i + 1)..net_boxes.len() {
                let pair = if net_boxes[i] < net_boxes[j] {
                    (net_boxes[i], net_boxes[j])
                } else {
                    (net_boxes[j], net_boxes[i])
                };
                if edge_pairs.contains(&pair) {
                    net_matches_edge = true;
                    break;
                }
            }
            if net_matches_edge {
                break;
            }
        }

        if net_matches_edge {
            for ep in &net.endpoints {
                if ep.pin_id > 0 {
                    used_pins.insert((ep.box_id, ep.pin_id));
                }
            }
        }
    }

    apply_facade_filter(graph, &used_pins);
}

/// Sub-layer facade: use nets directly to decide which pins to show.
fn pin_facade_sub(graph: &mut McVecGraph) {
    let mut used_pins: HashSet<(i64, i64)> = HashSet::new();
    for net in &graph.nets {
        for ep in &net.endpoints {
            if ep.pin_id > 0 {
                used_pins.insert((ep.box_id, ep.pin_id));
            }
        }
    }

    apply_facade_filter(graph, &used_pins);
}

/// Common facade filter: for each SubModule box, keep only pins in used_pins,
/// collapse to port groups, and remove R-3 pins.
fn apply_facade_filter(graph: &mut McVecGraph, used_pins: &HashSet<(i64, i64)>) {
    // Collect R-3 pin_ids (power rails without driver, consumer side).
    let r3_pins: HashSet<i64> = graph
        .nets
        .iter()
        .filter(|n| {
            n.rail.as_ref().map_or(false, |r| {
                r.class == crate::vector::model::RailClass::Power && r.driver_pin.is_none()
            })
        })
        .flat_map(|n| n.endpoints.iter().map(|ep| ep.pin_id))
        .collect();

    for b in &mut graph.boxes {
        if b.kind != BoxKind::SubModule {
            continue;
        }

        // Collect used pin_ids for this box, collapsed to port groups.
        let mut kept: HashSet<i64> = HashSet::new();
        for &(box_id, pin_id) in used_pins {
            if box_id != b.id {
                continue;
            }
            // Collapse member port → parent port group.
            let resolved = graph.pin_parent.get(&pin_id).copied().unwrap_or(pin_id);
            // Remove R-3 pins.
            if r3_pins.contains(&pin_id) {
                continue;
            }
            kept.insert(resolved);
        }

        let old_count = b.pins.len();
        let dropped: Vec<String> = b
            .pins
            .iter()
            .filter(|p| !kept.contains(&p.id))
            .map(|p| p.pin_id.clone())
            .collect();
        b.pins.retain(|p| kept.contains(&p.id));
        let new_count = b.pins.len();

        if new_count < old_count {
            crate::vlog!(
                "[facade] '{}' pins {} -> {} (dropped: {})",
                b.name,
                old_count,
                new_count,
                dropped.join(", ")
            );
        }
    }
}
