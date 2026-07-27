// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # 网络形状 —— 旁挂式 provenance
//!
//! ## 为什么是旁挂而不是改结构
//!
//! `McVecNet` / `McVec` 已经有几百个调用点，整套 islands / sp_model /
//! ladder_model 都建在上面。**动它们的形状 = 全库重编译 + 全部回归失效。**
//!
//! 但真正丢失的信息只有三件：
//!   1. 这条连接在源码里是**第几道** lane（`visit.rs` 的 `for k in 0..max_w`）
//!   2. 源码里的**箭头方向**（`->` / `<-` / `-` / `+`）
//!   3. 这一段是**穿过哪个二端器件**产生的
//!
//! 所以做法是：`McVecNet` 加**一个** `Option<NetShape>` 字段，
//! `None` 时所有老代码行为逐位一致；有值时下游可以停止启发式反推。
//!
//! ```text
//! 改动面：
//!   McVec           0 个字段        ← 一动不动
//!   McVecNet        +1 个 Option    ← 老构造函数保持原签名
//!   ConnPair        +3 个字段       ← 构造点只有 4 处（visit.rs）
//!   下游消费方       0 处必须改       ← 想用才用，不用就当没有
//! ```
//!
//! ## 与 `connection_type()` 的关系
//!
//! `McVecNet::connection_type()` 是从**合并后的点对**反推形状，
//! 它是网络合并的副产物，会把等电位点误判成总线
//! （`fromblock.rs::is_real_bus` 的注释已经承认了这点）。
//!
//! `NetShape` 是**源码写的**形状，两者不同源。
//! 迁移路线：下游先读 `shape`，`None` 时再退回 `connection_type()`。
//! 等 `shape` 覆盖率稳定到 95%+，再给 `connection_type()` 挂 `#[deprecated]`。

use std::fmt;

// ============================================================================
// PairDir —— 源码里的箭头方向
// ============================================================================

/// 一段连接在源码里的方向。
///
/// 对应 `mcrule.md §10.1`：
/// - `->` 带方向串联 -> [`PairDir::LtoR`]
/// - `<-` 反向（规则文档标注为「保留，尚未完全支持」）-> [`PairDir::RtoL`]
/// - `-` 串联 / `+` 并联 -> [`PairDir::Undirected`]
///
/// ★ 这是布局搜索的方向锚。没有它，`t4_current` 这类用例里所有边都是
/// Neutral，最优解与其左右镜像代价完全相同，只能靠字典序决胜 ——
/// 也就是「镜像 bug」的真身。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PairDir {
    /// 左 -> 右
    LtoR,
    /// 右 -> 左
    RtoL,
    /// 无方向（`-` / `+`），或来源不明
    #[default]
    Undirected,
}

impl PairDir {
    /// 反转方向（交换 ConnPair 的 left/right 时用）
    pub fn flipped(self) -> Self {
        match self {
            PairDir::LtoR => PairDir::RtoL,
            PairDir::RtoL => PairDir::LtoR,
            PairDir::Undirected => PairDir::Undirected,
        }
    }

    pub fn is_directed(self) -> bool {
        !matches!(self, PairDir::Undirected)
    }
}

impl fmt::Display for PairDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PairDir::LtoR => write!(f, "->"),
            PairDir::RtoL => write!(f, "<-"),
            PairDir::Undirected => write!(f, "--"),
        }
    }
}

// ============================================================================
// LaneRef —— 向量的第几道
// ============================================================================

/// 一条连接属于向量的哪一道。
///
/// 来源：`visit.rs` 的 `for k in 0..max_w` 循环里的 `k` 与 `member_name_opt`。
/// 这两个值在那个循环里是**完整的**，现在被 `ConnPair` 抹平，
/// 然后由 `connection.rs::build_star_topology` 用频次统计猜回来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneRef {
    /// 第几道，从 0 开始
    pub index: u16,
    /// 这一道的成员名（`"P"` / `"VDD_3V3"` / `"SCLK"`），取不到时为 None
    pub name: Option<String>,
}

impl LaneRef {
    pub fn new(index: u16, name: Option<String>) -> Self {
        Self { index, name }
    }
}

