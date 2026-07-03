// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_ida::{IdaSegment, McIda};
use super::mc_literal::McInt;
use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::builder::diagnostic::dlog_error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdsSegment {
    Int(Box<McInt>),
    Slice {
        from: Box<McInt>,
        to: Box<McInt>,
    },
    Ida(Box<McIda>),
    DotInt(Box<McInt>),
    DotIda(Box<McIda>),
    Curly(Vec<IdsSegment>),
    /// Square bracket segment, contains multiple members, e.g., [VDD, GND]
    Square(Vec<IdsSegment>),
}

impl IdsSegment {}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct McIds {
    pub segments: Vec<IdsSegment>,
}

impl From<&str> for McIds {
    fn from(s: &str) -> Self {
        Self {
            segments: vec![IdsSegment::Ida(Box::new(McIda::from(s)))],
        }
    }
}

impl McIds {
    pub fn new(node: &AstNode) -> Option<Self> {
        // 1. MCAST_IDS
        //    |- MCAST_ID/MCAST_IDA  - (MCAST_ID/MCAST_IDA/MCAST_OPD_DOT/MCAST_OPD_CURLY)*  - MCAST_INT+
        // where:
        // |- MCAST_OPD_DOT
        //     |- MCAST_ID/MCAST_IDA
        // |- MCAST_OPD_CURLY
        //     |- (MCAST_ID / MCAST_IDA / MCAST_INT / MCAST_OPD_COLON)*
        // 2. MCK_THIS / MCK_PINS
        //    |- MCK_THIS
        //    |- MCK_THIS mc_idm
        //    |- MCK_THIS MCPT_DOT mc_int
        //    |- MCK_THIS mc_idm MCPT_DOT mc_int
        //    |- MCK_PINS mc_idm
        //    |- MCK_PINS MCPT_DOT mc_int

        let mut segments = Vec::new();

        // Handle MCAST_OPD_THIS and MCAST_OPD_PINS cases
        match node.get_type() {
            // Use McIda to handle ID and IDA processing
            // Treat the entire IDA string as one IdsSegment::Ida to maintain consistency with McIds::from
            MCAST_ID | MCAST_IDA => {
                if let Some(ida) = McIda::new(node) {
                    segments.push(IdsSegment::Ida(Box::new(ida)));
                }
            }

            MCAST_PARAM => {
                // MCAST_PARAM is a wrapper, get its sub-node and recurse
                if let Some(sub) = node.get_sub_node() {
                    return McIds::new(&sub);
                }
                return None;
            }

            MCAST_OPD_THIS | MCAST_OPD_PINS => {
                // Add "this" or "pins" as an Ida segment
                let keyword = if node.get_type() == MCAST_OPD_THIS {
                    "this"
                } else {
                    "pins"
                };
                let ida = McIda::from(keyword);
                segments.push(IdsSegment::Ida(Box::new(ida)));

                // Handle subsequent child nodes
                let Some(mut current) = node.get_next() else {
                    // Only "this" or "pins" case
                    return Some(McIds { segments });
                };

                // Handle mc_idm (if exists)
                if current.get_type() != MCAST_OPD_DOT {
                    // Try to parse as McIda
                    if let Some(ida) = McIda::new(&current) {
                        segments.push(IdsSegment::DotIda(Box::new(ida)));

                        // Check if there's more .mc_int
                        if let Some(next) = current.get_next() {
                            current = next;
                        } else {
                            return Some(McIds { segments });
                        }
                    }
                }

                // Handle .mc_int
                if current.get_type() == MCAST_OPD_DOT {
                    if let Some(subnode) = current.get_sub_node() {
                        if subnode.get_type() == MCAST_INT {
                            if let Some(int) = McInt::new(&subnode) {
                                segments.push(IdsSegment::DotInt(Box::new(int)));
                            }
                        }
                    }
                }

                return Some(McIds { segments });
            }
            // Lemon automatically creates MCAST_* nodes for non-terminals
            // mc_opd returns wrapped MCAST_OPD, need to extract sub-node
            MCAST_OPD => {
                if let Some(sub) = node.get_sub_node() {
                    return McIds::new(&sub);
                }
                return None;
            }
            // Handle cases where square bracket vectors appear directly as nodes (not inside MCAST_IDS)
            // Example: [VDD2, GND2] in mc_phrase
            MCAST_OPD_SQUARE_VEC => {
                if let Some(square_seg) = Self::parse_square(node) {
                    segments.push(square_seg);
                }
            }
            MCAST_IDS => {
                // Original logic: handle MCAST_IDS case
                let Some(ids_subnodes) = node.get_sub_node() else {
                    dlog_error(1101, node, "IDS has no nodes.");
                    return None;
                };

                let mut new_segments = Vec::new();
                for each in ids_subnodes.iter() {
                    match each.get_type() {
                        // Use McInt to handle integer processing
                        MCAST_INT => {
                            if let Some(int_value) = McInt::new(&each) {
                                new_segments.push(IdsSegment::Int(Box::new(int_value)));
                            }
                        }
                        // Use McIda to handle ID and IDA processing
                        // Treat the entire IDA string as one IdsSegment::Ida to maintain consistency with MCAST_ID/MCAST_IDA branch
                        MCAST_ID | MCAST_IDA => {
                            if let Some(ida) = McIda::new(&each) {
                                new_segments.push(IdsSegment::Ida(Box::new(ida)));
                            }
                        }

                        MCAST_OPD_DOT => {
                            let Some(subnode) = each.get_sub_node() else {
                                dlog_error(1101, &each, "Missing subnode");
                                continue;
                            };
                            match subnode.get_type() {
                                MCAST_INT => {
                                    if let Some(int) = McInt::new(&subnode) {
                                        new_segments.push(IdsSegment::DotInt(Box::new(int)));
                                    }
                                }
                                MCAST_ID | MCAST_IDA => {
                                    if let Some(ida) = McIda::new(&subnode) {
                                        new_segments.push(IdsSegment::DotIda(Box::new(ida)));
                                    }
                                }
                                _ => {}
                            }
                        }

                        MCAST_OPD_CURLY => {
                            if let Some(curly_seg) = Self::parse_curly(&each) {
                                new_segments.push(curly_seg);
                            }
                        }

                        _ => {}
                    }
                }
                segments = new_segments;
            }
            _ => return None,
        };

        Some(McIds { segments })
    }

