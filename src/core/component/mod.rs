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
    basic::mc_conds::McConds,
    basic::mc_endpoint::{McEndpoint, McInstanceRef},
    basic::mc_param::McParamDeclares,
    basic::mc_phrase::McPhrase,
    mc_func::HasFindInst,
    mc_func::McFunctions,
};
use crate::{
    ast::ast_node::AstNode,
    ast::c_macros::*,
    ast::error::message::*,
    core::basic::mc_bus::{McBus, McList},
    core::basic::mc_ids::McIds,
    core::basic::mc_param::McParamValue,
    core::mc_inst::McInst,
    core::mc_inst::McInstance,
    core::mc_inst::McInstances,
    McURI,
};
use std::sync::Arc;

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
}

impl McComponent {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_COMPONENT
        // |- MCAST_NAME - MCAST_PARAMS (option) - MCAST_BODY
        let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);

        //1. new with name
        let comp_name = McIds::new(
            &subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))
                .expect(MISSING_SUBNODE)
                .get_sub_node() // ids
                .expect(MISSING_SUBNODE),
        )?;

        let mut newComp = Self {
            name: comp_name.clone(),
            params: McParamDeclares::new(),
            attrs: McAttributes::new(),
            pins: McPins::new(),
            funcs: McFunctions::new(),
            insts: McInstances::new(),
            uri: uri.clone(),
            layout: McLayout::empty(),
        };

        //2. param
        let _ = &subnodes
            .iter()
            .find(|x| x.is_type(MCAST_PARAMS))
            .map(|param_node| newComp.params.parse(&param_node));

        //3. body
        if let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) {
            if let Some(body_nodes) = body.get_sub_node() {
                //3. attributes
                body_nodes
                    .iter()
                    .filter(|x| x.is_type(MCAST_ATTRIBUTE))
                    .for_each(|x| {
                        if let Some(built_layout) = mc_layout::McLayout::new(&x) {
                            newComp.layout = built_layout;
                        } else {
                            newComp.attrs.parse(&x);
                        }
                    });

                //4. pins
                let pin_nodes: Vec<_> = body_nodes
                    .iter()
                    .filter(|x| x.is_type(MCAST_ATTRIBUTE_PIN) || x.is_type(MCAST_ATTRIBUTE_PINADD))
                    .collect();
                pin_nodes.iter().for_each(|x| newComp.pins.parse(x));

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
                //         newComp.pins.count()
                //     );
                // }

                //5. functions (parse header + body with context)
                // Use raw pointer to avoid conflicting borrows of newComp
                let context =
                    unsafe { &mut *(&mut newComp as *mut McComponent) as &mut dyn HasFindInst };
                body_nodes
                    .iter()
                    .filter(|x| x.is_type(MCAST_FUNCTION))
                    .for_each(|x| newComp.funcs.parse(&x, context));

                //6. todo: role
                //7. conds
                Self::parse_cond_pins_with_defaults(&mut newComp.pins, &body, &newComp.params);
                //8. todo: net not supported
            }
        }

        Some(newComp)
    }

    fn parse_cond_pins_with_defaults(
        pins: &mut McPins,
        body_node: &AstNode,
        params: &McParamDeclares,
    ) {
        let default_params = params.get_params_with_defaults();

        if default_params.is_empty() {
            return;
        }

        if let Some(body_subnodes) = body_node.get_sub_node() {
            for child in body_subnodes.iter() {
                let child_type = child.get_type();
                if child_type == MCAST_COND_IF || child_type == MCAST_COND_ELSE {
                    if let Some(conds) = McConds::new(&child) {
                        if let Some(selected_block) = conds.evaluate(&default_params) {
                            let block_type = selected_block.get_type();
                            if block_type == MCAST_ATTRIBUTE_PIN
                                || block_type == MCAST_ATTRIBUTE_PINADD
                            {
                                pins.parse(&selected_block);
                            }
                        }
                    }
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

    fn add_label(&mut self, name: String) -> Option<McPhrase> {
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
        _module: crate::core::module::Mc2Module,
    ) -> Option<McPhrase> {
        None
    }

    fn gen_anon_name(&mut self, _classname: &str) -> String {
        String::new()
    }

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
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
