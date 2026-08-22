// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::mc_opd::McOpd;
pub use super::mc_paramd::*;
use crate::semantic::component::mc_attr::{McAttrVal, McAttribute};
use crate::semantic::mc_func::HasFindInst;
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    semantic::{
        basic::mc_expr::McExpression,
        basic::mc_literal::{McConst, McHex, McLiteral, McString},
        basic::mc_phrase::McPhrase,
        basic::mc_uval::McUnitValue,
    },
    McFloat, McIds, McInt,
};

/// Global counter for R05 UNRESOLVED: unit-typed arguments that cannot claim any formal slot.
pub static R05_UNRESOLVED_UNIT: AtomicUsize = AtomicUsize::new(0);

/// Reset the R05 counter (call before each build run).
pub fn reset_r05_counter() {
    R05_UNRESOLVED_UNIT.store(0, Ordering::Relaxed);
}

// ============================================================================
// Parameter values (actual arguments)
// ============================================================================

/// Parameter value type (actual arguments passed at call time)
#[derive(Debug, Clone)]
pub enum McParamValue {
    NONE(String),
    NC(String),
    Const(McConst),
    Int(McInt),
    Hex(McHex),
    Float(McFloat),
    String(McString),
    UValue(McUnitValue),

    Ids(McIds),
    Opd(McOpd),

    Phrase(Box<McPhrase>),
    InlineAttrs(Vec<McAttribute>),

    Set(Vec<McParamValue>),
}

impl McParamValue {
    /// Parse parameter value from an AST node
    pub fn new(node: &AstNode, context: &mut dyn HasFindInst) -> Option<Self> {
        match node.get_type() {
            // Lemon automatically creates MCAST_* nodes for non-terminals, e.g. mc_param creates MCAST_PARAM
            // Need to extract the sub-node for processing
            MCAST_PARAM => {
                if let Some(sub) = node.get_sub_node() {
                    return McParamValue::new(&sub, context);
                }
                None
            }

            // Placeholder _ used for .Cap(_) etc.
            MCAST_OPD_USCORE => Some(McParamValue::NONE(String::from("_"))),

            MCAST_OPD_NC => Some(McParamValue::NC(String::from("NC"))),
            MCAST_CONST => McConst::new(node).map(McParamValue::Const),
            MCAST_INT => McInt::new(node).map(McParamValue::Int),
            MCAST_HEX => McHex::new(node).map(McParamValue::Hex),
            MCAST_FLOAT => McFloat::new(node).map(McParamValue::Float),
            MCAST_STRING => McString::new(node).map(McParamValue::String),
            MCAST_UVALUE | MCAST_UVALUE_AT | MCAST_RANGE_PLUSMINUS | MCAST_OPD_TILDE => {
                Self::uvalue_or_range(node)
            }

            // Identifier
            MCAST_ID | MCAST_IDA | MCAST_IDS => McIds::new(node).map(McParamValue::Ids),

            // Operand
            MCAST_OPD => {
                if let Some(opd) = McOpd::new(node) {
                    Some(McParamValue::Opd(opd))
                } else {
                    // Fallback: try to parse as phrase (handles bus references like lpa.VDD)
                    McPhrase::new(node, context)
                        .map(|phrase| McParamValue::Phrase(Box::new(phrase)))
                }
            }

            // Handle function body nodes - support attribute block as parameter
            MCAST_BODY => Self::inline_attrs_from_body(node),

            // Square bracket vector: [a -> b] is parsed as MCAST_SQUARE_VEC
            MCAST_SQUARE_VEC => {
                if let Some(subnodes) = node.get_sub_node() {
                    let values: Vec<McParamValue> = subnodes
                        .iter()
                        .filter_map(|n| McParamValue::new(&n, context))
                        .collect();
                    if !values.is_empty() {
                        return Some(McParamValue::Set(values));
                    }
                }
                None
            }

            // & square bracket vector: &[a b] is parsed as MCAST_OPD_SQUARE_VEC
            MCAST_OPD_SQUARE_VEC => {
                if let Some(subnodes) = node.get_sub_node() {
                    let values: Vec<McParamValue> = subnodes
                        .iter()
                        .filter_map(|n| McParamValue::new(&n, context))
                        .collect();
                    if !values.is_empty() {
                        return Some(McParamValue::Set(values));
                    }
                }
                None
            }

            // Net/arithmetic expressions
            MCAST_OPD_MINUS | MCAST_OPD_PLUS | MCAST_OPD_RIGHTARROW | MCAST_OPD_LEFTARROW
            | MCAST_OPD_MULTI | MCAST_OPD_DIVID => {
                McPhrase::new(node, context).map(|p| McParamValue::Phrase(Box::new(p)))
            }

            // Nested function call as argument value
            MCAST_OPD_FCALL => {
                McPhrase::new(node, context).map(|p| McParamValue::Phrase(Box::new(p)))
            }
            _ => None,
        }
    }

    /// Parse a single unit value or a range / plus-minus form
    /// (`2.5V~5.5V`, `±20%`). The RANGE_PLUSMINUS / TILDE AST nodes carry
    /// the marker in the node type, not in the child data, so the author's
    /// notation is rebuilt from the children: one child → `±X`, two linked
    /// children → `X~Y` (TILDE) or `X±Y` (RANGE_PLUSMINUS).
    pub(crate) fn uvalue_or_range(node: &AstNode) -> Option<Self> {
        let t = node.get_type();
        if t == MCAST_RANGE_PLUSMINUS || t == MCAST_OPD_TILDE {
            if let Some(left) = node.get_sub_node() {
                let l = left.to_string().unwrap_or_default();
                let text = if let Some(right) = left.get_next() {
                    let r = right.to_string().unwrap_or_default();
                    let op = if t == MCAST_OPD_TILDE { "~" } else { "±" };
                    format!("{l}{op}{r}")
                } else {
                    format!("±{l}")
                };
                // The range/plus-minus node's first child is the low bound
                // (e.g. `2.5V` in `2.5V~5.5V`, `20%` in `±20%`); parse that
                // child as the value so the value/unit pair is meaningful,
                // then echo the full author notation via the raw text.
                if let Some(uv) = McUnitValue::new(&left) {
                    return Some(McParamValue::UValue(uv.with_raw_text(text)));
                }
            }
        }
        McUnitValue::new(node).map(McParamValue::UValue)
    }

    /// Try to convert to constant
    pub fn as_const(&self) -> Option<&McConst> {
        match self {
            McParamValue::Const(c) => Some(c),
            _ => None,
        }
    }

    /// Parse an attribute block argument `{ cap = 1uF; volt = 50V }` into an
    /// [`McParamValue::InlineAttrs`] value, one attribute per named-argument
    /// entry.
    fn inline_attrs_from_body(node: &AstNode) -> Option<Self> {
        let subnode = node.get_sub_node()?;
        let mut attributes = Vec::new();
        for child in subnode
            .iter()
            .filter(|child| child.is_type(MCAST_ATTRIBUTE))
        {
            if let Some(attr) = McAttribute::new(&child) {
                attributes.push(attr);
            }
        }
        Some(McParamValue::InlineAttrs(attributes))
    }

