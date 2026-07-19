// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::semantic::basic::mc_param_type::McParamType;
use crate::semantic::basic::mc_uval::McUnitValueDeclare;
use crate::McIds;
use crate::{ast::ast_node::AstNode, ast::c_macros::*, builder::diagnostic::dlog_error};
use std::collections::HashMap;
use std::ops::Range;

/// Parameter declaration list
#[derive(Debug, Clone, Default)]
pub struct McParamDeclares {
    declares: Vec<McParamDeclare>,
    /// Definition spans for ALL parameters (never filtered — always available for goto-def).
    /// name -> Vec<Range>, multiple for bus/slice expansion.
    def_spans: HashMap<String, Vec<Range<usize>>>,
    /// Port spans for LSP goto-definition from net lines (Category A only).
    /// Filtered by `filter_port_spans()` after type inference.
    port_spans: HashMap<String, Vec<Range<usize>>>,
    /// Port reference spans from net lines (for LSP goto-definition)
    port_ref_spans: Vec<(Range<usize>, String, String)>, // (span, port_name, scope)
}

impl McParamDeclares {
    pub fn new() -> Self {
        Self {
            declares: Vec::new(),
            def_spans: HashMap::new(),
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

                // Determine IOType and port name(s), store spans.
                // Handle both MCAST_PARAM-wrapped and direct child forms.
                let inner = if body_type == MCAST_PARAM {
                    param_node
                        .get_sub_node()
                        .unwrap_or_else(|| param_node.clone())
                } else {
                    param_node.clone()
                };
                let inner_type = inner.get_type();

                match inner_type {
                    MCAST_ID | MCAST_IDA | MCAST_IDS => {
                        if let Some(ids) = McIds::new(&inner) {
                            let span = (inner.get_pos() as usize)
                                ..((inner.get_pos() + inner.get_len()) as usize);
                            self.store_def_span(&ids.to_string(), span);
                        }
                    }
                    MCAST_DECLARE_UV => {
                        // volt::UV.VOLT = 5V — the name precedes the DECLARE_UV
                        // node by name.len() + 2 bytes (for the "::" separator).
                        if let Some(paramd) = McParamDeclare::new(&inner) {
                            if let Some(name) = paramd.get_primary_name() {
                                let inner_pos = inner.get_pos() as usize;
                                let prefix_len = name.len() + 2; // "name::"
                                let start = if inner_pos > prefix_len {
                                    inner_pos - prefix_len
                                } else {
                                    inner_pos
                                };
                                let name_span = start..(start + name.len());
                                self.store_def_span(&name, name_span);
                            }
                            self.declares.push(paramd);
                        }
                    }
                    MCAST_SQUARE_VEC => {
                        // [VDD1, GND1] — iterate members and store each
                        // with its *individual* span so PortDefinition
                        // entries match declare_instance entries.
                        let mut current = inner.get_sub_node();
                        while let Some(phrase_node) = current {
                            let ids_node = phrase_node
                                .get_sub_node()
                                .unwrap_or_else(|| phrase_node.clone());
                            if let Some(ids) = McIds::new(&ids_node) {
                                let member_span = (ids_node.get_pos() as usize)
                                    ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                                self.store_def_span(&ids.to_string(), member_span);
                            }
                            current = phrase_node.get_next();
                        }
                    }
                    MCAST_IOTYPE => {
                        // Collect children of this IOTYPE node.
                        // Two call patterns:
                        // 1) Full MCAST_PARAMS: children follow the IOTYPE as siblings in param_iter.
                        // 2) Single MCAST_PARAM: children are inside the IOTYPE node itself.
                        let children: Vec<AstNode> = {
                            // First try siblings from param_iter (full-params call)
                            let mut v: Vec<AstNode> = Vec::new();
                            while let Some(next) = param_iter.peek() {
                                if next.get_type() == MCAST_IOTYPE {
                                    break;
                                }
                                v.push(param_iter.next().unwrap());
                            }
                            if v.is_empty() {
                                // Single-param call — iterate IOTYPE's own children
                                if let Some(first) = inner.get_sub_node() {
                                    // Skip the iotype token itself, iterate subsequent children
                                    let mut cur = first.get_next();
                                    while let Some(child) = cur {
                                        v.push(child.clone());
                                        cur = child.get_next();
                                    }
                                }
                            }
                            v
                        };
                        for current in &children {
                            let op_type = current.get_type();
                            if matches!(op_type, MCAST_ID | MCAST_IDA | MCAST_IDS) {
                                if let Some(paramd) = McParamDeclare::new(current) {
                                    if let Some(name) = paramd.get_primary_name() {
                                        let span = (current.get_pos() as usize)
                                            ..((current.get_pos() + current.get_len()) as usize);
                                        self.store_def_span(&name, span);
                                    }
                                    self.declares.push(paramd);
                                }
                            } else if op_type == MCAST_OPD
                                || op_type == MCAST_OPD_SQUARE_VEC
                                || op_type == MCAST_SQUARE_VEC
                            {
                                // For OPD_SQUARE_VEC, pass the node directly to McParamDeclare::new()
                                // (which handles it via the MCAST_OPD_SQUARE_VEC arm).
                                // For plain OPD, unwrap to reach the inner ID/SQUARE_VEC.
                                let inner = if op_type == MCAST_OPD_SQUARE_VEC
                                    || op_type == MCAST_SQUARE_VEC
                                {
                                    current.clone()
                                } else {
                                    let inner =
                                        current.get_sub_node().unwrap_or_else(|| current.clone());
                                    if matches!(inner.get_type(), MCAST_OPD) {
                                        inner.get_sub_node().unwrap_or(inner)
                                    } else {
                                        inner
                                    }
                                };
                                if let Some(paramd) = McParamDeclare::new(&inner) {
                                    let span = (current.get_pos() as usize)
                                        ..((current.get_pos() + current.get_len()) as usize);
                                    if let McParamDeclareKind::Multiple(members) = &paramd.kind {
                                        // Multiple stores Vec<McIds> (name-only, no pos).
                                        // Use parent span for all members.
                                        for m in members {
                                            if let Some(name) = m.get_primary_name() {
                                                self.store_def_span(&name, span.clone());
                                            }
                                        }
                                    } else if let Some(name) = paramd.get_primary_name() {
                                        self.store_def_span(&name, span);
                                    }
                                    self.declares.push(paramd);
                                }
                            }
                        }
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

    /// Store definition span for a parameter (called for ALL params during parse).
    /// Writes to both `def_spans` (never filtered, used for goto-def from any reference)
    /// and `port_spans` (filtered later for net connectivity only).
    pub(crate) fn store_def_span(&mut self, name: &str, span: Range<usize>) {
        self.def_spans
            .entry(name.to_string())
            .or_default()
            .push(span.clone());
        self.port_spans
            .entry(name.to_string())
            .or_default()
            .push(span);
    }

    /// Check if a name is a known parameter port (Category A only, for net connectivity).
    pub fn contains(&self, name: &str) -> bool {
        self.port_spans.contains_key(name)
    }

    /// Check if a name is a defined parameter (any category, for goto-def).
    pub fn is_defined(&self, name: &str) -> bool {
        self.def_spans.contains_key(name)
    }

    /// Iterate all parameter ports with their spans (Category A only).
    pub fn iter_ports_with_span(&self) -> impl Iterator<Item = (&str, Range<usize>)> + '_ {
        self.port_spans
            .iter()
            .flat_map(|(name, spans)| spans.iter().map(move |span| (name.as_str(), span.clone())))
    }

    /// Iterate all parameter definition spans (any category, for goto-def).
    pub fn iter_defs_with_span(&self) -> impl Iterator<Item = (&str, Range<usize>)> + '_ {
        self.def_spans
            .iter()
            .flat_map(|(name, spans)| spans.iter().map(move |span| (name.as_str(), span.clone())))
    }

    /// Record a reference to this parameter (for LSP goto-def from body references).
    /// Uses `def_spans` so ALL params (including B/C categories) support goto-def.
    pub(crate) fn record_port_ref(&mut self, span: Range<usize>, port_name: &str, scope: &str) {
        if let Some(spans) = self.def_spans.get_mut(port_name) {
            if !spans
                .iter()
                .any(|s| s.start == span.start && s.end == span.end)
            {
                self.port_ref_spans
                    .push((span, port_name.to_string(), scope.to_string()));
            }
        }
    }

    pub fn iter_port_refs(&self) -> impl Iterator<Item = &(Range<usize>, String, String)> + '_ {
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

    /// Get all parameter names (single params only, drops Multiples).
    pub fn names(&self) -> Vec<String> {
        self.declares
            .iter()
            .filter_map(|d| d.get_primary_name())
            .collect()
    }

    /// Get all parameter names including compound forms.
    /// `[VDD1, GND1]` style params are rendered as `[VDD1, GND1]`.
    pub fn names_full(&self) -> Vec<String> {
        self.declares.iter().map(|d| d.display_name()).collect()
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
    pub fn arity(&self) -> crate::semantic::basic::mc_param_type::McParamArity {
        crate::semantic::basic::mc_param_type::McParamArity::from_declares(&self.declares)
    }

    /// Finalize parameters after body parsing: run usage inference on Unknown params,
    /// check for unused parameters, filter port spans.
    ///
    /// Returns a list of diagnostic messages for unused/untyped parameters.
    pub fn finalize(&mut self, body: Option<&AstNode>, def_name: &str) -> Vec<GlobalDiag> {
        let mut diagnostics = Vec::new();

        // Step 1: Run usage-based inference for Unknown params
        if let Some(body_node) = body {
            let unused =
                crate::semantic::basic::mc_param_infer::find_unused_params(&self.declares, body_node);
            for name in &unused {
                // Try exact match first, then substring match (def_spans may
                // store the full form "rs485{A,B}" while display_name returns "rs485").
                let (pos, len) = self
                    .def_spans
                    .get(name)
                    .or_else(|| {
                        self.def_spans
                            .iter()
                            .find(|(k, _)| k.contains(name.as_str()) || name.contains(k.as_str()))
                            .map(|(_, v)| v)
                    })
                    .and_then(|spans| spans.first())
                    .map(|s| (s.start, s.end - s.start))
                    .unwrap_or((0, 0));
                diagnostics.push(GlobalDiag {
                    kind: GlobalDiagKind::Unused,
                    param_name: name.clone(),
                    definition: def_name.to_string(),
                    message: format!(
                        "Parameter '{}' in '{}' is never used. Consider removing it or adding a type annotation.",
                        name, def_name
                    ),
                    pos,
                    len,
                });
            }

            // Step 2: Run inference on Unknown (bare identifier) params
            for declare in self.declares.iter_mut() {
                if declare.param_type.kind
                    == crate::semantic::basic::mc_param_type::McParamTypeKind::Unknown
                {
                    if let Some(name) = declare.get_primary_name() {
                        if !unused.contains(&name) {
                            let result =
                                crate::semantic::basic::mc_param_infer::infer_param(&name, body_node);
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

        // Step 4: (reserved for future type-annotation suggestions)

        diagnostics
    }
}

/// Lightweight diagnostic returned by `finalize()` during parsing.
///
/// Callers convert these to regular diagnostics via [`mcc_log_global_diag`]
/// which routes them into the per-file [`DiagnosticManager`](crate::builder::diagnostic::DiagnosticManager).
///
/// Variants:
/// - `Unused`  — declared but unreferenced parameters / ports
/// - `Untyped` — parameters that could not be type-inferred
#[derive(Debug, Clone)]
pub struct GlobalDiag {
    pub kind: GlobalDiagKind,
    pub param_name: String,
    pub definition: String,
    pub message: String,
    /// Byte offset of the diagnostic in the source file.
    pub pos: usize,
    /// Byte length of the diagnostic span.
    pub len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalDiagKind {
    /// Parameter / port has no usages in the body
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
            MCAST_OPD_SQUARE_VEC => {
                // [VDD1, GND1] as operand (e.g. after ps/in/io).
                // Each child is an MCAST_OPD wrapping an ID — iterate and collect.
                let mut phrases = Vec::new();
                let mut current = subnode.get_sub_node();
                while let Some(opd_node) = current {
                    // Unwrap MCAST_OPD → inner ID node
                    let inner = opd_node.get_sub_node().unwrap_or_else(|| opd_node.clone());
                    let ids_node = if inner.get_type() == MCAST_OPD {
                        inner.get_sub_node().unwrap_or(inner)
                    } else {
                        inner
                    };
                    if let Some(ids) = McIds::new(&ids_node) {
                        phrases.push(ids);
                    }
                    current = opd_node.get_next();
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

    /// Human-readable display name, including compound forms.
    /// `[VDD1, GND1]` → `"[VDD1, GND1]"`, `GPIO[1:2]` → `"GPIO[1:2]"`, etc.
    pub fn display_name(&self) -> String {
        match &self.kind {
            McParamDeclareKind::Multiple(members) => {
                let names: Vec<String> = members.iter().map(|m| m.to_string()).collect();
                format!("[{}]", names.join(", "))
            }
            _ => self.get_primary_name().unwrap_or_default(),
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
            crate::semantic::basic::mc_param_type::McParamTypeKind::Interface { class_name }
            | crate::semantic::basic::mc_param_type::McParamTypeKind::InterfaceWithRole {
                class_name,
                ..
            }
            | crate::semantic::basic::mc_param_type::McParamTypeKind::ComponentInstance {
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

    /// Return all possible name forms for usage-site matching.
    pub fn all_name_forms(&self) -> Vec<String> {
        match &self.kind {
            McParamDeclareKind::Single(ids) => ids.all_name_forms(),
            McParamDeclareKind::Multiple(members) => members
                .iter()
                .flat_map(|ids| ids.all_name_forms())
                .collect(),
            McParamDeclareKind::Role(role) => role.all_name_forms(),
            McParamDeclareKind::UValue(uval) => uval.name.all_name_forms(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_def_spans_persist_after_port_filter() {
        let mut params = McParamDeclares::new();
        params.store_def_span("rs", 0..2);
        params.store_def_span("dc24v", 10..15);

        assert!(params.def_spans.contains_key("rs"));
        assert!(params.port_spans.contains_key("rs"));

        // Simulate: rs=B3 BareNumeric, dc24v=A1 Label
        params.declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("rs")),
            param_type: McParamType {
                kind: crate::semantic::basic::mc_param_type::McParamTypeKind::BareNumeric,
                direction: None,
            },
        });
        params.declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("dc24v")),
            param_type: McParamType {
                kind: crate::semantic::basic::mc_param_type::McParamTypeKind::Label,
                direction: None,
            },
        });

        params.filter_port_spans();

        // def_spans: ALL params kept (for goto-def)
        assert!(
            params.def_spans.contains_key("rs"),
            "rs should remain in def_spans"
        );
        assert!(params.def_spans.contains_key("dc24v"));
        // port_spans: only Category A
        assert!(
            !params.port_spans.contains_key("rs"),
            "rs removed from port_spans"
        );
        assert!(params.port_spans.contains_key("dc24v"));
        // goto-def: is_defined vs contains
        assert!(params.is_defined("rs"));
        assert!(!params.contains("rs"));
    }

    #[test]
    fn test_record_port_ref_uses_def_spans() {
        let mut params = McParamDeclares::new();
        params.store_def_span("rs", 0..2);
        params.declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("rs")),
            param_type: McParamType {
                kind: crate::semantic::basic::mc_param_type::McParamTypeKind::BareNumeric,
                direction: None,
            },
        });
        params.filter_port_spans();

        // Reference should still be recorded via def_spans
        params.record_port_ref(50..52, "rs", "test");
        assert_eq!(params.port_ref_spans.len(), 1);
        assert_eq!(params.port_ref_spans[0].1, "rs");
    }
}
