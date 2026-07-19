// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::{dlog_error, dlog_warning};
use crate::semantic::basic::mc_bus::{McBus, McList};
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::component::Mc2Component;
use crate::semantic::mc_inst::McInstance;
use crate::semantic::module::Mc2Module;
use crate::McIds;
use crate::McInstances;
use crate::{
    ast::ast_node::AstNode, ast::c_macros::*, ast::error::message::*,
    semantic::basic::mc_param::McParamDeclares,
};

// ============================================================================
// McFuncReturn — function return-value kind (parse-time)
// ============================================================================

/// Function return-value kind, decided at parse time from the body's `return`
/// statement (if any). Used by the call-site parser to validate method chains.
///
/// Chainability rules:
///   * `Implicit` — no explicit `return`; backwards-compatible default that
///     behaves like `return this`. Chainable.
///   * `This` — explicit `return this`. Chainable.
///   * `Endpoint(_)` — explicit `return <bus|label|expr>`. **Not** chainable;
///     the result is a value/endpoint, not the receiver, so `.next_method()`
///     after it is a hard error.
#[derive(Debug, Clone, Default)]
pub enum McFuncReturn {
    /// No explicit `return` statement.
    #[default]
    Implicit,
    /// Explicit `return this`.
    This,
    /// Explicit `return <expr>` where the expression resolves to a label/bus
    /// or any other non-`this` phrase.
    Endpoint(McPhrase),
}

impl McFuncReturn {
    /// Whether the return value supports continued method chaining.
    pub fn is_chainable(&self) -> bool {
        matches!(self, McFuncReturn::Implicit | McFuncReturn::This)
    }

    /// Short tag for diagnostics ("implicit"/"this"/"endpoint").
    pub fn kind_str(&self) -> &'static str {
        match self {
            McFuncReturn::Implicit => "implicit",
            McFuncReturn::This => "this",
            McFuncReturn::Endpoint(_) => "endpoint",
        }
    }
}

/// Trait for types that can provide instance lookup for symbol resolution
pub trait HasFindInst {
    fn find_inst(&self, id: &str) -> Option<McInstance>;
    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance>;
    /// Add a label, optionally recording its source span for LSP goto-def.
    fn add_label(&mut self, name: String) -> Option<McPhrase> {
        self.add_label_at(name, None)
    }
    /// Add a label with a known source span.
    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase>;
    fn add_component(
        &mut self,
        name: String,
        comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase>;
    fn add_module(
        &mut self,
        name: String,
        module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase>;
    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase>;
    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase>;
    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase>;
    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase>;
    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)>;
    fn is_component_bus(&self, base: &str, member: &str) -> bool;
    fn upgrade_label_to_bus(&mut self, name: &str) -> bool;
    fn uri(&self) -> &crate::McURI;
    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance>;
    fn gen_anon_name(&mut self, classname: &str) -> String;

    /// Look up a user-defined function in the surrounding scope and report
    /// its return kind. Used by [`McFuncCall`] to validate method chains.
    ///
    /// The default implementation returns `None`, meaning "no function with
    /// that name is visible in this scope". `McModule` / `McComponent` should
    /// override this to delegate into their own `funcs` table, e.g.:
    /// ```ignore
    /// fn find_func_return(&self, name: &str) -> Option<McFuncReturn> {
    ///     self.funcs.find(name).map(|f| f.returns.clone())
    /// }
    /// ```
    fn find_func_return(&self, _name: &str) -> Option<McFuncReturn> {
        None
    }

    /// Return the enclosing scope name (module/component/function name),
    /// or None for file-level scope.
    fn scope_name(&self) -> Option<String> {
        None
    }
}

/// Composite context for func body parsing: first searches func params,
/// then falls back to the parent (module/component) for module-level instances.
struct FuncBodyContext<'a> {
    param_names: &'a [String],
    parent: &'a mut dyn HasFindInst,
}

