// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

pub mod mc_attr;
pub mod mc_layout;
pub mod mc_pins; // mc_pins/mod.rs includes mc_pins/dynamic.rs

use self::mc_attr::McAttributes;
use self::mc_layout::McLayout;
use self::mc_pins::McPinPort;
use self::mc_pins::McPins;
use super::{
    basic::mc_conds::{McCondition, McConds},
    basic::mc_endpoint::{McEndpoint, McInstanceRef},
    basic::mc_param::McParamDeclares,
    basic::mc_phrase::McPhrase,
    mc_func::HasFindInst,
    mc_func::McFunctions,
};
use crate::{
    ast::ast_node::AstNode,
    ast::c_macros::*,
    db::cmie::tables as workspace,
    semantic::basic::mc_bus::{McBus, McList},
    semantic::basic::mc_ids::McIds,
    semantic::basic::mc_param::{McParamBindings, McParamValue},
    semantic::basic::mc_paramd::McParamDeclareKind,
    semantic::mc_inst::McInst,
    semantic::mc_inst::McInstance,
    semantic::mc_inst::McInstances,
    McURI,
};
use std::ops::Range;
use std::sync::Arc;

/// A conditional pin block: a condition and its parsed pins
#[derive(Debug, Clone)]
pub struct CondPins {
    pub if_blocks: Vec<(McCondition, McPins)>,
    pub else_pins: Option<McPins>,
    /// Source span of the whole `if`/`else if`/`else` chain (byte range in `uri`),
    /// so diagnostics point at the conditional block instead of the component name.
    pub span: Range<usize>,
}

/// A conditional attribute block: a condition and its parsed attributes
#[derive(Debug, Clone)]
pub struct CondAttrs {
    pub if_blocks: Vec<(McCondition, McAttributes)>,
    pub else_attrs: Option<McAttributes>,
    /// Source span of the whole `if`/`else if`/`else` chain (byte range in `uri`).
    pub span: Range<usize>,
}

#[derive(Debug)]
pub struct McComponent {
    pub name: McIds,
    pub params: McParamDeclares,
    pub pins: McPins,
    pub attrs: McAttributes,
    pub funcs: McFunctions,
    pub insts: McInstances,
    pub layout: McLayout,
    pub uri: McURI,
    /// Conditional pin blocks that could not be evaluated at parse time
    /// (because parameters have no default values). Evaluated at instantiation time.
    pub cond_pins: Vec<CondPins>,
    /// Conditional attribute blocks that could not be evaluated at parse time
    /// (because parameters have no default values). Evaluated at instantiation time.
    pub cond_attrs: Vec<CondAttrs>,
    /// Source span for LSP goto-definition (byte range in `uri`).
    pub span: crate::ast::ast_semantic::Span,
    /// Counter for anonymous-instance names (`@{classname}{counter}`), mirroring
    /// `McModule::anon_counter`. Anonymous chains in component func bodies
    /// (`XTAL2(...).Setup(VSS)`) name their receiver here (§3.1). pass2 never
    /// materializes comp.base.insts, so these names stay dormant until Part 3.
    pub anon_counter: usize,
}

impl McComponent {
    /// The formal params that instance construction arguments bind to.
    ///
    /// A same-name constructor func declares the actual construction arity
    /// (`FLASH.GD25Q32E flash(V3V3)` binds `V3V3` to `func GD25Q32E([V3V3, GND]::DC(3.3V))`,
    /// §P1 C6). When such a func exists its params are authoritative;
    /// otherwise the class-level params are used. Class params define class
    /// behavior and func params are local to the func — they never overlap
    /// (a same-name pair is rejected as COMPONENT_PARAM_FUNC_CONFLICT).
    pub fn bind_params(&self) -> &McParamDeclares {
        let last = self
            .name
            .to_string()
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_string();
        match self.funcs.find(&last) {
            Some(f) => &f.params,
            None => &self.params,
        }
    }

    /// Whether this component has any pin definitions: static pins, dynamic
    /// (range) pins, or conditional pin blocks. Conditional pins are stored
    /// separately in `cond_pins` (evaluated at instantiation time when the
    /// governing params are bound), so a component whose pins live inside
    /// `if`/`else` blocks still counts as having pins here.
    pub fn has_pin_defs(&self) -> bool {
        self.pins.has_any_pins() || !self.cond_pins.is_empty()
    }

    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_COMPONENT
        // |- MCAST_NAME - MCAST_PARAMS (option) - MCAST_BODY
        let subnodes = node.get_sub_node()?;

