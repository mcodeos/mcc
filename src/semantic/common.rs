// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::component::McComponent;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::{
    McIds, MCAST_IOTYPE, MCAST_IOTYPE_ANL, MCAST_IOTYPE_IN, MCAST_IOTYPE_IO, MCAST_IOTYPE_LABEL,
    MCAST_IOTYPE_NC, MCAST_IOTYPE_OUT, MCAST_IOTYPE_PS, MCAST_IOTYPE_RETURN,
};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

#[derive(Debug, Clone)]
pub enum IOType {
    In,
    Out,
    InOut,
    Power,
    Analog,
    Return,
    NonCon,
    Label,
    None,
}

impl IOType {
    pub(crate) fn new(node: &AstNode) -> Option<IOType> {
        if node.get_type() != MCAST_IOTYPE {
            return None;
        }
        if let Some(subnode) = node.get_sub_node() {
            match subnode.get_type() {
                MCAST_IOTYPE_IN => return Some(IOType::In),
                MCAST_IOTYPE_OUT => return Some(IOType::Out),
                MCAST_IOTYPE_IO => return Some(IOType::InOut),
                MCAST_IOTYPE_PS => return Some(IOType::Power),
                MCAST_IOTYPE_ANL => return Some(IOType::Analog),
                MCAST_IOTYPE_RETURN => return Some(IOType::Return),
                MCAST_IOTYPE_NC => return Some(IOType::NonCon),
                MCAST_IOTYPE_LABEL => return Some(IOType::Label),
                _ => return Some(IOType::None),
            }
        }
        None
    }
}

/// Source connection operator direction.
///
/// Maps to mcrule.md §10.1:
/// - `->` → [`ConnDir::LtoR`]
/// - `<-` → [`ConnDir::RtoL`] (kept, not yet fully supported)
/// - `-` / `+` → [`ConnDir::Undirected`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnDir {
    /// Left to right
    LtoR,
    /// Right to left
    RtoL,
    /// Undirected (`-` / `+`)
    #[default]
    Undirected,
}

pub enum McCMIE {
    Component(Arc<McComponent>),
    Module(Arc<McModule>),
    Interface(Arc<McInterface>),
    Enum(Arc<McEnumDef>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct McSpaceName {
    pub ident: McIds, //comp/mod/ifs/enum
    pub uri: UriId,   //dir.file (interned)
}

impl McSpaceName {
    pub(crate) fn new(ident: &McIds, uri: McURI) -> Self {
        Self {
            ident: ident.clone(),
            uri: uri_intern(&uri),
        }
    }

    /// Resolve the interned URI back to a string (output / serialization use).
    pub(crate) fn uri_string(&self) -> Arc<str> {
        uri_resolve(self.uri)
    }
}

impl std::fmt::Display for McSpaceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ident_str = self.ident.to_string();
        // Pad ident to min 25 chars for alignment
        let padded = format!("{ident_str:<25}");
        write!(f, "{} @{}", padded, self.uri_string())
    }
}

pub type McURI = String;

// ============================================================================
// SourcePos: unified source position (design: expansion-provenance.md §7.11(3))
// ============================================================================

/// Unified source position: file URI + absolute byte offset.
///
/// Line / column are derived on demand from the owning file's content
/// (`line_of_byte`); they are never stored in the position itself
/// (decision A, §7.1). Replaces the historical mixed forms
/// `Option<i32>` / `u32` / `(McURI, u32)` across the Pass2 instantiation
/// layer and its consumers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourcePos {
    /// URI of the file that contains the position.
    pub uri: McURI,
    /// Absolute byte offset in that file.
    pub offset: u32,
}

impl SourcePos {
    pub fn new(uri: impl Into<McURI>, offset: u32) -> Self {
        SourcePos {
            uri: uri.into(),
            offset,
        }
    }

    /// Offset as `usize` for slicing / line-index lookups.
    pub fn offset_usize(&self) -> usize {
        self.offset as usize
    }
}

// ============================================================================
// UriId: global append-only URI interning (design: name-space-global.md §5.5)
// ============================================================================

/// Globally-unique id for an interned file URI. Ids are never recycled, so a
/// published `UriId` stays valid across unload/reset (entries may stop being
/// referenced, but the id itself never dangles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UriId(pub u32);

impl UriId {
    /// Resolve this id back to its URI string.
    pub fn as_uri(&self) -> Arc<str> {
        uri_resolve(*self)
    }

    /// Suffix test against the resolved URI string (convenience so call sites
    /// can keep `space.uri.ends_with(x)` after the String → UriId migration).
    pub fn ends_with(&self, pat: &str) -> bool {
        self.as_uri().ends_with(pat)
    }

    /// Substring test against the resolved URI string (same convenience).
    pub fn contains(&self, pat: &str) -> bool {
        self.as_uri().contains(pat)
    }
}

impl std::fmt::Display for UriId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_uri())
    }
}

