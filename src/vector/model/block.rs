// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`McVecBlock`] —— Vectorized block
//!
//! Layout of `McVecBlock` as unit:
//!
//! - `insts`   Direct instances (components + self) in this block
//! - `nets`    All electrical nets in this block
//! - `blocks`  Sub-blocks (recursively)

use std::fmt;

use super::net::McVecNet;

/// Vectorized block
#[derive(Debug, Clone)]
pub struct McVecBlock {
    /// Global ID of this block in InstTable
    pub bid: i64,
    /// Name of this block (instance name)
    pub name: String,
    /// Sub-blocks (recursively)
    pub blocks: Vec<McVecBlock>,
    /// Direct instances (components + self) in this block
    pub insts: Vec<i64>,
    /// All electrical nets in this block
    pub nets: Vec<McVecNet>,
}

impl McVecBlock {
    /// Create a new empty block
    pub fn new(bid: i64, name: String) -> Self {
        Self {
            bid,
            name,
            blocks: vec![],
            insts: vec![],
            nets: vec![],
        }
    }

    /// Total number of nets in this block
    pub fn net_count(&self) -> usize {
        self.nets.len()
    }

    /// Total number of instances in this block
    pub fn inst_count(&self) -> usize {
        self.insts.len()
    }

    /// Recursively count total number of blocks (including self)
    pub fn total_blocks(&self) -> usize {
        1 + self.blocks.iter().map(|b| b.total_blocks()).sum::<usize>()
    }

    /// Recursively count total number of nets in this block
    pub fn total_nets(&self) -> usize {
        self.nets.len() + self.blocks.iter().map(|b| b.total_nets()).sum::<usize>()
    }

    /// Recursively count total number of instances (including self)
    pub fn total_insts(&self) -> usize {
        self.insts.len() + self.blocks.iter().map(|b| b.total_insts()).sum::<usize>()
    }

    /// Formatted output with indentation (internal recursive)
    fn fmt_with_indent(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);

        writeln!(
            f,
            "{}Block(bid={}, name=\"{}\")",
            indent, self.bid, self.name
        )?;

        if !self.insts.is_empty() {
            writeln!(f, "{}  insts: {:?}", indent, self.insts)?;
        }

        for net in &self.nets {
            writeln!(f, "{indent}  {net}")?;
        }

        for sub in &self.blocks {
            sub.fmt_with_indent(f, depth + 1)?;
        }

        Ok(())
    }
}

impl fmt::Display for McVecBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}
