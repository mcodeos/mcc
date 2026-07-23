// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

pub mod mc_attr;
pub mod mc_layout;
pub mod mc_pins; // mc_pins/mod.rs includes mc_pins/dynamic.rs

use self::mc_attr::McAttributes;
use self::mc_layout::McLayout;
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
    semantic::basic::mc_bus::{McBus, McList},
    semantic::basic::mc_ids::McIds,
    semantic::basic::mc_param::McParamValue,
    semantic::mc_inst::McInst,
    semantic::mc_inst::McInstance,
    semantic::mc_inst::McInstances,
    McURI,
};
use std::sync::Arc;

/// A conditional pin block: a condition and its parsed pins
#[derive(Debug, Clone)]
pub struct CondPins {
    pub if_blocks: Vec<(McCondition, McPins)>,
    pub else_pins: Option<McPins>,
}

/// A conditional attribute block: a condition and its parsed attributes
#[derive(Debug, Clone)]
pub struct CondAttrs {
    pub if_blocks: Vec<(McCondition, McAttributes)>,
    pub else_attrs: Option<McAttributes>,
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
}

impl McComponent {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_COMPONENT
        // |- MCAST_NAME - MCAST_PARAMS (option) - MCAST_BODY
        let subnodes = node.get_sub_node()?;

        //1. new with name
        let comp_name = McIds::new(
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
        };

        //2. param
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

                // ── [P2-DEF] temporary probe: commented out
                // if comp_name.to_string().contains("US513") {
                //     let body_types: Vec<u16> = body_nodes.iter().map(|x| x.get_type()).collect();
                //     let pin_node_types: Vec<u16> =
                //         pin_nodes.iter().map(|x| x.get_type()).collect();
                //     eprintln!(
                //         "[P2-DEF] comp={} body_node_types={:?}",
                //         comp_name.to_string(), body_types
                //     );
                //     eprintln!(
                //         "[P2-DEF] comp={} pin_nodes_found={} pin_node_types={:?} static_count_after_parse={}",
                //         comp_name.to_string(), pin_nodes.len(), pin_node_types,
                //         new_comp.pins.count()
                //     );
                // }

                //5. functions (parse header + body with context)
                // Use raw pointer to avoid conflicting borrows of new_comp
                let context =
                    unsafe { &mut *(&mut new_comp as *mut McComponent) as &mut dyn HasFindInst };
                body_nodes
                    .iter()
                    .filter(|x| x.is_type(MCAST_FUNCTION))
                    .for_each(|x| new_comp.funcs.parse(&x, context));

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
                if child_type == MCAST_COND_IF || child_type == MCAST_COND_ELSE {
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
                        cond_pins.push(CondPins {
                            if_blocks: if_pin_blocks,
                            else_pins,
                        });

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
                        cond_attrs.push(CondAttrs {
                            if_blocks: if_attr_blocks,
                            else_attrs,
                        });
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
    fn find_inst(&self, _id: &str) -> Option<McInstance> {
        None
    }

    fn find_inst_mut(&mut self, _id: &str) -> Option<&mut crate::McInstance> {
        None
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

    fn parse_declare(&mut self, _node: &AstNode) -> Vec<McInstance> {
        Vec::new()
    }

    fn add_component(&mut self, _name: String, _comp: Mc2Component) -> Option<McPhrase> {
        None
    }

    fn add_module(
        &mut self,
        _name: String,
        _module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        None
    }

    fn gen_anon_name(&mut self, _classname: &str) -> String {
        String::new()
    }

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
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

    /// Find the externally-exposed interface named id
    pub fn find_port(&self, id: &str) -> Option<McPhrase> {
        if let Some(found) = self.base.pins.find_pin(id) {
            let full_name = format!("{}.{}", self.name, found);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(McBus::new(&full_name)),
            ))));
        }
        None
    }
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
