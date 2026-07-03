// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::core::basic::mc_uval::McUnitValueDeclare;
use crate::McIds;
use crate::{ast::ast_node::AstNode, ast::c_macros::*, builder::diagnostic::dlog_error};
use std::collections::HashMap;
use std::ops::Range;

/// Parameter declaration list
#[derive(Debug, Clone, Default)]
pub struct McParamDeclares {
    declares: Vec<McParamDeclare>,
    /// Port spans for LSP goto-definition (name -> Vec<Range>, multiple for bus/slice expansion)
    port_spans: HashMap<String, Vec<Range<usize>>>,
    /// Port reference spans from net lines (for LSP goto-definition)
    port_ref_spans: Vec<(Range<usize>, String)>,
}

impl McParamDeclares {
    pub fn new() -> Self {
        Self {
            declares: Vec::new(),
            port_spans: HashMap::new(),
            port_ref_spans: Vec::new(),
        }
    }

    /// Parse parameter declaration list from AST node
    /// Collects port spans for all parameter ports (both IOTYPE-prefixed and plain).
    pub fn parse(&mut self, node: &AstNode) {
        // Recursively handle all parameter declaration nodes, supporting all rule branches
        if let Some(subnode) = node.get_sub_node() {
            let mut param_iter = subnode.iter().peekable();
            while let Some(param_node) = param_iter.next() {
                let body_type = param_node.get_type();

                // Determine IOType and port name(s), store spans
                match body_type {
                    MCAST_ID | MCAST_IDA | MCAST_IDS => {
                        if let Some(ids) = McIds::new(&param_node) {
                            let span = (param_node.get_pos() as usize)
                                ..((param_node.get_pos() + param_node.get_len()) as usize);
                            if let Some((bus_name, _)) = ids.as_bus() {
                                self.store_port_span(&bus_name, span);
                            } else if ids.is_square_only() {
                                self.store_port_span(&format!("@{}", self.port_spans.len()), span);
                            } else {
                                self.store_port_span(&ids.to_string(), span);
                            }
                        }
                    }
                    MCAST_SQUARE_VEC => {
                        // [VDD1, GND1] - anonymous set, store as @N
                        let span = (param_node.get_pos() as usize)
                            ..((param_node.get_pos() + param_node.get_len()) as usize);
                        self.store_port_span(&format!("@{}", self.port_spans.len()), span);
                    }
                    MCAST_IOTYPE => {
                        // IOTYPE-prefixed port: ps dc24v, in GPIO[1:2], etc.
                        // MCAST_IOTYPE is the param_node, operands are subsequent siblings (until next IOTYPE)
                        while let Some(next) = param_iter.peek() {
                            if next.get_type() == MCAST_IOTYPE {
                                break; // Stop at next IOTYPE keyword
                            }
                            let current = param_iter.next().unwrap();
                            let op_type = current.get_type();
                            if matches!(op_type, MCAST_ID | MCAST_IDA | MCAST_IDS) {
                                if let Some(ids) = McIds::new(&current) {
                                    let span = (current.get_pos() as usize)
                                        ..((current.get_pos() + current.get_len()) as usize);
                                    if let Some((bus_name, _)) = ids.as_bus() {
                                        self.store_port_span(&bus_name, span);
                                    } else if ids.is_square_only() {
                                        self.store_port_span(
                                            &format!("@{}", self.port_spans.len()),
                                            span,
                                        );
                                    } else {
                                        self.store_port_span(&ids.to_string(), span);
                                    }
                                }
                            } else if op_type == MCAST_SQUARE_VEC {
                                let span = (current.get_pos() as usize)
                                    ..((current.get_pos() + current.get_len()) as usize);
                                self.store_port_span(&format!("@{}", self.port_spans.len()), span);
                            }
                        }
                        // Don't call McParamDeclare::new for MCAST_IOTYPE - handled above
                        continue;
                    }
                    _ => {}
                }

                // Also parse as formal parameter
                if let Some(paramd) = McParamDeclare::new(&param_node) {
                    self.declares.push(paramd);
                }
            }
        }
        // else: empty parameter list is legal, no need to error
    }

