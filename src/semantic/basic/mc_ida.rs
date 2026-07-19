// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum IdaSegment {
    /// Regular identifier segment (alphanumeric)
    Id(String),
    /// Square bracket segment, contains multiple items
    Square(Vec<SquareItem>),
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
        let id_str = unsafe {
            std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char)
                .to_str()
                .expect("Bad encoding")
        };

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

                // Parse the square bracket segment
                chars.next(); // Skip '['
                let square_content = Self::parse_until_closing_bracket(&mut chars);
                if let Some(items) = Self::parse_square_content(&square_content) {
                    segments.push(IdaSegment::Square(items));
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
                IdaSegment::Square(_) => "",
            }
        } else {
            ""
        }
    }

    /// Check if it contains a square bracket segment
    pub fn has_square(&self) -> bool {
        self.segments
            .iter()
            .any(|seg| matches!(seg, IdaSegment::Square(_)))
    }

    /// Check if it contains parameter references (e.g. non-numeric square bracket ranges like rows, cols)
    /// e.g.: R[1:rows]C[1:cols] contains parameter references rows and cols
    pub fn has_param_ref(&self) -> bool {
        for segment in &self.segments {
            if let IdaSegment::Square(items) = segment {
                for item in items {
                    if let SquareItem::Range(start, end) = item {
                        // If the range endpoint cannot be parsed as a number, it is considered a parameter reference
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

    /// Use parameter bindings to replace parameter references in square brackets, generating a new McIda
    /// e.g.: R[1:rows]C[1:cols] bound with rows=2, cols=10 -> R[1:2]C[1:10]
    pub fn substitute_bindings(&self, bindings: &[(String, i64)]) -> Self {
        let new_segments: Vec<IdaSegment> = self
            .segments
            .iter()
            .map(|seg| match seg {
                IdaSegment::Id(id) => IdaSegment::Id(id.clone()),
                IdaSegment::Square(items) => {
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
                    IdaSegment::Square(new_items)
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
                        // If there are already segments to expand, append the current id to the end of all existing combinations
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
                IdaSegment::Square(items) => {
                    // Expand current square bracket segment
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
                        // Generate the numeric sequence
                        for num in start_num..=end_num {
                            expanded.push(num.to_string());
                        }
                    } else if start.len() == 1 && end.len() == 1 {
                        // Single letter range (e.g. a:e)
                        let start_char = start.chars().next().unwrap();
                        let end_char = end.chars().next().unwrap();

                        // Check if it is a letter
                        if start_char.is_alphabetic() && end_char.is_alphabetic() {
                            // Generate the letter sequence
                            for c in start_char..=end_char {
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

impl fmt::Display for IdaSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdaSegment::Id(id) => write!(f, "{id}"),
            IdaSegment::Square(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
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
