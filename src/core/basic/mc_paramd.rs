// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::core::basic::mc_param_type::McParamType;
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

    /// After type inference, filter port_spans: only Category A params are ports.
    pub fn filter_port_spans(&mut self) {
        let port_names: std::collections::HashSet<String> = self
            .declares
            .iter()
            .filter(|d| d.is_port())
            .filter_map(|d| d.get_primary_name())
            .collect();
        self.port_spans.retain(|name, _| port_names.contains(name));
    }

    /// Compute arity: total, required, and optional parameter counts.
    pub fn arity(&self) -> crate::core::basic::mc_param_type::McParamArity {
        crate::core::basic::mc_param_type::McParamArity::from_declares(&self.declares)
    }

    /// Finalize parameters after body parsing: run usage inference on Unknown params,
    /// check for unused parameters, filter port spans.
    ///
    /// Returns a list of diagnostic messages for unused/untyped parameters.
    pub fn finalize(&mut self, body: Option<&AstNode>, def_name: &str) -> Vec<ParamDiagnostic> {
        let mut diagnostics = Vec::new();

        // Step 1: Run usage-based inference for Unknown params
        if let Some(body_node) = body {
            let unused =
                crate::core::basic::mc_param_infer::find_unused_params(&self.declares, body_node);
            for name in &unused {
                diagnostics.push(ParamDiagnostic {
                    kind: ParamDiagKind::Unused,
                    param_name: name.clone(),
                    definition: def_name.to_string(),
                    message: format!(
                        "Parameter '{}' in '{}' is never used. Consider removing it or adding a type annotation.",
                        name, def_name
                    ),
                });
            }

            // Step 2: Run inference on Unknown (bare identifier) params
            for declare in self.declares.iter_mut() {
                if declare.param_type.kind
                    == crate::core::basic::mc_param_type::McParamTypeKind::Unknown
                {
                    if let Some(name) = declare.get_primary_name() {
                        if !unused.contains(&name) {
                            let result =
                                crate::core::basic::mc_param_infer::infer_param(&name, body_node);
                            if result.confidence >= 0.7 {
                                declare.set_param_type(result.param_type);
                            }
                        }
                    }
                }
            }
        }

        // Step 3: Filter port_spans based on final type classification
        self.filter_port_spans();

        // Step 4: Warn about remaining Unknown params (have usages but couldn't determine type)
        for declare in self.declares.iter() {
            if declare.param_type.kind
                == crate::core::basic::mc_param_type::McParamTypeKind::Unknown
            {
                if let Some(name) = declare.get_primary_name() {
                    // Only warn if it wasn't already flagged as unused
                    let already_unused = diagnostics.iter().any(|d| d.param_name == name);
                    if !already_unused {
                        diagnostics.push(ParamDiagnostic {
                            kind: ParamDiagKind::Untyped,
                            param_name: name.clone(),
                            definition: def_name.to_string(),
                            message: format!(
                                "Parameter '{}' in '{}' has no type annotation and its type could not be inferred. Consider adding ::INT, ::STRING, ::UV.VOLT, etc.",
                                name, def_name
                            ),
                        });
                    }
                }
            }
        }

        diagnostics
    }
}

