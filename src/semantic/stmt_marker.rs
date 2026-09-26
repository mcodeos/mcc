// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Statement-line tail-marker vocabulary — the closed `@word` slot (U305③).
//!
//! A trailing `@word(…)` on a statement line is a **closed** vocabulary, by
//! ruling 2026-09-26: an instance declaration line reads `@ncpin`/`@dnp`, a
//! connection line reads the relation words `@bridge`/`@couple`/`@clamp`/
//! `@star`. Any other word on those two lines reports
//! ([`errcodes::STMT_MARKER_UNKNOWN`]) instead of passing silently — before
//! U305③ an unknown word parsed, was claimed by nobody, and vanished with
//! zero diagnostics (`@nc_pin`, `@zzz`).
//!
//! Deliberately narrower than the attribute registry's open-vocabulary
//! doctrine (`semantic/basic/attr_keys.rs`): that doctrine governs the
//! declaration faces (body / pin row / spec / interface — the `@pair`,
//! `@drive`, `@class` rows), not statement tails. The two slots stay
//! separate.

use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::{errcodes, McIds};

/// The `@dnp` key (U305⑤) — the instance-line not-fitted marker. Declared
/// here because the vocabulary gate must admit it before its own reader
/// lands; the reader lives beside `nc_pin`'s.
pub(crate) const DNP_KEY: &str = "dnp";

/// Markers an instance declaration line reads.
const INSTANCE_KEYS: &[&str] = &[crate::semantic::nc_pin::NC_PIN_KEY, DNP_KEY];

/// Markers a connection line reads (the relation words — `pi::l1_edges`
/// plus the `@star` router hint).
const CONNECTION_KEYS: &[&str] = &["bridge", "couple", "clamp", "star"];

/// Which kind of statement line a marker check runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StmtLine {
    /// `CHIP d1 @ncpin(1,3)` — a declaration clause with its trailing
    /// attribute siblings.
    Instance,
    /// `p1 -> d1.1 @bridge(n1, n2)` — a connection clause with its trailing
    /// attribute siblings.
    Connection,
}

impl StmtLine {
    fn keys(self) -> &'static [&'static str] {
        match self {
            StmtLine::Instance => INSTANCE_KEYS,
            StmtLine::Connection => CONNECTION_KEYS,
        }
    }

    fn label(self) -> &'static str {
        match self {
            StmtLine::Instance => "instance line",
            StmtLine::Connection => "connection line",
        }
    }
}

/// The attribute node's key identifier, if it spells one. The single reader
/// for "what word is this `@word`" — [`crate::semantic::nc_pin::is_nc_pin_marker`]
/// shares the same decode through this function.
pub(crate) fn attribute_key(att: &AstNode) -> Option<String> {
    let id_node = att.get_sub_node()?;
    if !id_node.is_type(MCAST_ATT_ID) {
        return None;
    }
    let ids_node = id_node.get_sub_node()?;
    McIds::new(&ids_node).and_then(|ids| ids.get_primary_name())
}

/// The declaration line's `@dnp` flag, if written (U305⑤). A bare flag is the
/// only legal shape — `@dnp` means "this part is not fitted", declared by
/// being written — so a value list reports 5360 (the flag-arity shape, same
/// code the attribute registry's `AttrVocab::Flag` enforcement uses) and the
/// flag still counts as written.
///
/// Same sibling position as [`crate::semantic::nc_pin::read_nc_pins`] reads:
/// the marker rides as a next sibling of the `MCAST_DECLARE` node.
pub(crate) fn read_dnp(declare: &AstNode) -> bool {
    let mut cur = declare.get_next();
    while let Some(node) = cur {
        if node.is_type(MCAST_ATTRIBUTE) && attribute_key(&node).as_deref() == Some(DNP_KEY) {
            let has_values = node
                .get_sub_node()
                .and_then(|id| id.get_next())
                .is_some_and(|v| v.is_type(MCAST_ATT_VALUES));
            if has_values {
                dlog_error(
                    errcodes::ATTR_VALUE_NOT_IN_VOCABULARY,
                    &node,
                    &format!(
                        "Attribute '{DNP_KEY}' is a flag: it is declared by being written, and \
                         takes no value."
                    ),
                );
            }
            return true;
        }
        cur = node.get_next();
    }
    false
}

/// Report every trailing attribute of the clause whose key is outside the
/// line's vocabulary.
///
/// The trailing attributes ride as *next siblings* of the clause's phrase
/// head (`mc_net: mc_phrase mc_tattrs_opt`) — the same position
/// [`crate::semantic::nc_pin::read_nc_pins`] and `pi::collect_attrs` read —
/// so the walk starts at the head and follows `next`.
pub(crate) fn check_stmt_markers(clause: &AstNode, line: StmtLine) {
    let Some(head) = clause.get_sub_node() else {
        return;
    };
    let allowed = line.keys();
    let mut cur = head.get_next();
    while let Some(node) = cur {
        if node.is_type(MCAST_ATTRIBUTE) {
            if let Some(key) = attribute_key(&node) {
                if !allowed.contains(&key.as_str()) {
                    dlog_error(
                        errcodes::STMT_MARKER_UNKNOWN,
                        &node,
                        &errcodes::format_msg(
                            errcodes::STMT_MARKER_UNKNOWN,
                            &[&key, &line.label().to_string()],
                        ),
                    );
                }
            }
        }
        cur = node.get_next();
    }
}
