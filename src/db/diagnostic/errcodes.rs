// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Error code catalog — SINGLE SOURCE OF TRUTH.
//!
//! Every diagnostic code emitted by mcc must be declared here, with a symbolic
//! constant, a name, a description, and a message template. The message
//! template is the canonical emission text (with `{0}`, `{1}`… placeholders);
//! emission points render it via [`format_msg()`]. Maintain this file by
//! hand: every code needs a `pub const` and an `ALL_CODES` entry below.
//!
//! Numbering follows `mcc-error-code-unification-plan.md` §3.2:
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
//! 2. Add a matching `entry!()` row in the `ALL_CODES` table with the
//!    canonical emission message template (`{0}`, `{1}`, ... placeholders).

// Infrastructure

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

// Pass1a: duplicate definitions (1000-1049)

/// An interface with the same name already exists in this file.
pub const DUP_INTERFACE: u32 = 1001;

/// A component with the same name already exists in this file.
pub const DUP_COMPONENT: u32 = 1002;

/// An enum with the same name already exists in this file.
pub const DUP_ENUM: u32 = 1003;

/// A module with the same name already exists in this file.
pub const DUP_MODULE: u32 = 1004;

/// A recipe with the same name already exists in this file.
pub const DUP_RECIPE: u32 = 1006;

// Pass1a: definition structure / CMIE load (1050-1099)

/// Definition already exists.
pub const DEF_ALREADY_EXISTS: u32 = 1051;

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

// Pass1a: phrase member resolution (1100-1199)
//
// Traces emitted by `mc_phrase.rs`'s `dot_or_curly` / `curly_mn` when a member
// access cannot be resolved. Each must be a registered code, not a bare numeric
// literal: `format_msg` resolves the code through the registry, so an
// unregistered one returns an empty string and renders the trace blank.

/// `dot_or_curly` on a component operand: none of the requested members
/// matched a pin, so the access yields nothing.
pub const PHRASE_COMPONENT_MEMBER_NOT_FOUND: u32 = 1162;

/// `dot_or_curly` on a module operand: none of the requested members matched a
/// port, so the access yields nothing.
pub const PHRASE_MODULE_MEMBER_NOT_FOUND: u32 = 1163;

/// `dot_or_curly` on an interface operand: none of the requested members
/// matched a pin, so the access yields nothing.
pub const PHRASE_INTERFACE_MEMBER_NOT_FOUND: u32 = 1164;

/// `dot_or_curly` on a chain (`Series`) operand: a chain has no member list to
/// index, so the access is unsupported.
pub const PHRASE_MEMBER_ON_SERIES: u32 = 1168;

/// `dot_or_curly` on a node operand: a node exposes faces, not named members,
/// so the access is unsupported.
pub const PHRASE_MEMBER_ON_NODE: u32 = 1169;

/// `dot_or_curly` on a transposed (`'`) operand: transpose is a view over a
/// chain, so the access is unsupported.
pub const PHRASE_MEMBER_ON_TRANSPOSED: u32 = 1170;

/// `dot_or_curly` on a `_` lead placeholder: a lead carries no member, so the
/// access is unsupported.
pub const PHRASE_MEMBER_ON_LEAD: u32 = 1171;

/// `dot_or_curly` on a group operand: a group is expanded at statement level,
/// so the access is unsupported.
pub const PHRASE_MEMBER_ON_GROUP: u32 = 1172;

/// `dot_or_curly` on a closure whose output interface is empty: there is no
/// interface to search for the requested member.
pub const PHRASE_CLOSURE_EMPTY_OUTPUT: u32 = 1173;

/// `dot_or_curly` on a function call whose output interface is empty: there is
/// no interface to search for the requested member.
pub const PHRASE_FUNCALL_EMPTY_OUTPUT: u32 = 1174;

/// `dot_or_curly` fell through to the bare `Endpoint` arm: this endpoint kind
/// has no member list, so the access is unsupported.
pub const PHRASE_MEMBER_ON_ENDPOINT: u32 = 1175;

/// `dot_or_curly` on a `Member` phrase: a member reference has no member list
/// of its own, so the access is unsupported.
pub const PHRASE_MEMBER_ON_MEMBER: u32 = 1176;

/// `curly_mn` was given an empty left member list: the `{a | b}` operand has
/// nothing to pair, so the access yields nothing.
pub const PHRASE_CURLY_EMPTY_LEFT: u32 = 1197;

/// `curly_mn` was given an empty right member list: the `{a | b}` operand has
/// nothing to pair, so the access yields nothing.
pub const PHRASE_CURLY_EMPTY_RIGHT: u32 = 1198;

/// `curly_mn` met an operand kind it cannot convert to node elements, so the
/// `{a | b}` access yields nothing.
pub const PHRASE_CURLY_UNSUPPORTED_OPERAND: u32 = 1199;

// Pass1b: use statements (2000-2049)

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

/// Unexpected trailing node in a USE statement; it is ignored.
pub const USE_TRAILING_NODE: u32 = 2010;

// Pass1b: use-stage diagnostics (2050-2079)

/// Use of an undeclared dependency — add it to project.toml [dependencies] or load via --lib.
pub const USE_DEP_NOT_DECLARED: u32 = 2051;

/// Library referenced by `use` is not installed in the system root.
pub const USE_LIB_NOT_FOUND: u32 = 2052;

/// An imported symbol conflicts with an existing name.
pub const USE_SYMBOL_CONFLICT: u32 = 2061;

/// The imported symbol was not found in the target file.
pub const USE_IMPORTED_NOT_FOUND: u32 = 2071;

// Pass1b: parser / AST messages (2080-2119)

/// Generic syntax error.
pub const PARSER_SYNTAX_ERROR: u32 = 2080;

/// Invalid top-level declaration.
pub const PARSER_TOP_INVALID: u32 = 2081;

/// Invalid clause in a body.
pub const PARSER_CLAUSE_INVALID: u32 = 2082;

/// Invalid pin declaration.
pub const PARSER_PIN_INVALID: u32 = 2083;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PIN_ID_NOT_CONST: u32 = 2084;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PIN_NAME_NOT_CONST: u32 = 2085;

/// Net endpoint must be a port/label, not a literal.
pub const PARSER_NET_NOT_PORT: u32 = 2086;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_NET_INVALID: u32 = 2087;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_CONDS_INVALID: u32 = 2088;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_ROLE_INVALID: u32 = 2089;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_FUNC_INVALID: u32 = 2090;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PINS_INVALID: u32 = 2091;

/// Invalid import statement.
pub const PARSER_USE_INVALID: u32 = 2092;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_CONDBLOCK_INVALID: u32 = 2093;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_DECLAREB_INVALID: u32 = 2094;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_BODY_INVALID: u32 = 2095;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_JUDGE_INVALID: u32 = 2096;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PARD_INVALID: u32 = 2097;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_URI_INVALID: u32 = 2098;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PHRASES_INVALID: u32 = 2099;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_OPDS_INVALID: u32 = 2100;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PARAMS_INVALID: u32 = 2101;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PARDS_INVALID: u32 = 2102;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_ATTR_VALUES_INVALID: u32 = 2103;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_ATTR_LINES_INVALID: u32 = 2104;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_PINS_NAMES_INVALID: u32 = 2105;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_INSTS_INVALID: u32 = 2106;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_CONDS_ELIFS_INVALID: u32 = 2107;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_IDSS_INVALID: u32 = 2108;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
pub const PARSER_LEVELS_INVALID: u32 = 2109;

/// Retired: reserved for a per-production parser arm that was never
/// written; the grammar's recovery arms (E1002/E1003/E1004/E1007/E1013)
/// fire instead. Registration kept, no producer.
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

/// U177: a grammar-reserved word (direction/power/nc) at an `@key(…)` value
/// position is not a name — reserved words are nowhere in the name space
/// (declaration positions reject them identically). The parser drops the
/// tattr, keeps the row, and names the real problem instead of the generic
/// invalid-pin recovery. Numbering: the parser cluster 2080–2116 is full and
/// 2117+ belongs to the AST codes, so this threads the nearest gap (2120).
pub const PARSER_TATTR_RESERVED_WORD: u32 = 2120;

/// AST node is null/empty where a value was expected.
pub const AST_NODE_EMPTY: u32 = 2117;

/// AST node contains invalid UTF-8 data.
pub const AST_UTF8_ERROR: u32 = 2118;

/// AST node has an unexpected type.
pub const AST_TYPE_MISMATCH: u32 = 2119;

// Pass1b: name resolution (2120-2199)

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

// Pass2: vector shape validation (2900-2949)

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
/// (count mismatch); the operation generates no connection.
pub const SHAPE_EXPAND_DIM_MISMATCH: u32 = 2904;

/// Instance with 3+ pins cannot directly participate in `+` / `-`
/// (veccircuit.md inst constraint, eval.md §2): only 1x1 / 1x2 raw shapes can.
/// Emitted at Pass1 when the operand is a MultiPort component instance.
pub const SHAPE_INST_3PIN_PLUSMINUS: u32 = 2905;

/// NetShape missing on a net; the viz layer fell back to the deprecated
/// `connection_type()` inference (stage 3: rarely triggered, only on paths
/// that have not yet been covered by `build_net_shape`).
pub const SHAPE_INCOMPLETE: u32 = 2906;

/// Column-width mix in a `[...]` list (vec-arch.md §4.1.1 R4): a single-column
/// element (point / column vector) placed among double-column elements (two-pin
/// row / node) silently spans both columns of the resulting Node — e.g.
/// `[A, R101]` → `node{[A,R101.1] | [A,R101.2]}` makes A equipotential with
/// both pins. R4 forbids P⊕R / C⊕R / P⊕N / C⊕N; `_` is exempt (inherits the
/// sibling column width, so `[_, R101]` stays legal).
pub const SHAPE_COLUMN_WIDTH_MIXED: u32 = 2907;

/// Two **component bodies** with unequal port counts cannot be paralleled /
/// series-paired (`TP + R1`: a 1-port body against a 2-port body). A body is
/// a terminal-owning device, so `+` between two of them is a stacked body
/// pair whose terminals must line up; a body against a non-body (a net label,
/// a series result) is still the ordinary face-side law and stays legal --
/// that is the `... + TP1` "hang a test point on this node" idiom.
/// Emitted at Pass1. E2905 already rejects the 3+ port case, so the reachable
/// violation is 1-port against 2-port.
pub const SHAPE_INST_PORTCOUNT_PLUSMINUS: u32 = 2908;

/// Nested-subscript inner member group wider than two (vec-arch.md §4.1.1 R2,
/// CIMP U237 case-B ruling): `S[1:4][1,2,3]` reads each `S_i[1,2,3]` as a
/// `1*3` row vector, and a row vector of width >2 has no defined pairing
/// (the R5 row-vector law — two-pin devices are the only row sources). The
/// inner group must be a single member (a point, `4*1`) or a pair (a member
/// face, node `4*2`). Emitted at Pass1; the operand is dropped.
pub const SHAPE_MEMBER_GROUP_WIDTH: u32 = 2909;

// Pass1c: component definition (pins / attrs / units) (3000-3049)

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

/// Flat (non-grouped) pin mapping requires equal pin and name counts.
pub const PIN_FLAT_COUNT_MISMATCH: u32 = 3009;

/// Attribute node type mismatch.
pub const ATTR_TYPE_MISMATCH: u32 = 3021;

/// Attribute type is not supported.
pub const ATTR_TYPE_NOT_SUPPORTED: u32 = 3022;

/// Attribute node is missing a required subnode.
pub const ATTR_MISSING_SUBNODE: u32 = 3023;

/// Retired: the KVS value decode grew typed reading (pin-row value tails),
/// removing the untyped catch-all this code reported. Registration kept, no
/// producer (successor surface: the UVAL value family 3042+).
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

// Pass1c: module body (3050-3099)

/// Missing subnode in a module body clause.
pub const MODULE_MISSING_SUBNODE: u32 = 3051;

/// Module does not support PINS directly; use in/out/io declarations.
pub const MODULE_PINS_UNSUPPORTED: u32 = 3052;

/// Module does not support role definition.
pub const MODULE_ROLE_UNSUPPORTED: u32 = 3053;

/// Unexpected type in a module parameter.
pub const MODULE_PARAM_TYPE_UNEXPECTED: u32 = 3054;

/// Module header interface-typed (power/DC) parameter carries no direction word.
pub const MODULE_HEADER_IFACE_NEEDS_DIRECTION: u32 = 3055;

/// Function was not found in the class.
pub const MODULE_METHOD_NOT_FOUND: u32 = 3071;

/// Unexpected clause type in a module body.
pub const UNEXPECTED_CLAUSE_TYPE: u32 = 3081;

/// A row of a module-body `expects` clause is not one of the designed forms.
pub const EXPECTS_ROW_MALFORMED: u32 = 3082;

// Pass1c: params / functions (3100-3149)

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

/// A connection statement failed to parse.
pub const CONN_STMT_PARSE_FAILED: u32 = 3132;

/// Invalid function body node.
pub const FUNC_BODY_INVALID: u32 = 3133;

/// A connection statement was dropped because McPhrase::new returned None.
pub const FUNC_STMT_DROPPED: u32 = 3134;

/// Function call parse failure.
pub const FCALL_PARSE_FAILED: u32 = 3135;

/// A bare identifier in a function body's net statement does not resolve to a
/// declared pin, interface, parameter member, or func-local instance of the
/// component — it becomes a dangling net label.
pub const FUNC_FLOATING_LABEL: u32 = 3136;

/// An inline ghost-net created from a structured reference whose base resolves
/// to no declared instance (resolve-gate relax-everything — the bus is kept, not dropped)
/// is referenced only once — almost always a typo or a forgotten declaration.
/// Referenced twice or more it is a shared net and left alone.
pub const SINGLE_USE_INLINE_NET: u32 = 3137;

// Pass1c: instance declaration / reference (3150-3199)

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

/// Instance NC marker without a pin list.
pub const INST_NC_PIN_LIST_MISSING: u32 = 3158;

/// Instance NC marker operand is not a pin id list.
pub const INST_NC_PIN_VALUE_INVALID: u32 = 3159;

/// Malformed return statement.
pub const FUNC_RETURN_MALFORMED: u32 = 3161;

/// Invalid return expression — expected this or a label/bus.
pub const FUNC_RETURN_EXPR_INVALID: u32 = 3162;

/// A function may have at most one return statement.
pub const FUNC_MULTIPLE_RETURNS: u32 = 3163;

/// A `return` statement in a module body — dead syntax: only a function body
/// has a receiver to return to.
pub const MODULE_RETURN_NOT_ALLOWED: u32 = 3164;

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

/// Interface has no top-level pin definitions (all pins are inside role blocks); no pin-to-member
/// mapping is created.
pub const IFACE_NO_TOPLEVEL_PINS: u32 = 3180;

/// A referenced member is not defined on a declared bus / typed interface port.
/// The bus member set is fixed by the declaration; an undeclared member reference
/// would otherwise silently create a dangling net (e.g. `vout.VCC1V2` on
/// `out vout::DC(3.3V)` whose members are `{VCC, GND}`).
pub const BUS_MEMBER_UNDECLARED: u32 = 3181;

/// Phase 1 entry gate (resolve-gate-design.md §1.3/§1.4): the base name of a
/// structured dot/array reference (e.g. `uC.ADC.P`, `RS485.A`) is not declared
/// anywhere in scope — not an instance, func-local declare, module/component
/// inst, or FuncCall caller name — and the reference did not resolve by the
/// time its container finished parsing. The phantom bus was suppressed at parse
/// time (so two such references cannot short two rails together); this error is
/// the component-finish recheck's verdict.
pub const INSTANCE_REF_UNDECLARED: u32 = 3182;

/// Member/lane access on a module port that was declared without members
/// (a bare scalar `io X` / `out X` / `in X`). The declared port shape is
/// authoritative: a bare io is a single scalar net, and a `{P,N}` / `.member` /
/// `[...]` reference would implicitly widen it to a bus ("usage auto-expansion"),
/// which is prohibited. Declare the members or an interface type on the port,
/// or reference the whole port as a scalar.
pub const BUS_MEMBER_ON_SCALAR_PORT: u32 = 3183;

/// A direction-less module member (a `label` row, or a bus declared without a
/// direction word) accessed through an instance dot-path from outside its
/// module. A direction word is the only boundary ticket: members without one
/// are module-internal (name-space-global.md §3.2.3, U151).
pub const LABEL_NOT_EXPORTABLE: u32 = 3184;

/// A computed pin name (U211: an expression in the name slot) did not resolve
/// against the parameters bound at this site — the row carries no name rather
/// than dropping without a word.
pub const PIN_NAME_EXPR_UNRESOLVED: u32 = 3185;

/// A width-binder name (replicated-binding-design.md §4 check 1: a dynamic
/// range name not declared in the formal parameter table) appears *inside an
/// arithmetic width expression* (`1:count*2`). A bare whole-endpoint name
/// binds to the instance subscript width; a name inside arithmetic never
/// participates in back-solving — it must be given as an explicit parameter.
pub const DYN_WIDTH_EXPR_NEEDS_PARAM: u32 = 3186;

/// The binding row's subscript member count disagrees with the interface's
/// dynamic pin expansion count (replicated-binding-design.md §4 check 2, the
/// explicit-parameter leg: subscript members = dynamic expansion = physical
/// pins). The row-side counting face (subscript vs LHS pool) is E3111; this
/// code judges the subscript against the resolved dynamic expansion.
pub const IFACE_DYN_WIDTH_MISMATCH: u32 = 3187;

// Pass2: connection / shape (4000-4049)

/// Transposed connection size mismatch.
pub const CONN_TRANSPOSE_SIZE_MISMATCH: u32 = 4001;

/// Shape mismatch in a <- connection.
pub const CONN_LEFT_ARROW_SHAPE_MISMATCH: u32 = 4002;

/// Shape mismatch in a parallel connection.
pub const CONN_PARALLEL_SHAPE_MISMATCH: u32 = 4005;

/// Shape mismatch in a -> connection.
pub const CONN_SERIES_SHAPE_MISMATCH: u32 = 4007;

/// The operator is not supported in connection statements; use '+' for parallel, '-' / '->' for
/// series.
pub const CONN_OPERATOR_UNSUPPORTED: u32 = 4008;

/// Unexpected AST node type in a phrase.
pub const PHRASE_AST_TYPE_UNEXPECTED: u32 = 4009;

/// Member not found in the interface.
pub const PHRASE_IFACE_MEMBER_NOT_FOUND: u32 = 4022;

/// An iotype-prefixed port row carries a connection phrase. The row is read as a
/// port declaration, so nothing about the connection is registered; without this
/// check the whole statement is dropped in silence.
pub const PORT_ROW_WITH_CONNECTION: u32 = 4023;

/// A subscript is glued onto a reserved word in a phrase (`pins[2:3]`,
/// `this[2:3]`). The lexer keeps the subscript inside the identifier, so the
/// keyword is gone and the name addresses nothing.
pub const PHRASE_RESERVED_WORD_SUBSCRIBED: u32 = 4024;

/// An attribute key stands where a connection endpoint is required
/// (`uH.partno -> N1`, or a bare attribute name on a connection line). The name
/// does resolve — but in the definition space, to a value, and a value is not a
/// position, so the connection has no endpoint to attach to.
pub const ATTR_VALUE_NOT_A_TERMINAL: u32 = 4025;

/// A pin's recorded values carry no entry under the requested key
/// (`uH.1.volt`, where pin `1` declares only `desc`).
pub const PIN_VALUE_KEY_NOT_FOUND: u32 = 4026;

// Pass2: netlist heuristics (D-series / layout) (4050-4099)

/// A box has a placeholder pin not mapped to any real component pin.
pub const GHOST_PORT_BOX: u32 = 4050;

/// Multiple points resolve to the same node — possible short circuit.
pub const NET_MERGED_SHORT: u32 = 4051;

/// Retired (design doc interface-connect-rule-design.md section 6.1 D3):
/// judged "all member names differ" by comparing names, which the positional
/// law forbids - a crossed writing is legal. Registration kept, no producer
/// (same shape as CONN_TRANSPOSE_SIZE_MISMATCH 4001).
pub const NET_BUS_ORDER_MISMATCH: u32 = 4052;

/// Bus pin numbers are non-monotonic; the member-to-pin binding follows
/// declaration order, never numeric sorting - confirm the pairing is intended.
pub const SORT_HAZARD: u32 = 4053;

/// Retired: a `_` placeholder is not "unbound" — it has nowhere to bind by
/// construction. Detection merged into the open-lead doctrine family
/// (floating wire: EXPR_PLACEHOLDER_ONLY 5411; open lead: OPEN_LEAD 4065).
/// Registration kept, no producer; severity aligned Warning by the same
/// ruling (open-lead-design.md §6 ③).
pub const FLOATING_PLACEHOLDER: u32 = 4054;

/// A net endpoint is not mapped to any box — possible unexposed module boundary port.
pub const GHOST_PORT: u32 = 4055;

/// '_X' prefix identifier used as a standalone operand — it is a member name, not the wire '_'.
pub const LEAD_PREFIX_ID_AS_WIRE: u32 = 4058;

/// Retired: the legacy edge path that reported param-over-pin shadowing was
/// removed; binding is name-first and a same-named pin is shadowed by design.
/// Registration kept, no producer (live definition-side variant:
/// HW_FUNC_PARAM_SHADOWS_PIN 5510).
pub const FUNC_PARAM_SHADOWS_PIN: u32 = 4059;

/// A single-element square bracket expands to an unknown instance; the statement may produce no
/// nets or constraints.
pub const NET_DROPPED_STATEMENT: u32 = 4057;

/// Same logical net referenced more than once in a connection, always pairing to the same peer net
/// — redundant.
pub const NET_DUPLICATE_REF: u32 = 4060;

/// Same logical net referenced more than once in a connection, pairing to different peer nets —
/// possible short.
pub const NET_SHORT_REF: u32 = 4061;

/// GAP3 (§9.3.3 / vector-pipeline §2.3): two different declarations materialize
/// to the same physical pin id — a structural entity (component pin / module /
/// component) claims a flat path already occupied by a different structural
/// declaration. The second registration is silently merged into the first.
pub const PIN_OCCUPIED_BY_DECLARATION: u32 = 4062;