    /// Parse a parameter value without an instance-lookup context.
    ///
    /// Used for instance construction args (e.g. `mcu(V3V3, V1V2)`,
    /// `::DC(3.3V)`) that are parsed before a function/module context
    /// exists. Handles every literal kind of [`McParamValue::new`] plus
    /// plain identifiers and attribute bodies (`{ cap = 1uF; volt = 50V }`,
    /// as [`McParamValue::InlineAttrs`]); kinds that need a [`HasFindInst`]
    /// context (opd expressions, function calls) return `None` and are
    /// dropped, matching the historical instance-arg behavior.
    ///
    /// Plain identifiers keep the `Ids` representation: an OPD-wrapped id
    /// (`param > opd > ids > id V3V3`) and a square vector (`[VDD, GND]`)
    /// both produce `McParamValue::Ids` — [`McParamValue::new`] would
    /// yield `Opd` / `Set` for those shapes, which changes downstream
    /// binding. `McIds::new` already unwraps `MCAST_PARAM` / `MCAST_OPD`
    /// wrappers, so no manual unpacking is needed.
    pub fn new_no_ctx(node: &AstNode) -> Option<Self> {
        match node.get_type() {
            // MCAST_PARAM wraps exactly one value node; unwrap it and re-dispatch.
            MCAST_PARAM => node.get_sub_node().and_then(|sub| Self::new_no_ctx(&sub)),
            // Placeholder _ used for .Cap(_) etc.
            MCAST_OPD_USCORE => Some(McParamValue::NONE(String::from("_"))),
            MCAST_OPD_NC => Some(McParamValue::NC(String::from("NC"))),
            MCAST_BODY => Self::inline_attrs_from_body(node),
            MCAST_CONST => McConst::new(node).map(McParamValue::Const),
            MCAST_INT => McInt::new(node).map(McParamValue::Int),
            MCAST_HEX => McHex::new(node).map(McParamValue::Hex),
            MCAST_FLOAT => McFloat::new(node).map(McParamValue::Float),
            MCAST_STRING => McString::new(node).map(McParamValue::String),
            MCAST_UVALUE | MCAST_UVALUE_AT | MCAST_RANGE_PLUSMINUS | MCAST_OPD_TILDE => {
                Self::uvalue_or_range(node)
            }
            // Bare (non-OPD) square vector `[VDD, GND]` as a direct value
            // child — McIds::new accepts it since the grammar's non-`&`
            // variant shares the MCAST_OPD_SQUARE_VEC member shape, so the
            // argument is kept as Ids/Square instead of being dropped.
            MCAST_SQUARE_VEC => McIds::new(node)
                .filter(|ids| !ids.is_empty())
                .map(McParamValue::Ids),
            // Declared unit value (`name::DC(5V)`): a declaration structure,
            // not a plain argument value. Defensively extract the declared
            // name from the MCAST_INSTANCE child (mirroring
            // McUnitValueDeclare::new) so the argument is never silently
            // dropped; unit/default are only meaningful in declaration
            // contexts and have no McParamValue representation.
            MCAST_DECLARE_UV => {
                let sub = node.get_sub_node()?;
                let inst = sub.get_next()?;
                if inst.get_type() != MCAST_INSTANCE {
                    return None;
                }
                let name_node = inst.get_sub_node()?;
                McIds::new(&name_node)
                    .filter(|ids| !ids.is_empty())
                    .map(McParamValue::Ids)
            }
            // Plain identifiers (MCAST_ID / IDA / IDS / MCAST_OPD / square
            // vectors): McIds::new unwraps wrapper layers and yields Ids.
            _ => McIds::new(node)
                .filter(|ids| !ids.is_empty())
                .map(McParamValue::Ids),
        }
    }

    /// Try to convert to identifier
    pub fn as_ids(&self) -> Option<&McIds> {
        match self {
            McParamValue::Ids(ids) => Some(ids),
            //McParamValue::Opd(opd) => opd.as_ids(),
            _ => None,
        }
    }

    /// Check if it is a constant
    pub fn is_const(&self) -> bool {
        matches!(self, McParamValue::Const(_))
    }

    /// Check if it is an identifier
    pub fn is_ids(&self) -> bool {
        matches!(self, McParamValue::Ids(_))
    }

    /// Check if it is a Set
    pub fn is_set(&self) -> bool {
        matches!(self, McParamValue::Set(_))
    }

    /// Check if it is a named parameter in attribute form
    ///
    /// Named parameter syntax: `{ cap = 1uF; volt = 50V }`
    /// Corresponds to the `McParamValue::InlineAttrs(...)` variant.
    pub fn is_named_param(&self) -> bool {
        matches!(self, McParamValue::InlineAttrs(_))
    }

    /// Check whether any attribute inside this named-parameter block matches
    /// the given formal parameter name.
    ///
    /// Matching is case-insensitive on the attribute key; a bracketed key
    /// (`pins[6:9]`) matches its root segment (`pins`).
    pub fn matches_param_name(&self, name: &str) -> bool {
        match self {
            McParamValue::InlineAttrs(attrs) => attrs.iter().any(|a| attr_key_matches(&a.id, name)),
            _ => false,
        }
    }

    /// Try to get the attribute parameter's name
    ///
    /// Only valid for InlineAttrs; returns the first attribute's `id` string.
    pub fn get_param_name(&self) -> Option<String> {
        match self {
            McParamValue::InlineAttrs(attrs) => attrs.first().map(|a| a.id.to_string()),
            _ => None,
        }
    }
}

/// Case-insensitive key match against a formal parameter name, falling back
/// to the root segment for indexed keys (`pins[6:9]` → `pins`).
fn attr_key_matches(key: &McIds, name: &str) -> bool {
    let full = key.to_string();
    if full.eq_ignore_ascii_case(name) {
        return true;
    }
    key.root_name()
        .is_some_and(|r| r.eq_ignore_ascii_case(name))
}

/// Convert an attribute value into a [`McParamValue`] so a named argument can
/// participate in normal binding. Multi-value / unconvertible forms bind as
/// `NONE` (unspecified); the name itself is still claimed by Round 1.
fn attr_val_to_param_value(val: &McAttrVal) -> McParamValue {
    match val {
        McAttrVal::AttrLiteral(lit) => match lit {
            McLiteral::Int(i) => McParamValue::Int(i.clone()),
            McLiteral::Hex(h) => McParamValue::Hex(h.clone()),
            McLiteral::Float(f) => McParamValue::Float(f.clone()),
            McLiteral::String(s) => McParamValue::String(s.clone()),
            McLiteral::Const(c) => McParamValue::Const(c.clone()),
            McLiteral::Uval(u) => McParamValue::UValue(u.clone()),
        },
        McAttrVal::AttrVariable(opd, _) => match opd {
            McOpd::Id(ids) => McParamValue::Ids(ids.clone()),
            McOpd::This(ids) => McParamValue::Opd(McOpd::This(ids.clone())),
            McOpd::Pins(ids) => McParamValue::Opd(McOpd::Pins(ids.clone())),
            McOpd::Uscore => McParamValue::NONE(String::from("_")),
        },
        McAttrVal::AttrExpr(expr) => expr_to_param_value(expr),
        McAttrVal::Attributes(_) | McAttrVal::KVS(_) => McParamValue::NONE(String::from("_")),
    }
}