impl<'a> FuncBodyContext<'a> {
    fn find_param(&self, id: &str) -> Option<McInstance> {
        if self.param_names.iter().any(|n| n == id) {
            Some(McInstance::Label(id.to_string()))
        } else {
            None
        }
    }
}

impl<'a> HasFindInst for FuncBodyContext<'a> {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_param(id).or_else(|| self.parent.find_inst(id))
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance> {
        self.parent.find_inst_mut(id)
    }

    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        self.parent.add_label_at(name, span)
    }

    fn add_component(
        &mut self,
        name: String,
        comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase> {
        self.parent.add_component(name, comp)
    }

    fn add_module(
        &mut self,
        name: String,
        module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        self.parent.add_module(name, module)
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        self.parent.add_bus(name, members)
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        self.parent.add_list(name, members)
    }

    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase> {
        self.parent.add_bus_member(base, member)
    }

    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase> {
        self.parent
            .add_interface_member(component, interface, members)
    }

    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)> {
        self.parent.check_bus_member(base, member)
    }

    fn is_component_bus(&self, base: &str, member: &str) -> bool {
        self.parent.is_component_bus(base, member)
    }

    fn upgrade_label_to_bus(&mut self, name: &str) -> bool {
        self.parent.upgrade_label_to_bus(name)
    }

    fn uri(&self) -> &crate::McURI {
        self.parent.uri()
    }

    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        self.parent.parse_declare(node)
    }

    fn gen_anon_name(&mut self, classname: &str) -> String {
        self.parent.gen_anon_name(classname)
    }

    fn find_func_return(&self, name: &str) -> Option<McFuncReturn> {
        self.parent.find_func_return(name)
    }

    fn scope_name(&self) -> Option<String> {
        self.parent.scope_name()
    }
}

#[derive(Debug, Clone, Default)]
pub struct McFunctions {
    functions: Vec<McFunction>,
}

impl McFunctions {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
        }
    }

    /// Parse header + body with context (for symbol resolution)
    pub fn parse(&mut self, node: &AstNode, context: &mut dyn HasFindInst) {
        if let Some(mut new_node) = McFunction::new(node) {
            // Find and parse body
            if let Some(subnodes) = node.get_sub_node() {
                if let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) {
                    new_node.parse_body(context, &body);
                }
            }
            self.functions.push(new_node);
        }
    }

    /// Find by function name
    pub fn find(&self, name: &str) -> Option<&McFunction> {
        self.functions
            .iter()
            .find(|elem| elem.name.to_string() == name)
    }

    /// Find by function name (mutable reference)
    pub fn find_mut(&mut self, name: &str) -> Option<&mut McFunction> {
        self.functions
            .iter_mut()
            .find(|elem| elem.name.to_string() == name)
    }
}

impl std::ops::Deref for McFunctions {
    type Target = Vec<McFunction>;

    fn deref(&self) -> &Self::Target {
        &self.functions
    }
}

impl std::ops::DerefMut for McFunctions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.functions
    }
}

#[derive(Debug, Clone)]
pub struct McFunction {
    pub name: McIds,
    pub params: McParamDeclares,
    /// Return-value kind (Implicit / This / Endpoint).
    /// Set by [`parse_body`] when it encounters a `return` statement.
    pub returns: McFuncReturn,
    pub insts: McInstances,
    pub lines: Vec<McPhrase>,
    /// Pre-parsed function body connections (needs to be called after McModule is built to fill parse_body)
    pub called_time: u32,
    anon_counter: usize,
    uri: Option<crate::McURI>,
}

impl McFunction {
    pub fn new(node: &AstNode) -> Option<Self> {
        // MCAST_FUNCTION
        // |- MCAST_NAME - MCAST_PARAM (option) - MCAST_BODY
        let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);