impl fmt::Display for LaneRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(n) => write!(f, "[{}:{}]", self.index, n),
            None => write!(f, "[{}]", self.index),
        }
    }
}

// ============================================================================
// GroupRole —— 一组端点在源码里扮演什么
// ============================================================================

/// `McVecNet.nets` 里每一组的角色。
///
/// 注意与 `ConnectionType` 的区别：`ConnectionType` 是**推断**出来的拓扑，
/// 这个是**源码写的**角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRole {
    /// 单点：`GND`、`R1.1`
    Scalar,
    /// 总线的 N 道：`MIC{P,N}`、`[VDD_3V3, GND]`
    BusLanes(usize),
    /// 广播源：1 个点对 N 个点（`mcrule.md §10.4` 的「1 对多广播」）
    Broadcast(usize),
}

impl GroupRole {
    pub fn width(self) -> usize {
        match self {
            GroupRole::Scalar => 1,
            GroupRole::BusLanes(n) => n,
            GroupRole::Broadcast(_) => 1,
        }
    }

    pub fn is_bus(self) -> bool {
        matches!(self, GroupRole::BusLanes(n) if n >= 2)
    }
}

impl fmt::Display for GroupRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupRole::Scalar => write!(f, "1"),
            GroupRole::BusLanes(n) => write!(f, "bus{n}"),
            GroupRole::Broadcast(n) => write!(f, "1→{n}"),
        }
    }
}

// ============================================================================
// NetShape —— 挂在 McVecNet 上的那一个 Option
// ============================================================================

/// 一条网络在**源码里**的形状。
///
/// 全部字段都由 `visit.rs` 在建 `ConnPair` 的那一刻填，
/// 不做任何推断。取不到的字段留空，由日志里的覆盖率说话 ——
/// **绝不用启发式补**，否则就退化成现在这样：三层猜测互相打架。
#[derive(Debug, Clone, Default)]
pub struct NetShape {
    /// 各组的角色，顺序与 `McVecNet.nets` 一一对应
    pub groups: Vec<GroupRole>,

    /// 整条线的主方向（同一条 line 上多段方向不一致时取多数）
    pub dir: PairDir,

    /// 这条网所属的 lane（属于某个总线的第几道）；标量网为 None
    pub lane: Option<LaneRef>,

    /// 源码里这条网串过的二端器件（**顺序即拓扑顺序**）
    ///
    /// 用途：
    /// - `M4` 割集的强制规则「带里含无源器件 -> 永远 Wire」直接读这个，
    ///   不再用 `rails.rs` 的 `touches_passive` 启发式反推
    /// - `M3` 商图判断一条边是 SP 带还是 direct 带
    pub series_chain: Vec<i64>,

    /// 产生这条网的源码字节位置（诊断用）
    pub src_pos: Option<i32>,
}

impl NetShape {
    /// 这条网是不是总线的一道
    pub fn is_bus_lane(&self) -> bool {
        self.lane.is_some()
    }

    /// 总线宽度（取各组里最宽的），标量返回 1
    pub fn bus_width(&self) -> usize {
        self.groups.iter().map(|g| g.width()).max().unwrap_or(1)
    }

    /// 这条网穿过了无源器件（M4 的 forced-wire 判据）
    pub fn has_series_passive(&self) -> bool {
        !self.series_chain.is_empty()
    }

    /// 有没有实际信息 —— 全空的 shape 等价于 None，不要存
    pub fn is_informative(&self) -> bool {
        !self.groups.is_empty()
            || self.dir.is_directed()
            || self.lane.is_some()
            || !self.series_chain.is_empty()
    }
}

impl fmt::Display for NetShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let g: Vec<String> = self.groups.iter().map(|x| x.to_string()).collect();
        write!(f, "{} {}", g.join(" "), self.dir)?;
        if let Some(l) = &self.lane {
            write!(f, " lane{l}")?;
        }
        if !self.series_chain.is_empty() {
            write!(f, " via{:?}", self.series_chain)?;
        }
        Ok(())
    }
}

// ============================================================================
// 覆盖率统计 —— 这是本次改造唯一的验收指标
// ============================================================================

