// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M12 — Canonical hashing for deterministic comparison
//!
//! All hashes use stable field ordering and quantized floats. Same input
//! produces same hash across repeated runs.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::vector::graph::McVecGraph;

use super::key::{StableBoxKey, StableEndpointKey, StableNetKey};
use super::score::quantized_px;

// ============================================================================
// Canonical hash helpers
// ============================================================================

/// Compute a deterministic hash for a value using DefaultHasher.
pub fn canonical_hash(value: &impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Hash a float with quantization to avoid tiny-noise differences.
pub fn hash_f64(hasher: &mut DefaultHasher, v: f64) {
    quantized_px(v).hash(hasher);
}

// ============================================================================
// Graph component hashes
// ============================================================================

/// Hash box order deterministically.
pub fn hash_box_order(graph: &McVecGraph) -> String {
    let mut hasher = DefaultHasher::new();
    for (i, b) in graph.boxes.iter().enumerate() {
        let key = StableBoxKey::from_box(b, i);
        key.box_id.hash(&mut hasher);
        key.name.hash(&mut hasher);
        key.symbol_rank.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Hash net order deterministically.
pub fn hash_net_order(graph: &McVecGraph) -> String {
    let mut hasher = DefaultHasher::new();
    for (i, n) in graph.nets.iter().enumerate() {
        let key = StableNetKey::from_net(n, i);
        key.net_id.hash(&mut hasher);
        key.name.hash(&mut hasher);
        key.kind_rank.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Hash box geometry deterministically (quantized).
pub fn hash_box_geometry(graph: &McVecGraph) -> String {
    let mut hasher = DefaultHasher::new();
    for b in &graph.boxes {
        b.id.hash(&mut hasher);
        hash_f64(&mut hasher, b.x);
        hash_f64(&mut hasher, b.y);
        hash_f64(&mut hasher, b.w);
        hash_f64(&mut hasher, b.h);
    }
    format!("{:016x}", hasher.finish())
}

/// Hash pin anchors deterministically.
pub fn hash_pin_anchors(graph: &McVecGraph) -> String {
    use crate::vector::graph::EntrySide;
    let mut hasher = DefaultHasher::new();
    for b in &graph.boxes {
        b.id.hash(&mut hasher);
        // entry_points is Vec<EntryPoint>, sort deterministically
        let mut eps: Vec<_> = b.entry_points.iter().collect();
        eps.sort_by_key(|ep| ep.pin_id);
        for ep in &eps {
            ep.pin_id.hash(&mut hasher);
            let side_idx: u8 = match ep.side {
                EntrySide::Top => 0,
                EntrySide::Bottom => 1,
                EntrySide::Left => 2,
                EntrySide::Right => 3,
            };
            side_idx.hash(&mut hasher);
            hash_f64(&mut hasher, ep.offset);
        }
    }
    format!("{:016x}", hasher.finish())
}

/// Hash route geometry (segments) deterministically.
pub fn hash_route_geometry(graph: &McVecGraph) -> String {
    let mut hasher = DefaultHasher::new();
    for net in &graph.nets {
        net.nid.hash(&mut hasher);
        // Sort endpoints deterministically
        let mut eps: Vec<_> = net
            .endpoints
            .iter()
            .enumerate()
            .map(|(i, ep)| StableEndpointKey::from_endpoint(net.nid, ep, i))
            .collect();
        eps.sort();
        for ep in &eps {
            ep.box_id.hash(&mut hasher);
            ep.pin_id.hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

/// Hash idiom instances deterministically.
pub fn hash_idiom_instances(instances: &[super::super::idiom::model::IdiomInstance]) -> String {
    let mut hasher = DefaultHasher::new();
    for inst in instances {
        (inst.kind as u8).hash(&mut hasher);
        inst.anchor_box_id.hash(&mut hasher);
        for &sid in &inst.satellite_box_ids {
            sid.hash(&mut hasher);
        }
        inst.anchor_pin_id.hash(&mut hasher);
        inst.signal_net_id.hash(&mut hasher);
        inst.power_net_id.hash(&mut hasher);
        hash_f64(&mut hasher, inst.confidence);
    }
    format!("{:016x}", hasher.finish())
}

/// Hash placement constraints deterministically.
pub fn hash_placement_constraints(
    constraints: &[super::super::idiom::model::PlacementConstraint],
) -> String {
    let mut hasher = DefaultHasher::new();
    for c in constraints {
        (c.kind as u8).hash(&mut hasher);
        c.target_box_id.hash(&mut hasher);
        c.anchor_box_id.hash(&mut hasher);
        c.priority.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Hash placement decisions (selected candidates) deterministically.
pub fn hash_placement_decisions(
    decisions: &[super::super::idiom::model::PlacementDecisionRecord],
) -> String {
    let mut hasher = DefaultHasher::new();
    for d in decisions {
        (d.source_kind as u8).hash(&mut hasher);
        d.target_box_id.hash(&mut hasher);
        d.anchor_box_id.hash(&mut hasher);
        d.candidate_index.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Hash the full metrics-relevant graph state.
pub fn hash_metrics(graph: &McVecGraph) -> String {
    // Combine box geometry + net order into a single metrics hash
    let mut hasher = DefaultHasher::new();
    let box_hash = hash_box_geometry(graph);
    let net_hash = hash_net_order(graph);
    box_hash.hash(&mut hasher);
    net_hash.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::IoSummary;
    use crate::vector::graph::{
        BoxKind, EndpointRef, McVecBox, McVecGraph, NetKind, Symbol, VizNet,
    };

    fn make_graph() -> McVecGraph {
        let mut graph = McVecGraph::new(1, "test".into());
        let mut b1 = McVecBox::new_v2(
            1,
            "B1".into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Ic,
            None,
            None,
            2,
            IoSummary::new(),
        );
        b1.x = 10.0;
        b1.y = 20.0;
        b1.w = 100.0;
        b1.h = 80.0;
        let b2 = {
            let mut b = McVecBox::new_v2(
                2,
                "B2".into(),
                "".into(),
                BoxKind::TwoPin,
                Symbol::Resistor,
                None,
                None,
                2,
                IoSummary::new(),
            );
            b.x = 200.0;
            b.y = 20.0;
            b.w = 40.0;
            b.h = 30.0;
            b
        };
        graph.boxes.push(b1);
        graph.boxes.push(b2);
        graph.nets.push(VizNet::new(
            1,
            "VDD".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "VDD"), EndpointRef::new(2, 2, "1")],
        ));
        graph
    }

    #[test]
    fn hash_box_order_repeatable() {
        let graph = make_graph();
        let h1 = hash_box_order(&graph);
        let h2 = hash_box_order(&graph);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_net_order_repeatable() {
        let graph = make_graph();
        let h1 = hash_net_order(&graph);
        let h2 = hash_net_order(&graph);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_box_geometry_repeatable() {
        let graph = make_graph();
        let h1 = hash_box_geometry(&graph);
        let h2 = hash_box_geometry(&graph);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_insensitive_to_tiny_float_noise() {
        let mut graph = make_graph();
        let h1 = hash_box_geometry(&graph);
        // Slightly perturb a coordinate (within 0.01px)
        graph.boxes[0].x = 10.00001;
        let h2 = hash_box_geometry(&graph);
        assert_eq!(h1, h2, "Tiny float noise should not change hash");
    }
}
