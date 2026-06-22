// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    builder::diagnostic::dlog_error,
    McIds, McURI,
};

#[derive(Debug)]
pub struct McEnumDef {
    pub name: McIds,
    pub values: Vec<McIds>,
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

        let values = if let Some(sub_nodes) = body_node.get_sub_node() {
            let sub_nodes: &AstNode = &sub_nodes;
            sub_nodes
                .iter()
                .filter_map(|opdc_node: AstNode| McIds::new(&opdc_node))
                .collect()
        } else {
            Vec::new()
        };

        //3. Create enum definition
        Some(Self {
            name,
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
        let values_str: Vec<String> = self.values.iter().map(|v| v.to_string()).collect();
        writeln!(f, "Enum {}: {:?}", self.name, values_str)?;
        Ok(())
    }
}