    /// Find parameter declaration by name
    pub fn find(&self, name: &str) -> Option<&McParamDeclare> {
        self.declares.iter().find(|decl| decl.match_name(name))
    }

    /// Find parameter declaration by name (mutable reference)
    pub fn find_mut(&mut self, name: &str) -> Option<&mut McParamDeclare> {
        self.declares.iter_mut().find(|decl| decl.match_name(name))
    }

    /// Find parameter declaration by index
    pub fn find_by_index(&self, index: usize) -> Option<&McParamDeclare> {
        self.declares.get(index)
    }

    /// Store port span (called when a param port is registered)
    pub(crate) fn store_port_span(&mut self, name: &str, span: Range<usize>) {
        self.port_spans
            .entry(name.to_string())
            .or_default()
            .push(span);
    }

    /// Check if a name is a known parameter port
    pub fn contains(&self, name: &str) -> bool {
        self.port_spans.contains_key(name)
    }

    /// Iterate all parameter ports with their spans (expands bus/slice to multiple entries)
    pub fn iter_ports_with_span(&self) -> impl Iterator<Item = (&str, Range<usize>)> + '_ {
        self.port_spans
            .iter()
            .flat_map(|(name, spans)| spans.iter().map(move |span| (name.as_str(), span.clone())))
    }

    /// Record a reference to this param port (for LSP goto-definition from net lines)
    pub(crate) fn record_port_ref(&mut self, span: Range<usize>, port_name: &str) {
        if let Some(spans) = self.port_spans.get_mut(port_name) {
            // Only record ref if the span differs from all stored def spans
            if !spans
                .iter()
                .any(|s| s.start == span.start && s.end == span.end)
            {
                // Store ref separately - use a dedicated ref_spans map
                self.port_ref_spans.push((span, port_name.to_string()));
            }
        }
    }

    /// Iterate param port reference spans (from net lines)
    pub fn iter_port_refs(&self) -> impl Iterator<Item = &(Range<usize>, String)> + '_ {
        self.port_ref_spans.iter()
    }

    /// Get parameter count
    pub fn len(&self) -> usize {
        self.declares.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.declares.is_empty()
    }

    /// Get all parameter names
    pub fn names(&self) -> Vec<String> {
        self.declares
            .iter()
            .filter_map(|d| d.get_primary_name())
            .collect()
    }

    pub fn get_params_with_defaults(&self) -> Vec<(McIds, String)> {
        self.declares
            .iter()
            .filter_map(|d| d.get_name_with_default())
            .collect()
    }
}

impl std::ops::Deref for McParamDeclares {
    type Target = Vec<McParamDeclare>;

    fn deref(&self) -> &Self::Target {
        &self.declares
    }
}

impl std::ops::DerefMut for McParamDeclares {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.declares
    }
}

impl<'a> IntoIterator for &'a McParamDeclares {
    type Item = &'a McParamDeclare;
    type IntoIter = std::slice::Iter<'a, McParamDeclare>;

    fn into_iter(self) -> Self::IntoIter {
        self.declares.iter()
    }
}

/// Single parameter declaration
#[derive(Debug, Clone)]
pub enum McParamDeclare {
    Role(McIds),
    Single(McIds),
    Multiple(Vec<McIds>),
    UValue(McUnitValueDeclare),
}

