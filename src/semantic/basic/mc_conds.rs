// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::eval::{self, Compare, Value};
use crate::semantic::basic::mc_literal::strip_string_quotes;
use crate::{
    ast::{macros::*, node::AstNode},
    semantic::basic::mc_phrase::McPhrase,
    McIds,
};

#[derive(Debug, Clone)]
pub struct McCond {
    pub condition: McCondition,
    pub block: AstNode,
}

/// Structural equality (PartialEq) lets the validator detect a condition that
/// exactly duplicates an earlier branch of the same if/else-if chain — the
/// later branch is dead code.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Bitwise AND condition: `if (address & 0x01)` — true when the
    /// bitwise result is non-zero.
    BitAnd {
        left: McCondOperand,
        right: McCondOperand,
    },
    /// Bitwise OR condition: `if (address | 0x01)` — true when the
    /// bitwise result is non-zero.
    BitOr {
        left: McCondOperand,
        right: McCondOperand,
    },
    In {
        left: McCondOperand,
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McCondOperand {
    Ident(McIds),
    Literal(String),
}

impl std::fmt::Display for McCondOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McCondOperand::Ident(id) => write!(f, "{}", id),
            McCondOperand::Literal(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for McCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McCondition::Eq { left, right } => write!(f, "{} == {}", left, right),
            McCondition::NotEq { left, right } => write!(f, "{} != {}", left, right),
            McCondition::Lt { left, right } => write!(f, "{} < {}", left, right),
            McCondition::Gt { left, right } => write!(f, "{} > {}", left, right),
            McCondition::LtEq { left, right } => write!(f, "{} <= {}", left, right),
            McCondition::GtEq { left, right } => write!(f, "{} >= {}", left, right),
            McCondition::BitAnd { left, right } => write!(f, "{} & {}", left, right),
            McCondition::BitOr { left, right } => write!(f, "{} | {}", left, right),
            McCondition::In { left, values } => write!(f, "{} in [{}]", left, values.join(", ")),
        }
    }
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

        mcc_dbg!(
            "sem::conds",
            "[CONDS-NEW] node_type={} node_str={:?}",
            node.get_type(),
            node.to_string()
        );

        match node.get_type() {
            MCAST_COND_IF => {
                if let Some(cond) = Self::parse_cond_if(node) {
                    if_blocks.push(cond);
                }
            }
            MCAST_COND_ELSE => {
                Self::collect_else_branch(node, &mut if_blocks, &mut else_block);
            }
            _ => return None,
        }
        Self::collect_nested_branches(node, &mut if_blocks, &mut else_block);

        Some(Self {
            if_blocks,
            else_block,
        })
    }

    fn collect_nested_branches(
        node: &AstNode,
        if_blocks: &mut Vec<McCond>,
        else_block: &mut Option<AstNode>,
    ) {
        let Some(subnodes) = node.get_sub_node() else {
            return;
        };
        for child in subnodes.iter() {
            match child.get_type() {
                MCAST_COND_IF => {
                    if let Some(cond) = Self::parse_cond_if(&child) {
                        if_blocks.push(cond);
                    }
                    Self::collect_nested_branches(&child, if_blocks, else_block);
                }
                MCAST_COND_ELSE => {
                    Self::collect_else_branch(&child, if_blocks, else_block);
                    Self::collect_nested_branches(&child, if_blocks, else_block);
                }
                _ => {}
            }
        }
    }

    fn collect_else_branch(
        node: &AstNode,
        if_blocks: &mut Vec<McCond>,
        else_block: &mut Option<AstNode>,
    ) {
        let Some((condition, block)) = Self::parse_cond_else_with_cond(node) else {
            return;
        };
        if let Some(condition) = condition {
            if_blocks.push(McCond { condition, block });
        } else {
            *else_block = Some(block);
        }
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
                || node_type == MCAST_JUDGE_BITAND
                || node_type == MCAST_JUDGE_BITOR
                || node_type == MCAST_JUDGE_IN
            {
                condition_node = Some(child);
                has_condition = true;
            } else if node_type == MCAST_COND_BLOCK {
                block_node = Some(child.clone());
            } else if node_type == MCAST_NET {
                // Direct NET node as block (e.g., `if address == 0x36 VDD -> RES(100kΩ) -> GPIO.2`)
                block_node = Some(child.clone());
            } else if node_type == MCAST_BODY {
                // Braced block. May hold pin attributes (looked up below) OR
                // connection stmts (MCAST_NET). In the latter case keep the
                // BODY node itself so parse_block_stmts can iterate its net
                // stmts; otherwise `if (cond) { net }` silently drops the block.
                let mut found_attr = false;
                if let Some(body_sub) = child.get_sub_node() {
                    for inner in body_sub.iter() {
                        let inner_type = inner.get_type();
                        if inner_type == MCAST_ATTRIBUTE_PIN
                            || inner_type == MCAST_ATTRIBUTE_PINADD
                            || inner_type == MCAST_ATTRIBUTE
                        {
                            block_node = Some(inner.clone());
                            found_attr = true;
                            break;
                        }
                    }
                }
                if !found_attr {
                    block_node = Some(child.clone());
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
                || child_type == MCAST_JUDGE_BITAND
                || child_type == MCAST_JUDGE_BITOR
                || child_type == MCAST_JUDGE_IN
            {
                condition_node = Some(child);
            } else if child_type == MCAST_COND_BLOCK {
                block_node = Some(child.clone());
            } else if child_type == MCAST_NET {
                // Direct NET node as block (e.g., `else GPIO.2 - RES(100kΩ) -> GND`)
                block_node = Some(child.clone());
            } else if child_type == MCAST_BODY {
                // Braced block: pin attributes OR connection stmts. For net
                // stmts keep the BODY node itself so parse_block_stmts can
                // iterate; otherwise `else { net }` silently drops the block.
                let mut found_attr = false;
                if let Some(body_sub) = child.get_sub_node() {
                    for inner in body_sub.iter() {
                        let inner_type = inner.get_type();
                        if inner_type == MCAST_ATTRIBUTE_PIN
                            || inner_type == MCAST_ATTRIBUTE_PINADD
                            || inner_type == MCAST_ATTRIBUTE
                        {
                            else_if_block_node = Some(inner.clone());
                            found_attr = true;
                            break;
                        }
                    }
                }
                if !found_attr {
                    else_if_block_node = Some(child.clone());
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
            block_node.or(else_if_block_node).map(|block| (None, block))
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
            MCAST_JUDGE_BITAND => Some("&"),
            MCAST_JUDGE_BITOR => Some("|"),
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
                    MCAST_STRING => {
                        // Guarded accessor: the C parser can emit a NULL/small .data.
                        if let Ok(str_value) = child.data_as_cstr()?.to_str() {
                            let val = str_value.to_string();
                            let clean_val = strip_string_quotes(&val).to_string();
                            operands.push(McCondOperand::Literal(clean_val));
                        }
                    }
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
                                    // Guarded accessor: the C parser can emit a NULL/small .data.
                                    if let Ok(str_value) = item.data_as_cstr()?.to_str() {
                                        let val = str_value.to_string();
                                        let clean_val = strip_string_quotes(&val).to_string();
                                        values.push(clean_val);
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
            "&" => Some(McCondition::BitAnd { left, right }),
            "|" => Some(McCondition::BitOr { left, right }),
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
                        // Guarded accessor: the C parser can emit a NULL/small .data.
                        if let Some(str_value) = item.data_as_cstr().and_then(|c| c.to_str().ok()) {
                            let val = str_value.to_string();
                            let clean_val = strip_string_quotes(&val).to_string();
                            values.push(clean_val);
                        }
                    }
                    current = item.get_next();
                }
            }
        }

        Some(McCondition::In { left, values })
    }

    /// Evaluate the branches; the first satisfied one wins.
    ///
    /// A condition that cannot be evaluated is reported at `anchor` — the
    /// consumer's own syntax, because a condition node belongs to the file that
    /// declares the interface, not to the file being processed. The failing
    /// branch is read as "not satisfied", so branch selection is unchanged; the
    /// failure is reported once per call however long the `else if` chain is.
    pub fn evaluate(
        &self,
        params: &[(McIds, String)],
        anchor: Option<&AstNode>,
    ) -> Option<AstNode> {
        let mut failure: Option<eval::EvalError> = None;
        for cond in &self.if_blocks {
            match Self::check_condition_result(&cond.condition, params) {
                Ok(true) => return Some(cond.block.clone()),
                Ok(false) => {}
                Err(err) => failure = failure.or(Some(err)),
            }
        }
        if let (Some(err), Some(node)) = (&failure, anchor) {
            eval::report(err, node);
        }

        if let Some(block) = &self.else_block {
            return Some(block.clone());
        }

        None
    }

    pub fn check_condition(cond: &McCondition, params: &[(McIds, String)]) -> bool {
        // A condition with no node cannot carry a diagnostic, so the error half
        // is dropped here; `check_condition_result` is the same evaluation with
        // the failure preserved, and `McConds::evaluate` is the positioned
        // caller that reports it.
        Self::check_condition_result(cond, params).unwrap_or(false)
    }

    /// Evaluate one condition through the value engine (doc/eval V7). The
    /// operands are bound argument text, so they enter the engine by text and
    /// are normalized by the one suffix table — `1200mV` and `1.2V` are the
    /// same value, which the old suffix-stripping comparison could not see.
    pub fn check_condition_result(
        cond: &McCondition,
        params: &[(McIds, String)],
    ) -> Result<bool, eval::EvalError> {
        // Handle "in" condition separately (different structure)
        if let McCondition::In { left, values } = cond {
            let left_val = Value::from_text(&Self::resolve_operand(left, params));
            for value in values {
                if eval::satisfies(Compare::Eq, &left_val, &Value::from_text(value))? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }

        // Bitwise conditions (`if (address & 0x01)` / `if (address | 0x01)`):
        // apply the operation to the two integers and treat a non-zero result
        // as true. A non-integer operand keeps its historical reading — the
        // condition is simply not satisfied.
        if let McCondition::BitAnd { left, right } | McCondition::BitOr { left, right } = cond {
            let left_val = Value::from_text(&Self::resolve_operand(left, params));
            let right_val = Value::from_text(&Self::resolve_operand(right, params));
            let (Value::Int(l), Value::Int(r)) = (left_val, right_val) else {
                return Ok(false);
            };
            let result = if matches!(cond, McCondition::BitAnd { .. }) {
                l & r
            } else {
                l | r
            };
            return Ok(result != 0);
        }

        let (left_op, right_op, cmp) = match cond {
            McCondition::Eq { left, right } => (left, right, Compare::Eq),
            McCondition::NotEq { left, right } => (left, right, Compare::NotEq),
            McCondition::Lt { left, right } => (left, right, Compare::Lt),
            McCondition::Gt { left, right } => (left, right, Compare::Gt),
            McCondition::LtEq { left, right } => (left, right, Compare::LtEq),
            McCondition::GtEq { left, right } => (left, right, Compare::GtEq),
            McCondition::BitAnd { .. } | McCondition::BitOr { .. } | McCondition::In { .. } => {
                unreachable!()
            }
        };

        let left_val = Value::from_text(&Self::resolve_operand(left_op, params));
        let right_val = Value::from_text(&Self::resolve_operand(right_op, params));
        eval::satisfies(cmp, &left_val, &right_val)
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
}

// McFuncConds — parsed conditional blocks, storing McPhrase stmts for evaluation at instantiation
// time

/// A single parsed conditional branch
#[derive(Debug, Clone)]
pub struct McCondBlock {
    pub condition: McCondition,
    pub stmts: Vec<McPhrase>,
}

/// A parsed collection of conditional blocks (if/else if/else)
#[derive(Debug, Clone)]
pub struct McFuncConds {
    pub if_blocks: Vec<McCondBlock>,
    pub else_stmts: Vec<McPhrase>,
}

impl McFuncConds {
    /// Parse McPhrase stmts from McConds and the given context
    pub fn from_conds(
        conds: &McConds,
        context: &mut dyn crate::semantic::mc_func::HasFindInst,
    ) -> Self {
        let mut if_blocks = Vec::new();
        let mut else_stmts = Vec::new();

        for cond in &conds.if_blocks {
            let mut stmts = Vec::new();
            // The block is an AstNode; parse its content into McPhrase stmts
            Self::parse_block_stmts(&cond.block, context, &mut stmts);
            if_blocks.push(McCondBlock {
                condition: cond.condition.clone(),
                stmts,
            });
        }

        if let Some(else_block) = &conds.else_block {
            Self::parse_block_stmts(else_block, context, &mut else_stmts);
        }

        McFuncConds {
            if_blocks,
            else_stmts,
        }
    }

    /// Parse an AstNode block into McPhrase stmts
    fn parse_block_stmts(
        block: &AstNode,
        context: &mut dyn crate::semantic::mc_func::HasFindInst,
        stmts: &mut Vec<McPhrase>,
    ) {
        // The block can be:
        // - MCAST_COND_BLOCK: has subnodes, parse each child as a phrase
        // - MCAST_NET: a single connection stmt
        // - MCAST_ATTRIBUTE_PIN / MCAST_ATTRIBUTE: single stmt
        match block.get_type() {
            MCAST_COND_BLOCK | MCAST_BODY => {
                // MCAST_COND_BLOCK: dead parser type (kept for compat).
                // MCAST_BODY: the actual wrapper for `{ ... }` conditional
                // blocks — iterate its children (net stmts / pin attributes).
                if let Some(subnodes) = block.get_sub_node() {
                    for child in subnodes.iter() {
                        let child_type = child.get_type();
                        if child_type == MCAST_NET {
                            // A NET node may contain a DECLARE or a connection stmt
                            if let Some(net_sub) = child.get_sub_node() {
                                if net_sub.get_type() == MCAST_DECLARE {
                                    continue; // skip declarations in cond blocks
                                }
                                if let Some(phrase) = McPhrase::new(&net_sub, context) {
                                    stmts.push(phrase);
                                }
                            }
                        } else if child_type == MCAST_ATTRIBUTE_PIN
                            || child_type == MCAST_ATTRIBUTE_PINADD
                            || child_type == MCAST_ATTRIBUTE
                        {
                            // Single stmt attribute
                            if let Some(phrase) = McPhrase::new(&child, context) {
                                stmts.push(phrase);
                            }
                        }
                    }
                }
            }
            MCAST_NET => {
                if let Some(net_sub) = block.get_sub_node() {
                    if let Some(phrase) = McPhrase::new(&net_sub, context) {
                        stmts.push(phrase);
                    }
                }
            }
            MCAST_ATTRIBUTE_PIN | MCAST_ATTRIBUTE_PINADD | MCAST_ATTRIBUTE => {
                if let Some(phrase) = McPhrase::new(block, context) {
                    stmts.push(phrase);
                }
            }
            _ => {
                // Try to parse the block directly as a phrase
                if let Some(phrase) = McPhrase::new(block, context) {
                    stmts.push(phrase);
                }
            }
        }
    }

    /// Evaluate conditions against parameter bindings and return matching stmts.
    ///
    /// No expansion caller holds the call site's node, so a condition that
    /// cannot be evaluated is dropped here rather than reported; see
    /// [`McConds::check_condition`].
    pub fn evaluate(&self, params: &[(McIds, String)]) -> &[McPhrase] {
        for cond_block in &self.if_blocks {
            if McConds::check_condition(&cond_block.condition, params) {
                return &cond_block.stmts;
            }
        }
        &self.else_stmts
    }
}
