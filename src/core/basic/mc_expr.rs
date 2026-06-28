// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::core::basic::mc_literal::{McConst, McFloat, McInt, McString};
use crate::core::basic::mc_opd::McOpd;
use crate::core::basic::mc_uval::McUnitValue;
use crate::message::MISSING_SUBNODE;
use crate::McIds;

#[derive(Debug, Clone)]
pub struct McUnitValueAt {
    pub left: McUnitValue,
    pub right: McUnitValue,
}

impl McUnitValueAt {
    /// Parse MCAST_UVALUE_AT node (e.g., 1Mbps@0.5m)
    /// AST: MCAST_UVALUE_AT -> MCAST_UVAL_AT -> [MCAST_UVAL_BAUD, MCAST_UVAL_LEN]
    pub fn new(node: &AstNode) -> Option<Self> {
        // AST: MCAST_UVALUE_AT -> MCAST_UVAL_AT -> [MCAST_UVAL_BAUD, MCAST_UVAL_LEN] (siblings)
        // Each has its value data embedded directly (no subnode wrapper)
        let sub = node.get_sub_node()?;
        let left_node = &sub;
        let right_node = left_node.get_next()?;

        let data_str = || -> Option<&'static str> {
            let ptr = left_node.get_data() as *const i8;
            // SAFETY: the pointer is valid for the lifetime of this call
            unsafe { std::ffi::CStr::from_ptr(ptr).to_str().ok() }
        };
        let data_str2 = || -> Option<&'static str> {
            let ptr = right_node.get_data() as *const i8;
            // SAFETY: the pointer is valid for the lifetime of this call
            unsafe { std::ffi::CStr::from_ptr(ptr).to_str().ok() }
        };

        let left = McUnitValue::from_data_and_type(left_node, data_str()?)?;
        let right = McUnitValue::from_data_and_type(&right_node, data_str2()?)?;
        Some(Self { left, right })
    }
}

// McExpression enum
#[derive(Debug, Clone)]
pub enum McExpression {
    // Constant type
    Int(McInt),
    Float(McFloat),
    String(McString),
    UnitValue(McUnitValue),
    UnitValueAt(McUnitValueAt),
    Const(McConst),

    // Variable - received by McOpd
    Variable(McOpd),

    // Binary operator
    Plus(Box<McExpression>, Box<McExpression>),
    Minus(Box<McExpression>, Box<McExpression>),
    Multiply(Box<McExpression>, Box<McExpression>),
    Divide(Box<McExpression>, Box<McExpression>),

    // Slice and range
    Slice(Box<McExpression>, Box<McExpression>),
    Range(Box<McExpression>, Box<McExpression>),

    // Set
    Set(Vec<McExpression>),

    // Key-value pair
    KVS(Box<McIds>, Box<McExpression>),

    // Curly brace expansion, e.g. DC2{VDD, GND} => "DC2.VDD", "DC2.GND"
    CurlyExpand { base: String, names: Vec<String> },
}

