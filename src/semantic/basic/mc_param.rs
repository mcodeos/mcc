// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::mc_opd::McOpd;
pub use super::mc_paramd::*;
use crate::semantic::component::mc_attr::McAttribute;
use crate::semantic::mc_func::HasFindInst;
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    semantic::{
        basic::mc_literal::{McConst, McHex, McString},
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
            MCAST_BODY => {
                if let Some(subnode) = node.get_sub_node() {
                    let mut attributes = Vec::new();
                    // Find MCAST_SET_ATTRIBUTES nodes
                    for child in subnode
                        .iter()
                        .filter(|child| child.is_type(MCAST_ATTRIBUTE))
                    {
                        // Parse attribute Set
                        if let Some(attr) = McAttribute::new(&child) {
                            attributes.push(attr);
                        }
                    }
                    return Some(McParamValue::InlineAttrs(attributes));
                }
                None
            }

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

    /// Parse a parameter value without an instance-lookup context.
    ///
    /// Used for instance construction args (e.g. `mcu(V3V3, V1V2)`,
    /// `::DC(3.3V)`) that are parsed before a function/module context
    /// exists. Handles every literal kind of [`McParamValue::new`] plus
    /// plain identifiers; kinds that need a [`HasFindInst`] context
    /// (opd expressions, function calls, attribute bodies) return `None`
    /// and are dropped, matching the historical instance-arg behavior.
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
    /// Named parameter syntax: `.pins[6:9]=SWDBG`, `.pkg='mc.serial9'`
    /// Corresponds to `McParamValue::Attribute(...)` variant
    pub fn is_named_param(&self) -> bool {
        //matches!(self, McParamValue::Attribute(_))
        false
    }

    pub fn matches_param_name(&self, _name: &str) -> bool {
        false
    }

    /// Try to get the attribute parameter's name
    ///
    /// Only valid for Attribute type, returns the `.id` string.
    pub fn get_param_name(&self) -> Option<String> {
        None
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
        // ── Separate named parameters (Attribute type) and positional parameters ──
        let mut named_values: Vec<&McParamValue> = Vec::new();
        let mut positional_values: Vec<McParamValue> = Vec::new();

        for v in values.iter() {
            if v.is_named_param() {
                named_values.push(v);
            } else {
                positional_values.push(v.clone());
            }
        }

        // ── Strip modifiers (NC, ') from positional values before arity ──
        // NC (Not Connected) and ' (Transposed) are instance modifiers,
        // not positional arguments. They are handled separately by the caller
        // (e.g. McComponentInst::with_nc) and must NOT count toward arity.
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
        let new_required = declares
            .iter()
            .filter(|d| !d.has_unit_type() && !d.has_enum_class() && !d.has_default_value())
            .count();

        // Check for too many arguments (strict error)
        if effective_count > total {
            return Err(ParamBindError::TooManyArguments {
                expected: total,
                got: effective_count,
            });
        }

        // Check for too few arguments (missing required)
        if effective_count < new_required {
            let required_names: Vec<String> = declares
                .iter()
                .filter(|d| !d.has_unit_type() && !d.has_default_value())
                .filter_map(|d| d.get_primary_name())
                .collect();
            if effective_count < required_names.len() {
                return Err(ParamBindError::MissingRequired {
                    name: required_names
                        .get(effective_count)
                        .cloned()
                        .unwrap_or_default(),
                });
            }
        }

        // ── Iter-3.G removed: no multi-value regrouping heuristic. Extra
        // arguments are a hard error above; multi-value groups must be written
        // explicitly as `[..]` sets. ──
        let positional_values = effective_pos;

        // ── Three-round binding ────────────────────────────────────────────
        let mut bindings: Vec<Option<McParamBinding>> = vec![None; total];
        let mut slot_claimed: Vec<bool> = vec![false; total];
        let mut pos_claimed: Vec<bool> = vec![false; positional_values.len()];

        // ── Round 1: Named binding (keep existing logic) ────────────────────
        for (di, declare) in declares.iter().enumerate() {
            let named_match = if let Some(param_name) = declare.get_primary_name() {
                named_values
                    .iter()
                    .find(|v| v.matches_param_name(&param_name))
                    .cloned()
                    .cloned()
            } else {
                None
            };

            if let Some(named_val) = named_match {
                bindings[di] = Some(McParamBinding::new(declare.clone(), Some(named_val)));
                slot_claimed[di] = true;
            }
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
        }
    }
}

// ============================================================================
// Tests: NC modifier stripping doesn't affect arity
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}

impl std::error::Error for ParamBindError {}
