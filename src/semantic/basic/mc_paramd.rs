// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::basic::mc_param_type::McParamType;
use crate::semantic::basic::mc_uval::McUnitValueDeclare;
use crate::McIds;
use crate::{ast::ast_node::AstNode, ast::c_macros::*};
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
    net_ref_spans: Vec<(Range<usize>, String, String)>, // (span, port_name, scope)
    /// Name of the enclosing component/module, used for scoped enum resolution.
    /// e.g., "CAP" for `component CAP`, "CAP.CER" for `component CAP.CER`.
    pub enclosing_component_name: Option<McIds>,
}

impl McParamDeclares {
    pub fn new() -> Self {
        Self {
            declares: Vec::new(),
            def_spans: HashMap::new(),
            port_spans: HashMap::new(),
            net_ref_spans: Vec::new(),
            enclosing_component_name: None,
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
                // Some grammar rules (e.g., mc_pard -> mc_declare_b) produce
                // MCAST_PARAM -> MCAST_PARAM -> MCAST_DECLARE nesting.
                let inner = if body_type == MCAST_PARAM {
                    let mut unwrapped = param_node
                        .get_sub_node()
                        .unwrap_or_else(|| param_node.clone());
                    // Unwrap extra MCAST_PARAM layer (from mc_pard: mc_declare_b rules)
                    while unwrapped.get_type() == MCAST_PARAM {
                        unwrapped = unwrapped
                            .get_sub_node()
                            .unwrap_or_else(|| unwrapped.clone());
                    }
                    unwrapped
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
                        if let Some(paramd) =
                            McParamDeclare::new(&inner, self.enclosing_component_name.as_ref())
                        {
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
                            continue;
                        }
                    }
                    MCAST_DECLARE => {
                        // diel::CAP = X7R — the name precedes the DECLARE
                        // node by name.len() + 2 bytes (for the "::" separator).
                        if let Some(paramd) =
                            McParamDeclare::new(&inner, self.enclosing_component_name.as_ref())
                        {
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
                            // ★ §3.4.3: typed square-vec params (e.g.
                            // `[VDD_3V3,GND]::DC(3.3V)`) also register each member
                            // with its precise span, so refs like
                            // `uC.power([VDD_3V3,GND], ...)` resolve member-wise.
                            if let Some((whole_name, whole_span)) =
                                self.store_declare_square_member_spans(&inner)
                            {
                                // Override the whole-bracket span with the square-vec
                                // node's exact byte range: the canonical name renders
                                // with `", "` separators (e.g. `[VDD_3V3, GND]`) whose
                                // width differs from the source text `[VDD_3V3,GND]`.
                                if let Some(spans) = self.def_spans.get_mut(&whole_name) {
                                    if let Some(last) = spans.last_mut() {
                                        *last = whole_span.clone();
                                    }
                                }
                                if let Some(spans) = self.port_spans.get_mut(&whole_name) {
                                    if let Some(last) = spans.last_mut() {
                                        *last = whole_span;
                                    }
                                }
                            }
                            self.declares.push(paramd);
                            continue;
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
                                if let Some(paramd) = McParamDeclare::new(
                                    current,
                                    self.enclosing_component_name.as_ref(),
                                ) {
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
                                if let Some(paramd) = McParamDeclare::new(
                                    &inner,
                                    self.enclosing_component_name.as_ref(),
                                ) {
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
                if let Some(paramd) =
                    McParamDeclare::new(&param_node, self.enclosing_component_name.as_ref())
                {
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
    ///
    /// When `name` contains bus notation (e.g. `"rs485{A,B}"`), both the full
    /// form and the base name (`"rs485"`) are stored, so lookups by base name
    /// (e.g. from `find_unused_params`) don't need a suffix-stripping fallback.
    pub(crate) fn store_def_span(&mut self, name: &str, span: Range<usize>) {
        self.def_spans
            .entry(name.to_string())
            .or_default()
            .push(span.clone());
        self.port_spans
            .entry(name.to_string())
            .or_default()
            .push(span.clone());
        // Also register base name (strip "{...}" suffix) to avoid
        // suffix-stripping fallback in finalize().
        if let Some(brace) = name.find('{') {
            let base = &name[..brace];
            if base != name {
                self.def_spans
                    .entry(base.to_string())
                    .or_default()
                    .push(span.clone());
                self.port_spans
                    .entry(base.to_string())
                    .or_default()
                    .push(span.clone());
            }
        }
    }

    /// ★ §3.4.3: store each member of a typed square-vec param with its precise
    /// span, e.g. `[VDD_3V3,GND]::DC(3.3V)` → `VDD_3V3` and `GND` become
    /// independently navigable defs. Returns the whole bracket's exact span
    /// (as the canonical name + byte range) when a square-vec is found, so the
    /// caller can override the approximate whole ParamDef span. No-op for
    /// non-square-vec DECLARE params.
    fn store_declare_square_member_spans(
        &mut self,
        decl_node: &AstNode,
    ) -> Option<(String, Range<usize>)> {
        let decl_first_child = decl_node.get_sub_node()?;
        for child in decl_first_child.iter() {
            if child.get_type() != MCAST_INSTANCE {
                continue;
            }
            let Some(inner) = child.get_sub_node() else {
                continue;
            };
            let ids_node = if inner.get_type() == MCAST_OPD {
                inner.get_sub_node().unwrap_or(inner.clone())
            } else {
                inner.clone()
            };
            if !matches!(ids_node.get_type(), MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC) {
                continue;
            }
            let mut current = ids_node.get_sub_node();
            while let Some(phrase_node) = current {
                let member = phrase_node
                    .get_sub_node()
                    .unwrap_or_else(|| phrase_node.clone());
                if let Some(ids) = McIds::new(&member) {
                    let member_span = (member.get_pos() as usize)
                        ..((member.get_pos() + member.get_len()) as usize);
                    self.store_def_span(&ids.to_string(), member_span);
                }
                current = phrase_node.get_next();
            }
            let whole_span =
                (ids_node.get_pos() as usize)..((ids_node.get_pos() + ids_node.get_len()) as usize);
            let whole_name = McIds::new(&ids_node)?.to_string();
            return Some((whole_name, whole_span));
        }
        None
    }

    /// Check if `name` is a member of a square-vector parameter (e.g. `VDD_3V3`
    /// inside `[VDD_3V3,GND]`). Such member defs register as LabelDef (§3.4.3),
    /// while the whole bracket (e.g. `[VDD_3V3, GND]`) registers as ParamDef.
    pub fn is_square_member(&self, name: &str) -> bool {
        self.declares.iter().any(|d| {
            let members = match &d.kind {
                McParamDeclareKind::Multiple(ids) => {
                    ids.iter().map(|m| m.to_string()).collect::<Vec<_>>()
                }
                McParamDeclareKind::Single(ids) if ids.is_square_only() => {
                    ids.list_members().unwrap_or_default()
                }
                _ => return false,
            };
            members.iter().any(|m| m == name)
        })
    }

    /// Check if a name is a known parameter port (Category A only, for net connectivity).
    pub fn contains(&self, name: &str) -> bool {
        self.port_spans.contains_key(name)
    }

    /// Check if a name is a defined parameter (any category, for goto-def).
    pub fn is_defined(&self, name: &str) -> bool {
        self.def_spans.contains_key(name) || self.find(name).is_some()
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
    pub(crate) fn record_net_ref(&mut self, span: Range<usize>, port_name: &str, scope: &str) {
        // ★ Accept all refs — not all params have def_spans entries (e.g. func params
        //   registered via extract_func_param_spans). The lapper's lookup_declare_id
        //   will resolve or skip unmatched refs.
        self.net_ref_spans
            .push((span, port_name.to_string(), scope.to_string()));
    }

    pub fn iter_net_refs(&self) -> impl Iterator<Item = &(Range<usize>, String, String)> + '_ {
        self.net_ref_spans.iter()
    }

    /// Get parameter count
    pub fn len(&self) -> usize {
        self.declares.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.declares.is_empty()
    }

    /// Returns iterator over parameter declarations (P2-4).
    pub fn iter(&self) -> impl Iterator<Item = &McParamDeclare> {
        self.declares.iter()
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
            let unused = crate::semantic::basic::mc_param_infer::find_unused_params(
                &self.declares,
                body_node,
            );
            for name in &unused {
                // def_spans stores both the full form ("rs485{A,B}") and the
                // base name ("rs485"), so direct lookup always works.
                let (pos, len) = self
                    .def_spans
                    .get(name)
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
                            let result = crate::semantic::basic::mc_param_infer::infer_param(
                                &name, body_node,
                            );
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
/// which routes them into the per-file [`DiagnosticManager`](crate::db::diagnostic::diagnostic::DiagnosticManager).
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
    /// Semantic type classification — set during parse (explicitly annotated)
    /// or via usage-based inference (unannotated). Controls port filtering.
    pub param_type: McParamType,
}

/// Enum-class parameter declaration — `diel::CAP` or `diel::CAP = X7R`.
#[derive(Clone, Debug)]
pub struct McEnumClassDeclare {
    /// Parameter name — `diel`
    pub name: McIds,
    /// Enum class name — `CAP`
    pub class_name: String,
    /// Default value text — `X7R` (None if no `= value`)
    pub default_val: Option<String>,
}

impl McEnumClassDeclare {
    /// Validate that a value name is a member of this enum class.
    /// Returns `true` if the value is valid, `false` otherwise.
    pub fn is_valid_value(&self, value_name: &str) -> bool {
        crate::db::cmie::cmie::is_enum_member(&self.class_name, value_name)
    }
}

/// The structural form of a parameter declaration (shape, not type).
#[derive(Debug, Clone)]
pub enum McParamDeclareKind {
    Role {
        name: McIds,
        /// Default role value when declared as `role = Controller`
        default_role: Option<McIds>,
    },
    Single(McIds),
    Multiple(Vec<McIds>),
    UValue(McUnitValueDeclare),
    EnumClass(McEnumClassDeclare),
}

/// Reconstruct a dotted path string from an MCAST_IDS AST node.
/// E.g., MCAST_IDS(MCAST_ID("CAP"), MCAST_OPD_DOT(MCAST_ID("X7R"))) → "CAP.X7R"
fn ids_to_dotted_string(node: &AstNode) -> Option<String> {
    if node.get_type() != MCAST_IDS {
        return node.to_string();
    }
    let mut result = String::new();
    let mut current = node.get_sub_node();
    while let Some(child) = current {
        match child.get_type() {
            MCAST_ID | MCAST_IDA => {
                if let Some(s) = child.to_string() {
                    result.push_str(&s);
                }
            }
            MCAST_OPD_DOT => {
                result.push('.');
                if let Some(sub) = child.get_sub_node() {
                    if let Some(s) = sub.to_string() {
                        result.push_str(&s);
                    }
                }
            }
            _ => {}
        }
        current = child.get_next();
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

impl McParamDeclare {
    /// Create parameter declaration from AST node, with syntactic type classification.
    pub fn new(node: &AstNode, enclosing_comp_name: Option<&McIds>) -> Option<Self> {
        let subnode = if node.get_type() == MCAST_PARAM {
            let mut unwrapped = node.get_sub_node()?;
            // Unwrap extra MCAST_PARAM layer (from mc_pard: mc_declare_b rules)
            while unwrapped.get_type() == MCAST_PARAM {
                unwrapped = unwrapped
                    .get_sub_node()
                    .unwrap_or_else(|| unwrapped.clone());
            }
            unwrapped
        } else {
            node.clone()
        };

        // Syntactic type classification (handles explicitly annotated forms immediately)
        let mut param_type = McParamType::from_ast(node);

        let kind = match subnode.get_type() {
            MCAST_ROLE => {
                // Check for default role value via next sibling (role = Controller)
                // The C parser links the default value after MCAST_ROLE:
                // MCAST_PARAM(MCAST_ROLE("role") -> MCAST_IDS("Controller"))
                let default_role = subnode
                    .get_next()
                    .and_then(|n| ids_to_dotted_string(&n))
                    .map(|s| McIds::from(s.as_str()));
                McParamDeclareKind::Role {
                    name: McIds::from("role"),
                    default_role,
                }
            }
            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                if let Some(name_ids) = McIds::new(&subnode) {
                    // Check for default value (next sibling after the name node)
                    // e.g., diel = CAP.X7R → PARAM(IDS("diel"), IDS("CAP.X7R"))
                    if let Some(default_node) = subnode.get_next() {
                        if let Some(default_ids) = McIds::new(&default_node) {
                            let default_str = default_ids.to_string();
                            // Check if default is EnumClass.Value format (dotted) — structured
                            // segment extraction (`CAP.X7R` → ["CAP", "X7R"]), no
                            // `to_string()` + `trim_start_matches('.')` text re-processing.
                            // Non-plain chains (curly/square/array segments) fall through.
                            if let Some(parts) = default_ids.dot_chain_parts() {
                                if parts.len() > 1 {
                                    let class_name = parts[0].clone();
                                    let value_name: String = parts[1..].join(".");
                                    if crate::db::cmie::cmie::is_enum_class_name(&class_name) {
                                        param_type.kind = crate::semantic::basic::mc_param_type::McParamTypeKind::EnumClassDefault {
                                            class_name: class_name.clone(),
                                            default_val: Some(value_name.clone()),
                                        };
                                        return Some(Self {
                                            param_type,
                                            kind: McParamDeclareKind::EnumClass(
                                                McEnumClassDeclare {
                                                    name: name_ids,
                                                    class_name,
                                                    default_val: Some(value_name),
                                                },
                                            ),
                                        });
                                    }
                                } else {
                                    // Bare default (no dot): resolve against all known enums.
                                    // e.g., diel = X7R → search all enums for member "X7R".
                                    // Prefer the same-named enum (namespace merging) when available.
                                    let prefer_class =
                                        enclosing_comp_name.and_then(|n| n.root_name());
                                    if let Some(class_name) =
                                        crate::db::cmie::cmie::resolve_bare_enum_value(
                                            &default_str,
                                            prefer_class.as_deref(),
                                        )
                                    {
                                        param_type.kind = crate::semantic::basic::mc_param_type::McParamTypeKind::EnumClassDefault {
                                        class_name: class_name.clone(),
                                        default_val: Some(default_str.clone()),
                                    };
                                        return Some(Self {
                                            param_type,
                                            kind: McParamDeclareKind::EnumClass(
                                                McEnumClassDeclare {
                                                    name: name_ids,
                                                    class_name,
                                                    default_val: Some(default_str),
                                                },
                                            ),
                                        });
                                    }
                                }
                            }
                            // Non-enum default: falls through to Single below
                        }
                    }
                    McParamDeclareKind::Single(name_ids)
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
                // Reclassify as B5/B6 if CLASS is an enum (e.g. diel::CAP)
                param_type.reclassify_if_enum_class(&subnode);

                // Try enum-class path first: diel::CAP = X7R
                if let Some(class_name) = McParamType::extract_class_name_from_declare(&subnode) {
                    // Only treat as enum-class if the class name is actually an enum.
                    // Interface-typed params (e.g., USB_VBUS_1{VDD_3V, GND}::DC(3.3V))
                    // should fall through to the component-instance path so that
                    // bus_members can be extracted from curly/square segments.
                    if crate::db::cmie::cmie::is_enum_class_name(&class_name) {
                        // Extract instance name from MCAST_INSTANCE child
                        let mut inst_name: Option<McIds> = None;
                        let mut default_val: Option<String> = None;
                        if let Some(decl_first_child) = subnode.get_sub_node() {
                            for child in decl_first_child.iter() {
                                if child.get_type() == MCAST_INSTANCE {
                                    if let Some(inner) = child.get_sub_node() {
                                        // First child of INSTANCE is the param name
                                        let name_node = if inner.get_type() == MCAST_OPD {
                                            inner.get_sub_node().unwrap_or(inner.clone())
                                        } else {
                                            inner.clone()
                                        };
                                        if inst_name.is_none() {
                                            inst_name = McIds::new(&name_node);
                                        }
                                        // Check next sibling for default value (MCAST_EXPRESSION)
                                        let mut current = name_node.get_next();
                                        while let Some(c) = current {
                                            if c.get_type() == MCAST_EXPRESSION {
                                                default_val = c.to_string();
                                                break;
                                            }
                                            current = c.get_next();
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(name) = inst_name {
                            return Some(Self {
                                param_type,
                                kind: McParamDeclareKind::EnumClass(McEnumClassDeclare {
                                    name,
                                    class_name,
                                    default_val,
                                }),
                            });
                        }
                    }
                }

                // Fallback: existing component-instance path
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
            McParamDeclareKind::Role { name, .. } => name.match_name(target),
            McParamDeclareKind::Single(ids) => ids.match_name(target),
            McParamDeclareKind::Multiple(_) => false,
            McParamDeclareKind::UValue(_) => false,
            McParamDeclareKind::EnumClass(ec) => ec.name.match_name(target),
        }
    }

    pub fn get_primary_name(&self) -> Option<String> {
        match &self.kind {
            McParamDeclareKind::Role { name, .. } => name.get_primary_name(),
            McParamDeclareKind::Single(ids) => ids.get_primary_name(),
            McParamDeclareKind::Multiple(_) => None,
            McParamDeclareKind::UValue(uval) => uval.name.get_primary_name(),
            McParamDeclareKind::EnumClass(ec) => ec.name.get_primary_name(),
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

    /// Check if this parameter has an explicit type constraint (explicitly annotated, not Unknown).
    pub fn has_type_constraint(&self) -> bool {
        self.param_type.is_explicitly_typed()
    }

    /// Check if this parameter has a physical unit type (Category B: UnitValue / UnitValueDefault).
    /// Used for unit-based claiming in round 2 of parameter binding.
    pub fn has_unit_type(&self) -> bool {
        matches!(
            self.param_type.kind,
            crate::semantic::basic::mc_param_type::McParamTypeKind::UnitValue { .. }
                | crate::semantic::basic::mc_param_type::McParamTypeKind::UnitValueDefault { .. }
                | crate::semantic::basic::mc_param_type::McParamTypeKind::CompoundUnit { .. }
        )
    }

    /// Check if this parameter has an enum-class type (B5: EnumClass / B6: EnumClassDefault).
    pub fn has_enum_class(&self) -> bool {
        matches!(
            self.param_type.kind,
            crate::semantic::basic::mc_param_type::McParamTypeKind::EnumClass { .. }
                | crate::semantic::basic::mc_param_type::McParamTypeKind::EnumClassDefault { .. }
        )
    }

    /// Get the declared physical unit, if this parameter has a unit type.
    pub fn get_declared_unit(&self) -> Option<&crate::semantic::basic::mc_uval::McUnit> {
        match &self.param_type.kind {
            crate::semantic::basic::mc_param_type::McParamTypeKind::UnitValue { unit }
            | crate::semantic::basic::mc_param_type::McParamTypeKind::UnitValueDefault {
                unit,
                ..
            } => Some(unit),
            crate::semantic::basic::mc_param_type::McParamTypeKind::CompoundUnit {
                ref unit_type,
                ..
            } => Some(unit_type.head_unit()),
            _ => None,
        }
    }

    /// Get the full compound unit type tree, if any.
    pub fn get_unit_type(&self) -> Option<&crate::semantic::basic::mc_param_type::McUnitType> {
        match &self.param_type.kind {
            crate::semantic::basic::mc_param_type::McParamTypeKind::CompoundUnit {
                unit_type,
                ..
            } => Some(unit_type),
            _ => None,
        }
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
            }
            | crate::semantic::basic::mc_param_type::McParamTypeKind::EnumClass { class_name }
            | crate::semantic::basic::mc_param_type::McParamTypeKind::EnumClassDefault {
                class_name,
                ..
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
            McParamDeclareKind::Role { name, .. } => name.expand(),
            McParamDeclareKind::Single(ids) => ids.expand(),
            McParamDeclareKind::Multiple(_) => Vec::new(),
            McParamDeclareKind::UValue(_) => Vec::new(),
            McParamDeclareKind::EnumClass(ec) => ec.name.expand(),
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
            McParamDeclareKind::Role { name, .. } => name.all_name_forms(),
            McParamDeclareKind::UValue(uval) => uval.name.all_name_forms(),
            McParamDeclareKind::EnumClass(ec) => ec.name.all_name_forms(),
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
            McParamDeclareKind::EnumClass(ec) => ec
                .default_val
                .as_ref()
                .map(|default| (ec.name.clone(), default.clone())),
            McParamDeclareKind::Role { name, default_role } => default_role
                .as_ref()
                .map(|dr| (name.clone(), dr.to_string())),
            _ => None,
        }
    }

    // ── P2-4: extract port name and members for interface-type params ──
    /// Returns `(port_name, members)` for interface-type parameters.
    ///
    /// For `[VDD_3V3, GND]::DC(3.3V)` → `("[VDD_3V3, GND]", ["VDD_3V3", "GND"])`
    /// For `dc{VDD_3V3, GND}::DC(3.3V)` → `("dc{VDD_3V3, GND}", ["VDD_3V3", "GND"])`
    pub fn to_port_name_and_members(&self) -> Option<(String, Vec<String>)> {
        if !self.param_type.is_port() {
            return None;
        }
        match &self.kind {
            McParamDeclareKind::Multiple(ids_list) => {
                let members: Vec<String> = ids_list.iter().map(|ids| ids.to_string()).collect();
                let name = format!("[{}]", members.join(", "));
                Some((name, members))
            }
            McParamDeclareKind::Single(ids) => {
                let name = ids.to_string();
                Some((name, vec![]))
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for McParamDeclare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            McParamDeclareKind::Role { name, default_role } => {
                if let Some(ref dr) = default_role {
                    write!(f, "{name} = {dr}")
                } else {
                    write!(f, "{name}")
                }
            }
            McParamDeclareKind::Single(ids) => write!(f, "{ids}"),
            McParamDeclareKind::Multiple(_phrases) => write!(f, "[, ]"),
            McParamDeclareKind::UValue(uval) => write!(f, "{uval}"),
            McParamDeclareKind::EnumClass(ec) => {
                if let Some(ref dv) = ec.default_val {
                    write!(f, "{} = {}.{}", ec.name, ec.class_name, dv)
                } else {
                    write!(f, "{} = {}.?", ec.name, ec.class_name)
                }
            }
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
    fn test_record_net_ref_uses_def_spans() {
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
        params.record_net_ref(50..52, "rs", "test");
        assert_eq!(params.net_ref_spans.len(), 1);
        assert_eq!(params.net_ref_spans[0].1, "rs");
    }
}