// Equality against plain strings, so call sites can compare an interned id
// with a raw URI without resolving explicitly (`space.uri == some_uri`).
impl PartialEq<str> for UriId {
    fn eq(&self, other: &str) -> bool {
        self.as_uri().as_ref() == other
    }
}
impl PartialEq<&str> for UriId {
    fn eq(&self, other: &&str) -> bool {
        self.as_uri().as_ref() == *other
    }
}
impl PartialEq<String> for UriId {
    fn eq(&self, other: &String) -> bool {
        self.as_uri().as_ref() == other.as_str()
    }
}
impl PartialEq<UriId> for str {
    fn eq(&self, other: &UriId) -> bool {
        self == other.as_uri().as_ref()
    }
}
impl PartialEq<UriId> for String {
    fn eq(&self, other: &UriId) -> bool {
        self.as_str() == other.as_uri().as_ref()
    }
}

/// Append-only interning table: `id → uri` and `uri → id`.
struct UriTable {
    strings: Vec<Arc<str>>,
    ids: HashMap<String, UriId>,
}

impl UriTable {
    fn new() -> Self {
        Self {
            // id 0 is reserved for the empty uri (matches the legacy
            // file_id 0 = "" convention of the removed per-file file tables).
            strings: vec![Arc::from("")],
            ids: HashMap::new(),
        }
    }

    fn intern(&mut self, uri: &str) -> UriId {
        if let Some(id) = self.ids.get(uri) {
            return *id;
        }
        let id = UriId(self.strings.len() as u32);
        self.strings.push(Arc::from(uri));
        self.ids.insert(uri.to_string(), id);
        id
    }

    fn resolve(&self, id: UriId) -> Arc<str> {
        self.strings.get(id.0 as usize).cloned().unwrap_or_default()
    }
}

/// Process-global URI table (append-only; never cleared — `mcc_reset` only
/// stops referencing old entries, the ids stay valid).
static URI_TABLE: LazyLock<Mutex<UriTable>> = LazyLock::new(|| Mutex::new(UriTable::new()));

/// Intern `uri`, returning its stable global id.
pub fn uri_intern(uri: &str) -> UriId {
    URI_TABLE.lock().unwrap().intern(uri)
}

/// Resolve `id` back to its URI string.
pub fn uri_resolve(id: UriId) -> Arc<str> {
    URI_TABLE.lock().unwrap().resolve(id)
}

/// Resolve a raw interned file id (e.g. `SourceLocation.file_id`) to its URI.
pub fn uri_of_file_id(file_id: u32) -> Arc<str> {
    uri_resolve(UriId(file_id))
}

// ============================================================================
// ScopePath: hierarchical container chain for def/ref positioning
// ============================================================================

/// Kind of a container in the scope hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerKind {
    /// Inside a function body
    Function,
    /// Inside a component definition
    Component,
    /// Inside a module definition
    Module,
    /// Inside an interface definition
    Interface,
    /// Inside an enum definition
    Enum,
    /// At file level (no parent container)
    File,
}

impl ContainerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "func",
            Self::Component => "component",
            Self::Module => "module",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::File => "file",
        }
    }
}

/// A single container in the scope chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerInfo {
    pub kind: ContainerKind,
    pub name: String,
}

impl ContainerInfo {
    pub fn new(kind: ContainerKind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_string(),
        }
    }
}

/// Full hierarchical position of a def or ref.
///
/// Encodes the chain from inner to outer:
///   func → container (component/module/interface/enum) → file → project → libs
///
/// Used for priority-based lookup: when resolving a reference, search from
/// the innermost scope outward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopePath {
    /// Source file URI
    pub uri: McURI,
    /// Innermost function name (if inside a function body)
    pub func: Option<String>,
    /// Direct parent container
    pub container: ContainerInfo,
    /// Container chain from inner to outer (includes file-level)
    pub container_chain: Vec<ContainerInfo>,
}

impl ScopePath {
    /// Create a file-level ScopePath (no container).
    pub fn file_level(uri: &McURI) -> Self {
        Self {
            uri: uri.clone(),
            func: None,
            container: ContainerInfo::new(ContainerKind::File, ""),
            container_chain: vec![],
        }
    }