        //1. new
        let mut ret = Self {
            name: McIds::new(
                &subnodes
                    .iter()
                    .find(|x: &AstNode| x.is_type(MCAST_NAME))
                    .expect(MISSING_SUBNODE)
                    .get_sub_node() // ids
                    .expect(MISSING_SUBNODE),
            )?,
            params: McParamDeclares::new(),
            returns: McFuncReturn::Implicit,
            insts: McInstances::new(),
            lines: Vec::new(),
            called_time: 0,
            anon_counter: 1,
            uri: None,
        };

        //2. param
        let _ = &subnodes
            .iter()
            .find(|x: &AstNode| x.is_type(MCAST_PARAMS))
            .map(|param_node| ret.params.parse(&param_node));

        // ret.body
        //     .iter()
        //     .filter(|x| x.is_type(MCAST_ATTRIBUTE))
        //     .for_each(|x| ret.attrs.parse(x));

        Some(ret)
    }

    pub fn call_count_incr(&mut self) {
        self.called_time += 1;
    }

    /// Parse function body in McModule context
    ///
    /// Needs to be called after McModule is created (after symbol table is ready),
    /// because McOpd::new in function body needs symbol resolution through context.
    ///
    /// # Call timing
    /// In pass_defgen phase, all McModule members (components, submodules, labels, function declarations)
    /// after all parsed, iterate all functions calling this method:
    /// ```ignore
    /// for func in self.funcs.iter_mut() {
    ///     func.parse_body(self);
    /// }
    /// ```
    pub fn parse_body(&mut self, context: &mut dyn HasFindInst, body: &AstNode) {
        let uri = context.uri().clone();
        self.uri = Some(uri.clone());
        // ★ LSP: Set scope for instance registration with parent prefix
        let parent_scope = context.scope_name().unwrap_or_default();
        let full_scope = if parent_scope.is_empty() {
            self.name.to_string()
        } else {
            format!("{}.{}", parent_scope, self.name.to_string())
        };
        self.insts.scope = Some(full_scope);
        // ★ Fix: wrap context so func params are searchable by McPhrase::new
        let param_names: Vec<String> = self
            .params
            .iter()
            .filter_map(|p| p.get_primary_name())
            .collect();
        let mut wrapper = FuncBodyContext {
            param_names: &param_names,
            parent: context,
        };
        if let Some(body_nodes) = body.get_sub_node() {
            let body_nodes: AstNode = body_nodes;
            // ── [BODY-RAW] read-only diagnostic ─────────────────────────────
            // Pure print, no behavior change. List each top-level node's type under body + its
            // child node type sequence, used to confirm that `MIC{P,N} -> cap[4:5]::CAP() -> uC.ADC{P,N}`
            // this statement appears in AST in what form (or doesn't appear at all).
            // get_type() returns u16; against c_macros.rs: NET=33, DECLARE=26,
            // OPD=52, OPD_RIGHTARROW=71, OPD_DBCOLON=77, INSTANCE=29, CLASS=28。
            // {
            //     let mut idx = 0;
            //     for body_node in body_nodes.iter() {
            //         let bt = body_node.get_type();
            //         let child_types: Vec<u16> = body_node
            //             .get_sub_node()
            //             .map(|c| c.iter().map(|n| n.get_type()).collect())
            //             .unwrap_or_default();
            //         eprintln!(
            //             "[BODY-RAW] node[{}] type={} child_types={:?}",
            //             idx, bt, child_types
            //         );
            //         idx += 1;
            //     }
            //     eprintln!("[BODY-RAW] total {} top-level body nodes", idx);
            // }
            for body_node in body_nodes.iter() {
                match body_node.get_type() {
                    // MCAST_DECLARE: component/module instantiation
                    MCAST_DECLARE => {
                        self.insts.parse(&body_node, &uri);
                    }

                    MCAST_NET => {
                        if let Some(subnode) = body_node.get_sub_node() {
                            // ── return-statement detection ──────────────────
                            // The parser may wrap `return X` either as
                            //   NET → IOTYPE_RETURN(→ X | sibling X)        (typical)
                            //   NET → IOTYPE_RETURN  (bare `return`)
                            // Sniff for the marker first; if found, divert
                            // the line into the return slot instead of pushing
                            // it onto `self.lines`.
                            if Self::find_return_marker(&subnode).is_some() {
                                self.handle_return(&mut wrapper, &body_node, &subnode);
                                continue;
                            }

                            // MCAST_DECLARE inside MCAST_NET is a declaration - process it
                            if subnode.get_type() == MCAST_DECLARE {
                                self.insts.parse(&subnode, &uri);
                                continue;
                            }

                            match McPhrase::new(&subnode, &mut wrapper) {
                                Some(net) => {
                                    self.lines.push(net);
                                }
                                None => {
                                    // ── P1 fix: no longer silently discarded ────────────────────
                                    // Previously `None => {}` silently swallowed unresolvable connection lines,
                                    // causing whole line to disappear from netlist but errors=0/warnings=0
                                    // (typical: `MIC{P,N} -> cap[4:5]::CAP() -> uC.ADC{P,N}`).
                                    // Now upgraded to Warning (non-fatal, doesn't break errors=0 gate),
                                    // with reconstructed source text, making any "whole-line evaporation" immediately visible.
                                    let line_txt = subnode
                                        .to_string()
                                        .unwrap_or_else(|| "<unprintable>".to_string());
                                    dlog_warning(
                                        1309,
                                        &subnode,
                                        &format!(
                                            "Connection line dropped (McPhrase::new returned None): `{line_txt}`"
                                        ),
                                    );
                                }
                            }
                        } else {
                            dlog_error(1300, &body_node, "Empty NET");
                        }
                    }

                    // ── return statement appearing as a top-level body node ──
                    // (defensive: some parser shapes may not wrap `return` in NET)
                    MCAST_IOTYPE_RETURN => {
                        self.handle_return(&mut wrapper, &body_node, &body_node);
                    }

                    MCAST_COND_IF => {
                        // COND_IF node: child nodes are the phrase
                        //.. todo
                    }
                    _ => {
                        dlog_error(1308, &body_node, "Invalid function body node.");
                    }
                }
            }

            // ★ Smart Param (M5): Finalize after body parsed
            let func_name = self.name.to_string();
            // ★ Collect param references in function body for LSP goto-def
            crate::semantic::component::McComponent::collect_param_refs_in_body(
                body,
                &mut self.params,
                &func_name,
            );
            let diags = self.params.finalize(Some(body), &func_name);
            for d in &diags {
                mcc::mcc_log_global_diag(d);
            }
        }
    }

    // ========================================================================
    // return-statement helpers
    // ========================================================================

    /// Locate a `MCAST_IOTYPE_RETURN` marker inside a NET subnode (or a body
    /// node that already is the marker).
    ///
    /// Two AST shapes are accepted:
    ///   * `node` itself is `MCAST_IOTYPE_RETURN`  → return `Some(node.clone())`
    ///   * `node`'s first child is `MCAST_IOTYPE_RETURN` → return that child
    ///     (allows the expression to live as a sibling at the NET layer)
    fn find_return_marker(node: &AstNode) -> Option<AstNode> {
        if node.get_type() == MCAST_IOTYPE_RETURN {
            return Some(node.clone());
        }
        if let Some(first) = node.get_sub_node() {
            if first.get_type() == MCAST_IOTYPE_RETURN {
                return Some(first);
            }
        }
        None
    }

    /// Handle a recognised `return` statement.
    ///
    /// `body_node` — the outer node, used for error position.
    /// `wrapper`   — the NET subnode (or body node) that contains the marker.
    fn handle_return(
        &mut self,
        context: &mut dyn HasFindInst,
        body_node: &AstNode,
        wrapper: &AstNode,
    ) {
        // 1. Reject multiple returns. A function may have at most one.
        if !matches!(self.returns, McFuncReturn::Implicit) {
            dlog_error(
                1313,
                body_node,
                "Multiple `return` statements are not allowed; \
                 a function may have at most one return.",
            );
            return;
        }

        // 2. Locate the IOTYPE_RETURN marker.
        let Some(marker) = Self::find_return_marker(wrapper) else {
            dlog_error(1305, body_node, "Malformed return statement.");
            return;
        };

        // 3. Find the expression node — try `marker.sub_node` first (the
        //    common "tagged wrapper" shape), then fall back to the next
        //    sibling at the NET layer.
        let expr_node_opt = marker.get_sub_node().or_else(|| marker.get_next());

        let Some(expr_node) = expr_node_opt else {
            // Bare `return` with no expression — interpret as `return this`.
            self.returns = McFuncReturn::This;
            return;
        };

        // 4. Recognise `return this` first: it is the only chainable variant
        //    that needs explicit acknowledgement (we cannot represent `this`
        //    as a McPhrase, since `this` is the receiver itself).
        if Self::is_this_expr(&expr_node) {
            self.returns = McFuncReturn::This;
            return;
        }

        // 5. Otherwise treat the expression as a phrase. A successful parse
        //    means it's a label / bus / endpoint → non-chainable return.
        match McPhrase::new(&expr_node, context) {
            Some(phrase) => {
                self.returns = McFuncReturn::Endpoint(phrase);
            }
            None => {
                dlog_error(
                    1307,
                    body_node,
                    "Invalid `return` expression: expected `this` or a label/bus.",
                );
            }
        }
    }

    /// Recognise `this` across the few plausible AST shapes.
    fn is_this_expr(node: &AstNode) -> bool {
        if node.get_type() == MCAST_OPD_THIS {
            return true;
        }
        if let Some(sub) = node.get_sub_node() {
            if sub.get_type() == MCAST_OPD_THIS {
                return true;
            }
        }
        if let Some(s) = node.to_string() {
            return s == "this";
        }
        false
    }
}

