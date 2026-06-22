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

use super::vec::McVec;

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
}

impl McVecNet {
    /// Create a new net
    pub fn new(nid: i64, name: String, nets: Vec<McVec>) -> Self {
        Self { nid, name, nets }
    }

    /// Determine the connection topology type
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
        write!(f, "  [{}]", self.connection_type())
    }
}
