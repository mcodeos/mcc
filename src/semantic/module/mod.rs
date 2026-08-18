// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::{
    basic::mc_bus::{McBus, McList},
    basic::mc_endpoint::{McEndpoint, McInstanceRef},
    basic::mc_fcall::McFuncCall,
    basic::mc_phrase::McPhrase,
    mc_func::{HasFindInst, McFunctions},
    mc_inst::{McInst, McInstance, McInstances},
};
use crate::db::context::DB;
use crate::db::diagnostic::diagnostic::{dlog_error, Position};
use crate::refdef::types::ChainSegment;
use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};
use crate::semantic::component::Mc2Component;
use crate::semantic::context::resolve_cmie;
use crate::semantic::mc_func::McFuncReturn;
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    semantic::basic::mc_param::McParamDeclares,
    IOType, McCMIE, McIds, McParamValue, McURI,
};
use std::sync::Arc;
// ============================================================================
// McModule - Module definition
// ============================================================================

#[derive(Debug, Clone)]
pub struct McModule {
    pub name: McIds,
    pub params: McParamDeclares,
    pub insts: McInstances,
    pub lines: Vec<McPhrase>,
    /// Source span for each connection line in `lines` (parallel array).
    /// Used for diagnostic position reporting during instantiation.
    pub line_spans: Vec<crate::ast::ast_semantic::Span>,
    pub funcs: McFunctions,
    pub uri: McURI,
    /// Source span for LSP goto-definition (byte range in `uri`).
    pub span: crate::ast::ast_semantic::Span,
    anon_counter: usize,
}

