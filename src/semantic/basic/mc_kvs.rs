// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_expr::McExpression;
use super::mc_literal::{McConst, McLiteral};
use super::mc_opd::McOpd;
use crate::{
    ast::{macros::*, node::AstNode},
    semantic::component::mc_attr::{McAttrVal, McAttribute},
    McIds,
};

#[derive(Debug, Clone)]
pub enum KVSValue {
    Const(McConst),
    Square(Vec<McAttrVal>),
    Nested(Vec<McKVS>),
}

impl std::fmt::Display for KVSValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KVSValue::Const(c) => write!(f, "{c}"),
            KVSValue::Square(vals) => {
                let inner: Vec<String> = vals.iter().map(|v| format!("{v}")).collect();
                write!(f, "[{}]", inner.join(", "))
            }
            KVSValue::Nested(kvs_list) => {
                let inner: Vec<String> = kvs_list.iter().map(|k| format!("{k}")).collect();
                write!(f, "[{}]", inner.join(", "))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct McKVS {
    pub key: McIds,
    pub value: KVSValue,
}

impl std::fmt::Display for McKVS {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.key, self.value)
    }
}

impl McKVS {
    /// Parse one `key: value` entry of an attribute value list.
    ///
    /// `node` is the entry itself: a `key:value` colon, a `key:lo ~ hi` tilde
    /// (which wraps its lower bound's colon), or an expression wrapping either.
    /// The key is an operand, so a numeric slice such as `1:10` is not an entry.
    pub fn new(node: &AstNode) -> Option<Self> {
        let unwrapped = node
            .is_type(MCAST_EXPRESSION)
            .then(|| node.get_sub_node())
            .flatten();
        let entry = unwrapped.as_ref().unwrap_or(node);

        match entry.get_type() {
            MCAST_OPD_COLON => {
                let key_node = entry.get_sub_node()?;
                let value_node = key_node.get_next()?;
                let value = Self::value_of(&value_node)?;
                Self::pair(&key_node, value)
            }

            MCAST_OPD_TILDE => {
                let colon = entry.get_sub_node()?;
                let dotted_key = colon.get_sub_node()?;
                let lower = dotted_key.get_next()?;
                let upper = colon.get_next()?;
                let range = McExpression::Range(
                    Box::new(McExpression::new(&lower)?),
                    Box::new(McExpression::new(&upper)?),
                );
                let value = KVSValue::Square(vec![McAttrVal::AttrExpr(range)]);
                Self::pair(&dotted_key, value)
            }

            _ => None,
        }
    }

    fn pair(key_node: &AstNode, value: KVSValue) -> Option<Self> {
        if !key_node.is_type(MCAST_OPD) {
            return None;
        }
        let key = McIds::new(&key_node.get_sub_node()?)?;
        Some(Self { key, value })
    }

    fn value_of(node: &AstNode) -> Option<KVSValue> {
        match node.get_type() {
            // `key:[...]` holds a list; each member is an entry of its own.
            MCAST_OPD_SQUARE_VEC => {
                let sub = node.get_sub_node()?;
                let members = McAttribute::extract_kvs_from_iter(sub.iter());
                (!members.is_empty()).then_some(KVSValue::Square(members))
            }

            MCAST_OPD_COLON | MCAST_RANGE_PLUSMINUS => McExpression::new(node)
                .map(|expr| KVSValue::Square(vec![McAttrVal::AttrExpr(expr)])),

            // A bare name on the right (`voltage:VCC`) is a reference, not a literal.
            MCAST_OPD => McOpd::new(node).map(|opd| {
                let span = (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                KVSValue::Square(vec![McAttrVal::AttrVariable(opd, Some(span))])
            }),

            _ => McLiteral::new(node).map(|literal| match literal {
                McLiteral::Const(const_val) => KVSValue::Const(const_val),
                other => KVSValue::Square(vec![McAttrVal::AttrLiteral(other)]),
            }),
        }
    }
}
