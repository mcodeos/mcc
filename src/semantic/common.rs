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
use std::ops::Range;
use std::sync::Arc;

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

/// 源码连接符方向
///
/// 对应 mcrule.md §10.1：
/// - `->` → [`ConnDir::LtoR`]
/// - `<-` → [`ConnDir::RtoL`]（保留，尚未完全支持）
/// - `-` / `+` → [`ConnDir::Undirected`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnDir {
    /// 左 -> 右
    LtoR,
    /// 右 -> 左
    RtoL,
    /// 无方向（`-` / `+`）
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
    pub uri: McURI,   //dir.file
}

impl McSpaceName {
    pub(crate) fn new(ident: &McIds, uri: McURI) -> Self {
        Self {
            ident: ident.clone(),
            uri,
        }
    }
}

impl std::fmt::Display for McSpaceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ident_str = self.ident.to_string();
        // Pad ident to min 25 chars for alignment
        let padded = format!("{ident_str:<25}");
        write!(f, "{} @{}", padded, self.uri)
    }
}

pub type McURI = String;

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
    /// Scope path string (e.g. "US513.i2c").
    pub scope: String,
    /// The definition name.
    pub name: String,
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
// 向量形状 Shape + ShapeMatcher —— eval.md §1 / §3
// ============================================================================
//
// 纯函数实现，无任何依赖，供 semantic / instant / vector 各层复用。
// 语义来源：docs-new/concepts/vector-circuit/eval.md
//   - §1 形状系统（1*1 / 1*2 / N*1 / N*2 / N*M / 未知）
//   - §3 连接匹配约束表（行数必须相同）+ 连接结果表

/// 向量形状（行 × 列），对应 eval.md §1 形状系统。
///
/// | 形状 | 名称 | 构造器 |
/// |---|---|---|
/// | `1*1` | 节点 Node | [`Shape::node`] |
/// | `1*2` | 行向量 HVector | [`Shape::hvec`] |
/// | `N*1` | 列向量 VVector | [`Shape::vvec`] |
/// | `N*2` | 节点实例组合 | [`Shape::node_inst`] |
/// | `N*M` | 接口形状 | [`Shape::iface`] |
///
/// 未知形状（Pass1 尚未确定，如 FuncCall 返回值）用 [`Shape::unknown`]
/// （`rows == 0` 哨兵），匹配时视为可连接。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub rows: usize,
    pub cols: usize,
}

impl Shape {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }

    /// 1*1 节点（单点：标签 / 单管脚）
    pub fn node() -> Self {
        Self { rows: 1, cols: 1 }
    }

    /// 1*2 行向量（2 管脚元件缺省形状）
    pub fn hvec() -> Self {
        Self { rows: 1, cols: 2 }
    }

    /// N*1 列向量（`[VCC, GND]`、`TestPoint[1,2]`）
    pub fn vvec(rows: usize) -> Self {
        Self { rows, cols: 1 }
    }

    /// N*2 节点实例组合（N 个 2 端元件的行向量堆叠，`res[1:2]`）
    pub fn node_inst(rows: usize) -> Self {
        Self { rows, cols: 2 }
    }

    /// N*M 接口形状（`mcu.SPI`）
    pub fn iface(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }

    /// 未知形状（未解析 / Pass1 未定）
    pub fn unknown() -> Self {
        Self { rows: 0, cols: 0 }
    }

    pub fn is_unknown(&self) -> bool {
        self.rows == 0
    }

    /// 单行（1*1 或 1*2）
    pub fn is_row(&self) -> bool {
        self.rows == 1
    }

    /// 多行（N*1 / N*2 / N*M）
    pub fn is_multi_row(&self) -> bool {
        self.rows >= 2
    }

    /// N*M 接口形状：行 ≥ 2 且列 ≥ 3。
    /// `N*2`（cols == 2）是"节点实例组合"（[`Shape::node_inst`]），不是接口形状。
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

/// 连接匹配结果（eval.md §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchedShape {
    /// 结果形状：行不变，列取两边的较大者（§3 连接结果表）
    pub shape: Shape,
    /// `N*M` ↔ `N*M` 转为接口操作
    pub interface_op: bool,
}

/// 形状匹配错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeError {
    /// §3：行数必须相同才能连接
    RowMismatch { lhs: Shape, rhs: Shape },
}

/// §3 连接匹配约束表的纯函数实现。
pub struct ShapeMatcher;