    /// Create a module-level ScopePath.
    pub fn module(uri: &McURI, mod_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: None,
            container: ContainerInfo::new(ContainerKind::Module, mod_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Create a component-level ScopePath.
    pub fn component(uri: &McURI, comp_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: None,
            container: ContainerInfo::new(ContainerKind::Component, comp_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Create a function-level ScopePath inside a module.
    pub fn func_in_module(uri: &McURI, mod_name: &str, func_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: Some(func_name.to_string()),
            container: ContainerInfo::new(ContainerKind::Module, mod_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Create a function-level ScopePath inside a component.
    pub fn func_in_component(uri: &McURI, comp_name: &str, func_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: Some(func_name.to_string()),
            container: ContainerInfo::new(ContainerKind::Component, comp_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Build the scope string for name_to_declare_id key.
    /// Format: `"Container.func"` or `"Container"` if no func.
    pub fn scope_key(&self) -> String {
        match &self.func {
            Some(f) => format!("{}.{}", self.container.name, f),
            None => self.container.name.clone(),
        }
    }

    /// Priority level for lookup (higher = more inner = checked first).
    ///   func=5  component/module=4  file=3  project=2  libs=1
    pub fn priority(&self) -> u8 {
        if self.func.is_some() {
            5
        } else {
            match self.container.kind {
                ContainerKind::Component
                | ContainerKind::Module
                | ContainerKind::Interface
                | ContainerKind::Enum => 4,
                ContainerKind::File => 3,
                ContainerKind::Function => 5,
            }
        }
    }
}

impl Default for ScopePath {
    fn default() -> Self {
        Self {
            uri: McURI::new(),
            func: None,
            container: ContainerInfo::new(ContainerKind::File, ""),
            container_chain: vec![],
        }
    }
}

// ============================================================================
// Unified Lookup types (shared by pass1/pass2, F12, Hover, Completion)
// ============================================================================

/// Layered completion space a lookup result belongs to (§5 of the completion
/// design). Mirrors the P1-P5 name-space layering plus the special spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpaceLayer {
    /// Func space — params and labels of the current function (innermost).
    P1,
    /// Container space — ports, instances, params, pins, funcs of the
    /// current module/component.
    P2,
    /// Current file top-level classes (module/component/interface/enum).
    P3,
    /// Use-chain visibility — cross-file classes reachable from the file.
    P4,
    /// mcode system library classes (outermost).
    P5,
    /// Member access space (`uC.PA1`, `UART.TTL`).
    Member,
    /// Net expression space.
    Net,
    /// Instance declaration space.
    Instance,
    /// Attribute assignment space.
    Attr,
    /// Use-path space.
    UsePath,
}

impl SpaceLayer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
            Self::P4 => "P4",
            Self::P5 => "P5",
            Self::Member => "Member",
            Self::Net => "Net",
            Self::Instance => "Instance",
            Self::Attr => "Attr",
            Self::UsePath => "UsePath",
        }
    }
}

/// Result of a single symbol lookup.
#[derive(Debug, Clone)]
pub struct LookupResult {
    /// URI of the file where the definition was found.
    pub uri: McURI,
    /// Byte range of the definition in the source file.
    pub span: Range<usize>,
    /// Symbol kind for IDE features.
    pub kind: LookupSymbolKind,
    /// The container that owns this definition.
    pub container: Option<ContainerInfo>,
    /// Scope path string (e.g. "mod.sub.i2c").
    pub scope: String,
    /// The definition name.
    pub name: String,
    /// Layered completion space this result belongs to.
    pub layer: SpaceLayer,
}

/// Symbol kind for unified lookup results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupSymbolKind {
    Component,
    Module,
    Interface,
    Enum,
    EnumValue,
    Function,
    Port,
    Label,
    Param,
    Pin,
    Instance,
    Define,
    Role,
    Unknown,
}

impl LookupSymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Module => "module",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::EnumValue => "enum_value",
            Self::Function => "function",
            Self::Port => "port",
            Self::Label => "label",
            Self::Param => "param",
            Self::Pin => "pin",
            Self::Instance => "instance",
            Self::Define => "define",
            Self::Role => "role",
            Self::Unknown => "unknown",
        }
    }
}

/// Filter for `unified_lookup_all()`.
#[derive(Debug, Clone, Default)]
pub struct ScopeFilter {
    /// Only include results from this kind.
    pub kind: Option<ContainerKind>,
    /// Only include results whose name starts with this prefix.
    pub prefix: Option<String>,
    /// Max results to return.
    pub limit: Option<usize>,
}

impl ScopeFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_kind(mut self, kind: ContainerKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

#[allow(dead_code)]
pub fn print_backtrace(label: &str) {
    mcc_dbg!("sem::comp", "\n=== BACKTRACE: {label} ===");
    let bt = std::backtrace::Backtrace::capture();
    mcc_dbg!("sem::comp", "{bt}");
}

// ============================================================================
// Vector shape Shape + ShapeMatcher —— eval.md §1 / §3
// ============================================================================
//
// Pure function implementation with no dependencies, reused by the
// semantic / instant / vector layers.
// Semantics source: docs-new/concepts/vector-circuit/eval.md
//   - §1 shape system (1*1 / 1*2 / N*1 / N*2 / N*M / unknown)
//   - §3 connection matching constraint table (row counts must match) + connection result table

/// Vector shape (rows × cols), matching the eval.md §1 shape system.
///
/// | Shape | Name | Constructor |
/// |---|---|---|
/// | `1*1` | Node | [`Shape::node`] |
/// | `1*2` | Row vector HVector | [`Shape::hvec`] |
/// | `N*1` | Column vector VVector | [`Shape::vvec`] |
/// | `N*2` | Node-instance combination | [`Shape::node_inst`] |
/// | `N*M` | Interface shape | [`Shape::iface`] |
///
/// The unknown shape (not yet determined in Pass1, e.g. a FuncCall return value)
/// uses [`Shape::unknown`] (a `rows == 0` sentinel) and is treated as connectable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub rows: usize,
    pub cols: usize,
}

