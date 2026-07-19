// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::ast_node::AstNode, ast::c_macros::*, ast::error::message::*,
    semantic::basic::mc_ids::McIds, semantic::component::mc_attr::McAttributes, McURI,
};

#[derive(Debug)]
pub struct McDefineDef {
    pub name: McIds,
    pub attrs: McAttributes,
    pub body: AstNode,
    pub uri: McURI,
    pub span: crate::ast::ast_semantic::Span,
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
            span: crate::ast::ast_semantic::Span {
                start: node.get_pos() as usize,
                end: (node.get_pos() + node.get_len()) as usize,
            },
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
