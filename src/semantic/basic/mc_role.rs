// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    semantic::{component::mc_attr::McAttributes, component::mc_pins::McPins},
    McIds,
};

#[derive(Debug, Clone)]
pub struct McRole {
    pub name: McIds,
    pub attrs: McAttributes,
    pub pins: McPins,
    pub body: AstNode,
}

impl McRole {
    pub fn new(node: &AstNode) -> Option<Self> {
        // role DCE {
        //    |- MCAST_IDS (ids: DCE)  <- Note: NOT MCAST_NAME!
        //    |- MCAST_ATTRIBUTE (name = "...")
        //    |- MCAST_ATTRIBUTE (peer = DTE)
        //    |- MCAST_ATTRIBUTE_PIN (pins = [...])
        //    |- MCAST_BODY (optional)
        // }

        let subnodes = node.get_sub_node()?;

        // Get role name - from MCAST_IDS node
        let ids_node = subnodes.iter().find(|x| x.is_type(MCAST_IDS))?;
        let role_name = McIds::new(&ids_node)?;

        // Find MCAST_BODY
        let body_node_opt = subnodes.iter().find(|x| x.is_type(MCAST_BODY));

        let mut ret = Self {
            name: role_name,
            attrs: McAttributes::new(),
            pins: McPins::new(),
            body: match body_node_opt {
                Some(n) => n.clone(),
                None => node.clone(),
            },
        };

        // Parse attributes and pins
        for child in subnodes.iter() {
            match child.get_type() {
                MCAST_ATTRIBUTE => {
                    ret.attrs.parse(&child);
                }
                MCAST_ATTRIBUTE_PIN | MCAST_ATTRIBUTE_PINADD => {
                    ret.pins.parse(&child);
                }
                _ => {}
            }
        }

        Some(ret)
    }

    pub fn get_attr(&self, id: &str) -> Option<&crate::semantic::component::mc_attr::McAttribute> {
        self.attrs.find(&McIds::from(id))
    }
}

impl std::fmt::Display for McRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pins)
    }
}
