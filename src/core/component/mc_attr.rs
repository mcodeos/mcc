// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::{ast_node::AstNode, c_macros::*, error::message::*},
    builder::diagnostic::dlog_error,
    core::{
        basic::mc_expr::McExpression, basic::mc_kvs::McKVS, basic::mc_literal::McLiteral,
        basic::mc_uval::McUnitValue,
    },
    McIds, McOpd,
};
use std::vec;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McAttributes {
    attributes: Vec<McAttribute>,
}

impl McAttributes {
    pub fn new() -> Self {
        Self {
            attributes: Vec::new(),
        }
    }

    pub fn parse(&mut self, node: &AstNode) {
        if let Some(attribute) = McAttribute::new(node) {
            self.push(attribute);
        }
    }

    pub fn push(&mut self, attribute: McAttribute) {
        self.attributes.push(attribute);
    }

    pub fn len(&self) -> usize {
        self.attributes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, McAttribute> {
        self.attributes.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, McAttribute> {
        self.attributes.iter_mut()
    }

    pub fn get(&self, index: usize) -> Option<&McAttribute> {
        self.attributes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut McAttribute> {
        self.attributes.get_mut(index)
    }

    pub fn find(&self, id: &McIds) -> Option<&McAttribute> {
        self.attributes.iter().find(|attr| &attr.id == id)
    }

    pub fn find_mut(&mut self, id: &McIds) -> Option<&mut McAttribute> {
        self.attributes.iter_mut().find(|attr| &attr.id == id)
    }
}

// Implement Deref and DerefMut to get Vec-like inherited behavior
impl std::ops::Deref for McAttributes {
    type Target = Vec<McAttribute>;

    fn deref(&self) -> &Self::Target {
        &self.attributes
    }
}

impl std::ops::DerefMut for McAttributes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.attributes
    }
}

// Implement IntoIterator for easy iteration
impl IntoIterator for McAttributes {
    type Item = McAttribute;
    type IntoIter = vec::IntoIter<McAttribute>;

    fn into_iter(self) -> Self::IntoIter {
        self.attributes.into_iter()
    }
}

impl<'a> IntoIterator for &'a McAttributes {
    type Item = &'a McAttribute;
    type IntoIter = std::slice::Iter<'a, McAttribute>;

    fn into_iter(self) -> Self::IntoIter {
        self.attributes.iter()
    }
}

impl<'a> IntoIterator for &'a mut McAttributes {
    type Item = &'a mut McAttribute;
    type IntoIter = std::slice::IterMut<'a, McAttribute>;

    fn into_iter(self) -> Self::IntoIter {
        self.attributes.iter_mut()
    }
}

#[derive(Debug, Clone)]
pub enum McAttrVal {
    AttrLiteral(McLiteral),
    AttrVariable(McOpd),
    AttrExpr(McExpression),
    Attributes(Vec<McAttribute>),
    KVS(McKVS),
}

impl std::fmt::Display for McAttrVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McAttrVal::AttrLiteral(lit) => write!(f, "{lit}"),
            McAttrVal::AttrVariable(opd) => write!(f, "{opd}"),
            McAttrVal::AttrExpr(expr) => write!(f, "{expr}"),
            McAttrVal::Attributes(attrs) => {
                let inner: Vec<String> = attrs.iter().map(|a| format!("{a}")).collect();
                write!(f, "[{}]", inner.join(", "))
            }
            McAttrVal::KVS(kvs) => write!(f, "{kvs}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct McAttribute {
    pub no: i32,
    pub id: McIds,
    pub values: Vec<McAttrVal>,
    /// Source span of the key identifier (for LSP goto-definition).
    pub key_span: Option<std::ops::Range<usize>>,
}

impl McAttribute {
    pub fn new(node: &AstNode) -> Option<Self> {
        // MCAST_ATTRIBUTE
        // |- MCAST_ATT_ID --- MCAST_ATT_VALUES

        //1. Check child node: exists + type
        let subnode1 = node.get_sub_node().expect(MISSING_SUBNODE);
        let subnode2 = subnode1.get_next().expect(MISSING_SUBNODE);

        if !subnode1.is_type(MCAST_ATT_ID) {
            dlog_error(601, &subnode1, TYPE_MISMATCH);
            return None;
        }
        //2. Check child node content: exists
        let snode1_ids_node = subnode1.get_sub_node().expect(MISSING_SUBNODE);

        let attr_id = McIds::new(&snode1_ids_node)?;
        let key_span = Some(
            (snode1_ids_node.get_pos() as usize)
                ..((snode1_ids_node.get_pos() + snode1_ids_node.get_len()) as usize),
        );

        // Special case: if value is MCAST_OPD_SQUARE_VEC containing colon expressions,
        // treat the attribute id as the KVS key
        // volt:[low:0V ~ 0.7V, high:0.7V ~ 5V] structure
        if subnode2.get_type() == MCAST_OPD_SQUARE_VEC {
            if let Some(kvs_values) = Self::parse_square_vec_kvs(&attr_id, &subnode2) {
                return Some(Self {
                    no: 0,
                    id: attr_id,
                    values: kvs_values,
                    key_span,
                });
            }
        }

        Some(Self {
            no: 0,
            id: attr_id,
            values: McAttribute::new_attr_values(&subnode2)?,
            key_span,
        })
    }