    pub fn append(&mut self, node: &AstNode) {
        node.iter().for_each(|each| match each.get_type() {
            MCAST_OPD_DOT => {
                if let Some(subnode) = each.get_sub_node() {
                    match subnode.get_type() {
                        MCAST_ID | MCAST_IDA => {
                            if let Some(ida) = McIda::new(&subnode) {
                                self.segments.push(IdsSegment::DotIda(Box::new(ida)));
                            }
                        }
                        MCAST_INT => {
                            if let Some(int) = McInt::new(&subnode) {
                                self.segments.push(IdsSegment::DotInt(Box::new(int)));
                            }
                        }
                        _ => {}
                    }
                }
            }
            MCAST_OPD_CURLY => {
                if let Some(curly_seg) = Self::parse_curly(node) {
                    self.segments.push(curly_seg);
                }
            }
            MCAST_OPD_SQUARE_VEC => {
                if let Some(square_seg) = Self::parse_square(node) {
                    self.segments.push(square_seg);
                }
            }
            _ => {}
        });
    }

    fn parse_curly(node: &AstNode) -> Option<IdsSegment> {
        let Some(curly_subnodes) = node.get_sub_node() else {
            dlog_error(1101, node, "Missing subnode");
            return None;
        };

        let curly_segs = curly_subnodes
            .iter()
            .filter_map(|each| {
                match each.get_type() {
                    MCAST_INT => McInt::new(&each).map(|int| IdsSegment::Int(Box::new(int))),
                    MCAST_ID | MCAST_IDA => {
                        McIda::new(&each).map(|ida| IdsSegment::Ida(Box::new(ida)))
                    }
                    // Handle MCAST_OPD_COLON (e.g. 1:10)
                    MCAST_OPD_COLON => (|| -> Option<IdsSegment> {
                        let left = each.get_sub_node()?;
                        let right = left.get_next()?;

                        let left_int = McInt::new(&left)
                            .ok_or_else(|| {
                                dlog_error(1102, &left, "Failed to process left side of range");
                            })
                            .ok()?;

                        let right_int = McInt::new(&right)
                            .ok_or_else(|| {
                                dlog_error(1102, &right, "Failed to process right side of range");
                            })
                            .ok()?;

                        Some(IdsSegment::Slice {
                            from: Box::new(left_int),
                            to: Box::new(right_int),
                        })
                    })(),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        Some(IdsSegment::Curly(curly_segs))
    }

    /// Parse square bracket vector, e.g. [VDD, GND]
    fn parse_square(node: &AstNode) -> Option<IdsSegment> {
        let Some(square_subnodes) = node.get_sub_node() else {
            dlog_error(1101, node, "Missing subnode for square vector");
            return None;
        };

        let square_segs = square_subnodes
            .iter()
            .filter_map(|each| {
                // Each element of mc_phrases may be:
                // 1. mc_opd (MCAST_OPD) - subnode is mc_ids
                // 2. mc_literal (MCAST_LITERAL)
                // 3. Other direct nodes
                // If MCAST_OPD, need to get its mc_ids child node
                let ids_node = if each.get_type() == MCAST_OPD {
                    each.get_sub_node().unwrap_or(each.clone())
                } else {
                    each.clone()
                };

                // Try parsing with McIds::new
                if let Some(ids) = McIds::new(&ids_node) {
                    // McIds may have only one segment, take first
                    if let Some(seg) = ids.segments.into_iter().next() {
                        return Some(seg);
                    }
                }

                // Fallback: parse directly
                match ids_node.get_type() {
                    MCAST_INT => McInt::new(&ids_node).map(|int| IdsSegment::Int(Box::new(int))),
                    MCAST_ID | MCAST_IDA => {
                        McIda::new(&ids_node).map(|ida| IdsSegment::Ida(Box::new(ida)))
                    }
                    MCAST_IDS => {
                        // MCAST_IDS like [24,25] should be recursively parsed as nested square
                        Self::parse_square(&ids_node)
                    }
                    MCAST_EXPRESSION => {
                        // Handle expressions like 1:2 inside square brackets [1:2]
                        if let Some(exp_sub) = ids_node.get_sub_node() {
                            if exp_sub.get_type() == MCAST_OPD_COLON {
                                // Extract from and to for Slice
                                let from = exp_sub.get_sub_node()
                                    .and_then(|n| McInt::new(&n));
                                let to = exp_sub.get_sub_node()
                                    .and_then(|n| n.get_next())
                                    .and_then(|n| McInt::new(&n));
                                if let (Some(f), Some(t)) = (from, to) {
                                    return Some(IdsSegment::Slice {
                                        from: Box::new(f),
                                        to: Box::new(t),
                                    });
                                }
                            }
                        }
                        None
                    }
                    MCAST_OPD_COLON => {
                        // Handle colon range like 1:2 directly
                        let left = ids_node.get_sub_node()?;
                        let right = left.get_next()?;
                        if let (Some(from), Some(to)) = (McInt::new(&left), McInt::new(&right)) {
                            return Some(IdsSegment::Slice {
                                from: Box::new(from),
                                to: Box::new(to),
                            });
                        }
                        None
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        Some(IdsSegment::Square(square_segs))
    }

    pub fn len(&self) -> usize {
        self.segments
            .iter()
            .map(|seg| match seg {
                IdsSegment::Int(int) => int.to_string().len(),
                IdsSegment::Ida(ida) => ida.len(),
                IdsSegment::DotInt(int) => int.to_string().len() + 1,
                IdsSegment::DotIda(ida) => ida.to_string().len() + 1,
                IdsSegment::Curly(curly_segs) => {
                    curly_segs
                        .iter()
                        .map(|ids| {
                            // Calculate length for each segment inside curly braces
                            match ids {
                                IdsSegment::Int(int) => int.to_string().len(),
                                IdsSegment::Ida(ida) => ida.len(),
                                IdsSegment::Slice { from, to } => {
                                    // Slice format like "1:10", calculate its string length
                                    format!("{}:{}", from.value, to.value).len()
                                }
                                _ => ids.to_string().len(),
                            }
                        })
                        .sum::<usize>()
                        + 1
                }
                IdsSegment::Square(square_segs) => {
                    square_segs
                        .iter()
                        .map(|ids| ids.to_string().len())
                        .sum::<usize>()
                        + 2
                }
                IdsSegment::Slice { from, to } => format!("{}:{}", from.value, to.value).len(),
            })
            .sum::<usize>()
    }

    /// Get base name (without curly brace part)
    /// Example DC4{VDD, GND} returns "DC4"
    pub fn base_name(&self) -> String {
        let mut result = String::new();
        for seg in &self.segments {
            match seg {
                IdsSegment::Curly(_) | IdsSegment::Square(_) => break,
                IdsSegment::Int(int) => result.push_str(&int.to_string()),
                IdsSegment::Ida(ida) => {
                    // For Ida, only take the original prefix before square brackets, e.g., PWR_[VDD2, GND2] -> PWR_
                    result.push_str(ida.prefix());
                }
                IdsSegment::DotInt(num) => {
                    result.push('.');
                    result.push_str(&num.value.to_string());
                }
                IdsSegment::DotIda(ida) => {
                    result.push('.');
                    result.push_str(&ida.to_string());
                }
                IdsSegment::Slice { from, to } => {
                    result.push_str(&format!("{}:{}", from.value, to.value));
                }
            }
        }
        result
    }

    /// Check if it contains square bracket segment (Square)
    /// DC4{VDD, GND} returns false (only Curly)
    /// PWR_[VDD2, GND2] returns true (contains Square)
    pub fn has_square(&self) -> bool {
        self.segments.iter().any(|seg| match seg {
            IdsSegment::Square(_) => true,
            IdsSegment::Ida(ida) => ida.has_square(),
            IdsSegment::DotIda(ida) => ida.has_square(),
            _ => false,
        })
    }

    /// Only get prefix (don't expand, just take original string before square brackets)
    /// Example DC4{VDD, GND} returns "DC4", PWR_[VDD2, GND2] returns "PWR_"
    pub fn prefix_only(&self) -> String {
        let mut result = String::new();
        for seg in &self.segments {
            match seg {
                IdsSegment::Curly(_) | IdsSegment::Square(_) => break,
                IdsSegment::Int(int) => result.push_str(&int.to_string()),
                IdsSegment::Ida(ida) => {
                    // For Ida, only take the part before square brackets in the original string
                    result.push_str(ida.prefix());
                }
                IdsSegment::DotInt(num) => {
                    result.push('.');
                    result.push_str(&num.value.to_string());
                }
                IdsSegment::DotIda(ida) => {
                    result.push('.');
                    result.push_str(&ida.to_string());
                }
                IdsSegment::Slice { from, to } => {
                    result.push_str(&format!("{}:{}", from.value, to.value));
                }
            }
        }
        result
    }

    /// Check if only has square bracket segment (Square), no other prefix
    /// [VDD1, GND1] returns true
    /// PWR_[VDD2, GND2] returns false (because has prefix PWR_)
    pub fn is_square_only(&self) -> bool {
        self.segments.len() == 1 && matches!(&self.segments[0], IdsSegment::Square(_))
    }

    /// Get the last segment
    pub fn last_segment(&self) -> Option<&IdsSegment> {
        self.segments.last()
    }

    /// Check if the last segment is a curly bracket
    pub fn is_curly_bracket(&self) -> bool {
        matches!(self.last_segment(), Some(IdsSegment::Curly(_)))
    }

    /// Check if the last segment is a square bracket
    pub fn is_square_bracket(&self) -> bool {
        matches!(self.last_segment(), Some(IdsSegment::Square(_)))
    }

    pub fn expand(&self) -> Vec<String> {
        // First expand each segment to get possible string lists for each segment
        let expanded_segments: Vec<Vec<String>> = self
            .segments
            .iter()
            .map(|seg| {
                // Define expansion logic for each segment
                match seg {
                    IdsSegment::Int(int) => vec![int.to_string()],
                    IdsSegment::Ida(ida) => ida.expand(),
                    IdsSegment::DotIda(ida) => {
                        ida.expand().into_iter().map(|s| format!(".{s}")).collect()
                    }
                    IdsSegment::DotInt(num) => {
                        vec![format!(".{}", num.value)]
                    }
                    IdsSegment::Curly(curly_segs) => {
                        // For multiple segments inside curly braces, first expand each segment
                        // Example DC4{VDD, GND} -> DC4.VDD, DC4.GND
                        let mut curly_results: Vec<String> = Vec::new();
                        for curly_seg in curly_segs {
                            // Expand single segment
                            let expanded: Vec<String> = match curly_seg {
                                IdsSegment::Int(int) => vec![int.to_string()],
                                IdsSegment::Ida(ida) => ida.expand(),
                                IdsSegment::Slice { from, to } => {
                                    let start = from.value;
                                    let end = to.value;
                                    (start..=end).map(|i| i.to_string()).collect()
                                }
                                // Other types shouldn't appear in curly braces, or need special handling
                                _ => vec![curly_seg.to_string()],
                            };
                            // Add "." before each expanded item and add to result
                            for item in expanded {
                                curly_results.push(format!(".{item}"));
                            }
                        }

                        curly_results
                    }
                    IdsSegment::Square(square_segs) => {
                        // For Square, recursively expand nested Squares to preserve grouping
                        // while flattening scalar elements
                        let mut all_groups: Vec<Vec<String>> = Vec::new();
                        let mut current_group: Vec<String> = Vec::new();

                        for inner_seg in square_segs {
                            match inner_seg {
                                IdsSegment::Square(inner_square_segs) => {
                                    // Nested Square - recursively expand to get groups
                                    // First save current group if non-empty
                                    if !current_group.is_empty() {
                                        all_groups.push(current_group);
                                        current_group = Vec::new();
                                    }
                                    // Recursively expand nested Square
                                    // We need to handle this specially since we're inside a map
                                    // The nested Square should be treated as a group
                                    let nested: Vec<String> = inner_square_segs
                                        .iter()
                                        .filter_map(|s| match s {
                                            IdsSegment::Ida(ida) => ida.expand().into_iter().next(),
                                            IdsSegment::Int(int) => Some(int.to_string()),
                                            _ => None,
                                        })
                                        .collect();
                                    all_groups.push(nested);
                                }
                                _ => {
                                    // Scalar - expand normally and add to current group
                                    let expanded: Vec<String> = match inner_seg {
                                        IdsSegment::Ida(ida) => ida.expand(),
                                        IdsSegment::Int(int) => vec![int.to_string()],
                                        IdsSegment::Slice { from, to } => {
                                            let start = from.value;
                                            let end = to.value;
                                            (start..=end).map(|i| i.to_string()).collect()
                                        }
                                        _ => vec![inner_seg.to_string()],
                                    };
                                    current_group.extend(expanded);
                                }
                            }
                        }

                        // Handle remaining scalars in current group
                        if !current_group.is_empty() {
                            all_groups.push(current_group);
                        }

                        // If only one group, return it directly (flattened)
                        // Otherwise return all groups preserved
                        if all_groups.len() == 1 {
                            all_groups.into_iter().next().unwrap()
                        } else {
                            all_groups.into_iter().flatten().collect()
                        }
                    }
                    IdsSegment::Slice { from, to } => {
                        // Handle slice, e.g., 1:10
                        let start = from.value;
                        let end = to.value;
                        (start..=end).map(|i| i.to_string()).collect()
                    }
                }
            })
            .collect();

        // Cartesian product of all expanded segments
        let mut results = vec![String::new()];
        for options in expanded_segments {
            let mut new_results = Vec::new();
            for base in results {
                for option in options.iter() {
                    new_results.push(format!("{base}{option}"));
                }
            }
            results = new_results;
        }

        results
    }

    /// Expand with parameter bindings (e.g., R[1:rows]C[1:cols] with rows=2, cols=10 -> R1C1, R1C2, ..., R2C10)
    pub fn expand_with_bindings(&self, bindings: &[(String, i64)]) -> Vec<String> {
        // First substitute parameters for each segment
        let substituted_segments: Vec<IdsSegment> = self
            .segments
            .iter()
            .map(|seg| self.substitute_segment(seg, bindings))
            .collect();

        // Expand using substituted segments
        let expanded_segments: Vec<Vec<String>> = substituted_segments
            .iter()
            .map(|seg| self.expand_single_segment(seg))
            .collect();

        // Cartesian product
        let mut results = vec![String::new()];
        for options in expanded_segments {
            let mut new_results = Vec::new();
            for base in results {
                for option in options.iter() {
                    new_results.push(format!("{base}{option}"));
                }
            }
            results = new_results;
        }

        results
    }

    /// Substitute parameters for a single segment
    fn substitute_segment(&self, seg: &IdsSegment, bindings: &[(String, i64)]) -> IdsSegment {
        match seg {
            IdsSegment::Ida(ida) => {
                if ida.has_param_ref() {
                    IdsSegment::Ida(Box::new(ida.substitute_bindings(bindings)))
                } else {
                    seg.clone()
                }
            }
            _ => seg.clone(),
        }
    }

    /// Expand a single segment
    fn expand_single_segment(&self, seg: &IdsSegment) -> Vec<String> {
        match seg {
            IdsSegment::Int(int) => vec![int.to_string()],
            IdsSegment::Ida(ida) => ida.expand(),
            IdsSegment::DotIda(ida) => ida.expand().into_iter().map(|s| format!(".{s}")).collect(),
            IdsSegment::DotInt(num) => vec![format!(".{}", num.value)],
            IdsSegment::Curly(curly_segs) => {
                let mut curly_results: Vec<String> = Vec::new();
                for curly_seg in curly_segs {
                    let expanded = self.expand_single_segment(curly_seg);
                    for item in expanded {
                        curly_results.push(format!(".{item}"));
                    }
                }
                curly_results
            }
            IdsSegment::Square(square_segs) => {
                let mut all_groups: Vec<Vec<String>> = Vec::new();
                let mut current_group: Vec<String> = Vec::new();

                for inner_seg in square_segs {
                    match inner_seg {
                        IdsSegment::Square(inner_square) => {
                            if !current_group.is_empty() {
                                all_groups.push(current_group.clone());
                                current_group.clear();
                            }
                            let inner_expanded = self
                                .expand_single_segment(&IdsSegment::Square(inner_square.clone()));
                            all_groups.push(inner_expanded);
                        }
                        _ => {
                            let expanded = self.expand_single_segment(inner_seg);
                            current_group.extend(expanded);
                        }
                    }
                }
                if !current_group.is_empty() {
                    all_groups.push(current_group);
                }

                if all_groups.len() == 1 {
                    all_groups.into_iter().next().unwrap()
                } else {
                    all_groups.into_iter().flatten().collect()
                }
            }
            IdsSegment::Slice { from, to } => {
                (from.value..=to.value).map(|i| i.to_string()).collect()
            }
        }
    }

    /// Number of elements after expansion
    pub fn count(&self) -> usize {
        self.expand().len()
    }

    /// Check if contains parameter references
    pub fn has_param_ref(&self) -> bool {
        for seg in &self.segments {
            if let IdsSegment::Ida(ida) = seg {
                if ida.has_param_ref() {
                    return true;
                }
            }
        }
        false
    }

    /// Determine if it's Bus type (IDA{CurlyMembers} form)
    /// Example DC1{VDD, GND} returns true
    /// Note: uC.ADC{P,N} is not Bus, this is component member interface access
    /// Note: Square form (e.g., GPIO[1:2]) is not Bus, it's Multi/List
    pub fn is_bus(&self) -> bool {
        if self.segments.len() >= 2 {
            let last = &self.segments[self.segments.len() - 1];
            // Only Curly {} form counts as Bus
            if let IdsSegment::Curly(_) = last {
                let second_last = &self.segments[self.segments.len() - 2];
                return matches!(second_last, IdsSegment::Ida(_));
            }
            // Square form (e.g., GPIO[1:2] or PDM[CLK, DATA]) is not Bus
        }
        false
    }

    /// Determine if it's Multi/List type (IDA[SquareMembers] form)
    /// Example GPIO[1:2] or PDM[CLK, DATA] returns true
    /// Also supports pure Square form, e.g., [LX, GND]
    pub fn is_list(&self) -> bool {
        if self.segments.len() >= 2 {
            let last = &self.segments[self.segments.len() - 1];
            if let IdsSegment::Square(_) = last {
                let second_last = &self.segments[self.segments.len() - 2];
                return matches!(second_last, IdsSegment::Ida(_));
            }
        }
        // Support pure Square form, e.g. [LX, GND]
        if self.segments.len() == 1 {
            if let IdsSegment::Square(_) = &self.segments[0] {
                return true;
            }
        }
        false
    }

    /// Get Square portion members (only valid when is_list() returns true)
    /// e.g. PDM[CLK, DATA] returns ["CLK", "DATA"]
    /// e.g. GPIO[1:4] returns ["1", "2", "3", "4"]
    pub fn list_members(&self) -> Option<Vec<String>> {
        if !self.is_list() {
            return None;
        }

        // Get Square segment (might be first or last)
        let square_segs = if self.segments.len() == 1 {
            // Pure Square form, e.g. [LX, GND]
            if let IdsSegment::Square(segs) = &self.segments[0] {
                segs
            } else {
                return None;
            }
        } else {
            // IDA[SquareMembers] form, e.g. GPIO[1:2]
            let last = &self.segments[self.segments.len() - 1];
            if let IdsSegment::Square(segs) = last {
                segs
            } else {
                return None;
            }
        };

        let mut result = Vec::new();
        for seg in square_segs {
            match seg {
                IdsSegment::Ida(ida) => result.extend(ida.expand()),
                IdsSegment::Int(int_val) => result.push(int_val.to_string()),
                IdsSegment::Slice { from, to } => {
                    let from_val = from.value;
                    let to_val = to.value;
                    if from_val <= to_val {
                        for i in from_val..=to_val {
                            result.push(i.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        Some(result)
    }

    /// Get the Bus name and members (only valid when is_bus() returns true)
    pub fn as_bus(&self) -> Option<(String, Vec<String>)> {
        if !self.is_bus() || self.segments.len() < 2 {
            return None;
        }
        let second_last = &self.segments[self.segments.len() - 2];
        let last = &self.segments[self.segments.len() - 1];

        let name = match second_last {
            IdsSegment::Ida(ida) => {
                let expanded = ida.expand();
                if expanded.is_empty() {
                    return None;
                }
                expanded[0].clone()
            }
            _ => return None,
        };

        let members = match last {
            IdsSegment::Curly(curly_segs) => {
                let mut result = Vec::new();
                for seg in curly_segs {
                    match seg {
                        IdsSegment::Ida(ida) => result.extend(ida.expand()),
                        IdsSegment::Int(int_val) => result.push(int_val.to_string()),
                        _ => {}
                    }
                }
                result
            }
            IdsSegment::Square(square_segs) => {
                let mut result = Vec::new();
                for seg in square_segs {
                    match seg {
                        IdsSegment::Ida(ida) => result.extend(ida.expand()),
                        IdsSegment::Int(int_val) => result.push(int_val.to_string()),
                        IdsSegment::Slice { from, to } => {
                            // Expand range to individual values
                            let from_val = from.value;
                            let to_val = to.value;
                            if from_val <= to_val {
                                for i in from_val..=to_val {
                                    result.push(i.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                result
            }
            _ => return None,
        };

        Some((name, members))
    }

    /// Detect component member access pattern (COMPONENT.MEMBER{CurlyMembers} form)
    /// e.g. uC.ADC{P,N} returns Some(("uC", "ADC", ["P", "N"]))
    /// This pattern should not create a new instance, but should be treated as a member reference of the component
    pub fn as_component_member(&self) -> Option<(String, String, Vec<String>)> {
        if self.segments.len() >= 3 {
            let last = &self.segments[self.segments.len() - 1];
            let second_last = &self.segments[self.segments.len() - 2];
            let third_last = &self.segments[self.segments.len() - 3];

            if let (
                IdsSegment::Curly(curly_segs),
                IdsSegment::DotIda(dot_ida),
                IdsSegment::Ida(base_ida),
            ) = (last, second_last, third_last)
            {
                let component = base_ida.expand().first()?.clone();
                let member = dot_ida.expand().join(".");

                let members: Vec<String> = curly_segs
                    .iter()
                    .filter_map(|seg| match seg {
                        IdsSegment::Ida(ida) => Some(ida.expand().join(".")),
                        IdsSegment::Int(int_val) => Some(int_val.to_string()),
                        _ => None,
                    })
                    .collect();

                if !members.is_empty() {
                    return Some((component, member, members));
                }
            }
        }
        None
    }

    /// Check if operand matches target name
    pub fn match_name(&self, target: &str) -> bool {
        self.expand().iter().any(|expanded| expanded == target)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get the primary name
    pub fn get_primary_name(&self) -> Option<String> {
        if self.segments.is_empty() {
            None
        } else {
            Some(self.to_string())
        }
    }

    /// Get the member list
    pub fn get_members(&self) -> Vec<&McIds> {
        // McIds does not have the concept of members, return empty list
        vec![]
    }

    /// Get the base name (without the square bracket part)
    /// e.g. GPIO[1:2] returns Some("GPIO")
    /// e.g. DC2.VDD returns None (because of .)
    pub fn get_base_name(&self) -> Option<String> {
        // Only consider single-segment IDA
        if self.segments.len() == 1 {
            match &self.segments[0] {
                IdsSegment::Ida(ida) => {
                    // Check if there is a square bracket segment
                    for seg in &ida.segments {
                        if let IdaSegment::Square(_) = seg {
                            // Has square brackets, find the preceding Id segment
                            for id_seg in &ida.segments {
                                if let IdaSegment::Id(name) = id_seg {
                                    return Some(name.clone());
                                }
                            }
                        }
                    }
                    // No square brackets
                    None
                }
                _ => None,
            }
        } else {
            // Multi-segment may be like DC2.VDD, not handled
            None
        }
    }

    /// Detect if it is a DOT access pattern (e.g. DC2.VDD)
    /// Returns (base_name, member_name) if it is a DOT pattern, otherwise returns None
    pub fn as_dot_access(&self) -> Option<(String, String)> {
        if self.segments.len() == 2 {
            match (&self.segments[0], &self.segments[1]) {
                (IdsSegment::Ida(base), IdsSegment::DotIda(member)) => {
                    let base_name = base.expand().first()?.clone();
                    let member_name = member.expand().first()?.clone();
                    Some((base_name, member_name))
                }
                (IdsSegment::Ida(base), IdsSegment::DotInt(member)) => {
                    let base_name = base.expand().first()?.clone();
                    Some((base_name, member.value.to_string()))
                }
                _ => None,
            }
        } else {
            None
        }
    }
}

impl std::fmt::Display for McIds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let segments_str = self
            .segments
            .iter()
            .map(|seg| seg.to_string())
            .collect::<Vec<_>>()
            .join("");
        write!(f, "{segments_str}")
    }
}

impl Ord for McIds {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_string().cmp(&other.to_string())
    }
}

impl PartialOrd for McIds {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for IdsSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdsSegment::Int(int) => write!(f, "{}", int.value),
            IdsSegment::Ida(ida) => {
                write!(f, "{ida}")
            }
            IdsSegment::DotIda(ida) => {
                write!(f, ".{ida}")
            }
            IdsSegment::DotInt(num) => {
                write!(f, ".{}", num.value)
            }
            IdsSegment::Curly(curly_segs) => {
                write!(f, "{{")?;
                for (i, opdc) in curly_segs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{opdc}")?;
                }
                write!(f, "}}")
            }
            IdsSegment::Slice { from, to } => write!(f, "{}:{}", from.value, to.value),
            IdsSegment::Square(square_segs) => {
                write!(f, "[")?;
                for (i, seg) in square_segs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{seg}")?;
                }
                write!(f, "]")
            }
        }
    }
}
