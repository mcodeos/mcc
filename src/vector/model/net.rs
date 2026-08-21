// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`McVecNet`] —— an electrical net
//! [`ConnectionType`] —— connection topology type (used by the drawing side to choose different rendering strategies)
//!
//! `McVec`s within the same `McVecNet` are connected positionally:
//! - The i-th ID in `nets[0]` is connected to the i-th ID in `nets[1]`
//! - If one side has only 1 element, it broadcasts to all elements on the other side

use std::fmt;

use super::dock::DockRef;
use super::vec::McVec;

// ============================================================================
// ★ P7-3: RailSpec — declaration-driven power net spec (classification + driver)
// ============================================================================

/// Power net class (from the port declaration's member_info.role, not name matching)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailClass {
    /// Ground (member role == Ground, globally one and the same, R-1 has no driver)
    Ground,
    /// Power rail (member role == Power)
    Power,
}

/// The spec of one power net, resolved by the projection layer (viz/project.rs)
/// from port declarations:
/// * `class` —— Ground / Power
/// * `driver_pin` —— the producing-side endpoint's pin id (InstTable entry id); `None` = no driver (R-1)
/// * `volt` —— the `::DC(5V)` literal
#[derive(Debug, Clone)]
pub struct RailSpec {
    pub class: RailClass,
    pub driver_pin: Option<i64>,
    pub volt: Option<String>,
}

// ============================================================================
// ConnectionType
// ============================================================================

/// Connection topology type, used by the drawing side to choose different rendering strategies
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionType {
    /// All endpoints 1:1 chained (each McVec is a single point, >=3 in total)
    Chain,
    /// Two groups 1:1 correspondence (each McVec has 1 element)
    OneToOne,
    /// n:n correspondence connection (bus type, two groups equal length)
    NtoN(usize),
    /// 1:n broadcast connection (power distribution type)
    Broadcast(usize),
    /// Multiple-group mixed topology
    Complex,
    /// Isolated point (less than 2 McVecs)
    Isolated,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionType::Chain => write!(f, "chain"),
            ConnectionType::OneToOne => write!(f, "1:1"),
            ConnectionType::NtoN(n) => write!(f, "{n}:{n}"),
            ConnectionType::Broadcast(n) => write!(f, "1:{n}"),
            ConnectionType::Complex => write!(f, "complex"),
            ConnectionType::Isolated => write!(f, "isolated"),
        }
    }
}

// ============================================================================
// McVecNet
// ============================================================================

/// An electrical net, containing multiple groups of shape-aligned endpoint vectors
#[derive(Debug, Clone)]
pub struct McVecNet {
    /// Unique net ID
    pub nid: i64,
    /// Net name (VCC / GND / __net_N)
    pub name: String,
    /// Shape-aligned endpoint groups
    pub nets: Vec<McVec>,
    /// ★ Shape as written in the source. `None` = no provenance; downstream falls back to `connection_type()`.
    /// Never fill with heuristics —— the coverage is reported in the logs.
    pub shape: Option<super::netshape::NetShape>,
    /// ★ P7-3: power net spec (None = ordinary signal net). Filled by viz/project.rs from port declarations.
    pub rail: Option<RailSpec>,
    /// ★ P7-8: boundary terminal marker. When a net has a pseudo endpoint that is the
    /// module's own port declaration (not a rail), project.rs creates a BoundaryInfo
    /// instead of removing the endpoint. fromblock.rs reads it to create a PortTerminal box.
    pub boundary: Option<BoundaryInfo>,
    /// ★ P9-A2: source span for bidirectional traceability.
    /// `(file, line)` — which source file and line created this net.
    pub source_span: Option<crate::semantic::common::SourcePos>,
    /// ★ §8.9.6: structured group context that produced this net.
    /// `None` is a legal value; do not fill with heuristics.
    pub port_group: Option<crate::vector::model::dock::PortGroupCtx>,
    /// ★ §8.9.4: back-reference to the coarse `PortDock` this fine net belongs
    /// to (dock id + lane index). `None` = free net not covered by any dock.
    pub dock: Option<DockRef>,
}

/// ★ P7-8: boundary terminal info for port-level (not member-level) terminals.
///
/// Project.rs collects pseudo endpoints that are the module's own port declarations,
/// walks up to the nearest port-group ancestor, and writes a BoundaryInfo per net.
/// fromblock.rs reads it to create a single PortTerminal box per port group.
#[derive(Debug, Clone)]
pub struct BoundaryInfo {
    /// The port group id (e.g., I2C0 for SCL/SDA; DAC_OUT for DAC_OUT itself).
    pub port_group_id: i64,
    /// The port group name (e.g., "I2C0", "DAC_OUT").
    pub port_name: String,
    /// IO direction of the port.
    pub io: crate::vector::graph::netdef::IoDirection,
}

