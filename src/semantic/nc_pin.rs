// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Instance-level per-pin NC marker (`@ncpin(…)`) — the AST side.
//!
//! NC layer ③: "this pin, of *this* instance, is intentionally not connected".
//! The anchor is the declaration line's trailing attribute
//! (`CHIP d1 @ncpin(1,3)`): the grammar reaches it through
//! `mc_net: mc_phrase mc_tattrs_opt` with `mc_phrase: mc_declare_a1`, so the
//! marker is the **next sibling of the `MCAST_DECLARE` node** inside the net
//! clause — not a child of it, and not a clause of its own. (`CHIP d3, d4
//! @ncpin(1)`, the multi-instance list form, is outside that production and
//! stays a syntax error; measured 2026-09-15.)
//!
//! Everything here reads the parsed AST; nothing re-parses a display string.
//! The marker is a **pure suppression marker**: it never touches the netlist,
//! the connections, the BOM or the viz. Its only consumer is the "not
//! connected" diagnostic family, through `InstEntry::nc_marked`.

use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::basic::mc_expr::McExpression;
use crate::{errcodes, McOpd};

/// The marker key. One spelling, one home: this reader and the lapper's
/// enum-reference walk (`db::infra::mc_code::lapper_enum_refs`) both key off
/// it, because a dotted operand *inside* the marker is a pin path
/// (`@ncpin(MIC.P)`), not a class reference.
pub(crate) const NC_PIN_KEY: &str = "ncpin";

/// What one `@ncpin(…)` operand stands for, decoded from the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NcPinKind {
    /// Written pin identities, already expanded the way the language reads a
    /// group: `MIC{P,N}` → `MIC.P` / `MIC.N`, `[A,B]` → `A` / `B`. Expansion
    /// goes through [`McIds::expand`], so a vector suffix (`c[1:2]`) lands as
    /// the instance names it denotes, exactly as the declaration side reads it.
    Names(Vec<String>),
    /// An inclusive numeric range (`1:3` — both ends written in the marker).
    Range(i64, i64),
}

/// One operand of the marker, with the source offset it was written at (the
/// anchor every diagnostic about that operand uses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NcPinSpec {
    pub kind: NcPinKind,
    pub offset: u32,
}

/// The `@ncpin(…)` trailer of an instance declaration, if written.
///
/// An empty result means "no marker" — a bare `@ncpin` (no pin list) is
/// reported here (E3158) and contributes no specs.
pub(crate) fn read_nc_pins(declare: &AstNode) -> Vec<NcPinSpec> {
    let mut cur = declare.get_next();
    while let Some(node) = cur {
        if node.is_type(MCAST_ATTRIBUTE) && is_nc_pin_marker(&node) {
            return specs_of(&node);
        }
        cur = node.get_next();
    }
    Vec::new()
}

/// Is this attribute node the instance NC pin marker (`@ncpin(…)`)?
///
/// Crate-visible because the symbol lapper must recognise it too: an operand of
/// the marker is a pin path, and a dotted one (`@ncpin(MIC.P)`) is
/// indistinguishable from an enum reference to the lapper's walk, which would
/// then report the false `INST_CLASS_UNRESOLVED` on `MIC`. Both readers key off
/// this one predicate so the spelling has a single owner.
pub(crate) fn is_nc_pin_marker(att: &AstNode) -> bool {
    key_is(att, NC_PIN_KEY)
}

/// True when the attribute node's key identifier is `want`.
fn key_is(att: &AstNode, want: &str) -> bool {
    crate::semantic::stmt_marker::attribute_key(att).as_deref() == Some(want)
}

/// Decode the marker's value list. `MCAST_ATTRIBUTE` children are the pair
/// `[MCAST_ATT_ID, MCAST_ATT_VALUES]`; a bare flag (`@ncpin`) has no value
/// node at all, which is the missing-pin-list case.
///
/// [`AstNode::iter`] walks the `next` chain, i.e. *siblings* — so the operands
/// are reached by stepping into the first child and iterating from there, the
/// same way `db::infra::mc_code::lapper_enum_refs` reads attribute values.
fn specs_of(att: &AstNode) -> Vec<NcPinSpec> {
    let mut specs = Vec::new();
    let values = att
        .get_sub_node()
        .and_then(|id| id.get_next())
        .filter(|n| n.is_type(MCAST_ATT_VALUES));
    let values = values.as_ref().and_then(|v| v.get_sub_node());
    let Some(first_value) = values else {
        // No value node at all (bare `@ncpin`) or an empty list — both mean
        // "the marker lists no pins".
        dlog_error(
            errcodes::INST_NC_PIN_LIST_MISSING,
            att,
            &errcodes::format_msg(errcodes::INST_NC_PIN_LIST_MISSING, &[&NC_PIN_KEY]),
        );
        return specs;
    };
    for value in first_value.iter() {
        match decode(&value) {
            Some(kind) => specs.push(NcPinSpec {
                kind,
                offset: value.get_pos(),
            }),
            None => dlog_error(
                errcodes::INST_NC_PIN_VALUE_INVALID,
                &value,
                &errcodes::format_msg(
                    errcodes::INST_NC_PIN_VALUE_INVALID,
                    &[&NC_PIN_KEY, &value.to_string().unwrap_or_default()],
                ),
            ),
        }
    }
    specs
}

/// One written operand → its kind. Anything that is not a pin identity
/// (float, string, keyword constant, unit value, arithmetic) is `None`.
fn decode(node: &AstNode) -> Option<NcPinKind> {
    match McExpression::new(node)? {
        // A number is a pin id: components declare pins as `io 1 = A`, and the
        // flat table registers that pad under the id `1`.
        McExpression::Int(int) => Some(NcPinKind::Names(vec![int.value.to_string()])),
        McExpression::Variable(opd) => match &opd {
            McOpd::Id(ids) => Some(NcPinKind::Names(ids.expand())),
            // `this` / `pins` / `_` address no pin of the instance.
            _ => None,
        },
        McExpression::Slice(from, to) => match (&*from, &*to) {
            (McExpression::Int(from), McExpression::Int(to)) => {
                Some(NcPinKind::Range(from.value, to.value))
            }
            _ => None,
        },
        // `[A, B]` — the list's members, read flat (the list spelling carries no
        // further meaning here; the connection face cannot spell it either).
        McExpression::Set(items) => {
            let mut names = Vec::new();
            for item in &items {
                match item {
                    McExpression::Variable(McOpd::Id(ids)) => names.extend(ids.expand()),
                    _ => return None,
                }
            }
            (!names.is_empty()).then_some(NcPinKind::Names(names))
        }
        _ => None,
    }
}
