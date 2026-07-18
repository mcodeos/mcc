// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M13 — Rendered connectivity data model
//!
//! Defines the core types for rendered connectivity: pins, segments,
//! junctions, crossings, hops, and the connectivity graph.

use crate::vector::graph::{EndpointRef, EntrySide, McVecGraph};

// ============================================================================
// Geometry primitives
// ============================================================================

/// A 2D point with quantized key for deterministic ordering.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn quantized_key(&self) -> (i64, i64) {
        let qx = (self.x * 10.0).round() as i64;
        let qy = (self.y * 10.0).round() as i64;
        (qx, qy)
    }
}

// ============================================================================
// RenderedPin
// ============================================================================

/// A pin anchor extracted from the final rendered graph.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPin {
    pub endpoint: EndpointRef,
    pub net_id: i64,
    pub box_id: i64,
    pub pin_id: i64,
    pub pin_name: String,
    pub anchor: Point2D,
    pub side: EntrySide,
    pub reachable_segment_ids: Vec<usize>,
    pub distance_to_nearest_segment: f64,
}

// ============================================================================
// RenderedSegment
// ============================================================================

/// A wire segment extracted from the final route.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedSegment {
    pub segment_id: usize,
    pub net_id: i64,
    pub net_name: String,
    pub from: Point2D,
    pub to: Point2D,
    pub orientation: SegmentOrientation,
    pub source_route_index: usize,
    pub source_segment_index: usize,
    pub is_hop_segment: bool,
}

/// Orientation of a rendered segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentOrientation {
    Horizontal,
    Vertical,
    Diagonal,
    Degenerate,
}

// ============================================================================
// RenderedJunction
// ============================================================================

/// A junction point where segments meet.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedJunction {
    pub junction_id: usize,
    pub position: Point2D,
    pub net_id: i64,
    pub touching_segment_ids: Vec<usize>,
    pub touching_pin_ids: Vec<i64>,
    pub degree: usize,
    pub rendered_dot_expected: bool,
    pub rendered_dot_present: bool,
    pub kind: JunctionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JunctionKind {
    PinConnection,
    EndpointJoin,
    TJunction,
    MultiwayJunction,
    FalseJunction,
    Ambiguous,
}

// ============================================================================
// RenderedCrossing
// ============================================================================

/// A crossing between two different net segments.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedCrossing {
    pub crossing_id: usize,
    pub position: Point2D,
    pub net_a: i64,
    pub net_b: i64,
    pub segment_a: usize,
    pub segment_b: usize,
    pub has_hop: bool,
    pub has_junction_dot: bool,
    pub expected_visual: CrossVisual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossVisual {
    SameNetJoin,
    DifferentNetCrossNoConnect,
    DifferentNetCrossWithHop,
    IllegalOverlap,
    AmbiguousNearMiss,
}

// ============================================================================
// RenderedHop
// ============================================================================

/// A rendered hop (bridge) at a crossing.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedHop {
    pub hop_id: usize,
    pub crossing_id: usize,
    pub over_net_id: i64,
    pub under_net_id: i64,
    pub center: Point2D,
    pub radius: f64,
    pub source_segment_ids: Vec<usize>,
}

// ============================================================================
// RenderedConnectivityGraph
// ============================================================================

/// A graph built from rendered geometry primitives.
#[derive(Debug, Clone, Default)]
pub struct RenderedConnectivityGraph {
    pub nodes: Vec<ConnectivityNode>,
    pub edges: Vec<ConnectivityEdge>,
    pub connected_components_by_net: Vec<(i64, Vec<Vec<usize>>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectivityNode {
    PinNode(usize),
    JunctionNode(usize),
    SegmentEndpointNode(usize, bool),
    CrossingNode(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectivityEdge {
    pub edge_id: usize,
    pub from_node: usize,
    pub to_node: usize,
    pub edge_kind: ConnectivityEdgeKind,
    pub net_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityEdgeKind {
    SegmentEdge,
    PinTouchEdge,
    JunctionTouchEdge,
}

// ============================================================================
// Top-level RenderedConnectivity
// ============================================================================

/// The complete rendered connectivity model extracted from a graph.
#[derive(Debug, Clone)]
pub struct RenderedConnectivity {
    pub pin_anchors: Vec<RenderedPin>,
    pub segments: Vec<RenderedSegment>,
    pub junctions: Vec<RenderedJunction>,
    pub crossings: Vec<RenderedCrossing>,
    pub hops: Vec<RenderedHop>,
    pub graph: RenderedConnectivityGraph,
    pub warnings: Vec<String>,
}

impl RenderedConnectivity {
    /// Extract basic rendered connectivity from a graph (lightweight pass).
    pub fn extract(graph: &McVecGraph) -> Self {
        let mut conn = Self {
            pin_anchors: Vec::new(),
            segments: Vec::new(),
            junctions: Vec::new(),
            crossings: Vec::new(),
            hops: Vec::new(),
            graph: RenderedConnectivityGraph::default(),
            warnings: Vec::new(),
        };

        // Extract pin anchors from entry points
        for b in &graph.boxes {
            for ep in &b.entry_points {
                let (ax, ay) = match ep.side {
                    EntrySide::Top => (b.x + ep.offset * b.w, b.y),
                    EntrySide::Right => (b.x + b.w, b.y + ep.offset * b.h),
                    EntrySide::Bottom => (b.x + ep.offset * b.w, b.y + b.h),
                    EntrySide::Left => (b.x, b.y + ep.offset * b.h),
                };
                // Find the net that this endpoint belongs to
                let net_id = graph
                    .nets
                    .iter()
                    .find(|n| {
                        n.endpoints
                            .iter()
                            .any(|e| e.box_id == b.id && e.pin_id == ep.pin_id)
                    })
                    .map(|n| n.nid)
                    .unwrap_or(-1);

                let pin_name = b
                    .find_pin(ep.pin_id)
                    .map(|p| p.description.clone())
                    .unwrap_or_else(|| format!("P{}", ep.pin_id));

                conn.pin_anchors.push(RenderedPin {
                    endpoint: EndpointRef::new(b.id, ep.pin_id, pin_name.clone()),
                    net_id,
                    box_id: b.id,
                    pin_id: ep.pin_id,
                    pin_name,
                    anchor: Point2D::new(ax, ay),
                    side: ep.side.clone(),
                    reachable_segment_ids: Vec::new(),
                    distance_to_nearest_segment: 0.0,
                });
            }
        }

        // Extract segments from routed nets
        let mut seg_id = 0;
        for net in &graph.nets {
            if let Some(route) = &net.route {
                for seg in &route.segments {
                    let dx = (seg.to.x - seg.from.x).abs();
                    let dy = (seg.to.y - seg.from.y).abs();
                    let orientation = if dx > dy {
                        SegmentOrientation::Horizontal
                    } else if dy > 0.0 {
                        SegmentOrientation::Vertical
                    } else {
                        SegmentOrientation::Degenerate
                    };
                    conn.segments.push(RenderedSegment {
                        segment_id: seg_id,
                        net_id: net.nid,
                        net_name: net.name.clone(),
                        from: Point2D::new(seg.from.x, seg.from.y),
                        to: Point2D::new(seg.to.x, seg.to.y),
                        orientation,
                        source_route_index: 0,
                        source_segment_index: seg_id,
                        is_hop_segment: false,
                    });
                    seg_id += 1;
                }
            }
        }

        conn
    }
}
