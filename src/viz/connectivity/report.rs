// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M13 — RenderedConnectivityReport
//!
//! Report for rendered connectivity extraction and verification.

use std::collections::BTreeMap;

use super::model::RenderedConnectivity;

// ============================================================================
// RenderedConnectivityReport
// ============================================================================

/// Report produced after rendered connectivity extraction and verification.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderedConnectivityReport {
    pub is_perfect: bool,

    // Pin stats
    pub pins_total: usize,
    pub pins_reachable: usize,
    pub pins_unreachable: usize,

    // Net stats
    pub nets_total: usize,
    pub nets_perfect: usize,
    pub nets_with_render_mismatch: usize,

    // Connection errors
    pub false_connections: usize,
    pub missing_connections: usize,
    pub false_junctions: usize,
    pub missing_junctions: usize,

    // Crossing stats
    pub different_net_crossings: usize,
    pub different_net_crossings_with_hop: usize,
    pub different_net_crossings_without_hop: usize,
    pub ambiguous_near_misses: usize,

    // Per-net status
    pub per_net: BTreeMap<i64, RenderedNetStatus>,

    // Hash
    pub connectivity_hash: String,

    // Warnings
    pub warnings: Vec<String>,
}

/// Per-net rendered connectivity status.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderedNetStatus {
    pub net_id: i64,
    pub net_name: String,
    pub endpoints_total: usize,
    pub endpoints_reachable: usize,
    pub rendered_components: usize,
    pub expected_components: usize,
    pub is_connected_as_expected: bool,
    pub route_segments: usize,
    pub junctions: usize,
    pub crossings: usize,
    pub hops: usize,
    pub violations: Vec<RenderedConnectivityViolation>,
}

/// A rendered connectivity violation.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedConnectivityViolation {
    pub severity: ViolationSeverity,
    pub kind: ViolationKind,
    pub net_id: i64,
    pub related_net_id: Option<i64>,
    pub endpoint: Option<String>,
    pub position: Option<super::model::Point2D>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViolationSeverity {
    Hard,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    PinUnreachable,
    EndpointMissingAnchor,
    RouteMissing,
    RenderedDisconnected,
    RenderedFalseConnection,
    FalseJunction,
    MissingJunction,
    DifferentNetOverlapWithoutHop,
    DifferentNetTouchingPin,
    AmbiguousNearMiss,
    UnsupportedGeometry,
}

impl RenderedConnectivityReport {
    /// Build from a RenderedConnectivity model.
    pub fn from_connectivity(conn: &RenderedConnectivity) -> Self {
        let mut report = Self {
            pins_total: conn.pin_anchors.len(),
            nets_total: 0,
            warnings: conn.warnings.clone(),
            ..Default::default()
        };

        // Count pins reachable
        report.pins_reachable = conn
            .pin_anchors
            .iter()
            .filter(|p| p.distance_to_nearest_segment <= 0.5)
            .count();
        report.pins_unreachable = report.pins_total - report.pins_reachable;

        report.is_perfect = report.pins_unreachable == 0
            && report.false_connections == 0
            && report.missing_connections == 0
            && report.false_junctions == 0
            && report.different_net_crossings_without_hop == 0;

        report
    }

    /// Merge another report into this one.
    pub fn merge(&mut self, other: &RenderedConnectivityReport) {
        self.pins_total += other.pins_total;
        self.pins_reachable += other.pins_reachable;
        self.pins_unreachable += other.pins_unreachable;
        self.nets_total += other.nets_total;
        self.nets_perfect += other.nets_perfect;
        self.nets_with_render_mismatch += other.nets_with_render_mismatch;
        self.false_connections += other.false_connections;
        self.missing_connections += other.missing_connections;
        self.false_junctions += other.false_junctions;
        self.missing_junctions += other.missing_junctions;
        self.different_net_crossings += other.different_net_crossings;
        self.different_net_crossings_with_hop += other.different_net_crossings_with_hop;
        self.different_net_crossings_without_hop += other.different_net_crossings_without_hop;
        self.ambiguous_near_misses += other.ambiguous_near_misses;
        for (k, v) in &other.per_net {
            self.per_net.entry(*k).or_insert(v.clone());
        }
        self.warnings.extend(other.warnings.clone());
        self.is_perfect = self.is_perfect && other.is_perfect;
    }

    /// Single-line log summary.
    pub fn report_line(&self) -> String {
        format!(
            "[metrics] RENDERED-CONNECTIVITY: perfect={} pins={}/{} nets={}/{} \
             false_conn={} missing_conn={} false_junction={} missing_junction={} \
             crossings={} hops={} no_hop={} hash={}",
            self.is_perfect,
            self.pins_reachable,
            self.pins_total,
            self.nets_perfect,
            self.nets_total,
            self.false_connections,
            self.missing_connections,
            self.false_junctions,
            self.missing_junctions,
            self.different_net_crossings,
            self.different_net_crossings_with_hop,
            self.different_net_crossings_without_hop,
            &self.connectivity_hash[..8.min(self.connectivity_hash.len())],
        )
    }
}