    fn parse_square_vec_kvs(_attr_id: &McIds, square_vec: &AstNode) -> Option<Vec<McAttrVal>> {
        let sub = square_vec.get_sub_node()?;
        let kvs_list = Self::extract_kvs_from_iter(sub.iter());

        if kvs_list.is_empty() {
            return None;
        }
        Some(kvs_list)
    }

    fn extract_kvs_from_iter(iter: impl Iterator<Item = AstNode>) -> Vec<McAttrVal> {
        iter.filter_map(|child| {
            if child.get_type() == MCAST_OPD {
                if let Some(child_sub) = child.get_sub_node() {
                    if child_sub.get_type() == MCAST_OPD_COLON {
                        if let Some(kvs) = McKVS::new(&child) {
                            return Some(McAttrVal::KVS(kvs));
                        }
                    }
                }
            }
            None
        })
        .collect()
    }

    fn try_parse_kvs_expression(expr_node: &AstNode) -> Option<Option<Vec<McKVS>>> {
        // Structure: volt:[low:0V ~ 0.7V, high:0.7V ~ 5V]
        // MCAST_EXPRESSION -> MCAST_OPD_COLON -> (left: MCAST_OPD, right: MCAST_OPD_SQUARE_VEC)
        // MCAST_OPD_SQUARE_VEC -> list of MCAST_OPD_COLON pairs like low:0V ~ 0.7V
        // Returns Some(Vec) if parsed successfully, Some(None) if not a KVS expression

        let sub = expr_node.get_sub_node()?;

        // Check if this is a colon expression
        if sub.get_type() != MCAST_OPD_COLON {
            return Some(None);
        }

        // Get left and right operands of the colon
        let left = sub.get_sub_node()?;
        let right = left.get_next()?;

        // Right operand should be MCAST_OPD_SQUARE_VEC
        if right.get_type() != MCAST_OPD_SQUARE_VEC {
            return Some(None);
        }

        // Parse the square vector to extract KVS entries
        let square_sub = right.get_sub_node()?;
        let kvs_values = Self::extract_kvs_from_iter(square_sub.iter());

        if kvs_values.is_empty() {
            return Some(None);
        }

        let kvs_list: Vec<McKVS> = kvs_values
            .into_iter()
            .filter_map(|val| {
                if let McAttrVal::KVS(kvs) = val {
                    Some(kvs)
                } else {
                    None
                }
            })
            .collect();

        Some(Some(kvs_list))
    }