impl McParamDeclare {
    /// Create parameter declaration from AST node
    pub fn new(node: &AstNode) -> Option<Self> {
        //mc_pard: MCK_ROLE
        //       | mc_ids
        //       | MCPT_LBRACKET mc_phrases MCPT_RBRACKET
        //       | mc_ids MCPT_DBCOLON mc_unit_type
        //       | mc_ids MCPT_DBCOLON mc_unit_type MCOP_EQUAL mc_literal
        //       | mc_ids MCPT_DBCOLON mc_unit_type MCOP_EQUAL mc_phrase

        // ── Iter-3.B2 ────────────────────────────────────────────────────
        // Allow two incoming forms:
        //   (A) node IS MCAST_PARAM, with one child as actual param body
        //   (B) node IS the param body (MCAST_DECLARE / MCAST_ID / MCAST_SQUARE_VEC / ...)
        //       —— some parser paths pass MCAST_PARAMS child node directly,
        //       no longer wrapping MCAST_PARAM layer
        // Old code only recognized (A), once parser goes (B) path, the whole param gets swallowed,
        // causing both formal params of `func power(V3V3::DC(3.3V), V1V2::DC(1.2V))` to be lost.
        let subnode = if node.get_type() == MCAST_PARAM {
            node.get_sub_node()?
        } else {
            // (B) form: use node directly as param body
            node.clone()
        };

        match subnode.get_type() {
            MCAST_ROLE => Some(McParamDeclare::Role(McIds::from("role"))),
            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                if let Some(ids) = McIds::new(&subnode) {
                    Some(McParamDeclare::Single(ids))
                } else {
                    dlog_error(1304, node, "Invalid param name.");
                    None
                }
            }
            MCAST_SQUARE_VEC => {
                // [VDD, GND] parses as Set
                // subnode is mc_phrases (linked list), each element is mc_phrase
                let mut phrases = Vec::new();
                let mut current = subnode.get_sub_node();
                while let Some(phrase_node) = current {
                    // mc_phrase may be mc_ids, need to extract child node
                    let ids_node = phrase_node
                        .get_sub_node()
                        .unwrap_or_else(|| phrase_node.clone());
                    if let Some(ids) = McIds::new(&ids_node) {
                        phrases.push(ids);
                    }
                    current = phrase_node.get_next();
                }
                if !phrases.is_empty() {
                    Some(McParamDeclare::Multiple(phrases))
                } else {
                    dlog_error(1305, node, "Invalid param set.");
                    None
                }
            }

            MCAST_DECLARE_UV => {
                if let Some(uval) = McUnitValueDeclare::new(&subnode) {
                    Some(McParamDeclare::UValue(uval))
                } else {
                    dlog_error(1307, node, "Invalid param uval.");
                    None
                }
            }

            // ── Iter-3.B ────────────────────────────────────────────────
            // Handle `id::Interface(args)` / `id{members}::Interface()` /
            // `[members]::Interface()` / `id[range]::Interface()` forms.
            // These are MCAST_DECLARE nodes in AST:
            //   MCAST_DECLARE
            //     ├─ MCAST_CLASS (interface/type name, e.g. DC)
            //     └─ MCAST_INSTANCE (param name, e.g. V3V3 / dc{VDD_3V3, GND})
            //       [or multiple MCAST_INSTANCE, if parser pre-expands pwr[1:3]]
            // Previously no such branch, so `func power(V3V3::DC(3.3V), V1V2::DC(1.2V))`
            // both formal params were lost, causing TooManyArguments on call.
            //
            // ── Iter-3.B4 fix ──────────────────────────────────────────
            // Important: traversing MCAST_DECLARE children must first `get_sub_node()` to get
            // first child, then `.iter()` traverse sibling chain. Directly iter on subnode itself
            // only gets subnode and its siblings (usually itself), can't see inner MCAST_CLASS/
            // MCAST_INSTANCE. This is the culprit of Iter-3.B v1 silently returning 0 params.
            MCAST_DECLARE => {
                // Collect all MCAST_INSTANCE formal param names McIds
                let mut inst_ids_list: Vec<McIds> = Vec::new();
                if let Some(decl_first_child) = subnode.get_sub_node() {
                    for child in decl_first_child
                        .iter()
                        .filter(|n| n.get_type() == MCAST_INSTANCE)
                    {
                        if let Some(inner) = child.get_sub_node() {
                            // inner may be IDS directly, or MCAST_OPD wrapped
                            let ids_node = if inner.get_type() == MCAST_OPD {
                                inner.get_sub_node().unwrap_or(inner.clone())
                            } else {
                                inner.clone()
                            };

                            // [members]::Type form: ids_node is SQUARE_VEC -> flatten into Multiple
                            if ids_node.get_type() == MCAST_SQUARE_VEC {
                                let mut current = ids_node.get_sub_node();
                                while let Some(phrase_node) = current {
                                    let inner_ids = phrase_node
                                        .get_sub_node()
                                        .unwrap_or_else(|| phrase_node.clone());
                                    if let Some(ids) = McIds::new(&inner_ids) {
                                        inst_ids_list.push(ids);
                                    }
                                    current = phrase_node.get_next();
                                }
                            } else if let Some(ids) = McIds::new(&ids_node) {
                                // Common forms: id / id{members} / id[range]
                                inst_ids_list.push(ids);
                            }
                        }
                    }
                }

                match inst_ids_list.len() {
                    0 => {
                        dlog_error(
                            1310,
                            node,
                            "Failed to extract parameter name from MCAST_DECLARE",
                        );
                        None
                    }
                    1 => Some(McParamDeclare::Single(
                        inst_ids_list.into_iter().next().unwrap(),
                    )),
                    _ => Some(McParamDeclare::Multiple(inst_ids_list)),
                }
            }

            _ => {
                dlog_error(1303, node, "Invalid param declare node.");
                None
            }
        }
    }

    /// Check if the name matches
    pub fn match_name(&self, target: &str) -> bool {
        match self {
            McParamDeclare::Role(role) => role.match_name(target),
            McParamDeclare::Single(ids) => ids.match_name(target),
            McParamDeclare::Multiple(_) => false,
            McParamDeclare::UValue(_) => false,
        }
    }

    /// Get the primary name
    pub fn get_primary_name(&self) -> Option<String> {
        match self {
            McParamDeclare::Role(role) => role.get_primary_name(),
            McParamDeclare::Single(ids) => ids.get_primary_name(),
            McParamDeclare::Multiple(_) => None,
            McParamDeclare::UValue(uval) => uval.name.get_primary_name(),
        }
    }

    /// Get the type name string
    pub fn get_class_name(&self) -> Option<String> {
        match self {
            McParamDeclare::Role(_) => None,
            McParamDeclare::Single(_) => None,
            McParamDeclare::Multiple(_) => None,
            McParamDeclare::UValue(_) => None,
        }
    }

    /// Check if there is a type constraint
    pub fn has_type_constraint(&self) -> bool {
        match self {
            McParamDeclare::Role(_) => false,
            McParamDeclare::Single(_) => false,
            McParamDeclare::Multiple(_) => false,
            McParamDeclare::UValue(_) => false,
        }
    }

    /// Check if there are type parameters
    pub fn has_class_params(&self) -> bool {
        false
    }

    /// Expand parameter names into a string list
    pub fn expand(&self) -> Vec<String> {
        match self {
            McParamDeclare::Role(role) => role.expand(),
            McParamDeclare::Single(ids) => ids.expand(),
            McParamDeclare::Multiple(_) => Vec::new(),
            McParamDeclare::UValue(_) => Vec::new(),
        }
    }

    pub fn get_name_with_default(&self) -> Option<(McIds, String)> {
        match self {
            McParamDeclare::Single(ids) => ids
                .get_primary_name()
                .map(|name| (McIds::from(name.as_str()), String::new())),
            McParamDeclare::UValue(uval) => uval
                .default
                .as_ref()
                .map(|default| (uval.name.clone(), default.clone())),
            _ => None,
        }
    }
}

impl std::fmt::Display for McParamDeclare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McParamDeclare::Role(role) => write!(f, "{role}"),
            McParamDeclare::Single(ids) => write!(f, "{ids}"),
            McParamDeclare::Multiple(_phrases) => write!(f, "[, ]"),
            McParamDeclare::UValue(uval) => write!(f, "{uval}"),
        }
    }
}