impl Shape {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }

    /// 1*1 node (single point: label / single pin)
    pub fn node() -> Self {
        Self { rows: 1, cols: 1 }
    }

    /// 1*2 row vector (default shape for 2-pin devices)
    pub fn hvec() -> Self {
        Self { rows: 1, cols: 2 }
    }

    /// N*1 column vector (`[VCC, GND]`, `TestPoint[1,2]`)
    pub fn vvec(rows: usize) -> Self {
        Self { rows, cols: 1 }
    }

    /// N*2 node-instance combination (N stacked row vectors of 2-pin devices, `res[1:2]`)
    pub fn node_inst(rows: usize) -> Self {
        Self { rows, cols: 2 }
    }

    /// N*M interface shape (`mcu.SPI`)
    pub fn iface(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }

    /// Unknown shape (unresolved / undetermined in Pass1)
    pub fn unknown() -> Self {
        Self { rows: 0, cols: 0 }
    }

    pub fn is_unknown(&self) -> bool {
        self.rows == 0
    }

    /// Single row (1*1 or 1*2)
    pub fn is_row(&self) -> bool {
        self.rows == 1
    }

    /// Multiple rows (N*1 / N*2 / N*M)
    pub fn is_multi_row(&self) -> bool {
        self.rows >= 2
    }

    /// N*M interface shape: rows ≥ 2 and cols ≥ 3.
    /// `N*2` (cols == 2) is a "node-instance combination" ([`Shape::node_inst`]),
    /// not an interface shape.
    pub fn is_interface(&self) -> bool {
        self.rows >= 2 && self.cols >= 3
    }
}

impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_unknown() {
            write!(f, "?")
        } else {
            write!(f, "{}*{}", self.rows, self.cols)
        }
    }
}

/// Connection matching result (eval.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchedShape {
    /// Result shape: rows unchanged, columns take the larger of the two (§3 connection result table)
    pub shape: Shape,
    /// `N*M` ↔ `N*M` becomes an interface operation
    pub interface_op: bool,
}

/// Shape matching error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeError {
    /// §3: row counts must match to connect
    RowMismatch { lhs: Shape, rhs: Shape },
}

/// Pure function implementation of the §3 connection matching constraint table.
pub struct ShapeMatcher;

impl ShapeMatcher {
    /// Match two shapes and return the resulting connection shape.
    ///
    /// Rules (eval.md §3):
    /// - Either side unknown (`rows == 0`) → pass; the result takes the other side's shape;
    /// - Row counts differ → [`ShapeError::RowMismatch`] (e.g. `1*1` vs `N*1`);
    /// - Row counts match → rows unchanged, columns take the larger:
    ///   `1*1 +- 1*2 = 1*2`, `N*1 +- N*2 = N*2`;
    /// - Both sides are `N*M` → the result carries the `interface_op` flag (becomes an interface operation).
    pub fn match_shape(lhs: Shape, rhs: Shape) -> Result<MatchedShape, ShapeError> {
        if lhs.is_unknown() {
            return Ok(MatchedShape {
                shape: rhs,
                interface_op: false,
            });
        }
        if rhs.is_unknown() {
            return Ok(MatchedShape {
                shape: lhs,
                interface_op: false,
            });
        }
        if lhs.rows != rhs.rows {
            return Err(ShapeError::RowMismatch { lhs, rhs });
        }
        let interface_op = lhs.is_interface() && rhs.is_interface();
        Ok(MatchedShape {
            shape: Shape::new(lhs.rows, lhs.cols.max(rhs.cols)),
            interface_op,
        })
    }
}

/// Connection operator (eval.md §4): `-`/`->`/`<-` share series evaluation, `+` is parallel.
///
/// Correspondence with [`ConnDir`] (mcrule.md §10.1):
/// - `-` → `Series` + [`ConnDir::Undirected`]
/// - `->` → `Series` + [`ConnDir::LtoR`]
/// - `<-` → `Series` + [`ConnDir::RtoL`]
/// - `+` → `Parallel`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnOp {
    /// Series `-` (§4.1) / `->` (§4.3) / `<-` (§4.4)
    Series,
    /// Parallel `+` (§4.2)
    Parallel,
}

/// §4 single-port (1*1) representative rule: `+`/`-`/`<-` take **operand 1**, `->` takes **operand 2**.
///
/// Where it lands:
/// - `+`: Pass2 `wire_parallel_internal` anchors on opd[0] ("take operand 1");
/// - `->`: `MCAST_OPD_RIGHTARROW` calls `set_right_out` on the chain tail as the output end.
pub fn representative(dir: ConnDir, lhs: Shape, rhs: Shape) -> Shape {
    if dir == ConnDir::LtoR {
        rhs
    } else {
        lhs
    }
}