/// An .in/.out access on a component/class that declares no such pin is
/// isolated into a phantom endpoint (function-chain placeholder leak fallout).
pub const PHANTOM_IO_ACCESS: u32 = 4063;

/// A bare construction references a class that cannot be opened/resolved;
/// instantiation is dropped to an @? stub.
pub const UNRESOLVED_CLASS_STUB: u32 = 4064;

/// Open lead: a statement's anonymous `_` point gathers exactly one anchored
/// endpoint, leaving a free end nothing can reach. Judged per statement and
/// never merged across statements; two or more anchors make the point an
/// interior splice (silent), zero anchors is the floating wire
/// (EXPR_PLACEHOLDER_ONLY 5411). The judging source is the open-lead
/// doctrine (open-lead-design.md).
pub const OPEN_LEAD: u32 = 4065;

/// Retired: superseded by the per-slot missing-subnode codes (4085/4086/4088/
/// 4089/4091/4093) that name the exact slot. Registration kept, no producer.
pub const LAYOUT_MISSING_SUBNODE: u32 = 4081;

/// Layout attribute node type mismatch.
pub const LAYOUT_TYPE_MISMATCH: u32 = 4082;

/// Layout set is missing a required subnode.
pub const LAYOUT_SET_MISSING_SUBNODE: u32 = 4083;

/// Retired: superseded by LAYOUT_VALUE_TYPE_MISMATCH 4090, which checks each
/// value as it is parsed. Registration kept, no producer.
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

/// Retired: the layout face is deliberately lenient - unknown words warn and
/// extra nodes are ignored - so an extra-node refusal contradicts the face's
/// contract. Registration kept, no producer.
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

/// Retired: superseded by LAYOUT_EDGE_NAME_MISSING_SUBNODE 4088 and
/// LAYOUT_EDGE_NAME_ID_MISSING_SUBNODE 4096. Registration kept, no producer.
pub const LAYOUT_EDGE_NAME_NOT_ID: u32 = 4098;

// Pass2: netlist / interface binding (4100-4149)

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

/// A component pad is absent from every net.
pub const NET_PIN_UNWIRED: u32 = 4119;

/// Two endpoints of different interface families are connected (interface
/// connect rule §1.2 step 1): only same-family interfaces pair — `UART.TTL`
/// and `UART.RS232` are different families even though both are dotted `UART`.
pub const IFACE_CROSS_FAMILY_CONNECT: u32 = 4120;

/// Two roles of one interface family are connected but are not mutual peers
/// (interface connect rule §1.2 step 2): each side's role must name the other
/// in its `peer` attribute. Skipped when either side carries no role.
pub const IFACE_ROLE_INCOMPATIBLE: u32 = 4121;

/// An interface family whose definition declares `topology = "point to point"`
/// has more than two of its endpoints on one net (interface connect rule
/// §1.6 criterion 4).
pub const IFACE_ENDPOINT_COUNT_TOPOLOGY: u32 = 4122;

/// Both sides of an interface pair declare the same definition-level
/// attribute and their declared value sets share nothing (interface connect
/// rule §1.6 criterion 5 — judged only where both sides declare it).
pub const IFACE_ATTR_INCOMPATIBLE: u32 = 4123;

// Pass2: instantiation checks (4150-4199)

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

/// Statement references a component class whose instantiation failed; the whole statement is
/// skipped.
pub const INST_STMT_SKIP_FAILED_CLASS: u32 = 4155;

/// A connection statement failed to expand.
pub const INST_STMT_PARSE_FAILED: u32 = 4156;

/// Expanded builtin two-pin pair failed.
pub const INST_BUILTIN_TWOPIN_EXPAND_FAILED: u32 = 4157;

/// A member of a connection line failed to process.
pub const INST_MEMBER_PROCESS_FAILED: u32 = 4158;

/// Connection between adjacent members of a series failed.
pub const INST_ADJACENT_CONNECT_FAILED: u32 = 4159;

/// A module-level function body statement failed.
pub const INST_FUNC_BODY_STMT_FAILED: u32 = 4161;

/// Failed to instantiate a FuncCall during lane-by-lane wiring.
pub const INST_LANE_FUNCCALL_FAILED: u32 = 4162;

/// Failed to instantiate a Transposed member during lane-by-lane wiring.
pub const INST_LANE_TRANSPOSED_FAILED: u32 = 4163;

/// Retired: the R0 rework makes `+` always produce Parallel, so the
/// group-shape refusal has no producer left (sister code
/// CONN_SERIES_SHAPE_MISMATCH 4167 stays live). Registration kept, no producer.
pub const CONN_GROUP_SHAPE_MISMATCH: u32 = 4166;

/// Sub-module DC power port is never connected (missing power argument?).
pub const INST_POWER_PORT_UNBOUND: u32 = 4172;

/// A constructor function body line failed.
pub const INST_CTOR_BODY_STMT_FAILED: u32 = 4173;

/// Constructor parameter binding failed.
pub const INST_CTOR_PARAM_BIND_FAILED: u32 = 4174;

/// Instance argument has no formal port to bind (with module/bound details).
pub const INST_ARG_UNBOUND_DETAILED: u32 = 4175;

/// Declarative instance parameter binding failed (unknown / excess / missing-required argument).
pub const INST_PARAM_BIND_FAILED: u32 = 4176;

/// Component-level parameter shares a name with the same-name constructor
/// func parameter. Class params define class behavior; the constructor func's
/// params declare the construction arity, so they must not reuse a class name.
pub const COMPONENT_PARAM_FUNC_CONFLICT: u32 = 4177;

/// Instance is missing a required constructor parameter. Silent in dev mode
/// (Component-Spec Separation — circuit topology only needs pins and the
/// value comes from spec / the BOM); reported as a warning in strict mode
/// (`--strict`). The instance is always created with the supplied arguments.
pub const INST_PARAM_MISSING_REQUIRED: u32 = 4178;

/// Argument/formal vector width mismatch in arg-to-formal binding: a scalar
/// cannot be bound to a vector formal and equal-width vectors pair positionally
/// only. No implicit expansion or dropping of members is performed.
pub const VECTOR_WIDTH_MISMATCH: u32 = 4180;

/// Chain left/right vector width mismatch during pass-through pairing: the two
/// sides of a chain member must zip positionally at equal width; unequal widths
/// are an error, never flattened, never member-dropped.
pub const VECTOR_ZIP_WIDTH_MISMATCH: u32 = 4181;

/// A `_` lead joins two **different** nets. The lead is an ideal wire (a body,
/// vec-dianlu §5.4), so its two ends are meant to be the same net — a lane's
/// left and right ends of one member. Two distinct bare nets under one lead
/// means the wire shorts them together at zero impedance. Warning, never an
/// error: the author may have written the jumper on purpose.
pub const CONN_LEAD_CROSSNET: u32 = 4182;

/// `+` between two **bodiless** operands joins two different nets. A label or
/// rail is not a connection — it is the *name of an existing equipotential
/// region* (vec-dianlu §1.4/§5.4), so its potential is carried by its name and
/// two different names are two different potentials. With no body on either
/// side there is nothing to stack, so `+` can only merge the two regions into
/// one: a dead short. Error, not a warning — unlike a `_` lead there is no
/// "the author may have meant to jump them" reading here; two distinct
/// potentials have no legal merge.
pub const CONN_NET_CROSSNET: u32 = 4183;

/// A module port binding carries a role argument (`io bus[1:4]::GPIO(Controller)`).
/// Role (Controller / Peripheral / Master / Slave / …) is the *link identity of
/// an endpoint terminal pin* — the answer to "at what position does THIS device
/// join the chain". A module port is a role-less conduit: identity comes from
/// whatever terminal the port is wired to on each side, so the port itself
/// carries none. Replicated-binding-design R3; the check lives in
/// `validation::interface` and is an Error.
pub const MODULE_PORT_IFACE_ROLE: u32 = 4184;

/// A role-bearing interface's constructor argument is a literal
/// (`SPI::SPI("Slave")`, `PJ(123)`): the position takes a bare identifier —
/// the role name, which must resolve in the definition space. A literal there
/// classifies as a plain string/number parameter, so the binding parses but
/// the role is never recorded: E4104 role validation, E4184 port role gate
/// and peer matching are all silently bypassed, and a misspelling passes
/// clean (ident-vs-literal ruling, U144 first slice; the check lives in
/// `validation::iface_role_arg` and is an Error).
pub const IFACE_ROLE_ARG_LITERAL: u32 = 4185;

/// An interface's role table declares a different lane count than the
/// interface's role-less conductor view (`conductor-view-design.md` R-CV2,
/// the uniformity law). The interface-level `pins` table is the shape every
/// role-less binding resolves from; a role table with its own `pins` list
/// asserts the same wires, so its lane count must equal the view's — unequal
/// counts mean the interface itself is ambiguous and the error lands at the
/// definition, not at some distant instantiation point. A role without its
/// own `pins` table inherits the view and is never judged here. Error; the
/// check lives in `validation::interface`.
pub const IFACE_VIEW_LANE_MISMATCH: u32 = 4186;

/// R3 mediator half (replicated-binding-design.md §4 check 3): a role
/// argument is legal only on terminal pins rows. An interface instance in the
/// MIDDLE of a series chain (`A - bus::GPIO(Controller) - B`, two or more
/// adjacencies on both sides) is a wiring mediator — a role-less conductor
/// whose identity comes from the chain's terminal pins rows. The module-port
/// half is E4184 (`check_module_port_role_free`); this code judges the chain
/// position at instantiation (`process_series_branch_inplace`). Chain
/// endpoints keep their roles (design §2: a role-bearing bus instance wired
/// as an endpoint is a legal bus identity).
pub const MEDIATOR_IFACE_ROLE: u32 = 4187;

// Pass2: AssemblyGate netlist health — R-series report rows (4200-4249)
//
// The netcheck Tier-0 report (instant::netcheck) registers every R-series row
// here as a numeric code (rule-registry design §5-1), so each rule has one
// identity in the central `errcodes` table. The rules are still emitted as
// report rows (sink = GateReport), not as envelope diagnostics; the report's
// per-row level is the severity declared next to each code in `src/rules.rs`
// `GATE_RULES` (rule-registry design §7.3 gate/sink axes).

/// R01 — a vector reference reached the netlist unexpanded (literal braces).
pub const GATE_LITERAL_POINT: u32 = 4201;

/// R02 — both terminals of a two-terminal device land on the same net (short).
pub const GATE_SHORT_PASSIVE: u32 = 4202;

/// R03 — a net carries two different power-domain names (short).
pub const GATE_SHORT_RAIL: u32 = 4203;

/// R03a — a net carries multiple power-domain aliases (advisory).
pub const GATE_RAIL_ALIAS: u32 = 4204;

/// R04 — two members of the same bus land on one net (lane short).
pub const GATE_SHORT_LANE: u32 = 4205;

/// R05 — a unit-typed argument claims no formal parameter slot.
pub const GATE_UNRESOLVED_UNIT: u32 = 4206;

/// R06 — a non-power net is oversized (meganet, advisory).
pub const GATE_MEGANET: u32 = 4207;

/// R07 — a net references a device missing from the instance table (ghost).
pub const GATE_GHOST_INSTANCE: u32 = 4208;

/// R08 — an endpoint path has an unregistered middle segment (phantom path).
pub const GATE_PHANTOM_PATH: u32 = 4209;

/// R09 — a device power/ground pin is left unconnected (advisory).
pub const GATE_FLOATING_POWER_PIN: u32 = 4210;

/// R10 — pass2 device count fell below the pass1 symbol-table expectation.
pub const GATE_SYMBOL_CONSERVATION: u32 = 4211;

/// R11 — a same-name power rail is split into unconnected nets.
pub const GATE_SPLIT_RAIL: u32 = 4212;

/// R12 — a port net holds only its own point (advisory).
pub const GATE_DANGLING_PORT: u32 = 4213;

/// R14 — an instance is registered but appears in no net (advisory).
pub const GATE_ORPHAN_INSTANCE: u32 = 4214;

/// R15 — a synthetic terminal is not backed by any real pin (advisory).
pub const GATE_SYNTHETIC_PIN: u32 = 4215;

/// U155 — a connection replication count must be an int >= 2
/// (`Phrase * N` / `Phrase × N`).
pub const CONN_REPLICATION_COUNT: u32 = 4216;

// Pass3: duplicate validation (5000-5049)

/// Same name defined in another file (cross-file duplicate).
pub const DUP_CMIE_CROSS_FILE: u32 = 5001;

/// Duplicate definition within the same declaration.
pub const DUP_WITHIN: u32 = 5002;

/// Enum value appears more than once in the enum.
pub const DUP_ENUM_VALUE: u32 = 5003;

// Pass3: naming / style (5050-5099)

/// Component name starts with lowercase; convention is UPPER_SNAKE.
pub const NAME_COMPONENT_LOWERCASE: u32 = 5051;

/// Port name shadows a library CMIE name.
pub const NAME_PORT_SHADOWS_CMIE: u32 = 5052;

/// Pins use mixed naming conventions.
pub const NAME_PIN_MIXED_CONVENTION: u32 = 5053;

/// Instance name is a single character.
pub const NAME_INSTANCE_SINGLE_CHAR: u32 = 5054;

/// Port/instance name shadows a library CMIE name.
pub const NAME_PORT_INST_SHADOWS_CMIE: u32 = 5056;

/// Parameter name shadows a library CMIE name.
pub const NAME_PARAM_SHADOWS_CMIE: u32 = 5057;

// capability / variant (abstract-variant-capability plan) (5058-5066)

/// recipe body may only contain signal declarations and funcs.
pub const RECIPE_BODY_INVALID: u32 = 5058;

/// recipe func references a bare name that is not a declared signal, a
/// parameter, or a func-local instance (§3.2 self-consistency).
pub const RECIPE_FUNC_UNRESOLVED_REF: u32 = 5059;

/// variant may not write pins/params/func — those are inherited from the base
/// (data lock, §7.2).
pub const VARIANT_REDECLARES_PINS_PARAMS_FUNCS: u32 = 5060;

/// abstract component may not carry a variant base `:` (no variant chain).
pub const ABSTRACT_DERIVES_ABSTRACT: u32 = 5061;

/// `:` (variant) and `::` (recipe adoption) are mutually exclusive.
pub const VARIANT_ADOPTS: u32 = 5062;

/// `:` target is not an abstract component.
pub const VARIANT_BASE_NON_ABSTRACT: u32 = 5063;

/// `::` target is not a recipe.
pub const ADOPTS_NON_RECIPE: u32 = 5064;

/// Adopting component is missing a declared recipe signal.
pub const RECIPE_SIGNAL_MISSING: u32 = 5065;

/// Two adopted recipes expose the same func name and the component does
/// not override it.
pub const ADOPTED_FUNC_AMBIGUOUS: u32 = 5066;

/// BOM overlay value names a class that is not a `:` descendant of the
/// slot's declared abstract class (param-authoring-design.md section 4,
/// U245, check (1) = E5067). Covers an unresolvable value name too: a name
/// the defs do not know cannot be a descendant either.
pub const BOM_VALUE_NOT_DESCENDANT: u32 = 5067;

/// BOM block key does not designate an abstract-declared slot: either the
/// instance at that path declares a concrete class, or no instance lives at
/// the path at all (param-authoring-design.md section 4, U245, check (2),
/// branches b2/b3 = E5068).
pub const BOM_KEY_NOT_SLOT: u32 = 5068;

// Pass3: reference integrity (5100-5149)

/// Spec key references a parameter that is not declared.
pub const SPEC_KEY_UNDECLARED_PARAM: u32 = 5101;

/// Reference integrity violation.
pub const REF_INTEGRITY: u32 = 5102;

/// Function has parameters but no body (empty implementation).
pub const FUNC_PARAMS_NO_BODY: u32 = 5103;

// Pass3: ports / pins (5150-5199)

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

/// this :: TYPE declaration is not allowed.
pub const INST_THIS_TYPE: u32 = 5160;

/// Module port is declared but never connected.
pub const MODULE_PORT_UNUSED: u32 = 5162;

/// Condition compares against a single binary value.
pub const COND_SINGLE_BINARY: u32 = 5163;

// Pass3: functions / roles / defaults (5200-5249)

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

// Pass3: definition structure (M-series) (5250-5299)

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

/// Interface expects more pins than are bound.
pub const IFACE_PIN_COUNT_MISMATCH: u32 = 5262;

/// Function shares its name with a port/param.
pub const FUNC_SHARES_NAME_WITH_PORT: u32 = 5263;

/// Spec key appears more than once.
pub const SPEC_KEY_DUPLICATE: u32 = 5267;

// Pass3: .int class checks (5300-5349)

/// Same name used for different definition kinds.
pub const DEF_AMBIGUOUS_NAME: u32 = 5301;

/// Definition references a class that is not loaded.
pub const DEF_REF_NOT_LOADED: u32 = 5302;

/// Component has an unconventional '.int' suffix.
pub const COMPONENT_INT_SUFFIX: u32 = 5303;

/// Enum has an unconventional '.int' suffix.
pub const ENUM_INT_SUFFIX: u32 = 5304;

// Pass3: instance / attribute checks (5350-5399)

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

/// Dotted attribute name starts with a key that is neither the component name
/// nor a registered attribute key.
pub const ATTR_DOTTED_NAME_UNRESOLVED: u32 = 5358;

/// One attribute list declares the same key more than once.
pub const ATTR_KEY_DUPLICATE: u32 = 5359;

/// Attribute value is outside the key's registered word set, or a flag key
/// carries a value.
pub const ATTR_VALUE_NOT_IN_VOCABULARY: u32 = 5360;

/// An instance's parameter value falls outside the interval the class's
/// `ratings` clause declares for it (ratings-param-constraint-design.md §3).
pub const RATING_PARAM_OUT_OF_RANGE: u32 = 5361;

/// A `ratings` entry key names no constructor parameter of its class.
pub const RATING_KEY_NOT_A_PARAM: u32 = 5362;

// Pass3: enum / expression checks (5400-5449)

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

/// 'this' used in a top-level net statement; it is only valid inside instance/function contexts.
pub const EXPR_THIS_TOP_LEVEL: u32 = 5410;

/// Net connects only to '_' placeholder; the connection has no effect.
pub const EXPR_PLACEHOLDER_ONLY: u32 = 5411;

/// Attribute value equals its own key; likely a copy-paste mistake.
pub const ATTR_SELF_REFERENTIAL: u32 = 5412;

/// Division by zero while evaluating a value expression.
pub const EVAL_DIVIDE_BY_ZERO: u32 = 5413;

/// Arithmetic operator applied to operands it is not defined for (a value mixed
/// with text, two unit families that do not meet, or a family that forbids the
/// operator — temperature addition, decibel arithmetic, ordering of a
/// non-ordered family).
pub const EVAL_OPERAND_NOT_NUMERIC: u32 = 5414;

/// Arithmetic overflowed the integer (or finite real) range.
pub const EVAL_OVERFLOW: u32 = 5415;

/// A library author's `error(msg)` expression was evaluated (U212). The
/// message is the author's own — the template passes it through verbatim.
pub const EVAL_ERROR_EXPRESSION: u32 = 5416;

// Pass3: condition blocks (5450-5499)

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

// 5457 retired with the `anl` direction word (PIN_IO_MIX_ANALOG_POWER);
// the numeric gap is permanent.

/// Parameter shares its name with a pin.
pub const PARAM_PIN_NAME_SHADOW: u32 = 5458;

/// Module is a stub.
pub const MODULE_STUB: u32 = 5459;

/// A later if/else-if branch duplicates an earlier branch's condition —
/// the later branch can never be selected.
pub const COND_DUPLICATE: u32 = 5460;

/// A judge operand has a form the condition collector does not recognize
/// (a call, an unpowered operator form); the operand is dropped, the whole
/// judge cannot be built, and the condition reads as absent — the if-branch
/// never selects and no other diagnostic explains why.
pub const COND_JUDGE_OPERAND_DROPPED: u32 = 5461;

/// A condition judge compares two values of different lexical families —
/// a bare word against a quoted string (U144 residual 3, ruled 2026-09-20).
/// The comparison is not silently decided by text; it is rejected so the
/// author spells both sides in the same family.
pub const COND_FAMILY_MISMATCH: u32 = 5462;

// Pass3: hardware checks (5500-5549)

/// Pin numbers have gaps.
pub const HW_PIN_NUMBER_GAP: u32 = 5502;

/// Pin count is unusually high.
pub const HW_PIN_COUNT_HIGH: u32 = 5503;

/// Component has zero pins but parameter attributes.
pub const HW_ZERO_PINS_WITH_PARAMS: u32 = 5504;

/// Interface role names a peer that is not defined in this interface.
pub const HW_IFACE_PEER_DANGLING: u32 = 5506;

/// All pins have the same IO type.
pub const HW_ALL_SAME_IO_TYPE: u32 = 5507;

/// Interface role names a peer that does not name it back.
pub const HW_IFACE_PEER_NOT_MUTUAL: u32 = 5508;

/// Interface role and its declared peer declare different member widths.
pub const HW_IFACE_PEER_WIDTH_MISMATCH: u32 = 5509;

/// Function parameter shadows a pin name.
pub const HW_FUNC_PARAM_SHADOWS_PIN: u32 = 5510;

/// U133 phase 1 — both sides of an interface member connection declare the
/// `out` direction word at their adoption rows.
pub const IFACE_DIR_CONFLICT: u32 = 5511;

/// A `@pair(group)` group on an interface member table has other than two
/// legs — a differential signal has exactly two faces.
pub const HW_IFACE_PAIR_NOT_TWO: u32 = 5512;

/// The retired `diff_pair` interface-body key is still written; the member
/// rows must carry `@pair(group)` tags instead.
pub const HW_IFACE_DIFF_PAIR_RETIRED: u32 = 5513;

/// A `@pair(group, match: …)` constraint value is not a length quantity
/// (pair-constraint-design.md §3.2 — the tolerance is a board-drawing fact,
/// spelled in a length unit; the time conversion is the consumer's).
pub const HW_PAIR_CONSTRAINT_NOT_LENGTH: u32 = 5514;

/// The two legs of a `@pair` group disagree on a constraint slot — different
/// values, or one leg wrote the slot and the other did not
/// (pair-constraint-design.md §3.2: both legs write, values must be equal).
pub const HW_PAIR_CONSTRAINT_MISMATCH: u32 = 5515;

