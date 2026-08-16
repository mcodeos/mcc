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
//! ## Integration point
//! Called from `phase_prepare` in flow.rs, after `classify_rails` and before
//! `assign_default_sizes` / `assign_entry_points_coarse`.

use std::collections::HashSet;

use crate::vector::graph::{BoxKind, McVecGraph};

/// Apply pin facade to all SubModule boxes in the graph.
///
/// Prints a [facade] line per box that was trimmed.
pub fn pin_facade(graph: &mut McVecGraph) {
    // Collect all (box_id, pin_id) pairs from nets.
    let mut used_pins: HashSet<(i64, i64)> = HashSet::new();
    for net in &graph.nets {
        for ep in &net.endpoints {
            if ep.pin_id > 0 {
                used_pins.insert((ep.box_id, ep.pin_id));
            }
        }
    }

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

        // Step 1: collect used pin_ids for this box, collapsed to port groups.
        let mut kept: HashSet<i64> = HashSet::new();
        for &(box_id, pin_id) in &used_pins {
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