/// §4 four-operator evaluation table: given `(op, lhs, rhs)`, compute the
/// **result shape** of the connection.
///
/// The four shape-level sub-tables (§4.1-4.4) converge with the §3 connection
/// result table on the same rule: row counts must match (otherwise
/// [`ShapeError::RowMismatch`]), columns take the larger of the two:
/// `1*1 +- 1*2 = 1*2`, `N*1 +- N*2 = N*2`, `N*2 +- N*2 = N*2`.
///
/// The sub-tables only differ in the "anchor/splice structure" (e.g. `vector vs
/// node` returns `newNode{vector, node-right}`), which is materialized by the
/// Pass2 point pairing ([`ShapeMatcher`] + `try_connect_adjacent` /
/// `create_connection`); at the shape layer they converge to the same result.
/// Unknown shapes (`rows == 0`) pass through as a wildcard and take the other side.
///
/// Call sites:
/// - Pass1: `is_connectable` (shared by the four operator branches `-`/`+`/`->`/`<-`);
/// - Pass2: `try_connect_adjacent` (the unified entry for adjacent evaluation on
///   the three paths in line.rs).
pub fn eval_binary(op: ConnOp, lhs: Shape, rhs: Shape) -> Result<Shape, ShapeError> {
    // §4.1-4.4 shape convergence: `-`/`+`/`->`/`<-` all follow "rows unchanged,
    // columns take the larger"; op only provides semantic annotation (anchor/pairing
    // strategy is materialized in the Pass2 callers), the result here is identical.
    debug_assert!(
        matches!(op, ConnOp::Series | ConnOp::Parallel),
        "unexpected operator"
    );
    let matched = ShapeMatcher::match_shape(lhs, rhs)?;
    Ok(matched.shape)
}

// ============================================================================
// §1 classification of the three uses of the `_` wire (eval.md §1)
// ============================================================================

/// The three uses of `_` (eval.md §1):
///
/// Both placeholder and passthrough are **wires** ([`McPhrase::Lead`]); they differ
/// only in where they appear:
/// - Inside a vector `[_, R101]` → [placeholder][`LeadKind::Placeholder`]: keeps its
///   position and participates in vector splicing and expansion;
/// - As an independent operand `a1.gnd + _ + GND` → [passthrough][`LeadKind::Passthrough`]:
///   the left-side input is routed straight to the right side without processing,
///   which can be used to skip a node in a series chain.
///
/// A [`LeadKind::PrefixId`] prefix identifier is **not a wire** — it is a named member
/// of an IDA index (e.g. `M[1:4][_OPEN,_CLOSE]`); the underscore is the naming
/// convention's prefix and means something different from the wire `_`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeadKind {
    /// `[_, R101]` — placeholder inside a vector: keeps its position, participates in vector splicing and expansion
    Placeholder,
    /// `a1.gnd + _ + GND` — independent operand: passthrough connection (skips a node)
    Passthrough,
    /// `_OPEN` — prefix identifier: a named member of an IDA index, not a wire
    PrefixId,
}

/// Classify a single `_` token (§1): a bare `_` is a wire (inside a vector →
/// placeholder, otherwise → passthrough); `_FOO` (length > 1) is a prefix identifier.
pub fn classify_lead(name: &str, in_vector: bool) -> LeadKind {
    if name != "_" {
        LeadKind::PrefixId
    } else if in_vector {
        LeadKind::Placeholder
    } else {
        LeadKind::Passthrough
    }
}