/// A `@pair` row attr carries a named constraint slot but no group name —
/// the constraint has no pair to ride (pair-constraint-design.md §3.2).
pub const HW_PAIR_CONSTRAINT_ORPHAN: u32 = 5516;

// Pass3: type / unit compatibility (5550-5599)

/// Incompatible types or unit types.
pub const TYPE_INCOMPATIBLE: u32 = 5552;

// Pass3: global diagnostics (5600-5649)

/// Parameter or port is declared but never used.
pub const UNUSED_PARAM_OR_PORT: u32 = 5641;

/// Port is declared but never used in any net connection.
pub const PORT_NEVER_USED: u32 = 5642;

/// Parameter has no inferred type.
pub const UNTYPED_PARAM: u32 = 5643;

// ERC (electrical rule check) (6000-6099)

/// Placed abstract component with no selected part (partno unset).
pub const ABSTRACT_PART_UNSELECTED: u32 = 6005;

/// Variant still carries an unset (±0/empty) inherited spec item.
///
/// Registered but NOT yet emitted (abstract-variant-capability-plan §4.4 /
/// Open D6): disambiguating a `±0` placeholder from a genuine `±0` device
/// rating requires spec metadata, which this phase has none of. Gating the
/// warning on a `±0` value sentinel would be exactly the hardcoding the plan
/// rules out (rely on spec metadata, not hardcoded sentinels) — revisit when a spec-type table
/// exists. The materialized variant def keeps the base's `spec.*` leaves
/// untouched, so the unset state is always visible to a BOM/consumer.
pub const VARIANT_SPEC_UNSET: u32 = 6006;

/// DC `@bridge` subgraph forms a loop (parallel/cyclic legs) with no `@star`
/// discharge on a hub ref — PWR-2 (intent-design.md §3.4).
pub const POWER_BRIDGE_LOOP: u32 = 6007;

/// `@clamp(ref)` references a ref whose role is not protective/earth — PWR-7
/// (intent-design.md §11).
pub const CLAMP_REF_NOT_PROTECTIVE: u32 = 6008;

/// A DC `rail [hot,ret]::DC(…)` ctor argument does not decode to the contract
/// it names (nominal not a DC volts value, tol not ±%, capacity not a current,
/// eff not a factor, or an off-register param key) — the §13.2 Volt-arg decode
/// (intent-design.md §4.1/§5.2).
pub const POWER_RAIL_DECODE: u32 = 6009;

/// The same net is the `hot` member of two rails (two handwritten supply roots
/// on one S) — P3 fake-conflict (intent-design.md §4.1 — an intermediate net never enters a
/// domain).
pub const POWER_RAIL_TWO_ROOTS: u32 = 6010;

/// A sink (`psnk`) lands on a net whose derived supply S differs from the
/// sink's required nominal — the §4.4 mandatory-nominal check, the canonical P3/E-PWR-001 case
/// (intent-design.md §4.4/§11; e.g. a `::DC(3.3V)` sink on a 5V rail or
/// psrc-fed net). S comes from the net's handwritten supply roots (§4.3) or,
/// for a root-less net, the upstream root it is fed to through transparent
/// copper / a module boundary (§7 L4 reach, island-attribution-design.md).
/// Requirement nominal vs guarantee nominal is the comparison; window ⊆-checks
/// (req/abs, spec-declared) are a later S-set step.
pub const POWER_SINK_NOMINAL_MISMATCH: u32 = 6011;

/// A `psrc`/`psnk`/`psbi` `::DC(…)` ctor argument does not decode to the
/// contract it names — the pin-side of 6009 (§13.2 Volt-arg decode / §5.2
/// closed word-list discipline). A sink writes its nominal plus an optional
/// `amp` demand key (rail-contract-design.md §8.1); tol/capacity/eff are
/// source-exclusive (PWR-4) and `amp` is sink-exclusive, so either off its
/// register side is flagged; req/abs belong in the component `spec` (§4.4
/// write-site rule), never per-schematic. (intent-design.md §4.1/§5.2)
pub const POWER_PIN_DECODE: u32 = 6012;

/// Two or more `psrc` hard sources land their hot terminal on the same net
/// with no declared combine — the PWR-3 source-contention kernel
/// (intent-design.md §11). Wiring two regulators' outputs straight to one
/// node is an undeclared parallel source: without an ORing / combining element
/// between them a failed or slower source back-feeds the other. `psbi`
/// (conditional source: battery coexistence) and rail faces (6010's scope) are
/// not counted; copper pass-through propagation and converter re-anchoring
/// belong to the later S-set step.
pub const POWER_SOURCE_CONTENTION: u32 = 6013;

/// A declared DC `@bridge` joins an `@role(isolated)` member to a non-isolated
/// net — the §3.2 isolated-world zero-DC-bridge contract (intent-design.md
/// §3.2 / §11 PWR-9, the conduit-level half). An isolated ref, or a rail whose
/// return member is an isolated ref, may only cross out of its world through an
/// explicit Y-cap `@couple`; a DC bridge turns the "isolated" secondary side
/// into a hard connection. `isolated`↔`isolated` bridging merges two zero-DC
/// worlds and is accepted at the kernel; component-level crossings are PWR-9's
/// other half. (Design note: a rail whose return member is an
/// `@role(isolated)` conduit is how a net is derived into that isolated world —
/// intent-design.md §4.)
pub const ISOLATED_DC_BRIDGE: u32 = 6014;

/// An `@role(protective)` conduit carries more than one declared DC `@bridge` —
/// PWR-8 (intent-design.md §3.2/§11): a protective conduit is allowed
/// exactly one single-point bridge to its circuit main reference. A second
/// bridge (a parallel protective-ground leg, or a tie to a second island) is a
/// second single point and a ground loop under ESD — and, unlike a quiet-leg
/// loop, it is *not* discharged by `@star`: the protective single point is a
/// hard (1,0) invariant, not a simulation-deferred design choice.
pub const PROTECTIVE_MULTI_BRIDGE: u32 = 6015;

/// An `@role(earth)` conduit is incident to a declared DC `@bridge` — the §3.2
/// earth-row leak (intent-design.md §3.2 / §11 chassis/earth scene /
/// landing axis
/// ④). The chassis/earth reference couples to protective or main only through a
/// Y-cap `@couple` (AC-only, does not merge L1 classes); a DC `@bridge` is a
/// low-resistance direct tie and is flagged as a leakage warning. `@clamp` into
/// an earth ref stays legal (PWR-7 permits protective/earth clamp targets) —
/// only a DC bridge row leaks.
pub const EARTH_DC_LEAK: u32 = 6016;

/// A DC-bridged reference island carries other than exactly one `@role(main)`
/// root — the §3.2.1 "each L1 island has one main" contract
/// (intent-design.md §3.2.1 / §3.2 main row / landing axis ④). Reference
/// islands are the connected components of DC `@bridge` edges whose two
/// endpoints are both role-bearing reference identities (supply-side legs like
/// `@bridge(VDD_3V3, VDDA)` and a bound child leg naming a member the child
/// owns no role for stay out of the graph). Zero mains = a quiet/protective
/// group DC-joined with no island ground to return to; two or more mains = two
/// power worlds were DC-joined by a bridge — the doc's canonical
/// "two main islands must not be @bridge'd" case.
pub const REFERENCE_ISLAND_ROOT: u32 = 6017;

/// A `@role(quiet)`/`@role(protective)` reference conduit carries no declared
/// DC `@bridge` at all — the forgotten-declaration case
/// (conduit-equivalence-design.md §8.4). `@bridge` is explicit, never inferred
/// from a component type (a ferrite without a `@bridge` is an ordinary part),
/// but the ERC expects exactly one bridge to the island main for these two
/// roles and counts a bare zero as an unwired declaration — never silent. The
/// upper bound is discharged by 6007 (quiet second leg = loop, `@star`-exempt)
/// and 6015 (protective second leg = second single point, hard).
pub const ROLE_REF_MISSING_BRIDGE: u32 = 6018;

/// PWR-1 no-source face (intent-design.md §11): a flat net that carries
/// component power-sink (`psnk`) terminals but no supply root on the net itself
/// — no declared domain-rail face and no `psrc`/`psbi` hot pin. The net's loads
/// promise to draw from a supply that nothing on the net guarantees. Kernel
/// (mirrors 6011/6013): a root-less net *fed* through transparent copper or a
/// module boundary to an upstream supply root is adjudicated there by 6011 (net-
/// island-attribution-design.md §7 L4 reach), not a PWR-1 orphan here; module
/// boundary feed ports are structurally invisible because only
/// `InstKind::Component` parents are decoded as sinks.
pub const SINK_NET_NO_SOURCE: u32 = 6019;

/// §6.2③ combine-output re-anchor (rail-contract-design.md §6): a combine
/// element — ≥2 input-direction (`psnk`/`psbi`) power rows + ≥1 `psrc` output
/// row — is a pass-through OR-merge, not a regulator. Its output `psrc` writes
/// only the merged nominal `::DC(v)`: a ±tol window would claim the net's supply
/// holds tighter than any single live input can, the exact over-claim the
/// OR-merge ∪ semantics exists to catch under single-source states (§6.3).
pub const COMBINE_OUTPUT_TOL: u32 = 6020;

/// PWR-4 budget, net-local first kernel (rail-contract-design.md §8): a net
/// whose supply root declares a capacity (domain-rail face or psrc/psbi hot
/// pin carrying `capacity`) carries psnk sinks whose declared `amp` demand
/// (§8.1, sink-exclusive, opt-in) sums to more than that capacity. Net-local,
/// mirroring 6011/6013/6019: only loads wired directly to the capacity-bearing
/// net are counted — converter push-up (`I_in = ΣP_out/(|V_in|×eff)`, §7.2),
/// copper pass-through feed, and module-boundary feed are the S-set step.
pub const NET_BUDGET_EXCEEDED: u32 = 6021;

/// §8.5 cross-plane DC-relation completeness (conduit-equivalence-design.md
/// §8.5, PWR-2 upper clause) — the first consumer of the NetIslandIndex L1
/// (design island-attribution-design.md §7 L2). A two-terminal DC element
/// *is* a relation: when its pads resolve to two potential classes (conduit
/// copper, or rail hot/ret member net name, or an A′ boundary-inherited
/// ancestor class) whose domain-worlds are DISJOINT — not co-resident in one
/// declared `rail[hot,ret]` loop — the leg is a cross-plane DC relation
/// (return↔return, hot↔hot supply bead, hot↔foreign return) that must carry an
/// explicit `@bridge`/`@couple` on its own statement. Advisory Warning — a
/// forgotten single-point bridge or an undeclared bypass; the relation is never
/// inferred from the part type (§8.4).
pub const RETURN_LEG_UNDECLARED: u32 = 6022;

/// §6.1 regulator gate (rail-contract-design.md, window batch) — a regulator
/// declares its operating pre-condition `input_req` (the input window inside
/// which its `output` post-condition holds). The supply window actually riding
/// its input net must lie inside that declared input window: `S(input) ⊆
/// input_req`. A Resolved input window outside it fires an Error. Nets whose
/// window cannot be derived (NoSupply / Unresolved — module-boundary feed,
/// undeclared contention, an un-driven leg) are not adjudicated, mirroring the
/// nominal layer's leave.
pub const POWER_CONVERTER_GATE: u32 = 6023;

/// §6.3 sink req window (rail-contract-design.md, window batch) — a load whose
/// component spec declares `input_req` states the supply window it accepts.
/// The actual supply window on the net the sink's hot pin rides must sit inside
/// that accepted window: `S(supply net) ⊆ input_req`. A Resolved supply window
/// that escapes the accepted window (over/under-volts the load) is an Error.
/// A regulator's own `psnk` input row is gated per-net by 6023, not re-judged
/// here; nets without a derivable window stay silent (E-PWR-001 window step).
pub const POWER_SINK_WINDOW_MISMATCH: u32 = 6024;

/// §6.1/6023 partial-spec advisory (rail-contract-design.md, window batch) — a
/// component with a `psrc`/`psbi` output row writes a `spec` block but declares
/// only one of the Hoare triple's two windows: `input_req` (the pre-condition
/// its output needs) or `output` (the post-condition it guarantees). A regulator
/// needs both for the gate (6023) and the sink-window (6024) checks to judge it;
/// a one-sided write is an incomplete contract. Advisory Info — the written side
/// still decodes (an output-only regulator is treated as guaranteeing that
/// output; an input_req-only one as an un-gated load), it just cannot be gated.
/// A pure load (no output row) legitimately writes `input_req` alone and is not
/// flagged.
pub const POWER_CONVERTER_SPEC_INCOMPLETE: u32 = 6025;

/// §6.7 converter output vs same-net rail window (rail-contract-design.md §6.7,
/// window-notes batch) — a regulator's `spec.output` is the window it promises
/// to hold on its Src output net. When that net is a *declared rail face*, the
/// rail's own window is the scope's promise for the net (§4.1 priority 1, the
/// rail wins even over a converter landing on it). A converter that guarantees a
/// window the rail does not cover can deliver outside the rail's declared
/// tolerance → Error. A Src landing on a plain driven node (buck `LX` → filter
/// → rail net), or a degenerate rail face (a bare nominal with no ±tol — no
/// allowed spread is declared) is not checked.
pub const POWER_CONVERTER_OUTPUT_RAIL_WINDOW: u32 = 6026;

/// §8.6 device reference-pin cross-plane (conduit-equivalence-design.md §8.6,
/// adjudicated 2026-09-09) — the ≥3-pin functional sibling of 6022. A device
/// whose DC-pair *return* pins (the `ret` member of each `psnk`/`psrc`/`psbi`
/// `::DC` row) resolve to ≥2 disjoint potential classes is a candidate silent
/// merge: the die/substrate DC-joins two board return planes the declaration
/// layer never tied. Unlike a two-terminal leg (6022 — the part *is* the
/// relation and carries its own `@bridge`), a functional device's internal
/// return commonality is not a declarable leg, so the span must be covered by
/// ① a net-level declared `@bridge`/`@couple` on the class pair (the uc/GNDA
/// shape) or ② a declared power-isolation structure (the iso5/DC.ISO_SRC
/// shape: one return class is an `@role(isolated)` copper carried by a
/// source-side contract and another return class is sink-side — the device is
/// the isolator that defines the isolated world). A disjoint return-class span
/// under neither is a Warning — the merge is never inferred from a part type
/// (§8.4); only a declared net bridge or the declared isolation structure
/// exempts it.
pub const DEVICE_RETURN_SPAN_UNDECLARED: u32 = 6027;

/// §5.3.2 arrow-direction consistency (intent-design.md §5.3.2, PWR-10,
/// adjudicated 2026-09-10) — a direction-word terminal (module power port or
/// leaf component power pin) sitting at the wrong end of its own connection
/// chain. A `psrc` source face must lead the chain (first member / right side
/// of a `{L|R}` through), a `psnk` sink must trail it (last member / left side
/// of a `{L|R}` through); a module wiring its own body into its own face is
/// internal implementation and exempt (the direction word is the contract to
/// the *parent* frame). The direction word stays authoritative for semantic
/// rules (6011/6019/6021/pwrflow); this Warning tells the author the arrow /
/// terminal order disagrees with the declared direction contract.
pub const DC_BINDING_DIR_MISMATCH: u32 = 6028;

/// §8.7 port role contract: an `out` port's `@bind_role(<role>)` parent binding
/// must resolve to a reference of that role, else an Error.
pub const PORT_BIND_ROLE_MISMATCH: u32 = 6029;

/// Model A (§8.8, `[hot, ret]` pairing): a `::DC` power row declares a DC
/// crossing, and a crossing is the *pair* — the hot terminal plus the return it
/// closes over (`psnk [1,2] = VIN{Vin, GND}::DC(5V)`). A `::DC` row whose
/// contract names no second member (`psnk 5 = VCC::DC(5V)`) leaves
/// [`McPwrPin::ret`](crate::semantic::component::mc_pins::McPwrPin::ret) `None`,
/// and every consumer that adjudicates the crossing reads that member — 6022
/// (return leg), 6027 (device return span), the 6021 budget kernel. Structural
/// and name-free: the test is `::DC` present ∧ second member absent. A row
/// carrying no `::DC` is not a crossing at all (a passive leaf's bare `N = GND`,
/// whose direction belongs to the parent module port) and never reaches this
/// check, so no direction word is demanded of it.
pub const POWER_PIN_RETURN_MISSING: u32 = 6030;

/// PWR-6 (exposed-protection-design.md §3, ruled 2026-09-16): a port row
/// declaring `@exposed(<threat>)` puts its net at the board's transient
/// boundary, and a boundary net must carry a declared clamp onto a
/// `@role(protective)`/`@role(earth)` reference. Coverage is the declaration,
/// not the topology alone: an ordinary decoupling capacitor or series resistor
/// onto the protective island is not a clamp (the PI axis owns those), so the
/// covering device must sit on both the exposed net and the clamped reference
/// net while the same scope declares `@clamp(<that ref>)`. The reference's role
/// failing is PWR-7's (6008) verdict, not repeated here — this fires on the
/// missing clamp only.
pub const EXPOSED_NET_NO_CLAMP: u32 = 6031;

/// PWR-5 (exposed-protection-design.md §4, ruled 2026-09-16): a class whose
/// definition body declares `protect = shunt` says it dumps the transient it
/// exists for onto a reference — so at least one of its legs must land on a
/// reference the owning scope declares `@role(protective)`/`@role(earth)`.
/// Without that leg the declaration names an obligation the device cannot
/// discharge. The classification is the declaration alone (a fuse and an
/// ordinary copper pass are structurally identical two-terminal elements), and
/// whether that reference is *legitimate* — role correct, island single-point —
/// stays PWR-7/PWR-8's verdict, not repeated here.
pub const PROTECT_SHUNT_NO_REFERENCE: u32 = 6032;

/// PWR-5 (exposed-protection-design.md §4, ruled 2026-09-16): a class whose
/// definition body declares `protect = series` says it is an in-line
/// protective element (fuse/PTC), so it must actually be one — a two-terminal
/// element whose ends sit on two different nets, both on a supply tree — the
/// fed face (6019), not the budget root: a capacity-less source boundary is
/// deliberately opaque to the budget walk, so the budget root would call an
/// ordinary fuse downstream of such a source off the supply tree. Either
/// failure makes the placement meaningless: both ends on one net means the
/// device bypasses itself, and an end on no supply tree means it protects
/// nothing. The declared order (which side of the protected device the element
/// sits on) is deferred — §6 R4 owns it.
pub const PROTECT_SERIES_NOT_IN_PATH: u32 = 6033;

/// §3.1 (ac-axis-interface-design.md, ruled 2026-09-16): a domain's
/// `@nature(ac|dc)` word and the `::AC*`/`::DC` contract of a rail row declared
/// inside it name the same axis, so when both are written they must agree. The
/// two are written in different vocabularies — a lowercase value word against a
/// `::` iface name — so the verdict maps each side onto an axis rather than
/// comparing spellings. Reported `Info` and declaration-local, at the
/// contradicting rail row: a domain whose word is absent states the axis through
/// its rail contracts alone (intent-design.md §5.2 default), and one whose word
/// is outside the registered `{ac, dc}` vocabulary names no axis here at all
/// (that misspelling is the value-word vocabulary's verdict, not this rule's).
pub const RAIL_NATURE_MISMATCH: u32 = 6034;

/// PI-2 (power-quality-design.md §2.2, ruling 11 decided 2026-09-17): the
/// load-side decoupling a declared filter bridge owes. A `@bridge` whose two
/// endpoints are both hot faces of declared rails is a supply filter leg — the
/// ferrite alone is not the filter, it is the series half of one, and the LC
/// only exists once the load side carries a decoupling element. The **load
/// side** is the endpoint whose domain worlds read as a quiet/sensitive face
/// (§1.4: `@class(analog)`, `@noise(quiet)`, `@noise(sensitive)`) — the
/// declaration says which side is being protected, so no arrow order and no
/// upstream/downstream inference is needed (ruling 3).
///
/// Ruling 11 (2026-09-17) cuts this rule's face to **existence only**: the net is
/// a declared rail's hot member and *a* capacitor sits on it, or nothing does.
/// Where that capacitor's return leg lands is PI-3's object (`6038`,
/// `nets/decouple.rs`) — the same one-fact-one-code partition ruling 8 gave the
/// 6022 seam, so a mis-landed return reports 6038 alone instead of the same
/// defect twice.
///
/// Both endpoints are read through the net's **effective class** (the axis's one
/// read), so a bridge written in a parent scope whose load-side net is owned one
/// level down still resolves, and the candidate capacitor is read from the flat
/// carries (`element_class == Capacitive`), never from a name.
///
/// Error: `@bridge` is the declaration, and it names a filter leg — declaring the
/// series half while the load side carries no decoupling is the declaration
/// contradicting the topology, the same shape PWR-5/PWR-6 judge.
///
/// Not judged, never guessed (design §1.3): a bridge whose endpoints are not both
/// hot faces (a ground-side bridge — both ends on return faces — is not a supply
/// filter leg), one on which neither endpoint reads a quiet/sensitive face (§2.2
/// ruling 3: the declaration is the only witness, and with no quiet side there is
/// no load side), one whose two endpoints *both* read quiet (the load side would
/// be a guess), an endpoint whose class does not resolve, and a `@couple` edge
/// (a DC-blocking coupling element is not a filter leg).
pub const BRIDGE_LOAD_DECOUPLING_MISSING: u32 = 6037;

/// PWR-4b (package-thermal-design.md §3 shunt / §7 series, ruled 2026-09-16 and
/// 2026-09-17): the package's own dissipation ceiling against the power the
/// element actually dissipates in place. Both faces take the same candidate — a
/// declared class that is resistive (`spec.resistance`, the ledger's dissipating
/// certificate), two terminals on two different nets, a positive resistance and
/// a decodable `power_rated` — and differ only in how the current through the
/// part is read. A **shunt** across a declared rail carries a *declared* voltage
/// (the rail window's far corner `max(|lo|, |hi|)`), so `P = V^2/R` needs no
/// solver. A **series** element carries the demand of the region that loses its
/// feed once the part is cut out of the copper (the removal method), so
/// `P = I^2R` over a sum the budget engine reports identically. The declared
/// rating is compared as-is: the design's `derating_factor` stays 1.0
/// (rail-contract-design.md §8.6, the same ruling that keeps a derate
/// multiplier out of the budget axis). Advisory Warning: the verdict compares
/// two declarations rather than a topology violation, and both readings are
/// deliberately the conservative ones (the far corner of the window; the whole
/// downstream region).
pub const SHUNT_DISSIPATION_OVER_RATING: u32 = 6035;

