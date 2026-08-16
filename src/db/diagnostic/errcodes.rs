// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Error code catalog (M6) — SINGLE SOURCE OF TRUTH.
//!
//! Every diagnostic code emitted by mcc must be declared here, with a symbolic
//! constant, a name, a description, and a message template. The message
//! template is the canonical emission text (with `{0}`, `{1}`… placeholders);
//! emission points render it via [`format_msg()`]. Generated from
//! `scripts/error-code-mapping.json` (see `scripts/gen-errcodes.py`); do not
//! edit by hand — regenerate instead.
//!
//! Numbering follows `mcd/doc/mcc-error-code-unification-plan.md` §3.2:
//! thousands+hundreds = pipeline stage / semantic cluster.
//!   - 1xxx  Pass1a  type collection / definition structure
//!   - 2xxx  Pass1b  use statements / parser / name resolution
//!   - 3xxx  Pass1c  component/module/params/instances
//!   - 4xxx  Pass2   connection / netlist / interface binding
//!   - 5xxx  Pass3   validation checks
//!   - 6xxx  ERC
//!   - 9xxx  reserved
//!
//! ## Adding a new code
//!
//! 1. Add a `pub const` in the appropriate section below.
//! 2. Add a match arm / entry in [`describe()`] / `ALL_CODES`.
//! 3. Regenerate via `python3 scripts/gen-errcodes.py` to keep this file in sync.

// ============================================================================
// Infrastructure
// ============================================================================

/// A human-readable error code entry.
#[derive(Clone)]
pub struct ErrorCodeInfo {
    pub code: u32,
    pub name: &'static str,
    pub description: &'static str,
    /// Canonical emission message template (`{0}`, `{1}`, … placeholders).
    pub message: &'static str,
}

/// All registered error codes (used by `mcc explain` without arguments).
pub fn all_codes() -> &'static [ErrorCodeInfo] {
    &ALL_CODES
}

/// Look up a single error code. Returns `None` if unknown.
pub fn describe(code: u32) -> Option<ErrorCodeInfo> {
    ALL_CODES.iter().find(|e| e.code == code).cloned()
}

