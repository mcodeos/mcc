// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Network model (new and legacy coexisting)
//!
//! ## ★ NEW -- `VizNet` (multi-endpoint hyperedge)
//! This is the core of the new design: one electrical net = a set of endpoints + a path.
//! Replaces the legacy [`McVecEdge`] binary (src <-> dst) model.
//!
//! ## Compatibility -- `McVecEdge`
//! The legacy binary model is kept because `viz/render` is still using it.
//! After P3/P4 phase switches the router to `VizNet`, `McVecEdge` can be deprecated.
//!
//! ## Typical usage
//! ```ignore
//! // A VCC net connects 5 endpoints (legacy model: 10 pairwise edges)
//! let vcc = VizNet {
//!     nid: 1,
//!     name: "VCC".into(),
//!     kind: NetKind::Power,
//!     endpoints: vec![
//!         EndpointRef { box_id: 1, pin_id: 11, pin_name: "1".into() },
//!         EndpointRef { box_id: 2, pin_id: 21, pin_name: "VCC".into() },
//!         // ... 5 in total
//!     ],
//!     route: None,    // filled in by router
//! };
//! ```

use super::boxdef::Wire;
use super::kinds::{EdgeType, NetKind};

// ============================================================================
// ★ NEW P03: NetTopology -- topology shape (replacing legacy EdgeType's topology dimension)
// ============================================================================

/// A net's **topology shape** (endpoint count + driver direction), computed by `VizNet::topology()`
///
/// Note: this is orthogonal to [`NetKind`]:
/// - `NetKind` is the **semantic role** (Power / Signal / Bus / ...)
/// - `NetTopology` is the **geometric shape** (TwoPoint / Star / MultiDriver / ...)
///
/// The router dispatch in P11 will look at both (NetKind, NetTopology) to select a router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetTopology {
    /// Endpoint count <= 1, no line to draw
    Isolated,
    /// Two-point (source-sink or two-pin component)
    TwoPoint,
    /// 3+ endpoints, single driver (1 Output to multiple Inputs)
    StarOneDriver,
    /// 3+ endpoints, multi-driver (open-drain / bus, or DRC error)
    MultiDriver,
}

// ============================================================================
// ★ NEW P03: IoDirection -- endpoint electrical direction
// ============================================================================

/// Simplified IOType (only keeping semantics relevant for drawing)
///
/// Currently (P03) EndpointRef doesn't require this field, default value `Unknown` is fine.
/// P01 will let `fromblock::generate_viznets_from_block` translate
/// `InstTable.IOType` into this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IoDirection {
    Input,
    Output,
    Bidir,
    Power,
    Ground,
    /// No direction (resistor / capacitor)
    Passive,
    /// Unknown (default value before P01)
    #[default]
    Unknown,
}

// ============================================================================
// ★ NEW: VizNet (hyperedge)
// ============================================================================

/// An electrical net (multi-endpoint hyperedge)
///
/// Replaces the legacy [`McVecEdge`] binary model, accurately expressing multi-endpoint
/// topologies like "a VCC net connects 5 chips".
#[derive(Debug, Clone)]
pub struct VizNet {
    /// Unique ID of the net
    pub nid: i64,
    /// Net name (`VCC` / `GND` / `__net_N`)
    pub name: String,
    /// Net semantics (for router strategy selection)
    pub kind: NetKind,
    /// All endpoints (no limit on count, can be 1 / 2 / 3 / N)
    pub endpoints: Vec<EndpointRef>,
    /// Route result (filled by router, `None` when not routed)
    pub route: Option<Route>,
    /// Source position in the AST (for diagnostic source-line reporting)
    pub src_pos: Option<i32>,
}

impl VizNet {
    /// Create a new unrouted net
    pub fn new(nid: i64, name: String, kind: NetKind, endpoints: Vec<EndpointRef>) -> Self {
        Self {
            nid,
            name,
            kind,
            endpoints,
            route: None,
            src_pos: None,
        }
    }

    /// Endpoint count
    pub fn endpoint_count(&self) -> usize {
        self.endpoints.len()
    }

    /// All box_ids involved in this net (deduplicated)
    pub fn box_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = Vec::new();
        for e in &self.endpoints {
            if !ids.contains(&e.box_id) {
                ids.push(e.box_id);
            }
        }
        ids
    }

    /// Whether this net spans multiple boxes (>= 2)
    pub fn is_inter_box(&self) -> bool {
        self.box_ids().len() >= 2
    }

    /// Whether this net is entirely within a single box (internal connection, usually doesn't need drawing)
    pub fn is_intra_box(&self) -> bool {
        self.box_ids().len() <= 1
    }

    /// ★ NEW P03: this net's topology shape
    ///
    /// Algorithm:
    /// - endpoint count <= 1 -> `Isolated`
    /// - endpoint count = 2 -> `TwoPoint`
    /// - endpoint count >= 3 and driver count <= 1 -> `StarOneDriver`
    /// - endpoint count >= 3 and driver count >= 2 -> `MultiDriver`
    ///
    /// "Driver count" = number of endpoints whose `io_type` is in {Output, Bidir}.
    /// Before P01, io_type defaults to `Unknown`, in which case driver count = 0, will be
    /// classified as StarOneDriver.
    pub fn topology(&self) -> NetTopology {
        let n = self.endpoints.len();
        if n <= 1 {
            return NetTopology::Isolated;
        }
        if n == 2 {
            return NetTopology::TwoPoint;
        }
        let drivers = self
            .endpoints
            .iter()
            .filter(|e| matches!(e.io_type, IoDirection::Output | IoDirection::Bidir))
            .count();
        if drivers >= 2 {
            NetTopology::MultiDriver
        } else {
            NetTopology::StarOneDriver
        }
    }

    /// ★ NEW P03: whether this net is synthesized by rail-synth
    ///
    /// Criteria: any endpoint has `pin_id < 0` (synthesized endpoint has no real pin)
    pub fn is_synthetic(&self) -> bool {
        self.endpoints.iter().any(|e| e.pin_id < 0)
    }
}