impl McExpression {
    pub fn new(node: &AstNode) -> Option<Self> {
        let node_type = node.get_type();

        if node_type == MCAST_EXPRESSION {
            if let Some(sub) = node.get_sub_node() {
                return McExpression::new(&sub);
            }
            return None;
        }

        match node_type {
            // Constant: number
            MCAST_INT | MCAST_HEX => Some(McExpression::Int(McInt::new(node)?)),
            MCAST_FLOAT => Some(McExpression::Float(McFloat::new(node)?)),
            // Constant: string
            MCAST_STRING => Some(McExpression::String(McString::new(node)?)),
            // Constant: keyword constant
            MCAST_CONST => Some(McExpression::Const(McConst::new(node)?)),
            // Constant: unit value
            MCAST_UVALUE | MCAST_RANGE_PLUSMINUS => {
                Some(McExpression::UnitValue(McUnitValue::new(node)?))
            }
            MCAST_UVALUE_AT => {
                Some(McExpression::UnitValueAt(McUnitValueAt::new(node)?))
            }

            // Variable: received by McOpd
            MCAST_OPD_USCORE | MCAST_OPD_THIS | MCAST_OPD_PINS | MCAST_ID | MCAST_IDA
            | MCAST_OPD_DOT | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN | MCAST_OPD => {
                Some(McExpression::Variable(McOpd::new(node)?))
            }

            // Binary operator
            MCAST_OPD_PLUS => {
                let left = node.get_sub_node()?;
                let right = left.get_next()?;
                if let (Some(left_expr), Some(right_expr)) =
                    (McExpression::new(&left), McExpression::new(&right))
                {
                    Some(McExpression::Plus(
                        Box::new(left_expr),
                        Box::new(right_expr),
                    ))
                } else {
                    None
                }
            }
            MCAST_OPD_MINUS => {
                let left = node.get_sub_node()?;
                let right = left.get_next()?;
                if let (Some(left_expr), Some(right_expr)) =
                    (McExpression::new(&left), McExpression::new(&right))
                {
                    Some(McExpression::Minus(
                        Box::new(left_expr),
                        Box::new(right_expr),
                    ))
                } else {
                    None
                }
            }
            MCAST_OPD_MULTI => {
                let left = node.get_sub_node()?;
                let right = left.get_next()?;
                if let (Some(left_expr), Some(right_expr)) =
                    (McExpression::new(&left), McExpression::new(&right))
                {
                    Some(McExpression::Multiply(
                        Box::new(left_expr),
                        Box::new(right_expr),
                    ))
                } else {
                    None
                }
            }
            MCAST_OPD_DIVID => {
                let left = node.get_sub_node()?;
                let right = left.get_next()?;
                if let (Some(left_expr), Some(right_expr)) =
                    (McExpression::new(&left), McExpression::new(&right))
                {
                    Some(McExpression::Divide(
                        Box::new(left_expr),
                        Box::new(right_expr),
                    ))
                } else {
                    None
                }
            }

            // Slice and range
            MCAST_OPD_COLON => {
                let left = node.get_sub_node()?;
                let right = left.get_next()?;
                if let (Some(left_expr), Some(right_expr)) =
                    (McExpression::new(&left), McExpression::new(&right))
                {
                    Some(McExpression::Slice(
                        Box::new(left_expr),
                        Box::new(right_expr),
                    ))
                } else {
                    None
                }
            }
            MCAST_OPD_TILDE => {
                let left = node.get_sub_node()?;
                let right = left.get_next()?;
                if let (Some(left_expr), Some(right_expr)) =
                    (McExpression::new(&left), McExpression::new(&right))
                {
                    Some(McExpression::Range(
                        Box::new(left_expr),
                        Box::new(right_expr),
                    ))
                } else {
                    None
                }
            }

            MCAST_OPD_SQUARE_VEC => {
                let mut expressions = Vec::<McExpression>::new();
                node.get_sub_node()
                    .expect(MISSING_SUBNODE)
                    .iter()
                    .for_each(|astnode| {
                        if let Some(expr) = McExpression::new(&astnode) {
                            expressions.push(expr);
                        }
                    });
                Some(McExpression::Set(expressions))
            }

            // Handle MCAST_DECLARE inside MCAST_EXPRESSION (e.g., DC2{VDD,GND}::DC)
            MCAST_DECLARE => {
                let Some(sub) = node.get_sub_node() else {
                    return None;
                };
                // Look for MCAST_INSTANCE under declare to extract the variable
                for n in sub.iter() {
                    if n.get_type() == MCAST_INSTANCE {
                        if let Some(inst_sub) = n.get_sub_node() {
                            // inst_sub might be MCAST_OPD, MCAST_OPD_SQUARE_VEC, or MCAST_EXPRESSION
                            return McExpression::new(&inst_sub);
                        }
                    }
                }
                None
            }

            //.. kvs
            _ => None,
        }
    }

