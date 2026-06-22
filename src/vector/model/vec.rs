// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`McVec`] —— a group of shape-aligned endpoint IDs
//!
//! Internally stores global IDs from `InstTable` (pins / ports / labels).
//! Multiple `McVec`s within the same `McVecNet` are connected positionally one-to-one.

use std::fmt;

/// A group of shape-aligned endpoint IDs
#[derive(Debug, Clone)]
pub struct McVec(Vec<i64>);

impl McVec {
    /// Create from an ID list
    pub fn new(ids: Vec<i64>) -> Self {
        Self(ids)
    }

    /// Single-point McVec
    pub fn single(id: i64) -> Self {
        Self(vec![id])
    }

    /// Get the internal ID slice
    pub fn ids(&self) -> &[i64] {
        &self.0
    }

    /// Number of elements
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the i-th ID
    pub fn get(&self, i: usize) -> Option<i64> {
        self.0.get(i).copied()
    }

    /// Iterate all IDs
    pub fn iter(&self) -> impl Iterator<Item = &i64> {
        self.0.iter()
    }
}

impl fmt::Display for McVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, id) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{id}")?;
        }
        write!(f, "]")
    }
}