// ============================================================================
// ★ NEW: EndpointRef (a reference to an endpoint)
// ============================================================================

/// A net endpoint
///
/// Doesn't embed pin details, only IDs; details are reverse-looked-up via `InstTable` /
/// `McVecBox.entry_points`.
///
/// **P03 (S1)**: added `io_type` field, default `Unknown`, for P01 to fill in real direction.
/// **P01 (S2)**: added `pin_number` field (physical pin number), used for IC marking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointRef {
    /// ID of the box this endpoint belongs to (corresponds to [`super::boxdef::McVecBox::id`])
    pub box_id: i64,
    /// ID of the endpoint itself (global ID of the pin / port in `InstTable`)
    ///
    /// Special value: `-1` means **synthesized endpoint** (virtual endpoint produced by
    /// rail-synth, has no real pin)
    pub pin_id: i64,
    /// Endpoint name (used for labeling on the graph; router/render also use it)
    pub pin_name: String,
    /// ★ P03: electrical direction (Unknown default before P01, filled from InstTable.IOType after P01)
    pub io_type: IoDirection,
    /// ★ P01: physical pin number (1, 2, ..., used for IC marking), None if not available
    pub pin_number: Option<u32>,
}

impl EndpointRef {
    /// Simplest construction (io_type = Unknown, pin_number = None)
    ///
    /// Used by legacy code + rail-synth synthesized endpoints.
    pub fn new(box_id: i64, pin_id: i64, pin_name: impl Into<String>) -> Self {
        Self {
            box_id,
            pin_id,
            pin_name: pin_name.into(),
            io_type: IoDirection::Unknown,
            pin_number: None,
        }
    }

    /// Construction with io_type (added in P03, pin_number = None)
    pub fn with_io(
        box_id: i64,
        pin_id: i64,
        pin_name: impl Into<String>,
        io_type: IoDirection,
    ) -> Self {
        Self {
            box_id,
            pin_id,
            pin_name: pin_name.into(),
            io_type,
            pin_number: None,
        }
    }

    /// ★ P01: full construction (P01 uses this in from_block.rs to fill real direction + pin number)
    pub fn full(
        box_id: i64,
        pin_id: i64,
        pin_name: impl Into<String>,
        io_type: IoDirection,
        pin_number: Option<u32>,
    ) -> Self {
        Self {
            box_id,
            pin_id,
            pin_name: pin_name.into(),
            io_type,
            pin_number,
        }
    }

    /// Whether this is a rail-synth synthesized endpoint
    pub fn is_synthetic(&self) -> bool {
        self.pin_id < 0
    }
}

// ============================================================================
// ★ NEW: Route (route result, filled by router)
// ============================================================================

/// The concrete geometric path computed by the router
///
/// A multi-endpoint net may have multiple polyline segments + T-shaped junctions.
/// Filled by implementations of [`crate::viz::traits::Router`].
#[derive(Debug, Clone)]
pub struct Route {
    /// Collection of polyline segments
    pub segments: Vec<Segment>,
    /// T-shaped junctions (where 3+ wires meet, render will draw a small dot)
    pub junctions: Vec<Point>,
}

impl Route {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            junctions: Vec::new(),
        }
    }
}

impl Default for Route {
    fn default() -> Self {
        Self::new()
    }
}

/// Polyline segment (start -> end)
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    pub from: Point,
    pub to: Point,
}

/// A point on the plane
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

// ============================================================================
// Compatibility: McVecEdge (legacy binary model)
// ============================================================================

/// Legacy binary edge model (compatible with existing layout / render code)
///
/// **deprecated (after P03)** --
/// - The main pipeline (`from_block.rs`) no longer generates `McVecEdge`
/// - `wire.rs::render_edge` has been removed
/// - `components.rs::build_adjacency` / `entry_points.rs` have switched to reading `graph.nets`
///
/// Kept only for compatibility:
/// - `from_table.rs` (legacy builder, independent path, not yet migrated)
/// - Any legacy code still directly reading `graph.edges` (none in production)
///
/// Full deprecation requires first migrating `from_table.rs`, which is for a later sprint.
#[derive(Debug, Clone)]
pub struct McVecEdge {
    pub src_box: i64,
    pub dst_box: i64,
    pub edge_type: EdgeType,
    pub wires: Vec<Wire>,
    pub net_name: String,
}
