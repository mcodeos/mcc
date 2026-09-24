// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::eval::{self, Compare, Value};
use crate::semantic::basic::mc_literal::strip_string_quotes;
use crate::semantic::component::mc_attr::{attr_values_text, McAttributes};
use crate::semantic::component::mc_pins::{pin_value_text, McPins};
use crate::{
    ast::{macros::*, node::AstNode},
    semantic::basic::mc_phrase::McPhrase,
    McIds,
};

/// The definition-face read surface a condition operand resolves against: the
/// enclosing definition's own pins and keys (U42). A component body declares no
/// instances, so `inst.<pin>.<key>` has no instance space here.
#[derive(Clone, Copy)]
pub struct CondDefCtx<'a> {
    pub pins: &'a McPins,
    pub attrs: &'a McAttributes,
}

/// The lexical family a condition value was written in (U144 residual 3,
/// ruled 2026-09-20): `sel = FAST` only ever equals `sel == FAST`, `sel =
/// "FAST"` only equals `sel == "FAST"`. A judge that meets a bare word with
/// a quoted string is a family mismatch — reported, not silently decided
/// by text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondFamily {
    /// A bare identifier, a keyword constant, or a name falling back to its
    /// own text — the identity face.
    Bare,
    /// A quoted string literal — the data face a string literal carries.
    Quoted,
    /// A numeric or unit literal; the value engine owns these comparisons.
    Numeric,
    /// Text read off the enclosing definition (attributes, pin keys) or
    /// otherwise without a written family. Outside the bare/quoted gate:
    /// the definition face is the data face (doc §3), and its values are
    /// not re-judged against the word families.
    Def,
}

impl CondFamily {
    /// Whether this family takes part in the bare/quoted gate.
    fn is_word_family(self) -> bool {
        matches!(self, CondFamily::Bare | CondFamily::Quoted)
    }

    /// The diagnostic label for one side of a family mismatch.
    fn label(self, text: &str) -> String {
        match self {
            CondFamily::Bare => format!("bare word '{text}'"),
            CondFamily::Quoted => format!("text '{text}'"),
            CondFamily::Numeric => format!("number '{text}'"),
            CondFamily::Def => format!("definition text '{text}'"),
        }
    }
}

/// One bound parameter as the condition evaluator sees it: the name, the
/// value text (never quoted), and the family the value was written in.
/// Carrying the family is what lets a judge tell `sel = FAST` from
/// `sel = "FAST"` (U144 residual 3) without changing the text every other
/// consumer reads.
#[derive(Debug, Clone)]
pub struct CondParam {
    pub name: McIds,
    pub text: String,
    pub family: CondFamily,
}

impl CondParam {
    /// A parameter whose family is guessed from the text alone — the shape
    /// for producers that only hold the historical `(name, text)` tuple and
    /// know nothing about how the text was written.
    pub fn guessed(name: McIds, text: String) -> Self {
        Self {
            name,
            family: guess_family(&text),
            text,
        }
    }
}

/// The family a value text was most plausibly written in, for producers
/// that hold only the text. Quoted text keeps its quotes here; numeric and
/// unit spellings answer the engine's own readers; anything else is the
/// bare face.
pub fn guess_family(text: &str) -> CondFamily {
    if text.starts_with('"') || text.starts_with('\'') {
        CondFamily::Quoted
    } else if Value::from_text(text).unit().is_some() || text.parse::<i64>().is_ok() {
        CondFamily::Numeric
    } else {
        CondFamily::Bare
    }
}

