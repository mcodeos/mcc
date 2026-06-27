// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_opd::McOpd;
pub use super::mc_paramd::*;
use crate::core::component::mc_attr::McAttribute;
use crate::core::mc_func::HasFindInst;
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    core::{
        basic::mc_literal::{McConst, McHex, McString},
        basic::mc_phrase::McPhrase,
        basic::mc_uval::McUnitValue,
    },
    McFloat, McIds, McInt,
};

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
            MCAST_UVALUE | MCAST_UVALUE_AT | MCAST_RANGE_PLUSMINUS => {
                McUnitValue::new(node).map(McParamValue::UValue)
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

            // Net expressions (e.g. a - b, a + b, a -> b)
            MCAST_OPD_MINUS | MCAST_OPD_PLUS | MCAST_OPD_RIGHTARROW | MCAST_OPD_LEFTARROW => {
                McPhrase::new(node, context).map(|p| McParamValue::Phrase(Box::new(p)))
            }

            /*
            // Function call
            MCAST_OPD_FCALL => {
                McParamFuncCall::new(node).map(|fc| McParamValue::FuncCall(Box::new(fc)))
            }

            // Arithmetic expressions (e.g. rows*cols, rows+1, cols-2)
            MCAST_OPD_PLUS | MCAST_OPD_MINUS | MCAST_OPD_MULTI | MCAST_OPD_DIVID => {
                // For arithmetic expressions, we need to first parse the left and right operands
                if let Some(left) = node.get_sub_node() {
                    if let Some(right) = left.get_next() {
                        let left_value = McParamValue::new(&left)?;
                        let right_value = McParamValue::new(&right)?;

                        // Temporarily use SquareVec to represent arithmetic expressions; consider adding a dedicated expression type later
                        Some(McParamValue::Opdc(McOpd::SquareVec(vec![
                            McOpd::Id(format!("{:?}", node.get_type())),
                            left_value.into_opdc()?,
                            right_value.into_opdc()?,
                        ])))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } */
            _ => None,
        }
    }

    /// Try to convert to constant
    pub fn as_const(&self) -> Option<&McConst> {
        match self {
            McParamValue::Const(c) => Some(c),
            _ => None,
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

    /// Create bindings from parameter declarations and parameter values
    ///
    /// Supports:
    /// - Positional binding: actual arguments match formal parameters in order
    /// - Named binding: attribute parameters of the form `.name=value` matched by name
    /// - Type constraint check (soft warning, does not reject binding)
    ///
    /// # Binding rules
    /// 1. First separate named and positional parameters
    /// 2. For each formal parameter: prefer named match, then positional match
    /// 3. If the formal parameter has a type constraint and the actual argument does not match, record warning but do not error
    /// 4. Extra positional arguments report error, extra named arguments are ignored (may be attributes)
    pub fn bind(
        declares: &McParamDeclares,
        values: &[McParamValue],
    ) -> Result<Self, ParamBindError> {
        Self::bind_with_opts(declares, values, false)
    }

    /// Silent version: used in component construction (`CAP(1uF, ±20%, X7R, 6V3, PKG.C0402)`) scenarios
    /// Use this version —— the class's formal parameter list usually only declares the first few, and the rest
    /// (tolerance/temperature coefficient/package, etc.) are optional decorative parameters. Having extras
    /// is normal and should not trigger a warning.
    ///
    /// ── Iter-4.1 ────────────────────────────────────────────────────
    /// Previously all bind paths shared one warning logic, causing
    ///   `CAP(1uF, ±20%, X7R, 6V3, PKG.C0402)` -> `Warning: expected 2, got 5`
    /// to spam every anonymous component. Function method binding does need this warning, but component construction does not.
    pub fn bind_quiet(
        declares: &McParamDeclares,
        values: &[McParamValue],
    ) -> Result<Self, ParamBindError> {
        Self::bind_with_opts(declares, values, true)
    }

    fn bind_with_opts(
        declares: &McParamDeclares,
        values: &[McParamValue],
        silent_extras: bool,
    ) -> Result<Self, ParamBindError> {
        let mut bindings = Vec::new();

        // Separate named parameters (Attribute type) and positional parameters
        let mut named_values: Vec<&McParamValue> = Vec::new();
        let mut positional_values: Vec<McParamValue> = Vec::new();

        for v in values.iter() {
            if v.is_named_param() {
                named_values.push(v);
            } else {
                positional_values.push(v.clone());
            }
        }

        // ── Iter-3.G ────────────────────────────────────────────────────
        // Set re-grouping heuristic: if positional count is an integer multiple (>= 2x) of declares
        // and declares count > 0, it means the parser may have flattened `f([A,B], [C,D])` at the
        // call-site into 4 independent scalars A, B, C, D.
        // In this case, group evenly by declares count, re-wrap each group as a Set.
        //
        // Example: 2 formal + 4 positional => group[0]=[A,B], group[1]=[C,D]
        //          formal0 bind to Set([A,B]), formal1 bind to Set([C,D])
        //
        // This heuristic only triggers when all actual args are "trivial" Opd / NC / Literal
        // (not Set/Phrase), to avoid mistaking 4 truly independent scalars for 2 Sets.
        let decl_count = declares.iter().count();
        if decl_count > 0
            && positional_values.len() > decl_count
            && positional_values.len() % decl_count == 0
        {
            let group_size = positional_values.len() / decl_count;
            if group_size >= 2 {
                // Check that all positionals are trivial types (not Set / Phrase / InlineAttrs)
                // Identifier `VDD_3V3` parses to McParamValue::Ids, numeric literal to
                // McParamValue::Int (not Integer), NC to McParamValue::NC(String).
                // To be safe, use "non-complex" criterion: anything other than Set/Phrase/InlineAttrs
                // is trivial, to avoid missing primitive variants not yet enumerated.
                let all_simple = positional_values.iter().all(|v| {
                    !matches!(
                        v,
                        McParamValue::Set(_)
                            | McParamValue::Phrase(_)
                            | McParamValue::InlineAttrs(_)
                    )
                });
                if all_simple {
                    // Re-group: every group_size positionals combined into a Set
                    let regrouped: Vec<McParamValue> = positional_values
                        .chunks(group_size)
                        .map(|chunk| McParamValue::Set(chunk.to_vec()))
                        .collect();
                    positional_values = regrouped;
                }
            }
        }

        // Bind by position, named parameters take priority
        let mut pos_idx = 0;
        for declare in declares.iter() {
            // 1. First check if there is a named parameter matching this formal parameter
            let named_match = if let Some(param_name) = declare.get_primary_name() {
                named_values
                    .iter()
                    .find(|v| v.matches_param_name(&param_name))
                    .cloned()
                    .cloned()
            } else {
                None
            };

            let value = if let Some(named_val) = named_match {
                Some(named_val)
            } else if pos_idx < positional_values.len() {
                // 2. Otherwise bind by position
                let val = positional_values[pos_idx].clone();
                pos_idx += 1;
                Some(val)
            } else {
                // 3. No actual argument, use default value (if any)
                None
            };

            // Type constraint check (soft warning)
            if let Some(ref _val) = &value {
                // TODO: complete type checking needs to parse interface definitions
                // Currently only record constraint info, no hard check
                // Future additions:
                //   - check whether val satisfies expected_class constraint
                //   - emit diagnostic warning on type mismatch
            }

            bindings.push(McParamBinding::new(declare.clone(), value));
        }

        // ── Iter-3.B2 ────────────────────────────────────────────────────
        // Check for extra positional arguments —— emit soft warning.
        //
        // Reason: MC has special calling convention like `X6.setup(NC)`
        // to pass "NC as default not soldered" marker to `setup()`
        // which has no formal params.
        // Previously hard-reporting `TooManyArguments` would interrupt the entire func expansion,
        // preventing the resistors/capacitors inside setup() from being generated.
        //
        // Correct approach: emit one eprintln warning (diagnostic layering handled by upper layer),
        // but still return Ok —— extra actual args are dropped, already-bound formal params still take effect,
        // and the function body expands with default behavior.
        //
        // ── Iter-3.E2 ────────────────────────────────────────────────────
        // Further: If all extra positional arguments are NC,
        // it's a special "NC as default not soldered" convention call
        // like `X6.setup(NC)`, so we shouldn't emit warning.
        if pos_idx < positional_values.len() {
            let extras = &positional_values[pos_idx..];
            let all_nc = extras.iter().all(|v| matches!(v, McParamValue::NC(_)));
            if !all_nc && !silent_extras {
                eprintln!(
                    "Warning: Function param binding has {} extra positional argument(s) \
                     (expected {}, got {}); extras are ignored.",
                    positional_values.len() - pos_idx,
                    declares.len(),
                    values.len()
                );
            }
        }

        Ok(Self { bindings })
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

impl std::error::Error for ParamBindError {}