        //1. new with name
        let comp_name = McIds::new_with_dot(
            &subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))?
                .get_sub_node()?,
        )?;

        // Span from the component name (MCAST_NAME → MCAST_IDS), not the whole node
        let name_node = subnodes.iter().find(|x| x.is_type(MCAST_NAME))?;
        let ids_node = name_node.get_sub_node()?;
        let start = ids_node.get_pos() as usize;
        let end = start + ids_node.get_len() as usize;
        let mut new_comp = Self {
            name: comp_name.clone(),
            params: McParamDeclares::new(),
            attrs: McAttributes::new(),
            pins: McPins::new(),
            funcs: McFunctions::new(),
            insts: McInstances::new(),
            uri: uri.clone(),
            layout: McLayout::empty(),
            cond_pins: Vec::new(),
            cond_attrs: Vec::new(),
            span: crate::ast::ast_semantic::Span { start, end },
            anon_counter: 1,
        };

        //2. param
        new_comp.params.enclosing_component_name = Some(comp_name.clone());
        let _ = &subnodes
            .iter()
            .find(|x| x.is_type(MCAST_PARAMS))
            .map(|param_node| new_comp.params.parse(&param_node));

        //3. body
        if let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) {
            if let Some(body_nodes) = body.get_sub_node() {
                //3. attributes
                body_nodes
                    .iter()
                    .filter(|x| x.is_type(MCAST_ATTRIBUTE))
                    .for_each(|x| {
                        if let Some(built_layout) = mc_layout::McLayout::new(&x) {
                            new_comp.layout = built_layout;
                        } else {
                            new_comp.attrs.parse(&x);
                        }
                    });

                //4. pins
                let pin_nodes: Vec<_> = body_nodes
                    .iter()
                    .filter(|x| x.is_type(MCAST_ATTRIBUTE_PIN) || x.is_type(MCAST_ATTRIBUTE_PINADD))
                    .collect();
                pin_nodes.iter().for_each(|x| new_comp.pins.parse(x));

                //5. functions (parse header + body with context)
                // Use raw pointer to avoid conflicting borrows of new_comp
                let context =
                    unsafe { &mut *(&mut new_comp as *mut McComponent) as &mut dyn HasFindInst };
                body_nodes
                    .iter()
                    .filter(|x| x.is_type(MCAST_FUNCTION))
                    .for_each(|x| {
                        // ★ LSP: register interface class refs from the func
                        // header (`func GD25Q32E([V3V3, GND]::DC(3.3V))` →
                        // `DC`) so goto-def / hover resolve them (same path as
                        // module ports). Must run before create_lapper consumes
                        // declare_class_refs.
                        crate::query::refs::register_func_header_iface_refs(&x, &new_comp.uri);
                        new_comp.funcs.parse(&x, context);
                    });

                //6. todo: role
                //7. conds
                Self::parse_cond_blocks(
                    &mut new_comp.pins,
                    &mut new_comp.attrs,
                    &body,
                    &new_comp.params,
                    &mut new_comp.cond_pins,
                    &mut new_comp.cond_attrs,
                );
                //8. todo: net not supported

                // ★ LSP: Scan body for references to component parameters
                let comp_scope = new_comp.name.to_string();
                Self::collect_param_refs_in_body(&body, &mut new_comp.params, &comp_scope);

                // ★ Smart Param (M5): Finalize — run inference + unused check + port filter
                let diags = new_comp
                    .params
                    .finalize(Some(&body), &comp_name.to_string());
                for d in &diags {
                    crate::mcc_log_global_diag(d);
                }
            }
        }

        Some(new_comp)
    }

    fn parse_cond_blocks(
        pins: &mut McPins,
        attrs: &mut McAttributes,
        body_node: &AstNode,
        params: &McParamDeclares,
        cond_pins: &mut Vec<CondPins>,
        cond_attrs: &mut Vec<CondAttrs>,
    ) {
        let default_params = params.get_params_with_defaults();

        if let Some(body_subnodes) = body_node.get_sub_node() {
            for child in body_subnodes.iter() {
                let child_type = child.get_type();
                if child_type == MCAST_COND_IF {
                    // The `if` chain node nests every `else if` / `else` branch
                    // beneath it, so its span covers the whole conditional block.
                    // Diagnostics for the chain anchor here instead of at the
                    // component declaration line.
                    let chain_span =
                        (child.get_pos() as usize)..((child.get_pos() + child.get_len()) as usize);
                    if let Some(conds_obj) = McConds::new(&child) {
                        // Try to evaluate with default params first
                        if !default_params.is_empty() {
                            if let Some(selected_block) = conds_obj.evaluate(&default_params) {
                                let block_type = selected_block.get_type();
                                if block_type == MCAST_ATTRIBUTE_PIN
                                    || block_type == MCAST_ATTRIBUTE_PINADD
                                {
                                    pins.parse(&selected_block);
                                    continue;
                                }
                                if block_type == MCAST_ATTRIBUTE {
                                    attrs.parse(&selected_block);
                                    continue;
                                }
                                if block_type == MCAST_COND_BLOCK {
                                    if let Some(sub) = selected_block.get_sub_node() {
                                        for inner in sub.iter() {
                                            if inner.get_type() == MCAST_ATTRIBUTE {
                                                attrs.parse(&inner);
                                            }
                                        }
                                    }
                                    continue;
                                }
                            }
                        }
                        // If not evaluated (no defaults or condition didn't match),
                        // parse the blocks now and store for later evaluation

                        // ── Conditional pins ──
                        // Branch-local McPins stores only the conditional delta, but
                        // `pins +=` validation still needs the component's base context.
                        let has_base_pins = pins.has_base_pins;
                        let mut if_pin_blocks = Vec::new();
                        for cond in &conds_obj.if_blocks {
                            let mut block_pins = McPins::new();
                            block_pins.has_base_pins = has_base_pins;
                            let block_type = cond.block.get_type();
                            if block_type == MCAST_ATTRIBUTE_PIN
                                || block_type == MCAST_ATTRIBUTE_PINADD
                            {
                                block_pins.parse(&cond.block);
                            }
                            if_pin_blocks.push((cond.condition.clone(), block_pins));
                        }
                        let else_pins = conds_obj.else_block.as_ref().map(|block| {
                            let mut block_pins = McPins::new();
                            block_pins.has_base_pins = has_base_pins;
                            let block_type = block.get_type();
                            if block_type == MCAST_ATTRIBUTE_PIN
                                || block_type == MCAST_ATTRIBUTE_PINADD
                            {
                                block_pins.parse(block);
                            }
                            block_pins
                        });
                        let has_conditional_pins =
                            if_pin_blocks.iter().any(|(_, block)| block.count() > 0)
                                || else_pins.as_ref().is_some_and(|block| block.count() > 0);
                        if has_conditional_pins {
                            cond_pins.push(CondPins {
                                if_blocks: if_pin_blocks,
                                else_pins,
                                span: chain_span.clone(),
                            });
                        }

                        // ── Conditional attributes ──
                        let mut if_attr_blocks = Vec::new();
                        for cond in &conds_obj.if_blocks {
                            let mut block_attrs = McAttributes::new();
                            let block_type = cond.block.get_type();
                            if block_type == MCAST_ATTRIBUTE {
                                block_attrs.parse(&cond.block);
                            } else if block_type == MCAST_COND_BLOCK {
                                if let Some(sub) = cond.block.get_sub_node() {
                                    for inner in sub.iter() {
                                        if inner.get_type() == MCAST_ATTRIBUTE {
                                            block_attrs.parse(&inner);
                                        }
                                    }
                                }
                            }
                            if_attr_blocks.push((cond.condition.clone(), block_attrs));
                        }
                        let else_attrs = conds_obj.else_block.as_ref().map(|block| {
                            let mut block_attrs = McAttributes::new();
                            let block_type = block.get_type();
                            if block_type == MCAST_ATTRIBUTE {
                                block_attrs.parse(block);
                            } else if block_type == MCAST_COND_BLOCK {
                                if let Some(sub) = block.get_sub_node() {
                                    for inner in sub.iter() {
                                        if inner.get_type() == MCAST_ATTRIBUTE {
                                            block_attrs.parse(&inner);
                                        }
                                    }
                                }
                            }
                            block_attrs
                        });
                        let has_conditional_attrs =
                            if_attr_blocks.iter().any(|(_, block)| !block.is_empty())
                                || else_attrs.as_ref().is_some_and(|block| !block.is_empty());
                        if has_conditional_attrs {
                            cond_attrs.push(CondAttrs {
                                if_blocks: if_attr_blocks,
                                else_attrs,
                                span: chain_span.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    /// Recursively scan AST nodes in the component body for identifiers matching
    /// component parameter names (e.g. `spec.value = rs` where rs is a parameter).
    /// Record their spans for LSP goto-definition.
    pub(crate) fn collect_param_refs_in_body(
        body_node: &AstNode,
        params: &mut McParamDeclares,
        scope: &str,
    ) {
        Self::collect_param_refs_in_node(body_node, params, scope);
    }

    fn collect_param_refs_in_node(node: &AstNode, params: &mut McParamDeclares, scope: &str) {
        match node.get_type() {
            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                if let Some(text) = node.to_string() {
                    let matched = params.is_defined(&text)
                        || params.iter().any(|d| d.all_name_forms().contains(&text));
                    if matched {
                        let span =
                            (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                        params.record_net_ref(span, &text, scope);
                    }
                }
            }
            // MCAST_OPD wraps an operand — extract the inner identifier and
            // check it directly, then continue recursing for compound expressions.
            MCAST_OPD => {
                if let Some(sub) = node.get_sub_node() {
                    let inner_type = sub.get_type();
                    if matches!(inner_type, MCAST_ID | MCAST_IDA | MCAST_IDS) {
                        if let Some(text) = sub.to_string() {
                            let matched = params.is_defined(&text)
                                || params.iter().any(|d| d.all_name_forms().contains(&text));
                            if matched {
                                let span = (sub.get_pos() as usize)
                                    ..((sub.get_pos() + sub.get_len()) as usize);
                                params.record_net_ref(span, &text, scope);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some(sub) = node.get_sub_node() {
            let mut current = sub;
            loop {
                Self::collect_param_refs_in_node(&current, params, scope);
                match current.get_next() {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
    }
}

impl HasFindInst for McComponent {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    fn find_inst_mut(&mut self, _id: &str) -> Option<&mut crate::McInstance> {
        None
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        // P2 container category chain (§3.3): params → scoped enum → attrs →
        // pin names (whole) → pin names (expanded) → pin IDs → insts → funcs.
        // Each category is an independent scope unit in semantic::scope, with
        // the same hit logic (and stored spans) as the original hand-written
        // chain below it replaced.
        crate::semantic::scope::component_scope(self)
            .resolve(id)
            .map(|r| (r.inst, r.span))
    }

    fn add_label_at(
        &mut self,
        name: String,
        _span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Label(name),
        ))))
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Bus(McBus::new_with_members(&name, members)),
        ))))
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::List(McList::new_with_members(&name, members)),
        ))))
    }

    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase> {
        self.add_bus(base.to_string(), vec![member])
    }

    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase> {
        self.add_bus(format!("{component}.{interface}"), members)
    }

    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)> {
        if self.pins.is_bus(member) {
            return Some((format!("{base}.{member}"), member.to_string()));
        }
        None
    }

    fn is_component_bus(&self, _base: &str, _member: &str) -> bool {
        false
    }

    fn uri(&self) -> &McURI {
        &self.uri
    }

    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        // Mirror McFunction::parse_declare (mc_func.rs:1205) — the set-difference
        // (set-difference) pattern: snapshot the name set, register via the shared
        // `McInstances::parse_declare` monster (mc_inst.rs:1355), then return the
        // delta. A component func body's chained declare (`XTAL2 y(...).Setup(VSS)`)
        // routes here through FuncBodyContext::parse_declare (mc_func.rs:408), so
        // the subinstance now registers into the component's `insts` — the receiver
        // endpoint resolves and sibling funcs' `find_inst("y")` works (§3.1).
        // pass2 never materializes comp.base.insts, so registration is safe.
        let before: std::collections::HashSet<String> =
            self.insts.iter().map(|(k, _)| k.to_string()).collect();
        self.insts
            .parse_declare(node, &self.uri, &crate::semantic::common::IOType::None);
        self.insts
            .iter()
            .filter(|(k, _)| !before.contains(*k))
            .map(|(_, inst)| inst.clone())
            .collect()
    }

    fn add_component(&mut self, name: String, comp: Mc2Component) -> Option<McPhrase> {
        // Mirror McModule::add_component (module/mod.rs:740): create the
        // instance and — for `@`-prefixed anonymous names — skip insts
        // registration (they are created inline in connection stmts, so
        // nothing should declare them a second time). The Endpoint phrase is
        // what carries the receiver into the chain (`XTAL2(...).Setup(VSS)`).
        let inst = McInstance::Component(Arc::new(comp));
        if !name.starts_with('@') {
            self.insts.create_inst(&name, inst.clone());
        }
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_module(
        &mut self,
        _name: String,
        _module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        None
    }

    fn gen_anon_name(&mut self, classname: &str) -> String {
        // Mirror McModule::gen_anon_name (module/mod.rs:1059): `@`-prefix marks
        // the name as an inline anonymous instance (iter_port_names skips it,
        // pass2 auto-name skips it). Without this, anonymous chains in
        // component func bodies (`XTAL2(...).Setup(VSS)`) fall through to a
        // bare FuncCall with caller=None and the receiver never resolves (§3.1).
        let name = format!("@{}{}", classname, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
    }

    fn record_declareb_def(
        &mut self,
        name: &str,
        kind: crate::refdef::types::SymbolKind,
        span: std::ops::Range<usize>,
    ) {
        self.insts.record_declareb_def(name, kind, span);
    }

    fn scope_name(&self) -> Option<String> {
        Some(self.name.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Mc2Component {
    pub base: Arc<McComponent>,
    pub name: McIds,
    pub params: Vec<McParamValue>,
    pub insts: Vec<McInst>,
    pub nc: bool,
}

// ============================================================================
// Display implementation - concise format output
// ============================================================================

impl std::fmt::Display for McComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Component {}", self.name)?;
        write!(f, "{}", self.pins)
    }
}

impl Mc2Component {
    pub fn new(name: &str, base: Arc<McComponent>) -> Self {
        Self {
            name: McIds::from(name),
            base: base.clone(),
            params: Vec::new(),
            insts: Vec::new(),
            nc: false,
        }
    }

    pub fn with_nc(name: &str, base: Arc<McComponent>, is_nc: bool) -> Self {
        Self {
            name: McIds::from(name),
            base: base.clone(),
            params: Vec::new(),
            insts: Vec::new(),
            nc: is_nc,
        }
    }

    pub fn with_params(name: &str, base: Arc<McComponent>, params: Vec<McParamValue>) -> Self {
        let nc = params.iter().any(|p| matches!(p, McParamValue::NC(_)));
        Self {
            name: McIds::from(name),
            base: base.clone(),
            params,
            insts: Vec::new(),
            nc,
        }
    }

    fn integer_param_bindings(&self, bindings: &McParamBindings) -> Vec<(String, i64)> {
        let mut values = Vec::new();

        for binding in bindings.iter() {
            if let Some((name, value)) = binding.as_int_binding() {
                values.push((name, value));
                continue;
            }

            if let McParamDeclareKind::UValue(uval) = &binding.declare.kind {
                let name = uval.name.get_primary_name().unwrap_or_default();
                if let Some(McParamValue::Int(value)) = &binding.value {
                    values.push((name, value.value));
                } else if let Some(default) = &uval.default {
                    if let Ok(value) = default.parse::<i64>() {
                        values.push((name, value));
                    }
                }
            }
        }

        values
    }

    fn pins_contain(pins: &McPins, id: &str, integer_bindings: &[(String, i64)]) -> bool {
        pins.find_pin(id).is_some()
            || pins
                .resolve_dynamic_pins(integer_bindings)
                .iter()
                .any(|(pin_id, pin_name, _)| pin_id.to_string() == id || pin_name == id)
    }

    /// Resolve a pin against the concrete component instance, including the
    /// active conditional pin branch and parameter-dependent pin ranges.
    pub(crate) fn find_pin(&self, id: &str) -> Option<String> {
        if self.base.pins.find_pin(id).is_some() {
            return Some(id.to_string());
        }

        let bindings = McParamBindings::bind_quiet(&self.base.params, &self.params).ok()?;
        let integer_bindings = self.integer_param_bindings(&bindings);
        if Self::pins_contain(&self.base.pins, id, &integer_bindings) {
            return Some(id.to_string());
        }

        let eval_params = bindings.to_params_for_eval();
        for conditional in &self.base.cond_pins {
            let active = conditional
                .if_blocks
                .iter()
                .find(|(condition, _)| McConds::check_condition(condition, &eval_params))
                .map(|(_, pins)| pins)
                .or(conditional.else_pins.as_ref());

            if active.is_some_and(|pins| Self::pins_contain(pins, id, &integer_bindings)) {
                return Some(id.to_string());
            }
        }

        None
    }

    /// Find the externally-exposed interface named id
    pub fn find_port(&self, id: &str) -> Option<McPhrase> {
        if let Some(found) = self.find_pin(id) {
            let full_name = format!("{}.{}", self.name, found);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(McBus::new(&full_name)),
            ))));
        }
        None
    }
}

// ── Phase 1 helpers ──

/// Convert a [`McPinPort`] into a [`McInstance`] for the find_inst priority chain.
pub(crate) fn port_to_instance(port: &McPinPort) -> McInstance {
    match port {
        McPinPort::NC => McInstance::Label("NC".to_string()),
        McPinPort::Single(id) => McInstance::Label(id.clone()),
        McPinPort::Multi(ids) => McInstance::List(McList::new_with_members("multi", ids.clone())),
        McPinPort::MultiGroup(groups) => {
            let all: Vec<String> = groups.iter().flatten().cloned().collect();
            McInstance::List(McList::new_with_members("multi", all))
        }
        McPinPort::List(name, members) => {
            McInstance::List(McList::new_with_members(name, members.clone()))
        }
        McPinPort::Bus(bus) => McInstance::Bus(bus.clone()),
        McPinPort::Interface(iface) => McInstance::Interface(iface.clone()),
    }
}

/// Search workspace and global enum tables for a scoped enum value.
///
/// A "scoped enum" is an enum whose name matches a component's **family name**
/// (e.g. `enum CAP` makes `X7R` visible inside component `CAP_0603`).
///
/// Resolution follows the unified P1-P5 policy (§5.4): the enum **class** is
/// resolved first (P3 exact key in the referencing file, then P4 through the
/// use chain, then P5 mcode), and only then is the value located inside that
/// class's values — the value is never located by a name-only workspace scan.
///
/// Returns `(enum_name, def_uri, class_id, value_span)`:
/// - `def_uri` — the file that defines the enum class;
/// - `class_id` — the RefDefMap class id of the enum class registered in the
///   referencing file's global table (the packed value id =
///   `class_id + value index`); `None` when the class is not registered there.
pub(crate) fn find_scoped_enum_value(
    from_uri: &McURI,
    family_name: &McIds,
    id: &str,
) -> Option<(String, McURI, Option<u32>, Range<usize>)> {
    let family = family_name.to_string();

    // ① Resolve the enum class first (§5.4.3): P3 exact key → P4 use chain →
    //    P5 mcode. `find_scoped_enum_for_component` implements exactly this
    //    and carries the class's defining URI.
    let enum_def = crate::db::cmie::cmie::find_scoped_enum_for_component(family_name, from_uri)?;
    let def_uri = enum_def.uri.clone();

    // ② Locate the value inside the resolved class's values.
    for value in &enum_def.values {
        if value.name.to_string() == id {
            let span = value.span[0] as usize..value.span[1] as usize;
            let class_id = lookup_enum_class_id(from_uri, &def_uri, family_name);
            return Some((family, def_uri, class_id, span));
        }
    }

    None
}

/// Look up the RefDefMap class id of an enum class, as registered in the
/// referencing file's global table (keyed by `(def_uri, class_name)` —
/// mirroring `lapper_enum_refs`). Returns `None` when the referencing file is
/// not loaded or the class was never registered there.
///
/// Uses non-blocking `try_lock`: this lookup can run on the `create_lapper`
/// locked path (via `member_of` → `find_inst_with_span` → `ScopedEnumScope`),
/// where the same file's symbol tables are already held — a blocking `.lock()`
/// would self-deadlock (std Mutex is not reentrant). A lock failure degrades
/// gracefully to `None` (class id unavailable), never to a hang.
fn lookup_enum_class_id(from_uri: &McURI, def_uri: &McURI, class_name: &McIds) -> Option<u32> {
    let mcfile = workspace::WORKSPACE.mcodes.get(from_uri)?;
    let sem = mcfile.symbols.try_lock().ok()?;
    let gt = sem.global_table.try_lock().ok()?;
    gt.lookup_enum_class(def_uri, class_name).map(u32::from)
}

impl std::fmt::Display for Mc2Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.nc {
            write!(f, "{}(NC)", self.name)
        } else {
            write!(f, "{}", self.name)
        }
    }
}
