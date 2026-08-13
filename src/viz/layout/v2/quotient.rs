// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Quotient — 商图
//!
//! 将 zone 内器件之间的连接压成超边，供搜索器使用。
//!
//! ## 算法
//! 1. nodes = 所有 band 的端子 IC 并集
//! 2. 每条 band 一条边（允许重边）
//! 3. 方向优先级：模块端口 > 引脚 io_type > Neutral
//! 4. Rail net 不产生边
//!
//! ## 验收（M3-1）
//! - t4_current.mc: nodes=5, edges=6
//! - 所有 edge 的 w/h 非零

use std::collections::BTreeMap;

use crate::vector::graph::{McVecGraph, NetRole};

// ============================================================================
// 数据结构
// ============================================================================

/// 商图节点 ID（对应 box_id）
pub type NodeId = i64;

/// 边方向偏好（输入给搜索器，不是硬约束）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    /// 从左到右
    LeftToRight,
    /// 从右到左
    RightToLeft,
    /// 不明确
    Neutral,
}

/// 商图中一条边（两个端子 IC 之间的连接）
#[derive(Debug, Clone)]
pub struct QEdge {
    /// 源节点
    pub src: NodeId,
    /// 目标节点
    pub dst: NodeId,
    /// 方向偏好
    pub prefer: Direction,
    /// 该带的像素宽度（SP 的 COL_W=120，ladder 的 col_step=224）
    pub w: f64,
    /// 该带的像素高度（同端子数）
    pub h: f64,
    /// 边标签（用于调试）
    pub label: String,
}

/// 商图
#[derive(Debug, Clone)]
pub struct QuotientGraph {
    /// 节点 ID 列表（已排序）
    pub nodes: Vec<NodeId>,
    /// 边列表
    pub edges: Vec<QEdge>,
    /// 节点 ID → 显示标签
    pub labels: BTreeMap<NodeId, String>,
}

impl QuotientGraph {
    /// 从 graph 构建商图
    pub fn build(_graph: &McVecGraph) -> Self {
        unimplemented!("M3-1")
    }
}

