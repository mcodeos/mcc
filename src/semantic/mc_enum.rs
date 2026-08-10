// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::{basic::mc_phrase::McPhrase, mc_func::HasFindInst};
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    McIds, McInstance, McURI,
};

#[derive(Debug)]
pub struct McEnumValue {
    pub name: McIds,
    /// Byte span [start, end) of the value identifier within the source file.
    pub span: [u32; 2],
}

#[derive(Debug)]
pub struct McEnumDef {
    pub name: McIds,
    /// Byte span of the `enum PKG {` declaration (start of `enum` keyword
    /// through end of the declaration header — i.e. position of the enclosing
    /// `MCK_ENUM` node). Used by gotodef as the jump-to target for
    /// `enum_class_ref`.
    pub span: [u32; 2],
    pub values: Vec<McEnumValue>,
    pub uri: McURI,
}

impl McEnumDef {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_ENUM
        // |- MCAST_NAME - MCAST_ENUM_VALUES
        //     |- MCAST_ID/MCAST_IDS    |- MCAST_ID*
        let subnodes = match node.get_sub_node() {
            Some(nodes) => nodes,
            None => {
                dlog_error(1001, node, "Missing subnodes for enum");
                return None;
            }
        };

        //1. Get enum name
        let name_node: AstNode = match subnodes.iter().find(|x: &AstNode| x.is_type(MCAST_NAME)) {
            Some(node) => node,
            None => {
                dlog_error(1001, &subnodes, "Missing name for enum");
                return None;
            }
        };

        let name_ids = match name_node.get_sub_node() {
            Some(nodes) => nodes,
            None => {
                dlog_error(1001, &name_node, "Missing name ids for enum");
                return None;
            }
        };

        let name = McIds::new(&name_ids)?;

        //2. Get enum values
        let body_node: AstNode = match subnodes
            .iter()
            .find(|x: &AstNode| x.is_type(MCAST_ENUM_VALUES))
        {
            Some(node) => node,
            None => {
                dlog_error(1001, &subnodes, "Missing values for enum");
                return None;
            }
        };

        let values: Vec<McEnumValue> = if let Some(sub_nodes) = body_node.get_sub_node() {
            let sub_nodes: &AstNode = &sub_nodes;
            sub_nodes
                .iter()
                .filter_map(|opdc_node: AstNode| {
                    let name = McIds::new(&opdc_node)?;
                    let start = opdc_node.get_pos();
                    let end = start.saturating_add(opdc_node.get_len());
                    Some(McEnumValue {
                        name,
                        span: [start, end],
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        // Class span = the enum name (MCAST_IDS), not the entire node
        let class_start = name_ids.get_pos();
        let class_end = class_start.saturating_add(name_ids.get_len());

        //3. Create enum definition
        Some(Self {
            name,
            span: [class_start, class_end],
            values,
            uri: uri.clone(),
        })
    }
}

// ============================================================================
// HasFindInst for McEnumDef — namespace lookup (Phase 4.5)
// ============================================================================

impl HasFindInst for McEnumDef {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    fn find_inst_mut(&mut self, _id: &str) -> Option<&mut crate::McInstance> {
        None // Enum has no mutable instances at Pass1
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        let enum_name = self.name.to_string();
        for value in &self.values {
            if value.name.to_string() == id {
                let span = value.span[0] as usize..value.span[1] as usize;
                return Some((
                    McInstance::EnumVal {
                        enum_name,
                        value_name: id.to_string(),
                        span: Some(span.clone()),
                    },
                    Some(span),
                ));
            }
        }
        None
    }

    fn add_label_at(
        &mut self,
        _name: String,
        _span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        None // No-ops for enum (no net statements)
    }

    fn add_component(
        &mut self,
        _name: String,
        _comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase> {
        None
    }

    fn add_module(
        &mut self,
        _name: String,
        _module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        None
    }

    fn add_bus(&mut self, _name: String, _members: Vec<String>) -> Option<McPhrase> {
        None
    }

    fn add_list(&mut self, _name: String, _members: Vec<String>) -> Option<McPhrase> {
        None
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

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
    }

    fn uri(&self) -> &crate::McURI {
        &self.uri
    }

    fn parse_declare(&mut self, _node: &AstNode) -> Vec<McInstance> {
        Vec::new()
    }

    fn gen_anon_name(&mut self, _classname: &str) -> String {
        String::new()
    }
}

// ============================================================================
// Display implementation - concise format output
// ============================================================================

impl std::fmt::Display for McEnumDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let values_str: Vec<String> = self.values.iter().map(|v| v.name.to_string()).collect();
        writeln!(f, "Enum {}: {:?}", self.name, values_str)?;
        Ok(())
    }
}