impl HasFindInst for McFunction {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.insts.get(id).cloned()
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut McInstance> {
        self.insts.get_mut(id)
    }

    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        if let Some(s) = span {
            self.insts.store_port_span(&name, s);
        }
        self.add_label(name)
    }

    fn add_label(&mut self, name: String) -> Option<McPhrase> {
        if let Some(existing_inst) = self.insts.get(&name) {
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                existing_inst.clone(),
            ))));
        }
        for (inst_name, inst) in self.insts.iter() {
            let is_anon = inst_name.starts_with('@')
                || (inst_name.starts_with('[') && inst_name.contains(','));
            if !is_anon {
                continue;
            }
            match inst {
                McInstance::List(list) => {
                    if list.member.contains(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                }
                McInstance::Bus(bus) => {
                    if bus.full_members.contains(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                }
                McInstance::Interface(iface) => {
                    if iface.base.pins.names_to_id.contains_key(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                    let iface_members = iface.name.expand();
                    if iface_members.len() > 1 && iface_members.contains(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                }
                _ => {}
            }
        }
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Label(name),
        ))))
    }

    fn add_component(&mut self, name: String, comp: Mc2Component) -> Option<McPhrase> {
        let inst = McInstance::Component(std::sync::Arc::new(comp));
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_module(&mut self, name: String, module: Mc2Module) -> Option<McPhrase> {
        let inst = McInstance::Module(std::sync::Arc::new(module));
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let inst = McInstance::Bus(McBus::new_with_members(&name, members));
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let inst = McInstance::List(McList::new_with_members(&name, members));
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_bus_member(&mut self, _base: &str, _member: String) -> Option<McPhrase> {
        None
    }

    fn add_interface_member(
        &mut self,
        _component: &str,
        _interface: &str,
        _members: Vec<String>,
    ) -> Option<McPhrase> {
        None
    }

    fn check_bus_member(&mut self, _base: &str, _member: &str) -> Option<(String, String)> {
        None
    }

    fn is_component_bus(&self, _base: &str, _member: &str) -> bool {
        false
    }

    fn uri(&self) -> &crate::McURI {
        self.uri
            .as_ref()
            .expect("McFunction.uri not set, call parse_body first")
    }

    /// ── Iter-7.4 (parser fix) ────────────────────────────────────────────
    /// Let `MCAST_DECLARE` embedded in func body chain (named instance `R442::RES(1MΩ)`
    /// or anonymous instance `CAP(1nF)` etc.) be correctly instantiated as Component / Module / Bus instance,
    /// instead of falling back to mc_phrase.rs:335 fallback to become label.
    ///
    /// Historical reason:
    ///   Original implementation returns `Vec::new()`, causing all DECLARE encountered in func body chain
    ///   nodes to get no instance, can only be fallback as label in mc_phrase.rs.
    ///   Symptoms (root cause of bugfix_report errors 5/6/8):
    ///     - In `XTAL + R442::RES(1MΩ)` R442 doesn't appear in components list,
    ///       netlist shows as `X6.R442 ~ CAP_3.1` (R442 used as node)
    ///     - In `(CAP(1nF) + RES(10kΩ)) -> GND` two anonymous components as bare scalar nodes
    ///       (`@CAP5 ~ GND.GND`) form, no .1/.2 pin distinction → flattened
    ///     - Crystal setup() body 18pF capacitor topology confused
    ///
    /// Fix strategy:
    ///   1. Call existing `McInstances::parse_declare` (mc_inst.rs:669) to register instance
    ///      into `self.insts` —— this 870-line monster method already handles
    ///      class lookup, CMIE, NC, nested params, array instances, construction args and all
    ///      corner cases, reuse is most stable.
    ///   2. Use set difference to get newly registered instances, clone and return `Vec<McInstance>`.
    ///      These instances are wrapped into phrase at mc_phrase.rs:330-333
    ///      (Endpoint(Component(Arc<Mc2Component>))), phrase itself carries component
    ///      all info (partno, pins etc.), instantiation phase uses directly.
    ///
    /// # Note
    /// - This method assumes `self.uri` is set by `parse_body`. `McPhrase::new` in
    ///   func body parse context, parse_body has already completed line 232's
    ///   `self.uri = Some(uri.clone())`, so the precondition is satisfied.
    /// - For a second `parse_declare` with the same `inst_name`,
    ///   `McInstances::parse_declare` internally uses `insert` to overwrite,
    ///   so the difference-derived "newly added" set will be empty — this
    ///   situation does not actually occur (declaring the same name twice in
    ///   one chain line is a user error) and is not handled.
    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        // No uri means we have to give up (parse_body hasn't finished yet?) — preserve old behavior
        let uri = match self.uri.clone() {
            Some(u) => u,
            None => return Vec::new(),
        };

        // 1) Record the instance name set before the call
        let before: std::collections::HashSet<String> =
            self.insts.iter().map(|(k, _)| k.to_string()).collect();

        // 2) Call McInstances::parse_declare to register the new instance
        //    iotype is None — inline instances in a chain are not port/power types
        self.insts
            .parse_declare(node, &uri, &crate::semantic::common::IOType::None);

        // 3) Extract newly added instances (clone — McInstance itself is an enum,
        //    internal Component/Module is Arc-wrapped, so clone is cheap)
        self.insts
            .iter()
            .filter(|(k, _)| !before.contains(*k))
            .map(|(_, inst)| inst.clone())
            .collect()
    }

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
    }

    fn gen_anon_name(&mut self, classname: &str) -> String {
        // ── P4-e: Sanitize '.' ──
        // `DIO.ESD` → `@DIO_ESD{n}`; otherwise `@DIO.ESD{n}` will be misjudged
        // by `node_to_netpoint`'s `split_once('.')` as owner=`@DIO` → multiple
        // anonymous calls share the `@DIO` label → short circuit.
        // Aligned with P0-2 (safe_type) of `pass2 instantiate_component_construction`.
        let safe = classname.replace('.', "_");
        let name = format!("@{}{}", safe, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn scope_name(&self) -> Option<String> {
        Some(self.name.to_string())
    }
}