/// `shape` 的填充覆盖率。
///
/// **改造完成的判据不是「代码写完了」，是这张表里 `from_source` 占比 ≥ 90%。**
/// 覆盖率低说明还有路径在走旧的推断分支，那些路径就是下一批要修的。
///
/// ★ v4: `coverage()` = `from_source / total_nets`（有 shape 的网数 / 网总数），
/// 而非 `from_source / (from_source + inferred)`（那恒为 100%）。
#[derive(Debug, Default, Clone)]
pub struct ShapeStats {
    pub total: usize,
    pub total_nets: usize,
    pub from_source: usize,
    pub inferred: usize,
    pub dir_ltr: usize,
    pub dir_rtl: usize,
    pub dir_undirected: usize,
    pub bus_nets: usize,
    pub max_bus_width: usize,
    /// 没拿到 shape 的网名，用于定位下一个要修的路径
    pub uncovered: Vec<String>,
}

impl ShapeStats {
    pub fn observe(&mut self, name: &str, shape: Option<&NetShape>) {
        self.total += 1;
        match shape {
            Some(s) => {
                self.from_source += 1;
                match s.dir {
                    PairDir::LtoR => self.dir_ltr += 1,
                    PairDir::RtoL => self.dir_rtl += 1,
                    PairDir::Undirected => self.dir_undirected += 1,
                }
                if s.is_bus_lane() || s.bus_width() >= 2 {
                    self.bus_nets += 1;
                    self.max_bus_width = self.max_bus_width.max(s.bus_width());
                }
            }
            None => {
                self.inferred += 1;
                if self.uncovered.len() < 32 {
                    self.uncovered.push(name.to_string());
                }
            }
        }
    }

    pub fn coverage(&self) -> f64 {
        if self.total_nets == 0 {
            return 1.0;
        }
        self.from_source as f64 / self.total_nets as f64
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "[vec] SHAPE: from_source={} inferred={} (coverage {:.0}% = {}/{})\n",
            self.from_source,
            self.inferred,
            self.coverage() * 100.0,
            self.from_source,
            self.total_nets
        ));
        s.push_str(&format!(
            "[vec] DIR:   ltr={} rtl={} undirected={}\n",
            self.dir_ltr, self.dir_rtl, self.dir_undirected
        ));
        s.push_str(&format!(
            "[vec] LANES: bus nets={} max_width={}\n",
            self.bus_nets, self.max_bus_width
        ));
        if !self.uncovered.is_empty() {
            s.push_str(&format!("[vec] UNCOVERED: {}\n", self.uncovered.join(" ")));
        }
        s
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_flip() {
        assert_eq!(PairDir::LtoR.flipped(), PairDir::RtoL);
        assert_eq!(PairDir::Undirected.flipped(), PairDir::Undirected);
        assert!(PairDir::LtoR.is_directed());
        assert!(!PairDir::Undirected.is_directed());
    }

    #[test]
    fn empty_shape_is_not_informative() {
        // 全空的 shape 应该存 None，不要制造「有 shape 但没信息」的中间态
        assert!(!NetShape::default().is_informative());
        let s = NetShape {
            dir: PairDir::LtoR,
            ..Default::default()
        };
        assert!(s.is_informative());
    }

    #[test]
    fn bus_width() {
        let s = NetShape {
            groups: vec![GroupRole::BusLanes(2), GroupRole::BusLanes(2)],
            ..Default::default()
        };
        assert_eq!(s.bus_width(), 2);
        assert!(s.groups[0].is_bus());

        let scalar = NetShape {
            groups: vec![GroupRole::Scalar, GroupRole::Scalar],
            ..Default::default()
        };
        assert_eq!(scalar.bus_width(), 1);
    }

    #[test]
    fn stats_coverage() {
        let mut st = ShapeStats::default();
        let s = NetShape {
            dir: PairDir::LtoR,
            ..Default::default()
        };
        st.total_nets = 2;
        st.observe("a", Some(&s));
        st.observe("b", None);
        assert!((st.coverage() - 0.5).abs() < 1e-9);
        assert_eq!(st.uncovered, vec!["b".to_string()]);
    }
}