/// One member of an `in` list with its lexical family (U146 + U144
/// residual 3): a member written in another word family than the left side
/// never matches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMember {
    pub text: String,
    pub family: CondFamily,
}

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
        values: Vec<InMember>,
    },
    /// Logical composition of two judges (U145): `if (count == 5 && count > 3)`.
    /// The doubled spellings are the logical operators; the single `&`/`|`
    /// remain the bitwise judges above.
    And {
        left: Box<McCondition>,
        right: Box<McCondition>,
    },
    Or {
        left: Box<McCondition>,
        right: Box<McCondition>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McCondOperand {
    Ident(McIds),
    /// A numeric literal token (`2`, `0x01`, `100mA`) — the value engine
    /// reads it back through the one suffix table.
    Literal(String),
    /// A quoted string literal (`"FAST"`), kept with its family: the text
    /// is stored unquoted, the family marker is the variant itself
    /// (U144 residual 3).
    StrLit(String),
    /// A keyword constant (`HIGH`/`LOW`): raw text on the bare/identity face.
    Word(String),
    /// Arithmetic expression operand (U147): `count == 2 + 3`. Leaves resolve
    /// through the normal faces, then the value engine folds the operator —
    /// so unit families and normalization behave exactly as at a call site.
    Expr {
        op: eval::Op,
        left: Box<McCondOperand>,
        right: Box<McCondOperand>,
    },
}

impl std::fmt::Display for McCondOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McCondOperand::Ident(id) => write!(f, "{}", id),
            McCondOperand::Literal(s) => write!(f, "{}", s),
            McCondOperand::StrLit(s) => write!(f, "\"{}\"", s),
            McCondOperand::Word(s) => write!(f, "{}", s),
            McCondOperand::Expr { op, left, right } => {
                write!(f, "({} {} {})", left, op.symbol(), right)
            }
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
            McCondition::In { left, values } => write!(
                f,
                "{} in [{}]",
                left,
                values
                    .iter()
                    .map(|m| match m.family {
                        CondFamily::Quoted => format!("\"{}\"", m.text),
                        _ => m.text.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            McCondition::And { left, right } => write!(f, "({}) && ({})", left, right),
            McCondition::Or { left, right } => write!(f, "({}) || ({})", left, right),
        }
    }
}

impl McCondition {
    /// Whether one operand reads one of `param_names` — see
    /// [`McCondition::references_param`]. An arithmetic operand reads a param
    /// when either leaf does.
    fn operand_reads_param(op: &McCondOperand, param_names: &[String]) -> bool {
        match op {
            McCondOperand::Ident(ids) => {
                let name = ids.to_string();
                param_names.iter().any(|p| p == &name)
            }
            McCondOperand::Literal(_) | McCondOperand::StrLit(_) | McCondOperand::Word(_) => false,
            McCondOperand::Expr { left, right, .. } => {
                Self::operand_reads_param(left, param_names)
                    || Self::operand_reads_param(right, param_names)
            }
        }
    }

    /// Whether this condition reads one of `param_names`.
    ///
    /// A formal parameter is answered by the call site, so a condition that
    /// reads one is decided per instance. A consumer that decides the whole
    /// definition at once — the definition-time fold — is unsound for it
    /// (CIMP U54).
    pub fn references_param(&self, param_names: &[String]) -> bool {
        let named = |op: &McCondOperand| Self::operand_reads_param(op, param_names);
        match self {
            McCondition::In { left, .. } => named(left),
            McCondition::And { left, right } | McCondition::Or { left, right } => {
                left.references_param(param_names) || right.references_param(param_names)
            }
            McCondition::Eq { left, right }
            | McCondition::NotEq { left, right }
            | McCondition::Lt { left, right }
            | McCondition::Gt { left, right }
            | McCondition::LtEq { left, right }
            | McCondition::GtEq { left, right }
            | McCondition::BitAnd { left, right }
            | McCondition::BitOr { left, right } => named(left) || named(right),
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

    /// Whether any branch of this chain reads one of `param_names` — see
    /// [`McCondition::references_param`].
    pub fn references_param(&self, param_names: &[String]) -> bool {
        self.if_blocks
            .iter()
            .any(|c| c.condition.references_param(param_names))
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
                || node_type == MCAST_JUDGE_AND
                || node_type == MCAST_JUDGE_OR
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
                // U212: an `error(...)` clause in the body is content too —
                // keep the BODY whole so the error capture sees it; the
                // pins/attrs consumers pick their nodes from the children.
                let has_error_clause = child
                    .get_sub_node()
                    .is_some_and(|inner| inner.iter().any(|n| n.get_type() == MCAST_ERROR));
                if !found_attr || has_error_clause {
                    block_node = Some(child.clone());
                }
            } else if has_condition
                && block_node.is_none()
                && (node_type == MCAST_ATTRIBUTE_PIN
                    || node_type == MCAST_ATTRIBUTE_PINADD
                    || node_type == MCAST_ATTRIBUTE
                    || node_type == MCAST_ERROR)
            {
                // U212: a bare `error(...)` clause is the branch's body —
                // keeping it preserves the branch in the chain (dropping it
                // would misalign the remaining branch indices).
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
                || child_type == MCAST_JUDGE_AND
                || child_type == MCAST_JUDGE_OR
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
                // U212: an `error(...)` clause in the body is content — keep
                // the BODY whole (see parse_cond_if).
                let has_error_clause = child
                    .get_sub_node()
                    .is_some_and(|inner| inner.iter().any(|n| n.get_type() == MCAST_ERROR));
                if !found_attr || has_error_clause {
                    else_if_block_node = Some(child.clone());
                }
            } else if child_type == MCAST_ATTRIBUTE_PIN
                || child_type == MCAST_ATTRIBUTE_PINADD
                || child_type == MCAST_ATTRIBUTE
                || child_type == MCAST_ERROR
            {
                // U212: a bare `error(...)` clause is the branch's body.
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
            // Not a leaf judge — a logical composition (`&&`/`||`, U145) nests
            // two whole judges, so parse both sides as conditions.
            if node_type == MCAST_JUDGE_AND || node_type == MCAST_JUDGE_OR {
                let subnodes = node.get_sub_node()?;
                let mut parts = subnodes.iter();
                let left = Self::parse_condition(&parts.next()?)?;
                let right = Self::parse_condition(&parts.next()?)?;
                let (left, right) = (Box::new(left), Box::new(right));
                return Some(if node_type == MCAST_JUDGE_AND {
                    McCondition::And { left, right }
                } else {
                    McCondition::Or { left, right }
                });
            }
            return None;
        };

        // Handle "in" operator specially: extract the array of values
        if op_type_str == "in" {
            return Self::parse_in_condition(node);
        }

        let mut operands: Vec<McCondOperand> = Vec::new();

        if let Some(subnodes) = node.get_sub_node() {
            for child in subnodes.iter() {
                // Array operand: `param in [A, B, C]` reaches this collector
                // as `param == [A, B, C]` — the member list rides in as the
                // second judge child (MCAST_OPD_SQUARE_VEC).
                if child.get_type() == MCAST_OPD_SQUARE_VEC {
                    if let Some(in_cond) = Self::parse_in_array_operand(&child, operands.first())
                    {
                        return Some(in_cond);
                    }
                    continue;
                }
                match Self::parse_operand(&child) {
                    Some(operand) => operands.push(operand),
                    None => {
                        // A judge operand the collector cannot name (a call,
                        // a form with no reading) used to vanish silently:
                        // with fewer than two operands the whole judge was
                        // discarded and the if-branch never selected, with
                        // no diagnostic explaining why. The condition still
                        // reads as absent — the arm is not interpretable —
                        // but the drop is now audible (U144 condition face).
                        dlog_error(
                            crate::errcodes::COND_JUDGE_OPERAND_DROPPED,
                            &child,
                            "condition operand has a form the collector does \
                             not recognize; the whole judge is discarded, so \
                             this branch never selects",
                        );
                    }
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

    /// The member-list side of `param in [A, B, C]` — the `MCAST_OPD_SQUARE_VEC`
    /// judge child. Members carry their written family (U146 + U144
    /// residual 3): a quoted member reads as [`CondFamily::Quoted`], a bare
    /// member as [`CondFamily::Bare`], and a member of another word family
    /// than the left side never matches it. `None` when no member yields a
    /// value — the caller then keeps collecting, and the judge falls
    /// through unread.
    fn parse_in_array_operand(
        vec_node: &AstNode,
        left: Option<&McCondOperand>,
    ) -> Option<McCondition> {
        let left = left?;
        let mut values = Vec::new();
        let vec_first = vec_node.get_sub_node()?;
        let mut current = Some(vec_first);
        while let Some(item) = current {
            match item.get_type() {
                MCAST_STRING => {
                    // Guarded accessor: the C parser can emit a NULL/small .data.
                    if let Some(val) = item.data_as_cstr().and_then(|c| c.to_str().ok()) {
                        values.push(InMember {
                            text: strip_string_quotes(val).to_string(),
                            family: CondFamily::Quoted,
                        });
                    }
                }
                // Bare member: the value is the identifier's own text.
                MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_OPD | MCAST_EXPRESSION => {
                    if let Some(ids) = Self::bare_member_ids(&item) {
                        values.push(InMember {
                            text: ids.to_string(),
                            family: CondFamily::Bare,
                        });
                    }
                }
                _ => {}
            }
            current = item.get_next();
        }
        if values.is_empty() {
            return None;
        }
        Some(McCondition::In {
            left: left.clone(),
            values,
        })
    }

    /// The `McIds` a bare array member carries — directly as an ids node, or
    /// wrapped one level down in an `opd`/`expression` node.
    fn bare_member_ids(item: &AstNode) -> Option<McIds> {
        match item.get_type() {
            MCAST_OPD | MCAST_EXPRESSION => {
                let inner = item.get_sub_node()?;
                Self::bare_member_ids(&inner)
            }
            _ => McIds::new(item),
        }
    }

    /// One judge operand face: literals and identifiers as before, keyword
    /// constants by raw text, and arithmetic expression nodes (`2 + 3`, U147)
    /// as `Expr` trees the value engine folds at evaluation time. `None` for
    /// a node shape this face does not hold.
    fn parse_operand(node: &AstNode) -> Option<McCondOperand> {
        match node.get_type() {
            MCAST_ID | MCAST_IDA => McIds::new(node).map(McCondOperand::Ident),
            MCAST_INT | MCAST_HEX | MCAST_FLOAT | MCAST_UVALUE => {
                Some(McCondOperand::Literal(node.to_string().unwrap_or_default()))
            }
            MCAST_STRING => {
                // Guarded accessor: the C parser can emit a NULL/small .data.
                let val = node.data_as_cstr()?.to_str().ok()?;
                // The text is kept unquoted, but as a `StrLit`: the family
                // the author wrote survives the collector, so the judge can
                // refuse a bare-word counterpart (U144 residual 3).
                Some(McCondOperand::StrLit(strip_string_quotes(val).to_string()))
            }
            MCAST_CONST => {
                // Keyword constants (`HIGH`/`LOW`) carry their raw text as
                // data — the same face the default-value extractor reads,
                // so `sel == HIGH` compares against the same text that
                // `sel = HIGH` binds. No arm here meant the operand was
                // silently dropped and the whole condition parsed as
                // `None` (if-branch discarded, else always taken).
                let val = node.data_as_cstr()?.to_str().ok()?;
                Some(McCondOperand::Word(val.to_string()))
            }
            MCAST_OPD => {
                // Read the whole `MCAST_IDS` node, not its first child:
                // a dotted operand (`A.desc`) carries its dot segments
                // as siblings under `MCAST_IDS`, so taking the first
                // child silently truncates it to `A`.
                let opd_subnode = node.get_sub_node()?;
                let ids = McIds::new(&opd_subnode)?;
                Some(McCondOperand::Ident(ids))
            }
            MCAST_EXPRESSION => {
                // A phrase operand (`2 + 3` on a judge side) is wrapped in an
                // EXPRESSION node; the arithmetic lives one level down.
                let inner = node.get_sub_node()?;
                Self::parse_operand(&inner)
            }
            MCAST_OPD_PLUS | MCAST_OPD_MINUS | MCAST_OPD_MULTI | MCAST_OPD_DIVID => {
                let op = match node.get_type() {
                    MCAST_OPD_PLUS => eval::Op::Add,
                    MCAST_OPD_MINUS => eval::Op::Sub,
                    MCAST_OPD_MULTI => eval::Op::Mul,
                    _ => eval::Op::Div,
                };
                let subnodes = node.get_sub_node()?;
                let mut parts = subnodes.iter();
                let left = Box::new(Self::parse_operand(&parts.next()?)?);
                let right = Box::new(Self::parse_operand(&parts.next()?)?);
                Some(McCondOperand::Expr { op, left, right })
            }
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
                            values.push(InMember {
                                text: clean_val,
                                family: CondFamily::Quoted,
                            });
                        }
                    } else if item.get_type() == MCAST_ID
                        || item.get_type() == MCAST_IDA
                        || item.get_type() == MCAST_IDS
                        || item.get_type() == MCAST_OPD
                        || item.get_type() == MCAST_EXPRESSION
                    {
                        // Bare member (U146): the value is the identifier's own
                        // text on the bare/identity face.
                        if let Some(ids) = Self::bare_member_ids(&item) {
                            values.push(InMember {
                                text: ids.to_string(),
                                family: CondFamily::Bare,
                            });
                        }
                    } else if item.get_type() == MCAST_INT
                        || item.get_type() == MCAST_HEX
                        || item.get_type() == MCAST_FLOAT
                    {
                        // Numeric member: the literal's own text — the same
                        // face an `==` judge compares with. No arm here meant
                        // `n in [3, 4]` collected an empty member list and
                        // read as false for every argument.
                        if let Some(text) = item.to_string() {
                            values.push(InMember {
                                text,
                                family: CondFamily::Numeric,
                            });
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
        params: &[CondParam],
        def: Option<CondDefCtx<'_>>,
        anchor: Option<&AstNode>,
    ) -> Option<AstNode> {
        let mut failure: Option<eval::EvalError> = None;
        for cond in &self.if_blocks {
            match Self::check_condition_result(&cond.condition, params, def) {
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

    pub fn check_condition(
        cond: &McCondition,
        params: &[CondParam],
        def: Option<CondDefCtx<'_>>,
    ) -> bool {
        // A condition with no node cannot carry a diagnostic, so the error half
        // is dropped here; `check_condition_result` is the same evaluation with
        // the failure preserved, and `McConds::evaluate` is the positioned
        // caller that reports it.
        Self::check_condition_result(cond, params, def).unwrap_or(false)
    }

    /// Evaluate one condition through the value engine (doc/eval V7). The
    /// operands are bound argument text, so they enter the engine by text and
    /// are normalized by the one suffix table — `1200mV` and `1.2V` are the
    /// same value, which the old suffix-stripping comparison could not see.
    pub fn check_condition_result(
        cond: &McCondition,
        params: &[CondParam],
        def: Option<CondDefCtx<'_>>,
    ) -> Result<bool, eval::EvalError> {
        // Handle "in" condition separately (different structure)
        if let McCondition::In { left, values } = cond {
            let (left_text, left_family) = match left {
                McCondOperand::Expr { .. } => {
                    let val = Self::resolve_operand_value(left, params, def)?;
                    (val.text(), CondFamily::Numeric)
                }
                _ => Self::resolve_operand_typed(left, params, def),
            };
            let left_val = Value::from_text(&left_text);
            for member in values {
                // A member in the other word family than the left side never
                // matches it (U144 residual 3): the two texts meeting would
                // be exactly the cross-family equality the 2026-09-20 ruling
                // abolished. Non-word families keep the engine's own gate.
                if left_family.is_word_family()
                    && member.family.is_word_family()
                    && left_family != member.family
                {
                    continue;
                }
                if eval::satisfies(Compare::Eq, &left_val, &Value::from_text(&member.text))? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }

        // Logical composition (U145): `&&`/`||` between two judges,
        // short-circuited the way the written form reads.
        if let McCondition::And { left, right } | McCondition::Or { left, right } = cond {
            let is_and = matches!(cond, McCondition::And { .. });
            let left_val = Self::check_condition_result(left, params, def)?;
            if left_val != is_and {
                return Ok(left_val);
            }
            return Self::check_condition_result(right, params, def);
        }

        // Bitwise conditions (`if (address & 0x01)` / `if (address | 0x01)`):
        // apply the operation to the two integers and treat a non-zero result
        // as true. A non-integer operand keeps its historical reading — the
        // condition is simply not satisfied.
        if let McCondition::BitAnd { left, right } | McCondition::BitOr { left, right } = cond {
            let left_val = Self::resolve_operand_value(left, params, def)?;
            let right_val = Self::resolve_operand_value(right, params, def)?;
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
            McCondition::BitAnd { .. }
            | McCondition::BitOr { .. }
            | McCondition::In { .. }
            | McCondition::And { .. }
            | McCondition::Or { .. } => unreachable!(),
        };

        let (left_text, left_family) = match left_op {
            // An arithmetic operand keeps the engine's own reading (U147): it
            // folds to a value, so its family is the numeric face and the
            // word gate below cannot apply to it.
            McCondOperand::Expr { .. } => {
                let val = Self::resolve_operand_value(left_op, params, def)?;
                (val.text(), CondFamily::Numeric)
            }
            _ => Self::resolve_operand_typed(left_op, params, def),
        };
        let (right_text, right_family) = match right_op {
            McCondOperand::Expr { .. } => {
                let val = Self::resolve_operand_value(right_op, params, def)?;
                (val.text(), CondFamily::Numeric)
            }
            _ => Self::resolve_operand_typed(right_op, params, def),
        };
        // The strict word-family gate (U144 residual 3, ruled 2026-09-20): a
        // bare word never meets a quoted string. Both sides are text values,
        // so without this gate the engine would decide the judge by text
        // alone — the exact transcription the ruling abolished. The judge
        // reads as unsatisfied wherever the error is swallowed, and is
        // reported wherever the caller holds an anchor.
        if left_family.is_word_family()
            && right_family.is_word_family()
            && left_family != right_family
        {
            return Err(eval::EvalError::FamilyMismatch {
                lhs: left_family.label(&left_text),
                rhs: right_family.label(&right_text),
            });
        }
        eval::satisfies(
            cmp,
            &Value::from_text(&left_text),
            &Value::from_text(&right_text),
        )
    }

    /// The value an operand stands for: the plain text face for leaves, and
    /// for an arithmetic operand (`count == 2 + 3`, U147) the engine folds the
    /// operator after both leaves resolve — so unit families and the
    /// `1200mV`/`1.2V` normalization are the engine's, not the collector's.
    fn resolve_operand_value(
        op: &McCondOperand,
        params: &[CondParam],
        def: Option<CondDefCtx<'_>>,
    ) -> Result<Value, eval::EvalError> {
        match op {
            McCondOperand::Expr { op, left, right } => {
                let lv = Self::resolve_operand_value(left, params, def)?;
                let rv = Self::resolve_operand_value(right, params, def)?;
                eval::apply(*op, &lv, &rv)
            }
            other => Ok(Value::from_text(&Self::resolve_operand_typed(other, params, def).0)),
        }
    }

    /// The text and lexical family an operand stands for (U144 residual 3):
    /// the bound argument when the name is a formal param — answering with
    /// the family that value was written in — else the value the enclosing
    /// definition declares under that name (`<key>` or `<pin>.<key>`, U42,
    /// the definition face), else the name itself on the bare face.
    fn resolve_operand_typed(
        op: &McCondOperand,
        params: &[CondParam],
        def: Option<CondDefCtx<'_>>,
    ) -> (String, CondFamily) {
        match op {
            McCondOperand::Ident(ids) => {
                let name = ids.to_string();
                for param in params {
                    if param.name.to_string() == name {
                        return (param.text.clone(), param.family);
                    }
                }
                if let Some(text) = def.and_then(|def| Self::read_def_value(def, &name)) {
                    return (text, CondFamily::Def);
                }
                (name, CondFamily::Bare)
            }
            McCondOperand::Literal(val) => (val.clone(), CondFamily::Numeric),
            McCondOperand::StrLit(val) => (val.clone(), CondFamily::Quoted),
            McCondOperand::Word(val) => (val.clone(), CondFamily::Bare),
            McCondOperand::Expr { .. } => {
                // An expression is folded by the engine in
                // `resolve_operand_value`; this text face only sees leaves.
                unreachable!()
            }
        }
    }

    /// The text `def` declares under `name` — a pin row's value key
    /// (`<pin>.<key>`) first, then an attribute key, mirroring the body name
    /// face (attributes before pin names, G9). `None` for a name the definition
    /// does not hold; the caller then falls back to the name itself.
    fn read_def_value(def: CondDefCtx<'_>, name: &str) -> Option<String> {
        if let Some((pin_ref, key)) = name.split_once('.') {
            if let Some(pin) = def.pins.pin_of_ref(pin_ref) {
                return pin_value_text(pin, key);
            }
        }
        def.attrs
            .find(&McIds::from(name))
            .and_then(|attr| attr_values_text(attr.values.iter()))
    }
}

// McFuncConds — parsed conditional blocks, storing McPhrase stmts for evaluation at instantiation
// time

/// A single parsed conditional branch
#[derive(Debug, Clone)]
pub struct McCondBlock {
    pub condition: McCondition,
    pub stmts: Vec<McPhrase>,
    /// Source byte offset of each stmt in `stmts`, parallel by index
    /// (CIMP U166 ruling B): a cond stmt's products anchor at the stmt's own
    /// position, not the func header. Missing entries fall back to the header.
    pub stmt_offsets: Vec<u32>,
}

/// A parsed collection of conditional blocks (if/else if/else)
#[derive(Debug, Clone)]
pub struct McFuncConds {
    pub if_blocks: Vec<McCondBlock>,
    pub else_stmts: Vec<McPhrase>,
    /// Parallel to `else_stmts`, same discipline as [`McCondBlock::stmt_offsets`].
    pub else_stmt_offsets: Vec<u32>,
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
            let mut offsets = Vec::new();
            // The block is an AstNode; parse its content into McPhrase stmts
            Self::parse_block_stmts(&cond.block, context, &mut stmts, &mut offsets);
            if_blocks.push(McCondBlock {
                condition: cond.condition.clone(),
                stmts,
                stmt_offsets: offsets,
            });
        }

        let mut else_offsets = Vec::new();
        if let Some(else_block) = &conds.else_block {
            Self::parse_block_stmts(else_block, context, &mut else_stmts, &mut else_offsets);
        }

        McFuncConds {
            if_blocks,
            else_stmts,
            else_stmt_offsets: else_offsets,
        }
    }

    /// Parse an AstNode block into McPhrase stmts
    fn parse_block_stmts(
        block: &AstNode,
        context: &mut dyn crate::semantic::mc_func::HasFindInst,
        stmts: &mut Vec<McPhrase>,
        offsets: &mut Vec<u32>,
    ) {
        // The block can be:
        // - MCAST_COND_BLOCK: has subnodes, parse each child as a phrase
        // - MCAST_NET: a single connection stmt
        // - MCAST_ATTRIBUTE_PIN / MCAST_ATTRIBUTE: single stmt
        match block.get_type() {
            MCAST_COND_BLOCK | MCAST_BODY => {
                // MCAST_COND_BLOCK: dead parser type (kept for compat).
                // MCAST_BODY: the actual wrapper for `{ ... }` conditional
                // blocks — iterate its clauses (net stmts / pin attributes),
                // with an in-body partition read as transparent so a `block`
                // written inside a branch cannot swallow its own stmts.
                for child in block.clause_list() {
                    let child_type = child.get_type();
                    if child_type == MCAST_NET {
                        // A NET node may contain a DECLARE or a connection stmt
                        if let Some(net_sub) = child.get_sub_node() {
                            if net_sub.get_type() == MCAST_DECLARE {
                                continue; // skip declarations in cond blocks
                            }
                            if let Some(phrase) = McPhrase::new(&net_sub, context) {
                                offsets.push(net_sub.get_pos() as u32);
                                stmts.push(phrase);
                            }
                        }
                    } else if child_type == MCAST_ATTRIBUTE_PIN
                        || child_type == MCAST_ATTRIBUTE_PINADD
                        || child_type == MCAST_ATTRIBUTE
                    {
                        // Single stmt attribute
                        if let Some(phrase) = McPhrase::new(&child, context) {
                            offsets.push(child.get_pos() as u32);
                            stmts.push(phrase);
                        }
                    }
                }
            }
            MCAST_NET => {
                if let Some(net_sub) = block.get_sub_node() {
                    if let Some(phrase) = McPhrase::new(&net_sub, context) {
                        offsets.push(net_sub.get_pos() as u32);
                        stmts.push(phrase);
                    }
                }
            }
            MCAST_ATTRIBUTE_PIN | MCAST_ATTRIBUTE_PINADD | MCAST_ATTRIBUTE => {
                if let Some(phrase) = McPhrase::new(block, context) {
                    offsets.push(block.get_pos() as u32);
                    stmts.push(phrase);
                }
            }
            _ => {
                // Try to parse the block directly as a phrase
                if let Some(phrase) = McPhrase::new(block, context) {
                    offsets.push(block.get_pos() as u32);
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
    pub fn evaluate(&self, params: &[CondParam]) -> (&[McPhrase], &[u32]) {
        for cond_block in &self.if_blocks {
            if McConds::check_condition(&cond_block.condition, params, None) {
                return (&cond_block.stmts, &cond_block.stmt_offsets);
            }
        }
        (&self.else_stmts, &self.else_stmt_offsets)
    }
}
