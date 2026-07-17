// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M11 — Idiom-aware placement model
//!
//! Data types that bridge idiom detection (read-only) with placement (write).

use crate::vector::graph::EntrySide;

// ============================================================================
// IdiomPlacementModel
// ============================================================================

/// Top-level model: idiom instances + derived placement constraints.
#[derive(Debug, Clone, Default)]
pub struct IdiomPlacementModel {
    /// Recognized idiom instances with placement-relevant detail.
    pub instances: Vec<IdiomInstance>,
    /// Constraints derived from instances.
    pub constraints: Vec<PlacementConstraint>,
    /// Box IDs that must not be moved by idiom placement.
    pub protected_box_ids: Vec<i64>,
    /// Warnings about idioms that could not be safely applied.
    pub warnings: Vec<String>,
}

// ============================================================================
// IdiomInstance — one recognized idiom, placement-ready
// ============================================================================

/// A single recognized idiom instance with enough detail to drive placement.
#[derive(Debug, Clone, PartialEq)]
pub struct IdiomInstance {
    pub kind: IdiomInstanceKind,
    /// The box that idiom placement should NOT move (e.g. IC, connector).
    pub anchor_box_id: i64,
    /// Boxes that idiom placement IS allowed to move (e.g. cap, resistor).
    pub satellite_box_ids: Vec<i64>,
    /// The specific pin on the anchor that this idiom relates to.
    pub anchor_pin_id: Option<i64>,
    /// The signal net involved (for pullup/pulldown).
    pub signal_net_id: Option<i64>,
    /// The power net involved.
    pub power_net_id: Option<i64>,
    /// The ground net involved.
    pub ground_net_id: Option<i64>,
    /// Confidence [0.0, 1.0] that this is a genuine instance.
    pub confidence: f64,
    /// How this instance was detected.
    pub source: InstanceSource,
}

/// Idiom categories for placement (distinct from read-only IdiomKind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IdiomInstanceKind {
    /// Decoupling capacitor: capacitor between Power and Ground.
    Decoupling,
    /// Pullup resistor: resistor between Signal and Power.
    Pullup,
    /// Pulldown resistor: resistor between Signal and Ground.
    Pulldown,
    /// Differential pair: P/N signal pair.
    DiffPair,
}

/// How an idiom instance was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceSource {
    /// Net-semantic based (power/ground/signal net kind).
    NetSemantic,
    /// Net-name heuristic (e.g. _P/_N suffix).
    NetNameHeuristic,
    /// Topology pattern match.
    TopologyPattern,
}

// ============================================================================
// PlacementConstraint — what to do, not where to put it
// ============================================================================

/// A soft placement intent derived from an idiom.
///
/// These are *proposals*, not mandates. The apply phase may reject them if
/// they would cause collisions or violate protected geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementConstraint {
    pub kind: ConstraintKind,
    /// Box to move.
    pub target_box_id: i64,
    /// Reference box (anchor).
    pub anchor_box_id: i64,
    /// Preferred side relative to anchor.
    pub preferred_side: Option<AnchorSide>,
    /// Preferred axis alignment with anchor.
    pub align_axis: Option<AlignAxis>,
    /// Preferred distance range from anchor center (min, max).
    pub distance_range: Option<(f64, f64)>,
    /// Priority (lower = more important).
    pub priority: u8,
    /// Whether this is a hard constraint (must be satisfied).
    pub hard: bool,
}

/// Side relative to the anchor box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSide {
    Above,
    Below,
    Left,
    Right,
}

impl AnchorSide {
    pub fn to_entry_side(self) -> EntrySide {
        match self {
            AnchorSide::Above => EntrySide::Top,
            AnchorSide::Below => EntrySide::Bottom,
            AnchorSide::Left => EntrySide::Left,
            AnchorSide::Right => EntrySide::Right,
        }
    }
}

/// Axis alignment between target and anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignAxis {
    /// Align X centers.
    Vertical,
    /// Align Y centers.
    Horizontal,
}

/// Category of placement constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintKind {
    /// Place satellite near anchor (decoupling, pullup, pulldown).
    NearAnchor,
    /// Align satellite with anchor on an axis.
    AlignWithAnchor,
    /// Adjust pin side intent.
    PinSideIntent,
    /// Place symmetrically (diff pair).
    SymmetricPlacement,
}