/// PI-3 (power-quality-design.md §2.3, ruled 2026-09-16): a decoupling
/// capacitor's two legs are **one declared DC pair**. The rail a part sits
/// across states that pair — `rail [hot, ret]::DC(…)` — and the cap is the
/// element whose job is to close the loop the rail declares, so its return leg
/// must land on that rail's `ret` member. The certificate is the element class
/// ([`crate::semantic::basic::attr_keys::ElementClass`] `Capacitive`, read off
/// the definition's spec table), never a name or a pin shape: a capacitor with
/// no spec table is not a decoupling capacitor here.
///
/// Both legs are read through the net's **effective class** — the same read the
/// whole axis uses — so the verdict holds across a module boundary (a part
/// instantiated in a sub-module whose leg the parent layer owns resolves to the
/// parent's class; reading the raw island attribution instead would treat every
/// sub-module net as unjudged). The comparison is class-to-class (`EffClass.id`),
/// not name-to-name: the sub-module net `vin.GND` and the parent's `GND` are one
/// fact.
///
/// Error, and deliberately not filtered by `@class(analog)`: the return leg is a
/// **structural position** and the DC pair is explicit, so a decoupling cap whose
/// return lands on another plane is wrong everywhere, not just on the analog face
/// (§9.1's wording comes from this being the general law). A leg whose class does
/// not resolve, and a part not sitting across a declared rail at all, are never
/// guessed — that silence is the family's standing rule (§1.3), not a pass.
pub const DECOUPLING_RETURN_MISMATCH: u32 = 6038;

/// PI-1 (power-quality-design.md §2.1, ruled 2026-09-16 §5 ruling 2): a pin that
/// **draws** from a declared DC pair carries no decoupling capacitor on its hot
/// net. The subject is the load terminal, not the filter: a `psnk` power pin (a
/// component pin row and a module supply port are judged alike — §6 R2, first
/// landing includes ports) whose net is the hot member of a declared pair.
///
/// The pair is read from the pin's own **declared member** ([`InstEntry::pwr_member`]
/// — the flat carry of the `::DC` row that owns it), never from a name: a pin whose
/// row writes no pair, or whose net resolves no declared face, is not judged
/// (§2.1's "a rail with no return member" face — a single-phase AC shape belongs to
/// axis ④). The candidate is the flat element class (`Capacitive`, two terminals on
/// two distinct nets), never a name.
///
/// **Existence only (ruling 11's partition, applied here by the same reason)**:
/// whether a capacitor sits on the sink's hot net; where that capacitor's *return*
/// leg lands is PI-3's object (`6038`) and nowhere else. Without that cut a
/// mis-landed return would be reported twice — once as this code's "no decoupling"
/// and once as `6038`'s mis-placement — for one cause, the shape rulings 8 and 11
/// each cut for their own seam.
///
/// Warning (ruling 2, 2026-09-16): nothing in the declaration says "this rail must
/// be decoupled", so this reports design quality, not a contract breach — the same
/// level as PWR-2's return-completeness audit. The error alternative required
/// bulk capacitors on the golden board's `VMAIN_5V` / `V5V_ISO` gaps; the ruling
/// took the advisory instead.
pub const SINK_PIN_NO_DECOUPLING: u32 = 6036;

/// SN-3 (power-quality-design.md §3.3, ruling 10 decided 2026-09-16): a part
/// supplied from a **quiet/sensitive** face (§1.4: `@class(analog)`,
/// `@noise(quiet)`, `@noise(sensitive)`) whose declared DC pair returns into a
/// **noisy** face (`@noise(noisy)`). The plane a protected part returns to is
/// part of its protection — it is the reference the sensitive signal is
/// measured against — so landing that return on a noise source's own reference
/// puts the signal back onto the copper the quiet face was isolating it from.
///
/// The subject is a **declared pair of a part**: both members are read from one
/// `pins.pwr` row, so the return judged is the return *of the pair that was
/// declared*, and the two faces come from the net's own attribution against the
/// words the declaring scopes wrote ([`crate::semantic::validation::nets`]'
/// §1.4 read, shared with PI-2/PI-4/SN-2). A part whose definition declares no
/// pair carries no witness — a two-terminal passive's return placement is
/// `6038`'s object, and a *bridged* coupling between the two faces is `6040`'s.
///
/// Error (design §3.3): the landing is direct — no bridge, no filter — and a
/// protected face's reference is a declaration the topology contradicts, unlike
/// `6022`/`6027`'s advisory forgotten declaration. Ruling 10 measured the seam
/// with `6027`: that rule needs a ≥3-pin device's returns **spanning ≥2
/// classes**, so this rule's target shape (one sensitive return pin landing on
/// one wrong class) is no span at all — 6027 stays silent there, and the two
/// only coincide with different witnesses.
pub const SENSITIVE_RETURN_ON_NOISY: u32 = 6041;

/// SN-1 (power-quality-design.md §3.1, drafted 2026-09-16): a part supplied
/// from a quiet/sensitive face returns over a reference that face and the
/// scope's analog port row do not agree on. The subject is the **declaration
/// against the topology**, and the declaration is two agreeing statements: the
/// face's own rail (`rail [hot, R]::DC(…)`) and the port row's `@return(C)`,
/// which must name `R`. Only where they agree is the wiring judged — the
/// effective class of the net the part's return member lands on
/// ([`crate::semantic::validation::nets`]'s axis read), so the verdict needs no
/// name match and a sub-module leg bound to the parent's return still counts.
/// That guard is what keeps the verdict
/// per-face: a scope declaring two quiet faces with different references has one
/// declaration per face, so a port naming the first never judges the second's
/// parts. The parts a face protects are the ones it supplies, which is how
/// §3.1's "sink-side part" is reached without the source→sink chain the flat
/// table does not carry (§6 R3: that chain is the deferred half of this rule).
///
/// Error, as design §3.1 lists it: the same level as PWR-5/6 — a declaration
/// the topology contradicts. The PI leaves (6036-6038) judge a filter and a
/// load, SN-3 judges a pair *across* the two §1.4 faces; this judges the
/// reference a face is declared with, so the three verdicts stand on different
/// witnesses and cut the same board without overlapping.
pub const ANALOG_RETURN_MISMATCH: u32 = 6039;

/// SN-2 (power-quality-design.md §3.2, ruling 9 decided 2026-09-16): a declared
/// DC `@bridge` whose two ends are the **returns** of a noisy face (§1.4
/// `@noise(noisy)`) and of a quiet/sensitive one, carried by anything other
/// than a magnetic element — the two references meeting through plain copper,
/// with no filter to let the quiet face keep its own reference while they meet.
///
/// The two ends are returns by **negating PI-2's supply-leg test**: a bridge end
/// that is a declared rail's hot member makes the leg a supply filter (6037's
/// object), so a leg with neither end hot is the ground-side one this rule
/// judges. Both ends are read at the **name** level — the clause names nets of
/// the scope that wrote it, and that scope's own DC rails say what each name is
/// (a hot member, or a domain's return). Reading the written name rather than the
/// net's effective class is what lets the plainest form of the defect, a direct
/// copper tie (ruling 9's plain direct tie), be judged: a tie that merges the two
/// coppers collapses to one class answering both faces, while the declaration
/// still names two returns.
///
/// No filtering intent is ruling 9's reading (a) negated: the leg's carrier is
/// the two-terminal part whose legs land on exactly the two classes the bridge
/// joins (the design's bridge-carrier element), and a magnetic element there is a declared
/// filter — read from [`crate::semantic::basic::attr_keys::ElementClass`], never
/// a name. A filter that is declared but incomplete is PI-2's verdict (ruling
/// 11's partition), so one cause reports one code. A leg no single element
/// carries is reported with the message saying so; a leg carried by a *chain* of
/// elements is read the same way — no single bridge element is magnetic — and is
/// named as carried by no single element.
///
/// Error (§3.2): P7 lists the shared return as reportable, the same level as the
/// axis's other declaration-vs-topology cuts. The rule reads the **domain** face
/// only — ruling 1's part-level noise source has no flat consumer yet (see
/// [`crate::semantic::validation::nets`]' §1.4 read) — so a board whose only
/// noise source is a part definition body stays silent.
pub const SHARED_RETURN_BRIDGE: u32 = 6040;

/// PI-4 (power-quality-design.md §2.4, ruling 4 decided 2026-09-16): a sink
/// drawing from a declared filter leg's **load-side subface** declares a supply
/// pair that is not the quiet domain's own pair.
///
/// §2.2's supply leg protects a load side, and that side is a subface: the
/// quiet/sensitive domain's own declared pair, its hot member's copper plus its
/// return member's copper. The verdict is **member by member** on the sink's own
/// declared pair — its hot terminal's copper must be the rail's hot member and
/// its return member the rail's return — so a part that touches the subface with
/// only one member of its pair (supply from another domain's hot copper, or
/// return off the subface) is judged, and a part inside the domain is not. Both
/// sides of the comparison are **class ids**
/// ([`crate::semantic::validation::nets`]'s axis read), never spellings.
///
/// The pair is read **where it was bound**: the flat carries only the declaring
/// scope's member *spellings* (the flatten pass's member carry), so the pair is
/// located on the instance that owns the terminals — two instances of one class
/// at different call sites are judged apart, which a definition-space read could
/// never do. The prerequisite is §2.2's leg: a bridge whose ends are both hot
/// faces of declared rails and whose load side is the one quiet end. Undeclared
/// ⇒ no supply leg exists and this rule does not apply; declared ⇒ `6022` is
/// silenced by its own per-leg span match. No configuration makes both fire on
/// one witness, which is ruling 4's reason for an independent code.
///
/// Error (§2.4): the declaration and the topology contradict each other — the
/// same level as the axis's other cuts. Not judged, never guessed: a bridge that
/// is not a supply leg, a leg with no quiet side or two quiet ones (PI-2's own
/// silence), a rail whose return member resolves no class (no subface), a pair
/// whose members the instance does not carry, a terminal landing on no class,
/// and a pair touching no member of the subface at all. A part drawing from the
/// quiet face's analog input rather than from a supply terminal declares no
/// supply pair and is not this rule's object.
pub const FILTER_SUBFACE_OVERREACH: u32 = 6042;

/// PWR-6 **downstream chain** (exposed-protection-design.md §3.1, the upgrade
/// half; six rulings 2026-09-17): the exposed port's own copper is clamped
/// (`6031` silent), but an **unprotected quiet/sensitive face** is reachable
/// from it through transparent copper without crossing a declared series gate —
/// the transient the `@exposed` declares takes the branch the clamp does not
/// cover.
///
/// The read is a **region existence** test, not a path search
/// (`exposed-protection-design.md` §3.1.4): flood the current-transparent
/// copper body of the exposed port's own segments (the same walk `reach.rs` /
/// `budget.rs` / `budget_derive.rs` / `window.rs` each carry, here one shared
/// helper), stopping before `Ret`/`Reference` copper and **stopping at every
/// declared gate** (`InstEntry.protection == Some(Series)` — a fuse / PTC /
/// ferrite is the current-limit chain the canon's "already past a clamp or
/// current-limit chain" names).
/// Report when some net of that region, other than the port's own copper,
///
/// ```text
/// is a quiet/sensitive face   (the §1.4 read, via eff_class worlds)
/// and carries no declared clamp of its own.
/// ```
///
/// A gate is only ever a **declaration** (`protect = series`), never a shape:
/// an unmarked two-terminal pass element is ordinary copper and does not stop
/// the flood — the same "a declaration is the contract" rule PWR-5's
/// classification rests on.
///
/// Not judged, never guessed: a port whose segments do not resolve in its own
/// scope; a region net with no owning layer, no resolvable class or no declared
/// face (silence, never a guess — §1.3); a region net that is itself clamped
/// (its own `6031`-shaped coverage); and a board declaring no faces at all.
/// **Never stacked with `6031`**: that rule's object is the exposed net itself
/// and this one's is the region *minus* it, so an exposed net carrying no clamp
/// fires `6031` alone (object disjointness, §3.1.6).
pub const EXPOSED_NET_DOWNSTREAM_UNPROTECTED: u32 = 6044;

/// R4 **name collision** (intent-reference-layer-design.md §10.5 step 3,
/// §10.11.4 guard ④): a bare name in a chain word position is *both* a
/// whole-referenceable domain of the owning module (step 1 — exactly one `::DC`
/// rail, so the name states a `[hot, ret]` pair) *and* an endpoint already
/// declared in that scope (step 2 — instance / port / label). The two readings
/// name different nets, so the written word has no single meaning; letting the
/// reading order decide would be a silent pick, which is the one outcome this
/// layer forbids. **Report, never choose.**
///
/// Only a bare single identifier can raise this: a dotted / bracketed / braced
/// word position has its own reading (§10.11.4 guard ①), so a name that merely
/// carries a domain name as a prefix or element is out of this code's object.
pub const DOMAIN_ENDPOINT_NAME_COLLISION: u32 = 6050;

/// Pin copper expectation (pin-expectation-design.md §3, v0.1): a component
/// pin row carrying an expectation states the identity the pin expects to
/// land on — `@role(<word>)` for the copper-identity axis (the identity
/// words, v0.1: `quiet` is the one with a net-side reading) or
/// `@class(analog|digital)` for
/// the signal-class axis (v0.3 §4). The component layer cannot name a conduit
/// (conduit does not cross layers), so the word is an expectation and the
/// module layer's binding is the witness: the landed net's potential class
/// must anchor the expected identity — through a declared domain face (the
/// §1.4 read) or, on the identity axis, through the copper conduit's own
/// `@role` word. A class that anchors neither contradicts the expectation.
/// The contradicted leg is always Warning (the former @req strength tier is
/// retired — group-wise physical facts move to the barrier axis, U175).
pub const PIN_COPPER_EXPECTATION_MISMATCH: u32 = 6051;

/// The unanchored half of the same expectation: the landed net resolves no
/// potential class at all (a bare net — no conduit copper, no rail
/// membership), so there is no identity to compare against. Info, because
/// the expectation is unmet by absence rather than contradicted — an empty
/// reading is still a reading, and it is judged only where the pin row
/// actually declares one. This half stays Info regardless of anything the
/// part declares (unprovable ≠ violated — a board that never splits domains
/// is not forced into it).
pub const PIN_COPPER_EXPECTATION_UNANCHORED: u32 = 6052;

/// Cross-barrier merge (rules-catalog §2 B5's declarative subject,
/// barrier-design.md §3): pins of two **different** `@barrier` groups on one
/// component share a net. Isolation is a relation between pin groups —
/// `@barrier(pri)` on the primary rows and `@barrier(sec)` on the secondary
/// rows say "no net of this part may touch both groups" — so one net reaching
/// two groups is the schematic-level fact that the isolation is bridged (an
/// isolation transformer wired as an autotransformer, a secondary ground
/// returned on primary copper). Group names are compared by equality only;
/// unmarked pins are outside every barrier and judge nothing. Error by birth
/// (a physical fact between groups), never a tier of anything: the gate reads
/// its own axis and never the single-ended expectation axes (6051/6052).
/// A deliberate cross-barrier part (Y capacitor, feedback optocoupler) splits
/// the net in two and never fires — the gate judges one net, not a path.
pub const CROSS_BARRIER_NET: u32 = 6053;

/// U201 ①② (xtal-oscillator-design.md §2, role-anchored exclusive-peer
/// gate): one adoption lane of a role declaring `exclusive = true` reaches
/// more than one peer-role instance across its terminals. The law is a body
/// fact — a resonator body's terminals meet exactly one oscillator body
/// (`X1` onto one MCU and `X2` onto another is a torn pairing, each single
/// net quietly passing the point-to-point count). Judged from the flat table
/// over whole nets, and only where the declaration states the exclusivity:
/// a role without `exclusive` pairs unrestricted. One terminal with no peer
/// at all is the single-side silence law, not this gate's defect.
pub const IFACE_EXCLUSIVE_PEER_CONFLICT: u32 = 6054;

/// U217 ② (ac-interface-design.md §7, the AC return gate): a direction-word
/// `psrc/psnk/psbi` row declares an `::AC.*` face whose two members are the
/// pair — the supply face plus the return it closes over. One member wired
/// while its partner reaches no net is a single-line supply: the return is
/// the conductor the working current comes home on, and no consumer of the
/// face can read a half of it. Judged per face over the flat table (the
/// defect *is* the absent net, so a net walk cannot see it), only where a
/// direction word declares the face; a face whose members are all dangling
/// is an unused declaration and stays silent, and unwired *pins* stay the
/// unconnected-pin warning's object.
pub const AC_FACE_RETURN_MISSING: u32 = 6057;

/// U217 ④ (ac-interface-design.md §7, the AC nominal gate): two `::AC.*`
/// faces on one copper state different region nominals — 230 V / 50 Hz and
/// 120 V / 60 Hz are not one mains, and the copper cannot be both. The DC
/// twin is the declared-voltage mismatch gate (the 0.5 V band included); the
/// frequency axis compares exactly. The empty form `::AC.1P()` states no
/// nominal — a region-neutral face conflicts with nothing and is outside the
/// judge.
pub const AC_NOMINAL_CONFLICT: u32 = 6058;

/// U217 ③ (ac-interface-design.md §7, the protective-word gate): a pin row's
/// `@role(protective)`/`@role(earth)` word demands protective copper — the
/// net the pin lands on must touch a conductor the owning scope declares
/// with that role. The word is the demand, never the witness: another
/// role-marked pin on the same net does not satisfy it, only a declared
/// protective/earth conductor does. An unwired pin is the unconnected-pin
/// family's object, never this gate's.
pub const PROTECTIVE_PIN_NO_COPPER: u32 = 6059;

/// U112 ② **chain-level source reach** (clock-intent-design.md §2.2): a role
/// whose member pins all declare `in` — a sink-shaped lane of a
/// unidirectional family — reaches no source endpoint of its own family
/// along the adoption chain. The walk crosses whole nets and distributor
/// instances; the defect is the orphan (zero reachable sources), never a
/// second source (multi-input receivers are a legal shape). Triggered by the
/// role's declared pin-direction shape, never a family or role name.
pub const IFACE_CHAIN_SOURCE_UNREACHED: u32 = 6060;

/// E4 flat-net generic peer sweep (replicated-binding-design.md §4 check 4):
/// two role-bearing interface endpoints of the same family land on one flat
/// net (possibly through role-less mediators or module ports — both are
/// role-less conductors by law) and their roles are not mutual peers per the
/// interface's role-block `peer` table. The connection-time judge E4121 only
/// sees pairs meeting in one statement; this sweep walks the finished net
/// map, so a pair already reported by E4121 is reported again here by ruling
/// (2026-09-25: both fire, no dedup carry). Pairs with an unknown role on
/// either side stay silent (the single-side law shared with 6054).
pub const IFACE_ROLE_PEER_CONFLICT: u32 = 6061;

/// R3 **mixed bridge identity** (intent-reference-layer-design.md §10.4 bridge
/// identity three-state): a `@bridge(X, Y)` whose two arguments disagree on
/// kind — one names a whole-referenceable domain of the owning module, the
/// other names a plain net/copper endpoint. A domain half reads as a directed
/// rail member, an endpoint half as one conductor, so the pair states no single
/// crossing. **Report, never choose**: the statement keeps today's reading (the
/// attribute text pair is consumed verbatim by the net-level bridge rules),
/// only the domain-level license is withheld.
///
/// Two domain arguments (the licensed shape) and two non-domain arguments
/// (today's net-level bridge, unchanged) are both out of this code's object.
pub const DOMAIN_NET_MIXED_BRIDGE: u32 = 6046;

/// R3 **direction reversal** (intent-reference-layer-design.md §10.4 member
/// take, the mirrored writing is not re-read): the `@bridge(A, B)` argument order disagrees with
/// the written left-to-right order of the two domain words on the licensed
/// chain. The same crossing can be written mirrored, and re-reading the chain
/// from the other end to make the orders agree would be a silent rewrite —
/// so the reversed writing is reported, not reinterpreted.
///
/// Member resolution itself is unaffected: each domain word takes its member
/// from its own arrow direction (`->` hot / `<-` ret), which does not depend on
/// the argument order, so the statement stays licensed and this code is the
/// batch's only effect.
pub const DOMAIN_BRIDGE_DIRECTION_REVERSED: u32 = 6047;

/// R3 **leg inconsistency** (intent-reference-layer-design.md §10.4 witness):
/// on a statement licensed by `@bridge(A, B)`, the chain's two end words land
/// on opposite sides of the named pair — one on a hot member (a domain word
/// under a `->` chain, or a literal naming `A.hot`/`B.hot`) and the other on a
/// return member. A hot leg carrying a return-copper pin (or vice versa) is a
/// chain that does not state one crossing, so the member chain is inconsistent.
///
/// An end word that names neither member of the pair is *not* this code's
/// object — it simply witnesses nothing (§10.4 judges only hot-vs-return
/// placement); a pair left with no witness at all is 6049's object.
pub const DOMAIN_BRIDGE_LEG_INCONSISTENT: u32 = 6048;

/// R3 **dangling bridge** (intent-reference-layer-design.md §10.4 collection
/// completeness): a `(A, B)` domain pair named by a licensed `@bridge` across
/// the whole module body, but no licensed chain anywhere witnesses a leg of it
/// — every chain it names lands on words outside the pair's members. A named
/// crossing with zero witnessed legs declares a relation nothing realizes.
///
/// Per pair, not per statement: one witnessed leg (hot or return, §10.4 "at
/// least one leg") covers every statement naming the same pair; a single-sided
/// bridge is the undecided one-way question (§8 open 2) and is not judged here.
pub const DOMAIN_BRIDGE_DANGLING: u32 = 6049;

