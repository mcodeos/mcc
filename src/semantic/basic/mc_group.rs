// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_phrase::McPhrase;
use crate::ast::ast_node::AstNode;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::mc_func::HasFindInst;
use crate::semantic::mc_inst::McInstance;
use crate::semantic::validation::ledger::{self, LedgerAction, LedgerEntry, LedgerKind};
use tracing::warn;

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
            warn!(target: "mcc::group", "Left shape mismatch in Group");
            // ★ Ledger (resolve-gate §1.2③): shape mismatch is silently
            // absorbed into a `<error:shape_mismatch>` placeholder bus. Record it
            // (no uri/span here — deep in instantiation — so the opds display
            // form carries identity and request-scoped dedup collapses re-probes
            // of the same group).
            ledger::record(
                LedgerEntry::new(
                    LedgerKind::Fallback,
                    group_form(&self.opds),
                    "mc_group.rs:70 left shape_mismatch",
                )
                .with_action(LedgerAction::Silent),
            );
            vec![McBus::new("<error:shape_mismatch>")]
        }
    }

    /// Get right interface
    pub fn get_right(&self) -> Vec<McBus> {
        if self.right_match && !self.opds.is_empty() {
            self.opds[0].get_right()
        } else {
            warn!(target: "mcc::group", "Right shape mismatch in Group");
            ledger::record(
                LedgerEntry::new(
                    LedgerKind::Fallback,
                    group_form(&self.opds),
                    "mc_group.rs:88 right shape_mismatch",
                )
                .with_action(LedgerAction::Silent),
            );
            vec![McBus::new("<error:shape_mismatch>")]
        }
    }
}

/// Readable ledger form for a group: `(opd1, opd2, …)`. Display only — the
/// ledger records the fallback annotation, never re-parses it.
fn group_form(opds: &[McPhrase]) -> String {
    let inner: Vec<String> = opds.iter().map(|o| o.to_string()).collect();
    format!("({})", inner.join(", "))
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
