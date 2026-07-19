// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_phrase::McPhrase;
use crate::ast::ast_node::AstNode;
use crate::db::diagnostic::diagnostic::dlog_trace;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::mc_func::HasFindInst;
use crate::semantic::mc_inst::McInstance;

/// Group
#[derive(Debug, Clone)]
pub struct McGroup {
    pub opds: Vec<McPhrase>,
    pub left_match: bool,
    pub right_match: bool,
}

impl McGroup {
    /// Parse group from AST node
    pub fn parse(node: &AstNode, context: &mut dyn HasFindInst) -> Option<Self> {
        Self::parse_internal(node, context, |n, ctx| McPhrase::new(n, ctx))
    }

    /// Internal parse function, uses callback to avoid circular dependency
    fn parse_internal<F>(
        node: &AstNode,
        context: &mut dyn HasFindInst,
        parse_phrase: F,
    ) -> Option<Self>
    where
        F: Fn(&AstNode, &mut dyn HasFindInst) -> Option<McPhrase>,
    {
        let subnode = node
            .get_sub_node()
            .expect(crate::ast::error::message::MISSING_SUBNODE);

        let mut opds: Vec<McPhrase> = subnode
            .iter()
            .map(|line| parse_phrase(&line, context))
            .collect::<Option<Vec<_>>>()?;

        let (left_match, right_match) = group_shape_match_and_upgrade(&mut opds);

        Some(McGroup {
            opds,
            left_match,
            right_match,
        })
    }

    /// Get left interface
    pub fn get_left(&self) -> Vec<McBus> {
        if self.left_match && !self.opds.is_empty() {
            self.opds[0].get_left()
        } else {
            dlog_trace(1190, "Left shape mismatch in Group");
            vec![McBus::new("<error:shape_mismatch>")]
        }
    }

    /// Get right interface
    pub fn get_right(&self) -> Vec<McBus> {
        if self.right_match && !self.opds.is_empty() {
            self.opds[0].get_right()
        } else {
            dlog_trace(1195, "Right shape mismatch in Group");
            vec![McBus::new("<error:shape_mismatch>")]
        }
    }
}

/// Helper: check group shape match and upgrade
fn group_shape_match_and_upgrade(opds: &mut Vec<McPhrase>) -> (bool, bool) {
    fn get_size(elements: &[McBus]) -> usize {
        elements.iter().map(|each| each.size()).sum()
    }

    if let Some(first_determined) = opds.iter().find(|phrase| {
        !matches!(
            phrase,
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(_),
                ..
            }))
        )
    }) {
        let left_size = get_size(&first_determined.get_left());
        let right_size = get_size(&first_determined.get_right());
        (
            opds.iter().all(|each| match each {
                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Label(_),
                    ..
                })) => true,
                _ => get_size(&each.get_left()) == left_size,
            }),
            opds.iter().all(|each| match each {
                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Label(_),
                    ..
                })) => true,
                _ => get_size(&each.get_right()) == right_size,
            }),
        )
    } else {
        (true, true)
    }
}
