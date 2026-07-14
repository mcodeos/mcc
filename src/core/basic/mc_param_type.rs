// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Smart Parameter Type System
//!
//! Classifies every parameter declaration into one of:
//!   Category A: Label / Bus / Interface (ports — connection points)
//!   Category B: Numeric Values (physical unit or bare numeric)
//!   Category C: Basic Data Types (STRING, INT, HEX, FLOAT, ENUM)
//!   Keyword: role (container expansion, independent of A/B/C)
//!
//! Direction (in/out/io/ps/anl/nc) is an orthogonal modifier stored as
//! `direction: Option<McIoTy>`, not a separate variant.

use super::mc_uval::McUnit;
use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::McIds;

// ============================================================================
// Parameter Type Classification
// ============================================================================

/// The semantic type category of a parameter.
///
/// This is a struct (not a plain enum) so that `direction` is always available
/// as an orthogonal modifier for Category A parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McParamType {
    pub kind: McParamTypeKind,
    /// Direction modifier — only meaningful for Category A (ports).
    /// `None` for Category B, C, and Keyword parameters.
    pub direction: Option<McIoTy>,
}

impl Default for McParamType {
    fn default() -> Self {
        Self {
            kind: McParamTypeKind::Unknown,
            direction: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McParamTypeKind {
    // ── Category A: Label / Bus / Interface (ports) ──
    // 未标注 (Unannotated)
    /// A1: bare identifier used as a label/port
    Label,
    /// A2: indexed identifier — {curly named} or [square range], unified
    Idx,
    // 明显标注 (Explicitly Annotated)
    /// A3: interface-typed — `id::ClassName(params)`
    Interface { class_name: String },
    /// A4: interface-typed with role/enum value — `id::ClassName(Role)`
    InterfaceWithRole {
        class_name: String,
        role_val: String,
    },
    /// A5: component-instance typed with inline attributes
    ComponentInstance { class_name: String },

    // ── Category B: Numeric Values ──
    // 明显标注 (Explicitly Annotated — has physical unit type)
    /// B1: physical unit typed — `id::UV.VOLT`, `id::UV.CAP`, ...
    UnitValue { unit: McUnit },
    /// B2: physical unit typed with default — `id::UV.TEMP = 25°C`
    UnitValueDefault {
        unit: McUnit,
        default_val: Option<String>,
    },
    // 未标注 (Unannotated)
    /// B3: bare identifier used as a numeric value (inferred from usage)
    BareNumeric,

    // ── Category C: Basic Data Types (scalar primitives) ──
    // 明显标注 (Explicitly Annotated)
    /// C1: `id::STRING`
    BasicString { default_val: Option<String> },
    /// C2: `id::INT`
    BasicInt { default_val: Option<String> },
    /// C3: `id::HEX`
    BasicHex { default_val: Option<String> },
    /// C4: `id::FLOAT`
    BasicFloat { default_val: Option<String> },
    // 未标注 (Unannotated)
    /// C5: enum/role value — positional arg to interface constructor
    EnumValue,

    // ── Keyword ──
    /// `role` keyword — container expansion, independent of A/B/C
    Role,

    /// Type not yet determined (needs usage-based inference)
    Unknown,
}

// ============================================================================
// IO Direction Modifier
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McIoTy {
    Input,
    Output,
    InOut,
    PowerSupply,
    Analog,
    NotConnected,
}

impl McIoTy {
    /// Parse direction from MCAST_IOTYPE node
    pub fn from_ast(node: &AstNode) -> Option<Self> {
        let sub = node.get_sub_node()?;
        match sub.get_type() {
            MCAST_IOTYPE_IN => Some(Self::Input),
            MCAST_IOTYPE_OUT => Some(Self::Output),
            MCAST_IOTYPE_IO => Some(Self::InOut),
            MCAST_IOTYPE_PS => Some(Self::PowerSupply),
            MCAST_IOTYPE_ANL => Some(Self::Analog),
            MCAST_IOTYPE_NC => Some(Self::NotConnected),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Input => "in",
            Self::Output => "out",
            Self::InOut => "io",
            Self::PowerSupply => "ps",
            Self::Analog => "anl",
            Self::NotConnected => "nc",
        }
    }
}

// ============================================================================
// McParamType methods
// ============================================================================

impl McParamType {
    /// Syntactic classification from AST node — called during parse.
    ///
    /// This only handles 明显标注 (explicitly annotated) forms where the type
    /// is directly visible in the syntax. Bare identifiers return `Unknown`
    /// and need usage-based inference later.
    pub fn from_ast(node: &AstNode) -> Self {
        let subnode = if node.get_type() == MCAST_PARAM {
            match node.get_sub_node() {
                Some(s) => s,
                None => return Self::unknown(),
            }
        } else {
            node.clone()
        };

        match subnode.get_type() {
            MCAST_ROLE => Self {
                kind: McParamTypeKind::Role,
                direction: None,
            },

            // Bare identifiers → Unknown (will be refined by usage analysis)
            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                if let Some(ids) = McIds::new(&subnode) {
                    if ids.is_bus() || ids.is_list() || ids.is_square_only() {
                        // IDX form — A2
                        Self {
                            kind: McParamTypeKind::Idx,
                            direction: None,
                        }
                    } else {
                        // Bare — could be A1 (Label) or B3 (BareNumeric)
                        Self::unknown()
                    }
                } else {
                    Self::unknown()
                }
            }

            MCAST_SQUARE_VEC => Self {
                kind: McParamTypeKind::Idx,
                direction: None,
            },

            MCAST_DECLARE_UV => {
                if let Some(sub) = subnode.get_sub_node() {
                    // MCAST_CLASS → unit type
                    let class_node = sub; // first child is MCAST_CLASS
                    if class_node.get_type() == MCAST_CLASS {
                        if let Some(unit_node) = class_node.get_sub_node() {
                            if let Some(unit) = McUnit::from_ast(&unit_node) {
                                // Try to extract default value from MCAST_INSTANCE sibling
                                let default_val = Self::extract_default_from_declare_uv(&subnode);
                                return Self::classify_unit_type(&unit, default_val);
                            }
                        }
                    }
                }
                Self::unknown()
            }

            MCAST_DECLARE => {
                // Interface-typed: extract class name
                Self::classify_declare(&subnode)
            }

            _ => Self::unknown(),
        }
    }

    /// Classify a unit-typed parameter (MCAST_DECLARE_UV)
    fn classify_unit_type(unit: &McUnit, default_val: Option<String>) -> Self {
        match unit {
            // Physical units → Category B
            McUnit::Volt
            | McUnit::Amp
            | McUnit::Cap
            | McUnit::Ind
            | McUnit::Time
            | McUnit::Len
            | McUnit::Wat
            | McUnit::Ohm
            | McUnit::Temp
            | McUnit::Hz
            | McUnit::Db
            | McUnit::Ppm
            | McUnit::Percent
            | McUnit::Baud
            | McUnit::DataSize
            | McUnit::Sps
            | McUnit::Siemens
            | McUnit::Responsivity
            | McUnit::Angle
            | McUnit::AngularRate
            | McUnit::Energy
            | McUnit::Efield
            | McUnit::Hfield
            | McUnit::Flux
            | McUnit::Bfield
            | McUnit::Slew
            | McUnit::Noise => {
                if default_val.is_some() {
                    Self {
                        kind: McParamTypeKind::UnitValueDefault {
                            unit: unit.clone(),
                            default_val,
                        },
                        direction: None,
                    }
                } else {
                    Self {
                        kind: McParamTypeKind::UnitValue { unit: unit.clone() },
                        direction: None,
                    }
                }
            }
            // Scalar primitives → Category C
            McUnit::Int => Self {
                kind: McParamTypeKind::BasicInt { default_val },
                direction: None,
            },
            McUnit::Hex => Self {
                kind: McParamTypeKind::BasicHex { default_val },
                direction: None,
            },
            McUnit::Float => Self {
                kind: McParamTypeKind::BasicFloat { default_val },
                direction: None,
            },
            McUnit::String => Self {
                kind: McParamTypeKind::BasicString { default_val },
                direction: None,
            },
        }
    }

    /// Classify a MCAST_DECLARE node (interface-typed)
    fn classify_declare(node: &AstNode) -> Self {
        let mut class_name = String::new();
        let mut has_role_arg = false;
        let mut role_val = String::new();
        let mut has_inline_attrs = false;

        if let Some(first_child) = node.get_sub_node() {
            for child in first_child.iter() {
                match child.get_type() {
                    MCAST_CLASS => {
                        if let Some(name_node) = child.get_sub_node() {
                            if let Some(ids) = McIds::new(&name_node) {
                                class_name = ids.to_string();
                            }
                        }
                        // Check for role/enum args in class params
                        if let Some(params_node) = child.get_next() {
                            if params_node.get_type() == MCAST_PARAMS {
                                if let Some(param_list) = params_node.get_sub_node() {
                                    for param in param_list.iter() {
                                        if let Some(ids) = McIds::new(&param) {
                                            has_role_arg = true;
                                            role_val = ids.to_string();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    MCAST_BODY => {
                        has_inline_attrs = true;
                    }
                    _ => {}
                }
            }
        }

        if class_name.is_empty() {
            return Self::unknown();
        }

        if has_inline_attrs {
            Self {
                kind: McParamTypeKind::ComponentInstance { class_name },
                direction: None,
            }
        } else if has_role_arg {
            Self {
                kind: McParamTypeKind::InterfaceWithRole {
                    class_name,
                    role_val,
                },
                direction: None,
            }
        } else {
            Self {
                kind: McParamTypeKind::Interface { class_name },
                direction: None,
            }
        }
    }

    /// Extract default value string from MCAST_DECLARE_UV's MCAST_INSTANCE child
    fn extract_default_from_declare_uv(node: &AstNode) -> Option<String> {
        if let Some(sub) = node.get_sub_node() {
            for child in sub.iter() {
                if child.get_type() == MCAST_INSTANCE {
                    // Check for `= literal` after the instance name
                    let mut current = child.get_sub_node();
                    while let Some(c) = current {
                        if c.get_type() == MCAST_UVALUE
                            || c.get_type() == MCAST_STRING
                            || c.get_type() == MCAST_INT
                            || c.get_type() == MCAST_HEX
                            || c.get_type() == MCAST_FLOAT
                        {
                            return Some(format!("{:?}", c));
                        }
                        current = c.get_next();
                    }
                }
            }
        }
        None
    }

    // ── Port classification ──

    /// Only Category A parameters are ports (connection points in the net graph).
    /// Category B (numeric), C (data), and Keyword (role) are NOT ports.
    pub fn is_port(&self) -> bool {
        matches!(
            self.kind,
            McParamTypeKind::Label
                | McParamTypeKind::Idx
                | McParamTypeKind::Interface { .. }
                | McParamTypeKind::InterfaceWithRole { .. }
                | McParamTypeKind::ComponentInstance { .. }
        )
    }

    /// Whether this parameter has an explicit type annotation (`::TYPE`).
    /// Bare identifiers (A1, B3, Unknown) return false.
    pub fn is_explicitly_typed(&self) -> bool {
        !matches!(
            self.kind,
            McParamTypeKind::Label
                | McParamTypeKind::Idx
                | McParamTypeKind::BareNumeric
                | McParamTypeKind::EnumValue
                | McParamTypeKind::Unknown
        )
    }

    /// Category name for diagnostics/display
    pub fn category_name(&self) -> &'static str {
        match self.kind {
            McParamTypeKind::Label => "A1-Label",
            McParamTypeKind::Idx => "A2-IDX",
            McParamTypeKind::Interface { .. } => "A3-Interface",
            McParamTypeKind::InterfaceWithRole { .. } => "A4-Interface+Role",
            McParamTypeKind::ComponentInstance { .. } => "A5-Component+Attrs",
            McParamTypeKind::UnitValue { .. } => "B1-UnitValue",
            McParamTypeKind::UnitValueDefault { .. } => "B2-UnitValue+Default",
            McParamTypeKind::BareNumeric => "B3-BareNumeric",
            McParamTypeKind::BasicString { .. } => "C1-String",
            McParamTypeKind::BasicInt { .. } => "C2-Int",
            McParamTypeKind::BasicHex { .. } => "C3-Hex",
            McParamTypeKind::BasicFloat { .. } => "C4-Float",
            McParamTypeKind::EnumValue => "C5-Enum",
            McParamTypeKind::Role => "Keyword:role",
            McParamTypeKind::Unknown => "Unknown",
        }
    }

    /// Returns the default value string if this type carries one
    pub fn default_value(&self) -> Option<&str> {
        match &self.kind {
            McParamTypeKind::UnitValueDefault { default_val, .. } => default_val.as_deref(),
            McParamTypeKind::BasicString { default_val } => default_val.as_deref(),
            McParamTypeKind::BasicInt { default_val } => default_val.as_deref(),
            McParamTypeKind::BasicHex { default_val } => default_val.as_deref(),
            McParamTypeKind::BasicFloat { default_val } => default_val.as_deref(),
            _ => None,
        }
    }

    /// Whether this parameter has a default value (making it optional at call sites)
    pub fn has_default(&self) -> bool {
        self.default_value().is_some()
    }

    pub fn unknown() -> Self {
        Self::default()
    }
}

impl std::fmt::Display for McParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(dir) = &self.direction {
            write!(f, "{} ", dir.as_str())?;
        }
        write!(f, "{}", self.category_name())
    }
}

// ============================================================================
// Arity: required vs optional parameter counts
// ============================================================================

/// Tracks how many parameters are required vs optional (have defaults).
#[derive(Debug, Clone, Default)]
pub struct McParamArity {
    pub total: usize,
    pub required: usize,
    pub optional: usize,
}

impl McParamArity {
    pub fn from_declares(declares: &[super::mc_paramd::McParamDeclare]) -> Self {
        let total = declares.len();
        let optional = declares.iter().filter(|d| d.has_default_value()).count();
        let required = total - optional;
        Self {
            total,
            required,
            optional,
        }
    }

    /// Validate call-site argument count against this arity
    pub fn validate(&self, call_arg_count: usize) -> Result<(), ArityError> {
        if call_arg_count > self.total {
            Err(ArityError::TooManyArguments {
                max: self.total,
                got: call_arg_count,
            })
        } else if call_arg_count < self.required {
            Err(ArityError::TooFewArguments {
                min: self.required,
                got: call_arg_count,
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub enum ArityError {
    TooManyArguments { max: usize, got: usize },
    TooFewArguments { min: usize, got: usize },
}

impl std::fmt::Display for ArityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyArguments { max, got } => {
                write!(f, "too many arguments: max {max}, got {got}")
            }
            Self::TooFewArguments { min, got } => {
                write!(f, "too few arguments: min {min} required, got {got}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_classification() {
        // Category A → ports
        assert!(McParamType {
            kind: McParamTypeKind::Label,
            direction: None
        }
        .is_port());
        assert!(McParamType {
            kind: McParamTypeKind::Idx,
            direction: None
        }
        .is_port());
        assert!(McParamType {
            kind: McParamTypeKind::Interface {
                class_name: "DC".into()
            },
            direction: None
        }
        .is_port());

        // Category B → NOT ports
        assert!(!McParamType {
            kind: McParamTypeKind::UnitValue { unit: McUnit::Volt },
            direction: None
        }
        .is_port());
        assert!(!McParamType {
            kind: McParamTypeKind::BareNumeric,
            direction: None
        }
        .is_port());

        // Category C → NOT ports
        assert!(!McParamType {
            kind: McParamTypeKind::BasicString { default_val: None },
            direction: None
        }
        .is_port());
        assert!(!McParamType {
            kind: McParamTypeKind::BasicInt { default_val: None },
            direction: None
        }
        .is_port());

        // Keyword → NOT port
        assert!(!McParamType {
            kind: McParamTypeKind::Role,
            direction: None
        }
        .is_port());
        // Unknown → NOT port
        assert!(!McParamType {
            kind: McParamTypeKind::Unknown,
            direction: None
        }
        .is_port());
    }

    #[test]
    fn test_has_default() {
        assert!(McParamType {
            kind: McParamTypeKind::BasicInt {
                default_val: Some("5".into())
            },
            direction: None
        }
        .has_default());
        assert!(!McParamType {
            kind: McParamTypeKind::BasicInt { default_val: None },
            direction: None
        }
        .has_default());
        assert!(McParamType {
            kind: McParamTypeKind::UnitValueDefault {
                unit: McUnit::Volt,
                default_val: Some("5V".into()),
            },
            direction: None
        }
        .has_default());
    }

    #[test]
    fn test_arity_validation() {
        let arity = McParamArity {
            total: 3,
            required: 2,
            optional: 1,
        };

        // Valid: 2 required passed
        assert!(arity.validate(2).is_ok());
        // Valid: all 3 passed
        assert!(arity.validate(3).is_ok());
        // Too many
        assert!(arity.validate(4).is_err());
        // Too few
        assert!(arity.validate(1).is_err());
    }
}