impl McModule {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_MODULE
        // |- MCAST_NAME - MCAST_PARAM (option) - MCAST_BODY
        if let Some(subnodes) = node.get_sub_node() {
            let module_name = subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))
                .and_then(|n| n.get_sub_node())
                .and_then(|n| McIds::new(&n));

            let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) else {
                dlog_error(
                    crate::errcodes::MODULE_MISSING_SUBNODE,
                    node,
                    &crate::errcodes::format_msg(crate::errcodes::MODULE_MISSING_SUBNODE, &[]),
                );
                return None;
            };

            let module_name = module_name?;

            // Span from the module name (MCAST_NAME → MCAST_IDS), not the whole node
            let ids_node = subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))
                .and_then(|n| n.get_sub_node())?;
            let start = ids_node.get_pos() as usize;
            let end = start + ids_node.get_len() as usize;
            let mut module = Self {
                name: module_name,
                params: McParamDeclares::new(),
                funcs: McFunctions::new(),
                insts: McInstances::new(),
                lines: Vec::new(),
                line_spans: Vec::new(),
                uri: uri.clone(),
                span: crate::ast::ast_semantic::Span { start, end },
                anon_counter: 1,
            };

            // 2. Parse parameters
            if let Some(param_node) = subnodes.iter().find(|x| x.is_type(MCAST_PARAMS)) {
                module.parse_params(&param_node);
            }

            // 3. Parse body
            module.parse_body(&body);

            // ★ P4.1 ("Pass1b" hook): the whole body — including all `func`
            // definitions — is now parsed, so every FuncCall's return shape
            // (eval.md §8.1) can be resolved against the complete funcs table.
            // `this`/implicit → caller shape preserved; `return <expr>` → [0|N].
            {
                let lines = std::mem::take(&mut module.lines);
                for mut line in lines {
                    McFuncCall::fill_return_shapes(&mut line, &module);
                    module.lines.push(line);
                }
            }

            Some(module)
        } else {
            dlog_error(
                crate::errcodes::MODULE_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(crate::errcodes::MODULE_MISSING_SUBNODE, &[]),
            );
            None
        }
    }

    /// Test-only stub constructor: builds a minimal module with no parsed
    /// body. Available only under `#[cfg(test)]` so instance-layer scope
    /// unit tests can construct a [`McModuleInst`] without an AST.
    #[cfg(test)]
    pub fn test_stub(name: &str) -> Self {
        Self {
            name: McIds::from(name),
            params: McParamDeclares::new(),
            insts: McInstances::new(),
            lines: Vec::new(),
            line_spans: Vec::new(),
            funcs: McFunctions::new(),
            uri: McURI::default(),
            span: crate::ast::ast_semantic::Span {
                start: 0,
                end: name.len(),
            },
            anon_counter: 1,
        }
    }

    pub(crate) fn parse_params(&mut self, decl_node: &AstNode) {
        // Parameters divided into 2 categories: data and inst, each parsed separately
        // MCAST_PARAMS
        //   |- MCAST_PARAM
        //      |- MCAST_ROLE               : parse as params: McParamDeclares
        //      |- MCAST_IDS                : parse as params: McParamDeclares
        //      |- MCAST_SQUARE_VEC         : parse as params: McParamDeclares
        //      |- MCAST_DECLARE_UV         : parse as params: McParamDeclares

        //      |- MCAST_OPD                : parse as insts: McInstances
        //      |- MCAST_OPD_SQUARE_VEC     : parse as insts: McInstances
        //      |- MCAST_DECLARE            : parse as insts: McInstances

        if let Some(subnodes) = decl_node.get_sub_node() {
            for param_node in subnodes.iter() {
                // Each MCAST_PARAM child node determines its type
                let Some(subnode) = param_node.get_sub_node() else {
                    continue;
                };

                match subnode.get_type() {
                    // Data parameter -> params
                    MCAST_ROLE | MCAST_IDS | MCAST_SQUARE_VEC | MCAST_DECLARE_UV => {
                        self.params.parse(&param_node);
                    }
                    // Reference parameter -> insts (treated as port)
                    MCAST_OPD | MCAST_OPD_SQUARE_VEC => {
                        self.insts.parse(&subnode, &self.uri);
                    }
                    // Instance parameter -> insts, or enum-class/interface data param
                    MCAST_DECLARE => {
                        // Check if CLASS is an enum → data param (B5/B6)
                        let is_enum = McParamType::extract_class_name_from_declare(&subnode)
                            .map(|cn| crate::db::cmie::cmie::is_enum_class_name(&cn))
                            .unwrap_or(false);
                        // Check if CLASS is an interface → port param (A3/A4)
                        // e.g., USB_VBUS_1{VDD_3V, GND}::DC(3.3V) has name prefix
                        // and is parsed as MCAST_DECLARE (not MCAST_SQUARE_VEC).
                        let pt = McParamType::from_ast(&subnode);
                        let is_interface = matches!(
                            pt.kind,
                            McParamTypeKind::Interface { .. }
                                | McParamTypeKind::InterfaceWithRole { .. }
                        );
                        if is_enum || is_interface {
                            self.params.parse(&param_node);
                            // ★ LSP: register the interface class ref of a
                            // module port (`[VDD_3V3,GND]::DC(3.3V)` → `DC`) so
                            // goto-def lands on the library `interface DC`
                            // definition (same path as component pin ::ifaces).
                            if is_interface {
                                if let Some((class_name, class_span)) =
                                    Self::extract_declare_class_span(&subnode)
                                {
                                    tracing::info!(target: "mcc::lsp::audit",
                                        "[AUDIT-ModulePort-Iface] class={class_name} span={class_span:?} uri={}",
                                        self.uri);
                                    crate::query::refs::mcb_register_declare_class(
                                        &self.uri,
                                        &class_name,
                                        class_span,
                                    );
                                } else {
                                    tracing::info!(target: "mcc::lsp::audit",
                                        "[AUDIT-ModulePort-Iface] extract failed, subnode_type={}",
                                        subnode.get_type());
                                }
                                // ★ LSP: curly interface params
                                // (`dc{VDD_3V3, GND}::DC(3.3V)`) register a
                                // BusDef with declaration-site member spans so
                                // `dc.GND` / `dc.VDD_3V3` resolve via
                                // bus_member_hit to the member text in THIS
                                // file instead of a use-site span.
                                Self::register_curly_param_bus_def(&subnode, &mut self.insts);
                            }
                        } else {
                            self.insts.parse(&subnode, &self.uri);
                        }
                    }
                    // IOTYPE-prefix parameter -> insts + params (e.g. ps dc24v, in GPIO[1:2])
                    MCAST_IOTYPE => {
                        self.insts.parse(&param_node, &self.uri);
                        self.params.parse(&param_node); // also register for unused detection
                    }
                    _ => {
                        // Unknown type, try to parse as data parameter
                        dlog_error(
                            crate::errcodes::MODULE_PARAM_TYPE_UNEXPECTED,
                            &subnode,
                            &crate::errcodes::format_msg(
                                crate::errcodes::MODULE_PARAM_TYPE_UNEXPECTED,
                                &[],
                            ),
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn parse_body(&mut self, body: &AstNode) {
        // ★ LSP: Set scope for instance registration
        self.insts.scope = Some(self.name.to_string());
        if let Some(clauses) = body.get_sub_node() {
            for clause in clauses.iter() {
                let ct = clause.get_type();
                match ct {
                    MCAST_NET_PORTS => {
                        self.insts.parse(&clause, &self.uri);
                    }

                    MCAST_NET => {
                        if let Some(subnode) = clause.get_sub_node() {
                            if subnode.get_type() == MCAST_DECLARE {
                                // ★ LSP: instance declarations also reference their ctor args (`speaker(V3V3)`) — record
                                // them like MCAST_NET operands so F12 works.
                                self.collect_declare_ctor_refs(&subnode);
                                self.insts.parse(&subnode, &self.uri);
                                continue;
                            }
                            // Collect port reference spans before parsing the net
                            let scope = self.name.to_string();
                            Self::collect_net_refs_in_node(
                                &subnode,
                                &mut self.insts,
                                &mut self.params,
                                &scope,
                            );
                            match McPhrase::new(&subnode, self) {
                                Some(net) => {
                                    // Store definition spans + LSP lapper entries for inline ports
                                    Self::collect_net_def_spans(
                                        &subnode,
                                        &mut self.insts,
                                        &self.uri,
                                        &self.name.to_string(),
                                    );
                                    // Track source span for diagnostic position reporting
                                    let line_start = subnode.get_pos() as usize;
                                    let line_end = line_start + subnode.get_len() as usize;
                                    self.line_spans.push(crate::ast::ast_semantic::Span {
                                        start: line_start,
                                        end: line_end,
                                    });
                                    self.lines.push(net);
                                }
                                None => {
                                    dlog_error(
                                        crate::errcodes::CONN_LINE_PARSE_FAILED,
                                        &clause,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::CONN_LINE_PARSE_FAILED,
                                            &[],
                                        ),
                                    );
                                }
                            }
                        } else {
                            dlog_error(
                                crate::errcodes::FUNC_EMPTY_NET,
                                &clause,
                                &crate::errcodes::format_msg(crate::errcodes::FUNC_EMPTY_NET, &[]),
                            );
                        }
                    }

                    MCAST_FUNCTION => {
                        let context = unsafe { &mut *(self as *mut McModule) };
                        // ★ LSP: register interface class refs from the func
                        // header (`func power(V3V3::DC(3.3V))` → `DC`) so
                        // goto-def / hover resolve them (same path as module
                        // ports). Must run before create_lapper consumes
                        // declare_class_refs.
                        crate::query::refs::register_func_header_iface_refs(&clause, &self.uri);
                        self.funcs.parse(&clause, context);
                    }

                    MCAST_DECLARE => {
                        // ★ LSP: record ctor-arg refs for goto-def (see collect_declare_ctor_refs).
                        self.collect_declare_ctor_refs(&clause);
                        self.insts.parse(&clause, &self.uri);
                    }

                    MCAST_ROLE => {
                        dlog_error(
                            crate::errcodes::MODULE_ROLE_UNSUPPORTED,
                            &clause,
                            &crate::errcodes::format_msg(
                                crate::errcodes::MODULE_ROLE_UNSUPPORTED,
                                &[],
                            ),
                        );
                    }
                    MCAST_ATTRIBUTE_PIN | MCAST_ATTRIBUTE_PINADD => {
                        dlog_error(
                            crate::errcodes::MODULE_PINS_UNSUPPORTED,
                            &clause,
                            &crate::errcodes::format_msg(
                                crate::errcodes::MODULE_PINS_UNSUPPORTED,
                                &[],
                            ),
                        );
                    }
                    _ => {
                        dlog_error(
                            crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                            &clause,
                            &crate::errcodes::format_msg(
                                crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                                &[],
                            ),
                        );
                    }
                }
            }

            // ★ Smart Param (M5): Check both formal params and body ports.
            let mod_name = self.name.to_string();
            let diags = self.params.finalize(Some(body), &mod_name);
            let mut warned: std::collections::HashSet<String> =
                diags.iter().map(|d| d.param_name.clone()).collect();
            // finalize names params by their declared form (e.g. "GPIO[1:2]",
            // "DC1{VDD, GND}", "[VDD1, GND1]") while the instance table below
            // uses normalized keys ("GPIO1", "DC1", "@3"); fold every warned
            // declare's name forms into the set so the sweep below does not
            // re-report the same port (E5641 + E5642 duplicates).
            for declare in self.params.iter() {
                if warned.contains(&declare.display_name()) {
                    warned.extend(declare.all_name_forms());
                }
            }
            for d in diags {
                crate::mcc_log_global_diag(&d);
            }
            for port_name in self.insts.iter_port_names() {
                let all_forms = self.insts.all_name_forms_for(port_name);
                if all_forms.iter().any(|form| warned.contains(form)) {
                    continue;
                }
                let mut span = self
                    .insts
                    .port_spans()
                    .get(port_name)
                    .and_then(|s| s.first().cloned())
                    .unwrap_or(0..1);
                // Fallback for IDX members stored individually without span:
                // try base name (strip trailing digits) in port_spans.
                if span.start == 0 && span.end == 1 {
                    // Try sibling labels with same base (GPIO2 → find GPIO1's span)
                    if let Some(base) = McInstances::strip_trailing_digits(port_name) {
                        for other in self.insts.iter_port_names() {
                            if other == port_name {
                                continue;
                            }
                            if let Some(other_base) = McInstances::strip_trailing_digits(other) {
                                if other_base == base {
                                    if let Some(s) =
                                        self.insts.port_spans().get(other).and_then(|v| v.first())
                                    {
                                        span = s.clone();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                let has_recorded_ref = self
                    .insts
                    .iter_net_refs()
                    .any(|(_, name, _)| all_forms.iter().any(|form| form == name));
                let has_ast_usage = all_forms.iter().any(|form| {
                    crate::semantic::basic::mc_param_infer::collect_usages(form, body)
                        .iter()
                        .any(|usage| usage.pos != span.start)
                });
                if !has_recorded_ref && !has_ast_usage {
                    crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::PORT_NEVER_USED,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                        span.start as u32,
                        (span.end - span.start) as u32,
                        &crate::errcodes::format_msg(
                            crate::errcodes::PORT_NEVER_USED,
                            &[&port_name, &mod_name],
                        ),
                        &[],
                    );
                }
            }

            // ★ Inline labels: register bare names referenced in net lines that
            // are not ports/params/instances as Inline labels, so `show
            // instances` lists them (e.g. `GND` in `... -> GND`).
            let mut net_labels: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for line in &self.lines {
                crate::semantic::validation::body::collect_net_label_names(line, &mut net_labels);
            }
            for name in net_labels {
                if !Self::is_plain_label_candidate(&name) {
                    continue;
                }
                if self.insts.contains(&name) || self.params.contains(&name) {
                    continue;
                }
                self.insts
                    .create_inst(&name, McInstance::Label(name.clone()));
                self.insts
                    .set_label_kind(&name, crate::semantic::mc_inst::LabelKind::Inline);
            }
        }
    }

    /// ★ LSP: Record net refs for instance-declaration constructor arguments
    /// (`speaker(V3V3)`, `mcu(V3V3, V1V2)`). Instance declarations only walk
    /// `insts.parse` → `parse_declare`, which binds ctor args but never records
    /// lapper refs — unlike MCAST_NET operands (`collect_net_refs_in_node`).
    /// Route each `MCAST_PARAM` under every `MCAST_INSTANCE` through the same
    /// collector so argument identifiers get a LabelRef/PortRef and F12 finds
    /// their def (consistent with the MCAST_NET path).
    fn collect_declare_ctor_refs(&mut self, clause: &AstNode) {
        let scope = self.name.to_string();
        let Some(sub) = clause.get_sub_node() else {
            return;
        };
        for child in sub.iter() {
            if child.get_type() != MCAST_INSTANCE {
                continue;
            }
            // The ctor PARAMS node is the next sibling of the instance id node
            // (or of the instance node when the id has no sub) — mirror
            // collect_ctor_params in mc_inst.rs.
            let inst_id = child.get_sub_node().unwrap_or_else(|| child.clone());
            for cand in [inst_id.get_next(), child.get_next()] {
                let Some(n) = cand else { continue };
                if n.get_type() != MCAST_PARAMS {
                    continue;
                }
                if let Some(psub) = n.get_sub_node() {
                    for p in psub.iter() {
                        if p.get_type() == MCAST_PARAM {
                            Self::collect_net_refs_in_node(
                                &p,
                                &mut self.insts,
                                &mut self.params,
                                &scope,
                            );
                        }
                    }
                }
                break;
            }
        }
    }

    /// A bare identifier eligible to become an inline net label: no member
    /// separators (`.`/`{`), not an anonymous or bracketed name, not a
    /// reserved keyword.
    fn is_plain_label_candidate(name: &str) -> bool {
        if name.is_empty()
            || name == "this"
            || name == "lead"
            || name.starts_with('@')
            || name.starts_with('[')
            || name.starts_with('(')
            || name.contains('.')
            || name.contains('{')
            || name.contains('(')
            || name.contains(',')
            || name.contains(char::is_whitespace)
        {
            return false;
        }
        true
    }
    /// Extract the interface class name and its source span from an
    /// interface-typed module port parameter, e.g. `[VDD_3V3,GND]::DC(3.3V)`
    /// → (`McIds(DC)`, <span of "DC">). Mirrors `McParamType::classify_declare`.
    /// The `McIds` is taken directly from the AST node so the multi-segment
    /// structure is preserved for downstream registration / resolution.
    pub(crate) fn extract_declare_class_span(
        node: &AstNode,
    ) -> Option<(McIds, std::ops::Range<usize>)> {
        let first_child = node.get_sub_node()?;
        for child in first_child.iter() {
            if child.get_type() != MCAST_CLASS {
                continue;
            }
            let Some(name_node) = child.get_sub_node() else {
                continue;
            };
            // The class-name IDS can over-span the real name when the declare
            // carries ctor args in the func-header grammar (`::DC(3.3V)` yields
            // an IDS covering `DC(3.3V)` with only `DC` as a child), so compute
            // the span from the name-constituent children (ID/IDA/dot members)
            // and skip the ctor-arg container (MCAST_PARAMS). Falls back to the
            // IDS node's own span for leaf IDS nodes (plain names).
            let mut span_start: Option<Position> = None;
            let mut span_end: Option<Position> = None;
            let mut cur = name_node.get_sub_node();
            while let Some(n) = cur {
                if n.get_type() != MCAST_PARAMS {
                    if span_start.is_none() {
                        span_start = Some(n.get_pos());
                    }
                    span_end = Some(n.get_pos() + n.get_len());
                }
                cur = n.get_next();
            }
            let span = match (span_start, span_end) {
                (Some(s), Some(e)) => (s as usize)..(e as usize),
                _ => {
                    (name_node.get_pos() as usize)
                        ..((name_node.get_pos() + name_node.get_len()) as usize)
                }
            };
            if let Some(ids) = McIds::new(&name_node) {
                return Some((ids, span));
            }
        }
        None
    }

    /// ★ LSP: register a BusDef for curly interface module params such as
    /// `dc{VDD_3V3, GND}::DC(3.3V)`. The whole span covers the base identifier
    /// and each member span points at the member text, so member refs
    /// (`dc.GND`, `dc.VDD_3V3`) resolve to the declaration in THIS file via
    /// `bus_member_hit` rather than a first-use site span.
    fn register_curly_param_bus_def(node: &AstNode, insts: &mut McInstances) {
        // MCAST_DECLARE → MCAST_INSTANCE → MCAST_OPD → MCAST_IDS[base, opd_curly[...]]
        let Some(sub) = node.get_sub_node() else {
            return;
        };
        let mut cur = sub;
        let ids_node = loop {
            if cur.get_type() == MCAST_INSTANCE {
                let mut inner = cur.get_sub_node();
                let ids = loop {
                    match inner {
                        Some(n) if n.get_type() == MCAST_IDS => break n,
                        Some(n) => inner = n.get_sub_node(),
                        None => return,
                    }
                };
                break ids;
            }
            match cur.get_next() {
                Some(nx) => cur = nx,
                None => return,
            }
        };
        let Some((busname, members)) = McIds::new(&ids_node).and_then(|ids| ids.as_bus()) else {
            return;
        };
        if members.is_empty() {
            return;
        }
        let whole_span = ids_node
            .get_sub_node()
            .filter(|n| n.get_type() == MCAST_ID)
            .map(|n| {
                let p = n.get_pos() as usize;
                p..(p + busname.len())
            })
            .unwrap_or_else(|| {
                let p = ids_node.get_pos() as usize;
                p..(p + busname.len())
            });
        let mut member_spans: Vec<(String, std::ops::Range<usize>)> = Vec::new();
        let mut mcur = ids_node.get_sub_node();
        while let Some(child) = mcur {
            if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                let mut mc = child.get_sub_node();
                while let Some(m) = mc {
                    if let Some(mname) = m.to_string() {
                        let mstart = m.get_pos() as usize;
                        let mlen = mname.len();
                        member_spans.push((mname, mstart..(mstart + mlen)));
                    }
                    mc = m.get_next();
                }
            }
            mcur = child.get_next();
        }
        if !member_spans.is_empty() {
            tracing::info!(target: "mcc::lsp::audit",
                "[AUDIT-ParamBusDef] bus={busname} span={whole_span:?} members={member_spans:?}");
            insts.register_bus_def(&busname, whole_span, member_spans);
        }
        // Also register the actual bus instance with the FULL member set. Without
        // it the first member ref in a net line (`dc.GND`) auto-creates a
        // single-member bus via `McPhrase::add_bus`, so `show` lists only the
        // referenced member instead of the declared `dc{VDD_3V3, GND}`.
        insts.create_inst(
            &busname,
            McInstance::Bus(McBus::new_with_members(&busname, members)),
        );
    }

    pub(crate) fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.insts.get(id).cloned()
    }

    /// Add label to symbol table
    /// If instance exists, return reference to existing instance
    /// If not found, check members in anonymous List/Bus
    pub(crate) fn add_label(&mut self, name: String) -> McPhrase {
        if let Some(existing_inst) = self.insts.get(&name) {
            return McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                existing_inst.clone(),
            )));
        }
        if let Some(member_ref) = self.find_member_in_anon_insts(&name) {
            return member_ref;
        }
        self.insts
            .create_inst(&name, McInstance::Label(name.clone()));
        self.insts
            .set_label_kind(&name, crate::semantic::mc_inst::LabelKind::Inline);
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
            name,
        ))))
    }

    /// Find member in anonymous List/Bus/Interface
    /// Anonymous instance: name starts with @, or [member1, member2] format (no total name)
    fn find_member_in_anon_insts(&self, member_name: &str) -> Option<McPhrase> {
        for (inst_name, inst) in self.insts.iter() {
            let is_anon = inst_name.starts_with('@')
                || (inst_name.starts_with('[') && inst_name.contains(','));
            if !is_anon {
                continue;
            }
            match inst {
                McInstance::List(list) => {
                    if list.member.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                McInstance::Bus(bus) => {
                    if bus.full_members.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                McInstance::Interface(iface) => {
                    if iface.base.pins.names_to_id.contains_key(member_name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                    let iface_members = iface.name.expand();
                    if iface_members.len() > 1 && iface_members.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Add component instance to symbol table
    pub(crate) fn add_component(&mut self, name: String, comp: Mc2Component) -> McPhrase {
        let inst = McInstance::Component(Arc::new(comp));
        // ── P2-10: anonymous components (names starting with @) are created
        // inline in connection lines. They must NOT be stored in insts,
        // otherwise instantiate_declarations_resilient will create them as
        // declarations with no connections, duplicating the line-created ones.
        if !name.starts_with('@') {
            self.insts.create_inst(&name, inst.clone());
        }
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Add module instance to symbol table
    pub(crate) fn add_module(&mut self, name: String, module: Mc2Module) -> McPhrase {
        let inst = McInstance::Module(Arc::new(module));
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Add bus to symbol table
    pub(crate) fn add_bus(&mut self, name: String, members: Vec<String>) -> McPhrase {
        let bus = McBus::new_with_members(&name, members);
        let inst = McInstance::Bus(bus);
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Add list to symbol table
    pub(crate) fn add_list(&mut self, name: String, members: Vec<String>) -> McPhrase {
        let list = McList::new_with_members(&name, members);
        let inst = McInstance::List(list);
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Get all input ports' McBus
    pub fn get_input_elements(&self) -> Vec<McBus> {
        self.insts
            .get_all_inputs()
            .iter()
            .map(|p| p.to_node_element())
            .collect()
    }

    /// Get all output ports' McBus
    pub fn get_output_elements(&self) -> Vec<McBus> {
        self.insts
            .get_all_outputs()
            .iter()
            .map(|p| p.to_node_element())
            .collect()
    }
}

impl HasFindInst for McModule {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance> {
        self.insts.get_mut(id)
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        // P2 container category chain (§3.3): param ports → param defs →
        // ports → labels → non-port insts (uniform Bus/List/Interface/
        // Component coverage) → funcs. Each category is an independent scope
        // unit in semantic::scope with the same hit logic (and stored spans)
        // as the original hand-written chain it replaced.
        crate::semantic::scope::module_scope(self)
            .resolve(id)
            .map(|r| (r.inst, r.span))
    }

    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        if let Some(s) = span {
            self.insts.store_port_span(&name, s);
        }
        Some(self.add_label(name))
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let bus = McBus::new_with_members(&name, members);
        let inst = McInstance::Bus(bus);
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let list = McList::new_with_members(&name, members);
        let inst = McInstance::List(list);
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase> {
        let is_component_with_bus = self
            .insts
            .get(base)
            .map(|inst| {
                if let McInstance::Component(comp) = inst {
                    comp.base.pins.is_bus(&member)
                } else {
                    false
                }
            })
            .unwrap_or(false);

        if is_component_with_bus {
            let full_name = format!("{base}.{member}");
            if !self.insts.contains(&full_name) {
                let members = if let Some(inst) = self.insts.get(base) {
                    if let McInstance::Component(comp) = inst {
                        comp.base.pins.get_bus_members(&member).unwrap_or_default()
                    } else {
                        vec![member.clone()]
                    }
                } else {
                    vec![member.clone()]
                };
                let mut new_bus = McBus::new_with_members(&full_name, members);
                new_bus.add_member(&member);
                self.insts.create_inst(&full_name, McInstance::Bus(new_bus));
            } else if let Some(existing_inst) = self.insts.get_mut(&full_name) {
                if let McInstance::Bus(bus) = existing_inst {
                    if !bus.full_members.iter().any(|m| m == &member) {
                        bus.add_member(&member);
                    }
                }
            }
            let member_ref = McBus::member_ref(&full_name, member);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(member_ref),
            ))));
        }

        if let Some(inst) = self.insts.get_mut(base) {
            if let McInstance::Bus(bus) = inst {
                let fn_base = base.to_string();
                bus.add_member(&member);
                let full_members_clone = bus.full_members.clone();
                if !self.insts.contains(&fn_base) {
                    let bus_to_add = McBus::new_with_members(&fn_base, full_members_clone);
                    self.insts
                        .create_inst(&fn_base, McInstance::Bus(bus_to_add));
                }
                let member_ref = McBus::member_ref(&fn_base, member);
                return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::Bus(member_ref),
                ))));
            }
        }

        let bus = McBus::new_with_members(base, vec![member.clone()]);
        let inst = McInstance::Bus(bus);
        self.insts.create_inst(base, inst.clone());
        let member_ref = McBus::member_ref(base, member);
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Bus(member_ref),
        ))))
    }

    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase> {
        let full_name = format!("{component}.{interface}");
        if let Some(comp_inst) = self.insts.get(component) {
            if let McInstance::Component(comp) = comp_inst {
                if comp.base.pins.is_interface(interface) {
                    let iface_ref = McBus::new_with_members(&full_name, members);
                    return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(iface_ref),
                    ))));
                }
            }
        }
        if let Some(McCMIE::Interface(_)) = resolve_cmie(&DB, &McIds::from("ADC.DIFF"), self.uri())
        {
            let iface_ref = McBus::new_with_members(&full_name, members);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(iface_ref),
            ))));
        }
        if let Some(McCMIE::Interface(_)) = resolve_cmie(
            &DB,
            &McIds::from(&format!("{component}.{interface}") as &str),
            self.uri(),
        ) {
            let iface_ref = McBus::new_with_members(&full_name, members);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(iface_ref),
            ))));
        }
        None
    }

    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)> {
        if let Some(inst) = self.insts.get(base) {
            if let McInstance::Component(comp) = inst {
                if comp.base.pins.is_bus(member) {
                    return Some((format!("{base}.{member}"), member.to_string()));
                }
            }
        }
        None
    }

    fn is_component_bus(&self, base: &str, member: &str) -> bool {
        if let Some(inst) = self.insts.get(base) {
            if let McInstance::Component(comp) = inst {
                return comp.base.pins.is_bus(member);
            }
        }
        false
    }

    fn uri(&self) -> &McURI {
        &self.uri
    }

    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        let before: Vec<String> = self.insts.get_all_names();
        self.insts.parse(node, &self.uri);
        // Collect newly created instances to return to callers (mc_phrase.rs, mc_fcall.rs)
        self.insts
            .get_all_names()
            .into_iter()
            .filter(|k| !before.contains(k))
            .filter_map(|k| self.insts.get(&k).cloned())
            .collect()
    }

    fn add_component(
        &mut self,
        name: String,
        comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase> {
        Some(self.add_component(name, comp))
    }

    fn add_module(
        &mut self,
        name: String,
        module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        Some(self.add_module(name, module))
    }

    /// Generate an anonymous instance name: `@{classname}{counter}` (e.g. `@RES1`, `@CAP2`).
    ///
    /// # Design rule
    /// Anonymous instances are created inline in connection statements
    /// (e.g. `-> RES(10kΩ) ->`). Their declaration position **is** their usage
    /// position — they exist solely as part of a connection chain and do not
    /// need to be referenced from elsewhere.
    ///
    /// Diagnostics that check for "unused" ports/instances must skip names
    /// produced by this function. See [`McInstances::iter_port_names`].
    fn gen_anon_name(&mut self, classname: &str) -> String {
        let name = format!("@{}{}", classname, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn store_inst_span(&mut self, name: &str, span: std::ops::Range<usize>) {
        self.insts.store_port_span(name, span);
    }

    fn record_declareb_def(
        &mut self,
        name: &str,
        kind: crate::refdef::types::SymbolKind,
        span: std::ops::Range<usize>,
    ) {
        self.insts.record_declareb_def(name, kind, span);
    }

    fn upgrade_label_to_bus(&mut self, name: &str) -> bool {
        if let Some(inst) = self.insts.get_mut(name) {
            if matches!(inst, McInstance::Label(_)) {
                let new_bus = McBus::new(name);
                *inst = McInstance::Bus(new_bus);
                return true;
            }
        }
        false
    }

    fn find_func_return(&self, name: &str) -> Option<McFuncReturn> {
        self.funcs.find(name).map(|f| f.returns.clone())
    }

    fn scope_name(&self) -> Option<String> {
        Some(self.name.to_string())
    }
}

impl McModule {
    /// Recursively scan AST nodes in a net expression for identifiers that match
    /// known port names (both from body insts and params), and record their spans for LSP goto-definition.
    /// Walk AST nodes in a net phrase and store definition spans + LSP lapper
    /// entries for any identifier that becomes an inline port instance.
    fn collect_net_def_spans(node: &AstNode, insts: &mut McInstances, uri: &McURI, scope: &str) {
        match node.get_type() {
            MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC
            | MCAST_OPD_CURLY => {
                if let Some(text) = node.to_string() {
                    // ★ §3.4.3 (rev) check-before-register: member chains
                    // (`USB_VBUS_1.GND`, `dc.VDD_3V3`) are REFS to members of an
                    // already-declared bus; the member defs were registered at the
                    // declaration site (module param / io line) via register_bus_def
                    // → BusMemberDef. Skipping them here prevents the whole-chain
                    // span from being stored as the base bus's port span — which
                    // would register spurious BusDef/LabelDef at the use site and
                    // make F12 on the member self-locate (def == ref span).
                    //
                    // Two shapes slip through a plain dotted-text check:
                    //   - the chain node itself (`USB_VBUS_1.GND`); and
                    //   - its first MCAST_ID segment (`USB_VBUS_1`), whose
                    //     `get_len()` is extended to the whole chain by
                    //     mc_value_link (§5.1: never trust get_len() for ids
                    //     chains) while `to_string()` returns only the segment.
                    // `node_len > text.len()` detects the latter.
                    let node_len = node.get_len() as usize;
                    let is_member_chain = text.contains('.') || node_len > text.len();
                    if is_member_chain {
                        // member-chain ref: def already exists at declaration site
                    } else {
                        let start = node.get_pos() as usize;
                        let span = start..(start + node_len);
                        let key = insts.resolve_idx(&text).unwrap_or(text);
                        if insts.get(&key).is_some() && insts.port_spans().get(&key).is_none() {
                            insts.store_port_span(&key, span.clone());
                            // Register in name_to_declare_id so goto-def can find this inline port
                            if let Some(mcode) = crate::db::cmie::tables::WORKSPACE.mcodes.get(uri)
                            {
                                if let Ok(mut sem) = mcode.symbols.lock() {
                                    sem.local_table.add_declare_with_name(
                                        uri,
                                        crate::ast::ast_semantic::SourceLocation::from_span(&span),
                                        Some(key),
                                        Some(scope),
                                    );
                                }
                            }
                        } // end else (non-member-chain def registration)
                    }
                }
            }
            _ => {}
        }
        if let Some(sub) = node.get_sub_node() {
            let mut cur = sub;
            loop {
                Self::collect_net_def_spans(&cur, insts, uri, scope);
                match cur.get_next() {
                    Some(next) => cur = next,
                    None => break,
                }
            }
        }
    }

    pub(crate) fn collect_net_refs_in_node(
        node: &AstNode,
        insts: &mut McInstances,
        params: &mut McParamDeclares,
        scope: &str,
    ) {
        let handled = match node.get_type() {
            MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN => {
                Self::record_scoped_net_ref(node, insts, params, scope);
                true
            }
            // ★ SQUARE_VEC / OPD_SQUARE_VEC (e.g. [VDD_3V3,GND]):
            //   text starts with `[` so split-by-`[` gives empty base.
            //   Iterate members and look up each individually — matching how
            //   McParamDeclares::parse stores them as individual keys in def_spans.
            MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC => {
                tracing::info!(target: "mcc::lsp",
                    "SQUARE_VEC_REF node_type={} pos={} len={}",
                    node.get_type(),
                    node.get_pos(),
                    node.get_len()
                );
                let mut current = node.get_sub_node();
                while let Some(phrase_node) = current {
                    let ids_node = phrase_node
                        .get_sub_node()
                        .unwrap_or_else(|| phrase_node.clone());
                    let mut handled = false;
                    if let Some(ids) = crate::semantic::basic::mc_ids::McIds::new(&ids_node) {
                        let name = ids.to_string();
                        let member_span = (ids_node.get_pos() as usize)
                            ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                        // ★ Dot-chain members (`dc.VDD_3V3`, `lpa.IN.N`) must
                        // NOT be folded into their base key here — resolve_idx
                        // would map them to `dc`/`lpa` and lose the member
                        // context. Route them through the chain path below.
                        let is_chain = name.contains('.');
                        if !is_chain {
                            // Resolve the member to its owning port (e.g. `VDD_3V3`
                            // inside `[VDD_3V3, GND]::DC(3.3V)` maps to the bracket
                            // key) so usage of bracket members counts as usage of
                            // the whole bracket port.
                            let matched_key = insts.resolve_idx(&name);
                            let in_params = params.is_defined(&name);
                            tracing::info!(
                                "SQUARE_VEC_REF member='{name}' span=[{},{}] key={:?} in_params={in_params} scope='{scope}'",
                                member_span.start, member_span.end, matched_key
                            );
                            if let Some(key) = matched_key {
                                insts.record_net_ref(member_span, &key, scope);
                                handled = true;
                            } else if in_params {
                                params.record_net_ref(member_span, &name, scope);
                                handled = true;
                            }
                        }
                    }
                    if !handled {
                        // ★ Unmatched / chain members (e.g. `dc.VDD_3V3`,
                        // `RES(..) -> (lpa.VO1 + spk.N)`) were previously
                        // dropped entirely — recurse so the chain path
                        // (has_dot_chain → try_record_chain_ref) registers
                        // them instead. Walk next-siblings too: a member may
                        // be a full connection (`dc.VDD_3V3 -> wm7121.VCC`)
                        // whose arrow's right operand hangs off the left
                        // operand's get_next() — without the walk it is
                        // swallowed and never registered as a pin ref.
                        let mut cur = Some(ids_node.clone());
                        while let Some(c) = cur {
                            Self::collect_net_refs_in_node(&c, insts, params, scope);
                            cur = c.get_next();
                        }
                    }
                    current = phrase_node.get_next();
                }
                true
            }
            MCAST_OPD => {
                // If this OPD contains dot separators between identifiers
                // (e.g., `uC.i2c(0x36).I2C0`), return false so that
                // try_record_chain_ref handles it with AST-structured segments
                // instead of falling through to simple name recording.
                if Self::has_dot_chain(node) {
                    false
                } else if let Some(sub) = node.get_sub_node() {
                    let inner_type = sub.get_type();
                    if matches!(
                        inner_type,
                        MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN
                    ) {
                        Self::record_scoped_net_ref(&sub, insts, params, scope);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        };

        if !handled {
            // ★ Chain detection: an MCAST_OPD / MCAST_OPD_DOT with dotted
            // segments like `uC.i2c(0x36).I2C0` (root is MCAST_OPD_DOT whose
            // sub is the fcall and next is the member). Record the full chain
            // as a net-ref so the chain resolver can find the cross-container
            // member def (e.g., the MCU pin I2C0::I2C(Master) rather than the
            // local module port I2C0).
            if matches!(node.get_type(), MCAST_OPD | MCAST_OPD_DOT)
                && Self::try_record_chain_ref(node, insts, scope)
            {
                return;
            }
            let Some(sub) = node.get_sub_node() else {
                return;
            };
            let mut current = sub;
            loop {
                Self::collect_net_refs_in_node(&current, insts, params, scope);
                match current.get_next() {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
    }

    /// Check whether an MCAST_OPD node contains dot-separated identifiers
    /// (i.e., it's a member-chain expression like `uC.i2c(0x36).I2C0`).
    /// Returns `true` if an MCAST_OPD_DOT appears as a flat child, inside a
    /// nested MCAST_OPD (chain-tail operands), or merged into an ID/IDA/IDS
    /// sub_node (`uC.ADC{P,N}` is one MCAST_IDS with a dotted sub_node).
    fn has_dot_chain(node: &AstNode) -> bool {
        let mut current = node.get_sub_node();
        while let Some(n) = current {
            if n.get_type() == MCAST_OPD_DOT {
                return true;
            }
            // Chain-tail operands wrap their children in a nested MCAST_OPD
            // (e.g. `MIC -> uC.ADC{P,N}` — the tail is nested), so recurse.
            if n.get_type() == MCAST_OPD && Self::has_dot_chain(&n) {
                return true;
            }
            // ID/IDA/IDS may merge the dot chain into their sub_node:
            // `uC.ADC{P,N}` is one MCAST_IDS whose sub_node is
            // [MCAST_ID "uC", MCAST_OPD_DOT "ADC", MCAST_OPD_CURLY "P{N}"].
            if matches!(n.get_type(), MCAST_ID | MCAST_IDA | MCAST_IDS) {
                if let Some(sub) = n.get_sub_node() {
                    let mut sc = sub;
                    loop {
                        if sc.get_type() == MCAST_OPD_DOT {
                            return true;
                        }
                        match sc.get_next() {
                            Some(nx) => sc = nx,
                            None => break,
                        }
                    }
                }
            }
            current = n.get_next();
        }
        false
    }

    /// Walk the AST children of a chain expression (MCAST_OPD / MCAST_OPD_DOT)
    /// and extract structured [`ChainSegment`]s. Records the chain via
    /// [`McInstances::record_chain_ref`] so the chain resolver can use the
    /// already-parsed structure instead of re-parsing brackets from raw text.
    /// Returns `true` if the chain was recorded (≥2 segments).
    fn try_record_chain_ref(node: &AstNode, insts: &mut McInstances, scope: &str) -> bool {
        let mut segments: Vec<ChainSegment> = Vec::new();
        let mut chain_end: Option<usize> = None;
        // Whether the last recorded segment came from a bracketed AST node
        // (curly group or fcall). Decided by the AST node type, not by
        // string content — the parser excludes the closing delimiter from
        // node spans, so a curly group always needs one extra byte for `}`.
        let mut closing_delim = false;

        // A chain whose root is MCAST_OPD_DOT has the receiver/fcall as its
        // `sub` and the member as the fcall's `next` (e.g. `uC.i2c(0x36).I2C0`
        // parses as DOT(sub=FCALL(uC.i2c(0x36)), next=IDS(I2C0))). Start the
        // walk at the sub node; the fcall's `next` is traversed as siblings.
        let mut current = node.get_sub_node();
        while let Some(n) = current {
            let ty = n.get_type();

            // Stop at connection operators `->` or `-`.
            if ty == MCAST_OPD_LEFTARROW || ty == MCAST_OPD_MINUS {
                break;
            }

            // Handle DOT member references:
            //   - `.19`  → DOT wraps MCAST_INT    → extract "19"
            //   - `.ADC` → DOT wraps MCAST_IDA/IDS → extract identifier text
            //   - `.ADC{P,N}` → DOT wraps "ADC" + sibling MCAST_OPD_CURLY "P{N}"
            //     (merged dotted form is handled inside collect_ident_segments)
            if ty == MCAST_OPD_DOT {
                if let Some(sub) = n.get_sub_node() {
                    if sub.get_type() == MCAST_INT {
                        if let Some(num) = sub.to_string() {
                            segments.push(ChainSegment::Ident(num));
                            chain_end = Some(sub.get_pos() as usize + sub.get_len() as usize);
                            closing_delim = false;
                        }
                    } else {
                        // IDA / IDS — reuse the ident walker so curly groups
                        // report their closing delimiter from the AST type.
                        Self::collect_ident_segments(
                            &sub,
                            &mut segments,
                            &mut chain_end,
                            &mut closing_delim,
                        );
                    }
                }
                current = n.get_next();
                continue;
            }
            if ty == MCAST_OPD_COLON || ty == MCAST_OPD_DBCOLON {
                current = n.get_next();
                continue;
            }

            if ty == MCAST_ID || ty == MCAST_IDA || ty == MCAST_IDS {
                Self::collect_ident_segments(&n, &mut segments, &mut chain_end, &mut closing_delim);
            } else if ty == MCAST_OPD_FCALL {
                Self::collect_fcall_segments(&n, &mut segments, &mut chain_end, &mut closing_delim);
            } else if ty == MCAST_OPD {
                // Nested MCAST_OPD wrapping the chain — recurse into it.
                Self::walk_chain_children(&n, &mut segments, &mut chain_end, &mut closing_delim);
                break;
            }

            current = n.get_next();
        }

        // Need at least 2 segments for a cross-container chain (e.g., `uC.I2C0`).
        if segments.len() < 2 {
            return false;
        }
        let mut chain_end = match chain_end {
            Some(e) => e,
            None => return false,
        };

        // ★ The parser excludes closing delimiters from AST node spans: the
        // curly node for `uC.ADC{P,N}` covers the members only and the `}`
        // lands right after them; an fcall's `)` lands right after its last
        // argument. When the final segment came from such a node (decided by
        // AST type above), extend the recorded span by one byte so
        // hover/tooltip shows the whole `uC.ADC{P,N}` instead of `uC.ADC{P,N`.
        if closing_delim {
            chain_end += 1;
        }

        let start = node.get_pos() as usize;
        let span = start..chain_end;
        insts.record_chain_ref(span, segments, scope);
        true
    }

    /// Collect chain segments from a `MCAST_INSTANCE` node (the receiver of a
    /// method call, e.g. `uC` in `uC.i2c(0x36)`). The instance wraps an
    /// MCAST_OPD whose sub is the identifier(s), so delegate to the ident
    /// walker.
    fn collect_instance_segments(
        n: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        if let Some(opd) = n.get_sub_node() {
            if let Some(ids) = opd.get_sub_node() {
                Self::collect_ident_segments(&ids, segments, chain_end, closing_delim);
            }
        }
    }

    /// Collect chain segments from an `MCAST_OPD_FCALL` node. A method call
    /// `uC.i2c(0x36)` has children [MCAST_INSTANCE uC, MCAST_NAME i2c,
    /// MCAST_PARAMS 0x36]: push the receiver instance as an Ident segment and
    /// the function name as an Fcall segment (the resolver treats Fcall as a
    /// transparent hop since the function returns `this`).
    fn collect_fcall_segments(
        n: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        let end = n.get_pos() as usize + n.get_len() as usize;
        let mut child = n.get_sub_node();
        while let Some(c) = child {
            match c.get_type() {
                MCAST_INSTANCE => {
                    Self::collect_instance_segments(&c, segments, chain_end, closing_delim);
                }
                MCAST_NAME => {
                    if let Some(name) = c.to_string() {
                        if !name.is_empty() {
                            segments.push(ChainSegment::Fcall(name));
                            *chain_end = Some(end);
                            *closing_delim = true;
                        }
                    }
                }
                _ => {}
            }
            child = c.get_next();
        }
    }

    /// Walk children of a nested MCAST_OPD node, collecting chain segments.
    fn walk_chain_children(
        node: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        let mut current = node.get_sub_node();
        while let Some(n) = current {
            let ty = n.get_type();

            if ty == MCAST_OPD_LEFTARROW || ty == MCAST_OPD_MINUS {
                break;
            }

            // Handle DOT member references:
            //   - `.19`  → DOT wraps MCAST_INT    → extract "19"
            //   - `.ADC` → DOT wraps MCAST_IDA/IDS → extract identifier text
            //   - `.ADC{P,N}` → DOT wraps "ADC" + sibling MCAST_OPD_CURLY "P{N}"
            //     (merged dotted form is handled inside collect_ident_segments)
            if ty == MCAST_OPD_DOT {
                if let Some(sub) = n.get_sub_node() {
                    if sub.get_type() == MCAST_INT {
                        if let Some(num) = sub.to_string() {
                            segments.push(ChainSegment::Ident(num));
                            *chain_end = Some(sub.get_pos() as usize + sub.get_len() as usize);
                            *closing_delim = false;
                        }
                    } else {
                        Self::collect_ident_segments(&sub, segments, chain_end, closing_delim);
                    }
                }
                current = n.get_next();
                continue;
            }
            if ty == MCAST_OPD_COLON || ty == MCAST_OPD_DBCOLON {
                current = n.get_next();
                continue;
            }

            if ty == MCAST_ID || ty == MCAST_IDA || ty == MCAST_IDS {
                Self::collect_ident_segments(&n, segments, chain_end, closing_delim);
            } else if ty == MCAST_OPD_FCALL {
                Self::collect_fcall_segments(&n, segments, chain_end, closing_delim);
            }

            current = n.get_next();
        }
    }

    /// Extract chain segments from an identifier node, handling the merged
    /// dotted form where `uC.ADC{P,N}` is a single MCAST_IDS whose sub_node
    /// is `[MCAST_ID "uC", MCAST_OPD_DOT "ADC", MCAST_OPD_CURLY "P{N}"]`.
    fn collect_ident_segments(
        n: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        let end = n.get_pos() as usize + n.get_len() as usize;
        if let Some(sub) = n.get_sub_node() {
            let mut pending_dot: Option<String> = None;
            let mut cur = sub;
            loop {
                let st = cur.get_type();
                if st == MCAST_OPD_DOT {
                    if let Some(t) = cur.to_string() {
                        // ★ Consecutive dots (`lpa.IN.N` → [DOT IN, DOT N]):
                        // flush the previous member first so the middle
                        // segment is not dropped (segments would become
                        // [lpa, N] instead of [lpa, IN, N]).
                        if let Some(prev) = pending_dot.take() {
                            segments.push(ChainSegment::Ident(prev));
                        }
                        pending_dot = Some(t);
                    }
                } else if st == MCAST_OPD_CURLY || st == MCAST_OPD_CURLY_MN {
                    // Combine the pending dot member with the group members:
                    // "ADC" + [P, N] → Group { base: "ADC", members: [P, N] }.
                    let members = Self::collect_curly_members(&cur);
                    let member = pending_dot.take().unwrap_or_default();
                    segments.push(ChainSegment::Group {
                        base: member,
                        members,
                    });
                    // A curly node is a bracketed AST node: its `}` is
                    // excluded from the node span, so the chain needs +1.
                    *closing_delim = true;
                } else if let Some(t) = cur.to_string() {
                    // Base identifier (first child) or other member.
                    segments.push(ChainSegment::Ident(t));
                    *closing_delim = false;
                }
                match cur.get_next() {
                    Some(nx) => cur = nx,
                    None => break,
                }
            }
            // Flush a pending dot member with no following group (e.g. `uC.ADC`).
            if let Some(d) = pending_dot {
                segments.push(ChainSegment::Ident(d));
                *closing_delim = false;
            }
            *chain_end = Some(end);
        } else if let Some(text) = n.to_string() {
            segments.push(ChainSegment::Ident(text));
            *chain_end = Some(end);
            *closing_delim = false;
        }
    }

    /// Collect the member names of a curly group node (`{P,N}` → `["P", "N"]`).
    /// Numeric ranges (`{1:3}`) are expanded to their individual members.
    fn collect_curly_members(curly: &AstNode) -> Vec<String> {
        let mut members: Vec<String> = Vec::new();
        if let Some(sub) = curly.get_sub_node() {
            let mut cur = sub;
            loop {
                if cur.get_type() == MCAST_OPD_COLON {
                    // `{1:3}` range — expand to individual members.
                    if let Some((from, to)) = Self::curly_range(&cur) {
                        for i in from..=to {
                            members.push(i.to_string());
                        }
                    }
                } else if let Some(t) = cur.to_string() {
                    if !t.is_empty() && t != "," {
                        members.push(t);
                    }
                }
                match cur.get_next() {
                    Some(nx) => cur = nx,
                    None => break,
                }
            }
        }
        members
    }

    /// Parse a colon-range child of a curly group (`1:3`) into its bounds.
    fn curly_range(node: &AstNode) -> Option<(i64, i64)> {
        let sub = node.get_sub_node()?;
        let from = sub.to_string()?.parse::<i64>().ok()?;
        let to = sub.get_next()?.to_string()?.parse::<i64>().ok()?;
        (from <= to).then_some((from, to))
    }

    fn record_scoped_net_ref(
        node: &AstNode,
        insts: &mut McInstances,
        params: &mut McParamDeclares,
        scope: &str,
    ) {
        let Some(text) = node.to_string() else {
            return;
        };
        let ids = McIds::new(node);
        let root = ids.as_ref().and_then(McIds::root_name).unwrap_or_else(|| {
            text.split(|c: char| c == '.' || c == '{' || c == '[')
                .next()
                .unwrap_or(&text)
                .to_string()
        });
        if root.is_empty() {
            return;
        }

        let start = node.get_pos() as usize;
        let span = start..(start + root.len().min(node.get_len() as usize));
        let matched_key = if insts.contains(&root) {
            Some(root.clone())
        } else {
            insts
                .resolve_idx(&text)
                .or_else(|| insts.resolve_idx(&root))
        };

        if let Some(key) = matched_key {
            insts.record_net_ref(span, &key, scope);
        } else if params.is_defined(&root) {
            params.record_net_ref(span, &root, scope);
        } else {
            insts.record_net_ref(span, &root, scope);
        }

        // ★ §3.4.3 (rev): per-segment member refs — curly-bus members and
        // dot members get their own refs so F12 lands on the member text.
        // Member node pos is reliable; len is not (mc_value_link extension).
        //
        // Two curly forms are handled:
        //   - `MIC{P,N}`        — McIds parses the whole bus (`as_bus` hits);
        //   - `U_MCU{I2C0.SCL}` — McIds only parses the leading segment, so the
        //     curly child carries the members; base = first segment (`root`).
        if let Some(ids) = ids {
            let base_from_bus = ids.as_bus().map(|(b, _members)| b);
            let has_curly_child = node.get_sub_node().map_or(false, |sub| {
                let mut cur = Some(sub);
                loop {
                    let Some(child) = cur else { break false };
                    if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                        break true;
                    }
                    cur = child.get_next();
                }
            });
            if base_from_bus.is_some() || has_curly_child {
                // Register each curly member as `<base>.<member>` so F12 lands
                // on the member text (`MIC.P`, `U_MCU.I2C0.SCL`).
                let bus = base_from_bus.unwrap_or_else(|| root.clone());
                let mut cur = node.get_sub_node();
                while let Some(child) = cur {
                    if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                        let mut mc = child.get_sub_node();
                        while let Some(m) = mc {
                            if let Some(mname) = m.to_string() {
                                let mstart = m.get_pos() as usize;
                                insts.record_net_ref(
                                    mstart..(mstart + mname.len()),
                                    &format!("{bus}.{mname}"),
                                    scope,
                                );
                            }
                            mc = m.get_next();
                        }
                    }
                    cur = child.get_next();
                }
            } else if ids.count() >= 2 {
                // `MIC.P`: register the member segment (text after the dot).
                let member_start = start + root.len() + 1;
                let member_len = text.len().saturating_sub(root.len() + 1);
                if member_len > 0 {
                    insts.record_net_ref(member_start..(member_start + member_len), &text, scope);
                }
            } else {
                // ★ Dot chain: `MIC.P`, `U_MCU.I2C0.SCL`. `node.to_string()`
                // returns only the base segment (`U_MCU`), while `ids.to_string()`
                // carries the full chain. Register the member segment(s) after
                // the first dot so F12 lands on the member text and lapper can
                // resolve it via the member chain (Phase 3).
                let full = ids.to_string();
                if !ids.is_square_only() && full.contains('.') {
                    let member_start = start + root.len() + 1;
                    let member_end = start + full.len();
                    if member_end > member_start {
                        insts.record_net_ref(member_start..member_end, &full, scope);
                    }
                }
            }
        }
    }
}

// ============================================================================
// Mc2Module - Module instance wrapper
// ============================================================================

#[derive(Debug, Clone)]
pub struct Mc2Module {
    pub base: Arc<McModule>,
    pub name: McIds,
    pub args: Vec<McParamValue>,
    pub insts: Vec<McInst>,
}

impl Mc2Module {
    pub fn new(name: &str, base: Arc<McModule>) -> Self {
        Self {
            base,
            name: McIds::from(name),
            args: Vec::new(),
            insts: Vec::new(),
        }
    }

    pub fn with_params(name: &str, base: Arc<McModule>, args: Vec<McParamValue>) -> Self {
        Self {
            base,
            name: McIds::from(name),
            args,
            insts: Vec::new(),
        }
    }

    /// Find externally exposed ports
    pub fn find_port(&self, id: &str) -> Option<McPhrase> {
        // 1. Find in interface definitions
        if let Some(_port) = self.base.insts.find_port(id) {
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(McBus::new_with_members(
                    &self.name.to_string(),
                    vec![id.to_string()],
                )),
            ))));
        }

        // 2. Support dot-path lookup (e.g. "in.data")
        if let Some((first, rest)) = id.split_once('.') {
            if let Some((_iotype, port)) = self.base.insts.get_with_iotype(first) {
                // Find in port's sub-members
                for member_name in port.members() {
                    if member_name == rest {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(&format!(
                                "{}.{}.{}",
                                self.name, first, rest
                            ))),
                        ))));
                    }
                }
            }
        }

        // 3. Find in functions (supports method calls)
        // TODO: phase 2 implementation

        None
    }

    /// Get all input ports
    pub fn get_input_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_inputs()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }

    /// Get all output ports
    pub fn get_output_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_outputs()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }

    /// Get all ports
    pub fn get_all_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_ports()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }
}