// ── 9xxx: the acceptance (fulfillment) family ──
// The static `expects` acceptance engine (circuit-intent-acceptance-design.md
// §4): one verdict per row of the top module's ledger, judged on the frozen
// flat world. Legality lives in 4xxx/5xxx/6xxx; these codes say the built top
// does not fulfill what the author asked it to fulfill. Structural rows
// (absence / class mismatch / not-driven) are errors; a value-window conflict
// is a design-choice report (warning). A row with no declared fact to compare
// against is DEFER — a verdict, never a diagnostic.

/// The row's target is absent (or of a kind the row form cannot address) in
/// the built top: a class row names no instance, a driven/window row names no
/// net. Only what the top instantiates or declares can be expected of it.
pub const EXPECTATION_TARGET_MISSING: u32 = 9001;

/// The target instance exists but carries no declared face matching the row's
/// class word. The expectable faces of an instance are its class itself, its
/// variant base chain, and its adopted recipes — the same keys `::`
/// binding reads.
pub const EXPECTATION_CLASS_MISMATCH: u32 = 9002;

/// The target net exists but carries no declared driver: no endpoint on it is
/// an `Out` pin or a declared power source (the same declared-face rule the
/// undriven-net gate reads).
pub const EXPECTATION_NOT_DRIVEN: u32 = 9003;

/// The target net declares a DC rail value that conflicts with the row's
/// window — a design-choice report (warning), not a wiring defect.
pub const EXPECTATION_VALUE_OUT_OF_WINDOW: u32 = 9004;

