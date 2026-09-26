// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::basic::mc_conds::{CondFamily, CondParam};
use crate::semantic::basic::mc_literal::strip_string_quotes;
use crate::semantic::basic::mc_param_type::McParamType;
use crate::semantic::basic::mc_uval::McUnitValueDeclare;
use crate::McIds;
use crate::{ast::macros::*, ast::node::AstNode};
use std::collections::HashMap;
use std::ops::Range;

/// Parameter declaration list
#[derive(Debug, Clone, Default)]
pub struct McParamDeclares {
    declares: Vec<McParamDeclare>,
    /// Definition spans for ALL parameters (never filtered — always available for goto-def).
    /// name -> Vec<Range>, multiple for bus/slice expansion.
    def_spans: HashMap<String, Vec<Range<usize>>>,
    /// Port spans for LSP goto-definition from net stmts (Category A only).
    /// Filtered by `filter_port_spans()` after type inference.
    port_spans: HashMap<String, Vec<Range<usize>>>,
    /// Port reference spans from net stmts (for LSP goto-definition)
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
                        self.register_declare_param(&inner);
                        continue;
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
                        // 1) Full MCAST_PARAMS: children follow the IOTYPE as siblings in
                        // param_iter.
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
                                // For OPD_SQUARE_VEC, pass the node directly to
                                // McParamDeclare::new()
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
                            } else if op_type == MCAST_DECLARE {
                                // Direction-word header port: `psnk [VDD,GND]::DC(3.3V)`.
                                // The DECLARE carrying the interface class and the
                                // port names sits beside the IOTYPE token.
                                self.register_declare_param(current);
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

    /// Register one closure formal (`=> |ports| { ... }`, U300 M2).
    ///
    /// Closure formals are plain operand names, not module-header param rows,
    /// so the shared [`Self::parse`] walk cannot classify them; this entry
    /// point records the name (with its span) so the closure body and the
    /// floating-label pass see it as a declared face.
    pub fn push_closure_formal(&mut self, node: &AstNode) {
        if let Some(paramd) = McParamDeclare::new(node, self.enclosing_component_name.as_ref()) {
            if let Some(name) = paramd.get_primary_name() {
                let span = (node.get_pos() as usize)
                    ..((node.get_pos() + node.get_len()) as usize);
                self.store_def_span(&name, span);
            }
            self.declares.push(paramd);
        }
    }

    /// Register a `name::Class(args)` parameter declaration: the name text
    /// precedes the DECLARE node, square-vec members get their own spans.
    fn register_declare_param(&mut self, inner: &AstNode) {
        if let Some(paramd) = McParamDeclare::new(inner, self.enclosing_component_name.as_ref()) {
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
            // §3.4.3: typed square-vec params register each member with its
            // precise span, and the whole bracket takes the exact span.
            if let Some((whole_name, whole_span)) = self.store_declare_square_member_spans(inner) {
                if !self.def_spans.contains_key(&whole_name) {
                    self.store_def_span(&whole_name, whole_span.clone());
                }
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
        }
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

    /// Find the formal parameter whose declared name forms cover `name`.
    ///
    /// Name forms include the canonical form (`dc{VDD_3V3, GND}`), expanded
    /// member names (`dc.VDD_3V3`), and the base name of a bus/list formal
    /// (`dc`). Base-name matching is structural: the base alias left
    /// `def_spans` (T7, G8), so goto-def for a chain base such as `dc` in
    /// `dc.GND` must still land on the whole parameter declaration instead of
    /// falling through to a synthetic instance def.
    pub fn find_form(&self, name: &str) -> Option<&McParamDeclare> {
        self.declares
            .iter()
            .find(|d| d.all_name_forms().iter().any(|f| f == name))
    }

    /// Store definition span for a parameter (called for ALL params during parse).
    /// Writes to both `def_spans` (never filtered, used for goto-def from any reference)
    /// and `port_spans` (filtered later for net connectivity only).
    ///
    /// T7 (G8): the two projections carry different keys. `def_spans` keeps
    /// the whole declared form (e.g. `rs485{A,B}`) for goto-def; `port_spans`
    /// keeps only the port identity — the base name of a curly-bus form — so
    /// a bus parameter is one Category-A port, never registered twice.
    pub(crate) fn store_def_span(&mut self, name: &str, span: Range<usize>) {
        self.def_spans
            .entry(name.to_string())
            .or_default()
            .push(span.clone());
        let port_key = name.split('{').next().unwrap_or(name);
        self.port_spans
            .entry(port_key.to_string())
            .or_default()
            .push(span);
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
    ///
    /// The backing `port_spans` map is a `HashMap`, so iteration is sorted by
    /// source position first to keep registration order (and the resulting
    /// symbol ids) stable across runs.
    pub fn iter_ports_with_span(&self) -> impl Iterator<Item = (&str, Range<usize>)> + '_ {
        let mut items: Vec<(&str, Range<usize>)> = self
            .port_spans
            .iter()
            .flat_map(|(name, spans)| spans.iter().map(move |span| (name.as_str(), span.clone())))
            .collect();
        items.sort_by(|a, b| (a.1.start, a.0).cmp(&(b.1.start, b.0)));
        items.into_iter()
    }

    /// Iterate all parameter definition spans (any category, for goto-def).
    ///
    /// Sorted by source position for the same determinism reason as
    /// [`Self::iter_ports_with_span`].
    pub fn iter_defs_with_span(&self) -> impl Iterator<Item = (&str, Range<usize>)> + '_ {
        let mut items: Vec<(&str, Range<usize>)> = self
            .def_spans
            .iter()
            .flat_map(|(name, spans)| spans.iter().map(move |span| (name.as_str(), span.clone())))
            .collect();
        items.sort_by(|a, b| (a.1.start, a.0).cmp(&(b.1.start, b.0)));
        items.into_iter()
    }

    /// Look up the first definition span by canonical name (any category).
    /// `def_spans` is never filtered (unlike `port_spans`), so this also
    /// resolves square-vec signature ports such as `[VDD_3V3, GND]::DC(3.3V)`:
    /// `filter_port_spans` drops their whole-bracket name (Multiple form has no
    /// primary name), while `def_spans` keeps it with the exact bracket span.
    pub fn get_def_span(&self, name: &str) -> Option<Range<usize>> {
        self.def_spans.get(name).and_then(|v| v.first().cloned())
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

    /// Get all parameter names including compound forms, with interface
    /// annotations preserved — `V3V3::DC(3.3V)`, `[VDD, GND]::DC(3.3V)`.
    pub fn names_full_annotated(&self) -> Vec<String> {
        self.declares
            .iter()
            .map(|d| {
                let name = d.display_name();
                match d.interface_annotation() {
                    Some((class, args)) if args.is_empty() => format!("{name}::{class}"),
                    Some((class, args)) => format!("{name}::{class}({})", args.join(", ")),
                    None => name,
                }
            })
            .collect()
    }

    pub fn get_params_with_defaults(&self) -> Vec<(McIds, String)> {
        self.declares
            .iter()
            .filter_map(|d| d.get_name_with_default())
            .collect()
    }

    /// The same defaults table as [`Self::get_params_with_defaults`], as
    /// condition-evaluator input: each default's lexical family travels
    /// beside its text (U144 residual 3), so a judge can tell `sel = FAST`
    /// from `sel = "FAST"`.
    pub fn get_cond_params_with_defaults(&self) -> Vec<CondParam> {
        self.declares
            .iter()
            .filter_map(|d| d.get_cond_default())
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
/// which routes them into the per-file
/// [`DiagnosticManager`](crate::db::diagnostic::diagnostic::DiagnosticManager).
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
    /// The written default value, as text — `10kΩ`, `X7R`, `FAST`.
    ///
    /// This is the one authority for "does this formal carry a default"
    /// (CIMP U54). A typed form's default is carried by the type kind or by
    /// the declaration's own kind, and [`Self::recorded`] copies it here as
    /// the declaration is assembled; an untyped form's default (`sel = FAST`
    /// with no `::STRING` annotation) is carried by nothing else, so it is
    /// recorded here and nowhere else. `None` means the formal is required.
    ///
    /// Usage-based inference ([`Self::set_param_type`]) may later replace
    /// `param_type` without touching this field: whether the source wrote a
    /// default is a fact about the syntax, not about the inferred type.
    pub default_val: Option<String>,
    /// Whether the written default was a quoted string literal (`sel =
    /// "FAST"`). The text above is stored unquoted — every reader wants the
    /// value — but the condition evaluator compares lexical families
    /// strictly (U144 residual 3), so the family the author wrote survives
    /// here beside the text.
    pub default_quoted: bool,
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

/// The text of a literal default value, with the lexical family it was
/// written in. A string literal records the quoted family (`quoted = true`)
/// while its text loses the quote marks — every consumer wants the value,
/// and the condition side reads the family separately (U144 residual 3).
fn default_from_literal(node: &AstNode) -> Option<(String, bool)> {
    if node.get_type() == MCAST_STRING {
        let raw = node.data_as_cstr()?.to_str().ok()?;
        return Some((strip_string_quotes(raw).to_string(), true));
    }
    node.to_string().map(|text| (text, false))
}

/// A written default and the family it was written in (U144 residual 3).
struct WrittenDefault {
    text: String,
    quoted: bool,
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

        // The written default of a form whose type cannot carry one (CIMP U54).
        let mut written_default: Option<WrittenDefault> = None;

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
                                        return Some(Self::recorded(
                                            McParamDeclareKind::EnumClass(McEnumClassDeclare {
                                                name: name_ids,
                                                class_name,
                                                default_val: Some(value_name),
                                            }),
                                            param_type,
                                            None,
                                        ));
                                    }
                                } else {
                                    // Bare default (no dot): resolve against all known enums.
                                    // e.g., diel = X7R → search all enums for member "X7R".
                                    // Prefer the same-named enum (namespace merging) when
                                    // available.
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
                                        return Some(Self::recorded(
                                            McParamDeclareKind::EnumClass(McEnumClassDeclare {
                                                name: name_ids,
                                                class_name,
                                                default_val: Some(default_str),
                                            }),
                                            param_type,
                                            None,
                                        ));
                                    }
                                }
                            }
                            // Non-enum default: falls through to Single below.
                            // The type cannot hold the value, so the
                            // declaration records it (CIMP U54); dropping it
                            // here is what let an author's default vanish.
                            written_default = Some(WrittenDefault {
                                text: default_str,
                                quoted: false,
                            });
                        } else if let Some((text, quoted)) = default_from_literal(&default_node) {
                            // A literal default of an untyped formal (CIMP U66):
                            // no type carries it, so the declaration does. The
                            // family the literal was written in survives beside
                            // the text (U144 residual 3).
                            written_default = Some(WrittenDefault { text, quoted });
                        }
                    }
                    McParamDeclareKind::Single(name_ids)
                } else {
                    dlog_error(
                        crate::errcodes::PARAM_NAME_INVALID,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::PARAM_NAME_INVALID, &[]),
                    );
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
                    dlog_error(
                        crate::errcodes::PARAM_SET_INVALID,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::PARAM_SET_INVALID, &[]),
                    );
                    return None;
                }
            }
            MCAST_OPD_SQUARE_VEC => {
                // [VDD1, GND1] as operand (e.g. after psnk/in/io).
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
                    dlog_error(
                        crate::errcodes::PARAM_SET_INVALID,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::PARAM_SET_INVALID, &[]),
                    );
                    return None;
                }
            }

            MCAST_OPD => {
                // an operand-shaped child wraps the ids exactly as the
                // bracket-vector form wraps its members above; the declare
                // face reads it as a plain Single.
                let inner = subnode
                    .get_sub_node()
                    .unwrap_or_else(|| subnode.clone());
                if let Some(name_ids) = McIds::new(&inner) {
                    McParamDeclareKind::Single(name_ids)
                } else {
                    dlog_error(
                        crate::errcodes::PARAM_NAME_INVALID,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::PARAM_NAME_INVALID, &[]),
                    );
                    return None;
                }
            }

            MCAST_DECLARE_UV => {
                if let Some(uval) = McUnitValueDeclare::new(&subnode) {
                    McParamDeclareKind::UValue(uval)
                } else {
                    dlog_error(
                        crate::errcodes::PARAM_UVAL_INVALID,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::PARAM_UVAL_INVALID, &[]),
                    );
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
                            return Some(Self::recorded(
                                McParamDeclareKind::EnumClass(McEnumClassDeclare {
                                    name,
                                    class_name,
                                    default_val,
                                }),
                                param_type,
                                None,
                            ));
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

                            // Square-vec names keep their vector structure:
                            // `[V3V3, GND]::DC(3.3V)` declares a Multiple of
                            // two independent McIds, never a single McIds that
                            // renders as the whole `[V3V3, GND]` string. Both
                            // the bare (MCAST_SQUARE_VEC) and the OPD-wrapped
                            // (MCAST_OPD_SQUARE_VEC) forms appear as the
                            // DECLARE instance name. McIds::new already unwraps
                            // MCAST_OPD / MCAST_PARAM wrappers, so the member
                            // iteration below is identical for both shapes.
                            if matches!(
                                ids_node.get_type(),
                                MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC
                            ) {
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
                            crate::errcodes::PARAM_NAME_EXTRACT_FAILED,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::PARAM_NAME_EXTRACT_FAILED,
                                &[],
                            ),
                        );
                        return None;
                    }
                    1 => McParamDeclareKind::Single(inst_ids_list.into_iter().next().unwrap()),
                    _ => McParamDeclareKind::Multiple(inst_ids_list),
                }
            }

            _ => {
                dlog_error(
                    crate::errcodes::PARAM_DECLARE_INVALID,
                    node,
                    &crate::errcodes::format_msg(crate::errcodes::PARAM_DECLARE_INVALID, &[]),
                );
                return None;
            }
        };

        Some(Self::recorded(kind, param_type, written_default))
    }

    // ── Name matching ──

    pub fn match_name(&self, target: &str) -> bool {
        match &self.kind {
            McParamDeclareKind::Role { name, .. } => name.match_name(target),
            McParamDeclareKind::Single(ids) => ids.match_name(target),
            // `[V3V3, GND]::DC(3.3V)` declares a vector formal; each member is
            // an independent formal slot referenced by name inside the func
            // body, so a member name must match its own vector (find() relies
            // on this for member substitution, matching-rules-design.md §6).
            McParamDeclareKind::Multiple(members) => {
                members.iter().any(|ids| ids.match_name(target))
            }
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

    /// Interface class binding for interface-typed params —
    /// `[VDD, GND]::DC(3.3V)` → `("DC", ["3.3V"])`. `None` when the parameter
    /// is not bound to an interface.
    pub fn interface_annotation(&self) -> Option<(String, Vec<String>)> {
        match &self.param_type.kind {
            crate::semantic::basic::mc_param_type::McParamTypeKind::Interface {
                class_name,
                params,
            } => Some((class_name.clone(), params.clone())),
            crate::semantic::basic::mc_param_type::McParamTypeKind::InterfaceWithRole {
                class_name,
                role_val,
                ..
            } => Some((class_name.clone(), vec![role_val.clone()])),
            _ => None,
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

    /// Get the enum class name for an enum-class parameter, if any.
    pub fn get_enum_class(&self) -> Option<&str> {
        match &self.kind {
            McParamDeclareKind::EnumClass(ec) => Some(&ec.class_name),
            _ => None,
        }
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
            crate::semantic::basic::mc_param_type::McParamTypeKind::Interface {
                class_name,
                ..
            }
            | crate::semantic::basic::mc_param_type::McParamTypeKind::InterfaceWithRole {
                class_name,
                ..
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
        self.default_val.is_some()
    }

    /// Assemble a declaration, recording the written default value.
    ///
    /// `written` carries the default of a form whose type cannot hold one:
    /// the bare-identifier form `sel = FAST`, where the type stays `Unknown`
    /// and the value would otherwise be dropped on the floor (CIMP U54). Every
    /// other form goes through [`Self::name_and_default`], the same reading the
    /// declaration's own kind and type always supported.
    fn recorded(
        kind: McParamDeclareKind,
        param_type: McParamType,
        written: Option<WrittenDefault>,
    ) -> Self {
        let mut decl = Self {
            kind,
            param_type,
            default_val: None,
            default_quoted: false,
        };
        match decl.name_and_default() {
            Some((_, dv)) => decl.default_val = Some(dv),
            // The written default only carries the day when no typed or
            // kind-level default exists; its family travels with it.
            None => {
                if let Some(w) = written {
                    decl.default_quoted = w.quoted;
                    decl.default_val = Some(w.text);
                }
            }
        }
        decl
    }

    /// The name and default the KIND or the TYPE carries, before they are
    /// copied onto [`Self::default_val`]. Used only while assembling the
    /// declaration; every later reader goes through `default_val`.
    fn name_and_default(&self) -> Option<(McIds, String)> {
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

    // ── Expansion ──

    pub fn expand(&self) -> Vec<String> {
        match &self.kind {
            McParamDeclareKind::Role { name, .. } => name.expand(),
            McParamDeclareKind::Single(ids) => ids.expand(),
            // A vector formal's member list is its expansion (matching-rules-
            // design.md §6): subst.rs uses it to map member i -> bound lane i.
            McParamDeclareKind::Multiple(members) => {
                members.iter().flat_map(|ids| ids.expand()).collect()
            }
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
        let default = self.default_val.clone()?;

        let name = match &self.kind {
            McParamDeclareKind::Single(ids) => McIds::from(ids.get_primary_name()?.as_str()),
            McParamDeclareKind::UValue(uval) => uval.name.clone(),
            McParamDeclareKind::EnumClass(ec) => ec.name.clone(),
            McParamDeclareKind::Role { name, .. } => name.clone(),
            // A vector formal's members are its expansion, not one value
            // (matching-rules-design.md §6).
            McParamDeclareKind::Multiple(_) => return None,
        };

        Some((name, default))
    }

    /// The name and default as a [`CondParam`] — the condition evaluator's
    /// reading of the defaults table (U144 residual 3). A quoted string
    /// literal default carries the quoted family; every other default is
    /// guessed from the text.
    pub fn get_cond_default(&self) -> Option<CondParam> {
        let (name, text) = self.get_name_with_default()?;
        let family = if self.default_quoted {
            CondFamily::Quoted
        } else {
            crate::semantic::basic::mc_conds::guess_family(&text)
        };
        Some(CondParam { name, text, family })
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
    fn sem_paramd__def_spans_persist_after_port_filter() {
        let mut params = McParamDeclares::new();
        params.store_def_span("rs", 0..2);
        params.store_def_span("dc24v", 10..15);

        assert!(params.def_spans.contains_key("rs"));
        assert!(params.port_spans.contains_key("rs"));

        // Simulate: rs=B3 BareNumeric, dc24v=A1 Label
        params.declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("rs")),
        default_quoted: false,
            param_type: McParamType {
                kind: crate::semantic::basic::mc_param_type::McParamTypeKind::BareNumeric,
                direction: None,
            },
            default_val: None,
        });
        params.declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("dc24v")),
        default_quoted: false,
            param_type: McParamType {
                kind: crate::semantic::basic::mc_param_type::McParamTypeKind::Label,
                direction: None,
            },
            default_val: None,
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
    fn sem_paramd__record_net_ref_uses_def_spans() {
        let mut params = McParamDeclares::new();
        params.store_def_span("rs", 0..2);
        params.declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("rs")),
            param_type: McParamType {
                kind: crate::semantic::basic::mc_param_type::McParamTypeKind::BareNumeric,
                direction: None,
            },
            default_val: None,
            default_quoted: false,
        });
        params.filter_port_spans();

        // Reference should still be recorded via def_spans
        params.record_net_ref(50..52, "rs", "test");
        assert_eq!(params.net_ref_spans.len(), 1);
        assert_eq!(params.net_ref_spans[0].1, "rs");
    }
}