/// Diagnostic from parameter analysis.
#[derive(Debug, Clone)]
pub struct ParamDiagnostic {
    pub kind: ParamDiagKind,
    pub param_name: String,
    pub definition: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamDiagKind {
    /// Parameter has no usages in the body
    Unused,
    /// Parameter is untyped and could not be inferred
    Untyped,
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
pub struct McParamDeclare {
    pub kind: McParamDeclareKind,
    /// Semantic type classification — set during parse (明显标注)
    /// or via usage-based inference (未标注). Controls port filtering.
    pub param_type: McParamType,
}

/// The structural form of a parameter declaration (shape, not type).
#[derive(Debug, Clone)]
pub enum McParamDeclareKind {
    Role(McIds),
    Single(McIds),
    Multiple(Vec<McIds>),
    UValue(McUnitValueDeclare),
}

impl McParamDeclare {
    /// Create parameter declaration from AST node, with syntactic type classification.
    pub fn new(node: &AstNode) -> Option<Self> {
        let subnode = if node.get_type() == MCAST_PARAM {
            node.get_sub_node()?
        } else {
            node.clone()
        };

        // Syntactic type classification (handles 明显标注 immediately)
        let param_type = McParamType::from_ast(node);

        let kind = match subnode.get_type() {
            MCAST_ROLE => McParamDeclareKind::Role(McIds::from("role")),
            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                if let Some(ids) = McIds::new(&subnode) {
                    McParamDeclareKind::Single(ids)
                } else {
                    dlog_error(1304, node, "Invalid param name.");
                    return None;
                }
            }
            MCAST_SQUARE_VEC => {
                let mut phrases = Vec::new();
                let mut current = subnode.get_sub_node();
                while let Some(phrase_node) = current {
                    let ids_node = phrase_node
                        .get_sub_node()
                        .unwrap_or_else(|| phrase_node.clone());
                    if let Some(ids) = McIds::new(&ids_node) {
                        phrases.push(ids);
                    }
                    current = phrase_node.get_next();
                }
                if !phrases.is_empty() {
                    McParamDeclareKind::Multiple(phrases)
                } else {
                    dlog_error(1305, node, "Invalid param set.");
                    return None;
                }
            }

            MCAST_DECLARE_UV => {
                if let Some(uval) = McUnitValueDeclare::new(&subnode) {
                    McParamDeclareKind::UValue(uval)
                } else {
                    dlog_error(1307, node, "Invalid param uval.");
                    return None;
                }
            }

            MCAST_DECLARE => {
                let mut inst_ids_list: Vec<McIds> = Vec::new();
                if let Some(decl_first_child) = subnode.get_sub_node() {
                    for child in decl_first_child
                        .iter()
                        .filter(|n| n.get_type() == MCAST_INSTANCE)
                    {
                        if let Some(inner) = child.get_sub_node() {
                            let ids_node = if inner.get_type() == MCAST_OPD {
                                inner.get_sub_node().unwrap_or(inner.clone())
                            } else {
                                inner.clone()
                            };

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
                        return None;
                    }
                    1 => McParamDeclareKind::Single(inst_ids_list.into_iter().next().unwrap()),
                    _ => McParamDeclareKind::Multiple(inst_ids_list),
                }
            }

            _ => {
                dlog_error(1303, node, "Invalid param declare node.");
                return None;
            }
        };

        Some(Self { kind, param_type })
    }

    // ── Name matching ──

    pub fn match_name(&self, target: &str) -> bool {
        match &self.kind {
            McParamDeclareKind::Role(role) => role.match_name(target),
            McParamDeclareKind::Single(ids) => ids.match_name(target),
            McParamDeclareKind::Multiple(_) => false,
            McParamDeclareKind::UValue(_) => false,
        }
    }

    pub fn get_primary_name(&self) -> Option<String> {
        match &self.kind {
            McParamDeclareKind::Role(role) => role.get_primary_name(),
            McParamDeclareKind::Single(ids) => ids.get_primary_name(),
            McParamDeclareKind::Multiple(_) => None,
            McParamDeclareKind::UValue(uval) => uval.name.get_primary_name(),
        }
    }

    // ── Type classification ──

    /// Check if this parameter has an explicit type constraint (明显标注, not Unknown).
    pub fn has_type_constraint(&self) -> bool {
        self.param_type.is_explicitly_typed()
    }

    /// Get the class/interface name if this is an interface-typed param (A3-A5).
    pub fn get_class_name(&self) -> Option<String> {
        match &self.param_type.kind {
            crate::core::basic::mc_param_type::McParamTypeKind::Interface { class_name }
            | crate::core::basic::mc_param_type::McParamTypeKind::InterfaceWithRole {
                class_name,
                ..
            }
            | crate::core::basic::mc_param_type::McParamTypeKind::ComponentInstance {
                class_name,
            } => Some(class_name.clone()),
            _ => None,
        }
    }

    /// Check if this is an interface-typed parameter (has class params like `DC(5V)`).
    pub fn has_class_params(&self) -> bool {
        self.get_class_name().is_some()
    }

    // ── Port classification ──

    /// Whether this is a port (Category A) — affects port_spans and LSP goto-def.
    pub fn is_port(&self) -> bool {
        self.param_type.is_port()
    }

    /// Set the type (called by usage-based inference post-parse).
    pub fn set_param_type(&mut self, pt: McParamType) {
        self.param_type = pt;
    }

    // ── Default value ──

    /// Whether this parameter has a default value (making it optional at call sites).
    pub fn has_default_value(&self) -> bool {
        self.param_type.has_default()
    }

    // ── Expansion ──

    pub fn expand(&self) -> Vec<String> {
        match &self.kind {
            McParamDeclareKind::Role(role) => role.expand(),
            McParamDeclareKind::Single(ids) => ids.expand(),
            McParamDeclareKind::Multiple(_) => Vec::new(),
            McParamDeclareKind::UValue(_) => Vec::new(),
        }
    }

    pub fn get_name_with_default(&self) -> Option<(McIds, String)> {
        match &self.kind {
            McParamDeclareKind::Single(ids) => {
                let name = ids.get_primary_name()?;
                self.param_type
                    .default_value()
                    .map(|dv| (McIds::from(name.as_str()), dv.to_string()))
            }
            McParamDeclareKind::UValue(uval) => uval
                .default
                .as_ref()
                .map(|default| (uval.name.clone(), default.clone())),
            _ => None,
        }
    }
}

impl std::fmt::Display for McParamDeclare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            McParamDeclareKind::Role(role) => write!(f, "{role}"),
            McParamDeclareKind::Single(ids) => write!(f, "{ids}"),
            McParamDeclareKind::Multiple(_phrases) => write!(f, "[, ]"),
            McParamDeclareKind::UValue(uval) => write!(f, "{uval}"),
        }
    }
}