/// Walk the phrase tree and classify the use of every wire `Lead` in order of
/// appearance (§1):
///
/// - A `Lead` directly inside a `Multiple` (`[...]` vector) → [placeholder][`LeadKind::Placeholder`];
/// - Any other position (an operand in Series / Parallel / Group / Transposed / Member) →
///   [passthrough][`LeadKind::Passthrough`].
///
/// A prefix identifier `_OPEN` is not a `Lead` (it parses to an Id/member name),
/// so it never appears in the results.
pub fn classify_phrase_leads(phrase: &McPhrase) -> Vec<LeadKind> {
    fn walk(p: &McPhrase, out: &mut Vec<LeadKind>) {
        match p {
            McPhrase::Lead => out.push(LeadKind::Passthrough),
            McPhrase::Multiple(inner) => {
                for m in inner {
                    if matches!(m, McPhrase::Lead) {
                        // Count only direct members: `_` in `[_, R101]` is a placeholder;
                        // `_` inside nested expressions (e.g. `[a1.gnd + _ + GND, ...]`)
                        // recurses → passthrough.
                        out.push(LeadKind::Placeholder);
                    } else {
                        walk(m, out);
                    }
                }
            }
            McPhrase::Series(inner, _) | McPhrase::Parallel(inner) => {
                for m in inner {
                    walk(m, out);
                }
            }
            McPhrase::Transposed(inner) => walk(inner, out),
            McPhrase::Group(g) => {
                for opd in &g.opds {
                    walk(opd, out);
                }
            }
            McPhrase::Member(inner, _) => walk(inner, out),
            McPhrase::FuncCall(f) => {
                if let Some(caller) = &f.caller {
                    walk(caller, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(phrase, &mut out);
    out
}

#[cfg(test)]
mod shape_tests {
    use super::*;
    use crate::semantic::basic::mc_group::McGroup;

    // ---- §3 matching constraint table (5×5 row-count table) ----

    #[test]
    fn match_row_vs_row_ok() {
        // 1*1 vs 1*1 → 1*1
        assert_eq!(
            ShapeMatcher::match_shape(Shape::node(), Shape::node()),
            Ok(MatchedShape {
                shape: Shape::node(),
                interface_op: false,
            })
        );
        // 1*1 vs 1*2 → 1*2
        assert_eq!(
            ShapeMatcher::match_shape(Shape::node(), Shape::hvec()),
            Ok(MatchedShape {
                shape: Shape::hvec(),
                interface_op: false,
            })
        );
        // 1*2 vs 1*2 → 1*2
        assert_eq!(
            ShapeMatcher::match_shape(Shape::hvec(), Shape::hvec()),
            Ok(MatchedShape {
                shape: Shape::hvec(),
                interface_op: false,
            })
        );
    }

    #[test]
    fn match_multi_row_vs_multi_row_ok() {
        // N*1 vs N*1 → N*1
        let lhs = Shape::vvec(4);
        assert_eq!(
            ShapeMatcher::match_shape(lhs, Shape::vvec(4)),
            Ok(MatchedShape {
                shape: Shape::vvec(4),
                interface_op: false,
            })
        );
        // N*1 vs N*2 → N*2
        assert_eq!(
            ShapeMatcher::match_shape(lhs, Shape::node_inst(4)),
            Ok(MatchedShape {
                shape: Shape::node_inst(4),
                interface_op: false,
            })
        );
        // N*2 vs N*2 → N*2
        assert_eq!(
            ShapeMatcher::match_shape(Shape::node_inst(4), Shape::node_inst(4)),
            Ok(MatchedShape {
                shape: Shape::node_inst(4),
                interface_op: false,
            })
        );
    }

    #[test]
    fn match_row_vs_multi_row_rejected() {
        // 1*1 vs N*1 → ❌
        assert_eq!(
            ShapeMatcher::match_shape(Shape::node(), Shape::vvec(4)),
            Err(ShapeError::RowMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(4),
            })
        );
        // 1*2 vs N*1 → ❌
        assert_eq!(
            ShapeMatcher::match_shape(Shape::hvec(), Shape::vvec(3)),
            Err(ShapeError::RowMismatch {
                lhs: Shape::hvec(),
                rhs: Shape::vvec(3),
            })
        );
        // N*2 vs 1*1 → ❌
        assert_eq!(
            ShapeMatcher::match_shape(Shape::node_inst(2), Shape::node()),
            Err(ShapeError::RowMismatch {
                lhs: Shape::node_inst(2),
                rhs: Shape::node(),
            })
        );
        // Different N row counts are also rejected (3*1 vs 4*1)
        assert!(ShapeMatcher::match_shape(Shape::vvec(3), Shape::vvec(4)).is_err());
    }

    #[test]
    fn match_interface_op_flag() {
        // N*M ↔ N*M → interface operation
        let iface = Shape::iface(4, 4);
        let m = ShapeMatcher::match_shape(iface, iface).unwrap();
        assert!(m.interface_op);
        assert_eq!(m.shape, iface);
        // N*M ↔ N*1 matches normally (no interface-op flag)
        let m2 = ShapeMatcher::match_shape(iface, Shape::vvec(4)).unwrap();
        assert!(!m2.interface_op);
    }

    #[test]
    fn match_unknown_wildcard() {
        // Unknown shape passes through; the result takes the known side
        assert_eq!(
            ShapeMatcher::match_shape(Shape::unknown(), Shape::vvec(4)),
            Ok(MatchedShape {
                shape: Shape::vvec(4),
                interface_op: false,
            })
        );
        assert_eq!(
            ShapeMatcher::match_shape(Shape::node(), Shape::unknown()),
            Ok(MatchedShape {
                shape: Shape::node(),
                interface_op: false,
            })
        );
        // Both sides unknown → returns unknown
        assert_eq!(
            ShapeMatcher::match_shape(Shape::unknown(), Shape::unknown()),
            Ok(MatchedShape {
                shape: Shape::unknown(),
                interface_op: false,
            })
        );
    }

    // ---- §3 connection result table (nets.mc comment) ----

    #[test]
    fn result_table_matches_spec() {
        let cases = [
            (Shape::node(), Shape::node(), Shape::node()), // 1*1 +- 1*1 = 1*1
            (Shape::node(), Shape::hvec(), Shape::hvec()), // 1*1 +- 1*2 = 1*2
            (Shape::hvec(), Shape::hvec(), Shape::hvec()), // 1*2 +- 1*2 = 1*2
            (
                Shape::vvec(4),
                Shape::vvec(4),
                Shape::vvec(4), // N*1 +- N*1 = N*1
            ),
            (
                Shape::vvec(4),
                Shape::node_inst(4),
                Shape::node_inst(4), // N*1 +- N*2 = N*2
            ),
            (
                Shape::node_inst(4),
                Shape::node_inst(4),
                Shape::node_inst(4), // N*2 +- N*2 = N*2
            ),
        ];
        for (l, r, expected) in cases {
            assert_eq!(
                ShapeMatcher::match_shape(l, r).unwrap().shape,
                expected,
                "match {l} x {r}"
            );
        }
    }

    // ---- Shape helper predicates ----

    #[test]
    fn shape_classifiers() {
        assert!(Shape::node().is_row());
        assert!(Shape::hvec().is_row());
        assert!(!Shape::vvec(4).is_row());
        assert!(Shape::vvec(4).is_multi_row());
        assert!(Shape::iface(4, 3).is_interface());
        assert!(Shape::iface(4, 4).is_interface());
        assert!(!Shape::node_inst(4).is_interface()); // N*2 (cols == 2) is a node-instance combination, not an N*M interface shape
        assert!(!Shape::vvec(4).is_interface());
        assert!(Shape::unknown().is_unknown());
        assert_eq!(format!("{}", Shape::vvec(4)), "4*1");
        assert_eq!(format!("{}", Shape::unknown()), "?");
    }

    // ---- §4 four-operator evaluation table (eval_binary) ----

    /// §4.1/4.3/4.4 series: row constraints and result shapes for `-` / `->` / `<-`.
    #[test]
    fn eval_series_ok_and_rejected() {
        for op in [ConnOp::Series] {
            // 1*1 - 1*2 = 1*2 (§4.1 node vs vector → newNode{node-left, vector})
            assert_eq!(
                eval_binary(op, Shape::node(), Shape::hvec()),
                Ok(Shape::hvec())
            );
            // 1*2 - 1*2 = 1*2 (§4.1 vector vs vector → direct splice, returns the right vector)
            assert_eq!(
                eval_binary(op, Shape::hvec(), Shape::hvec()),
                Ok(Shape::hvec())
            );
            // N*1 - N*2 = N*2 (§4.1 vector vs vector)
            assert_eq!(
                eval_binary(op, Shape::vvec(4), Shape::node_inst(4)),
                Ok(Shape::node_inst(4))
            );
            // N*2 - N*2 = N*2
            assert_eq!(
                eval_binary(op, Shape::node_inst(4), Shape::node_inst(4)),
                Ok(Shape::node_inst(4))
            );
            // Row counts differ → ❌ (1*1 vs N*1)
            assert_eq!(
                eval_binary(op, Shape::node(), Shape::vvec(4)),
                Err(ShapeError::RowMismatch {
                    lhs: Shape::node(),
                    rhs: Shape::vvec(4),
                })
            );
        }
    }

    /// §4.2 parallel `+`: row constraints and result shape match series (the left operand is the anchor).
    #[test]
    fn eval_parallel_ok_and_rejected() {
        // R101 + R102 = [R101.1, R101.2] (returns the left vector shape)
        assert_eq!(
            eval_binary(ConnOp::Parallel, Shape::hvec(), Shape::hvec()),
            Ok(Shape::hvec())
        );
        // N*2 + N*2 = N*2
        assert_eq!(
            eval_binary(ConnOp::Parallel, Shape::node_inst(3), Shape::node_inst(3)),
            Ok(Shape::node_inst(3))
        );
        // Row counts differ → ❌
        assert!(eval_binary(ConnOp::Parallel, Shape::vvec(2), Shape::vvec(3)).is_err());
    }

    /// §4 unknown shape wildcard: either side `rows == 0` → pass; the result takes the known side.
    #[test]
    fn eval_unknown_wildcard() {
        for op in [ConnOp::Series, ConnOp::Parallel] {
            assert_eq!(
                eval_binary(op, Shape::unknown(), Shape::vvec(4)),
                Ok(Shape::vvec(4))
            );
            assert_eq!(
                eval_binary(op, Shape::hvec(), Shape::unknown()),
                Ok(Shape::hvec())
            );
        }
    }

    /// §4 single-port representative rule: `+`/`-`/`<-` take **operand 1**, `->` takes **operand 2**.
    ///
    /// Representative-side shape (for a 1*1 single port):
    /// - `->` ([`ConnDir::LtoR`]) → op2; the output end of `VEXT -> power.v1v3` is power.v1v3;
    /// - `-`/`+` ([`ConnDir::Undirected`]) → op1; the head of the `VEXT - power.v1v3` chain is VEXT;
    /// - `<-` ([`ConnDir::RtoL`]) → op1; the target net of `DC.PVCC24 <- Diode(...)` is DC.PVCC24.
    #[test]
    fn representative_rule() {
        let lhs = Shape::node(); // 1*1
        let rhs = Shape::hvec(); // 1*2
                                 // `->` (LtoR): result takes operand 2
        assert_eq!(representative(ConnDir::LtoR, lhs, rhs), rhs);
        // `-` / `+` (Undirected): result takes operand 1
        assert_eq!(representative(ConnDir::Undirected, lhs, rhs), lhs);
        // `<-` (RtoL): result takes operand 1
        assert_eq!(representative(ConnDir::RtoL, lhs, rhs), lhs);
    }

    /// §4 representative rule for equal single ports (1*1 +- 1*1): both sides agree, no shape difference.
    #[test]
    fn representative_equal_single_ports() {
        let lhs = Shape::node();
        let rhs = Shape::node();
        for dir in [ConnDir::Undirected, ConnDir::LtoR, ConnDir::RtoL] {
            assert_eq!(representative(dir, lhs, rhs), lhs);
            assert_eq!(representative(dir, lhs, rhs), rhs);
        }
    }

    // ---- §1 classification of the three uses of `_` (P5.1) ----

    /// `classify_lead`: a bare `_` is a wire, classified as placeholder/passthrough by position; `_FOO` is a prefix identifier.
    #[test]
    fn classify_lead_wire_vs_prefix_id() {
        // Wire `_`: inside a vector → placeholder
        assert_eq!(classify_lead("_", true), LeadKind::Placeholder);
        // Wire `_`: independent operand → passthrough
        assert_eq!(classify_lead("_", false), LeadKind::Passthrough);
        // Prefix identifier: length > 1, not a wire
        assert_eq!(classify_lead("_OPEN", true), LeadKind::PrefixId);
        assert_eq!(classify_lead("_LEFT", false), LeadKind::PrefixId);
        assert_eq!(classify_lead("__CLR", false), LeadKind::PrefixId);
    }

    /// `classify_phrase_leads`: `_` inside the `[_, R101]` vector → placeholder.
    #[test]
    fn phrase_placeholder_in_vector() {
        let phrase = McPhrase::Multiple(vec![McPhrase::Lead, McPhrase::label("R101".into())]);
        assert_eq!(classify_phrase_leads(&phrase), vec![LeadKind::Placeholder]);
    }

    /// `classify_phrase_leads`: independent operand `a1.gnd + _ + GND` → passthrough.
    #[test]
    fn phrase_passthrough_operand() {
        let phrase = McPhrase::Parallel(vec![
            McPhrase::label("a1.gnd".into()),
            McPhrase::Lead,
            McPhrase::label("GND".into()),
        ]);
        assert_eq!(classify_phrase_leads(&phrase), vec![LeadKind::Passthrough]);
    }

    /// `classify_phrase_leads`: `_` in a Series chain → passthrough (`VEXT - _ - GND`).
    #[test]
    fn phrase_passthrough_in_series() {
        let phrase = McPhrase::Series(
            vec![
                McPhrase::label("VEXT".into()),
                McPhrase::Lead,
                McPhrase::label("GND".into()),
            ],
            ConnDir::Undirected,
        );
        assert_eq!(classify_phrase_leads(&phrase), vec![LeadKind::Passthrough]);
    }

    /// `classify_phrase_leads`: in the nested expression `[a1.gnd + _ + GND, R101]`,
    /// only a direct member `_` is a placeholder; a `_` inside the nested Parallel is a passthrough.
    #[test]
    fn phrase_nested_expression_keeps_passthrough() {
        let nested = McPhrase::Parallel(vec![
            McPhrase::label("a1.gnd".into()),
            McPhrase::Lead,
            McPhrase::label("GND".into()),
        ]);
        let phrase = McPhrase::Multiple(vec![nested, McPhrase::label("R101".into())]);
        assert_eq!(classify_phrase_leads(&phrase), vec![LeadKind::Passthrough]);
    }

    /// `classify_phrase_leads`: `_` in a Group operand → passthrough (`(a, b, c) - _`).
    #[test]
    fn phrase_passthrough_in_group() {
        let group = McGroup {
            opds: vec![
                McPhrase::label("a".into()),
                McPhrase::label("b".into()),
                McPhrase::Lead,
            ],
            left_match: true,
            right_match: true,
        };
        let phrase = McPhrase::Group(group);
        assert_eq!(classify_phrase_leads(&phrase), vec![LeadKind::Passthrough]);
    }

    /// `classify_phrase_leads`: a phrase without `_` → empty list.
    #[test]
    fn phrase_no_lead_empty() {
        let phrase = McPhrase::Series(
            vec![McPhrase::label("A".into()), McPhrase::label("B".into())],
            ConnDir::LtoR,
        );
        assert!(classify_phrase_leads(&phrase).is_empty());
    }
}