impl ShapeMatcher {
    /// 匹配两个形状，返回连接结果形状。
    ///
    /// 规则（eval.md §3）：
    /// - 任一侧未知（`rows == 0`）→ 放行，结果取另一侧形状；
    /// - 行数不同 → [`ShapeError::RowMismatch`]（如 `1*1` vs `N*1`）；
    /// - 行数相同 → 结果行不变、列取较大者：
    ///   `1*1 +- 1*2 = 1*2`、`N*1 +- N*2 = N*2`；
    /// - 双侧均为 `N*M` → 结果带 `interface_op` 标记（转为接口操作）。
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

/// 连接运算符（eval.md §4）：`-`/`->`/`<-` 共享串联求值，`+` 为并联求值。
///
/// 与 [`ConnDir`] 的对应关系（mcrule.md §10.1）：
/// - `-` → `Series` + [`ConnDir::Undirected`]
/// - `->` → `Series` + [`ConnDir::LtoR`]
/// - `<-` → `Series` + [`ConnDir::RtoL`]
/// - `+` → `Parallel`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnOp {
    /// 串联 `-`（§4.1）/ `->`（§4.3）/ `<-`（§4.4）
    Series,
    /// 并联 `+`（§4.2）
    Parallel,
}

/// §4 单端口（1*1）代表规则：`+`/`-`/`<-` 结果取**运算数 1**，`->` 结果取**运算数 2**。
///
/// 落地位置：
/// - `+`：Pass2 `wire_parallel_internal` 以 opd[0] 为锚（"take operand 1"）；
/// - `->`：`MCAST_OPD_RIGHTARROW` 对链尾 `set_right_out` 作为输出端。
pub fn representative(dir: ConnDir, lhs: Shape, rhs: Shape) -> Shape {
    if dir == ConnDir::LtoR {
        rhs
    } else {
        lhs
    }
}

/// §4 四算子求值表：对 `(op, lhs, rhs)` 求连接后的**结果形状**。
///
/// 形状层的四条子表（§4.1-4.4）与 §3 连接结果表收敛为同一规则：
/// 行数必须相同（否则 [`ShapeError::RowMismatch`]），列取两边的较大者：
/// `1*1 +- 1*2 = 1*2`、`N*1 +- N*2 = N*2`、`N*2 +- N*2 = N*2`。
///
/// 子表间的差异在"锚点/拼接结构"（`向量 vs 节点` 返回 `newNode{向量, 节点右}`
/// 等），由 Pass2 点配对（[`ShapeMatcher`] + `try_connect_adjacent` /
/// `create_connection`）落地，形状层面收敛为同一结果。未知形状（`rows == 0`）
/// 通配放行，结果取另一侧。
///
/// 调用点：
/// - Pass1：`is_connectable`（`-`/`+`/`->`/`<-` 四个算子分支共享）；
/// - Pass2：`try_connect_adjacent`（line.rs 三条路径的相邻求值统一入口）。
pub fn eval_binary(op: ConnOp, lhs: Shape, rhs: Shape) -> Result<Shape, ShapeError> {
    // §4.1-4.4 形状收敛：`-`/`+`/`->`/`<-` 均为"行不变、列取较大"；
    // op 用于语义标注（锚点/配对策略在 Pass2 调用方落地），此处结果一致。
    debug_assert!(
        matches!(op, ConnOp::Series | ConnOp::Parallel),
        "unexpected operator"
    );
    let matched = ShapeMatcher::match_shape(lhs, rhs)?;
    Ok(matched.shape)
}

// ============================================================================
// §1 `_` 导线三种用法分类（eval.md §1）
// ============================================================================

/// `_` 的三种用法（eval.md §1）：
///
/// 占位与直通都是**导线**（[`McPhrase::Lead`]），区别在于出现位置：
/// - `[_, R101]` 向量内 → [占位][`LeadKind::Placeholder`]：保留位置，参与向量拼接和展开；
/// - `a1.gnd + _ + GND` 独立运算数 → [直通][`LeadKind::Passthrough`]：左侧输入不经处理
///   直接连到右侧，可用于串联链路中跳过节点。
///
/// [`LeadKind::PrefixId`] 前缀标识符**不是导线**——是 IDA 索引中的命名成员
/// （如 `M[1:4][_OPEN,_CLOSE]`），下划线是命名约定的前缀，与导线 `_` 含义不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeadKind {
    /// `[_, R101]` — 向量内占位：保留位置，参与向量拼接和展开
    Placeholder,
    /// `a1.gnd + _ + GND` — 独立运算数：直通连接（跳过节点）
    Passthrough,
    /// `_OPEN` — 前缀标识符：IDA 索引中的命名成员，不是导线
    PrefixId,
}

/// 分类单个 `_` token（§1）：裸 `_` 是导线（向量内 → 占位，否则 → 直通）；
/// `_FOO`（长度 > 1）是前缀标识符。
pub fn classify_lead(name: &str, in_vector: bool) -> LeadKind {
    if name != "_" {
        LeadKind::PrefixId
    } else if in_vector {
        LeadKind::Placeholder
    } else {
        LeadKind::Passthrough
    }
}

