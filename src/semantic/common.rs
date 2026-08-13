// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
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

#[cfg(test)]
mod shape_tests {
    use super::*;

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
}
