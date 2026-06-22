// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_func::McFunctions;
use crate::{
    ast_node::AstNode, c_macros::*, error::message::*,
    core::component::mc_attr::McAttributes,
    mc_ids::McIds, mc_param::McParamDeclares, McURI,
};
use std::sync::Arc;

#[derive(Debug)]
pub struct McDefineDef {
    pub name: McIds,
    pub attrs: McAttributes,
    pub body: AstNode,
    pub uri: McURI,
}

impl McDefineDef {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);

        // 1. Create basic structure
        let mut ret = Self {
            name: McIds::new(
                &subnodes
                    .iter()
                    .find(|x| x.is_type(MCAST_NAME))
                    .expect(MISSING_SUBNODE)
                    .get_sub_node() // ids
                    .expect(MISSING_SUBNODE),
            )?,
            attrs: McAttributes::new(),
            body: subnodes
                .iter()
                .find(|x| x.is_type(MCAST_BODY))
                .expect(MISSING_SUBNODE),
            uri: uri.clone(),
        };

        // 2. Parse attributes
        if !ret.body.get_sub_node().is_none() {
            ret.body
                .get_sub_node()
                .unwrap()
                .iter()
                .filter(|x| x.is_type(MCAST_ATTRIBUTE))
                .for_each(|x| ret.attrs.parse(&x));
        }
        
        Some(ret)
    }

}