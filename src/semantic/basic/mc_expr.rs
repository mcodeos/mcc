// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::eval;
use crate::message::MISSING_SUBNODE;
use crate::semantic::basic::mc_literal::{McConst, McFloat, McInt, McString};
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_uval::McUnitValue;

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

        let data_str = || -> Option<&str> {
            // Guarded accessor: the C parser can emit a NULL/small .data that
            // would segfault inside CStr::from_ptr -> strlen.
            left_node.data_as_cstr()?.to_str().ok()
        };
        let data_str2 = || -> Option<&str> { right_node.data_as_cstr()?.to_str().ok() };

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

    // Builtin call `name(arg, ...)` (U216): the value-level primitives the
    // engine dispatches by spelled name (`eval::call_builtin`). The
    // receiver-shaped form `x.f(...)` has no value reading here.
    Call {
        name: String,
        args: Vec<McExpression>,
    },
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
            MCAST_UVALUE_AT => Some(McExpression::UnitValueAt(McUnitValueAt::new(node)?)),

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

            // Builtin call `name(arg, ...)` (U216). The AST spells the bare
            // form as FCALL(NAME, PARAMS) siblings under the call node; the
            // receiver-shaped form (FCALL(INSTANCE, NAME, PARAMS)) has no
            // value reading here and stays unsupported.
            MCAST_OPD_FCALL => {
                let sub = node.get_sub_node()?;
                let mut name = String::new();
                let mut args = Vec::<McExpression>::new();
                for child in sub.iter() {
                    match child.get_type() {
                        MCAST_NAME => {
                            // The name text lives in NAME's sub node (an
                            // MCAST_ID leaf), matching the to_string read in
                            // src/ast/node.rs.
                            let inner = child.get_sub_node()?;
                            name = inner.to_string()?.trim().to_string();
                        }
                        MCAST_PARAMS => {
                            if let Some(children) = child.get_sub_node() {
                                for p in children.iter() {
                                    // Each argument is wrapped in a
                                    // MCAST_PARAM node (the PARAMS wrapper's
                                    // children, same shape the to_string read
                                    // in src/ast/node.rs:458 unpacks).
                                    if p.get_type() == MCAST_PARAM {
                                        let inner = p.get_sub_node()?;
                                        args.push(McExpression::new(&inner)?);
                                    } else {
                                        args.push(McExpression::new(&p)?);
                                    }
                                }
                            }
                        }
                        _ => return None,
                    }
                }
                if name.is_empty() {
                    return None;
                }
                Some(McExpression::Call { name, args })
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
                            // inst_sub might be MCAST_OPD, MCAST_OPD_SQUARE_VEC, or
                            // MCAST_EXPRESSION
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

    /// The expression read as a whole number — `1 + 2` is 3, never the text
    /// `"1 + 2"`. Arithmetic runs on the value engine, so a division by zero
    /// and an overflow come back as errors (5413 / 5415) instead of a wrong
    /// value; a form that has no integer reading is the same kind of failure.
    pub fn eval_int(&self) -> Result<i64, eval::EvalError> {
        let (op, left, right) = match self {
            McExpression::Int(int_val) => return Ok(int_val.value),
            McExpression::Plus(l, r) => (eval::Op::Add, l, r),
            McExpression::Minus(l, r) => (eval::Op::Sub, l, r),
            McExpression::Multiply(l, r) => (eval::Op::Mul, l, r),
            McExpression::Divide(l, r) => (eval::Op::Div, l, r),
            other => return Err(not_a_whole_number(other)),
        };
        let lhs = eval::Value::Int(left.eval_int()?);
        let rhs = eval::Value::Int(right.eval_int()?);
        match eval::apply(op, &lhs, &rhs) {
            Ok(eval::Value::Int(value)) => Ok(value),
            Ok(_) | Err(eval::EvalError::OperandNotNumeric { .. }) => Err(not_a_whole_number(self)),
            Err(err) => Err(err),
        }
    }

    /// Expand expression to Vec<String> for pin name resolution
    pub fn expand(&self) -> Vec<String> {
        match self {
            McExpression::Variable(opdc) => opdc.expand(),
            McExpression::Int(int_val) => vec![int_val.value.to_string()],
            McExpression::Set(items) => {
                let mut result = Vec::new();
                for item in items {
                    result.extend(item.expand());
                }
                result
            }
            McExpression::Slice(left, right) => {
                if let (Ok(start), Ok(end)) = (left.eval_int(), right.eval_int()) {
                    if start <= end {
                        (start..=end).map(|x| x.to_string()).collect()
                    } else {
                        (end..=start).rev().map(|x| x.to_string()).collect()
                    }
                } else {
                    vec![]
                }
            }
            _ => vec![self.to_string()],
        }
    }

    /// Resolve an `error(...)` message to text (U212). Variables go through
    /// `lookup` — the caller's bound parameters; a name with no binding stays
    /// spelled out rather than killing the message. `+` concatenates and
    /// interpolates through the value engine, so `"got " + 7` renders the same
    /// way it does in an attribute value. `None` when the form has no text
    /// reading — the caller falls back to the raw source text.
    pub fn resolve_message(&self, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        match self {
            McExpression::String(s) => Some(s.value.clone()),
            McExpression::Int(i) => Some(eval::Value::Int(i.value).text()),
            McExpression::Float(fl) => Some(eval::Value::Float(fl.value).text()),
            McExpression::Variable(opd) => {
                let names = opd.expand();
                match names.as_slice() {
                    [name] => Some(lookup(name).unwrap_or_else(|| name.clone())),
                    _ => Some(names.join(" ")),
                }
            }
            McExpression::Plus(l, r) => {
                let left = eval::Value::Str(l.resolve_message(lookup)?);
                let right = eval::Value::Str(r.resolve_message(lookup)?);
                eval::apply(eval::Op::Add, &left, &right)
                    .ok()
                    .map(|v| v.text())
            }
            _ => None,
        }
    }
}

/// The failure for a form that is not a number: it has no integer reading, so
/// whatever asked for one cannot proceed.
fn not_a_whole_number(expr: &McExpression) -> eval::EvalError {
    eval::EvalError::OperandNotNumeric {
        op: "int".to_string(),
        lhs: expr.to_string(),
        rhs: String::new(),
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
            McExpression::Call { name, args } => {
                let args_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{name}({args_str})")
            }
        }
    }
}