/// Convert a subset of [`McExpression`] forms into [`McParamValue`]; anything
/// too complex to carry as a parameter value becomes `NONE` (unspecified).
fn expr_to_param_value(expr: &McExpression) -> McParamValue {
    match expr {
        McExpression::Int(i) => McParamValue::Int(i.clone()),
        McExpression::Float(f) => McParamValue::Float(f.clone()),
        McExpression::String(s) => McParamValue::String(s.clone()),
        McExpression::UnitValue(u) => McParamValue::UValue(u.clone()),
        McExpression::UnitValueAt(u) => McParamValue::UValue(u.left.clone()),
        McExpression::Const(c) => McParamValue::Const(c.clone()),
        McExpression::Variable(opd) => match opd {
            McOpd::Id(ids) => McParamValue::Ids(ids.clone()),
            other => McParamValue::Opd(other.clone()),
        },
        McExpression::Set(items) => {
            McParamValue::Set(items.iter().map(expr_to_param_value).collect())
        }
        // `±X` / `X~Y` ranges: bind the right (nominal) bound as the value.
        McExpression::Range(_, right) | McExpression::Slice(_, right) => expr_to_param_value(right),
        McExpression::Plus(l, r) | McExpression::Minus(l, r) => {
            // Arithmetic on values: keep the left operand as a best effort.
            expr_to_param_value(l).or_best_effort(expr_to_param_value(r))
        }
        _ => McParamValue::NONE(String::from("_")),
    }
}

impl McParamValue {
    /// Prefer the primary value; fall back to the alternative when `self` is
    /// `NONE` (used only inside the attribute-value conversion above).
    fn or_best_effort(self, alt: McParamValue) -> McParamValue {
        match self {
            McParamValue::NONE(_) => alt,
            v => v,
        }
    }
}

impl std::fmt::Display for McParamValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McParamValue::NONE(_) => write!(f, "_"),
            McParamValue::NC(_) => write!(f, "NC"),
            McParamValue::Const(c) => write!(f, "{c}"),
            McParamValue::Int(mc_int) => write!(f, "{mc_int}"),
            McParamValue::Hex(mc_hex) => write!(f, "{mc_hex}"),
            McParamValue::Float(mc_float) => write!(f, "{mc_float}"),
            McParamValue::String(s) => write!(f, "{}", s.value),
            McParamValue::UValue(mc_unit_value) => write!(f, "{mc_unit_value}"),
            McParamValue::Ids(ids) => write!(f, "{ids}"),
            McParamValue::Opd(opd) => write!(f, "{opd}"),
            McParamValue::InlineAttrs(attrs) => {
                write!(f, "[")?;
                for (i, attr) in attrs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{attr}")?;
                }
                write!(f, "]")
            }
            McParamValue::Set(values) => {
                write!(f, "[")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            McParamValue::Phrase(mc_phrase) => write!(f, "{mc_phrase}"),
        }
    }
}

// ============================================================================
// Auxiliary structures
// ============================================================================

/// Function call (as parameter value)
/*#[derive(Debug, Clone)]
pub struct McParamFuncCall {
    /// Caller (if any): the `net` in `net.rc2`
    pub caller: Option<McIds>,

    /// Function/class name: `CAP`, `rc2`, `filter`
    pub name: McIds,

    /// Parameter list
    pub params: Vec<McParamValue>,

    /// Chained call: the `.filter(dc24v)` part
    pub chain: Option<Box<McParamFuncCall>>,
}

impl McParamFuncCall {
    pub fn new(node: &AstNode) -> Option<Self> {
        // MCAST_OPD_FCALL
        // |- caller - MCAST_NAME - MCAST_PARAM
        //
        // Example:
        // CAP(0.1uF, 50V)
        // net.rc2(2Ω,2.2uF).filter(dc24v)

        let subnode = node.get_sub_node()?;

        let mut caller: Option<McIds> = None;
        let mut name: Option<McIds> = None;
        let mut params: Vec<McParamValue> = Vec::new();

        for each in subnode.iter() {
            match each.get_type() {
                MCAST_NAME => {
                    let snode = each.get_sub_node().expect(MISSING_SUBNODE);
                    name = McIds::new(&snode);
                }

                MCAST_PARAMS => {
                    if let Some(param_nodes) = each.get_sub_node() {
                        for param_node in param_nodes.iter() {
                            if let Some(value) = McParamValue::new(&param_node) {
                                params.push(value);
                            }
                        }
                    }
                }

                // Caller may be in various opd forms
                MCAST_ID | MCAST_OPD_DOT => {
                    caller = McIds::new(&each);
                }

                _ => {
                    // Other types may be part of a chained call
                }
            }
        }

        Some(Self {
            caller,
            name: name?,
            params,
            chain: None, // Chained calls need separate handling
        })
    }

    /// Get the complete call path
    pub fn full_name(&self) -> String {
        let mut result = String::new();
        if let Some(ref caller) = self.caller {
            result.push_str(&caller.to_string());
            result.push('.');
        }
        result.push_str(&self.name.to_string());
        result
    }
}

impl std::fmt::Display for McParamFuncCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(", self.full_name())?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param)?;
        }
        write!(f, ")")?;

        if let Some(ref chain) = self.chain {
            write!(f, ".{}", chain)?;
        }

        Ok(())
    }
}
*/

/*
/// Net expression (as parameter value)
#[derive(Debug, Clone)]
pub struct McParamNetExpr {
    /// Expression type
    pub op: NetExprOp,

    /// Left operand
    pub left: McParamValue,

    /// Right operand
    pub right: McParamValue,
}

/// Net expression operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetExprOp {
    /// `-` series
    Minus,
    /// `+` parallel
    Plus,
    /// `->` right arrow
    RightArrow,
    /// `<-` left arrow
    LeftArrow,
}

impl McParamNetExpr {
    pub fn new(node: &AstNode) -> Option<Self> {
        let op = match node.get_type() {
            MCAST_OPD_MINUS => NetExprOp::Minus,
            MCAST_OPD_PLUS => NetExprOp::Plus,
            MCAST_OPD_RIGHTARROW => NetExprOp::RightArrow,
            MCAST_OPD_LEFTARROW => NetExprOp::LeftArrow,
            _ => return None,
        };

        let left_node = node.get_sub_node()?;
        let right_node = left_node.get_next()?;

        let left = McParamValue::new(&left_node)?;
        let right = McParamValue::new(&right_node)?;

        Some(Self { op, left, right })
    }
}
*/
// ============================================================================
// Parameter bindings (for instantiation)
// ============================================================================

