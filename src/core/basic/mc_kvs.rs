// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_expr::McExpression;
use super::mc_literal::McConst;
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    builder::diagnostic::dlog_error,
    core::component::mc_attr::{McAttrVal, McAttribute},
    McIds,
};

#[derive(Debug, Clone)]
pub enum KVSValue {
    Const(McConst),
    Square(Vec<McAttrVal>),
    Nested(Vec<McKVS>),
}

#[derive(Debug, Clone)]
pub struct McKVS {
    pub key: McIds,
    pub value: KVSValue,
}

impl McKVS {
    pub fn new(node: &AstNode) -> Option<Self> {
        // MCAST_KVS
        // |- MCAST_KVS_KEY - MCAST_KVS_VALUE
        //    |- opdc         |- const, MCAST_SET, MCAST_SET_ATTRIBUTES

        // MCAST_ATT_VALUES
        // └── MCAST_EXPRESSION (35)
        //     └── MCAST_OPD_COLON (76)
        //         ├── left operand: MCAST_OPD (52) → MCAST_IDS "volt"
        //         └── right operand: MCAST_OPD_SQUARE_VEC (62)
        //             ├── MCAST_OPD_COLON (76) → low:0V ~ 0.7V
        //             │   ├── MCAST_OPD (52) → MCAST_IDS "low"
        //             │   └── MCAST_UVALUE "0V ~ 0.7V"
        //             └── MCAST_OPD_COLON (76) → high:0.7V ~ 5V
        //                 ├── MCAST_OPD (52) → MCAST_IDS "high"
        //                 └── MCAST_UVALUE "0.7V ~ 5V"

        let key_node = node.get_sub_node()?;
        let value_node = key_node.get_next()?;
        let key = key_node.get_sub_node()?;
        let value_data = value_node.get_sub_node()?;

        // First parse the key
        let key_ids = McIds::new(&key)?;

        match value_data.get_type() {
            MCAST_CONST => {
                let const_val = McConst::new(&value_data)?;
                Some(Self {
                    key: key_ids,
                    value: KVSValue::Const(const_val),
                })
            }

            MCAST_SET => {
                let square_vals = McAttribute::new_attr_values(&value_data)?;
                Some(Self {
                    key: key_ids,
                    value: KVSValue::Square(square_vals),
                })
            }

            // Handle direct unit value types
            t if (MCAST_UVAL_VOLT..=MCAST_UVAL_RESPONSIVITY).contains(&t) => {
                McConst::new(&value_data).map(|const_val| Self {
                    key: key_ids,
                    value: KVSValue::Const(const_val),
                })
            }

            // Handle ranges
            MCAST_OPD_COLON | MCAST_RANGE_PLUSMINUS => {
                if let Some(expr) = McExpression::new(&value_data) {
                    let attr_val = McAttrVal::AttrExpr(expr);
                    Some(Self {
                        key: key_ids,
                        value: KVSValue::Square(vec![attr_val]),
                    })
                } else {
                    None
                }
            }

            // Handle other types that can be converted to McConst
            MCAST_INT | MCAST_FLOAT | MCAST_HEX | MCAST_STRING => {
                McConst::new(&value_data).map(|const_val| Self {
                    key: key_ids,
                    value: KVSValue::Const(const_val),
                })
            }

            // Handle MCAST_UVALUE
            MCAST_UVALUE => McConst::new(&value_data).map(|const_val| Self {
                key: key_ids,
                value: KVSValue::Const(const_val),
            }),

            _ => {
                dlog_error(301, &value_data, "Invalid value type in KVS node.");
                None
            }
        }
    }
}