/// 遍历短语树，按出现顺序分类每个导线 `Lead` 的用法（§1）：
///
/// - 直接位于 `Multiple`（`[...]` 向量）内的 `Lead` → [占位][`LeadKind::Placeholder`]；
/// - 其余位置（Series / Parallel / Group / Transposed / Member 中的运算数）→
///   [直通][`LeadKind::Passthrough`]。
///
/// 前缀标识符 `_OPEN` 不是 `Lead`（解析为 Id/成员名），不会出现在结果中。
pub fn classify_phrase_leads(phrase: &McPhrase) -> Vec<LeadKind> {
    fn walk(p: &McPhrase, out: &mut Vec<LeadKind>) {
        match p {
            McPhrase::Lead => out.push(LeadKind::Passthrough),
            McPhrase::Multiple(inner) => {
                for m in inner {
                    if matches!(m, McPhrase::Lead) {
                        // 只统计直接成员：`[_, R101]` 中的 `_` 是占位；
                        // 嵌套表达式里的 `_`（如 `[a1.gnd + _ + GND, ...]`）走递归 → 直通。
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

    // ---- §3 匹配约束表（5×5 行数表）----

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
        // N 行数不同也拒绝（3*1 vs 4*1）
        assert!(ShapeMatcher::match_shape(Shape::vvec(3), Shape::vvec(4)).is_err());
    }

    #[test]
    fn match_interface_op_flag() {
        // N*M ↔ N*M → 接口操作
        let iface = Shape::iface(4, 4);
        let m = ShapeMatcher::match_shape(iface, iface).unwrap();
        assert!(m.interface_op);
        assert_eq!(m.shape, iface);
        // N*M ↔ N*1 正常匹配（不触发接口操作标记）
        let m2 = ShapeMatcher::match_shape(iface, Shape::vvec(4)).unwrap();
        assert!(!m2.interface_op);
    }

    #[test]
    fn match_unknown_wildcard() {
        // 未知形状放行，结果取已知侧
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
        // 双侧未知 → 返回未知
        assert_eq!(
            ShapeMatcher::match_shape(Shape::unknown(), Shape::unknown()),
            Ok(MatchedShape {
                shape: Shape::unknown(),
                interface_op: false,
            })
        );
    }

    // ---- §3 连接结果表（nets.mc 注释）----

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

    // ---- Shape 辅助判断 ----

    #[test]
    fn shape_classifiers() {
        assert!(Shape::node().is_row());
        assert!(Shape::hvec().is_row());
        assert!(!Shape::vvec(4).is_row());
        assert!(Shape::vvec(4).is_multi_row());
        assert!(Shape::iface(4, 3).is_interface());
        assert!(Shape::iface(4, 4).is_interface());
        assert!(!Shape::node_inst(4).is_interface()); // N*2（cols==2）是节点实例组合，不是 N*M 接口
        assert!(!Shape::vvec(4).is_interface());
        assert!(Shape::unknown().is_unknown());
        assert_eq!(format!("{}", Shape::vvec(4)), "4*1");
        assert_eq!(format!("{}", Shape::unknown()), "?");
    }

    // ---- §4 四算子求值表（eval_binary）----

    /// §4.1/4.3/4.4 串联：`-` / `->` / `<-` 的行约束与结果形状。
    #[test]
    fn eval_series_ok_and_rejected() {
        for op in [ConnOp::Series] {
            // 1*1 - 1*2 = 1*2（§4.1 节点 vs 向量 → newNode{节点左, 向量}）
            assert_eq!(
                eval_binary(op, Shape::node(), Shape::hvec()),
                Ok(Shape::hvec())
            );
            // 1*2 - 1*2 = 1*2（§4.1 向量 vs 向量 → 直接拼接，返回右向量）
            assert_eq!(
                eval_binary(op, Shape::hvec(), Shape::hvec()),
                Ok(Shape::hvec())
            );
            // N*1 - N*2 = N*2（§4.1 向量 vs 向量）
            assert_eq!(
                eval_binary(op, Shape::vvec(4), Shape::node_inst(4)),
                Ok(Shape::node_inst(4))
            );
            // N*2 - N*2 = N*2
            assert_eq!(
                eval_binary(op, Shape::node_inst(4), Shape::node_inst(4)),
                Ok(Shape::node_inst(4))
            );
            // 行数不同 → ❌（1*1 vs N*1）
            assert_eq!(
                eval_binary(op, Shape::node(), Shape::vvec(4)),
                Err(ShapeError::RowMismatch {
                    lhs: Shape::node(),
                    rhs: Shape::vvec(4),
                })
            );
        }
    }

    /// §4.2 并联 `+`：行约束与结果形状与串联一致（左运算数为锚）。
    #[test]
    fn eval_parallel_ok_and_rejected() {
        // R101 + R102 = [R101.1, R101.2]（返回左向量形状）
        assert_eq!(
            eval_binary(ConnOp::Parallel, Shape::hvec(), Shape::hvec()),
            Ok(Shape::hvec())
        );
        // N*2 + N*2 = N*2
        assert_eq!(
            eval_binary(ConnOp::Parallel, Shape::node_inst(3), Shape::node_inst(3)),
            Ok(Shape::node_inst(3))
        );
        // 行数不同 → ❌
        assert!(eval_binary(ConnOp::Parallel, Shape::vvec(2), Shape::vvec(3)).is_err());
    }

    /// §4 未知形状通配：任一侧 `rows == 0` → 放行，结果取已知侧。
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

    /// §4 单端口代表规则：`+`/`-`/`<-` 取运算数 1，`->` 取运算数 2。
    ///
    /// 代表侧形状（对 1*1 单端口）：
    /// - `->`（[`ConnDir::LtoR`]）→ op2；`VEXT -> power.v1v3` 输出端是 power.v1v3；
    /// - `-`/`+`（[`ConnDir::Undirected`]）→ op1；`VEXT - power.v1v3` 链首 VEXT；
    /// - `<-`（[`ConnDir::RtoL`]）→ op1；`DC.PVCC24 <- Diode(...)` 目标网 DC.PVCC24。
    #[test]
    fn representative_rule() {
        let lhs = Shape::node(); // 1*1
        let rhs = Shape::hvec(); // 1*2
                                 // `->`（LtoR）：结果取运算数 2
        assert_eq!(representative(ConnDir::LtoR, lhs, rhs), rhs);
        // `-` / `+`（Undirected）：结果取运算数 1
        assert_eq!(representative(ConnDir::Undirected, lhs, rhs), lhs);
        // `<-`（RtoL）：结果取运算数 1
        assert_eq!(representative(ConnDir::RtoL, lhs, rhs), lhs);
    }

    /// §4 代表规则对等量单端口（1*1 +- 1*1）两侧一致：形状无差别。
    #[test]
    fn representative_equal_single_ports() {
        let lhs = Shape::node();
        let rhs = Shape::node();
        for dir in [ConnDir::Undirected, ConnDir::LtoR, ConnDir::RtoL] {
            assert_eq!(representative(dir, lhs, rhs), lhs);
            assert_eq!(representative(dir, lhs, rhs), rhs);
        }
    }

    // ---- §1 `_` 三种用法分类（P5.1）----

    /// `classify_lead`：裸 `_` 是导线，按位置分占位/直通；`_FOO` 是前缀标识符。
    #[test]
    fn classify_lead_wire_vs_prefix_id() {
        // 导线 `_`：向量内 → 占位
        assert_eq!(classify_lead("_", true), LeadKind::Placeholder);
        // 导线 `_`：独立运算数 → 直通
        assert_eq!(classify_lead("_", false), LeadKind::Passthrough);
        // 前缀标识符：长度 > 1，不是导线
        assert_eq!(classify_lead("_OPEN", true), LeadKind::PrefixId);
        assert_eq!(classify_lead("_LEFT", false), LeadKind::PrefixId);
        assert_eq!(classify_lead("__CLR", false), LeadKind::PrefixId);
    }

    /// `classify_phrase_leads`：`[_, R101]` 向量内的 `_` → 占位。
    #[test]
    fn phrase_placeholder_in_vector() {
        let phrase = McPhrase::Multiple(vec![McPhrase::Lead, McPhrase::label("R101".into())]);
        assert_eq!(classify_phrase_leads(&phrase), vec![LeadKind::Placeholder]);
    }

    /// `classify_phrase_leads`：`a1.gnd + _ + GND` 独立运算数 → 直通。
    #[test]
    fn phrase_passthrough_operand() {
        let phrase = McPhrase::Parallel(vec![
            McPhrase::label("a1.gnd".into()),
            McPhrase::Lead,
            McPhrase::label("GND".into()),
        ]);
        assert_eq!(classify_phrase_leads(&phrase), vec![LeadKind::Passthrough]);
    }

    /// `classify_phrase_leads`：Series 链中的 `_` → 直通（`VEXT - _ - GND`）。
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

    /// `classify_phrase_leads`：嵌套表达式 `[a1.gnd + _ + GND, R101]` 中，
    /// 直接成员 `_` 才占位，嵌套 Parallel 里的 `_` 是直通。
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

    /// `classify_phrase_leads`：Group 运算数中的 `_` → 直通（`(a, b, c) - _`）。
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

    /// `classify_phrase_leads`：无 `_` 的短语 → 空列表。
    #[test]
    fn phrase_no_lead_empty() {
        let phrase = McPhrase::Series(
            vec![McPhrase::label("A".into()), McPhrase::label("B".into())],
            ConnDir::LtoR,
        );
        assert!(classify_phrase_leads(&phrase).is_empty());
    }
}