/// Parameter binding (binds actual argument to formal parameter)
#[derive(Debug, Clone)]
pub struct McParamBinding {
    /// Formal parameter declaration
    pub declare: McParamDeclare,

    /// Bound actual argument value
    pub value: Option<McParamValue>,

    /// Whether default value is used
    pub is_default: bool,
}

impl McParamBinding {
    pub fn new(declare: McParamDeclare, value: Option<McParamValue>) -> Self {
        let is_default = value.is_none();
        Self {
            declare,
            value,
            is_default,
        }
    }

    pub fn as_int_binding(&self) -> Option<(String, i64)> {
        let name = self.declare.get_primary_name()?;
        let value = self.value.as_ref()?;

        match value {
            McParamValue::Int(i) => Some((name, i.value)),
            _ => None,
        }
    }

    /// Get the actual value (prefer passed-in value, otherwise use default)
    pub fn get_value(&self) -> Option<&McParamValue> {
        self.value.as_ref()
    }

    /// Get the member value of the parameter binding
    ///
    /// STUB — NOT IMPLEMENTED: this function unconditionally returns `None`
    /// (the `_idx`/`_value` locals below are unused). The member-level formal
    /// parameter substitution it was meant to serve (subst.rs `dc24v.VCC` ->
    /// actual member `V1`) therefore does not run and member names stay as-is.
    /// Documented as design gap A in eval.md §11.5.
    ///
    /// Used for parameter declarations with members like `dc24v{VCC24, GND}`,
    /// to get the corresponding member at the position of the given member name in the bound value.
    ///
    /// # How it works
    /// 1. Get the member list from the formal parameter declaration and the index of `member_name`
    /// 2. Extract the value at the corresponding index from the actual argument value
    ///
    /// # Supported actual argument forms
    /// - `McOpd::WithMember { member: [...] }` -> get member by index
    /// - `McParamValue::Set([...])` -> get Set element by index
    ///
    /// # Example
    /// ```text
    /// // declaration: dc24v{VCC24, GND}
    /// // argument: my_dc[V1, G1]
    /// binding.get_member_value("VCC24") -> Some(Opd(Id("V1")))
    /// binding.get_member_value("GND")   -> Some(Opd(Id("G1")))
    /// ```
    pub fn get_member_value(&self, member_name: &str) -> Option<McParamValue> {
        // 1. Get the member list of the formal parameter declaration
        let declare_members = self.declare.expand();
        if declare_members.is_empty() {
            return None;
        }

        // 2. Find the position of member_name in the formal parameter member list
        let _idx = declare_members
            .iter()
            .position(|m: &String| m == member_name)?;

        // 3. Extract the value at the corresponding index from the actual argument value
        let _value = self.get_value()?;
        None
    }

    /// Get the list of expanded names for the parameter binding
    ///
    /// Combines the formal parameter name with its members, returning all expanded names.
    /// Used for name substitution when expanding function bodies.
    ///
    /// # Example
    /// ```text
    /// // declaration: dc24v{VCC24, GND}
    /// expand_names() -> ["dc24v.VCC24", "dc24v.GND"]
    /// // declaration: pwr (no members)
    /// expand_names() -> ["pwr"]
    /// ```
    pub fn expand_names(&self) -> Vec<String> {
        self.declare.expand()
    }
}

/// Parameter binding list
#[derive(Debug, Clone, Default)]
pub struct McParamBindings {
    bindings: Vec<McParamBinding>,
}

impl McParamBindings {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Create bindings from parameter declarations and parameter values.
    ///
    /// Binding priority (highest first):
    /// 1. Named binding: `.name=value` matched by formal parameter name.
    /// 2. Type-directed match: a typed argument (unit / enum class / interface
    ///    class) claims the uniquely matching formal slot.
    /// 3. Positional fallback: remaining untyped arguments fill remaining
    ///    unclaimed slots in written order.
    ///
    /// Strict arity: extra arguments and arguments that cannot be matched to
    /// any formal parameter are hard errors. Nothing is silently ignored.
    pub fn bind(
        declares: &McParamDeclares,
        values: &[McParamValue],
    ) -> Result<Self, ParamBindError> {
        Self::bind_inner(declares, values)
    }

    /// Component-construction binding. Kept as an alias of [`Self::bind`]:
    /// all parameters are now declared explicitly in the component signature,
    /// so extra or unmatched arguments are errors just like function/method
    /// calls. There is no longer a "silent extras" mode.
    pub fn bind_quiet(
        declares: &McParamDeclares,
        values: &[McParamValue],
    ) -> Result<Self, ParamBindError> {
        Self::bind_inner(declares, values)
    }