impl McVecNet {
    /// Create a new net (no provenance)
    pub fn new(nid: i64, name: String, nets: Vec<McVec>) -> Self {
        Self {
            nid,
            name,
            nets,
            shape: None,
            rail: None,
            boundary: None,
            source_span: None,
            port_group: None,
            dock: None,
        }
    }

    /// Create a new net with provenance
    pub fn with_shape(
        nid: i64,
        name: String,
        nets: Vec<McVec>,
        shape: super::netshape::NetShape,
    ) -> Self {
        // A fully empty shape is stored as None; don't create an intermediate state of "has a shape but no information"
        let shape = if shape.is_informative() {
            Some(shape)
        } else {
            None
        };
        Self {
            nid,
            name,
            nets,
            shape,
            rail: None,
            boundary: None,
            source_span: None,
            port_group: None,
            dock: None,
        }
    }

    /// Determine the connection topology type
    #[deprecated(
        since = "P3.2",
        note = "Use shape_type_key() based on NetShape instead"
    )]
    pub fn connection_type(&self) -> ConnectionType {
        if self.nets.len() < 2 {
            return ConnectionType::Isolated;
        }

        let shapes: Vec<usize> = self.nets.iter().map(|v| v.len()).collect();

        // All McVecs are single points
        if shapes.iter().all(|&s| s == 1) {
            if shapes.len() == 2 {
                return ConnectionType::OneToOne;
            } else {
                return ConnectionType::Chain;
            }
        }

        // Exactly two groups
        if shapes.len() == 2 {
            if shapes[0] == shapes[1] {
                return ConnectionType::NtoN(shapes[0]);
            }
            if shapes[0] == 1 || shapes[1] == 1 {
                let n = shapes[0].max(shapes[1]);
                return ConnectionType::Broadcast(n);
            }
        }

        ConnectionType::Complex
    }

    /// NetShape-based topology classification key (replaces `connection_type()`).
    /// Returns a short string: "1:1", "n:n", "broadcast", "chain", "complex", "isolated".
    pub fn shape_type_key(&self) -> &'static str {
        if let Some(shape) = &self.shape {
            let n = shape.groups.len();
            if n < 2 {
                return "isolated";
            }
            let counts: Vec<usize> = shape
                .groups
                .iter()
                .map(|g| match g {
                    super::netshape::GroupRole::Scalar => 1,
                    super::netshape::GroupRole::Broadcast(k) => *k,
                })
                .collect();
            if n == 2 {
                if counts[0] == 1 && counts[1] == 1 {
                    return "1:1";
                }
                if counts[0] == counts[1] && counts[0] > 1 {
                    return "n:n";
                }
                return "broadcast";
            }
            if counts.iter().all(|&c| c == 1) {
                return "chain";
            }
            return "complex";
        }
        // No shape provenance → fallback to legacy connection_type()
        #[allow(deprecated)]
        match self.connection_type() {
            ConnectionType::OneToOne => "1:1",
            ConnectionType::Broadcast(_) => "broadcast",
            ConnectionType::NtoN(_) => "n:n",
            ConnectionType::Chain => "chain",
            ConnectionType::Complex => "complex",
            ConnectionType::Isolated => "isolated",
        }
    }

    /// NetShape-based topology classification for Display (replaces `connection_type()`).
    pub fn shape_type_name(&self) -> String {
        let key = self.shape_type_key();
        if key == "chain" {
            "Chain".to_string()
        } else if key == "isolated" {
            "Isolated".to_string()
        } else if key == "complex" {
            "Complex".to_string()
        } else {
            key.to_string()
        }
    }

    /// All endpoint IDs involved in this net (deduplicated, order preserved)
    pub fn all_point_ids(&self) -> Vec<i64> {
        let mut ids = Vec::new();
        for vec in &self.nets {
            for &id in vec.ids() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }

    /// Total number of endpoints involved in this net (not deduplicated)
    pub fn total_points(&self) -> usize {
        self.nets.iter().map(|v| v.len()).sum()
    }
}

impl fmt::Display for McVecNet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "net({}, \"{}\"): ", self.nid, self.name)?;
        for (i, vec) in self.nets.iter().enumerate() {
            if i > 0 {
                write!(f, " <-> ")?;
            }
            write!(f, "{vec}")?;
        }
        write!(f, "  [{}]", self.shape_type_name())
    }
}