// ============================================================================
// Display implementation - concise format output
// ============================================================================

impl std::fmt::Display for McModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Module {}", self.name)?;
        writeln!(f, "  Insts:")?;

        // Collect inst data for alignment calculation
        #[derive(Debug)]
        struct InstRow {
            io: String,
            name: String,
            inst: String,
            inst_type: String,
            has_io: bool,
            type_order: u8, // 0=Component/Module, 1=Interface, 2=Label, 3=Bus, 4=other
        }

        let mut rows: Vec<InstRow> = Vec::new();
        for (name, (io, inst)) in self.insts.iter_with_iotype() {
            let has_io = !matches!(*io, IOType::None);
            let io_str = if has_io {
                format!("{io:?}")
            } else {
                String::new()
            };
            // Strip type prefixes from instance display and collect type separately
            let (inst_str, type_str, type_order) = match inst {
                McInstance::Component(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Component:").to_string(),
                        "Component".to_string(),
                        0,
                    )
                }
                McInstance::Module(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Module:").to_string(),
                        "Module".to_string(),
                        0,
                    )
                }
                McInstance::Label(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("L:").to_string(),
                        "Label".to_string(),
                        2,
                    )
                }
                McInstance::Interface(_) => (inst.to_string(), "Interface".to_string(), 1),
                McInstance::Bus(_) => (inst.to_string(), "Bus".to_string(), 3),
                McInstance::BusRef { .. } => (inst.to_string(), "Ref".to_string(), 4),
                McInstance::List(_) | McInstance::Unresolved { .. } => {
                    (inst.to_string(), "Unresolved".to_string(), 5)
                }
                McInstance::Pins => ("pins".to_string(), "Pins".to_string(), 6),
                McInstance::PinId(id) => (id.clone(), "PinId".to_string(), 6),
                McInstance::Attr(_) => (inst.to_string(), "Attr".to_string(), 7),
                McInstance::Func(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Func:").to_string(),
                        "Func".to_string(),
                        8,
                    )
                }
                McInstance::EnumVal { .. } => (inst.to_string(), "EnumVal".to_string(), 9),
            };
            rows.push(InstRow {
                io: io_str,
                name: name.to_string(),
                inst: inst_str,
                inst_type: type_str,
                has_io,
                type_order,
            });
        }

        // Sort: 1. has_io=true first, 2. type_order, 3. name
        rows.sort_by(|a, b| {
            let io_cmp = b.has_io.cmp(&a.has_io);
            if io_cmp != std::cmp::Ordering::Equal {
                return io_cmp;
            }
            let type_cmp = a.type_order.cmp(&b.type_order);
            if type_cmp != std::cmp::Ordering::Equal {
                return type_cmp;
            }
            a.name.cmp(&b.name)
        });

        // Calculate column widths
        let io_width = rows.iter().map(|r| r.io.len()).max().unwrap_or(0);
        let name_width = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
        let inst_width = rows.iter().map(|r| r.inst.len()).max().unwrap_or(0);

        // Output with alignment
        for row in &rows {
            if row.io.is_empty() {
                if row.inst_type.is_empty() {
                    writeln!(
                        f,
                        "    {:<width$} {:<name_width$} = {:<inst_width$}",
                        "",
                        row.name,
                        row.inst,
                        width = io_width,
                        name_width = name_width,
                        inst_width = inst_width
                    )?;
                } else {
                    writeln!(
                        f,
                        "    {:<width$} {:<name_width$} = {:<inst_width$}  {}",
                        "",
                        row.name,
                        row.inst,
                        row.inst_type,
                        width = io_width,
                        name_width = name_width,
                        inst_width = inst_width
                    )?;
                }
            } else if row.inst_type.is_empty() {
                writeln!(
                    f,
                    "    {:<width$} {:<name_width$} = {:<inst_width$}",
                    row.io,
                    row.name,
                    row.inst,
                    width = io_width,
                    name_width = name_width,
                    inst_width = inst_width
                )?;
            } else {
                writeln!(
                    f,
                    "    {:<width$} {:<name_width$} = {:<inst_width$}  {}",
                    row.io,
                    row.name,
                    row.inst,
                    row.inst_type,
                    width = io_width,
                    name_width = name_width,
                    inst_width = inst_width
                )?;
            }
        }

        writeln!(f, "  Lines:")?;
        for line in &self.lines {
            writeln!(f, "    {line}")?;
        }
        Ok(())
    }
}