    fn bind_inner(
        declares: &McParamDeclares,
        values: &[McParamValue],
    ) -> Result<Self, ParamBindError> {
        // ── Separate named parameters (InlineAttrs) and positional parameters ──
        // Each attribute inside `{ cap = 1uF; volt = 50V }` becomes one named
        // argument `(formal_name, value)`; everything else is positional.
        let mut named_entries: Vec<(String, McParamValue)> = Vec::new();
        let mut positional_values: Vec<McParamValue> = Vec::new();

        for v in values.iter() {
            match v {
                McParamValue::InlineAttrs(attrs) => {
                    for attr in attrs {
                        let name = attr.id.to_string();
                        let value = attr
                            .values
                            .first()
                            .map(attr_val_to_param_value)
                            .unwrap_or_else(|| McParamValue::NONE(String::from("_")));
                        named_entries.push((name, value));
                    }
                }
                other if other.is_named_param() => {
                    // Other named forms (future): keep the whole value.
                    if let Some(name) = other.get_param_name() {
                        named_entries.push((name, other.clone()));
                    }
                }
                _ => positional_values.push(v.clone()),
            }
        }

        // ── Strip modifiers (NC, ') from positional values before arity ──
        // NC (Not Connected) is a system keyword that occupies NO positional
        // slot and provides no value: it is removed before arity checking and
        // never covers a missing required parameter (a strict call like
        // `DIO.ESD("ESD9B5V-2/TR", NC)` still reports the missing `rating`).
        // NC is meaningful only in a class construction (`CLASS(NC)`) or a
        // constructor argument list; callers outside those contexts reject
        // NC before reaching this function. ' (Transposed) is an instance
        // modifier handled by the caller.
        let effective_pos: Vec<McParamValue> = positional_values
            .iter()
            .filter(|v| !matches!(v, McParamValue::NC(_)))
            .cloned()
            .collect();
        let effective_count = effective_pos.len();

        // ── New arity rule ─────────────────────────────────────────────────
        // required: only params that have NO unit type AND NO default value.
        // Unit-typed params without a matching arg → bind as `_` (unspecified), not an error.
        let total = declares.iter().count();

        // Check for too many arguments (strict error, fast path). With named
        // args claiming slots the precise check runs again after Round 3.
        if effective_count > total {
            return Err(ParamBindError::TooManyArguments {
                expected: total,
                got: effective_count,
            });
        }

        // ── Iter-3.G removed: no multi-value regrouping heuristic. Extra
        // arguments are a hard error above; multi-value groups must be written
        // explicitly as `[..]` sets. ──
        let positional_values = effective_pos;

        // ── Three-round binding ────────────────────────────────────────────
        let mut bindings: Vec<Option<McParamBinding>> = vec![None; total];
        let mut slot_claimed: Vec<bool> = vec![false; total];
        let mut pos_claimed: Vec<bool> = vec![false; positional_values.len()];

        // ── Round 1: Named binding ─────────────────────────────────────────
        // Each named argument (`{ cap = 1uF; ... }`) claims the formal slot
        // whose name matches (case-insensitive). Orphan named arguments —
        // names that match no formal parameter — are a hard error.
        let mut named_claimed: Vec<bool> = vec![false; named_entries.len()];
        for (di, declare) in declares.iter().enumerate() {
            let Some(param_name) = declare.get_primary_name() else {
                continue;
            };
            for (ni, (name, value)) in named_entries.iter().enumerate() {
                if named_claimed[ni] {
                    continue;
                }
                if name.eq_ignore_ascii_case(&param_name) || declare.match_name(name) {
                    bindings[di] = Some(McParamBinding::new(declare.clone(), Some(value.clone())));
                    slot_claimed[di] = true;
                    named_claimed[ni] = true;
                    break;
                }
            }
        }
        // Orphan named arguments: no formal parameter has this name.
        for (ni, (name, _)) in named_entries.iter().enumerate() {
            if !named_claimed[ni] {
                return Err(ParamBindError::UnknownParameter { name: name.clone() });
            }
        }

        // Check for too few arguments (missing required), accounting for
        // required slots already claimed by named args in Round 1. NC does
        // not relax this check: a not-connected instance still reports
        // genuinely missing required parameters (E4176).
        let unclaimed_required: Vec<String> = declares
            .iter()
            .enumerate()
            .filter(|(di, d)| {
                !slot_claimed[*di]
                    && !d.has_unit_type()
                    && !d.has_enum_class()
                    && !d.has_default_value()
            })
            .filter_map(|(_, d)| d.get_primary_name())
            .collect();
        if effective_count < unclaimed_required.len() {
            return Err(ParamBindError::MissingRequired {
                name: unclaimed_required
                    .get(effective_count)
                    .cloned()
                    .unwrap_or_default(),
            });
        }

        // ── Round 2: Unit claiming ──────────────────────────────────────────
        // For each positional arg with a unit, try to claim a formal slot
        // whose declared unit matches the argument's unit.
        for (pi, pos_val) in positional_values.iter().enumerate() {
            if let McParamValue::UValue(uval) = pos_val {
                let arg_unit = uval.unit();
                let mut claimed = false;
                for (di, declare) in declares.iter().enumerate() {
                    if slot_claimed[di] {
                        continue;
                    }
                    if let Some(decl_unit) = declare.get_declared_unit() {
                        if decl_unit == arg_unit {
                            bindings[di] =
                                Some(McParamBinding::new(declare.clone(), Some(pos_val.clone())));
                            slot_claimed[di] = true;
                            pos_claimed[pi] = true;
                            claimed = true;
                            break;
                        }
                    }
                }
                if !claimed {
                    // A unit-typed argument cannot match any declared unit. This
                    // is a hard error: typed arguments never fall back to
                    // positional binding (which could silently bind a value to
                    // an unrelated parameter).
                    R05_UNRESOLVED_UNIT.fetch_add(1, Ordering::Relaxed);
                    return Err(ParamBindError::TypeMismatch {
                        param_name: uval.to_string(),
                        expected: format!("a parameter declared with unit {arg_unit:?}"),
                        got: "no matching unit declaration".to_string(),
                    });
                }
            }
        }

        // ── Round 2.5: Enum / interface class claiming ─────────────────────
        // Class-typed arguments — bare enum member `X7R` or dotted
        // `CAP.X7R` / `PKG.C0402` (which parse as `Opd(Id)`), or a dotted
        // interface member `DC.IVCC5` — claim the formal slot that declares
        // the same class. An enum-typed argument that cannot match any
        // enum-class slot is a hard error; an interface-typed argument that
        // matches no interface-class slot falls through to positional
        // fallback (it may be a plain dotted net reference).
        for (pi, pos_val) in positional_values.iter().enumerate() {
            if pos_claimed[pi] {
                continue;
            }
            let ids = match pos_val {
                McParamValue::Ids(ids) => Some(ids),
                McParamValue::Opd(McOpd::Id(ids)) => Some(ids),
                _ => None,
            };
            let Some(ids) = ids else {
                continue;
            };
            let parts = match ids.dot_chain_parts() {
                Some(p) if !p.is_empty() => p,
                _ => continue,
            };
            let arg_member = parts[parts.len() - 1].as_str();
            let dotted_class: Option<String> = if parts.len() > 1 {
                Some(parts[0].clone())
            } else {
                None
            };
            // Dotted args are only enum-typed when their class is a known
            // enum class (`CAP.X7R`); `uC.I2C0` style net references are not.
            let enum_class: Option<String> = match &dotted_class {
                Some(c) if crate::db::cmie::cmie::is_enum_class_name(c) => Some(c.clone()),
                None => crate::db::cmie::cmie::resolve_bare_enum_value(arg_member, None),
                _ => None,
            };

            if let Some(arg_class) = enum_class {
                let mut claimed = false;
                for (di, declare) in declares.iter().enumerate() {
                    if slot_claimed[di] {
                        continue;
                    }
                    if declare.get_enum_class() == Some(arg_class.as_str()) {
                        bindings[di] =
                            Some(McParamBinding::new(declare.clone(), Some(pos_val.clone())));
                        slot_claimed[di] = true;
                        pos_claimed[pi] = true;
                        claimed = true;
                        break;
                    }
                }
                if !claimed {
                    // An enum-typed argument that cannot match any declared
                    // enum-class slot is a hard error. This catches package
                    // values (`PKG.R0402`, `R0603`) passed to components that
                    // declare no enum-class parameter for them.
                    return Err(ParamBindError::TypeMismatch {
                        param_name: pos_val.to_string(),
                        expected: format!("a parameter declared with enum class {arg_class}"),
                        got: "no matching enum-class declaration".to_string(),
                    });
                }
                continue;
            }

            // Interface claiming: dotted arg whose first segment matches an
            // interface-class formal (or its first segment), e.g.
            // `DC.IVCC5` → a formal declared as `dc24v::DC(24V)`.
            if let Some(arg_class) = dotted_class {
                for (di, declare) in declares.iter().enumerate() {
                    if slot_claimed[di] {
                        continue;
                    }
                    let Some((formal_class, _)) = declare.interface_annotation() else {
                        continue;
                    };
                    let formal_first = formal_class.split('.').next().unwrap_or(&formal_class);
                    if formal_first == arg_class || formal_class == arg_class {
                        bindings[di] =
                            Some(McParamBinding::new(declare.clone(), Some(pos_val.clone())));
                        slot_claimed[di] = true;
                        pos_claimed[pi] = true;
                        break;
                    }
                }
            }
        }

        // ── Round 3: Positional fallback ────────────────────────────────────
        // Remaining unclaimed positional args (strings, enums, package names, etc.)
        // fill remaining unclaimed slots in order.
        let remaining_pos: Vec<(usize, &McParamValue)> = positional_values
            .iter()
            .enumerate()
            .filter(|(i, _)| !pos_claimed[*i])
            .collect();
        let mut rp_idx = 0;
        for (di, declare) in declares.iter().enumerate() {
            if slot_claimed[di] {
                continue;
            }
            if rp_idx < remaining_pos.len() {
                let (_pi, pos_val) = remaining_pos[rp_idx];
                bindings[di] = Some(McParamBinding::new(declare.clone(), Some(pos_val.clone())));
                slot_claimed[di] = true;
                rp_idx += 1;
            }
        }
        // Positional args that could not claim any slot (because named args
        // already occupied them) are too many — hard error.
        if rp_idx < remaining_pos.len() {
            let got = positional_values.len();
            let expected = total - named_claimed.iter().filter(|c| **c).count();
            return Err(ParamBindError::TooManyArguments { expected, got });
        }

        // ── Fill unclaimed slots ────────────────────────────────────────────
        // Unclaimed slots with unit type → bind as `_` (unspecified), no error.
        // Unclaimed slots with default → bind with default value.
        // Unclaimed slots without default → bind as None (represents `_`).
        let mut final_bindings = Vec::new();
        for (di, declare) in declares.iter().enumerate() {
            if let Some(binding) = bindings[di].take() {
                final_bindings.push(binding);
            } else {
                // Slot not claimed by any round
                if declare.has_default_value() {
                    final_bindings.push(McParamBinding::new(declare.clone(), None));
                } else {
                    // Unit-typed or non-required: bind as `_` (unspecified)
                    final_bindings.push(McParamBinding::new(declare.clone(), None));
                }
            }
        }

        // ── Enum value validation ───────────────────────────────────────────
        // Verify that enum-class parameter values are valid enum members.
        // Only plain-Ids values are checked: dotted / Opd-wrapped values in a
        // chain expression may legitimately be positional fallback arguments
        // (e.g. a package value `PKG.R0402`) and are not member-checked here.
        for binding in &final_bindings {
            if let McParamDeclareKind::EnumClass(ec) = &binding.declare.kind {
                let ids_primary: Option<String> = match &binding.value {
                    Some(McParamValue::Ids(ids)) => ids.get_primary_name(),
                    _ => None,
                };
                let val_name: Option<&str> = match &binding.value {
                    Some(McParamValue::Ids(_)) => ids_primary.as_deref(),
                    Some(_) => None,
                    None if binding.is_default => ec.default_val.as_deref(),
                    None => None,
                };
                if let Some(vn) = val_name {
                    if !ec.is_valid_value(vn) {
                        // An enum-class parameter bound to a value that is not a
                        // member of its enum is a hard error.
                        return Err(ParamBindError::TypeMismatch {
                            param_name: ec.name.to_string(),
                            expected: format!("a valid member of enum {}", ec.class_name),
                            got: vn.to_string(),
                        });
                    }
                }
            }
        }

        Ok(Self {
            bindings: final_bindings,
        })
    }

