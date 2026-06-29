// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    McIds,
};

#[derive(Debug, Clone)]
pub struct McCond {
    pub condition: McCondition,
    pub block: AstNode,
}

#[derive(Debug, Clone)]
pub enum McCondition {
    Eq {
        left: McCondOperand,
        right: McCondOperand,
    },
    NotEq {
        left: McCondOperand,
        right: McCondOperand,
    },
    Lt {
        left: McCondOperand,
        right: McCondOperand,
    },
    Gt {
        left: McCondOperand,
        right: McCondOperand,
    },
    LtEq {
        left: McCondOperand,
        right: McCondOperand,
    },
    GtEq {
        left: McCondOperand,
        right: McCondOperand,
    },
    In {
        left: McCondOperand,
        values: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub enum McCondOperand {
    Ident(McIds),
    Literal(String),
}

#[derive(Debug, Clone)]
pub struct McConds {
    pub if_blocks: Vec<McCond>,
    pub else_block: Option<AstNode>,
}

impl McConds {
    pub fn new(node: &AstNode) -> Option<Self> {
        let mut if_blocks = Vec::new();
        let mut else_block = None;

        let node_type = node.get_type();

        if node_type == MCAST_COND_IF {
            if let Some(cond) = Self::parse_cond_if(node) {
                if_blocks.push(cond);
            }
        }

        let Some(subnodes) = node.get_sub_node() else {
            return Some(Self {
                if_blocks,
                else_block,
            });
        };

        for child in subnodes.iter() {
            let child_type = child.get_type();
            match child_type {
                MCAST_COND_IF => {
                    if let Some(cond) = Self::parse_cond_if(&child) {
                        if_blocks.push(cond);
                    }
                }
                MCAST_COND_ELSE => {
                    if let Some((cond, block)) = Self::parse_cond_else_with_cond(&child) {
                        if let Some(c) = cond {
                            if_blocks.push(McCond {
                                condition: c,
                                block,
                            });
                        } else {
                            else_block = Some(block);
                        }
                    }
                }
                _ => {}
            }
        }

        Some(Self {
            if_blocks,
            else_block,
        })
    }

    fn parse_cond_if(node: &AstNode) -> Option<McCond> {
        let Some(subnodes) = node.get_sub_node() else {
            return None;
        };

        let mut condition_node: Option<AstNode> = None;
        let mut block_node: Option<AstNode> = None;
        let mut has_condition = false;

        for child in subnodes.iter() {
            let node_type = child.get_type();
            if node_type == MCAST_JUDGE_EQEQ
                || node_type == MCAST_JUDGE_NOTEQ
                || node_type == MCAST_JUDGE_LESSTHAN
                || node_type == MCAST_JUDGE_GREATERTHAN
                || node_type == MCAST_JUDGE_LESSEQTHAN
                || node_type == MCAST_JUDGE_GREATEREQTHAN
                || node_type == MCAST_JUDGE_IN
            {
                condition_node = Some(child);
                has_condition = true;
            } else if node_type == MCAST_COND_BLOCK {
                block_node = Some(child.clone());
            } else if node_type == MCAST_BODY {
                // Some parser paths wrap the pin block in MCAST_BODY
                // Look inside for ATTRIBUTE_PIN, ATTRIBUTE_PINADD, or ATTRIBUTE
                if let Some(body_sub) = child.get_sub_node() {
                    for inner in body_sub.iter() {
                        let inner_type = inner.get_type();
                        if inner_type == MCAST_ATTRIBUTE_PIN
                            || inner_type == MCAST_ATTRIBUTE_PINADD
                            || inner_type == MCAST_ATTRIBUTE
                        {
                            block_node = Some(inner.clone());
                            break;
                        }
                    }
                }
            } else if has_condition
                && block_node.is_none()
                && (node_type == MCAST_ATTRIBUTE_PIN
                    || node_type == MCAST_ATTRIBUTE_PINADD
                    || node_type == MCAST_ATTRIBUTE)
            {
                block_node = Some(child.clone());
            }
        }

        let condition = condition_node.and_then(|n| Self::parse_condition(&n))?;
        if has_condition && block_node.is_none() {
            return None;
        }
        let block = block_node.clone()?;

        Some(McCond { condition, block })
    }

    fn parse_cond_else(node: &AstNode) -> Option<AstNode> {
        let Some(subnodes) = node.get_sub_node() else {
            return None;
        };

        for child in subnodes.iter() {
            if child.get_type() == MCAST_COND_BLOCK {
                return Some(child.clone());
            }
        }
        None
    }

    fn parse_cond_else_with_cond(node: &AstNode) -> Option<(Option<McCondition>, AstNode)> {
        let Some(subnodes) = node.get_sub_node() else {
            return None;
        };

        let mut condition_node: Option<AstNode> = None;
        let mut block_node: Option<AstNode> = None;
        let mut else_if_block_node: Option<AstNode> = None;

        for child in subnodes.iter() {
            let child_type = child.get_type();
            if child_type == MCAST_JUDGE_EQEQ
                || child_type == MCAST_JUDGE_NOTEQ
                || child_type == MCAST_JUDGE_LESSTHAN
                || child_type == MCAST_JUDGE_GREATERTHAN
                || child_type == MCAST_JUDGE_LESSEQTHAN
                || child_type == MCAST_JUDGE_GREATEREQTHAN
                || child_type == MCAST_JUDGE_IN
            {
                condition_node = Some(child);
            } else if child_type == MCAST_COND_BLOCK {
                block_node = Some(child.clone());
            } else if child_type == MCAST_BODY {
                // Some parser paths wrap the pin block in MCAST_BODY
                if let Some(body_sub) = child.get_sub_node() {
                    for inner in body_sub.iter() {
                        let inner_type = inner.get_type();
                        if inner_type == MCAST_ATTRIBUTE_PIN
                            || inner_type == MCAST_ATTRIBUTE_PINADD
                            || inner_type == MCAST_ATTRIBUTE
                        {
                            else_if_block_node = Some(inner.clone());
                            break;
                        }
                    }
                }
            } else if child_type == MCAST_ATTRIBUTE_PIN
                || child_type == MCAST_ATTRIBUTE_PINADD
                || child_type == MCAST_ATTRIBUTE
            {
                else_if_block_node = Some(child.clone());
            }
        }

        if let Some(cond_node) = condition_node {
            let condition = Self::parse_condition(&cond_node)?;
            let block = else_if_block_node
                .or(block_node)
                .unwrap_or_else(|| cond_node.clone());
            Some((Some(condition), block))
        } else {
            block_node.map(|block| (None, block))
        }
    }

    fn parse_condition(node: &AstNode) -> Option<McCondition> {
        let node_type = node.get_type();

        let op_type = match node_type {
            MCAST_JUDGE_EQEQ => Some("=="),
            MCAST_JUDGE_NOTEQ => Some("!="),
            MCAST_JUDGE_LESSTHAN => Some("<"),
            MCAST_JUDGE_GREATERTHAN => Some(">"),
            MCAST_JUDGE_LESSEQTHAN => Some("<="),
            MCAST_JUDGE_GREATEREQTHAN => Some(">="),
            MCAST_JUDGE_IN => Some("in"),
            _ => None,
        };

        let Some(op_type_str) = op_type else {
            return None;
        };

        // Handle "in" operator specially: extract the array of values
        if op_type_str == "in" {
            return Self::parse_in_condition(node);
        }

        let mut operands: Vec<McCondOperand> = Vec::new();

        if let Some(subnodes) = node.get_sub_node() {
            for child in subnodes.iter() {
                match child.get_type() {
                    MCAST_ID | MCAST_IDA => {
                        if let Some(ids) = McIds::new(&child) {
                            operands.push(McCondOperand::Ident(ids));
                        }
                    }
                    MCAST_INT | MCAST_HEX => {
                        let val = child.to_string().unwrap_or_default();
                        operands.push(McCondOperand::Literal(val));
                    }
                    MCAST_FLOAT | MCAST_UVALUE => {
                        let val = child.to_string().unwrap_or_default();
                        operands.push(McCondOperand::Literal(val));
                    }
                    MCAST_STRING => unsafe {
                        let c_str =
                            std::ffi::CStr::from_ptr(child.get_data() as *const std::ffi::c_char);
                        if let Ok(str_value) = c_str.to_str() {
                            let val = str_value.to_string();
                            let clean_val =
                                if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                                    val[1..val.len() - 1].to_string()
                                } else {
                                    val
                                };
                            operands.push(McCondOperand::Literal(clean_val));
                        }
                    },
                    MCAST_OPD => {
                        if let Some(opd_subnode) = child.get_sub_node() {
                            if opd_subnode.get_type() == MCAST_IDS {
                                if let Some(ids_subnode) = opd_subnode.get_sub_node() {
                                    if let Some(ids) = McIds::new(&ids_subnode) {
                                        operands.push(McCondOperand::Ident(ids));
                                    }
                                }
                            } else if let Some(ids) = McIds::new(&opd_subnode) {
                                operands.push(McCondOperand::Ident(ids));
                            }
                        }
                    }
                    // Handle array operand: "param in [A, B, C]" parsed as "param == [A, B, C]"
                    // by the C parser. Detect this and convert to In condition.
                    MCAST_OPD_SQUARE_VEC => {
                        let mut values = Vec::new();
                        if let Some(vec_first) = child.get_sub_node() {
                            let mut current = Some(vec_first);
                            while let Some(item) = current {
                                if item.get_type() == MCAST_STRING {
                                    unsafe {
                                        let c_str = std::ffi::CStr::from_ptr(
                                            item.get_data() as *const std::ffi::c_char
                                        );
                                        if let Ok(str_value) = c_str.to_str() {
                                            let val = str_value.to_string();
                                            let clean_val = if val.starts_with('"')
                                                && val.ends_with('"')
                                                && val.len() >= 2
                                            {
                                                val[1..val.len() - 1].to_string()
                                            } else {
                                                val
                                            };
                                            values.push(clean_val);
                                        }
                                    }
                                }
                                current = item.get_next();
                            }
                        }
                        // If we have a left operand and values, this is an "in" condition
                        if !operands.is_empty() && !values.is_empty() {
                            return Some(McCondition::In {
                                left: operands[0].clone(),
                                values,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        if operands.len() < 2 {
            return None;
        }

        let left = operands[0].clone();
        let right = operands[1].clone();

        match op_type_str {
            "==" => Some(McCondition::Eq { left, right }),
            "!=" => Some(McCondition::NotEq { left, right }),
            "<" => Some(McCondition::Lt { left, right }),
            ">" => Some(McCondition::Gt { left, right }),
            "<=" => Some(McCondition::LtEq { left, right }),
            ">=" => Some(McCondition::GtEq { left, right }),
            _ => None,
        }
    }

    /// Parse "in" condition: `param in ["val1", "val2", ...]`
    fn parse_in_condition(node: &AstNode) -> Option<McCondition> {
        let first_child = node.get_sub_node()?;

        // First child may be wrapped in MCAST_OPD, unwrap it
        let (id_node, next_sibling) = if first_child.get_type() == MCAST_OPD {
            let inner = first_child.get_sub_node()?;
            (inner, first_child.get_next())
        } else {
            let next = first_child.get_next();
            (first_child, next)
        };

        // First child is the left operand (identifier)
        let left = if id_node.get_type() == MCAST_ID
            || id_node.get_type() == MCAST_IDA
            || id_node.get_type() == MCAST_IDS
        {
            McIds::new(&id_node).map(McCondOperand::Ident)
        } else {
            None
        }?;

        // Second child is MCAST_OPD_SQUARE_VEC containing the array of strings
        let right_child = next_sibling?;
        let mut values = Vec::new();

        if right_child.get_type() == MCAST_OPD_SQUARE_VEC {
            if let Some(vec_first) = right_child.get_sub_node() {
                let mut current = Some(vec_first);
                while let Some(item) = current {
                    if item.get_type() == MCAST_STRING {
                        unsafe {
                            let c_str = std::ffi::CStr::from_ptr(
                                item.get_data() as *const std::ffi::c_char
                            );
                            if let Ok(str_value) = c_str.to_str() {
                                let val = str_value.to_string();
                                let clean_val =
                                    if val.starts_with('"') && val.ends_with('"') && val.len() >= 2
                                    {
                                        val[1..val.len() - 1].to_string()
                                    } else {
                                        val
                                    };
                                values.push(clean_val);
                            }
                        }
                    }
                    current = item.get_next();
                }
            }
        }

        Some(McCondition::In { left, values })
    }

    pub fn evaluate(&self, params: &[(McIds, String)]) -> Option<AstNode> {
        for cond in &self.if_blocks {
            if Self::check_condition(&cond.condition, params) {
                return Some(cond.block.clone());
            }
        }

        if let Some(block) = &self.else_block {
            return Some(block.clone());
        }

        None
    }

    pub fn check_condition(cond: &McCondition, params: &[(McIds, String)]) -> bool {
        // Handle "in" condition separately (different structure)
        if let McCondition::In { left, values } = cond {
            let left_val = Self::resolve_operand(left, params);
            return values.iter().any(|v| v == &left_val);
        }

        let (left_val, right_val) = match cond {
            McCondition::Eq { left, right } => (
                Self::resolve_operand(left, params),
                Self::resolve_operand(right, params),
            ),
            McCondition::NotEq { left, right } => (
                Self::resolve_operand(left, params),
                Self::resolve_operand(right, params),
            ),
            McCondition::Lt { left, right } => (
                Self::resolve_operand(left, params),
                Self::resolve_operand(right, params),
            ),
            McCondition::Gt { left, right } => (
                Self::resolve_operand(left, params),
                Self::resolve_operand(right, params),
            ),
            McCondition::LtEq { left, right } => (
                Self::resolve_operand(left, params),
                Self::resolve_operand(right, params),
            ),
            McCondition::GtEq { left, right } => (
                Self::resolve_operand(left, params),
                Self::resolve_operand(right, params),
            ),
            McCondition::In { .. } => unreachable!(),
        };

        match cond {
            McCondition::Eq { .. } => {
                Self::compare_values(&left_val, &right_val) == std::cmp::Ordering::Equal
            }
            McCondition::NotEq { .. } => {
                Self::compare_values(&left_val, &right_val) != std::cmp::Ordering::Equal
            }
            McCondition::Lt { .. } => {
                Self::compare_values(&left_val, &right_val) == std::cmp::Ordering::Less
            }
            McCondition::Gt { .. } => {
                Self::compare_values(&left_val, &right_val) == std::cmp::Ordering::Greater
            }
            McCondition::LtEq { .. } => {
                let cmp = Self::compare_values(&left_val, &right_val);
                cmp == std::cmp::Ordering::Less || cmp == std::cmp::Ordering::Equal
            }
            McCondition::GtEq { .. } => {
                let cmp = Self::compare_values(&left_val, &right_val);
                cmp == std::cmp::Ordering::Greater || cmp == std::cmp::Ordering::Equal
            }
            McCondition::In { .. } => unreachable!(),
        }
    }

    fn resolve_operand(op: &McCondOperand, params: &[(McIds, String)]) -> String {
        match op {
            McCondOperand::Ident(ids) => {
                let name = ids.to_string();
                for (param_name, param_value) in params {
                    if param_name.to_string() == name {
                        return param_value.clone();
                    }
                }
                name
            }
            McCondOperand::Literal(val) => val.clone(),
        }
    }

    fn compare_values(left: &str, right: &str) -> std::cmp::Ordering {
        let left_num = Self::extract_number(left);
        let right_num = Self::extract_number(right);
        if let (Some(l), Some(r)) = (left_num, right_num) {
            l.partial_cmp(&r).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            left.cmp(right)
        }
    }

    fn extract_number(s: &str) -> Option<f64> {
        let s = s.trim();
        if let Ok(n) = s.parse::<f64>() {
            return Some(n);
        }

        let re_pattern = r"^(-?[\d.]+)([a-zA-Z]*)$";
        if let Ok(re) = regex::Regex::new(re_pattern) {
            if let Some(caps) = re.captures(s) {
                if let Ok(n) = caps.get(1).unwrap().as_str().parse::<f64>() {
                    return Some(n);
                }
            }
        }

        None
    }
}