/// Render the canonical emission message for `code` by substituting `{i}`
/// placeholders with `args[i]`. Placeholders without a matching argument are
/// left verbatim; unknown codes render an empty string.
pub fn format_msg(code: u32, args: &[&dyn std::fmt::Display]) -> String {
    let Some(tmpl) = ALL_CODES.iter().find(|e| e.code == code) else {
        return String::new();
    };
    let mut out = String::with_capacity(tmpl.message.len());
    let mut rest = tmpl.message;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end_rel) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let end = start + end_rel;
        let inner = &rest[start + 1..end];
        if let Ok(i) = inner.parse::<usize>() {
            if let Some(a) = args.get(i) {
                out.push_str(&a.to_string());
            } else {
                out.push_str(&rest[start..=end]);
            }
        } else {
            out.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Lowest C-parser warning code — used by `mc_code.rs` to dedup overlapping
/// parser diagnostics (warnings are more specific than syntax errors).
pub const PARSER_WARNING_CODE_BASE: u32 = 2111;

macro_rules! entry {
    ($const:ident, $desc:expr, $msg:expr) => {
        ErrorCodeInfo {
            code: $const,
            name: stringify!($const),
            description: $desc,
            message: $msg,
        }
    };
}

// ============================================================================
// Pass1a: duplicate definitions (1000-1049)
// ============================================================================

/// An interface with the same name already exists in this file.
pub const DUP_INTERFACE: u32 = 1001;

/// A component with the same name already exists in this file.
pub const DUP_COMPONENT: u32 = 1002;

/// An enum with the same name already exists in this file.
pub const DUP_ENUM: u32 = 1003;

/// A module with the same name already exists in this file.
pub const DUP_MODULE: u32 = 1004;

/// A define with the same name already exists in this file.
pub const DUP_DEFINE: u32 = 1005;

// ============================================================================
// Pass1a: definition structure / CMIE load (1050-1099)
// ============================================================================

/// Definition already exists.
pub const DEF_ALREADY_EXISTS: u32 = 1051;

/// The declaration is a define or an unexpected type and cannot be loaded as a CMIE.
pub const CMIE_LOAD_REJECTED: u32 = 1052;

/// Missing subnode in an instance declaration.
pub const INST_MISSING_SUBNODE: u32 = 1053;

/// Missing subnode in a pins declaration.
pub const PINS_MISSING_SUBNODE: u32 = 1054;

/// Enum definition is missing its subnodes.
pub const ENUM_MISSING_SUBNODES: u32 = 1055;

/// Enum definition is missing its name.
pub const ENUM_MISSING_NAME: u32 = 1056;

/// Enum definition is missing its name ids.
pub const ENUM_MISSING_NAME_IDS: u32 = 1057;

/// Enum definition is missing its values.
pub const ENUM_MISSING_VALUES: u32 = 1058;

/// Malformed IO type node in a pin/port declaration.
pub const MALFORMED_IOTYPE: u32 = 1059;

/// The name is a define; a define cannot be loaded as a CMIE.
pub const CMIE_IS_DEFINE: u32 = 1060;

// ============================================================================
// Pass1b: use statements (2000-2049)
// ============================================================================

/// Invalid path in a use statement.
pub const USE_PATH_INVALID: u32 = 2001;

/// Unrecognized URI prefix — expected $, /, ./, or ../.
pub const USE_URI_PREFIX_INVALID: u32 = 2002;

/// The use target file was not found.
pub const USE_TARGET_NOT_FOUND: u32 = 2003;

/// File imports itself via a use statement.
pub const USE_SELF_IMPORT: u32 = 2004;

/// A use alias collides with an existing name.
pub const USE_ALIAS_COLLISION: u32 = 2005;

/// The versioned use target file was not found.
pub const USE_VERSIONED_TARGET_NOT_FOUND: u32 = 2006;

/// A symbol listed in use import(...) was not found in the target file.
pub const USE_IMPORT_SYMBOL_NOT_FOUND: u32 = 2007;

/// A symbol in pub use import(...) was not found and cannot be re-exported.
pub const USE_REEXPORT_SYMBOL_NOT_FOUND: u32 = 2008;

/// A use path mixes '.' and '/' separators.
pub const USE_MIXED_PATH_SEPARATORS: u32 = 2009;

/// Unexpected trailing node in a USE statement; it is ignored.
pub const USE_TRAILING_NODE: u32 = 2010;

// ============================================================================
// Pass1b: use-stage diagnostics (2050-2079)
// ============================================================================

/// Use of an undeclared dependency — add it to project.toml [dependencies] or load via --lib.
pub const USE_DEP_NOT_DECLARED: u32 = 2051;

/// Library referenced by `use` is not installed in the system root.
pub const USE_LIB_NOT_FOUND: u32 = 2052;

/// An imported symbol conflicts with an existing name.
pub const USE_SYMBOL_CONFLICT: u32 = 2061;

/// The imported symbol was not found in the target file.
pub const USE_IMPORTED_NOT_FOUND: u32 = 2071;

// ============================================================================
// Pass1b: parser / AST messages (2080-2119)
// ============================================================================

/// Generic syntax error.
pub const PARSER_SYNTAX_ERROR: u32 = 2080;

/// Invalid top-level declaration.
pub const PARSER_TOP_INVALID: u32 = 2081;

/// Invalid clause in a body.
pub const PARSER_CLAUSE_INVALID: u32 = 2082;

/// Invalid pin declaration.
pub const PARSER_PIN_INVALID: u32 = 2083;

/// Pin ID must be a constant integer, not an expression.
pub const PARSER_PIN_ID_NOT_CONST: u32 = 2084;

/// Pin name must be a constant identifier, not an expression.
pub const PARSER_PIN_NAME_NOT_CONST: u32 = 2085;

/// Net endpoint must be a port/label, not a literal.
pub const PARSER_NET_NOT_PORT: u32 = 2086;

/// Invalid net/connection expression.
pub const PARSER_NET_INVALID: u32 = 2087;

/// Invalid if/else condition block.
pub const PARSER_CONDS_INVALID: u32 = 2088;

/// Invalid role block.
pub const PARSER_ROLE_INVALID: u32 = 2089;

/// Invalid function definition.
pub const PARSER_FUNC_INVALID: u32 = 2090;

/// Invalid pins declaration.
pub const PARSER_PINS_INVALID: u32 = 2091;

/// Invalid import statement.
pub const PARSER_USE_INVALID: u32 = 2092;

/// Invalid condition body.
pub const PARSER_CONDBLOCK_INVALID: u32 = 2093;

/// Invalid instance declaration (:: syntax).
pub const PARSER_DECLAREB_INVALID: u32 = 2094;

/// Invalid body.
pub const PARSER_BODY_INVALID: u32 = 2095;

/// Invalid condition expression.
pub const PARSER_JUDGE_INVALID: u32 = 2096;

/// Invalid parameter declaration.
pub const PARSER_PARD_INVALID: u32 = 2097;

/// Invalid import path.
pub const PARSER_URI_INVALID: u32 = 2098;

/// Invalid expression list.
pub const PARSER_PHRASES_INVALID: u32 = 2099;

/// Invalid operand list.
pub const PARSER_OPDS_INVALID: u32 = 2100;

/// Invalid parameter list.
pub const PARSER_PARAMS_INVALID: u32 = 2101;

/// Invalid parameter declaration list.
pub const PARSER_PARDS_INVALID: u32 = 2102;

/// Invalid attribute value list.
pub const PARSER_ATTR_VALUES_INVALID: u32 = 2103;

/// Invalid attribute line list.
pub const PARSER_ATTR_LINES_INVALID: u32 = 2104;

/// Invalid pin name list.
pub const PARSER_PINS_NAMES_INVALID: u32 = 2105;

/// Invalid instance list.
pub const PARSER_INSTS_INVALID: u32 = 2106;

/// Invalid else-if chain.
pub const PARSER_CONDS_ELIFS_INVALID: u32 = 2107;

/// Invalid identifier list.
pub const PARSER_IDSS_INVALID: u32 = 2108;

/// Invalid path in import.
pub const PARSER_LEVELS_INVALID: u32 = 2109;

/// Invalid expression.
pub const PARSER_PHRASE_INVALID: u32 = 2110;

/// Single '|' used as a binary operator outside a pin context.
pub const PARSER_SINGLE_OR: u32 = 2111;

/// '±' used as a binary operator outside a tolerance context.
pub const PARSER_PLUSMINUS: u32 = 2112;

/// Transpose (') on a literal has no effect.
pub const PARSER_TRANSPOSE_ON_LITERAL: u32 = 2113;

/// Caret (^) on a literal has no effect.
pub const PARSER_CARET_ON_LITERAL: u32 = 2114;

/// Empty body — no clauses defined.
pub const PARSER_EMPTY_BODY: u32 = 2115;

/// Empty pins declaration.
pub const PARSER_EMPTY_PINS: u32 = 2116;

/// AST node is null/empty where a value was expected.
pub const AST_NODE_EMPTY: u32 = 2117;

/// AST node contains invalid UTF-8 data.
pub const AST_UTF8_ERROR: u32 = 2118;

/// AST node has an unexpected type.
pub const AST_TYPE_MISMATCH: u32 = 2119;

// ============================================================================
// Pass1b: name resolution (2120-2199)
// ============================================================================

/// IDS has no nodes.
pub const NAME_IDS_NO_NODES: u32 = 2121;

/// Missing subnode in a name reference.
pub const NAME_MISSING_SUBNODE: u32 = 2122;

/// Failed to parse a DECLARE node.
pub const NAME_DECLARE_PARSE_FAILED: u32 = 2123;

/// Missing subnode for a square vector.
pub const NAME_SQUARE_VECTOR_MISSING_SUBNODE: u32 = 2124;

/// Failed to process a side of a range.
pub const NAME_RANGE_SIDE_FAILED: u32 = 2125;

/// Definition not found; falling back to a label.
pub const NAME_DEF_NOT_FOUND_LABEL_FALLBACK: u32 = 2126;

/// Failed to extract ID/IDA data from a node.
pub const NAME_ID_EXTRACT_FAILED: u32 = 2127;

/// This syntax is parsed but not yet supported by the semantic layer; the declaration is ignored.
pub const NOT_SUPPORTED_YET: u32 = 2171;

/// Symbol could not be resolved to any definition after the full P1–P5 lookup
/// chain (P1 func → P2 container → P3 file → P4 use chain → P5 mcode system
/// library). Mirrors the design docs: `name-space-global.md` §1.3 /
/// `name-space-internal.md` §1.3 "not found → Unresolved / diagnostic error".
pub const SYMBOL_NOT_FOUND: u32 = 2172;

// ============================================================================
// Pass2: vector shape validation (2900-2949)
// ============================================================================

/// Shape row count mismatch (eval.md §3) recovered by broadcast/truncation.
/// Emitted at Pass2 instantiation when the §3 matching constraint table
/// rejects the pair (1-row vs N-row / N vs M) but the generator still
/// recovers (broadcast fan-out / truncate by min). Not emitted for legal
/// group semantics (DC power bus role-aligned / interface member passthrough).
pub const CONN_SHAPE_ROW_MISMATCH_RECOVERED: u32 = 2901;

/// Transpose operand shape out of range (eval.md §5.5): only 1*1 / 1*2 / 2*1 / 2*2
/// are transposable. Emitted at Pass1 (McPhrase) when the operand's derived row
/// count is known and >= 3 (e.g. `[A, B, C]'`), i.e. the operator would "merge"
/// an already-broken-apart expression, which is not meaningful.
pub const SHAPE_TRANSPOSE_LIMIT: u32 = 2902;

/// Reverse `^` is a no-op on a vector operand (eval.md §9 / examples L180):
/// parallel (`A + B`) and transposed (`X'`) operands carry no order to reverse.
/// Emitted as a hint at Pass1 when the operand is already a vector.
pub const SHAPE_REVERSE_NOOP: u32 = 2903;

/// Vector expansion dimension mismatch (eval.md §7 rule 3): both sides are
/// vectors with different row counts and implicit auto-expansion is forbidden.
/// Emitted at Pass2 (`create_connection`) when `expand_match` rejects the pair
/// (count mismatch) and truncation recovery is applied.
pub const SHAPE_EXPAND_DIM_MISMATCH: u32 = 2904;

/// Instance with 3+ pins cannot directly participate in `+` / `-`
/// (veccircuit.md inst constraint, eval.md §2): only 1x1 / 1x2 raw shapes can.
/// Emitted at Pass1 when the operand is a MultiPort component instance.
pub const SHAPE_INST_3PIN_PLUSMINUS: u32 = 2905;

/// NetShape missing on a net; the viz layer fell back to the deprecated
/// `connection_type()` inference (stage 3: rarely triggered, only on paths
/// that have not yet been covered by `build_net_shape`).
pub const SHAPE_INCOMPLETE: u32 = 2906;

// ============================================================================
// Pass1c: component definition (pins / attrs / units) (3000-3049)
// ============================================================================

/// Pin ID and pin name do not match.
pub const PIN_ID_NAME_MISMATCH: u32 = 3001;

/// Pin id count error.
pub const PIN_ID_COUNT_ERROR: u32 = 3002;

/// pins += is used without a prior pins = definition.
pub const PINS_PLUS_WITHOUT_BASE: u32 = 3003;

/// Pin name has an unsupported type.
pub const PIN_NAME_TYPE_UNSUPPORTED: u32 = 3004;

/// Pin/port name count error.
pub const PIN_NAME_COUNT_ERROR: u32 = 3005;

/// Port name has an unsupported type.
pub const PORT_NAME_TYPE_UNSUPPORTED: u32 = 3006;

/// Port name count error.
pub const PORT_NAME_COUNT_ERROR: u32 = 3007;

/// Pin expression node has an unexpected type.
pub const PIN_EXPR_TYPE_MISMATCH: u32 = 3008;

/// Attribute node type mismatch.
pub const ATTR_TYPE_MISMATCH: u32 = 3021;

/// Attribute type is not supported.
pub const ATTR_TYPE_NOT_SUPPORTED: u32 = 3022;

/// Attribute node is missing a required subnode.
pub const ATTR_MISSING_SUBNODE: u32 = 3023;

/// Invalid value type in a KVS node.
pub const KVS_VALUE_TYPE_INVALID: u32 = 3041;

/// Invalid unit value type.
pub const UVAL_VALUE_TYPE_INVALID: u32 = 3042;

/// Invalid unit value data node.
pub const UVAL_DATA_NODE_INVALID: u32 = 3043;

/// Invalid unit.
pub const UVAL_UNIT_INVALID: u32 = 3044;

/// Invalid unit value.
pub const UVAL_VALUE_INVALID: u32 = 3045;

/// The unit is not supported.
pub const UVAL_UNIT_UNSUPPORTED: u32 = 3046;

/// Missing unit value data node.
pub const UVAL_MISSING_DATA_NODE: u32 = 3047;

/// Invalid unit value or float format.
pub const UVAL_FORMAT_INVALID: u32 = 3048;

/// Invalid unit variant (angle, charge, magnetic flux, slew rate, ...).
pub const UVAL_UNIT_VARIANT_INVALID: u32 = 3049;

// ============================================================================
// Pass1c: module body (3050-3099)
// ============================================================================

/// Missing subnode in a module body clause.
pub const MODULE_MISSING_SUBNODE: u32 = 3051;

/// Module does not support PINS directly; use in/out/io declarations.
pub const MODULE_PINS_UNSUPPORTED: u32 = 3052;

/// Module does not support role definition.
pub const MODULE_ROLE_UNSUPPORTED: u32 = 3053;

/// Unexpected type in a module parameter.
pub const MODULE_PARAM_TYPE_UNEXPECTED: u32 = 3054;

/// Function was not found in the class.
pub const MODULE_METHOD_NOT_FOUND: u32 = 3071;

/// Unexpected clause type in a module body.
pub const UNEXPECTED_CLAUSE_TYPE: u32 = 3081;

// ============================================================================
// Pass1c: params / functions (3100-3149)
// ============================================================================

/// Empty net in a function or module body.
pub const FUNC_EMPTY_NET: u32 = 3101;

/// Invalid parameter declaration node.
pub const PARAM_DECLARE_INVALID: u32 = 3103;

/// Invalid parameter name.
pub const PARAM_NAME_INVALID: u32 = 3104;

/// Invalid parameter set.
pub const PARAM_SET_INVALID: u32 = 3105;

/// Invalid parameter unit value.
pub const PARAM_UVAL_INVALID: u32 = 3106;

/// Expected a class in the declaration unit value.
pub const PARAM_CLASS_EXPECTED: u32 = 3107;

/// Expected an instance in the declaration unit value.
pub const PARAM_INSTANCE_EXPECTED: u32 = 3108;

/// Failed to extract the parameter name.
pub const PARAM_NAME_EXTRACT_FAILED: u32 = 3109;

/// Instance::class lookup failed; the binding is treated as a plain pin alias.
pub const PARAM_INST_LOOKUP_FAILED: u32 = 3110;

/// Interface pin count does not match the number of declared pin IDs.
pub const PARAM_DECLARE_IFACE_PINS: u32 = 3111;

/// Missing function name in a function call.
pub const FUNC_CALL_MISSING_NAME: u32 = 3131;

/// A connection line failed to parse.
pub const CONN_LINE_PARSE_FAILED: u32 = 3132;

/// Invalid function body node.
pub const FUNC_BODY_INVALID: u32 = 3133;

/// A connection line was dropped because McPhrase::new returned None.
pub const FUNC_LINE_DROPPED: u32 = 3134;

/// Function call parse failure.
pub const FCALL_PARSE_FAILED: u32 = 3135;

// ============================================================================
// Pass1c: instance declaration / reference (3150-3199)
// ============================================================================

/// Failed to parse an instance in an expression context.
pub const INST_EXPR_PARSE_FAILED: u32 = 3151;

/// Curly-member construction requires a component or module base.
pub const CURLY_MN_WRONG_BASE: u32 = 3152;

/// No class node found in the instance declaration.
pub const INST_CLASS_NODE_MISSING: u32 = 3153;

/// No instance node found.
pub const INST_NODE_MISSING: u32 = 3154;

/// Missing class id node.
pub const INST_CLASS_ID_MISSING: u32 = 3155;

/// Failed to parse class ids.
pub const INST_CLASS_IDS_PARSE_FAILED: u32 = 3156;

/// Unresolved class — the library may not be loaded.
pub const INST_CLASS_UNRESOLVED: u32 = 3157;

/// Malformed return statement.
pub const FUNC_RETURN_MALFORMED: u32 = 3161;

/// Invalid return expression — expected this or a label/bus.
pub const FUNC_RETURN_EXPR_INVALID: u32 = 3162;

/// A function may have at most one return statement.
pub const FUNC_MULTIPLE_RETURNS: u32 = 3163;

/// Interface member not found in the component.
pub const IFACE_MEMBER_NOT_FOUND: u32 = 3171;

/// Cannot access interface members using curly-bracket syntax.
pub const IFACE_CURLY_MEMBER_INVALID: u32 = 3172;

/// Component not found for the interface reference.
pub const IFACE_COMPONENT_NOT_FOUND: u32 = 3173;

/// Interface not found for a bus reference.
pub const IFACE_BUS_NOT_FOUND: u32 = 3174;

/// Port(s) not found in the module.
pub const MODULE_PORT_NOT_FOUND: u32 = 3175;

/// Name is already an instance; cannot create a bus with these members.
pub const BUS_NAME_ALREADY_INSTANCE: u32 = 3176;

/// Pin(s) not found in the interface.
pub const IFACE_PIN_NOT_FOUND: u32 = 3177;

/// Interface member lookup failed.
pub const IFACE_MEMBER_LOOKUP_FAILED: u32 = 3178;

/// Pin(s) not found in the component or interface.
pub const COMPONENT_PIN_NOT_FOUND: u32 = 3179;

/// Interface has no top-level pin definitions (all pins are inside role blocks); no pin-to-member mapping is created.
pub const IFACE_NO_TOPLEVEL_PINS: u32 = 3180;

// ============================================================================
// Pass2: connection / shape (4000-4049)
// ============================================================================

/// Transposed connection size mismatch.
pub const CONN_TRANSPOSE_SIZE_MISMATCH: u32 = 4001;

/// Shape mismatch in a <- connection.
pub const CONN_LEFT_ARROW_SHAPE_MISMATCH: u32 = 4002;

/// The transpose operator is not allowed at this position.
pub const CONN_CANNOT_TRANSPOSE: u32 = 4003;

/// An instance with 3+ pins cannot directly participate in a '+' operation.
pub const CONN_PARALLEL_INVALID: u32 = 4004;

/// Shape mismatch in a parallel connection.
pub const CONN_PARALLEL_SHAPE_MISMATCH: u32 = 4005;

/// An instance with 3+ pins cannot directly participate in a '-' operation.
pub const CONN_SERIES_INVALID: u32 = 4006;

/// Shape mismatch in a -> connection.
pub const CONN_SERIES_SHAPE_MISMATCH: u32 = 4007;

/// The operator is not supported in connection lines; use '+' for parallel, '-' / '->' for series.
pub const CONN_OPERATOR_UNSUPPORTED: u32 = 4008;

/// Unexpected AST node type in a phrase.
pub const PHRASE_AST_TYPE_UNEXPECTED: u32 = 4009;

/// No ports found in the component.
pub const CONN_NO_PORTS_COMPONENT: u32 = 4010;

/// No ports found in the module.
pub const CONN_NO_PORTS_MODULE: u32 = 4011;

/// No ports found in the interface.
pub const CONN_NO_PORTS_INTERFACE: u32 = 4012;

/// Dot operator does not apply to a Series.
pub const CONN_DOT_SERIES: u32 = 4013;

/// Dot operator does not apply to a Node.
pub const CONN_DOT_NODE: u32 = 4014;

/// Dot operator does not apply to a Transposed.
pub const CONN_DOT_TRANSPOSED: u32 = 4015;

/// Dot operator does not apply to a Lead.
pub const CONN_DOT_LEAD: u32 = 4016;

/// Dot operator does not apply to a Group.
pub const CONN_DOT_GROUP: u32 = 4017;

/// Dot operator does not apply to an Endpoint.
pub const CONN_DOT_ENDPOINT: u32 = 4018;

/// Dot operator already applied to a Member.
pub const CONN_DOT_MEMBER: u32 = 4019;

/// Closure has no output interface to access.
pub const CLOSURE_NO_OUTPUT_IFACE: u32 = 4020;

/// FuncCall has no return interface to access.
pub const FUNCCALL_NO_RETURN_IFACE: u32 = 4021;

/// Member not found in the interface.
pub const PHRASE_IFACE_MEMBER_NOT_FOUND: u32 = 4022;

/// Curly-member construction: left member list is empty.
pub const CURLY_LEFT_EMPTY: u32 = 4023;

/// Curly-member construction: right member list is empty.
pub const CURLY_RIGHT_EMPTY: u32 = 4024;

/// Cannot convert the curly-member result to a bus.
pub const CURLY_NOT_BUS: u32 = 4025;

/// Groups with different branch counts cannot connect.
pub const GROUP_BRANCH_COUNT_MISMATCH: u32 = 4026;

// ============================================================================
// Pass2: netlist heuristics (D-series / layout) (4050-4099)
// ============================================================================

/// A box has a placeholder pin not mapped to any real component pin.
pub const GHOST_PORT_BOX: u32 = 4050;

/// Multiple points resolve to the same node — possible short circuit (E2003).
pub const NET_MERGED_SHORT: u32 = 4051;

/// Bus member order mismatch after sorting (E2005).
pub const NET_BUS_ORDER_MISMATCH: u32 = 4052;

/// Bus pin numbers are non-monotonic; member→pin mapping may be wrong after sorting.
pub const SORT_HAZARD: u32 = 4053;

/// A '_' placeholder could not be bound to any pin.
pub const FLOATING_PLACEHOLDER: u32 = 4054;

/// A net endpoint is not mapped to any box — possible unexposed module boundary port.
pub const GHOST_PORT: u32 = 4055;

/// '_X' prefix identifier used as a standalone operand — it is a member name, not the wire '_'.
pub const LEAD_PREFIX_ID_AS_WIRE: u32 = 4058;

/// Func param shadows a same-named component pin during func body expansion.
pub const FUNC_PARAM_SHADOWS_PIN: u32 = 4059;

/// Pullup/pulldown degenerated into a signal-signal bridge.
pub const PULLUP_DEGENERATE: u32 = 4056;

/// A single-element square bracket expands to an unknown instance; the statement may produce no nets or constraints.
pub const NET_DROPPED_STATEMENT: u32 = 4057;

/// Layout attribute is missing a required subnode.
pub const LAYOUT_MISSING_SUBNODE: u32 = 4081;

/// Layout attribute node type mismatch.
pub const LAYOUT_TYPE_MISMATCH: u32 = 4082;

/// Layout set is missing a required subnode.
pub const LAYOUT_SET_MISSING_SUBNODE: u32 = 4083;

/// Layout values node type mismatch.
pub const LAYOUT_VALUES_TYPE_MISMATCH: u32 = 4084;

/// Layout name is missing a required subnode.
pub const LAYOUT_NAME_MISSING_SUBNODE: u32 = 4085;

/// Layout edge is missing a subnode.
pub const LAYOUT_EDGE_MISSING_SUBNODE: u32 = 4086;

/// Layout edge node type mismatch.
pub const LAYOUT_EDGE_TYPE_MISMATCH: u32 = 4087;

/// Layout edge name is missing a subnode.
pub const LAYOUT_EDGE_NAME_MISSING_SUBNODE: u32 = 4088;

/// Layout value is missing a subnode.
pub const LAYOUT_VALUE_MISSING_SUBNODE: u32 = 4089;

/// Layout value node type mismatch.
pub const LAYOUT_VALUE_TYPE_MISMATCH: u32 = 4090;

/// Layout set is missing a subnode.
pub const LAYOUT_SET_SUBNODE_MISSING: u32 = 4091;

/// Malformed layout: unexpected extra nodes.
pub const LAYOUT_EXTRA_NODES: u32 = 4092;

/// Layout values are missing a subnode.
pub const LAYOUT_VALUES_MISSING_SUBNODE: u32 = 4093;

/// CONST node is missing its INT subnode.
pub const LAYOUT_CONST_MISSING_INT: u32 = 4094;

/// Parse error in a layout pin number.
pub const LAYOUT_PIN_NUMBER_PARSE: u32 = 4095;

/// Layout edge name id is missing a subnode.
pub const LAYOUT_EDGE_NAME_ID_MISSING_SUBNODE: u32 = 4096;

/// Invalid layout edge.
pub const LAYOUT_EDGE_INVALID: u32 = 4097;

/// Malformed layout: edge name is not an ID.
pub const LAYOUT_EDGE_NAME_NOT_ID: u32 = 4098;

// ============================================================================
// Pass2: netlist / interface binding (4100-4149)
// ============================================================================

/// Net has multiple drivers — possible short circuit.
pub const NET_MULTI_DRIVE: u32 = 4101;

/// Interface requires more pins than are bound to physical pins.
pub const IFACE_PINS_NOT_ALL_BOUND: u32 = 4102;

/// Net has inputs but no output/power driver.
pub const NET_NO_DRIVER: u32 = 4103;

/// Interface role referenced by a param does not exist in the interface.
pub const IFACE_ROLE_NOT_FOUND: u32 = 4104;

/// Power nets with different voltages are shorted together.
pub const NET_VOLTAGE_MISMATCH: u32 = 4105;

/// Interface referenced by a param is not loaded.
pub const IFACE_NOT_LOADED: u32 = 4106;

/// Deprecated interface/component/param used.
pub const IFACE_DEPRECATED_CMIE: u32 = 4107;

/// An input port is not connected to any net.
pub const NET_INPUT_UNCONNECTED: u32 = 4108;

/// An NC port is connected to a net.
pub const NET_NC_CONNECTED: u32 = 4109;

/// An output drives nothing.
pub const NET_OUTPUT_UNDRIVEN: u32 = 4110;

/// Net has both an output and a power supply — backfeed risk.
pub const NET_BACKFEED_RISK: u32 = 4111;

/// Instance has no pins connected to any net.
pub const NET_INSTANCE_UNCONNECTED: u32 = 4112;

/// Net has outputs and power but no input.
pub const NET_OUTPUTS_NO_INPUT: u32 = 4113;

/// Module port is not connected to any net.
pub const NET_MODULE_PORT_UNCONNECTED: u32 = 4114;

/// Net has only one endpoint — possible dangling connection.
pub const NET_DANGLING_ENDPOINT: u32 = 4115;

/// Only some of the instance pins are connected.
pub const NET_PARTIAL_CONNECTION: u32 = 4116;

/// A bidirectional port is not connected to any net.
pub const NET_BIDIR_UNCONNECTED: u32 = 4117;

/// Design has many power nets; review for consolidation.
pub const NET_POWER_NET_COUNT: u32 = 4118;

// ============================================================================
// Pass2: instantiation checks (4150-4199)
// ============================================================================

/// A chain link was skipped because the method is not defined on the instance.
pub const INST_CHAIN_LINK_SKIPPED: u32 = 4150;

/// Instance argument has no formal port to bind.
pub const INST_ARG_NO_FORMAL_PORT: u32 = 4151;

/// Instance method could not be resolved; passed through instead.
pub const INST_METHOD_FALLBACK: u32 = 4152;

/// Interface instantiation failed.
pub const INST_IFACE_INSTANTIATE_FAILED: u32 = 4153;

/// Sub-module instantiation failed.
pub const INST_SUBMODULE_INSTANTIATE_FAILED: u32 = 4154;

/// Line references a component class whose instantiation failed; the whole line is skipped.
pub const INST_LINE_SKIP_FAILED_CLASS: u32 = 4155;

/// A connection line failed to expand.
pub const INST_LINE_PARSE_FAILED: u32 = 4156;

/// Expanded builtin two-pin pair failed.
pub const INST_BUILTIN_TWOPIN_EXPAND_FAILED: u32 = 4157;

/// A member of a connection line failed to process.
pub const INST_MEMBER_PROCESS_FAILED: u32 = 4158;

/// Connection between adjacent members of a series failed.
pub const INST_ADJACENT_CONNECT_FAILED: u32 = 4159;

/// A `.Cap(_)` shunt member failed to process.
pub const INST_SHUNT_PROCESS_FAILED: u32 = 4160;

/// A module-level function body line failed.
pub const INST_FUNC_BODY_LINE_FAILED: u32 = 4161;

/// Failed to instantiate a FuncCall during lane-by-lane wiring.
pub const INST_LANE_FUNCCALL_FAILED: u32 = 4162;

/// Failed to instantiate a Transposed member during lane-by-lane wiring.
pub const INST_LANE_TRANSPOSED_FAILED: u32 = 4163;

/// Connection shape mismatch; truncated to the smaller side.
pub const CONN_SHAPE_MISMATCH_TRUNCATED: u32 = 4164;

/// Parallel '+' operand dimension mismatch; operand merged into the anchor's left net.
pub const CONN_PARALLEL_DIM_MISMATCH: u32 = 4165;

/// Group connection shape mismatch; truncated by branch count.
pub const CONN_GROUP_SHAPE_MISMATCH: u32 = 4166;

/// Component input pin count mismatch in a function call.
pub const INST_INPUT_PIN_COUNT_MISMATCH: u32 = 4167;

/// Component output pin count mismatch in a function call.
pub const INST_OUTPUT_PIN_COUNT_MISMATCH: u32 = 4168;

/// Inline module instantiation failed.
pub const INST_INLINE_MODULE_FAILED: u32 = 4169;

/// Module input port count mismatch in a function call.
pub const INST_INPUT_PORT_COUNT_MISMATCH: u32 = 4170;

/// Module output port count mismatch in a function call.
pub const INST_OUTPUT_PORT_COUNT_MISMATCH: u32 = 4171;

/// Sub-module DC power port is never connected (missing power argument?).
pub const INST_POWER_PORT_UNBOUND: u32 = 4172;

/// A constructor function body line failed.
pub const INST_CTOR_BODY_LINE_FAILED: u32 = 4173;

/// Constructor parameter binding failed.
pub const INST_CTOR_PARAM_BIND_FAILED: u32 = 4174;

/// Instance argument has no formal port to bind (with module/bound details).
pub const INST_ARG_UNBOUND_DETAILED: u32 = 4175;

// ============================================================================
// Pass3: duplicate validation (5000-5049)
// ============================================================================

/// Same name defined in another file (cross-file duplicate).
pub const DUP_CMIE_CROSS_FILE: u32 = 5001;

/// Duplicate definition within the same declaration.
pub const DUP_WITHIN: u32 = 5002;

/// Enum value appears more than once in the enum.
pub const DUP_ENUM_VALUE: u32 = 5003;

// ============================================================================
// Pass3: naming / style (5050-5099)
// ============================================================================

/// Component name starts with lowercase; convention is UPPER_SNAKE.
pub const NAME_COMPONENT_LOWERCASE: u32 = 5051;

/// Port name shadows a library CMIE name.
pub const NAME_PORT_SHADOWS_CMIE: u32 = 5052;

/// Pins use mixed naming conventions.
pub const NAME_PIN_MIXED_CONVENTION: u32 = 5053;

/// Instance name is a single character.
pub const NAME_INSTANCE_SINGLE_CHAR: u32 = 5054;

/// Pin name is purely numeric.
pub const NAME_PIN_NUMERIC: u32 = 5055;

/// Port/instance name shadows a library CMIE name.
pub const NAME_PORT_INST_SHADOWS_CMIE: u32 = 5056;

/// Parameter name shadows a library CMIE name.
pub const NAME_PARAM_SHADOWS_CMIE: u32 = 5057;

// ============================================================================
// Pass3: reference integrity (5100-5149)
// ============================================================================

/// Spec key references a parameter that is not declared.
pub const SPEC_KEY_UNDECLARED_PARAM: u32 = 5101;

/// Reference integrity violation.
pub const REF_INTEGRITY: u32 = 5102;

/// Function has parameters but no body (empty implementation).
pub const FUNC_PARAMS_NO_BODY: u32 = 5103;

/// pins.X references an undefined pin name.
pub const EXPR_PINS_X_UNDEFINED: u32 = 5104;

// ============================================================================
// Pass3: ports / pins (5150-5199)
// ============================================================================

/// Instance is declared more than once in the module.
pub const INST_DECLARED_MULTIPLE: u32 = 5151;

/// Duplicate port name in the module — ambiguous.
pub const PORT_DUPLICATE_NAME: u32 = 5152;

/// The class is a component/module/enum, not an interface.
pub const NOT_AN_INTERFACE: u32 = 5153;

/// Name is both a value parameter and an instance.
pub const NAME_PARAM_AND_INSTANCE: u32 = 5154;

/// Pin is not connected to any net.
pub const PIN_UNCONNECTED: u32 = 5155;

/// Pin uses conflicting option names.
pub const PIN_CONFLICTING_OPTIONS: u32 = 5156;

/// Return statement used outside a function.
pub const FUNC_RETURN_OUTSIDE_FUNCTION: u32 = 5157;

/// Return statement specifies a literal instead of an endpoint.
pub const FUNC_RETURN_LITERAL_INVALID: u32 = 5158;

/// Empty instance table in a [] :: TYPE declaration.
pub const INST_EMPTY_TABLE: u32 = 5159;

/// this :: TYPE declaration is not allowed.
pub const INST_THIS_TYPE: u32 = 5160;

/// Role used as a function-call argument.
pub const FUNC_ROLE_AS_ARG: u32 = 5161;

/// Module port is declared but never connected.
pub const MODULE_PORT_UNUSED: u32 = 5162;

/// Condition compares against a single binary value.
pub const COND_SINGLE_BINARY: u32 = 5163;

// ============================================================================
// Pass3: functions / roles / defaults (5200-5249)
// ============================================================================

/// Enum has only one value.
pub const ENUM_SINGLE_VALUE: u32 = 5201;

/// Integer param has a string default.
pub const PARAM_INT_DEFAULT_STRING: u32 = 5202;

/// String param has a numeric-looking default.
pub const PARAM_STRING_DEFAULT_NUMERIC: u32 = 5203;

/// Unit-value param default has no unit suffix (e.g. '5V').
pub const PARAM_UV_DEFAULT_NO_UNIT: u32 = 5204;

/// Param has an invalid float default.
pub const PARAM_FLOAT_DEFAULT_INVALID: u32 = 5205;

/// Integer param default is negative.
pub const PARAM_NEGATIVE_DEFAULT: u32 = 5206;

// ============================================================================
// Pass3: definition structure (M-series) (5250-5299)
// ============================================================================

/// Parameter uses a reserved keyword.
pub const PARAM_RESERVED_KEYWORD: u32 = 5251;

/// Function has an empty body.
pub const FUNC_EMPTY_BODY: u32 = 5252;

/// Component has no params, pins, attributes, or functions.
pub const COMPONENT_EMPTY: u32 = 5253;

/// Component has no pin definitions.
pub const COMPONENT_NO_PINS: u32 = 5254;

/// Interface has no pins or roles.
pub const INTERFACE_EMPTY: u32 = 5255;

/// Instance references a class that is not loaded.
pub const INST_CLASS_NOT_LOADED: u32 = 5256;

/// Component name uses mixed case; convention is UPPER_SNAKE.
pub const COMPONENT_MIXED_CASE: u32 = 5257;

/// Bus has a duplicate member.
pub const BUS_DUPLICATE_MEMBER: u32 = 5258;

/// Component has functions with the same body.
pub const COMPONENT_DUPLICATE_FUNC_BODY: u32 = 5259;

/// Define has no attributes.
pub const DEFINE_NO_ATTRS: u32 = 5260;

/// Define contains a non-attribute clause.
pub const DEFINE_NON_ATTR_CLAUSE: u32 = 5261;

/// Interface expects more pins than are bound.
pub const IFACE_PIN_COUNT_MISMATCH: u32 = 5262;

/// Function shares its name with a port/param.
pub const FUNC_SHARES_NAME_WITH_PORT: u32 = 5263;

/// Net connects two outputs.
pub const NET_BOTH_OUTPUTS: u32 = 5264;

/// Inline function body literal used as a call argument.
pub const FUNC_INLINE_BODY_LITERAL_ARG: u32 = 5265;

/// Function declares parameters it never uses.
pub const FUNC_PARAMS_UNUSED: u32 = 5266;

/// Spec key appears more than once.
pub const SPEC_KEY_DUPLICATE: u32 = 5267;

// ============================================================================
// Pass3: .int class checks (5300-5349)
// ============================================================================

/// Same name used for different definition kinds.
pub const DEF_AMBIGUOUS_NAME: u32 = 5301;

/// Definition references a class that is not loaded.
pub const DEF_REF_NOT_LOADED: u32 = 5302;

/// Component has an unconventional '.int' suffix.
pub const COMPONENT_INT_SUFFIX: u32 = 5303;

/// Enum has an unconventional '.int' suffix.
pub const ENUM_INT_SUFFIX: u32 = 5304;

// ============================================================================
// Pass3: instance / attribute checks (5350-5399)
// ============================================================================

/// Attribute uses a reserved keyword.
pub const ATTR_RESERVED_KEYWORD: u32 = 5351;

/// Instance passes more/fewer args than the class declares.
pub const INST_ARG_COUNT_MISMATCH: u32 = 5352;

/// Role has an empty body.
pub const ROLE_EMPTY_BODY: u32 = 5353;

/// Role shares its name with a parameter or pin/port.
pub const ROLE_NAME_SHADOWS: u32 = 5354;

/// Attribute nesting depth exceeds 16.
pub const ATTR_NESTING_TOO_DEEP: u32 = 5355;

/// Attribute references an undefined pin group, or role used outside a component.
pub const ATTR_PIN_GROUP_UNDEFINED: u32 = 5356;

/// Component mixes pins = and pins.X = attributes, or uses a non-constant default.
pub const PINS_PLUS_AND_PINS_CONFLICT: u32 = 5357;

// ============================================================================
// Pass3: enum / expression checks (5400-5449)
// ============================================================================

/// Enum has a duplicate value.
pub const ENUM_DUPLICATE_VALUE: u32 = 5401;

/// Enum member contains a dot.
pub const ENUM_MEMBER_DOT: u32 = 5402;

/// Enum member starts with a digit.
pub const ENUM_MEMBER_LEADING_DIGIT: u32 = 5403;

/// Enum member is a reserved keyword.
pub const ENUM_MEMBER_RESERVED: u32 = 5404;

/// Attribute has an infinite float value.
pub const ATTR_INFINITE_FLOAT: u32 = 5405;

/// Attribute has a suspiciously large integer value.
pub const ATTR_LARGE_INT: u32 = 5406;

/// Range appears reversed; did you mean the opposite order?
pub const RANGE_REVERSED: u32 = 5407;

/// Range expands to a single element.
pub const RANGE_SINGLE_ELEMENT: u32 = 5408;

/// IDX key has multiple slice specifications.
pub const IDX_MULTIPLE_SLICE_SPEC: u32 = 5409;

/// 'this' used in a top-level net line; it is only valid inside instance/function contexts.
pub const EXPR_THIS_TOP_LEVEL: u32 = 5410;

/// Net connects only to '_' placeholder; the connection has no effect.
pub const EXPR_PLACEHOLDER_ONLY: u32 = 5411;

/// Attribute value equals its own key; likely a copy-paste mistake.
pub const ATTR_SELF_REFERENTIAL: u32 = 5412;

// ============================================================================
// Pass3: condition blocks (5450-5499)
// ============================================================================

/// Conditional block has an empty body.
pub const COND_EMPTY_BODY: u32 = 5451;

/// if without a matching else.
pub const COND_IF_WITHOUT_ELSE: u32 = 5452;

/// NC pin used at component level.
pub const PIN_NC_COMPONENT_LEVEL: u32 = 5453;

/// Power pin has no voltage attribute.
pub const POWER_PIN_NO_VOLTAGE: u32 = 5454;

/// Pin mixes In and Out IO types.
pub const PIN_IO_MIX_IN_OUT: u32 = 5455;

/// Pin mixes Output and Power IO types.
pub const PIN_IO_MIX_OUTPUT_POWER: u32 = 5456;

/// Pin mixes Analog and Power IO types.
pub const PIN_IO_MIX_ANALOG_POWER: u32 = 5457;

/// Parameter shares its name with a pin.
pub const PARAM_PIN_NAME_SHADOW: u32 = 5458;

/// Module is a stub.
pub const MODULE_STUB: u32 = 5459;

// ============================================================================
// Pass3: hardware checks (5500-5549)
// ============================================================================

/// Too many power pins.
pub const HW_POWER_PINS_EXCESS: u32 = 5501;

/// Pin numbers have gaps.
pub const HW_PIN_NUMBER_GAP: u32 = 5502;

/// Pin count is unusually high.
pub const HW_PIN_COUNT_HIGH: u32 = 5503;

/// Component has zero pins but parameter attributes.
pub const HW_ZERO_PINS_WITH_PARAMS: u32 = 5504;

/// Consecutive NC pins.
pub const HW_NC_PINS_CONTIGUOUS: u32 = 5505;

/// Interface role is never bound.
pub const HW_IFACE_ROLE_UNBOUND: u32 = 5506;

/// All pins have the same IO type.
pub const HW_ALL_SAME_IO_TYPE: u32 = 5507;

/// Missing 'name' attribute.
pub const HW_MISSING_NAME_ATTR: u32 = 5508;

/// Has a name but no description.
pub const HW_NAME_WITHOUT_DESC: u32 = 5509;

/// Function parameter shadows a pin name.
pub const HW_FUNC_PARAM_SHADOWS_PIN: u32 = 5510;

/// Interface is defined but never bound.
pub const HW_IFACE_NEVER_BOUND: u32 = 5511;

// ============================================================================
// Pass3: type / unit compatibility (5550-5599)
// ============================================================================

/// Closure references a free variable that is not declared.
pub const TYPE_CLOSURE_FREE_VAR: u32 = 5551;

/// Incompatible types or unit types.
pub const TYPE_INCOMPATIBLE: u32 = 5552;

// ============================================================================
// Pass3: global diagnostics (5600-5649)
// ============================================================================

/// Parameter or port is declared but never used.
pub const UNUSED_PARAM_OR_PORT: u32 = 5641;

/// Port is declared but never used in any net connection.
pub const PORT_NEVER_USED: u32 = 5642;

/// Parameter has no inferred type.
pub const UNTYPED_PARAM: u32 = 5643;

// ============================================================================
// ERC (electrical rule check) (6000-6099)
// ============================================================================

/// Single-point net: only one connection.
pub const ERC_SINGLE_POINT_NET: u32 = 6001;

/// Unconnected port: not connected to any net.
pub const ERC_UNCONNECTED_PORT: u32 = 6002;

/// Multi-drive net.
pub const ERC_MULTI_DRIVE_NET: u32 = 6003;

/// Floating net.
pub const ERC_FLOATING_NET: u32 = 6004;

static ALL_CODES: &[ErrorCodeInfo] = &[
    // ---- section ----
    entry!(DUP_INTERFACE, "An interface with the same name already exists in this file.", "Duplicate interface"),
    entry!(DUP_COMPONENT, "A component with the same name already exists in this file.", "Duplicate component"),
    entry!(DUP_ENUM, "An enum with the same name already exists in this file.", "Duplicate enum"),
    entry!(DUP_MODULE, "A module with the same name already exists in this file.", "Duplicate module"),
    entry!(DUP_DEFINE, "A define with the same name already exists in this file.", "Duplicate define"),
    // ---- section ----
    entry!(DEF_ALREADY_EXISTS, "Definition already exists.", "Definition already exists"),
    entry!(CMIE_LOAD_REJECTED, "The declaration is a define or an unexpected type and cannot be loaded as a CMIE.", "Unexpected declaration type {0} for CMIE load"),
    entry!(INST_MISSING_SUBNODE, "Missing subnode in an instance declaration.", "Missing subnode in an instance declaration."),
    entry!(PINS_MISSING_SUBNODE, "Missing subnode in a pins declaration.", "Missing subnode in a pins declaration."),
    entry!(ENUM_MISSING_SUBNODES, "Enum definition is missing its subnodes.", "Missing subnodes for enum"),
    entry!(ENUM_MISSING_NAME, "Enum definition is missing its name.", "Missing name for enum"),
    entry!(ENUM_MISSING_NAME_IDS, "Enum definition is missing its name ids.", "Missing name ids for enum"),
    entry!(ENUM_MISSING_VALUES, "Enum definition is missing its values.", "Missing values for enum"),
    entry!(MALFORMED_IOTYPE, "Malformed IO type node in a pin/port declaration.", "Malformed IOTYPE node"),
    entry!(CMIE_IS_DEFINE, "The name is a define; a define cannot be loaded as a CMIE.", "'{0}' is a define; not loadable as a CMIE"),
    // ---- section ----
    entry!(USE_PATH_INVALID, "Invalid path in a use statement.", "Invalid path in USE"),
    entry!(USE_URI_PREFIX_INVALID, "Unrecognized URI prefix — expected $, /, ./, or ../.", "Unrecognized URI prefix — expected $, /, ./, or ../"),
    entry!(USE_TARGET_NOT_FOUND, "The use target file was not found.", "use target not found: {0}"),
    entry!(USE_SELF_IMPORT, "File imports itself via a use statement.", "File imports itself via a use statement."),
    entry!(USE_ALIAS_COLLISION, "A use alias collides with an existing name.", "A use alias collides with an existing name."),
    entry!(USE_VERSIONED_TARGET_NOT_FOUND, "The versioned use target file was not found.", "The versioned use target file was not found."),
    entry!(USE_IMPORT_SYMBOL_NOT_FOUND, "A symbol listed in use import(...) was not found in the target file.", "A symbol listed in use import(...) was not found in the target file."),
    entry!(USE_REEXPORT_SYMBOL_NOT_FOUND, "A symbol in pub use import(...) was not found and cannot be re-exported.", "A symbol in pub use import(...) was not found and cannot be re-exported."),
    entry!(USE_MIXED_PATH_SEPARATORS, "A use path mixes '.' and '/' separators.", "A use path mixes '.' and '/' separators."),
    entry!(USE_TRAILING_NODE, "Unexpected trailing node in a USE statement; it is ignored.", "unexpected trailing node {0} in USE statement; it is ignored"),
    // ---- section ----
    entry!(USE_DEP_NOT_DECLARED, "Use of an undeclared dependency — add it to project.toml [dependencies] or load via --lib.", "use of undeclared dependency '{0}': add it to project.toml [dependencies] or load via --lib"),
    entry!(USE_LIB_NOT_FOUND, "The library is not installed in the system root — install it with `mcc lib install` or load it with --lib.", "library '{0}' not found in the system root; install it with `mcc lib install` or load it with --lib"),
    entry!(USE_SYMBOL_CONFLICT, "An imported symbol conflicts with an existing name.", "symbol conflict in module '{0}': {1} collides with previous use from '{2}'. Use 'as' alias to disambiguate"),
    entry!(USE_IMPORTED_NOT_FOUND, "The imported symbol was not found in the target file.", "imported symbol '{0}' not found in '{1}'"),
    // ---- section ----
    entry!(PARSER_SYNTAX_ERROR, "Generic syntax error.", "Generic syntax error."),
    entry!(PARSER_TOP_INVALID, "Invalid top-level declaration.", "Invalid top-level declaration."),
    entry!(PARSER_CLAUSE_INVALID, "Invalid clause in a body.", "Invalid clause in a body."),
    entry!(PARSER_PIN_INVALID, "Invalid pin declaration.", "Invalid pin declaration."),
    entry!(PARSER_PIN_ID_NOT_CONST, "Pin ID must be a constant integer, not an expression.", "Pin ID must be a constant integer, not an expression."),
    entry!(PARSER_PIN_NAME_NOT_CONST, "Pin name must be a constant identifier, not an expression.", "Pin name must be a constant identifier, not an expression."),
    entry!(PARSER_NET_NOT_PORT, "Net endpoint must be a port/label, not a literal.", "Net endpoint must be a port/label, not a literal."),
    entry!(PARSER_NET_INVALID, "Invalid net/connection expression.", "Invalid net/connection expression."),
    entry!(PARSER_CONDS_INVALID, "Invalid if/else condition block.", "Invalid if/else condition block."),
    entry!(PARSER_ROLE_INVALID, "Invalid role block.", "Invalid role block."),
    entry!(PARSER_FUNC_INVALID, "Invalid function definition.", "Invalid function definition."),
    entry!(PARSER_PINS_INVALID, "Invalid pins declaration.", "Invalid pins declaration."),
    entry!(PARSER_USE_INVALID, "Invalid import statement.", "Invalid import statement."),
    entry!(PARSER_CONDBLOCK_INVALID, "Invalid condition body.", "Invalid condition body."),
    entry!(PARSER_DECLAREB_INVALID, "Invalid instance declaration (:: syntax).", "Invalid instance declaration (:: syntax)."),
    entry!(PARSER_BODY_INVALID, "Invalid body.", "Invalid body."),
    entry!(PARSER_JUDGE_INVALID, "Invalid condition expression.", "Invalid condition expression."),
    entry!(PARSER_PARD_INVALID, "Invalid parameter declaration.", "Invalid parameter declaration."),
    entry!(PARSER_URI_INVALID, "Invalid import path.", "Invalid import path."),
    entry!(PARSER_PHRASES_INVALID, "Invalid expression list.", "Invalid expression list."),
    entry!(PARSER_OPDS_INVALID, "Invalid operand list.", "Invalid operand list."),
    entry!(PARSER_PARAMS_INVALID, "Invalid parameter list.", "Invalid parameter list."),
    entry!(PARSER_PARDS_INVALID, "Invalid parameter declaration list.", "Invalid parameter declaration list."),
    entry!(PARSER_ATTR_VALUES_INVALID, "Invalid attribute value list.", "Invalid attribute value list."),
    entry!(PARSER_ATTR_LINES_INVALID, "Invalid attribute line list.", "Invalid attribute line list."),
    entry!(PARSER_PINS_NAMES_INVALID, "Invalid pin name list.", "Invalid pin name list."),
    entry!(PARSER_INSTS_INVALID, "Invalid instance list.", "Invalid instance list."),
    entry!(PARSER_CONDS_ELIFS_INVALID, "Invalid else-if chain.", "Invalid else-if chain."),
    entry!(PARSER_IDSS_INVALID, "Invalid identifier list.", "Invalid identifier list."),
    entry!(PARSER_LEVELS_INVALID, "Invalid path in import.", "Invalid path in import."),
    entry!(PARSER_PHRASE_INVALID, "Invalid expression.", "Invalid expression."),
    entry!(PARSER_SINGLE_OR, "Single '|' used as a binary operator outside a pin context.", "Single '|' used as a binary operator outside a pin context."),
    entry!(PARSER_PLUSMINUS, "'±' used as a binary operator outside a tolerance context.", "'±' used as a binary operator outside a tolerance context."),
    entry!(PARSER_TRANSPOSE_ON_LITERAL, "Transpose (') on a literal has no effect.", "Transpose (') on a literal has no effect."),
    entry!(PARSER_CARET_ON_LITERAL, "Caret (^) on a literal has no effect.", "Caret (^) on a literal has no effect."),
    entry!(PARSER_EMPTY_BODY, "Empty body — no clauses defined.", "Empty body — no clauses defined."),
    entry!(PARSER_EMPTY_PINS, "Empty pins declaration.", "Empty pins declaration."),
    entry!(AST_NODE_EMPTY, "AST node is null/empty where a value was expected.", "AST: Node is empty"),
    entry!(AST_UTF8_ERROR, "AST node contains invalid UTF-8 data.", "Invalid UTF-8 string"),
    entry!(AST_TYPE_MISMATCH, "AST node has an unexpected type.", "AST: Node type mismatch"),
    // ---- section ----
    entry!(NAME_IDS_NO_NODES, "IDS has no nodes.", "IDS has no nodes."),
    entry!(NAME_MISSING_SUBNODE, "Missing subnode in a name reference.", "Missing subnode in a name reference."),
    entry!(NAME_DECLARE_PARSE_FAILED, "Failed to parse a DECLARE node.", "Failed to parse DECLARE"),
    entry!(NAME_SQUARE_VECTOR_MISSING_SUBNODE, "Missing subnode for a square vector.", "Missing subnode for square vector"),
    entry!(NAME_RANGE_SIDE_FAILED, "Failed to process a side of a range.", "Failed to process {0} side of a range."),
    entry!(NAME_DEF_NOT_FOUND_LABEL_FALLBACK, "Definition not found; falling back to a label.", "CURLY_MN: '{0}' definition not found, using label fallback"),
    entry!(NAME_ID_EXTRACT_FAILED, "Failed to extract ID/IDA data from a node.", "Failed to extract ID/IDA data"),
    entry!(NOT_SUPPORTED_YET, "This syntax is parsed but not yet supported by the semantic layer; the declaration is ignored.", "pins.subcls = [...] is parsed but not supported yet; the sub-class name is ignored"),
    entry!(SYMBOL_NOT_FOUND, "Symbol could not be resolved to any definition after the full P1–P5 lookup chain.", "Cannot find '{0}'"),
    // ---- section ----
    entry!(PIN_ID_NAME_MISMATCH, "Pin ID and pin name do not match.", "Pin ID and name not match"),
    entry!(PIN_ID_COUNT_ERROR, "Pin id count error.", "Pin id count error"),
    entry!(PINS_PLUS_WITHOUT_BASE, "pins += is used without a prior pins = definition.", "pins += used without prior pins = definition"),
    entry!(PIN_NAME_TYPE_UNSUPPORTED, "Pin name has an unsupported type.", "Pin name not support type"),
    entry!(PIN_NAME_COUNT_ERROR, "Pin/port name count error.", "Pin name count error."),
    entry!(PORT_NAME_TYPE_UNSUPPORTED, "Port name has an unsupported type.", "Port name not support type"),
    entry!(PORT_NAME_COUNT_ERROR, "Port name count error.", "Port name count error"),
    entry!(PIN_EXPR_TYPE_MISMATCH, "Pin expression node has an unexpected type.", "Pin expression node has an unexpected type."),
    entry!(ATTR_TYPE_MISMATCH, "Attribute node type mismatch.", "Attribute node type mismatch."),
    entry!(ATTR_TYPE_NOT_SUPPORTED, "Attribute type is not supported.", "Attribute type not support (node_type={0})"),
    entry!(ATTR_MISSING_SUBNODE, "Attribute node is missing a required subnode.", "Attribute node is missing a required subnode."),
    entry!(KVS_VALUE_TYPE_INVALID, "Invalid value type in a KVS node.", "Invalid value type in KVS node."),
    entry!(UVAL_VALUE_TYPE_INVALID, "Invalid unit value type.", "Invalid unit value type."),
    entry!(UVAL_DATA_NODE_INVALID, "Invalid unit value data node.", "Invalid unit value data node."),
    entry!(UVAL_UNIT_INVALID, "Invalid unit.", "Invalid unit."),
    entry!(UVAL_VALUE_INVALID, "Invalid unit value.", "Invalid value."),
    entry!(UVAL_UNIT_UNSUPPORTED, "The unit is not supported.", "Unsupported unit '{0}'."),
    entry!(UVAL_MISSING_DATA_NODE, "Missing unit value data node.", "missing unit value data node."),
    entry!(UVAL_FORMAT_INVALID, "Invalid unit value or float format.", "Invalid unit value or float format."),
    entry!(UVAL_UNIT_VARIANT_INVALID, "Invalid unit variant (angle, charge, magnetic flux, slew rate, ...).", "Invalid unit variant '{0}'."),
    // ---- section ----
    entry!(MODULE_MISSING_SUBNODE, "Missing subnode in a module body clause.", "Missing subnode in a module body clause."),
    entry!(MODULE_PINS_UNSUPPORTED, "Module does not support PINS directly; use in/out/io declarations.", "Module does not support PINS directly. Use in/out/io declarations."),
    entry!(MODULE_ROLE_UNSUPPORTED, "Module does not support role definition.", "Module does not support role definition."),
    entry!(MODULE_PARAM_TYPE_UNEXPECTED, "Unexpected type in a module parameter.", "Unexpected type in module param"),
    entry!(MODULE_METHOD_NOT_FOUND, "Function was not found in the class.", "function '{0}' not found in class '{1}'"),
    entry!(UNEXPECTED_CLAUSE_TYPE, "Unexpected clause type in a module body.", "Unexpected clause type in module body"),
    // ---- section ----
    entry!(FUNC_EMPTY_NET, "Empty net in a function or module body.", "Empty NET"),
    entry!(PARAM_DECLARE_INVALID, "Invalid parameter declaration node.", "Invalid param declare node."),
    entry!(PARAM_NAME_INVALID, "Invalid parameter name.", "Invalid param name."),
    entry!(PARAM_SET_INVALID, "Invalid parameter set.", "Invalid parameter set."),
    entry!(PARAM_UVAL_INVALID, "Invalid parameter unit value.", "Invalid param uval."),
    entry!(PARAM_CLASS_EXPECTED, "Expected a class in the declaration unit value.", "Expected MCAST_CLASS in MCAST_DECLARE_UV."),
    entry!(PARAM_INSTANCE_EXPECTED, "Expected an instance in the declaration unit value.", "Expected MCAST_INSTANCE in MCAST_DECLARE_UV."),
    entry!(PARAM_NAME_EXTRACT_FAILED, "Failed to extract the parameter name.", "Failed to extract parameter name from MCAST_DECLARE"),
    entry!(PARAM_INST_LOOKUP_FAILED, "Instance::class lookup failed; the binding is treated as a plain pin alias.", "'{0}::{1}' lookup failed; treating '{0}' as plain pin alias. If you intended an interface binding, check that '{1}' is defined (and `use`d, if from a library)."),
    entry!(PARAM_DECLARE_IFACE_PINS, "Interface pin count does not match the number of declared pin IDs.", "Interface '{0}' declares {1} pin(s) (members: {2}) but {3} pin ID(s) given; the counts must match. Use a range like `a:b` to declare exactly {1} pin(s)."),
    entry!(FUNC_CALL_MISSING_NAME, "Missing function name in a function call.", "Missing function name in a function call."),
    entry!(CONN_LINE_PARSE_FAILED, "A connection line failed to parse.", "connection line failed to parse"),
    entry!(FUNC_BODY_INVALID, "Invalid function body node.", "Invalid function body node."),
    entry!(FUNC_LINE_DROPPED, "A connection line was dropped because McPhrase::new returned None.", "Connection line dropped (McPhrase::new returned None): `{0}`"),
    entry!(FCALL_PARSE_FAILED, "Function call parse failure.", "Cannot chain `.{0}` after `{1}(...)`: function `{2}` returns a bus/label (endpoint), not `this`. Only functions that return `this` can be chained."),
    // ---- section ----
    entry!(INST_EXPR_PARSE_FAILED, "Failed to parse an instance in an expression context.", "Failed to parse MCAST_INSTANCE in expression context"),
    entry!(CURLY_MN_WRONG_BASE, "Curly-member construction requires a component or module base.", "CURLY_MN requires Component or Module"),
    entry!(INST_CLASS_NODE_MISSING, "No class node found in the instance declaration.", "No class node found"),
    entry!(INST_NODE_MISSING, "No instance node found.", "No instance node found"),
    entry!(INST_CLASS_ID_MISSING, "Missing class id node.", "Missing class id node"),
    entry!(INST_CLASS_IDS_PARSE_FAILED, "Failed to parse class ids.", "Failed to parse class ids"),
    entry!(INST_CLASS_UNRESOLVED, "Unresolved class — the library may not be loaded.", "unresolved class '{0}' — library not loaded?"),
    entry!(FUNC_RETURN_MALFORMED, "Malformed return statement.", "Malformed return statement."),
    entry!(FUNC_RETURN_EXPR_INVALID, "Invalid return expression — expected this or a label/bus.", "Invalid `return` expression: expected `this` or a label/bus."),
    entry!(FUNC_MULTIPLE_RETURNS, "A function may have at most one return statement.", "Multiple `return` statements are not allowed; a function may have at most one return."),
    entry!(IFACE_MEMBER_NOT_FOUND, "Interface member not found in the component.", "Interface '{0}.{1}' not found in component '{2}'"),
    entry!(IFACE_CURLY_MEMBER_INVALID, "Cannot access interface members using curly-bracket syntax.", "Component '{0}' not found for interface '{1}.{2}'"),
    entry!(IFACE_COMPONENT_NOT_FOUND, "Component not found for the interface reference.", "Cannot access members on interface '{0}' using curly bracket syntax"),
    entry!(IFACE_BUS_NOT_FOUND, "Interface not found for a bus reference.", "Interface '{0}' not found for bus '{1}[{2}]'"),
    entry!(MODULE_PORT_NOT_FOUND, "Port(s) not found in the module.", "Port(s) '{0}' not found in module '{1}'. Available ports: [{2}]"),
    entry!(BUS_NAME_ALREADY_INSTANCE, "Name is already an instance; cannot create a bus with these members.", "Name '{0}' is already an instance, cannot create bus with members [{1}]"),
    entry!(IFACE_PIN_NOT_FOUND, "Pin(s) not found in the interface.", "Pin(s) '{0}' not found in interface '{1}'. Available pins: [{2}]"),
    entry!(IFACE_MEMBER_LOOKUP_FAILED, "Interface member lookup failed.", "Interface '{0}' not found (looked up from '{1}'); check that it is defined and imported via `use`."),
    entry!(COMPONENT_PIN_NOT_FOUND, "Pin(s) not found in the component or interface.", "Pin(s) '{0}' not found in component '{1}'. Available pins: [{2}]"),
    entry!(IFACE_NO_TOPLEVEL_PINS, "Interface has no top-level pin definitions (all pins are inside role blocks); no pin-to-member mapping is created.", "Interface '{0}' has no top-level pins (all pins are inside `role` blocks, e.g. UART.X); no pin-to-member mapping will be created. If you want the role-specific pins registered, list them explicitly (e.g. `pins = TX, RX, GND`)."),
    // ---- section ----
    entry!(CONN_TRANSPOSE_SIZE_MISMATCH, "Transposed connection size mismatch.", "Transposed connection size mismatch"),
    entry!(CONN_LEFT_ARROW_SHAPE_MISMATCH, "Shape mismatch in a <- connection.", "Shape mismatch in a <- connection"),
    entry!(CONN_CANNOT_TRANSPOSE, "The transpose operator is not allowed at this position.", "Cannot transpose"),
    entry!(CONN_PARALLEL_INVALID, "An instance with 3+ pins cannot directly participate in a '+' operation.", "Instance with 3+ pins cannot directly participate in `+` operation. Use `->` for pass-through connection or `::` for type annotation."),
    entry!(CONN_PARALLEL_SHAPE_MISMATCH, "Shape mismatch in a parallel connection.", "Shape mismatch in parallel connection"),
    entry!(CONN_SERIES_INVALID, "An instance with 3+ pins cannot directly participate in a '-' operation.", "Instance with 3+ pins cannot directly participate in `-` operation. Use `->` for pass-through connection."),
    entry!(CONN_SERIES_SHAPE_MISMATCH, "Shape mismatch in a -> connection.", "Shape mismatch in -> connection"),
    entry!(CONN_OPERATOR_UNSUPPORTED, "The operator is not supported in connection lines; use '+' for parallel, '-' / '->' for series.", "node={0} Operator '{1}' is not supported in connection lines; use '+' for parallel, '-' / '->' for series"),
    entry!(PHRASE_AST_TYPE_UNEXPECTED, "Unexpected AST node type in a phrase.", "node={0} Unexpected AST node type {1} in McPhrase::new"),
    entry!(CONN_NO_PORTS_COMPONENT, "No ports found in the component.", "No ports found in the component."),
    entry!(CONN_NO_PORTS_MODULE, "No ports found in the module.", "No ports found in the module."),
    entry!(CONN_NO_PORTS_INTERFACE, "No ports found in the interface.", "No ports found in the interface."),
    entry!(CONN_DOT_SERIES, "Dot operator does not apply to a Series.", "Dot operator does not apply to a Series."),
    entry!(CONN_DOT_NODE, "Dot operator does not apply to a Node.", "Dot operator does not apply to a Node."),
    entry!(CONN_DOT_TRANSPOSED, "Dot operator does not apply to a Transposed.", "Dot operator does not apply to a Transposed."),
    entry!(CONN_DOT_LEAD, "Dot operator does not apply to a Lead.", "Dot operator does not apply to a Lead."),
    entry!(CONN_DOT_GROUP, "Dot operator does not apply to a Group.", "Dot operator does not apply to a Group."),
    entry!(CONN_DOT_ENDPOINT, "Dot operator does not apply to an Endpoint.", "Dot operator does not apply to an Endpoint."),
    entry!(CONN_DOT_MEMBER, "Dot operator already applied to a Member.", "Dot operator already applied to a Member."),
    entry!(CLOSURE_NO_OUTPUT_IFACE, "Closure has no output interface to access.", "Closure has no output interface to access."),
    entry!(FUNCCALL_NO_RETURN_IFACE, "FuncCall has no return interface to access.", "FuncCall has no return interface to access."),
    entry!(PHRASE_IFACE_MEMBER_NOT_FOUND, "Member not found in the interface.", "Member '{0}' not found in interface"),
    entry!(CURLY_LEFT_EMPTY, "Curly-member construction: left member list is empty.", "Curly-member construction: left member list is empty."),
    entry!(CURLY_RIGHT_EMPTY, "Curly-member construction: right member list is empty.", "Curly-member construction: right member list is empty."),
    entry!(CURLY_NOT_BUS, "Cannot convert the curly-member result to a bus.", "Cannot convert the curly-member result to a bus."),
    entry!(GROUP_BRANCH_COUNT_MISMATCH, "Groups with different branch counts cannot connect.", "Groups with different branch counts cannot connect."),
    // ---- section ----
    entry!(GHOST_PORT_BOX, "A box has a placeholder pin not mapped to any real component pin.", "GHOST_PORT: box '{0}' (id={1}) has placeholder pin '{2}' (id={3}) that is not mapped to any real component pin. The component declared only an estimated pin count (pins = N) without actual pin definitions."),
    entry!(NET_MERGED_SHORT, "Multiple points resolve to the same node — possible short circuit (E2003).", "MERGED_SHORT: net '{0}' (module '{1}') has {2} point(s) resolving to the same node (id={3}). Paths: {4}. This may indicate a bracket expansion duplicate or a port declared without bit width causing signal merging."),
    entry!(NET_BUS_ORDER_MISMATCH, "Bus member order mismatch after sorting (E2005).", "Bus member order mismatch after sorting (E2005)."),
    entry!(SORT_HAZARD, "Bus pin numbers are non-monotonic; member→pin mapping may be wrong after sorting.", "SORT_HAZARD: pin numbers in component '{0}' bus '{1}' are non-monotonic. Member→pin binding: [{2}]. Pin declaration order differs from member order, which may cause incorrect mapping after sorting."),
    entry!(FLOATING_PLACEHOLDER, "A '_' placeholder could not be bound to any pin.", "FLOATING_PLACEHOLDER: '_' placeholder in net '{0}' (module '{1}') could not be bound to any existing pin. The placeholder is floating."),
    entry!(LEAD_PREFIX_ID_AS_WIRE, "'_X' is a prefix identifier (member name), not the wire '_'.", "PREFIX_ID_AS_WIRE: '{0}' is a prefix identifier (member name) like '_OPEN', not the wire '_'. If you meant pass-through in a connection line, write '_' instead."),
    entry!(FUNC_PARAM_SHADOWS_PIN, "Func param shadows a same-named component pin in function body expansion; the param takes priority.", "FUNC_PARAM_SHADOWS_PIN: func param '{0}' shadows pin of component '{1}' in function body expansion - param takes priority."),
    entry!(GHOST_PORT, "A net endpoint is not mapped to any box — possible unexposed module boundary port.", "GHOST_PORT: net '{0}' endpoint id={1} is not mapped to any box. This pin may cross a module boundary without being properly exposed as a port."),
    entry!(PULLUP_DEGENERATE, "Pullup/pulldown degenerated into a signal-signal bridge.", "PULLUP_DEGENERATE: '{0}' both ends are non-rail nets ({1} ~ {2}). Pullup/Pulldown may have degenerated into a signal-signal bridge instead of (signal, rail)."),
    entry!(NET_DROPPED_STATEMENT, "A single-element square bracket expands to an unknown instance; the statement may produce no nets or constraints.", "DROPPED_STATEMENT: indexed alias '{0}' expands to '{1}' which is not a known instance. The statement may produce no nets or constraints."),
    entry!(LAYOUT_MISSING_SUBNODE, "Layout attribute is missing a required subnode.", "Layout attribute is missing a required subnode."),
    entry!(LAYOUT_TYPE_MISMATCH, "Layout attribute node type mismatch.", "Layout attribute node type mismatch."),
    entry!(LAYOUT_SET_MISSING_SUBNODE, "Layout set is missing a required subnode.", "Layout set is missing a required subnode."),
    entry!(LAYOUT_VALUES_TYPE_MISMATCH, "Layout values node type mismatch.", "Layout values node type mismatch."),
    entry!(LAYOUT_NAME_MISSING_SUBNODE, "Layout name is missing a required subnode.", "Layout name is missing a required subnode."),
    entry!(LAYOUT_EDGE_MISSING_SUBNODE, "Layout edge is missing a subnode.", "While building layout: Missing subnode for edge"),
    entry!(LAYOUT_EDGE_TYPE_MISMATCH, "Layout edge node type mismatch.", "Type mismatch in layout edge"),
    entry!(LAYOUT_EDGE_NAME_MISSING_SUBNODE, "Layout edge name is missing a subnode.", "Missing subnode for layout edge name"),
    entry!(LAYOUT_VALUE_MISSING_SUBNODE, "Layout value is missing a subnode.", "Missing subnode for layout value"),
    entry!(LAYOUT_VALUE_TYPE_MISMATCH, "Layout value node type mismatch.", "Type mismatch in layout value"),
    entry!(LAYOUT_SET_SUBNODE_MISSING, "Layout set is missing a subnode.", "Missing subnode for layout set"),
    entry!(LAYOUT_EXTRA_NODES, "Malformed layout: unexpected extra nodes.", "Malformed layout: unexpected extra nodes"),
    entry!(LAYOUT_VALUES_MISSING_SUBNODE, "Layout values are missing a subnode.", "Missing subnode for layout values"),
    entry!(LAYOUT_CONST_MISSING_INT, "CONST node is missing its INT subnode.", "CONST node missing subnode INT"),
    entry!(LAYOUT_PIN_NUMBER_PARSE, "Parse error in a layout pin number.", "Parse error in layout pin number"),
    entry!(LAYOUT_EDGE_NAME_ID_MISSING_SUBNODE, "Layout edge name id is missing a subnode.", "Missing subnode for layout edge name id"),
    entry!(LAYOUT_EDGE_INVALID, "Invalid layout edge.", "Invalid edge. Edges should be one of: \"left\", \"right\", \"top\", \"bottom\""),
    entry!(LAYOUT_EDGE_NAME_NOT_ID, "Malformed layout: edge name is not an ID.", "Malformed layout: edge name not an ID"),
    // ---- section ----
    entry!(NET_MULTI_DRIVE, "Net has multiple drivers — possible short circuit.", "Net has multiple drivers — possible short circuit."),
    entry!(IFACE_PINS_NOT_ALL_BOUND, "Interface requires more pins than are bound to physical pins.", "Interface requires more pins than are bound to physical pins."),
    entry!(NET_NO_DRIVER, "Net has inputs but no output/power driver.", "Net has inputs but no output/power driver."),
    entry!(IFACE_ROLE_NOT_FOUND, "Interface role referenced by a param does not exist in the interface.", "Interface role referenced by a param does not exist in the interface."),
    entry!(NET_VOLTAGE_MISMATCH, "Power nets with different voltages are shorted together.", "Power nets with different voltages are shorted together."),
    entry!(IFACE_NOT_LOADED, "Interface referenced by a param is not loaded.", "Interface referenced by a param is not loaded."),
    entry!(IFACE_DEPRECATED_CMIE, "Deprecated interface/component/param used.", "Deprecated interface/component/param used."),
    entry!(NET_INPUT_UNCONNECTED, "An input port is not connected to any net.", "An input port is not connected to any net."),
    entry!(NET_NC_CONNECTED, "An NC port is connected to a net.", "An NC port is connected to a net."),
    entry!(NET_OUTPUT_UNDRIVEN, "An output drives nothing.", "An output drives nothing."),
    entry!(NET_BACKFEED_RISK, "Net has both an output and a power supply — backfeed risk.", "Net has both an output and a power supply — backfeed risk."),
    entry!(NET_INSTANCE_UNCONNECTED, "Instance has no pins connected to any net.", "Instance has no pins connected to any net."),
    entry!(NET_OUTPUTS_NO_INPUT, "Net has outputs and power but no input.", "Net has outputs and power but no input."),
    entry!(NET_MODULE_PORT_UNCONNECTED, "Module port is not connected to any net.", "Module port is not connected to any net."),
    entry!(NET_DANGLING_ENDPOINT, "Net has only one endpoint — possible dangling connection.", "Net has only one endpoint — possible dangling connection."),
    entry!(NET_PARTIAL_CONNECTION, "Only some of the instance pins are connected.", "Only some of the instance pins are connected."),
    entry!(NET_BIDIR_UNCONNECTED, "A bidirectional port is not connected to any net.", "A bidirectional port is not connected to any net."),
    entry!(NET_POWER_NET_COUNT, "Design has many power nets; review for consolidation.", "Design has many power nets; review for consolidation."),
    // ---- section ----
    entry!(INST_CHAIN_LINK_SKIPPED, "A chain link was skipped because the method is not defined on the instance.", "Method '{0}' not defined in {1} '{2}'; chain link skipped, no body expanded."),
    entry!(INST_ARG_NO_FORMAL_PORT, "Instance argument has no formal port to bind.", "Instance '{0}' arg{1} '{2}' has no formal port to bind"),
    entry!(INST_METHOD_FALLBACK, "Instance method could not be resolved; passed through instead.", "Unrecognized function call '{0}' in module '{1}' — treated as pass-through (class not loaded or name misspelled)"),
    entry!(INST_IFACE_INSTANTIATE_FAILED, "Interface instantiation failed.", "Interface instantiation failed: {0}"),
    entry!(INST_SUBMODULE_INSTANTIATE_FAILED, "Sub-module instantiation failed.", "Sub-module '{0}' instantiation failed: {1}"),
    entry!(INST_LINE_SKIP_FAILED_CLASS, "Line references a component class whose instantiation failed; the whole line is skipped.", "Line references a component class whose instantiation failed; skipping entire line."),
    entry!(INST_LINE_PARSE_FAILED, "A connection line failed to expand.", "Connection line #{0} failed: {1}"),
    entry!(INST_BUILTIN_TWOPIN_EXPAND_FAILED, "Expanded builtin two-pin pair failed.", "Expanded builtin twopin pair failed: {0}"),
    entry!(INST_MEMBER_PROCESS_FAILED, "A member of a connection line failed to process.", "Member processing failed: {0}"),
    entry!(INST_ADJACENT_CONNECT_FAILED, "Connection between adjacent members of a series failed.", "Connection between members #{0} and #{1} failed: {2}"),
    entry!(INST_SHUNT_PROCESS_FAILED, "A `.Cap(_)` shunt member failed to process.", "`.Cap(_)` shunt: {0}"),
    entry!(INST_FUNC_BODY_LINE_FAILED, "A module-level function body line failed.", "Module-level function '{0}' body line failed: {1}"),
    entry!(INST_LANE_FUNCCALL_FAILED, "Failed to instantiate a FuncCall during lane-by-lane wiring.", "Failed to instantiate FuncCall in lane-by-lane wiring: {0}"),
    entry!(INST_LANE_TRANSPOSED_FAILED, "Failed to instantiate a Transposed member during lane-by-lane wiring.", "Failed to instantiate Transposed in lane-by-lane: {0}"),
    entry!(CONN_SHAPE_MISMATCH_TRUNCATED, "Connection shape mismatch; truncated to the smaller side.", "Shape mismatch: left={0}, right={1}, truncating to min({2})"),
    entry!(CONN_SHAPE_ROW_MISMATCH_RECOVERED, "Vector shape row mismatch (eval.md §3) recovered by truncation.", "Vector shape mismatch: left {0} vs right {1}, truncating to min({2})"),
    entry!(SHAPE_TRANSPOSE_LIMIT, "Transpose operand must be 1*1 / 1*2 / 2*1 / 2*2 (eval.md §5.5).", "Transpose operand has {0} rows; only 1*1, 1*2, 2*1 or 2*2 shapes can be transposed."),
    entry!(SHAPE_REVERSE_NOOP, "Reverse `^` has no effect on a vector (parallel / transposed) operand (eval.md §9).", "Reverse `^` on '{0}' has no effect: a vector operand (parallel or transposed) carries no order to reverse."),
    entry!(SHAPE_EXPAND_DIM_MISMATCH, "Vector expansion dimension mismatch (eval.md §7 rule 3); implicit auto-expansion forbidden.", "Vector expansion dimension mismatch: left {0} rows vs right {1} rows. {2}"),
    entry!(SHAPE_INST_3PIN_PLUSMINUS, "Instance with 3+ pins cannot directly participate in `+`/`-`; only 1x1/1x2 instances can (veccircuit.md).", "Instance '{0}' with {1} pins cannot directly participate in `+`/`-`. Use `->` for a pass-through connection."),
    entry!(SHAPE_INCOMPLETE, "NetShape missing; fell back to the deprecated connection_type() inference (stage 3).", "SHAPE_INCOMPLETE: net '{0}' has no NetShape provenance; fell back to connection_type() inference."),
    entry!(CONN_PARALLEL_DIM_MISMATCH, "Parallel '+' operand dimension mismatch; operand merged into the anchor's left net.", "Parallel '+' operand dimension mismatch (anchor={0}x1, opd[{1}]={2}x1 left / {3}x1 right): merging operand into anchor's left net."),
    entry!(CONN_GROUP_SHAPE_MISMATCH, "Group connection shape mismatch; truncated by branch count.", "Group shape mismatch: {0} external points vs {1} group points ({2} branches), truncating"),
    entry!(INST_INPUT_PIN_COUNT_MISMATCH, "Component input pin count mismatch in a function call.", "Component '{0}' ({1}) input pin count mismatch: {2} connections vs {3} input pins"),
    entry!(INST_OUTPUT_PIN_COUNT_MISMATCH, "Component output pin count mismatch in a function call.", "Component '{0}' ({1}) output pin count mismatch: {2} connections vs {3} output pins"),
    entry!(INST_INLINE_MODULE_FAILED, "Inline module instantiation failed.", "Inline module '{0}' ({1}) instantiation failed: {2}"),
    entry!(INST_INPUT_PORT_COUNT_MISMATCH, "Module input port count mismatch in a function call.", "Module '{0}' ({1}) input port count mismatch: {2} connections vs {3} input ports"),
    entry!(INST_OUTPUT_PORT_COUNT_MISMATCH, "Module output port count mismatch in a function call.", "Module '{0}' ({1}) output port count mismatch: {2} connections vs {3} output ports"),
    entry!(INST_POWER_PORT_UNBOUND, "Sub-module DC power port is never connected (missing power argument?).", "Sub-module instance '{0}' DC power port '{1}' is never connected (missing power argument?)"),
    entry!(INST_CTOR_BODY_LINE_FAILED, "A constructor function body line failed.", "Constructor '{0}' body line failed: {1}"),
    entry!(INST_CTOR_PARAM_BIND_FAILED, "Constructor parameter binding failed.", "Constructor '{0}' on '{1}' param bind: {2}"),
    entry!(INST_ARG_UNBOUND_DETAILED, "Instance argument has no formal port to bind (with module/bound details).", "Instance '{0}' arg '{1}' has no formal port to bind (module='{2}', {3}/{4} formal ports already bound)"),
    // ---- section ----
    entry!(DUP_CMIE_CROSS_FILE, "Same name defined in another file (cross-file duplicate).", "Same name defined in another file (cross-file duplicate)."),
    entry!(DUP_WITHIN, "Duplicate definition within the same declaration.", "Duplicate definition within the same declaration."),
    entry!(DUP_ENUM_VALUE, "Enum value appears more than once in the enum.", "Enum value appears more than once in the enum."),
    // ---- section ----
    entry!(NAME_COMPONENT_LOWERCASE, "Component name starts with lowercase; convention is UPPER_SNAKE.", "Component name starts with lowercase; convention is UPPER_SNAKE."),
    entry!(NAME_PORT_SHADOWS_CMIE, "Port name shadows a library CMIE name.", "Port name shadows a library CMIE name."),
    entry!(NAME_PIN_MIXED_CONVENTION, "Pins use mixed naming conventions.", "Pins use mixed naming conventions."),
    entry!(NAME_INSTANCE_SINGLE_CHAR, "Instance name is a single character.", "Instance name is a single character."),
    entry!(NAME_PIN_NUMERIC, "Pin name is purely numeric.", "Pin name is purely numeric."),
    entry!(NAME_PORT_INST_SHADOWS_CMIE, "Port/instance name shadows a library CMIE name.", "Port/instance name shadows a library CMIE name."),
    entry!(NAME_PARAM_SHADOWS_CMIE, "Parameter name shadows a library CMIE name.", "Parameter name shadows a library CMIE name."),
    // ---- section ----
    entry!(SPEC_KEY_UNDECLARED_PARAM, "Spec key references a parameter that is not declared.", "Spec key references a parameter that is not declared."),
    entry!(REF_INTEGRITY, "Reference integrity violation.", "Reference integrity violation."),
    entry!(FUNC_PARAMS_NO_BODY, "Function has parameters but no body (empty implementation).", "Function has parameters but no body (empty implementation)."),
    entry!(EXPR_PINS_X_UNDEFINED, "pins.X references an undefined pin name.", "pins.X references an undefined pin name."),
    // ---- section ----
    entry!(INST_DECLARED_MULTIPLE, "Instance is declared more than once in the module.", "Instance is declared more than once in the module."),
    entry!(PORT_DUPLICATE_NAME, "Duplicate port name in the module — ambiguous.", "Duplicate port name in the module — ambiguous."),
    entry!(NOT_AN_INTERFACE, "The class is a component/module/enum, not an interface.", "'{0}' is a component/module/enum, not an interface."),
    entry!(NAME_PARAM_AND_INSTANCE, "Name is both a value parameter and an instance.", "Name is both a value parameter and an instance."),
    entry!(PIN_UNCONNECTED, "Pin is not connected to any net.", "Pin is not connected to any net."),
    entry!(PIN_CONFLICTING_OPTIONS, "Pin uses conflicting option names.", "Pin uses conflicting option names."),
    entry!(FUNC_RETURN_OUTSIDE_FUNCTION, "Return statement used outside a function.", "Return statement used outside a function."),
    entry!(FUNC_RETURN_LITERAL_INVALID, "Return statement specifies a literal instead of an endpoint.", "Return statement specifies a literal instead of an endpoint."),
    entry!(INST_EMPTY_TABLE, "Empty instance table in a [] :: TYPE declaration.", "Empty instance table in a [] :: TYPE declaration."),
    entry!(INST_THIS_TYPE, "this :: TYPE declaration is not allowed.", "this :: TYPE declaration is not allowed."),
    entry!(FUNC_ROLE_AS_ARG, "Role used as a function-call argument.", "Role used as a function-call argument."),
    entry!(MODULE_PORT_UNUSED, "Module port is declared but never connected.", "Module port is declared but never connected."),
    entry!(COND_SINGLE_BINARY, "Condition compares against a single binary value.", "Condition compares against a single binary value."),
    // ---- section ----
    entry!(ENUM_SINGLE_VALUE, "Enum has only one value.", "Enum has only one value."),
    entry!(PARAM_INT_DEFAULT_STRING, "Integer param has a string default.", "Integer param has a string default."),
    entry!(PARAM_STRING_DEFAULT_NUMERIC, "String param has a numeric-looking default.", "String param has a numeric-looking default."),
    entry!(PARAM_UV_DEFAULT_NO_UNIT, "Unit-value param default has no unit suffix (e.g. '5V').", "Unit-value param default has no unit suffix (e.g. '5V')."),
    entry!(PARAM_FLOAT_DEFAULT_INVALID, "Param has an invalid float default.", "Param has an invalid float default."),
    entry!(PARAM_NEGATIVE_DEFAULT, "Integer param default is negative.", "Integer param default is negative."),
    // ---- section ----
    entry!(PARAM_RESERVED_KEYWORD, "Parameter uses a reserved keyword.", "Parameter uses a reserved keyword."),
    entry!(FUNC_EMPTY_BODY, "Function has an empty body.", "Function has an empty body."),
    entry!(COMPONENT_EMPTY, "Component has no params, pins, attributes, or functions.", "Component has no params, pins, attributes, or functions."),
    entry!(COMPONENT_NO_PINS, "Component has no pin definitions.", "Component has no pin definitions."),
    entry!(INTERFACE_EMPTY, "Interface has no pins or roles.", "Interface has no pins or roles."),
    entry!(INST_CLASS_NOT_LOADED, "Instance references a class that is not loaded.", "Instance references a class that is not loaded."),
    entry!(COMPONENT_MIXED_CASE, "Component name uses mixed case; convention is UPPER_SNAKE.", "Component name uses mixed case; convention is UPPER_SNAKE."),
    entry!(BUS_DUPLICATE_MEMBER, "Bus has a duplicate member.", "Bus has a duplicate member."),
    entry!(COMPONENT_DUPLICATE_FUNC_BODY, "Component has functions with the same body.", "Component has functions with the same body."),
    entry!(DEFINE_NO_ATTRS, "Define has no attributes.", "Define has no attributes."),
    entry!(DEFINE_NON_ATTR_CLAUSE, "Define contains a non-attribute clause.", "Define contains a non-attribute clause."),
    entry!(IFACE_PIN_COUNT_MISMATCH, "Interface expects more pins than are bound.", "Interface expects more pins than are bound."),
    entry!(FUNC_SHARES_NAME_WITH_PORT, "Function shares its name with a port/param.", "Function shares its name with a port/param."),
    entry!(NET_BOTH_OUTPUTS, "Net connects two outputs.", "Net connects two outputs."),
    entry!(FUNC_INLINE_BODY_LITERAL_ARG, "Inline function body literal used as a call argument.", "Inline function body literal used as a call argument."),
    entry!(FUNC_PARAMS_UNUSED, "Function declares parameters it never uses.", "Function declares parameters it never uses."),
    entry!(SPEC_KEY_DUPLICATE, "Spec key appears more than once.", "Spec key appears more than once."),
    // ---- section ----
    entry!(DEF_AMBIGUOUS_NAME, "Same name used for different definition kinds.", "Same name used for different definition kinds."),
    entry!(DEF_REF_NOT_LOADED, "Definition references a class that is not loaded.", "Definition references a class that is not loaded."),
    entry!(COMPONENT_INT_SUFFIX, "Component has an unconventional '.int' suffix.", "Component has an unconventional '.int' suffix."),
    entry!(ENUM_INT_SUFFIX, "Enum has an unconventional '.int' suffix.", "Enum has an unconventional '.int' suffix."),
    // ---- section ----
    entry!(ATTR_RESERVED_KEYWORD, "Attribute uses a reserved keyword.", "Attribute uses a reserved keyword."),
    entry!(INST_ARG_COUNT_MISMATCH, "Instance passes more/fewer args than the class declares.", "Instance passes more/fewer args than the class declares."),
    entry!(ROLE_EMPTY_BODY, "Role has an empty body.", "Role has an empty body."),
    entry!(ROLE_NAME_SHADOWS, "Role shares its name with a parameter or pin/port.", "Role shares its name with a parameter or pin/port."),
    entry!(ATTR_NESTING_TOO_DEEP, "Attribute nesting depth exceeds 16.", "Attribute nesting depth exceeds 16."),
    entry!(ATTR_PIN_GROUP_UNDEFINED, "Attribute references an undefined pin group, or role used outside a component.", "Attribute references an undefined pin group, or role used outside a component."),
    entry!(PINS_PLUS_AND_PINS_CONFLICT, "Component mixes pins = and pins.X = attributes, or uses a non-constant default.", "Component mixes pins = and pins.X = attributes, or uses a non-constant default."),
    // ---- section ----
    entry!(ENUM_DUPLICATE_VALUE, "Enum has a duplicate value.", "Enum has a duplicate value."),
    entry!(ENUM_MEMBER_DOT, "Enum member contains a dot.", "Enum member contains a dot."),
    entry!(ENUM_MEMBER_LEADING_DIGIT, "Enum member starts with a digit.", "Enum member starts with a digit."),
    entry!(ENUM_MEMBER_RESERVED, "Enum member is a reserved keyword.", "Enum member is a reserved keyword."),
    entry!(ATTR_INFINITE_FLOAT, "Attribute has an infinite float value.", "Attribute has an infinite float value."),
    entry!(ATTR_LARGE_INT, "Attribute has a suspiciously large integer value.", "Attribute has a suspiciously large integer value."),
    entry!(RANGE_REVERSED, "Range appears reversed; did you mean the opposite order?", "Range appears reversed; did you mean the opposite order?"),
    entry!(RANGE_SINGLE_ELEMENT, "Range expands to a single element.", "Range expands to a single element."),
    entry!(IDX_MULTIPLE_SLICE_SPEC, "IDX key has multiple slice specifications.", "IDX key has multiple slice specifications."),
    entry!(EXPR_THIS_TOP_LEVEL, "'this' used in a top-level net line; it is only valid inside instance/function contexts.", "'this' used in a top-level net line; it is only valid inside instance/function contexts."),
    entry!(EXPR_PLACEHOLDER_ONLY, "Net connects only to '_' placeholder; the connection has no effect.", "Net connects only to '_' placeholder; the connection has no effect."),
    entry!(ATTR_SELF_REFERENTIAL, "Attribute value equals its own key; likely a copy-paste mistake.", "Attribute value equals its own key; likely a copy-paste mistake."),
    // ---- section ----
    entry!(COND_EMPTY_BODY, "Conditional block has an empty body.", "Conditional block has an empty body."),
    entry!(COND_IF_WITHOUT_ELSE, "if without a matching else.", "if without a matching else."),
    entry!(PIN_NC_COMPONENT_LEVEL, "NC pin used at component level.", "NC pin used at component level."),
    entry!(POWER_PIN_NO_VOLTAGE, "Power pin has no voltage attribute.", "Power pin has no voltage attribute."),
    entry!(PIN_IO_MIX_IN_OUT, "Pin mixes In and Out IO types.", "Pin mixes In and Out IO types."),
    entry!(PIN_IO_MIX_OUTPUT_POWER, "Pin mixes Output and Power IO types.", "Pin mixes Output and Power IO types."),
    entry!(PIN_IO_MIX_ANALOG_POWER, "Pin mixes Analog and Power IO types.", "Pin mixes Analog and Power IO types."),
    entry!(PARAM_PIN_NAME_SHADOW, "Parameter shares its name with a pin.", "Parameter shares its name with a pin."),
    entry!(MODULE_STUB, "Module is a stub.", "Module is a stub."),
    // ---- section ----
    entry!(HW_POWER_PINS_EXCESS, "Too many power pins.", "Too many power pins."),
    entry!(HW_PIN_NUMBER_GAP, "Pin numbers have gaps.", "Pin numbers have gaps."),
    entry!(HW_PIN_COUNT_HIGH, "Pin count is unusually high.", "Pin count is unusually high."),
    entry!(HW_ZERO_PINS_WITH_PARAMS, "Component has zero pins but parameter attributes.", "Component has zero pins but parameter attributes."),
    entry!(HW_NC_PINS_CONTIGUOUS, "Consecutive NC pins.", "Consecutive NC pins."),
    entry!(HW_IFACE_ROLE_UNBOUND, "Interface role is never bound.", "Interface role is never bound."),
    entry!(HW_ALL_SAME_IO_TYPE, "All pins have the same IO type.", "All pins have the same IO type."),
    entry!(HW_MISSING_NAME_ATTR, "Missing 'name' attribute.", "Missing 'name' attribute."),
    entry!(HW_NAME_WITHOUT_DESC, "Has a name but no description.", "Has a name but no description."),
    entry!(HW_FUNC_PARAM_SHADOWS_PIN, "Function parameter shadows a pin name.", "Function parameter shadows a pin name."),
    entry!(HW_IFACE_NEVER_BOUND, "Interface is defined but never bound.", "Interface is defined but never bound."),
    // ---- section ----
    entry!(TYPE_CLOSURE_FREE_VAR, "Closure references a free variable that is not declared.", "Closure references a free variable that is not declared."),
    entry!(TYPE_INCOMPATIBLE, "Incompatible types or unit types.", "Incompatible types or unit types."),
    // ---- section ----
    entry!(UNUSED_PARAM_OR_PORT, "Parameter or port is declared but never used.", "Parameter or port is declared but never used."),
    entry!(PORT_NEVER_USED, "Port is declared but never used in any net connection.", "Port '{0}' in '{1}' is declared but never used in any net connection."),
    entry!(UNTYPED_PARAM, "Parameter has no inferred type.", "Parameter has no inferred type."),
    // ---- section ----
    entry!(ERC_SINGLE_POINT_NET, "Single-point net: only one connection.", "single-point net: '{0}' has only one connection"),
    entry!(ERC_UNCONNECTED_PORT, "Unconnected port: not connected to any net.", "unconnected port: '{0}' is not connected to any net"),
    entry!(ERC_MULTI_DRIVE_NET, "Multi-drive net.", "multi-drive net: '{0}' has {1} drivers ({2})"),
    entry!(ERC_FLOATING_NET, "Floating net.", "floating net: '{0}' has no driver"),
];