    pub fn new_attr_values(node: &AstNode) -> Option<Vec<McAttrVal>> {
        // - MCAST_ATT_VALUES
        //  | mc_attr_value:
        //             | mc_literal
        //             | mc_opd
        //             | mc_phrase (MCAST_EXPRESSION)
        //             | MCPT_LBRACKET mc_attr_lines MCPT_RBRACKET -> MCAST_SET_ATTRIBUTES

        //1. Type
        if !matches!(node.get_type(), MCAST_ATT_VALUES) {
            dlog_error(601, node, TYPE_MISMATCH);
            return None;
        }
        //2. Child node: exists
        let Some(subnodes) = node.get_sub_node() else {
            dlog_error(601, node, MISSING_SUBNODE);
            return None;
        };

        let mut values = Vec::<McAttrVal>::new();

        //3. Child node: type
        for each in subnodes.iter() {
            match each.get_type() {
                MCAST_INT | MCAST_FLOAT | MCAST_HEX | MCAST_STRING | MCAST_CONST | MCAST_UVALUE => {
                    if let Some(lit) = McLiteral::new(&each) {
                        values.push(McAttrVal::AttrLiteral(lit));
                    }
                }

                MCAST_OPD => {
                    if let Some(opd) = McOpd::new(&each) {
                        values.push(McAttrVal::AttrVariable(opd));
                    }
                }

                MCAST_RANGE_PLUSMINUS => {
                    // ±15kV → Range(-15kV, +15kV)
                    if let Some(uval) = McUnitValue::new(&each) {
                        let neg_expr = McExpression::UnitValue(uval.negated());
                        let pos_expr = McExpression::UnitValue(uval);
                        values.push(McAttrVal::AttrExpr(McExpression::Range(
                            Box::new(neg_expr),
                            Box::new(pos_expr),
                        )));
                    }
                }

                MCAST_EXPRESSION => {
                    // Check if this is a KVS expression like volt:[low:0V ~ 0.7V, high:0.7V ~ 5V]
                    if let Some(kvs_result) = Self::try_parse_kvs_expression(&each) {
                        if let Some(kvs_list) = kvs_result {
                            for kvs in kvs_list {
                                values.push(McAttrVal::KVS(kvs));
                            }
                            continue;
                        }
                    }

                    let child = each.get_sub_node().expect(MISSING_SUBNODE);
                    if let Some(expr) = McExpression::new(&child) {
                        values.push(McAttrVal::AttrExpr(expr));
                    }
                }

                MCAST_SET_ATTRIBUTES => {
                    let sub = each.get_sub_node();
                    if sub.is_none() {
                        continue;
                    }
                    let mut attributes = Vec::<McAttribute>::new();

                    for astnode in sub.unwrap().iter() {
                        if let Some(attr) = McAttribute::new(&astnode) {
                            attributes.push(attr);
                        }
                    }

                    if !attributes.is_empty() {
                        values.push(McAttrVal::Attributes(attributes));
                    }
                }

                // Handle MCAST_OPD_SQUARE_VEC which can contain KVS-like entries
                // e.g., volt:[low:0V ~ 0.7V, high:0.7V ~ 5V]
                MCAST_OPD_SQUARE_VEC => {
                    let Some(sub) = each.get_sub_node() else {
                        continue;
                    };

                    let kvs_values = Self::extract_kvs_from_iter(sub.iter());
                    values.extend(kvs_values);
                }

                _ => {
                    dlog_error(
                        603,
                        &each,
                        &format!("Attribute type not support (node_type={})", each.get_type()),
                    );
                    continue;
                }
            }
        }
        Some(values)
    }
}

impl PartialEq for McAttribute {
    fn eq(&self, other: &Self) -> bool {
        self.no == other.no && self.id == other.id
    }
}

impl std::fmt::Display for McAttribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.values.is_empty() {
            write!(f, "{}", self.id)
        } else {
            let vals: Vec<String> = self.values.iter().map(|v| format!("{v}")).collect();
            write!(f, "{} = {}", self.id, vals.join(", "))
        }
    }
}

impl Eq for McAttribute {}
