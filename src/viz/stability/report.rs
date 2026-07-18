// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M12 — DeterminismReport and StabilityReport
//!
//! Hash-based reports for repeated-run determinism verification.

use crate::vector::graph::McVecGraph;

use super::hash;

// ============================================================================
// DeterminismReport
// ============================================================================

/// Hash-based determinism report. Compare two reports from repeated runs
/// to verify deterministic output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeterminismReport {
    pub graph_input_hash: String,
    pub box_order_hash: String,
    pub net_order_hash: String,
    pub idiom_instance_hash: String,
    pub placement_constraint_hash: String,
    pub placement_decision_hash: String,
    pub pin_anchor_hash: String,
    pub route_schedule_hash: String,
    pub route_geometry_hash: String,
    pub metrics_hash: String,
    pub unstable_decisions: usize,
    pub warnings: Vec<String>,
}

impl DeterminismReport {
    /// Build from a graph and optional idiom/placement data.
    pub fn from_graph(graph: &McVecGraph) -> Self {
        Self {
            graph_input_hash: String::new(),
            box_order_hash: hash::hash_box_order(graph),
            net_order_hash: hash::hash_net_order(graph),
            idiom_instance_hash: String::new(),
            placement_constraint_hash: String::new(),
            placement_decision_hash: String::new(),
            pin_anchor_hash: hash::hash_pin_anchors(graph),
            route_schedule_hash: String::new(),
            route_geometry_hash: hash::hash_route_geometry(graph),
            metrics_hash: hash::hash_metrics(graph),
            unstable_decisions: 0,
            warnings: Vec::new(),
        }
    }

    /// Set idiom-related hashes.
    pub fn with_idiom(
        mut self,
        instances: &[super::super::idiom::model::IdiomInstance],
        constraints: &[super::super::idiom::model::PlacementConstraint],
        decisions: &[super::super::idiom::model::PlacementDecisionRecord],
    ) -> Self {
        self.idiom_instance_hash = hash::hash_idiom_instances(instances);
        self.placement_constraint_hash = hash::hash_placement_constraints(constraints);
        self.placement_decision_hash = hash::hash_placement_decisions(decisions);
        self
    }

    /// Single-line log summary.
    pub fn report_line(&self) -> String {
        format!(
            "[metrics] DETERMINISM: unstable={} box_hash={} net_hash={} idiom_hash={} \
             placement_hash={} pin_hash={} route_hash={} metrics_hash={}",
            self.unstable_decisions,
            &self.box_order_hash[..8.min(self.box_order_hash.len())],
            &self.net_order_hash[..8.min(self.net_order_hash.len())],
            &self.idiom_instance_hash[..8.min(self.idiom_instance_hash.len())],
            &self.placement_decision_hash[..8.min(self.placement_decision_hash.len())],
            &self.pin_anchor_hash[..8.min(self.pin_anchor_hash.len())],
            &self.route_geometry_hash[..8.min(self.route_geometry_hash.len())],
            &self.metrics_hash[..8.min(self.metrics_hash.len())],
        )
    }
}

// ============================================================================
// StabilityReport (for small-edit locality)
// ============================================================================

/// Soft tracking of locality under small changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StabilityReport {
    pub unchanged_boxes_total: usize,
    pub unchanged_boxes_moved: usize,
    pub max_unchanged_box_delta: f64,
    pub route_hashes_changed: usize,
    pub locality_warning: bool,
}

impl StabilityReport {
    pub fn report_line(&self) -> String {
        format!(
            "[metrics] STABILITY: unchanged_boxes={}/{} max_delta={:.1} \
             route_hashes_changed={} locality_warning={}",
            self.unchanged_boxes_moved,
            self.unchanged_boxes_total,
            self.max_unchanged_box_delta,
            self.route_hashes_changed,
            self.locality_warning,
        )
    }
}