// ============================================================================
// 测试辅助函数
// ============================================================================

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;
    use crate::vector::graph::boxdef::McVecBox;
    use crate::vector::graph::{BoxKind, EndpointRef, IoDirection, IoSummary, NetKind, VizNet};

    /// 构造一个多端子 IC box
    pub fn mk_ic(id: i64, name: &str, pin_count: usize) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::SubModule,
            crate::vector::graph::Symbol::Module,
            None,
            None,
            pin_count,
            IoSummary::new(),
            format!("main.{}", name),
            Vec::new(),
        )
    }

    /// 构造一个被动器件（R/C/L）
    pub fn mk_passive(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::TwoPin,
            crate::vector::graph::Symbol::Unknown,
            None,
            None,
            2,
            IoSummary::new(),
            format!("main.{}", name),
            Vec::new(),
        )
    }

    /// 构造一条信号 net（指定端点）
    pub fn mk_signal_net(
        id: i64,
        name: &str,
        endpoints: Vec<EndpointRef>,
    ) -> VizNet {
        VizNet::new(id, name.into(), NetKind::Signal, NetRole::Signal, endpoints)
    }

    /// 构造一个端点（带方向）
    pub fn mk_ep(box_id: i64, pin_id: i64, name: &str, io: IoDirection) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, name, io)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;
    use crate::vector::graph::boxdef::McVecBox;
    use crate::vector::graph::{BoxKind, EndpointRef, IoDirection, IoSummary, NetKind, NetRole, VizNet};

    /// ── t4_current: 5 节点 6 条边，最优解 [u4][u2][u1][u3,u5] ──
    ///
    /// 拓扑：
    ///   u1(mcu) ←→ u2(ldo_in) ←→ u4(ldo_out) ←→ u3(spk) ←→ u5(flash)
    ///   所有边都是 Neutral（IN↔IN、OUT↔OUT），纯靠方向锚 + 字典序。
    fn make_t4_current() -> McVecGraph {
        let mut g = McVecGraph::new(0, "main".into());
        // 5 个 IC
        g.boxes.push(mk_ic(1, "u1_mcu", 4));
        g.boxes.push(mk_ic(2, "u2_ldo_in", 3));
        g.boxes.push(mk_ic(3, "u3_spk", 3));
        g.boxes.push(mk_ic(4, "u4_ldo_out", 3));
        g.boxes.push(mk_ic(5, "u5_flash", 3));

        // 6 条边（全部 Neutral 方向）
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u4", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(4, 41, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(12, "u4_u3", vec![
            mk_ep(4, 42, "OUT", IoDirection::Output),
            mk_ep(3, 31, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(13, "u3_u5", vec![
            mk_ep(3, 32, "OUT", IoDirection::Output),
            mk_ep(5, 51, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(14, "u1_u4", vec![
            mk_ep(1, 12, "CTRL", IoDirection::Output),
            mk_ep(4, 43, "CTRL", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(15, "u1_u5", vec![
            mk_ep(1, 13, "CLK", IoDirection::Output),
            mk_ep(5, 52, "CLK", IoDirection::Input),
        ]));
        g
    }

    #[test]
    fn quotient_t4_current() {
        let g = make_t4_current();
        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 5, "t4_current: 5 nodes");
        assert_eq!(q.edges.len(), 6, "t4_current: 6 edges");
        // 所有边不应有零宽高
        for e in &q.edges {
            assert!(e.w > 0.0, "edge {:?} has zero width", e.label);
            assert!(e.h > 0.0, "edge {:?} has zero height", e.label);
        }
    }

    /// ── t2_cycle: 2 节点循环 → backward=1 ──
    #[test]
    fn quotient_t2_cycle() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        // 双向连接形成循环
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u1", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(1, 12, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 2);
        assert_eq!(q.edges.len(), 2);
    }

    /// ── t3_cycle: 3 节点循环 → backward=1 ──
    #[test]
    fn quotient_t3_cycle() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        // u1→u2→u3→u1 循环
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u3", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(3, 31, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(12, "u3_u1", vec![
            mk_ep(3, 32, "OUT", IoDirection::Output),
            mk_ep(1, 12, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 3);
        assert_eq!(q.edges.len(), 3);
    }

    /// ── t1_chain: 单边 → [u1][u2] ──
    #[test]
    fn quotient_t1_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 2);
        assert_eq!(q.edges.len(), 1);
    }

    /// ── t2_chain: 线性链 → [u3][u1][u2] ──
    #[test]
    fn quotient_t2_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.nets.push(mk_signal_net(10, "u3_u1", vec![
            mk_ep(3, 31, "OUT", IoDirection::Output),
            mk_ep(1, 11, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u1_u2", vec![
            mk_ep(1, 12, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 3);
        assert_eq!(q.edges.len(), 2);
    }

    /// ── t3_chain: 4 节点线性链 → [u3][u1][u2][u4] ──
    #[test]
    fn quotient_t3_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
        g.boxes.push(mk_ic(4, "u4", 3));
        g.nets.push(mk_signal_net(10, "u3_u1", vec![
            mk_ep(3, 31, "OUT", IoDirection::Output),
            mk_ep(1, 11, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u1_u2", vec![
            mk_ep(1, 12, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(12, "u2_u4", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(4, 41, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 4);
        assert_eq!(q.edges.len(), 3);
    }

    /// ── Rail net 不产生边 ──
    #[test]
    fn quotient_rail_nets_ignored() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(100, "V3V3", 1));
        // 信号 net
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        // 电源 net（应被忽略）
        g.nets.push(VizNet::new(
            11, "V3V3".into(), NetKind::Power, NetRole::Rail,
            vec![
                EndpointRef::with_io(100, 1001, "V3V3", IoDirection::Power),
                EndpointRef::with_io(1, 12, "VDD", IoDirection::Power),
                EndpointRef::with_io(2, 22, "VDD", IoDirection::Power),
            ],
        ));

        let q = QuotientGraph::build(&g);
        assert_eq!(q.nodes.len(), 2, "rail nets should not add nodes");
        assert_eq!(q.edges.len(), 1, "rail nets should not produce edges");
    }
}