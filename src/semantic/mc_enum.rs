// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    McIds, McURI,
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

        // Class span = the entire `enum PKG { ... }` head (parent MCK_ENUM node)
        let class_start = node.get_pos();
        let class_end = class_start.saturating_add(node.get_len());

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
// Display implementation - concise format output
// ============================================================================

impl std::fmt::Display for McEnumDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let values_str: Vec<String> = self.values.iter().map(|v| v.name.to_string()).collect();
        writeln!(f, "Enum {}: {:?}", self.name, values_str)?;
        Ok(())
    }
}
