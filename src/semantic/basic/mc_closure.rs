// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_bus::McBus;
use super::mc_param::McParamDeclares;
use super::mc_phrase::McPhrase;
use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::core::mc_func::HasFindInst;

/// Closure
#[derive(Debug, Clone)]
pub struct McClosure {
    /// Parameter declarations
    pub params: McParamDeclares,
    /// Output interface
    pub right: Vec<McBus>,
    /// Closure body (connection lines) - simplified with Vec<McPhrase>
    pub body: Vec<McPhrase>,
}

impl McClosure {
    /// Parse closure from an AST node
    pub fn parse(node: &AstNode, context: &mut dyn HasFindInst) -> Option<Self> {
        let subnode = node
            .get_sub_node()
            .expect(crate::ast::error::message::MISSING_SUBNODE);

        let mut params = McParamDeclares::new();
        let mut body_lines: Vec<McPhrase> = Vec::new();

        for each in subnode.iter() {
            match each.get_type() {
                MCAST_PARAMS => {
                    params.parse(&each);
                }

                MCAST_BODY => {
                    if let Some(body_nodes) = each.get_sub_node() {
                        for body_node in body_nodes.iter() {
                            match body_node.get_type() {
                                MCAST_NET => {
                                    if let Some(net_sub) = body_node.get_sub_node() {
                                        if let Some(phrase) = McPhrase::new(&net_sub, context) {
                                            body_lines.push(phrase);
                                        }
                                    }
                                }
                                _ => {
                                    if let Some(phrase) = McPhrase::new(&body_node, context) {
                                        body_lines.push(phrase);
                                    }
                                }
                            }
                        }
                    }
                }

                _ => {
                    if let Some(phrase) = McPhrase::new(&each, context) {
                        body_lines.push(phrase);
                    }
                }
            }
        }

        let right = if let Some(last_line) = body_lines.last() {
            last_line.get_right()
        } else {
            vec![]
        };

        Some(McClosure {
            params,
            right,
            body: body_lines,
        })
    }
}