static ALL_CODES: &[ErrorCodeInfo] = &[
    // section
    entry!(DUP_INTERFACE, "An interface with the same name already exists in this file.", "Duplicate interface"),
    entry!(DUP_COMPONENT, "A component with the same name already exists in this file.", "Duplicate component"),
    entry!(DUP_ENUM, "An enum with the same name already exists in this file.", "Duplicate enum"),
    entry!(DUP_MODULE, "A module with the same name already exists in this file.", "Duplicate module"),
    entry!(DUP_RECIPE, "A recipe with the same name already exists in this file.", "Duplicate recipe"),
    // section
    entry!(DEF_ALREADY_EXISTS, "Definition already exists.", "Definition already exists"),
    entry!(INST_MISSING_SUBNODE, "Missing subnode in an instance declaration.", "Missing subnode in an instance declaration."),
    entry!(PINS_MISSING_SUBNODE, "Missing subnode in a pins declaration.", "Missing subnode in a pins declaration."),
    entry!(ENUM_MISSING_SUBNODES, "Enum definition is missing its subnodes.", "Missing subnodes for enum"),
    entry!(ENUM_MISSING_NAME, "Enum definition is missing its name.", "Missing name for enum"),
    entry!(ENUM_MISSING_NAME_IDS, "Enum definition is missing its name ids.", "Missing name ids for enum"),
    entry!(ENUM_MISSING_VALUES, "Enum definition is missing its values.", "Missing values for enum"),
    entry!(MALFORMED_IOTYPE, "Malformed IO type node in a pin/port declaration.", "Malformed IOTYPE node"),
    // section
    entry!(PHRASE_COMPONENT_MEMBER_NOT_FOUND, "Member access on a component operand matched none of the requested members against the component's pins.", "Member access on the component operand: none of the requested members matched a pin, so the access resolves to nothing."),
    entry!(PHRASE_MODULE_MEMBER_NOT_FOUND, "Member access on a module operand matched none of the requested members against the module's ports.", "Member access on the module operand: none of the requested members matched a port, so the access resolves to nothing."),
    entry!(PHRASE_INTERFACE_MEMBER_NOT_FOUND, "Member access on an interface operand matched none of the requested members against the interface's pins.", "Member access on the interface operand: none of the requested members matched a pin, so the access resolves to nothing."),
    entry!(PHRASE_MEMBER_ON_SERIES, "Member access on a chain operand: a chain has no member list to index, so the access is unsupported.", "Member access on a chain (Series) operand is unsupported: a chain has no member list to index."),
    entry!(PHRASE_MEMBER_ON_NODE, "Member access on a node operand: a node exposes faces, not named members.", "Member access on a node operand is unsupported: a node exposes faces, not named members."),
    entry!(PHRASE_MEMBER_ON_TRANSPOSED, "Member access on a transposed operand: transpose is a view over a chain, which has no member list.", "Member access on a transposed operand is unsupported: transpose is a view over a chain."),
    entry!(PHRASE_MEMBER_ON_LEAD, "Member access on a '_' lead placeholder: a lead carries no member.", "Member access on a '_' lead placeholder is unsupported: a lead carries no member."),
    entry!(PHRASE_MEMBER_ON_GROUP, "Member access on a group operand: a group is expanded at statement level and has no member list.", "Member access on a group operand is unsupported: the group is expanded at statement level."),
    entry!(PHRASE_CLOSURE_EMPTY_OUTPUT, "Member access on a closure whose output interface is empty; there is no interface to search.", "Member access on a closure with an empty output interface is unsupported."),
    entry!(PHRASE_FUNCALL_EMPTY_OUTPUT, "Member access on a function call whose output interface is empty; there is no interface to search.", "Member access on a function call with an empty output interface is unsupported."),
    entry!(PHRASE_MEMBER_ON_ENDPOINT, "Member access on this endpoint kind: it has no member list.", "Member access on this endpoint kind is unsupported: it has no member list."),
    entry!(PHRASE_MEMBER_ON_MEMBER, "Member access on a member reference: it has no member list of its own.", "Member access on a member reference is unsupported: it has no member list of its own."),
    entry!(PHRASE_CURLY_EMPTY_LEFT, "Curly member access with an empty left member list; there is nothing to pair.", "Curly member access has an empty left member list, so there is nothing to pair."),
    entry!(PHRASE_CURLY_EMPTY_RIGHT, "Curly member access with an empty right member list; there is nothing to pair.", "Curly member access has an empty right member list, so there is nothing to pair."),
    entry!(PHRASE_CURLY_UNSUPPORTED_OPERAND, "Curly member access met an operand kind that cannot be converted to node elements.", "Curly member access met an operand kind it cannot convert to node elements."),
    // section
    entry!(USE_PATH_INVALID, "Invalid path in a use statement.", "Invalid path in USE"),
    entry!(USE_URI_PREFIX_INVALID, "Unrecognized URI prefix — expected $, /, ./, or ../.", "Unrecognized URI prefix — expected $, /, ./, or ../"),
    entry!(USE_TARGET_NOT_FOUND, "The use target file was not found.", "use target not found: {0}"),
    entry!(USE_SELF_IMPORT, "File imports itself via a use statement.", "File imports itself via a use statement."),
    entry!(USE_ALIAS_COLLISION, "A use alias collides with an existing name.", "A use alias collides with an existing name."),
    entry!(USE_VERSIONED_TARGET_NOT_FOUND, "The versioned use target file was not found.", "The versioned use target file was not found."),
    entry!(USE_IMPORT_SYMBOL_NOT_FOUND, "A symbol listed in use import(...) was not found in the target file.", "A symbol listed in use import(...) was not found in the target file."),
    entry!(USE_REEXPORT_SYMBOL_NOT_FOUND, "A symbol in pub use import(...) was not found and cannot be re-exported.", "A symbol in pub use import(...) was not found and cannot be re-exported."),
    entry!(USE_TRAILING_NODE, "Unexpected trailing node in a USE statement; it is ignored.", "unexpected trailing node {0} in USE statement; it is ignored"),
    // section
    entry!(USE_DEP_NOT_DECLARED, "Use of an undeclared dependency — add it to project.toml [dependencies] or load via --lib.", "use of undeclared dependency '{0}': add it to project.toml [dependencies] or load via --lib"),
    entry!(USE_LIB_NOT_FOUND, "The library is not installed in the system root — install it with `mcc lib install` or load it with --lib.", "library '{0}' not found in the system root; install it with `mcc lib install` or load it with --lib"),
    entry!(USE_SYMBOL_CONFLICT, "An imported symbol conflicts with an existing name.", "symbol conflict in module '{0}': {1} collides with previous use from '{2}'. Use 'as' alias to disambiguate"),
    entry!(USE_IMPORTED_NOT_FOUND, "The imported symbol was not found in the target file.", "imported symbol '{0}' not found in '{1}'"),
    // section
    entry!(PARSER_SYNTAX_ERROR, "Generic syntax error.", "Generic syntax error."),
    entry!(PARSER_TOP_INVALID, "Invalid top-level declaration.", "Invalid top-level declaration."),
    entry!(PARSER_CLAUSE_INVALID, "Invalid clause in a body.", "Invalid clause in a body."),
    entry!(PARSER_PIN_INVALID, "Invalid pin declaration.", "Invalid pin declaration."),
    entry!(PARSER_PIN_ID_NOT_CONST, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PIN_NAME_NOT_CONST, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_NET_NOT_PORT, "Net endpoint must be a port/label, not a literal.", "Net endpoint must be a port/label, not a literal."),
    entry!(PARSER_NET_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_CONDS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_ROLE_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_FUNC_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PINS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_USE_INVALID, "Invalid import statement.", "Invalid import statement."),
    entry!(PARSER_CONDBLOCK_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_DECLAREB_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_BODY_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_JUDGE_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PARD_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_URI_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PHRASES_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_OPDS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PARAMS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PARDS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_ATTR_VALUES_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_ATTR_LINES_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PINS_NAMES_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_INSTS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_CONDS_ELIFS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_IDSS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_LEVELS_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_PHRASE_INVALID, "Retired: no producer.", "Retired - no producer. Reserved for a per-production parser arm that was never written; the grammar fires its recovery codes instead."),
    entry!(PARSER_SINGLE_OR, "Single '|' used as a binary operator outside a pin context.", "Single '|' used as a binary operator outside a pin context."),
    entry!(PARSER_PLUSMINUS, "'±' used as a binary operator outside a tolerance context.", "'±' used as a binary operator outside a tolerance context."),
    entry!(PARSER_TRANSPOSE_ON_LITERAL, "Transpose (') on a literal has no effect.", "Transpose (') on a literal has no effect."),
    entry!(PARSER_CARET_ON_LITERAL, "Caret (^) on a literal has no effect.", "Caret (^) on a literal has no effect."),
    entry!(PARSER_EMPTY_BODY, "Empty body — no clauses defined.", "Empty body — no clauses defined."),
    entry!(PARSER_EMPTY_PINS, "Empty pins declaration.", "Empty pins declaration."),
    entry!(PARSER_TATTR_RESERVED_WORD, "Reserved word used as an attribute value.", "A grammar-reserved word (in/out/io/nc/psrc/psnk/psbi) cannot be an attribute value — reserved words are not names anywhere. Pick another spelling (the library uses input/output)."),
    entry!(AST_NODE_EMPTY, "AST node is null/empty where a value was expected.", "AST: Node is empty"),
    entry!(AST_UTF8_ERROR, "AST node contains invalid UTF-8 data.", "Invalid UTF-8 string"),
    entry!(AST_TYPE_MISMATCH, "AST node has an unexpected type.", "AST: Node type mismatch"),
    // section
    entry!(NAME_IDS_NO_NODES, "IDS has no nodes.", "IDS has no nodes."),
    entry!(NAME_MISSING_SUBNODE, "Missing subnode in a name reference.", "Missing subnode in a name reference."),
    entry!(NAME_DECLARE_PARSE_FAILED, "Failed to parse a DECLARE node.", "Failed to parse DECLARE"),
    entry!(NAME_SQUARE_VECTOR_MISSING_SUBNODE, "Missing subnode for a square vector.", "Missing subnode for square vector"),
    entry!(NAME_RANGE_SIDE_FAILED, "Failed to process a side of a range.", "Failed to process {0} side of a range."),
    entry!(NAME_DEF_NOT_FOUND_LABEL_FALLBACK, "Definition not found; falling back to a label.", "CURLY_MN: '{0}' definition not found, using label fallback"),
    entry!(NAME_ID_EXTRACT_FAILED, "Failed to extract ID/IDA data from a node.", "Failed to extract ID/IDA data"),
    entry!(NOT_SUPPORTED_YET, "This syntax is parsed but not yet supported by the semantic layer; the declaration is ignored.", "This syntax is parsed but not yet supported by the semantic layer; the declaration is ignored."),
    entry!(SYMBOL_NOT_FOUND, "Symbol could not be resolved to any definition after the full P1–P5 lookup chain.", "Cannot find '{0}'"),
    // section
    entry!(PIN_ID_NAME_MISMATCH, "Pin ID and pin name do not match.", "Pin ID and name not match"),
    entry!(PIN_ID_COUNT_ERROR, "Pin id count error.", "Pin id count error"),
    entry!(PINS_PLUS_WITHOUT_BASE, "pins += is used without a prior pins = definition.", "pins += used without prior pins = definition"),
    entry!(PIN_NAME_TYPE_UNSUPPORTED, "Pin name has an unsupported type.", "Pin name not support type"),
    entry!(PIN_NAME_COUNT_ERROR, "Pin/port name count error.", "Pin name count error."),
    entry!(PIN_FLAT_COUNT_MISMATCH, "Flat pin mapping requires equal pin and name counts.", "Flat pin mapping `[{0}]` gives {1} pin ID(s) but {3} name(s) `[{2}]`; pin↔name counts must match 1:1 for a flat (non-grouped) mapping. Add the missing name(s), or use nested groups `[[...]]` to broadcast one name over several pins."),
    entry!(PORT_NAME_TYPE_UNSUPPORTED, "Port name has an unsupported type.", "Port name not support type"),
    entry!(PORT_NAME_COUNT_ERROR, "Port name count error.", "Port name count error"),
    entry!(PIN_EXPR_TYPE_MISMATCH, "Pin expression node has an unexpected type.", "Pin expression node has an unexpected type."),
    entry!(ATTR_TYPE_MISMATCH, "Attribute node type mismatch.", "Attribute node type mismatch."),
    entry!(ATTR_TYPE_NOT_SUPPORTED, "Attribute type is not supported.", "Attribute type not support (node_type={0})"),
    entry!(ATTR_MISSING_SUBNODE, "Attribute node is missing a required subnode.", "Attribute node is missing a required subnode."),
    entry!(KVS_VALUE_TYPE_INVALID, "Retired: no producer.", "Retired - no producer. The KVS value decode reads typed values (UVAL family 3042+); the untyped catch-all this code reported is gone."),
    entry!(UVAL_VALUE_TYPE_INVALID, "Invalid unit value type.", "Invalid unit value type."),
    entry!(UVAL_DATA_NODE_INVALID, "Invalid unit value data node.", "Invalid unit value data node."),
    entry!(UVAL_UNIT_INVALID, "Invalid unit.", "Invalid unit."),
    entry!(UVAL_VALUE_INVALID, "Invalid unit value.", "Invalid value."),
    entry!(UVAL_UNIT_UNSUPPORTED, "The unit is not supported.", "Unsupported unit '{0}'."),
    entry!(UVAL_MISSING_DATA_NODE, "Missing unit value data node.", "missing unit value data node."),
    entry!(UVAL_FORMAT_INVALID, "Invalid unit value or float format.", "Invalid unit value or float format."),
    entry!(UVAL_UNIT_VARIANT_INVALID, "Invalid unit variant (angle, charge, magnetic flux, slew rate, ...).", "Invalid unit variant '{0}'."),
    // section
    entry!(MODULE_MISSING_SUBNODE, "Missing subnode in a module body clause.", "Missing subnode in a module body clause."),
    entry!(MODULE_PINS_UNSUPPORTED, "Module does not support PINS directly; use in/out/io declarations.", "Module does not support PINS directly. Use in/out/io declarations."),
    entry!(MODULE_ROLE_UNSUPPORTED, "Module does not support role definition.", "Module does not support role definition."),
    entry!(MODULE_PARAM_TYPE_UNEXPECTED, "Unexpected type in a module parameter.", "Unexpected type in module param"),
    entry!(MODULE_HEADER_IFACE_NEEDS_DIRECTION, "Module header interface-typed parameter is missing a direction word.", "module header interface-typed parameter (class `{1}`) in `module {0}` carries no direction word — write an explicit power direction `psrc`/`psnk`/`psbi`, e.g. `module {0}(psnk [VDD, GND]::DC(v))`; the no-direction header sugar is removed"),
    entry!(MODULE_METHOD_NOT_FOUND, "Function was not found in the class.", "function '{0}' not found in class '{1}'"),
    entry!(UNEXPECTED_CLAUSE_TYPE, "Unexpected clause type in a module body.", "Unexpected clause type in module body"),
    entry!(EXPECTS_ROW_MALFORMED, "A row of this expects clause is not one of the designed forms: a class/role word, `driven`, a [low:/high:] window, or a `~` range.", "A row of this expects clause is not one of the designed forms: a class/role word, `driven`, a [low:/high:] window, or a `~` range."),
    // section
    entry!(FUNC_EMPTY_NET, "Empty net in a function or module body.", "Empty NET"),
    entry!(PARAM_DECLARE_INVALID, "Invalid parameter declaration node.", "Invalid param declare node."),
    entry!(PARAM_NAME_INVALID, "Invalid parameter name.", "Invalid param name."),
    entry!(PARAM_SET_INVALID, "Invalid parameter set.", "Invalid parameter set."),
    entry!(PARAM_UVAL_INVALID, "Invalid parameter unit value.", "Invalid param uval."),
    entry!(PARAM_CLASS_EXPECTED, "Expected a class in the declaration unit value.", "Expected MCAST_CLASS in MCAST_DECLARE_UV."),
    entry!(PARAM_INSTANCE_EXPECTED, "Expected an instance in the declaration unit value.", "Expected MCAST_INSTANCE in MCAST_DECLARE_UV."),
    entry!(PARAM_NAME_EXTRACT_FAILED, "Failed to extract the parameter name.", "Failed to extract parameter name from MCAST_DECLARE"),
    entry!(PARAM_INST_LOOKUP_FAILED, "Instance::class lookup failed; the binding is treated as a plain pin alias.", "'{0}::{1}' lookup failed; treating '{0}' as plain pin alias. If you intended an interface binding, check that '{1}' is defined (and `use`d, if from a library)."),
    entry!(PARAM_DECLARE_IFACE_PINS, "Interface pin count does not match the number of declared pin IDs.", "Interface '{0}' declares {1} pin(s) (members: {2}) but {3} {4} given; the counts must match. Use a range like `a:b` to declare exactly {1} pin(s)."),
    entry!(FUNC_CALL_MISSING_NAME, "Missing function name in a function call.", "Missing function name in a function call."),
    entry!(CONN_STMT_PARSE_FAILED, "A connection statement failed to parse.", "connection statement failed to parse"),
    entry!(FUNC_BODY_INVALID, "Invalid function body node.", "Invalid function body node."),
    entry!(FUNC_STMT_DROPPED, "A connection statement was dropped because McPhrase::new returned None.", "Connection statement dropped (McPhrase::new returned None): `{0}`"),
    entry!(FCALL_PARSE_FAILED, "Function call parse failure.", "Cannot chain `.{0}` after `{1}(...)`: function `{2}` returns a bus/label (endpoint), not `this`. Only functions that return `this` can be chained."),
    entry!(FUNC_FLOATING_LABEL, "Net endpoint in a function body that resolves to nothing declared.", "`{0}` does not resolve to a declared pin, interface, parameter member, or instance of this component — floating label. If it is a local net, declare it (e.g. `RES R[1:2](...)`) or connect it to a component pin."),
    entry!(SINGLE_USE_INLINE_NET, "An inline ghost-net (reference base resolves to no declared instance) is referenced only once.", "`{0}` has no declared base and connects to nothing else — inline ghost-net referenced only once; declare it or fix the name."),
    // section
    entry!(INST_EXPR_PARSE_FAILED, "Failed to parse an instance in an expression context.", "Failed to parse MCAST_INSTANCE in expression context"),
    entry!(CURLY_MN_WRONG_BASE, "Curly-member construction requires a component or module base.", "CURLY_MN requires Component or Module"),
    entry!(INST_CLASS_NODE_MISSING, "No class node found in the instance declaration.", "No class node found"),
    entry!(INST_NODE_MISSING, "No instance node found.", "No instance node found"),
    entry!(INST_CLASS_ID_MISSING, "Missing class id node.", "Missing class id node"),
    entry!(INST_CLASS_IDS_PARSE_FAILED, "Failed to parse class ids.", "Failed to parse class ids"),
    entry!(INST_CLASS_UNRESOLVED, "Unresolved class — the library may not be loaded.", "unresolved class '{0}' — the library may not be loaded"),
    entry!(INST_NC_PIN_LIST_MISSING, "Instance NC marker with no pin list.", "`@{0}` lists no pins; write the pin ids to mark as not connected (e.g. `@{0}(1,3)`), or drop the marker."),
    entry!(INST_NC_PIN_VALUE_INVALID, "Instance NC marker operand is not a pin id.", "`@{0}(...)` operand `{1}` is not a pin id list; write pin ids, pin names, or an inclusive numeric range (`1:3`)."),
    entry!(FUNC_RETURN_MALFORMED, "Malformed return statement.", "Malformed return statement."),
    entry!(FUNC_RETURN_EXPR_INVALID, "Invalid return expression — expected this or a label/bus.", "Invalid `return` expression: expected `this` or a label/bus."),
    entry!(FUNC_MULTIPLE_RETURNS, "A function may have at most one return statement.", "Multiple `return` statements are not allowed; a function may have at most one return."),
    entry!(MODULE_RETURN_NOT_ALLOWED, "A module body cannot contain a return statement.", "`return` is only valid inside a function body; a module body cannot return."),
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
    entry!(BUS_MEMBER_UNDECLARED, "Referenced member is not defined on the declared bus.", "Definition exists for '{0}': {2}; referenced member '{1}' is not defined."),
    entry!(INSTANCE_REF_UNDECLARED, "A structured instance/member reference resolves to no declared instance in scope.", "The base name '{0}' of the structured reference '{1}' resolves to no declared instance in this component/module; declare it or fix the name."),
    entry!(BUS_MEMBER_ON_SCALAR_PORT, "Member/lane access on a module port declared without members (scalar io/out/in).", "Port '{0}' is declared scalar (no members); member/lane access '{1}' is not allowed. Declare its members or an interface type, or reference the whole port."),
    entry!(LABEL_NOT_EXPORTABLE, "Module-internal label accessed from outside its module.", "'{0}' is module-internal in '{1}' (declared without a direction word) and cannot be accessed through an instance path. Declare it with a direction word ('in'/'out'/'io') to put it on the module boundary — 'io' is the neutral choice."),
    entry!(PIN_NAME_EXPR_UNRESOLVED, "A pin name expression did not resolve against the bound parameters.", "Pin name expression '{0}' did not resolve against the parameters bound here; the row registers no name. Bind every parameter the expression reads, or write the name as plain text."),
    entry!(DYN_WIDTH_EXPR_NEEDS_PARAM, "A width-binder name sits inside an arithmetic width expression; expression widths are never back-solved.", "Dynamic pin range '{0}' reads '{1}' inside an arithmetic expression, but '{1}' is not declared in the parameter table. An undeclared name is a width binder only as a whole range endpoint — it binds the instance subscript width; inside arithmetic it never participates in back-solving (replicated-binding-design.md §4 check 1). Declare '{1}' as a parameter and give it explicitly here, or write the range end as a bare name."),
    entry!(IFACE_DYN_WIDTH_MISMATCH, "The binding row's subscript member count disagrees with the interface's dynamic pin expansion.", "Interface binding '{0}' names {1} subscript member(s) but the interface's dynamic pin range expands to {2} pin(s). With an explicit width parameter the three counts — subscript members, dynamic expansion, physical pins — must agree (replicated-binding-design.md §4 check 2). Match the subscript to the expansion, or drop the explicit parameter and let the width binder tie the counts."),
    // section
    entry!(CONN_TRANSPOSE_SIZE_MISMATCH, "Transposed connection size mismatch.", "Transposed connection size mismatch"),
    entry!(CONN_LEFT_ARROW_SHAPE_MISMATCH, "Shape mismatch in a <- connection.", "Shape mismatch in a <- connection"),
    entry!(CONN_PARALLEL_SHAPE_MISMATCH, "Shape mismatch in a parallel connection.", "Shape mismatch in parallel connection"),
    entry!(CONN_SERIES_SHAPE_MISMATCH, "Shape mismatch in a -> connection.", "Shape mismatch in -> connection"),
    entry!(CONN_OPERATOR_UNSUPPORTED, "The operator is not supported in connection statements; use '+' for parallel, '-' / '->' for series.", "Operator '{1}' is not supported in connection statements; use '+' for parallel, '-' / '->' for series"),
    entry!(PHRASE_AST_TYPE_UNEXPECTED, "Unexpected AST node type in a phrase.", "Unexpected AST node type {1} in McPhrase::new"),
    entry!(PHRASE_IFACE_MEMBER_NOT_FOUND, "Member not found in the interface.", "Member '{0}' not found in interface"),
    entry!(PORT_ROW_WITH_CONNECTION, "An iotype-prefixed port row carries a connection.", "port row carries a connection ('{0}'); a port row only declares ports — write the connection on a line of its own"),
    entry!(PHRASE_RESERVED_WORD_SUBSCRIBED, "A subscript is glued onto a reserved word, which addresses nothing.", "name carries a subscript in the reserved word '{0}', where a subscript selects nothing: write '{0}{...}' or '{0}.N'"),
    entry!(ATTR_VALUE_NOT_A_TERMINAL, "An attribute used as a connection endpoint.", "'{0}' is an attribute key: it holds a value, not a terminal, so it cannot be a connection endpoint; connect a pin, a port, or a net instead"),
    entry!(PIN_VALUE_KEY_NOT_FOUND, "A pin value key was not found.", "'{0}' has no value key '{1}'; declared keys: [{2}]"),
    // section
    entry!(GHOST_PORT_BOX, "A box has a placeholder pin not mapped to any real component pin.", "GHOST_PORT: box '{0}' (id={1}) has placeholder pin '{2}' (id={3}) that is not mapped to any real component pin. The component declared only an estimated pin count (pins = N) without actual pin definitions."),
    entry!(NET_MERGED_SHORT, "Multiple points resolve to the same node — possible short circuit.", "MERGED_SHORT: net '{0}' (module '{1}') has {2} point(s) resolving to the same node (id={3}). Paths: {4}. This may indicate a bracket expansion duplicate or a port declared without bit width causing signal merging."),
    entry!(NET_BUS_ORDER_MISMATCH, "Retired: bus member order mismatch (name-based, 2026-09-20).", "Retired - no producer. The name-based order judgment contradicted the positional pairing law; a crossed writing is legal and names are labels, never a pairing criterion (interface-connect-rule-design.md section 6.1 D3)."),
    entry!(SORT_HAZARD, "Bus pin numbers are non-monotonic; the binding follows declaration order, never numeric sorting.", "SORT_HAZARD: pin numbers in component '{0}' bus '{1}' are non-monotonic. Member-to-pin binding: [{2}]. Binding follows declaration order and is never sorted numerically; if you expected numeric pairing, reorder the members or the pins."),
    entry!(FLOATING_PLACEHOLDER, "A '_' placeholder could not be bound to any pin.", "FLOATING_PLACEHOLDER: '_' placeholder in net '{0}' (module '{1}') could not be bound to any existing pin. The placeholder is floating."),
    entry!(LEAD_PREFIX_ID_AS_WIRE, "'_X' is a prefix identifier (member name), not the wire '_'.", "PREFIX_ID_AS_WIRE: '{0}' is a prefix identifier (member name) like '_OPEN', not the wire '_'. If you meant pass-through in a connection line, write '_' instead."),
    entry!(FUNC_PARAM_SHADOWS_PIN, "Retired: no producer.", "Retired - no producer. The legacy edge path was removed; binding is name-first and a same-named pin is shadowed by design. The definition-side check lives on as HW_FUNC_PARAM_SHADOWS_PIN (5510)."),
    entry!(GHOST_PORT, "A net endpoint is not mapped to any box — possible unexposed module boundary port.", "GHOST_PORT: net '{0}' endpoint id={1} is not mapped to any box. This pin may cross a module boundary without being properly exposed as a port."),
    entry!(NET_DROPPED_STATEMENT, "A connection statement materialized no physical pins; no nets or constraints were produced.", "DROPPED_STATEMENT: {0} {1}. The statement may produce no nets or constraints."),
    entry!(NET_DUPLICATE_REF, "Same logical net referenced more than once in a connection, always pairing to the same peer net — redundant.", "DUPLICATE_REF: logical net '{0}' is referenced more than once and always pairs to the same net '{1}'. The result is identical to '{0} -> {1}'; simplify the redundant reference."),
    entry!(NET_SHORT_REF, "Same logical net referenced more than once in a connection, pairing to different peer nets — possible short.", "SHORT_REF: logical net '{0}' is referenced more than once and pairs to different nets [{1}]. Those nets are shorted together through the same-name group's pads; review the connection."),
    entry!(PIN_OCCUPIED_BY_DECLARATION, "Two different declarations materialized to the same physical pin id; the later registration is merged into the first.", "OCCUPIED_PIN: physical pin '{0}' (declaration class '{1}') is also claimed by a second, different declaration (class '{2}'). Two different-named declarations materialized to the same physical pin id; the later registration is absorbed by the first."),
    entry!(PHANTOM_IO_ACCESS, "An .in/.out access on a component/class that declares no such pin was isolated into a phantom endpoint.", "PHANTOM_IO: access '{0}' is isolated as a phantom endpoint: '{1}' has no declared pin '{2}'. The access does not connect to any net; it is fallout of an internal function-chain placeholder, not a real {1} pin."),
    entry!(UNRESOLVED_CLASS_STUB, "A class-looking construction was not opened into a real instance; it is dropped to an @? stub that produces no nets or parts.", "UNRESOLVED_CLASS: construction of '{0}' was not opened into a real instance and is dropped to an @? stub — it produces no nets or parts. '{0}' is a registered class, so review the construction / caller shape (a func-call dispatcher PassThrough fallback), or an alias-normalization gap that routed it here instead of real construction."),
    entry!(LAYOUT_MISSING_SUBNODE, "Retired: no producer.", "Retired - no producer. Superseded by the per-slot missing-subnode codes (4085, 4086, 4088, 4089, 4091, 4093)."),
    entry!(LAYOUT_TYPE_MISMATCH, "Layout attribute node type mismatch.", "Layout attribute node type mismatch."),
    entry!(LAYOUT_SET_MISSING_SUBNODE, "Layout set is missing a required subnode.", "Layout set is missing a required subnode."),
    entry!(LAYOUT_VALUES_TYPE_MISMATCH, "Retired: no producer.", "Retired - no producer. Superseded by LAYOUT_VALUE_TYPE_MISMATCH (4090)."),
    entry!(LAYOUT_NAME_MISSING_SUBNODE, "Layout name is missing a required subnode.", "Layout name is missing a required subnode."),
    entry!(LAYOUT_EDGE_MISSING_SUBNODE, "Layout edge is missing a subnode.", "While building layout: Missing subnode for edge"),
    entry!(LAYOUT_EDGE_TYPE_MISMATCH, "Layout edge node type mismatch.", "Type mismatch in layout edge"),
    entry!(LAYOUT_EDGE_NAME_MISSING_SUBNODE, "Layout edge name is missing a subnode.", "Missing subnode for layout edge name"),
    entry!(LAYOUT_VALUE_MISSING_SUBNODE, "Layout value is missing a subnode.", "Missing subnode for layout value"),
    entry!(LAYOUT_VALUE_TYPE_MISMATCH, "Layout value node type mismatch.", "Type mismatch in layout value"),
    entry!(LAYOUT_SET_SUBNODE_MISSING, "Layout set is missing a subnode.", "Missing subnode for layout set"),
    entry!(LAYOUT_EXTRA_NODES, "Retired: no producer.", "Retired - no producer. The layout face ignores extra nodes by design; unknown words warn only."),
    entry!(LAYOUT_VALUES_MISSING_SUBNODE, "Layout values are missing a subnode.", "Missing subnode for layout values"),
    entry!(LAYOUT_CONST_MISSING_INT, "CONST node is missing its INT subnode.", "CONST node missing subnode INT"),
    entry!(LAYOUT_PIN_NUMBER_PARSE, "Parse error in a layout pin number.", "Parse error in layout pin number"),
    entry!(LAYOUT_EDGE_NAME_ID_MISSING_SUBNODE, "Layout edge name id is missing a subnode.", "Missing subnode for layout edge name id"),
    entry!(LAYOUT_EDGE_INVALID, "Invalid layout edge.", "Invalid edge. Edges should be one of: \"left\", \"right\", \"top\", \"bottom\""),
    entry!(LAYOUT_EDGE_NAME_NOT_ID, "Retired: no producer.", "Retired - no producer. Superseded by 4088 and 4096."),
    // section
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
    entry!(NET_PIN_UNWIRED, "A component pad is absent from every net.", "A component pad is absent from every net."),
    entry!(IFACE_CROSS_FAMILY_CONNECT, "Endpoints of different interface families are connected.", "IFACE_CROSS_FAMILY_CONNECT: '{0}' and '{1}' are different interface families and cannot pair — the interface connect rule pairs only same-family interfaces (ordinal k on the two sides is the same wire). Renaming a member does not bridge families."),
    entry!(IFACE_ROLE_INCOMPATIBLE, "Connected interface roles are not mutual peers.", "IFACE_ROLE_INCOMPATIBLE: '{0}' and '{1}' of interface '{2}' are connected, but neither role names the other as its `peer` — declare `peer = {1}` on '{0}' and `peer = {0}` on '{1}', or connect a roleless side (which pairs positionally without the role check)."),
    entry!(IFACE_DIR_CONFLICT, "Both sides of an interface member connection declare the out direction.", "IFACE_DIR_CONFLICT: roles '{0}' and '{1}' of interface '{2}' are connected with the `out` direction word on both adoption rows — two push-pull outputs wired against each other is a drive fight; one side must read, not drive. The in-in and bidir cells of the compatibility matrix are other checks' territory (interface-member-config-design.md section 3, phase 1 covers out-out only)."),
    entry!(IFACE_ENDPOINT_COUNT_TOPOLOGY, "A point-to-point interface family has more than two endpoints on one net.", "IFACE_ENDPOINT_COUNT_TOPOLOGY: interface '{0}' declares `topology = \"point to point\"` but this net carries {1} of its endpoints — a point-to-point pair fits exactly two; split the net, or declare `topology = \"multi-point\"` on the interface if a shared bus is intended."),
    entry!(IFACE_ATTR_INCOMPATIBLE, "Connected interface sides declare incompatible values for the same attribute.", "IFACE_ATTR_INCOMPATIBLE: the two '{0}' endpoints both declare `{1}` but their declared value sets share nothing ({2} vs {3}) — declared attributes are judged only where both sides declare them, and disjoint sets mean the two ends were specified for different operating points."),
    // section
    entry!(INST_CHAIN_LINK_SKIPPED, "A chain link was skipped because the method is not defined on the instance.", "Method '{0}' not defined in {1} '{2}'; chain link skipped, no body expanded."),
    entry!(INST_ARG_NO_FORMAL_PORT, "Instance argument has no formal port to bind.", "Instance '{0}' arg{1} '{2}' has no formal port to bind"),
    entry!(INST_METHOD_FALLBACK, "Instance method could not be resolved; passed through instead.", "Unrecognized function call '{0}' in module '{1}' — treated as pass-through (class not loaded or name misspelled)"),
    entry!(INST_IFACE_INSTANTIATE_FAILED, "Interface instantiation failed.", "Interface instantiation failed: {0}"),
    entry!(INST_SUBMODULE_INSTANTIATE_FAILED, "Sub-module instantiation failed.", "Sub-module '{0}' instantiation failed: {1}"),
    entry!(INST_STMT_SKIP_FAILED_CLASS, "Statement references a component class whose instantiation failed; the whole statement is skipped.", "Statement references a component class whose instantiation failed; skipping entire statement."),
    entry!(INST_STMT_PARSE_FAILED, "A connection statement failed to expand.", "Connection statement #{0} failed: {1}"),
    entry!(INST_BUILTIN_TWOPIN_EXPAND_FAILED, "Expanded builtin two-pin pair failed.", "Expanded builtin twopin pair failed: {0}"),
    entry!(INST_MEMBER_PROCESS_FAILED, "A member of a connection statement failed to process.", "Member processing failed: {0}"),
    entry!(INST_ADJACENT_CONNECT_FAILED, "Connection between adjacent members of a series failed.", "Connection between members #{0} and #{1} failed: {2}"),
    entry!(INST_FUNC_BODY_STMT_FAILED, "A module-level function body statement failed.", "Module-level function '{0}' body statement failed: {1}"),
    entry!(INST_LANE_FUNCCALL_FAILED, "Failed to instantiate a FuncCall during lane-by-lane wiring.", "Failed to instantiate FuncCall in lane-by-lane wiring: {0}"),
    entry!(INST_LANE_TRANSPOSED_FAILED, "Failed to instantiate a Transposed member during lane-by-lane wiring.", "Failed to instantiate Transposed in lane-by-lane: {0}"),
    entry!(SHAPE_TRANSPOSE_LIMIT, "Transpose operand must be 1*1 / 1*2 / 2*1 / 2*2 (eval.md §5.5).", "Transpose operand has {0} rows; only 1*1, 1*2, 2*1 or 2*2 shapes can be transposed."),
    entry!(SHAPE_REVERSE_NOOP, "Reverse `^` has no effect on a vector (parallel / transposed) operand (eval.md §9).", "Reverse `^` on '{0}' has no effect: a vector operand (parallel or transposed) carries no order to reverse."),
    entry!(SHAPE_EXPAND_DIM_MISMATCH, "Vector expansion dimension mismatch (eval.md §7 rule 3); implicit auto-expansion forbidden.", "Vector expansion dimension mismatch: left {0} rows vs right {1} rows. {2}"),
    entry!(SHAPE_INST_3PIN_PLUSMINUS, "Instance with 3+ pins cannot directly participate in `+`/`-`; only 1x1/1x2 instances can (veccircuit.md).", "Instance '{0}' with {1} pins cannot directly participate in `+`/`-`. Use `->` for a pass-through connection."),
    entry!(SHAPE_INCOMPLETE, "NetShape missing; fell back to the deprecated connection_type() inference (stage 3).", "SHAPE_INCOMPLETE: net '{0}' has no NetShape provenance; fell back to connection_type() inference."),
    entry!(SHAPE_COLUMN_WIDTH_MIXED, "Column-width mix in a `[...]` list (vec-arch.md §4.1.1 R4): a single-column element among two-pin/node elements silently spans both columns.", "Column-width mix in a list: '{0}' spans both columns (single-column element among two-pin/node elements, e.g. `[A, R101]`). All elements must be single-column or all two-pin/node; use '_' to inherit the sibling column width."),
    entry!(SHAPE_INST_PORTCOUNT_PLUSMINUS, "Two component bodies with unequal port counts cannot participate in `+`/`-` (a body pair must have matching terminals).", "Instances '{0}' ({1} ports) and '{2}' ({3} ports) cannot participate in `+`/`-`: two component bodies must have equal port counts. Use a single pin ('{0}.1') to attach one body to a net."),
    entry!(SHAPE_MEMBER_GROUP_WIDTH, "Nested-subscript inner member group wider than two (vec-arch.md §4.1.1 R2, U237 case B): a `1*3` row vector has no defined pairing.", "Inner member group of '{0}' has {1} members; a nested-subscript inner group must be a single member (a point, `N*1`) or a pair (a member face, node `N*2`). A `1*{1}` row vector has no defined pairing (R5 row-vector law)."),
    entry!(CONN_GROUP_SHAPE_MISMATCH, "Retired: no producer.", "Retired - no producer. The R0 rework removed the group-shape refusal path; sister code CONN_SERIES_SHAPE_MISMATCH (4167) stays live."),
    entry!(INST_POWER_PORT_UNBOUND, "Sub-module DC power port is never connected (missing power argument?).", "Sub-module instance '{0}' DC power port '{1}' is never connected (missing power argument?)"),
    entry!(INST_CTOR_BODY_STMT_FAILED, "A constructor function body statement failed.", "Constructor '{0}' body statement failed: {1}"),
    entry!(INST_CTOR_PARAM_BIND_FAILED, "Constructor parameter binding failed.", "Constructor '{0}' on '{1}' param bind: {2}"),
    entry!(INST_ARG_UNBOUND_DETAILED, "Instance argument has no formal port to bind (with module/bound details).", "Instance '{0}' arg '{1}' has no formal port to bind (module='{2}', {3}/{4} formal ports already bound)"),
    entry!(INST_PARAM_BIND_FAILED, "Declarative instance parameter binding failed (unknown / excess / type-mismatched argument).", "Instance '{0}' ({1}) param bind: {2}"),
    entry!(INST_PARAM_MISSING_REQUIRED, "Instance is missing a required constructor parameter; silent in dev mode, reported as a warning in strict mode (--strict). The instance is created anyway with the supplied arguments.", "Instance '{0}' ({1}) is missing required parameter '{2}'; created with the supplied arguments"),
    entry!(VECTOR_WIDTH_MISMATCH, "Argument/formal vector width mismatch in arg-to-formal binding; no implicit expansion or member dropping.", "Vector width mismatch in arg-to-formal binding: formal '{0}' expects {1} member(s), actual '{2}' provides {3}. Scalar-to-vector and unequal-width pairing are errors; pass the full vector with members paired positionally."),
    entry!(VECTOR_ZIP_WIDTH_MISMATCH, "Vector member-set mismatch in a connection; the two ends of a lane must zip positionally at equal width (no flattening, no member dropping).", "Vector pairing width mismatch: left side provides {0} member(s), right side provides {1}; one-to-one member correspondence requires equal widths. Scalar args apply per-member (the func per-member dispatch layer, vec-dianlu §7.6); a vector-slice arg must be written at the receiver's member count — no broadcast."),
    entry!(CONN_LEAD_CROSSNET, "A '_' lead joins two different nets: an ideal wire (a body, vec-dianlu §5.4) crossing nets shorts them at zero impedance.", "Lead '_' joins two different nets: '{0}' and '{1}'. A lead is an ideal wire (vec-dianlu §5.4) — its two ends are meant to be the same net; joining distinct nets shorts them at zero impedance."),
    entry!(CONN_NET_CROSSNET, "Parallel '+' between two bodiless operands (labels/rails) joins two different nets: merging two distinct potentials shorts them at zero impedance.", "Parallel '+' joins two different nets: '{0}' and '{1}'. Neither operand is a body (vec-dianlu §1.4/§5.4) — a label or rail names an existing equipotential region, so its potential is carried by its name and two different names are two different potentials. With nothing to stack, '+' can only merge the two regions: a dead short."),
    entry!(MODULE_PORT_IFACE_ROLE, "Module port binding carries a role argument; role belongs to endpoint terminal pins only (replicated-binding-design R3).", "Module '{0}': port '{1}' binds interface '{2}' with role '{3}' — role is the link identity of endpoint terminal pins; module ports are role-less conduits. Drop the role argument: '{1}::{2}()'."),
    entry!(IFACE_ROLE_ARG_LITERAL, "Role-position constructor argument is a quoted or numeric literal; the role position takes a bare identifier (ident-vs-literal ruling, U144).", "Interface '{2}' takes a bare identifier in its role position, but '{0}::{1}' carries literal argument(s) [{3}] — the role is never recorded and role validation is silently bypassed. Write the role name bare: '{0}::{1}(Role)'."),
    entry!(IFACE_VIEW_LANE_MISMATCH, "An interface's role table declares a different lane count than the interface's role-less conductor view (conductor-view-design.md R-CV2, the uniformity law).", "Interface '{0}': role '{1}' declares {2} pin lane(s) but the interface's role-less conductor view declares {3} — every role table must match the view's lane count, otherwise role-less bindings and role bindings resolve different shapes from the same interface. Give the role {3} lane(s), or drop its `pins` table to inherit the view."),
    entry!(MEDIATOR_IFACE_ROLE, "A wiring mediator in the middle of a series chain carries a role argument; roles are legal only on terminal pins rows.", "Interface instance '{0}' sits between two chain members and binds role '{1}'. A mediator is a role-less conductor: it forwards the chain and takes no side, so its role can never become an endpoint identity — the chain's identity is read from the terminal pins rows alone (replicated-binding-design.md §4 check 3, R3; the module-port half of the same law is reported as 4184). Drop the role argument — write '{0}()' — or move this instance to a terminal position of the chain."),
    entry!(COMPONENT_PARAM_FUNC_CONFLICT, "Component-level parameter shares a name with the same-name constructor func parameter.", "Component '{0}' declares parameter '{1}' that also appears in constructor func '{2}' params. Class params define class behavior and constructor params declare the construction arity; they must not reuse the same name. Rename one of them."),
    // section
    entry!(DUP_CMIE_CROSS_FILE, "Same name defined in another file (cross-file duplicate).", "Same name defined in another file (cross-file duplicate)."),
    entry!(DUP_WITHIN, "Duplicate definition within the same declaration.", "Duplicate definition within the same declaration."),
    entry!(DUP_ENUM_VALUE, "Enum value appears more than once in the enum.", "Enum value appears more than once in the enum."),
    // section
    entry!(NAME_COMPONENT_LOWERCASE, "Component name starts with lowercase; convention is UPPER_SNAKE.", "Component name starts with lowercase; convention is UPPER_SNAKE."),
    entry!(NAME_PORT_SHADOWS_CMIE, "Port name shadows a library CMIE name.", "Port name shadows a library CMIE name."),
    entry!(NAME_PIN_MIXED_CONVENTION, "Pins use mixed naming conventions.", "Pins use mixed naming conventions."),
    entry!(NAME_INSTANCE_SINGLE_CHAR, "Instance name is a single character.", "Instance name is a single character."),
    entry!(NAME_PORT_INST_SHADOWS_CMIE, "Port/instance name shadows a library CMIE name.", "Port/instance name shadows a library CMIE name."),
    entry!(NAME_PARAM_SHADOWS_CMIE, "Parameter name shadows a library CMIE name.", "Parameter name shadows a library CMIE name."),
    entry!(RECIPE_BODY_INVALID, "recipe body may only contain signal declarations and funcs.", "recipe body may only contain signal declarations and funcs"),
    entry!(RECIPE_FUNC_UNRESOLVED_REF, "recipe func references a name that is not a declared signal, parameter, or func-local instance.", "'{0}' is not a declared signal, parameter, or local in this recipe func"),
    entry!(VARIANT_REDECLARES_PINS_PARAMS_FUNCS, "A variant may not declare pins, construction params, or funcs — they are inherited from the abstract base.", "variant '{0}' may not declare pins, params, or funcs (inherited from the base)"),
    entry!(ABSTRACT_DERIVES_ABSTRACT, "An abstract component may not carry a variant base — abstract inherits abstract is forbidden.", "abstract component may not derive with ':' (no variant chain)"),
    entry!(VARIANT_ADOPTS, "Variant inheritance (':') and recipe adoption ('::') are mutually exclusive.", "':' and '::' are mutually exclusive on one component"),
    entry!(VARIANT_BASE_NON_ABSTRACT, "A variant base must be an abstract component.", "'{0}' is not an abstract component — use '::' to adopt a recipe"),
    entry!(ADOPTS_NON_RECIPE, "Adoption target must be a recipe.", "'{0}' is not a recipe — use ':' to derive a variant from an abstract component"),
    entry!(RECIPE_SIGNAL_MISSING, "An adopting component must declare every recipe signal (name + direction + interface).", "'{0}' is missing recipe signal '{1}'; {2}"),
    entry!(ADOPTED_FUNC_AMBIGUOUS, "Two adopted recipes expose the same func name and the component does not override it.", "adopted recipes share func '{0}'; define '{0}' here to override"),
    entry!(BOM_VALUE_NOT_DESCENDANT, "A bom.mc value names a class that is not a `:` descendant of the slot's declared class.", "bom key '{0}' names '{1}', which is not a `:` descendant of the slot's declared class '{2}' — the bom block picks a variant of the declared base, it does not retype the slot. Name a variant whose `: base` chain reaches '{2}', or change the module face to declare the base the bom value derives from (param-authoring-design.md section 4)."),
    entry!(BOM_KEY_NOT_SLOT, "A bom.mc key does not designate an abstract-declared instance.", "bom key '{0}' does not designate an abstract-declared slot ({1}) — the bom block binds part selections to slots, and a slot is an instance whose module declares it on an `abstract component` base. Remove the key, or make the module face declare the base and let the bom block pick the variant (param-authoring-design.md section 4)."),
    // section
    entry!(SPEC_KEY_UNDECLARED_PARAM, "Spec key references a parameter that is not declared.", "Spec key references a parameter that is not declared."),
    entry!(REF_INTEGRITY, "Reference integrity violation.", "Reference integrity violation."),
    entry!(FUNC_PARAMS_NO_BODY, "Function has parameters but no body (empty implementation).", "Function has parameters but no body (empty implementation)."),
    // section
    entry!(INST_DECLARED_MULTIPLE, "Instance is declared more than once in the module.", "Instance is declared more than once in the module."),
    entry!(PORT_DUPLICATE_NAME, "Duplicate port name in the module — ambiguous.", "Duplicate port name in the module — ambiguous."),
    entry!(NOT_AN_INTERFACE, "The class is a component/module/enum, not an interface.", "'{0}' is a component/module/enum, not an interface."),
    entry!(NAME_PARAM_AND_INSTANCE, "Name is both a value parameter and an instance.", "Name is both a value parameter and an instance."),
    entry!(PIN_UNCONNECTED, "Pin is not connected to any net.", "Pin is not connected to any net."),
    entry!(PIN_CONFLICTING_OPTIONS, "Pin uses conflicting option names.", "Instance {0} pin {1} is reached as both {2} and {3}, and the two names belong to different `|` options of the pin — one physical pin cannot carry two functions at once. Connect one option per pin, or split the connection across the option's own pin group."),
    entry!(INST_THIS_TYPE, "this :: TYPE declaration is not allowed.", "this :: TYPE declaration is not allowed."),
    entry!(MODULE_PORT_UNUSED, "Module port is declared but never connected.", "Module port is declared but never connected."),
    entry!(COND_SINGLE_BINARY, "Condition compares against a single binary value.", "Condition compares against a single binary value."),
    // section
    entry!(ENUM_SINGLE_VALUE, "Enum has only one value.", "Enum has only one value."),
    entry!(PARAM_INT_DEFAULT_STRING, "Integer param has a string default.", "Integer param has a string default."),
    entry!(PARAM_STRING_DEFAULT_NUMERIC, "String param has a numeric-looking default.", "String param has a numeric-looking default."),
    entry!(PARAM_UV_DEFAULT_NO_UNIT, "Unit-value param default has no unit suffix (e.g. '5V').", "Unit-value param default has no unit suffix (e.g. '5V')."),
    entry!(PARAM_FLOAT_DEFAULT_INVALID, "Param has an invalid float default.", "Param has an invalid float default."),
    entry!(PARAM_NEGATIVE_DEFAULT, "Integer param default is negative.", "Integer param default is negative."),
    // section
    entry!(PARAM_RESERVED_KEYWORD, "Parameter uses a reserved keyword.", "Parameter uses a reserved keyword."),
    entry!(FUNC_EMPTY_BODY, "Function has an empty body.", "Function has an empty body."),
    entry!(COMPONENT_EMPTY, "Component has no params, pins, attributes, or functions.", "Component has no params, pins, attributes, or functions."),
    entry!(COMPONENT_NO_PINS, "Component has no pin definitions.", "Component has no pin definitions."),
    entry!(INTERFACE_EMPTY, "Interface has no pins or roles.", "Interface has no pins or roles."),
    entry!(INST_CLASS_NOT_LOADED, "Instance references a class that is not loaded.", "Instance references a class that is not loaded."),
    entry!(COMPONENT_MIXED_CASE, "Component name uses mixed case; convention is UPPER_SNAKE.", "Component name uses mixed case; convention is UPPER_SNAKE."),
    entry!(BUS_DUPLICATE_MEMBER, "Bus has a duplicate member.", "Bus has a duplicate member."),
    entry!(IFACE_PIN_COUNT_MISMATCH, "Interface expects more pins than are bound.", "Interface expects more pins than are bound."),
    entry!(FUNC_SHARES_NAME_WITH_PORT, "Function shares its name with a port/param.", "Function shares its name with a port/param."),
    entry!(SPEC_KEY_DUPLICATE, "Spec key appears more than once.", "Spec key appears more than once."),
    // section
    entry!(DEF_AMBIGUOUS_NAME, "Same name used for different definition kinds.", "Same name used for different definition kinds."),
    entry!(DEF_REF_NOT_LOADED, "Definition references a class that is not loaded.", "Definition references a class that is not loaded."),
    entry!(COMPONENT_INT_SUFFIX, "Component has an unconventional '.int' suffix.", "Component has an unconventional '.int' suffix."),
    entry!(ENUM_INT_SUFFIX, "Enum has an unconventional '.int' suffix.", "Enum has an unconventional '.int' suffix."),
    // section
    entry!(ATTR_RESERVED_KEYWORD, "Attribute uses a reserved keyword.", "Attribute uses a reserved keyword."),
    entry!(INST_ARG_COUNT_MISMATCH, "Instance passes more/fewer args than the class declares.", "Instance passes more/fewer args than the class declares."),
    entry!(ROLE_EMPTY_BODY, "Role has an empty body.", "Role has an empty body."),
    entry!(ROLE_NAME_SHADOWS, "Role shares its name with a parameter or pin/port.", "Role shares its name with a parameter or pin/port."),
    entry!(ATTR_NESTING_TOO_DEEP, "Attribute nesting depth exceeds 16.", "Attribute nesting depth exceeds 16."),
    entry!(ATTR_PIN_GROUP_UNDEFINED, "Attribute references an undefined pin group, or role used outside a component.", "Attribute references an undefined pin group, or role used outside a component."),
    entry!(PINS_PLUS_AND_PINS_CONFLICT, "Component mixes pins = and pins.X = attributes, or uses a non-constant default.", "Component mixes pins = and pins.X = attributes, or uses a non-constant default."),
    entry!(ATTR_DOTTED_NAME_UNRESOLVED, "Dotted attribute name starts with an unregistered key.", "Dotted attribute name starts with a key that is neither the component name nor a registered attribute key."),
    entry!(ATTR_KEY_DUPLICATE, "Attribute key declared more than once in one attribute list.", "Attribute key is declared more than once in one attribute list."),
    entry!(ATTR_VALUE_NOT_IN_VOCABULARY, "Attribute value is outside the key's registered word set.", "Attribute value is outside the key's registered word set."),
    entry!(RATING_PARAM_OUT_OF_RANGE, "An instance parameter value falls outside the interval its class's ratings clause declares.", "Instance '{0}' of component '{1}': parameter '{2}' = {3} violates its ratings ({4} {5}). Set the value within the declared bounds or choose a part whose ratings admit it."),
    entry!(RATING_KEY_NOT_A_PARAM, "A ratings entry key names no constructor parameter of its class.", "Ratings key '{0}' in component '{1}' names no constructor parameter. Key each entry with a parameter declared in the class signature."),
    // section
    entry!(ENUM_DUPLICATE_VALUE, "Enum has a duplicate value.", "Enum has a duplicate value."),
    entry!(ENUM_MEMBER_DOT, "Enum member contains a dot.", "Enum member contains a dot."),
    entry!(ENUM_MEMBER_LEADING_DIGIT, "Enum member starts with a digit.", "Enum member starts with a digit."),
    entry!(ENUM_MEMBER_RESERVED, "Enum member is a reserved keyword.", "Enum member is a reserved keyword."),
    entry!(ATTR_INFINITE_FLOAT, "Attribute has an infinite float value.", "Attribute has an infinite float value."),
    entry!(ATTR_LARGE_INT, "Attribute has a suspiciously large integer value.", "Attribute has a suspiciously large integer value."),
    entry!(RANGE_REVERSED, "Range appears reversed; did you mean the opposite order?", "Range appears reversed; did you mean the opposite order?"),
    entry!(RANGE_SINGLE_ELEMENT, "Range expands to a single element.", "Range expands to a single element."),
    entry!(IDX_MULTIPLE_SLICE_SPEC, "IDX key has multiple slice specifications.", "IDX key has multiple slice specifications."),
    entry!(EXPR_THIS_TOP_LEVEL, "'this' used in a top-level net statement; it is only valid inside instance/function contexts.", "'this' used in a top-level net statement; it is only valid inside instance/function contexts."),
    entry!(EXPR_PLACEHOLDER_ONLY, "Net connects only to '_' placeholder; the connection has no effect.", "Net connects only to '_' placeholder; the connection has no effect."),
    entry!(OPEN_LEAD, "Open lead: an anchored end meets a free '_' placeholder.", "Open lead: '{0}' leaves a free '_' placeholder — the anonymous point gathers a single anchor and nothing can reach it."),
    entry!(ATTR_SELF_REFERENTIAL, "Attribute value equals its own key; likely a copy-paste mistake.", "Attribute value equals its own key; likely a copy-paste mistake."),
    entry!(EVAL_DIVIDE_BY_ZERO, "Division by zero while evaluating a value expression.", "Division by zero while evaluating a value expression."),
    entry!(EVAL_OPERAND_NOT_NUMERIC, "Arithmetic operator applied to operands it is not defined for.", "Operator '{0}' is not defined for {1} and {2}."),
    entry!(EVAL_OVERFLOW, "Arithmetic overflowed the representable range.", "Integer overflow in '{0}' with operands {1} and {2}."),
    entry!(EVAL_ERROR_EXPRESSION, "A library author's error() expression was evaluated.", "{0}"),
    // section
    entry!(COND_EMPTY_BODY, "Conditional block has an empty body.", "Conditional block has an empty body."),
    entry!(COND_IF_WITHOUT_ELSE, "if without a matching else.", "if without a matching else."),
    entry!(PIN_NC_COMPONENT_LEVEL, "NC pin used at component level.", "NC pin used at component level."),
    entry!(POWER_PIN_NO_VOLTAGE, "Power pin has no voltage attribute.", "Power pin has no voltage attribute."),
    entry!(PIN_IO_MIX_IN_OUT, "Pin mixes In and Out IO types.", "Pin mixes In and Out IO types."),
    entry!(PIN_IO_MIX_OUTPUT_POWER, "Pin mixes Output and Power IO types.", "Pin mixes Output and Power IO types."),
    entry!(PARAM_PIN_NAME_SHADOW, "Parameter shares its name with a pin.", "Parameter shares its name with a pin."),
    entry!(MODULE_STUB, "Module is a stub.", "Module is a stub."),
    entry!(COND_DUPLICATE, "Duplicate condition in if/else-if chain.", "A later if/else-if branch duplicates an earlier branch's condition, so it can never be selected."),
    entry!(COND_JUDGE_OPERAND_DROPPED, "Condition operand was not recognized.", "A judge operand has a form the condition collector does not recognize (for example a call), so the whole judge is discarded and the branch never selects. Rewrite the operand as a parameter reference or a literal."),
    entry!(COND_FAMILY_MISMATCH, "Condition compares a bare word with a quoted string.", "The two sides of the judge belong to different lexical families ({0} vs {1}), so they can never be equal under the strict reading: a bare word only equals the same bare word, a quoted string only the same quoted text. Spell both sides in the same family, or bind the parameter so its family matches the judge."),
    // section
    entry!(HW_PIN_NUMBER_GAP, "Pin numbers have gaps.", "Pin numbers have gaps."),
    entry!(HW_PIN_COUNT_HIGH, "Pin count is unusually high.", "Pin count is unusually high."),
    entry!(HW_ZERO_PINS_WITH_PARAMS, "Component has zero pins but parameter attributes.", "Component has zero pins but parameter attributes."),
    entry!(HW_IFACE_PEER_DANGLING, "Interface role references an undefined peer.", "Interface role names a peer that is not defined in this interface (interface-connect-rule-design.md §3.3 C)."),
    entry!(HW_ALL_SAME_IO_TYPE, "Only one active IO type across all pins.", "Only one active IO type is present across the component's pins; the remaining pins declare no direction. Verify the pin definitions are complete."),
    entry!(HW_IFACE_PEER_NOT_MUTUAL, "Interface role peer is not mutual.", "Interface role names a peer that does not name it back; peer pairs must be mutual (interface-connect-rule-design.md §3.3 A). Exempt when either side declares a multi-peer relay set (D8)."),
    entry!(HW_IFACE_PEER_WIDTH_MISMATCH, "Interface role peer width mismatch.", "Interface role and its declared peer declare different member widths (interface-connect-rule-design.md §3.3 B). Exempt when either side declares a multi-peer relay set (D8)."),
    entry!(HW_IFACE_PAIR_NOT_TWO, "A @pair group does not have two legs.", "A differential pair has exactly two faces: the interface member rows sharing a @pair(group) tag must be two rows, no more and no fewer (diff-pair-design.md §3.2)."),
    entry!(HW_IFACE_DIFF_PAIR_RETIRED, "The diff_pair key is retired.", "The interface body still writes `diff_pair = [A, B]`; tag the member rows with @pair(group) instead — the two rows sharing a group are the legs of one differential signal (diff-pair-design.md, ruled 2026-09-23)."),
    entry!(HW_PAIR_CONSTRAINT_NOT_LENGTH, "A @pair constraint value is not a length.", "The match slot of @pair(group, match: …) is a board-drawing tolerance spelled in a length unit (0.2mm, 8mil); this value is not a length quantity (pair-constraint-design.md §3.2)."),
    entry!(HW_PAIR_CONSTRAINT_MISMATCH, "The legs of a @pair group disagree on a constraint.", "Both legs write the slot and both write the same value; a leg whose partner wrote the slot and wrote no (or a different) value disagrees (pair-constraint-design.md §3.2)."),
    entry!(HW_PAIR_CONSTRAINT_ORPHAN, "A @pair constraint slot has no group.", "The row carries a named constraint slot but no group name — the constraint has no pair to ride; write @pair(group, match: …) (pair-constraint-design.md §3.2)."),

    entry!(HW_FUNC_PARAM_SHADOWS_PIN, "Function parameter shadows a pin name.", "Function parameter shadows a pin name."),
    // section
    entry!(TYPE_INCOMPATIBLE, "Incompatible types or unit types.", "Incompatible types or unit types."),
    // section
    entry!(UNUSED_PARAM_OR_PORT, "Parameter or port is declared but never used.", "Parameter or port is declared but never used."),
    entry!(PORT_NEVER_USED, "Port is declared but never used in any net connection.", "Port '{0}' in '{1}' is declared but never used in any net connection."),
    entry!(UNTYPED_PARAM, "Parameter has no inferred type.", "Parameter has no inferred type."),
    // section
    entry!(ABSTRACT_PART_UNSELECTED, "Placed abstract component has no selected part (partno unset).", "abstract component instance '{0}' is unselected (no partno); BOM must pick a variant"),
    entry!(VARIANT_SPEC_UNSET, "Variant still carries an unset inherited spec item.", "variant '{0}' leaves spec item '{1}' unset (±0/empty)"),
    entry!(POWER_BRIDGE_LOOP, "A DC @bridge subgraph contains a loop (parallel/cyclic legs).", "parallel DC @bridge between '{0}' and '{1}' forms a loop; declare @star on a hub ref to discharge it (PWR-2)"),
    entry!(CLAMP_REF_NOT_PROTECTIVE, "@clamp(ref) must reference a protective/earth-role ref.", "@clamp target '{0}' must be an @role(protective)/@role(earth) ref, but its role is '{1}' (PWR-7)"),
    entry!(POWER_RAIL_DECODE, "A DC rail contract argument does not decode.", "rail [{0}, {1}]::DC: {2}"),
    entry!(POWER_RAIL_TWO_ROOTS, "A net is the hot member of two rails (two handwritten supply roots).", "net '{0}' is a rail guarantee in both domain '{1}' and domain '{2}' — one net carries one handwritten supply root (P3)"),
    entry!(POWER_SINK_NOMINAL_MISMATCH, "A sink terminal's required DC nominal does not match the supply guarantee of the net it lands on.", "sink '{0}' on net '{1}' requires {2}, but the net's supply guarantee is {3} — sink nominal ≠ supply nominal (P3/E-PWR-001)"),
    entry!(POWER_PIN_DECODE, "A power pin DC contract argument does not decode.", "power pin '{0}.{1}': {2}"),
    entry!(POWER_SOURCE_CONTENTION, "Two or more psrc hard sources drive one net (undeclared parallel).", "net '{0}' carries {1} psrc hard sources ({2}) — an undeclared parallel source; ORing needs a declared combine element (PWR-3)"),
    entry!(ISOLATED_DC_BRIDGE, "An isolated world is DC-bridged to a non-isolated net.", "isolated '{0}' is DC-bridged to non-isolated '{1}' — an isolated world carries zero DC bridges to the outside (§3.2/PWR-9); only an explicit Y-cap @couple to earth may cross"),
    entry!(PROTECTIVE_MULTI_BRIDGE, "A protective conduit carries more than one DC single-point bridge.", "protective '{0}' has {1} DC single-point bridges to the circuit side — exactly one declared bridge is allowed (PWR-8)"),
    entry!(EARTH_DC_LEAK, "An earth reference is DC-bridged to another net (leakage).", "earth '{0}' is DC-bridged to '{1}' — the chassis/earth reference couples only through a Y-cap @couple, never a DC @bridge (§3.2 earth row); a DC tie is a leakage warning"),
    entry!(REFERENCE_ISLAND_ROOT, "A DC-bridged reference island must have exactly one @role(main) root.", "reference island '{0}' carries {1} @role(main) roots — each DC-bridged L1 island has exactly one main root: zero means the joined reference identities have no island ground to return to, more than one means two power worlds were DC-joined by a @bridge (§3.2.1)"),
    entry!(ROLE_REF_MISSING_BRIDGE, "A quiet/protective reference conduit has no declared DC @bridge.", "role conduit '{0}' carries no declared DC @bridge — an @role(quiet)/@role(protective) conduit expects exactly one bridge to its main reference; a bare zero means the declaration was never wired (conduit-equivalence-design.md §8.4)"),
    entry!(SINK_NET_NO_SOURCE, "A net carrying power sinks (psnk) has no declared source root on it.", "net '{0}' carries component power-sink terminals but has no supply root on the net — it is neither a declared domain-rail face nor driven by a psrc/psbi hot pin, so its loads draw from nothing that guarantees power (PWR-1 no-source face). Attach the loads to a declared rail face or drive the net from a psrc source; a feed through a series pass element (inductor/ferrite/fuse) or a module boundary only counts once that upstream net itself carries a resolvable source root (island-attribution-design.md §7 L4)"),
    entry!(COMBINE_OUTPUT_TOL, "A combine element's output psrc declares a tolerance window it cannot re-anchor.", "combine element '{0}' output '{1}' declares tol {2} — a pass-through OR-merge can't guarantee tighter than its live input, so a literal OUT window over-claims under single-source states (rail-contract-design.md §6.2③); write the nominal-only ::DC(v), or add spec.output to model a regulator"),
    entry!(NET_BUDGET_EXCEEDED, "A net's declared psnk load demand exceeds its supply capacity (PWR-4).", "net '{0}' declares {1} of psnk load ({2} sinks) but its supply root declares capacity {3} — Σ amp ≤ capacity is the PWR-4 budget (rail-contract-design.md §8.2). Either the loads really overdraw the rail (cut the load / raise the source capacity / split the rail), or a sink's amp is mis-declared. Undeclared sinks draw unknown current and are not counted; converter-input push-up and cross-net feed are the S-set step"),
    entry!(RETURN_LEG_UNDECLARED, "A two-terminal DC element links two disjoint potential classes (planes) with no declared DC relation on this leg (§8.5).", "two-terminal '{2}' links planes '{0}' and '{1}' but this leg carries no @bridge/@couple — the classes are not co-resident in one declared rail loop, so the DC relation is explicit: add the forgotten single-point @bridge here, or declare the intentional bypass on this leg (conduit-equivalence-design.md §8.5)"),
    entry!(POWER_CONVERTER_GATE, "A regulator's declared input window excludes the supply window on its input net.", "regulator '{2}' declares input window {1}, but the supply window riding its input net is {0} — S(input) ⊄ input_req: the rail or source feeding it sags or soars outside its operating pre-condition, so the declared output guarantee cannot be trusted. Fix the feed (or the input_req if it is mis-declared); a net whose supply window cannot be derived is not adjudicated (rail-contract-design.md §6.1 gate)"),
    entry!(POWER_SINK_WINDOW_MISMATCH, "A load's declared acceptable window excludes the actual supply window on its net.", "load '{2}' accepts supply only within {1}, but the supply window actually riding its net is {0} — S(net) ⊄ input_req: the rail or source can over- or under-volt the load outside what it tolerates. Fix the feed, or correct the load's declared window; a net whose supply window cannot be derived is not adjudicated (rail-contract-design.md §6.3 sink window)"),
    entry!(POWER_CONVERTER_SPEC_INCOMPLETE, "A spec block on a power-output component declares only one of input_req / output.", "component '{0}' has a psrc/psbi output row and a spec block, but declares only '{1}' — a regulator needs both windows of the Hoare triple for the gate (6023) and sink-window (6024) checks to judge it. The written side still decodes (an output-only regulator is treated as guaranteeing that output; an input_req-only one as an un-gated feed), it just cannot be gated: add the missing '{2}' (rail-contract-design.md §6.1)"),
    entry!(POWER_CONVERTER_OUTPUT_RAIL_WINDOW, "A converter's declared output guarantee is not covered by the declared rail window of the rail net it drives.", "converter '{2}' guarantees output {0} on rail net '{3}', but the rail's declared window is only {1} — the guarantee escapes the rail's allowed window: the converter can deliver outside what the scope declares on that net. Fix the spec.output, or the rail tolerance if the rail is mis-declared (rail-contract-design.md §6.7)"),
    entry!(DEVICE_RETURN_SPAN_UNDECLARED, "A device's DC return pins span disjoint return classes (planes) with no declared relation covering the span.", "device '{2}' returns across planes '{0}' and '{1}' but no net-level @bridge/@couple and no declared isolation structure covers the span — its return-side pins silently DC-join the two classes through the die/substrate: add a net-level @bridge/@couple between the return nets, or check whether one return is an @role(isolated) source-side copper the device legitimately feeds (conduit-equivalence-design.md §8.6)"),
    entry!(DC_BINDING_DIR_MISMATCH, "A direction-word power terminal sits at the wrong end of its own connection chain.", "'{0}' is declared {1} but occupies {2} — the wrong end of its own connection chain: a source (psrc) face must lead the chain (first member / right of a {L|R} through), a sink (psnk) must trail it (last member / left of a {L|R} through). The direction word stays authoritative (6011/6019/6021/pwrflow): flip the arrow or move the terminal so the chain direction agrees with the declared direction contract (intent-design.md §5.3.2, PWR-10)"),
    entry!(PORT_BIND_ROLE_MISMATCH, "An out port declaring @bind_role(<role>) is bound in its parent scope to a reference whose declared role differs, or to a net with no role identity.", "port '{0}' declares @bind_role({1}) but its parent binding '{2}' resolves to role {3} — the child names a role, never an ancestor conduit, so the parent binding must witness it: bind the port to a conduit of that role (or forward it to a sibling port re-declaring the same role). Bind to the {1} conduit, or fix the @bind_role if the contract itself is mis-declared (conduit-equivalence-design.md §8.7)"),
    entry!(POWER_PIN_RETURN_MISSING, "A ::DC power row declares no return member, so the DC crossing is incomplete.", "{0} '{1}' carries a ::DC contract but no second (return) member — a DC crossing is the pair [hot, ret] and every consumer of the crossing reads that member: write it as a pair on one row, e.g. `psnk [1,2] = VIN{Vin, GND}::DC(...)`, naming the return the hot terminal closes over (conduit-equivalence-design.md §8.8, model A)"),
    entry!(EXPOSED_NET_NO_CLAMP, "The net of a declared @exposed endpoint carries no declared clamp onto a protective/earth reference.", "'{0}' declares @exposed({1}) but net '{2}' carries no clamp onto a protective/earth reference — an exposed net is at the board's transient boundary, so a device on it must have its dump leg on a @role(protective)/@role(earth) conduit that the same scope declares @clamp(<ref>) on: add the clamp (an ESD array channel onto the protective island), or drop the @exposed when this endpoint is not at the transient boundary (exposed-protection-design.md §3, PWR-6)"),
    entry!(PROTECT_SHUNT_NO_REFERENCE, "A class declaring protect = shunt has no leg on a protective/earth reference.", "component '{0}' declares protect = shunt in its definition body but no leg of it lands on a reference its scope declares @role(protective)/@role(earth) (its legs carry {1}) — a shunt protection device must be able to dump the transient it exists for, so route one of its legs to the protective island, or drop the declaration when the device is not a shunt protection element (exposed-protection-design.md §4, PWR-5)"),
    entry!(PROTECT_SERIES_NOT_IN_PATH, "A class declaring protect = series is not a two-terminal element on a supply path.", "component '{0}' declares protect = series in its definition body but {1} — a series protection element (fuse/PTC) must carry the supply through itself, so it has to be a two-terminal device whose ends sit on two different nets, both on a supply tree. Put it in series on the path it protects instead of bypassing it or leaving an end off the supply tree (exposed-protection-design.md §4, PWR-5)"),
    entry!(RAIL_NATURE_MISMATCH, "A domain's @nature word contradicts the axis of a rail contract declared inside it.", "domain '{0}' declares @nature({1}) but its rail row writes {2} — @nature and the rail contract name the same axis, so writing both makes them agree: fix the @nature word, or the rail's `::` contract if the domain's word is the true one (ac-axis-interface-design.md §3.1)"),
    entry!(SHUNT_DISSIPATION_OVER_RATING, "An element's dissipation in place exceeds its declared package rating.", "'{0}' dissipates {1} W, above its declared power_rated {2} W — {3}: raise the resistance, use a package rated for more, or reduce the current or window it carries (package-thermal-design.md §3.1 shunt `V^2/R` / §7 series `I^2R`)"),
    entry!(DECOUPLING_RETURN_MISMATCH, "A decoupling capacitor's return leg does not land on the return member the rail it sits across declares.", "capacitor '{0}' sits across the declared rail {1} whose return member is {2}, but its other leg lands on {3} — a decoupling capacitor's two legs are one declared DC pair, so its return must close the loop the rail declares: move the return leg onto {2}, or declare the pair this capacitor actually bridges (power-quality-design.md §2.3, PI-3)"),
    entry!(BRIDGE_LOAD_DECOUPLING_MISSING, "A declared filter bridge's load side carries no decoupling capacitor.", "declared filter bridge {0} puts its load side on rail hot member '{1}' (domain {2}) but no capacitor sits on that net — the ferrite is the series half of a filter, so the LC only exists once the load side it protects carries a decoupling element: add the load-side capacitor (power-quality-design.md §2.2, PI-2)"),
    entry!(SINK_PIN_NO_DECOUPLING, "A sink power pin's declared DC pair carries no decoupling capacitor.", "sink pin '{0}' draws from the declared pair {1} / {2} but no capacitor sits on its hot net '{1}' — the pair is declared, so the load it feeds is expected to be decoupled across it: add a decoupling capacitor from '{1}' to '{2}' (where that capacitor's return leg lands is a separate finding, PI-3; power-quality-design.md §2.1, PI-1)"),
    entry!(SENSITIVE_RETURN_ON_NOISY, "A part supplied from a quiet/sensitive face returns into a noisy one.", "part '{0}' is supplied from the quiet/sensitive face {1}, but the return member '{2}' of that same declared pair lands on net '{3}' in the noisy face {4} — the plane a protected part returns to is part of its protection, so its return must close over the {1} reference instead: move that return member onto the quiet face's reference, or bridge the two faces through a filter declared for the crossing (power-quality-design.md §3.3, SN-3)"),
    entry!(ANALOG_RETURN_MISMATCH, "A part supplied from a declared analog face returns over a reference the face and the scope's analog port agree on.", "part '{0}' draws from the analog face {1} but its return member lands on '{2}', while the face's rail and the analog port of scope '{3}' both name the reference as {4} — a face's declared reference is the plane its protected parts are measured against, so the returns they actually close over must be that reference: land this return on {4}, or correct the rail's return member if the face really closes over '{2}' (power-quality-design.md §3.1, SN-1). Only the declared half is judged: a port stating no @return, and one whose reference names no face of its own scope, are not adjudicated (the source→sink chain has no carrier — §6 R3)"),
    entry!(SHARED_RETURN_BRIDGE, "A DC ground bridge joins a noisy face's return to a quiet/sensitive face's return with no filtering element on the leg.", "the ground bridge {0} puts the noisy face {1} and the quiet/sensitive face {2} on one copper, {3} — a filter is what lets a quiet face keep its own reference while the two coppers meet, so without one the plane the protected parts are measured against sits straight on the noise source's return: carry this leg with a ferrite/inductor (a declared filter, whose completeness PI-2 then judges — 6037), or keep the two returns apart and tie them only where the design declares the crossing (power-quality-design.md §3.2, SN-2)"),
    entry!(FILTER_SUBFACE_OVERREACH, "A sink drawing from a declared filter leg's load-side subface declares a supply pair other than the quiet domain's own.", "sink '{0}' declares the supply pair {1}, and that pair draws from the load side of the declared filter leg {3} — but the subface that leg protects is {4}, the {2} domain's own pair, so only a part declaring {4} is inside the domain the filter was declared for: declare this terminal's pair as {4}, or feed it from the rail its own domain declares and leave this filter's load side to the domain it protects (power-quality-design.md §2.4, PI-4)"),
    entry!(DOMAIN_ENDPOINT_NAME_COLLISION, "A name in a chain word position is both a whole-referenceable domain and an already-declared endpoint.", "'{0}' is declared as a domain whose one ::DC rail states the pair [{1}, {2}], and the same name is already an endpoint in this scope — the two readings name different nets, so this word has no single meaning: rename the domain (or the endpoint), or write the pair out as [{1}, {2}] at the word positions that meant the domain's pair (intent-reference-layer-design.md §10.5, R4)"),
    entry!(DOMAIN_NET_MIXED_BRIDGE, "A @bridge names a whole-referenceable domain on one side and a plain endpoint on the other.", "'{0}' names a domain whose one ::DC rail states the pair [{1}, {2}], but the other @bridge argument names a plain endpoint — one half reads as a directed rail member and the other as one conductor, so the pair states no single crossing: name domains on both sides for a domain-level bridge, or endpoint names on both sides for a net-level one (intent-reference-layer-design.md §10.4, R3)"),
    entry!(DOMAIN_BRIDGE_DIRECTION_REVERSED, "A licensed domain @bridge's argument order is reversed against the written order of the domain words on its chain.", "the @bridge names {0} first and {1} second, but on this chain the word for {1} is written left of the word for {0} — the same crossing written mirrored is one reading, and re-reading the chain from the other end to match the arguments would silently rewrite it: order the arguments as the domain words are written (intent-reference-layer-design.md §10.4, R3)"),
    entry!(DOMAIN_BRIDGE_LEG_INCONSISTENT, "A licensed domain bridge chain's two end words land on opposite sides of the named pair.", "this chain is licensed by @bridge({0}, {1}), but one end lands on a hot member and the other on a return member of the pair — a hot leg carrying a return-copper pin (or the reverse) does not state one crossing: point both ends at the side the chain's arrow direction takes, or split the legs into one hot chain and one return chain (intent-reference-layer-design.md §10.4, R3)"),
    entry!(DOMAIN_BRIDGE_DANGLING, "A named domain bridge pair has no witnessed leg anywhere in the module.", "the crossing @bridge({0}, {1}) names is witnessed by no chain in this module — every statement naming the pair lands its ends on words outside the pair's members, so the declared relation is realized by nothing: wire at least one leg between the two domains' members, or drop the @bridge when no crossing was meant (intent-reference-layer-design.md §10.4, R3)"),
    entry!(EXPOSED_NET_DOWNSTREAM_UNPROTECTED, "The clamp on a declared @exposed endpoint leaves an unprotected quiet/sensitive face downstream.", "'{0}' declares @exposed({1}) and is clamped, but net '{2}' — the quiet/sensitive face {3} — is reachable from it through transparent copper without crossing a declared series gate, and '{2}' carries no declared clamp of its own: a clamp covers its own side of every branch, so the transient the @exposed declares still pours into the quiet face. Clamp '{2}' as well, put a declared gate on this branch (protect = series on the fuse/ferrite it should pass), or drop the @exposed when this endpoint is not the boundary the threat enters from (exposed-protection-design.md §3.1, PWR-6)"),
    // section
    entry!(GATE_LITERAL_POINT, "R01 — a vector reference reached the netlist unexpanded (literal braces).", "unexpanded vector reference: {0}"),
    entry!(GATE_SHORT_PASSIVE, "R02 — both terminals of a two-terminal device land on the same net.", "two-terminal device short circuit: {0}"),
    entry!(GATE_SHORT_RAIL, "R03 — a net carries two different power-domain names.", "net contains both supply and ground: {0}"),
    entry!(GATE_RAIL_ALIAS, "R03a — a net carries multiple power-domain aliases.", "net contains multiple power-domain aliases: {0}"),
    entry!(GATE_SHORT_LANE, "R04 — two members of the same bus land on one net.", "bus lane short: {0}"),
    entry!(GATE_UNRESOLVED_UNIT, "R05 — a unit-typed argument claims no formal parameter slot.", "unit-typed argument claims no formal parameter slot"),
    entry!(GATE_MEGANET, "R06 — a non-power net is oversized.", "non-power net is a meganet: {0}"),
    entry!(GATE_GHOST_INSTANCE, "R07 — a net references a device missing from the instance table.", "unregistered device referenced in a net: {0}"),
    entry!(GATE_PHANTOM_PATH, "R08 — an endpoint path has an unregistered middle segment.", "phantom path: {0}"),
    entry!(GATE_FLOATING_POWER_PIN, "R09 — a device power/ground pin is left unconnected.", "floating power/ground pin: {0}"),
    entry!(GATE_SYMBOL_CONSERVATION, "R10 — pass2 device count fell below the pass1 symbol-table expectation.", "symbol conservation mismatch: {0}"),
    entry!(GATE_SPLIT_RAIL, "R11 — a same-name power rail is split into unconnected nets.", "split power rail: {0}"),
    entry!(GATE_DANGLING_PORT, "R12 — a port net holds only its own point.", "dangling port net: {0}"),
    entry!(GATE_ORPHAN_INSTANCE, "R14 — an instance is registered but appears in no net.", "orphan instance: {0}"),
    entry!(GATE_SYNTHETIC_PIN, "R15 — a synthetic terminal is not backed by any real pin.", "synthetic terminal: {0}"),
    entry!(CONN_REPLICATION_COUNT, "U155 — a connection replication count must be an int >= 2.", "replication count: {0}"),
    entry!(PIN_COPPER_EXPECTATION_MISMATCH, "A pin row declares an identity or signal-class expectation the net it lands on contradicts.", "pin '{0}' expects {1} but lands on class '{2}' — the class anchors neither a matching declared face nor a matching copper identity: bind the pin to a net of the expected identity, or change the expectation (pin-expectation-design.md §3-§4)"),
    entry!(PIN_COPPER_EXPECTATION_UNANCHORED, "A pin row declares an expectation but its net resolves no identity at all.", "pin '{0}' expects {1} but its net carries no declared identity — a bare net is no face at all: declare the copper (conduit or domain rail) in the owning module, or the expectation stays a wish (pin-expectation-design.md §3-§4)"),
    entry!(CROSS_BARRIER_NET, "Pins of two different @barrier groups on one component share a net — the declared isolation is bridged.", "component '{0}' carries barrier groups {1} on one net '{2}' — a group-wise isolation fact, physical by birth: split the net so no copper of this part reaches two groups, or drop the @barrier rows that overstate the part (barrier-design.md §3)"),
    entry!(IFACE_EXCLUSIVE_PEER_CONFLICT, "An exclusive interface role lane reaches more than one peer instance.", "interface '{0}' lane '{1}' on '{2}' adopts role '{3}', which declares `exclusive = true` — the lane's terminals must all pair with one peer instance, but they reach {4}: {5}. An exclusive pairing is one body to one body (a resonator body meets one oscillator body); wire the lane's terminals to a single peer, or drop the `exclusive` declaration if multi-peer pairing is intended (xtal-oscillator-design.md §2, U201 ①②)."),
    entry!(AC_FACE_RETURN_MISSING, "An energized AC mains face leaves one of its members unconnected.", "AC face '{0}' declares the member group ({1}), but only '{2}' reaches a net — '{3}' is on no net at all, which makes the face a single-line supply: the return is the conductor the working current comes home on. Wire the return member to the face's return conductor, or drop the whole row if the face is not used (ac-interface-design.md §7, U217)."),
    entry!(AC_NOMINAL_CONFLICT, "Two AC mains faces state different region nominals on one copper.", "the net '{0}' carries two declared AC region nominals on one copper: '{2}' states {3}, while '{4}' states {5} — different {1}, not one mains, and the copper cannot be both. State the region nominal at the consumer faces and keep the region-neutral empty form on the inlet components (ac-interface-design.md §5/§7, U217)."),
    entry!(PROTECTIVE_PIN_NO_COPPER, "A pin declaring @role(protective) or @role(earth) shares a net with no protective conductor.", "the pin '{0}' declares @role({1}), but its net '{2}' touches no conductor the owning scope declares protective or earth — the role word is a promise about the copper, and a plain net does not keep it. Wire the pin to a `conduit`/port declared `@role(protective)`/`@role(earth)` (the single-point the clamp rules read), or drop the role word if the terminal is not protective (ac-interface-design.md §4/§7, U217; the beta ruling: PE is not an interface member, it lives in the role machinery)."),
    entry!(IFACE_CHAIN_SOURCE_UNREACHED, "A sink-shaped adoption lane reaches no source of its family along the adoption chain.", "interface '{0}' lane '{1}' on '{2}' adopts role '{3}', whose pins all declare `in` — a sink-shaped lane — but no source endpoint of the same family is reachable along the adoption chain: the walk crossed every net the family's lanes lead to from here and found only further sink lanes. A unidirectional lane fed from nowhere is a dangling input — an orphan clock input is a clock that never arrives. Wire a source-role endpoint onto the chain (directly, or through a sink lane whose instance also declares a source pin of the same family), or drop the direction words if the lane is not genuinely a consumer (clock-intent-design.md §2.2, U112 ②)."),
    entry!(IFACE_ROLE_PEER_CONFLICT, "Two role-bearing endpoints of one family on a flat net are not mutual peers per the interface's role table.", "Net '{0}' joins '{1}' (role {2}) and '{3}' (role {4}) of family '{5}', but the roles are not mutual peers: the role block of '{2}' does not declare '{4}' in its `peer` set (and/or the reverse). The pair met only through role-less conductors — a wiring mediator or a module port — so the statement-level judge (4121) never saw it; the flat net walk did (replicated-binding-design.md §4 check 4). Give one endpoint the matching role, or extend the peer tables so the two roles name each other."),
    entry!(EXPECTATION_TARGET_MISSING, "An `expects` row names a target the built top does not contain.", "the `expects` row '{0}' addresses a {1}, but the built top has no {1} named '{0}' — only what the top instantiates or declares can carry an expectation (circuit-intent-acceptance-design.md §4)"),
    entry!(EXPECTATION_CLASS_MISMATCH, "The instance an `expects` row names instantiates no face matching the row's class word.", "the instance '{0}' instantiates '{1}', and none of its declared faces matches the expected '{2}' — a class row reads the same keys `::` binding reads: the class itself, its variant base chain, and its adopted recipes ({3}) (circuit-intent-acceptance-design.md §3-§4)"),
    entry!(EXPECTATION_NOT_DRIVEN, "The net an `expects = driven` row names carries no declared driver.", "the net '{0}' carries no declared driver — no endpoint on it is an `Out` pin or a declared power source, the same declared-face rule the undriven-net gate reads; wire a source onto it, or drop the `driven` row if the net is a passive branch (circuit-intent-acceptance-design.md §3-§4)"),
    entry!(EXPECTATION_VALUE_OUT_OF_WINDOW, "The net an `expects` row bounds declares a DC rail value outside the row's window.", "the net '{0}' declares {1}, which the window [{2}] of its `expects` row does not cover — the declared DC fact and the asked-for window disagree: widen the window, re-declare the rail, or rewire the net (circuit-intent-acceptance-design.md §5.1)"),
];
