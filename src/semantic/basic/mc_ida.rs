// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_ids::{expand_char_slice, expand_numeric_slice};
use crate::ast::node::AstNode;
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum IdaSegment {
    /// Regular identifier segment (alphanumeric)
    Id(String),
    /// Square bracket segment, contains multiple items
    Square(Vec<SquareItem>),
    /// Explicitly marked expansion layer, `name[[items]]` (U385 leg G1,
    /// layer-expansion-law.md §3). Same item shape as `Square`; the mark only
    /// changes when the layer expands, and every current consumer is a flat
    /// consumer, so flat expansion treats the two variants identically. The
    /// retained-grouping read rides the expansion leg.
    SquareExpanded(Vec<SquareItem>),
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum SquareItem {
    /// Single identifier or number
    Id(String),
    /// Range expression (start:end)
    Range(String, String),
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct McIda {
    pub segments: Vec<IdaSegment>,
}

impl McIda {
    pub fn new(node: &AstNode) -> Option<Self> {
        // Guarded accessor: the C parser can emit a NULL/small .data that
        // would segfault inside CStr::from_ptr -> strlen.
        let id_str = node.data_as_cstr()?.to_str().ok()?;

        // Directly parse the entire IDA string
        let segments = Self::parse_ida_string(id_str);
        if !segments.is_empty() {
            Some(Self { segments })
        } else {
            None
        }
    }

    /// Parse the entire IDA string
    fn parse_ida_string(s: &str) -> Vec<IdaSegment> {
        let mut segments = Vec::new();
        let mut chars = s.chars().peekable();
        let mut current_id = String::new();

        while let Some(c) = chars.peek() {
            if *c == '[' {
                // Save the current regular identifier segment
                if !current_id.is_empty() {
                    segments.push(IdaSegment::Id(current_id.clone()));
                    current_id.clear();
                }

                // U385 leg G1 (b4500): `[[` opens an explicitly marked
                // expansion layer — consume `[ [ content ] ]` as one segment.
                chars.next(); // Skip the first '['
                let marked = chars.peek() == Some(&'[');
                if marked {
                    chars.next(); // Skip the second '['
                }
                let square_content = Self::parse_until_closing_bracket(&mut chars);
                if marked {
                    // Consume the second closer so it does not leak into the
                    // identifier stream; an unpaired spelling cannot reach
                    // here (the grammar only accepts the doubled form).
                    for c2 in chars.by_ref() {
                        if c2 == ']' {
                            break;
                        }
                    }
                }
                if let Some(items) = Self::parse_square_content(&square_content) {
                    if marked {
                        segments.push(IdaSegment::SquareExpanded(items));
                    } else {
                        segments.push(IdaSegment::Square(items));
                    }
                }
            } else if *c == '\\' {
                // §2.12: Escape character — `\+` → `+`, `\-` → `-`, `\x` → `x`
                chars.next(); // Consume '\'
                if let Some(next) = chars.next() {
                    current_id.push(next);
                }
            } else {
                // Collect regular identifier characters
                current_id.push(chars.next().unwrap());
            }
        }

        // Save the last regular identifier segment
        if !current_id.is_empty() {
            segments.push(IdaSegment::Id(current_id));
        }

        segments
    }

    /// Parse until the matching right bracket is found
    fn parse_until_closing_bracket(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut content = String::new();

        for c in chars.by_ref() {
            match c {
                ']' => {
                    break;
                }
                _ => {
                    content.push(c);
                }
            }
        }

        content
    }

    /// Parse the content within square brackets
    fn parse_square_content(content: &str) -> Option<Vec<SquareItem>> {
        let mut items = Vec::new();

        // Directly use split(',') to split items, then process each part
        for part in content.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                items.push(Self::parse_single_item(trimmed));
            }
        }

        if !items.is_empty() {
            Some(items)
        } else {
            None
        }
    }

    /// Parse a single item (may be a range or single value)
    fn parse_single_item(item_str: &str) -> SquareItem {
        if let Some((start, end)) = item_str.split_once(':') {
            let start_trimmed = start.trim();
            let end_trimmed = end.trim();

            if !start_trimmed.is_empty() && !end_trimmed.is_empty() {
                // Verify whether the range is valid: numeric range or single character range
                let is_valid_range =
                    // Numeric range
                    (start_trimmed.parse::<i64>().is_ok() && end_trimmed.parse::<i64>().is_ok()) ||
                    // Single character range
                    (start_trimmed.len() == 1 && end_trimmed.len() == 1 && start_trimmed.chars().next().unwrap().is_alphabetic() && end_trimmed.chars().next().unwrap().is_alphabetic()) ||
                    // Mixed range with parameter reference (e.g. 1:rows, rows:10)
                    (start_trimmed.parse::<i64>().is_ok() && !end_trimmed.parse::<i64>().is_ok() && !end_trimmed.is_empty()) ||
                    (!start_trimmed.parse::<i64>().is_ok() && end_trimmed.parse::<i64>().is_ok() && !start_trimmed.is_empty()) ||
                    // Both sides are non-numeric identifiers (e.g. rows:cols)
                    (!start_trimmed.parse::<i64>().is_ok() && !end_trimmed.parse::<i64>().is_ok() && !start_trimmed.is_empty() && !end_trimmed.is_empty());

                if is_valid_range {
                    return SquareItem::Range(start_trimmed.to_string(), end_trimmed.to_string());
                }
            }
        }

        // If not a valid range, treat as a single item
        SquareItem::Id(item_str.to_string())
    }

    pub fn to_string(&self) -> String {
        self.segments
            .iter()
            .map(|segment| segment.to_string())
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get the prefix (only the part before the square brackets)
    /// e.g. PWR_[VDD2, GND2] returns "PWR_"
    pub fn prefix(&self) -> &str {
        if let Some(first) = self.segments.first() {
            match first {
                IdaSegment::Id(s) => s,
                IdaSegment::Square(_) | IdaSegment::SquareExpanded(_) => "",
            }
        } else {
            ""
        }
    }

    /// Check if it contains a square bracket segment
    pub fn has_square(&self) -> bool {
        self.segments
            .iter()
            .any(|seg| seg.square_items().is_some())
    }

    /// Check if it contains parameter references (e.g. non-numeric square bracket ranges like rows,
    /// cols)
    /// e.g.: R[1:rows]C[1:cols] contains parameter references rows and cols
    pub fn has_param_ref(&self) -> bool {
        for segment in &self.segments {
            if let Some(items) = segment.square_items() {
                for item in items {
                    if let SquareItem::Range(start, end) = item {
                        // If the range endpoint cannot be parsed as a number, it is considered a
                        // parameter reference
                        if start.parse::<i64>().is_err() || end.parse::<i64>().is_err() {
                            // Further check: single-character letter ranges are allowed
                            let is_letter_range = start.len() == 1
                                && end.len() == 1
                                && start.chars().next().unwrap().is_alphabetic()
                                && end.chars().next().unwrap().is_alphabetic();
                            if !is_letter_range {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Use parameter bindings to replace parameter references in square brackets, generating a new
    /// McIda
    /// e.g.: R[1:rows]C[1:cols] bound with rows=2, cols=10 -> R[1:2]C[1:10]
    pub fn substitute_bindings(&self, bindings: &[(String, i64)]) -> Self {
        let new_segments: Vec<IdaSegment> = self
            .segments
            .iter()
            .map(|seg| match seg {
                IdaSegment::Id(id) => IdaSegment::Id(id.clone()),
                IdaSegment::Square(items) | IdaSegment::SquareExpanded(items) => {
                    let new_items: Vec<SquareItem> = items
                        .iter()
                        .map(|item| match item {
                            SquareItem::Id(id) => SquareItem::Id(id.clone()),
                            SquareItem::Range(start, end) => {
                                // Try to replace parameter references in start and end
                                let new_start = Self::substitute_param(start, bindings);
                                let new_end = Self::substitute_param(end, bindings);
                                SquareItem::Range(new_start, new_end)
                            }
                        })
                        .collect();
                    // The mark survives substitution: it is a property of the
                    // layer, not of the items.
                    if matches!(seg, IdaSegment::SquareExpanded(_)) {
                        IdaSegment::SquareExpanded(new_items)
                    } else {
                        IdaSegment::Square(new_items)
                    }
                }
            })
            .collect();

        McIda {
            segments: new_segments,
        }
    }

    /// Replace parameter reference in a single string
    fn substitute_param(name: &str, bindings: &[(String, i64)]) -> String {
        for (param, value) in bindings {
            if *name == *param {
                return value.to_string();
            }
        }
        name.to_string()
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Expand IDA string, supports numeric ranges and Cartesian product
    /// e.g.:
    /// - id[1] -> ["id1"]
    /// - id[1:3] -> ["id1", "id2", "id3"]
    /// - id[1:3][4:6] -> ["id14", "id15", "id16", "id24", "id25", "id26", "id34", "id35", "id36"]
    ///
    /// Not yet supported:
    /// - letter ranges (e.g. id[a:e])
    /// - special keyword ranges (e.g. id[start:5])
    pub fn expand(&self) -> Vec<String> {
        // Collect all segments that need to be expanded
        let mut expandable_segments: Vec<Vec<String>> = Vec::new();
        let mut base_str = String::new();

        for segment in &self.segments {
            match segment {
                IdaSegment::Id(id) => {
                    // If there are no segments to expand, add directly to the base string
                    if expandable_segments.is_empty() {
                        base_str.push_str(id);
                    } else {
                        // If there are already segments to expand, append the current id to the end
                        // of all existing combinations
                        // this ensures correct order, e.g. id[1]b -> id1b instead of idb1
                        let mut new_segments: Vec<Vec<String>> = Vec::new();
                        for existing in &expandable_segments {
                            for e in existing {
                                new_segments.push(vec![format!("{}{}", e, id)]);
                            }
                        }
                        expandable_segments = new_segments;
                    }
                }
                IdaSegment::Square(items) | IdaSegment::SquareExpanded(items) => {
                    // Expand current square bracket segment. The marked
                    // variant (`[[..]]`) expands identically here — every
                    // current consumer is a flat consumer, so the marked layer
                    // joins the same Cartesian product (layer-expansion-law.md
                    // §3.1 flat-consumption law). The retained-grouping read
                    // lands with the expansion leg's structure consumers.
                    let expanded = self.expand_square_items(items);
                    if expanded.is_empty() {
                        continue;
                    }

                    // If this is the first segment to expand, add directly
                    if expandable_segments.is_empty() {
                        expandable_segments.push(expanded);
                    } else {
                        // Otherwise compute the Cartesian product
                        let mut new_segments: Vec<String> = Vec::new();
                        for existing in &expandable_segments {
                            for e in existing {
                                for item in &expanded {
                                    new_segments.push(format!("{e}{item}"));
                                }
                            }
                        }
                        expandable_segments = vec![new_segments];
                    }
                }
            }
        }

        // If there are no segments to expand, return the base string directly
        if expandable_segments.is_empty() {
            return vec![base_str];
        }

        // Merge all expanded combinations with the base string
        let mut result = Vec::new();
        for segment in expandable_segments {
            for item in segment {
                result.push(format!("{base_str}{item}"));
            }
        }

        result
    }

    /// Expand all items within a single square bracket
    /// e.g. [1:7] -> ["1", "2", "3", "4", "5", "6", "7"]
    /// e.g. [VDD, GND] -> ["VDD", "GND"]
    fn expand_square_items(&self, items: &[SquareItem]) -> Vec<String> {
        let mut expanded: Vec<String> = Vec::new();

        for item in items {
            match item {
                SquareItem::Id(id) => {
                    // Add identifier directly
                    expanded.push(id.clone());
                }
                SquareItem::Range(start, end) => {
                    // First try to convert the range to numbers
                    if let (Ok(start_num), Ok(end_num)) = (start.parse::<i64>(), end.parse::<i64>())
                    {
                        // Generate the numeric sequence in declaration order
                        // (§11.1): 1:4 -> 1..4, 4:1 -> 4..1.
                        for num in expand_numeric_slice(start_num, end_num) {
                            expanded.push(num.to_string());
                        }
                    } else if start.len() == 1 && end.len() == 1 {
                        // Single letter range (e.g. a:e)
                        let start_char = start.chars().next().unwrap();
                        let end_char = end.chars().next().unwrap();

                        // Check if it is a letter
                        if start_char.is_alphabetic() && end_char.is_alphabetic() {
                            // Generate the letter sequence in declaration
                            // order (e.g. f:c -> f, e, d, c).
                            for c in expand_char_slice(start_char, end_char) {
                                expanded.push(c.to_string());
                            }
                        }
                    } else {
                        // Add the range string directly (used for tests)
                        expanded.push(format!("{start}:{end}"));
                    }
                }
            }
        }

        expanded
    }
}

impl IdaSegment {
    /// The item list of a square segment, marked or not. Flat consumers share
    /// one read over both variants (layer-expansion-law.md §2: the mark only
    /// changes when the layer expands, not the pairing law).
    pub fn square_items(&self) -> Option<&[SquareItem]> {
        match self {
            IdaSegment::Square(items) | IdaSegment::SquareExpanded(items) => Some(items),
            IdaSegment::Id(_) => None,
        }
    }
}

impl fmt::Display for IdaSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdaSegment::Id(id) => write!(f, "{id}"),
            IdaSegment::Square(items) | IdaSegment::SquareExpanded(items) => {
                // The marked variant round-trips with its doubled spelling so
                // Display output stays re-parseable as the same IDA.
                if matches!(self, IdaSegment::SquareExpanded(_)) {
                    write!(f, "[")?;
                }
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")?;
                if matches!(self, IdaSegment::SquareExpanded(_)) {
                    write!(f, "]")?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for SquareItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SquareItem::Id(id) => write!(f, "{id}"),
            SquareItem::Range(start, end) => write!(f, "{start}:{end}"),
        }
    }
}

/// Outcome of judging a `{{order}}` spec (U385 engine leg 2,
/// layer-expansion-law.md §4).
pub enum OrderSpecError {
    /// A spec item is not a positive 1-based position (`0`, `x`, `2.5`) — the
    /// offending item text rides along for the diagnostic.
    InvalidPosition(String),
    /// The positions are not a permutation of `1..=n` — repeated, out of
    /// range, or incomplete (the law's initial ruling: a permutation, not a
    /// resampling).
    NotPermutation,
}

/// Expand a `{{order}}` spec to 1-based positions in spelling order: a bare
/// item is one position, a range expands start..=end with the written
/// direction (`4:1` = 4,3,2,1 — the descending spelling is the reverse).
/// Repeats inside the spec are rejected here; the `1..=n` completeness is
/// judged against the member count at the call site
/// ([`judge_order_positions`]).
pub fn order_positions(items: &[SquareItem]) -> Result<Vec<usize>, OrderSpecError> {
    fn one(text: &str) -> Result<usize, OrderSpecError> {
        // A position is a plain positive decimal integer.
        match text.parse::<usize>() {
            Ok(p) if p >= 1 => Ok(p),
            _ => Err(OrderSpecError::InvalidPosition(text.to_string())),
        }
    }
    let mut out = Vec::new();
    for item in items {
        match item {
            SquareItem::Id(p) => out.push(one(p)?),
            SquareItem::Range(a, b) => {
                let (start, end) = (one(a)?, one(b)?);
                if start <= end {
                    out.extend(start..=end);
                } else {
                    out.extend((end..=start).rev());
                }
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for p in &out {
        if !seen.insert(*p) {
            return Err(OrderSpecError::NotPermutation);
        }
    }
    Ok(out)
}

/// Judge the expanded spec against a member-sequence length: a permutation
/// must hit every position exactly once.
pub fn judge_order_positions(positions: &[usize], n: usize) -> Result<(), OrderSpecError> {
    if positions.len() == n && positions.iter().all(|p| (1..=n).contains(p)) {
        Ok(())
    } else {
        Err(OrderSpecError::NotPermutation)
    }
}

impl fmt::Display for McIda {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl fmt::Debug for IdaSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Debug for SquareItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Debug for McIda {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl From<&str> for McIda {
    fn from(value: &str) -> Self {
        let segments = Self::parse_ida_string(value);
        Self { segments }
    }
}