    /// Find binding by parameter name
    pub fn find(&self, name: &str) -> Option<&McParamBinding> {
        self.bindings.iter().find(|b| b.declare.match_name(name))
    }

    /// Convert bindings to (McIds, String) pairs for condition evaluation
    pub fn to_params_for_eval(&self) -> Vec<(McIds, String)> {
        self.bindings
            .iter()
            .filter_map(|b| {
                let name = b.declare.get_primary_name()?;
                let value = b.get_value().map(|v| format!("{v}")).unwrap_or_default();
                Some((McIds::from(name.as_str()), value))
            })
            .collect()
    }

    /// Get all bindings
    pub fn iter(&self) -> impl Iterator<Item = &McParamBinding> {
        self.bindings.iter()
    }

    /// Get the binding count
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Find binding by index
    pub fn find_by_index(&self, index: usize) -> Option<&McParamBinding> {
        self.bindings.get(index)
    }

    /// Find binding value by member name
    ///
    /// Iterates all bindings, searches the given member name in each binding that has members,
    /// and returns the first matching member value.
    ///
    /// Used to resolve member references like `dc24v.VCC24`.
    pub fn find_member_value(&self, member_name: &str) -> Option<McParamValue> {
        for binding in &self.bindings {
            if let Some(val) = binding.get_member_value(member_name) {
                return Some(val);
            }
        }
        None
    }

    /// ── P3: derive a sub-binding excluding the specified formal parameter names ──
    /// Used for submodule methods: boundary formal params (bound to parent scope references) do not
    /// participate in body substitution, preserving the formal param names as submodule boundary
    /// labels to be reconnected at the parent module boundary.
    pub(crate) fn subset_excluding(&self, exclude: &std::collections::HashSet<String>) -> Self {
        Self {
            bindings: self
                .bindings
                .iter()
                .filter(|b| {
                    b.declare
                        .get_primary_name()
                        .is_none_or(|n| !exclude.contains(&n))
                })
                .cloned()
                .collect(),
        }
    }
}

/// Parameter binding error
#[derive(Debug, Clone)]
pub enum ParamBindError {
    /// Too many arguments
    TooManyArguments { expected: usize, got: usize },

    /// Missing required parameter
    MissingRequired { name: String },

    /// Type mismatch
    TypeMismatch {
        param_name: String,
        expected: String,
        got: String,
    },

    /// Named argument whose name matches no formal parameter
    UnknownParameter { name: String },
}

impl std::fmt::Display for ParamBindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamBindError::TooManyArguments { expected, got } => {
                write!(f, "Too many arguments: expected {expected}, got {got}")
            }
            ParamBindError::MissingRequired { name } => {
                write!(f, "Missing required parameter: {name}")
            }
            ParamBindError::TypeMismatch {
                param_name,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Type mismatch for parameter '{param_name}': expected {expected}, got {got}"
                )
            }
            ParamBindError::UnknownParameter { name } => {
                write!(f, "Unknown parameter: no formal parameter named '{name}'")
            }
        }
    }
}