    /// Evaluate the expression (placeholder for future implementation)
    pub fn evaluate(&self) -> Option<String> {
        // This is a placeholder - actual evaluation logic will be implemented later
        // based on the expression type and context
        match self {
            McExpression::Variable(opdc) => Some(opdc.expand().join(" ")),
            McExpression::Int(int_val) => Some(int_val.value.to_string()),
            McExpression::Float(float_val) => Some(float_val.value.to_string()),
            McExpression::String(str_val) => Some(str_val.value.clone()),
            McExpression::UnitValue(unit_val) => {
                Some(format!("{}{:?}", unit_val.value(), unit_val.unit()))
            }
            McExpression::UnitValueAt(unit_val_at) => Some(format!(
                "{}@{} {}@{}",
                unit_val_at.left.value(),
                unit_val_at.left.unit(),
                unit_val_at.right.value(),
                unit_val_at.right.unit()
            )),
            McExpression::Const(const_val) => Some(format!("{const_val:?}")),
            McExpression::Plus(left, right) => {
                if let (Some(l), Some(r)) = (left.evaluate(), right.evaluate()) {
                    Some(format!("{l} + {r}"))
                } else {
                    None
                }
            }
            McExpression::Minus(left, right) => {
                if let (Some(l), Some(r)) = (left.evaluate(), right.evaluate()) {
                    Some(format!("{l} - {r}"))
                } else {
                    None
                }
            }
            McExpression::Multiply(left, right) => {
                if let (Some(l), Some(r)) = (left.evaluate(), right.evaluate()) {
                    Some(format!("{l} * {r}"))
                } else {
                    None
                }
            }
            McExpression::Divide(left, right) => {
                if let (Some(l), Some(r)) = (left.evaluate(), right.evaluate()) {
                    Some(format!("{l} / {r}"))
                } else {
                    None
                }
            }
            McExpression::Slice(left, right) => {
                if let (Some(l), Some(r)) = (left.evaluate(), right.evaluate()) {
                    Some(format!("{l}:{r}"))
                } else {
                    None
                }
            }
            McExpression::Range(left, right) => {
                if let (Some(l), Some(r)) = (left.evaluate(), right.evaluate()) {
                    Some(format!("{l}~{r}"))
                } else {
                    None
                }
            }
            McExpression::Set(expressions) => {
                let expr_strs: Vec<String> = expressions
                    .iter()
                    .map(|expr| expr.evaluate().unwrap_or_else(|| "?".to_string()))
                    .collect();
                Some(format!("[{}]", expr_strs.join(", ")))
            }
            McExpression::KVS(mc_ids, mc_expression) => {
                if let (ids, Some(expr)) = (mc_ids.to_string(), mc_expression.evaluate()) {
                    Some(format!("{ids}: {expr}"))
                } else {
                    None
                }
            }
            McExpression::CurlyExpand { base, names } => {
                // Returns something like "DC2.VDD, DC2.GND"
                let expanded: Vec<String> =
                    names.iter().map(|name| format!("{base}.{name}")).collect();
                Some(expanded.join(", "))
            }
        }
    }

    /// Expand expression to Vec<String> for pin name resolution
    pub fn expand(&self) -> Vec<String> {
        match self {
            McExpression::Variable(opdc) => opdc.expand(),
            McExpression::CurlyExpand { base, names } => {
                names.iter().map(|name| format!("{base}.{name}")).collect()
            }
            McExpression::Int(int_val) => vec![int_val.value.to_string()],
            McExpression::Set(items) => {
                let mut result = Vec::new();
                for item in items {
                    result.extend(item.expand());
                }
                result
            }
            McExpression::Slice(left, right) => {
                if let (Some(l), Some(r)) = (left.evaluate(), right.evaluate()) {
                    if let (Ok(start), Ok(end)) = (l.parse::<i64>(), r.parse::<i64>()) {
                        if start <= end {
                            (start..=end).map(|x| x.to_string()).collect()
                        } else {
                            (end..=start).rev().map(|x| x.to_string()).collect()
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            _ => self.evaluate().map(|s| vec![s]).unwrap_or_default(),
        }
    }
}

impl std::fmt::Display for McExpression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McExpression::Int(i) => write!(f, "{}", i.value),
            McExpression::Float(fl) => write!(f, "{}", fl.value),
            McExpression::String(s) => write!(f, "\"{}\"", s.value),
            McExpression::UnitValue(uv) => write!(f, "{uv}"),
            McExpression::UnitValueAt(uva) => write!(f, "{}@{}", uva.left, uva.right),
            McExpression::Const(c) => write!(f, "{c}"),
            McExpression::Variable(opd) => write!(f, "{opd}"),
            McExpression::Plus(l, r) => write!(f, "{l} + {r}"),
            McExpression::Minus(l, r) => write!(f, "{l} - {r}"),
            McExpression::Multiply(l, r) => write!(f, "{l} * {r}"),
            McExpression::Divide(l, r) => write!(f, "{l} / {r}"),
            McExpression::Slice(l, r) => write!(f, "{l}:{r}"),
            McExpression::Range(l, r) => write!(f, "{l}~{r}"),
            McExpression::Set(items) => {
                let items_str = items
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{items_str}]")
            }
            McExpression::KVS(key, val) => write!(f, "{key}: {val}"),
            McExpression::CurlyExpand { base, names } => {
                let names_str = names.join(", ");
                write!(f, "{base}{{{names_str}}}")
            }
        }
    }
}