// ============================================================================
// Tests: NC modifier stripping doesn't affect arity
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};
    use crate::{McIda, McSpaceName};

    /// t1: X6.setup(GND, NC) → bind success, arity=1
    /// NC is a modifier, not a positional argument. It must be stripped
    /// before arity checking and NOT count toward call_count.
    #[test]
    fn test_nc_stripped_from_instance_method_arity() {
        use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};

        // Simulate func setup(GND) — 1 param, no default, no unit type
        let mut declares = McParamDeclares::new();
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("GND")),
            param_type: McParamType {
                kind: McParamTypeKind::Unknown,
                direction: None,
            },
        });

        // Call: setup(GND, NC)
        let values = vec![
            McParamValue::Ids(McIds::from("GND")),
            McParamValue::NC("NC".into()),
        ];

        // bind (not bind_quiet) — this is the instance method path
        let result = McParamBindings::bind(&declares, &values);
        assert!(
            result.is_ok(),
            "X6.setup(GND, NC) should succeed with NC stripped, got: {:?}",
            result.err()
        );
    }

    /// t2: dio[1:2]::DIO.ESD("ESD9B5V-2/TR", NC) → bind success
    /// Named array declaration path: Mc2Component::with_params sets nc=true
    /// when NC is in params, and instantiate_declarations_resilient uses
    /// with_nc (skipping binding). NC must not cause TooManyArguments.
    #[test]
    fn test_nc_named_array_does_not_trigger_too_many_args() {
        use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};

        // Simulate DIO.ESD(partno::STRING, rating::STRING) — 2 string params
        let mut declares = McParamDeclares::new();
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("partno")),
            param_type: McParamType {
                kind: McParamTypeKind::Unknown,
                direction: None,
            },
        });
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("rating")),
            param_type: McParamType {
                kind: McParamTypeKind::Unknown,
                direction: None,
            },
        });

        // Call: DIO.ESD("ESD9B5V-2/TR", NC)
        let values = vec![
            McParamValue::String(McString {
                value: "ESD9B5V-2/TR".to_string(),
            }),
            McParamValue::NC("NC".into()),
        ];

        // bind_quiet — component construction path (silent extras)
        let result = McParamBindings::bind_quiet(&declares, &values);
        // With NC stripped, effective_count=1 but new_required=2.
        // Named array path uses with_nc, so binding is skipped entirely.
        // The anonymous inline path now also uses with_nc when NC is present.
        // This test verifies that NC doesn't cause TooManyArguments.
        match result {
            Ok(_) => {} // with_nc path would succeed
            Err(ParamBindError::MissingRequired { .. }) => {
                // This is expected when going through bind_quiet directly
                // (without the with_nc shortcut). The real code path uses with_nc.
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    /// t3: DIO.ESD("ESD9B5V-2/TR", NC) anonymous inline → bind success
    /// Same as t2 — anonymous inline path now also checks for NC and uses
    /// with_nc, converging with the named array path.
    #[test]
    fn test_nc_anonymous_inline_same_as_named_array() {
        use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};

        // Same declares as t2
        let mut declares = McParamDeclares::new();
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("partno")),
            param_type: McParamType {
                kind: McParamTypeKind::Unknown,
                direction: None,
            },
        });
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("rating")),
            param_type: McParamType {
                kind: McParamTypeKind::Unknown,
                direction: None,
            },
        });

        let values = vec![
            McParamValue::String(McString {
                value: "ESD9B5V-2/TR".to_string(),
            }),
            McParamValue::NC("NC".into()),
        ];

        // bind_quiet — same path as t2
        let result = McParamBindings::bind_quiet(&declares, &values);
        match result {
            Ok(_) => {}
            Err(ParamBindError::MissingRequired { .. }) => {
                // Expected when going through bind_quiet directly
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    // ── p3: named-argument binding ────────────────────────────────────────

    /// Build a single-name formal parameter declaration.
    fn single_declare(name: &str) -> McParamDeclare {
        McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from(name)),
            param_type: McParamType::default(),
        }
    }

    /// Build a named argument attribute `name = <int>`.
    fn attr_int(name: &str, value: i64) -> McAttribute {
        McAttribute {
            no: 0,
            id: McIds::from(name),
            values: vec![McAttrVal::AttrLiteral(McLiteral::Int(McInt { value }))],
            key_span: None,
        }
    }

    /// Build a dotted-id chain `a.b` (as produced by AST `a.b` operand parsing).
    fn dotted(parts: &[&str]) -> McIds {
        use crate::semantic::basic::mc_ids::IdsSegment;
        McIds {
            segments: parts
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i == 0 {
                        IdsSegment::Ida(Box::new(McIda::from(*p)))
                    } else {
                        IdsSegment::DotIda(Box::new(McIda::from(*p)))
                    }
                })
                .collect(),
        }
    }

    /// t4: `{ cap = 10; volt = 25 }` binds by name, case-insensitively.
    #[test]
    fn test_named_args_bind_by_name_case_insensitive() {
        let mut declares = McParamDeclares::new();
        declares.push(single_declare("cap"));
        declares.push(single_declare("volt"));

        // Uppercase attribute key must still claim the `cap` slot.
        let values = vec![McParamValue::InlineAttrs(vec![
            attr_int("CAP", 10),
            attr_int("volt", 25),
        ])];

        let bindings =
            McParamBindings::bind(&declares, &values).expect("named args should bind by name");
        let cap = bindings.find("cap").expect("cap should be bound");
        assert!(matches!(cap.get_value(), Some(McParamValue::Int(i)) if i.value == 10));
        let volt = bindings.find("volt").expect("volt should be bound");
        assert!(matches!(volt.get_value(), Some(McParamValue::Int(i)) if i.value == 25));
    }

    /// t5: named args in reversed written order still claim the correct slots.
    #[test]
    fn test_named_args_out_of_order_bind_by_name() {
        let mut declares = McParamDeclares::new();
        declares.push(single_declare("cap"));
        declares.push(single_declare("volt"));

        let values = vec![McParamValue::InlineAttrs(vec![
            attr_int("volt", 25),
            attr_int("cap", 10),
        ])];

        let bindings = McParamBindings::bind(&declares, &values)
            .expect("reversed named args should still bind");
        let cap = bindings.find("cap").expect("cap should be bound");
        assert!(matches!(cap.get_value(), Some(McParamValue::Int(i)) if i.value == 10));
        let volt = bindings.find("volt").expect("volt should be bound");
        assert!(matches!(volt.get_value(), Some(McParamValue::Int(i)) if i.value == 25));
    }

    /// t6: a named argument whose name matches no formal parameter is a hard error.
    #[test]
    fn test_orphan_named_arg_is_error() {
        let mut declares = McParamDeclares::new();
        declares.push(single_declare("cap"));

        let values = vec![McParamValue::InlineAttrs(vec![attr_int("nope", 5)])];

        match McParamBindings::bind(&declares, &values) {
            Err(ParamBindError::UnknownParameter { name }) => assert_eq!(name, "nope"),
            other => panic!("expected UnknownParameter, got {:?}", other),
        }
    }

    /// t7: named args claim their slot; remaining positional args fill the rest.
    #[test]
    fn test_named_arg_plus_positional_fill_remaining() {
        let mut declares = McParamDeclares::new();
        declares.push(single_declare("cap"));
        declares.push(single_declare("volt"));

        // f({ volt = 25 }, 10) — volt by name, cap positionally.
        let values = vec![
            McParamValue::InlineAttrs(vec![attr_int("volt", 25)]),
            McParamValue::Int(McInt { value: 10 }),
        ];

        let bindings =
            McParamBindings::bind(&declares, &values).expect("named + positional should bind");
        let cap = bindings.find("cap").expect("cap should be bound");
        assert!(matches!(cap.get_value(), Some(McParamValue::Int(i)) if i.value == 10));
        let volt = bindings.find("volt").expect("volt should be bound");
        assert!(matches!(volt.get_value(), Some(McParamValue::Int(i)) if i.value == 25));
    }

    /// t8: named args occupying slots can push positional args over the limit.
    #[test]
    fn test_named_claim_causing_positional_overflow() {
        let mut declares = McParamDeclares::new();
        declares.push(single_declare("cap"));
        declares.push(single_declare("volt"));

        // f({ cap = 10 }, 25, 99) — cap by name; 25 → volt; 99 has no slot left.
        let values = vec![
            McParamValue::InlineAttrs(vec![attr_int("cap", 10)]),
            McParamValue::Int(McInt { value: 25 }),
            McParamValue::Int(McInt { value: 99 }),
        ];

        match McParamBindings::bind(&declares, &values) {
            Err(ParamBindError::TooManyArguments { .. }) => {}
            other => panic!("expected TooManyArguments, got {:?}", other),
        }
    }

    // ── p4: enum / interface class heuristic claiming ─────────────────────

    /// Register a small `CAP { X7R, C0G }` enum in the global table so
    /// enum-class claiming sees it (mirrors library loading).
    fn register_test_enum() {
        use crate::db::infra::global::mcc_enums;
        use crate::semantic::common::uri_intern;
        use crate::semantic::mc_enum::{McEnumDef, McEnumValue};
        use std::sync::Arc;

        let def = McEnumDef {
            name: McIds::from("CAP"),
            span: [0, 3],
            values: vec![
                McEnumValue {
                    name: McIds::from("X7R"),
                    span: [0, 3],
                },
                McEnumValue {
                    name: McIds::from("C0G"),
                    span: [0, 3],
                },
            ],
            uri: String::from("test.mc"),
        };
        mcc_enums.insert(
            McSpaceName {
                ident: McIds::from("CAP"),
                uri: uri_intern("test.mc"),
            },
            Arc::new(def),
        );
    }

    /// t9: a bare enum member `X7R` claims the enum-class slot `diel::CAP`.
    #[test]
    fn test_bare_enum_member_claims_enum_class_slot() {
        register_test_enum();
        let mut declares = McParamDeclares::new();
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::EnumClass(McEnumClassDeclare {
                name: McIds::from("diel"),
                class_name: String::from("CAP"),
                default_val: None,
            }),
            param_type: McParamType::default(),
        });

        let values = vec![McParamValue::Ids(McIds::from("X7R"))];
        let bindings = McParamBindings::bind(&declares, &values)
            .expect("bare enum member should claim the enum-class slot");
        let diel = bindings.find("diel").expect("diel should be bound");
        assert_eq!(diel.get_value().unwrap().to_string(), "X7R");
    }

    /// t10: a dotted `CAP.X7R` (Opd form) claims the enum-class slot.
    #[test]
    fn test_dotted_enum_member_opd_claims_enum_class_slot() {
        register_test_enum();
        let mut declares = McParamDeclares::new();
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::EnumClass(McEnumClassDeclare {
                name: McIds::from("diel"),
                class_name: String::from("CAP"),
                default_val: None,
            }),
            param_type: McParamType::default(),
        });

        let values = vec![McParamValue::Opd(McOpd::Id(dotted(&["CAP", "X7R"])))];
        let bindings = McParamBindings::bind(&declares, &values)
            .expect("dotted CAP.X7R should claim the enum-class slot");
        let diel = bindings.find("diel").expect("diel should be bound");
        assert_eq!(diel.get_value().unwrap().to_string(), "CAP.X7R");
    }

    /// t11: an enum member with no enum-class slot is a hard error.
    #[test]
    fn test_enum_member_without_enum_slot_is_error() {
        register_test_enum();
        let mut declares = McParamDeclares::new();
        declares.push(single_declare("volt"));

        // X7R is a known enum member but no formal declares an enum class.
        let values = vec![McParamValue::Ids(McIds::from("X7R"))];
        match McParamBindings::bind(&declares, &values) {
            Err(ParamBindError::TypeMismatch { .. }) => {}
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    /// t12: an enum member bound to the wrong enum class is a hard error.
    #[test]
    fn test_invalid_enum_member_value_is_error() {
        register_test_enum();
        let mut declares = McParamDeclares::new();
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::EnumClass(McEnumClassDeclare {
                name: McIds::from("diel"),
                class_name: String::from("CAP"),
                default_val: None,
            }),
            param_type: McParamType::default(),
        });

        // ZZZ is not a CAP member: claiming fails and validation rejects it.
        let values = vec![McParamValue::Ids(McIds::from("ZZZ"))];
        match McParamBindings::bind(&declares, &values) {
            Err(ParamBindError::TypeMismatch { got, .. }) => assert_eq!(got, "ZZZ"),
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    /// t13: a dotted interface member `DC.IVCC5` claims the interface-class slot.
    #[test]
    fn test_interface_member_claims_interface_slot() {
        let mut declares = McParamDeclares::new();
        declares.push(McParamDeclare {
            kind: McParamDeclareKind::Single(McIds::from("dc24v")),
            param_type: McParamType {
                kind: McParamTypeKind::Interface {
                    class_name: String::from("DC"),
                    params: vec![String::from("24V")],
                },
                direction: None,
            },
        });

        let values = vec![McParamValue::Opd(McOpd::Id(dotted(&["DC", "IVCC5"])))];
        let bindings = McParamBindings::bind(&declares, &values)
            .expect("DC.IVCC5 should claim the DC interface slot");
        let dc24v = bindings.find("dc24v").expect("dc24v should be bound");
        assert_eq!(dc24v.get_value().unwrap().to_string(), "DC.IVCC5");
    }

    /// t14: a dotted net reference (`uC.I2C0`) is not enum/interface typed and
    /// falls through to positional binding.
    #[test]
    fn test_dotted_net_ref_falls_through_to_positional() {
        let mut declares = McParamDeclares::new();
        declares.push(single_declare("bus"));

        let values = vec![McParamValue::Opd(McOpd::Id(dotted(&["uC", "I2C0"])))];
        let bindings =
            McParamBindings::bind(&declares, &values).expect("uC.I2C0 should bind positionally");
        let bus = bindings.find("bus").expect("bus should be bound");
        assert_eq!(bus.get_value().unwrap().to_string(), "uC.I2C0");
    }
}

impl std::error::Error for ParamBindError {}
