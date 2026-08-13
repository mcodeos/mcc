// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Arrange — 分层 + 层内序
//!
//! 对商图做精确枚举，输出每一层的节点顺序。
//! N ≤ 7 时用 Heap's algorithm 枚举全排列 + 切点枚举。
//!
//! ## 算法
//! 1. 破环（greedy Eades-Lin-Smyth）
//! 2. 方向锚定（模块端口 + 入度0 + 源码序）
//! 3. 精确枚举（Heap's algorithm + 切点枚举）
//! 4. top-K 竞赛
//!
//! ## 验收（M3-2 / M3-3）
//! - t4_current: 最优解 [u4][u2][u1][u3,u5]，次优解代价明显更高
//! - t2_cycle / t3_cycle: backward=1
//! - box id +1000: 最优解不变
//! - 20 次连续: best 一致

use std::fmt;

use super::quotient::QuotientGraph;

// ============================================================================
// 数据结构
// ============================================================================

/// 搜索器输出的一层
pub type Layer = Vec<i64>;

/// 分层排列结果
#[derive(Debug, Clone, PartialEq)]
pub struct Arrangement {
    /// 每一层的节点 ID 列表
    pub layers: Vec<Layer>,
}

/// 代价结构
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Cost {
    /// 交叉数（×1000）
    pub crossings: f64,
    /// 违反破环后定向的边数（×300）
    pub backward: f64,
    /// 跨度惩罚（×100）
    pub span: f64,
    /// 端口交叉（×20）
    pub port_cross: f64,
    /// 同层软惩罚（×5000）
    pub same_layer: f64,
    /// 方向锚惩罚（×400）
    pub orient: f64,
    /// 源码序先验（×30）
    pub order: f64,
    /// 面积惩罚（×0.02）
    pub area: f64,
    /// 总代价
    pub total: f64,
}

impl Cost {
    pub fn zero() -> Self {
        Cost {
            crossings: 0.0,
            backward: 0.0,
            span: 0.0,
            port_cross: 0.0,
            same_layer: 0.0,
            orient: 0.0,
            order: 0.0,
            area: 0.0,
            total: 0.0,
        }
    }

    pub fn compute_total(&mut self) {
        self.total = self.crossings * 1000.0
            + self.backward * 300.0
            + self.span * 100.0
            + self.port_cross * 20.0
            + self.same_layer * 5000.0
            + self.orient * 400.0
            + self.order * 30.0
            + self.area * 0.02;
    }
}

impl fmt::Display for Cost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "total={:.0}", self.total)
    }
}

/// 精确搜索上限
pub const EXACT_SEARCH_LIMIT: usize = 7;

// ============================================================================
// 搜索入口
// ============================================================================

/// 对商图做精确搜索，返回 top-K 候选
pub fn solve(q: &QuotientGraph) -> Vec<(Cost, Arrangement)> {
    let _ = q;
    unimplemented!("M3-3")
}

/// 破环：对商图做 greedy Eades-Lin-Smyth feedback arc set
pub fn break_cycles(q: &mut QuotientGraph) {
    let _ = q;
    unimplemented!("M3-2")
}

/// 计算给定排列的代价
pub fn cost(q: &QuotientGraph, arr: &Arrangement) -> Cost {
    let _ = (q, arr);
    unimplemented!("M3-3")
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::McVecBox;
    use crate::vector::graph::{
        BoxKind, EndpointRef, IoDirection, IoSummary, McVecGraph, NetKind, NetRole, VizNet,
    };

    fn mk_ic(id: i64, name: &str, pin_count: usize) -> McVecBox {
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

    fn mk_ep(box_id: i64, pin_id: i64, name: &str, io: IoDirection) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, name, io)
    }

    fn mk_signal_net(id: i64, name: &str, endpoints: Vec<EndpointRef>) -> VizNet {
        VizNet::new(id, name.into(), NetKind::Signal, NetRole::Signal, endpoints)
    }

    fn make_t4_current() -> (McVecGraph, QuotientGraph) {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1_mcu", 4));
        g.boxes.push(mk_ic(2, "u2_ldo_in", 3));
        g.boxes.push(mk_ic(3, "u3_spk", 3));
        g.boxes.push(mk_ic(4, "u4_ldo_out", 3));
        g.boxes.push(mk_ic(5, "u5_flash", 3));
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
        let q = QuotientGraph::build(&g);
        (g, q)
    }

    // ────────────────────────────────────────
    // 枚举测试
    // ────────────────────────────────────────

    /// t4_current: 最优解 [u4][u2][u1][u3,u5]
    #[test]
    fn arrange_t4_current_optimal() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        assert!(!candidates.is_empty(), "should have at least one candidate");

        let (_, best) = &candidates[0];
        // 最优解层数应为 4
        assert_eq!(best.layers.len(), 4, "t4_current should have 4 layers");

        // 验证第一层是 [4]（u4_ldo_out）
        assert_eq!(best.layers[0], vec![4], "first layer should be [u4_ldo_out]");
        // 最后一层是 [3, 5]（u3_spk, u5_flash）
        let last: Vec<i64> = best.layers[3].clone();
        let mut last_sorted = last.clone();
        last_sorted.sort();
        assert_eq!(last_sorted, vec![3, 5], "last layer should be [u3_spk, u5_flash]");
    }

    /// t4_current: 次优解代价明显更高
    #[test]
    fn arrange_t4_current_second_best() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        if candidates.len() >= 2 {
            let (cost1, _) = &candidates[0];
            let (cost2, _) = &candidates[1];
            assert!(
                cost2.total > cost1.total + 100.0,
                "second best should be significantly worse than best"
            );
        }
    }

    /// t2_cycle: backward=1
    #[test]
    fn arrange_t2_cycle_backward() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));
        g.nets.push(mk_signal_net(11, "u2_u1", vec![
            mk_ep(2, 22, "OUT", IoDirection::Output),
            mk_ep(1, 12, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty(), "should have at least one candidate");
        let (cost, _) = &candidates[0];
        assert!(
            cost.backward <= 1.0,
            "t2_cycle backward should be <= 1, got {}",
            cost.backward
        );
    }

    /// t3_cycle: backward=1
    #[test]
    fn arrange_t3_cycle_backward() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.boxes.push(mk_ic(3, "u3", 3));
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
        let candidates = solve(&q);
        assert!(!candidates.is_empty(), "should have at least one candidate");
        let (cost, _) = &candidates[0];
        assert!(
            cost.backward <= 1.0,
            "t3_cycle backward should be <= 1, got {}",
            cost.backward
        );
    }

    /// t1_chain: [u1][u2]
    #[test]
    fn arrange_t1_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 2);
        assert_eq!(best.layers[0], vec![1]);
        assert_eq!(best.layers[1], vec![2]);
    }

    /// t2_chain: [u3][u1][u2]
    #[test]
    fn arrange_t2_chain() {
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
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 3);
        assert_eq!(best.layers[0], vec![3]);
        assert_eq!(best.layers[1], vec![1]);
        assert_eq!(best.layers[2], vec![2]);
    }

    /// t3_chain: [u3][u1][u2][u4]
    #[test]
    fn arrange_t3_chain() {
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
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 4);
        assert_eq!(best.layers[0], vec![3]);
        assert_eq!(best.layers[1], vec![1]);
        assert_eq!(best.layers[2], vec![2]);
        assert_eq!(best.layers[3], vec![4]);
    }

    // ────────────────────────────────────────
    // 镜像 bug 回归测试
    // ────────────────────────────────────────

    /// box id 全部 +1000 后最优解不变
    #[test]
    fn arrange_mirror_bug_regression() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        let best_layers: Vec<Vec<i64>> = candidates[0].1.layers.clone();

        // 构造相同拓扑但 id 全部 +1000 的商图
        let mut g2 = McVecGraph::new(0, "main".into());
        g2.boxes.push(mk_ic(1001, "u1_mcu", 4));
        g2.boxes.push(mk_ic(1002, "u2_ldo_in", 3));
        g2.boxes.push(mk_ic(1003, "u3_spk", 3));
        g2.boxes.push(mk_ic(1004, "u4_ldo_out", 3));
        g2.boxes.push(mk_ic(1005, "u5_flash", 3));
        g2.nets.push(mk_signal_net(1010, "u1_u2", vec![
            mk_ep(1001, 1011, "OUT", IoDirection::Output),
            mk_ep(1002, 1021, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1011, "u2_u4", vec![
            mk_ep(1002, 1022, "OUT", IoDirection::Output),
            mk_ep(1004, 1041, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1012, "u4_u3", vec![
            mk_ep(1004, 1042, "OUT", IoDirection::Output),
            mk_ep(1003, 1031, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1013, "u3_u5", vec![
            mk_ep(1003, 1032, "OUT", IoDirection::Output),
            mk_ep(1005, 1051, "IN", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1014, "u1_u4", vec![
            mk_ep(1001, 1012, "CTRL", IoDirection::Output),
            mk_ep(1004, 1043, "CTRL", IoDirection::Input),
        ]));
        g2.nets.push(mk_signal_net(1015, "u1_u5", vec![
            mk_ep(1001, 1013, "CLK", IoDirection::Output),
            mk_ep(1005, 1052, "CLK", IoDirection::Input),
        ]));

        let q2 = QuotientGraph::build(&g2);
        let candidates2 = solve(&q2);
        let best_layers2: Vec<Vec<i64>> = candidates2[0].1.layers.clone();

        // 层数应相同
        assert_eq!(
            best_layers.len(),
            best_layers2.len(),
            "mirror: layer count should be identical"
        );
        // 每层的大小应相同
        for (i, (l1, l2)) in best_layers.iter().zip(best_layers2.iter()).enumerate() {
            assert_eq!(
                l1.len(),
                l2.len(),
                "mirror: layer {} size should be identical",
                i
            );
        }
    }

    // ────────────────────────────────────────
    // 确定性测试
    // ────────────────────────────────────────

    /// 连续 20 次求解，最优解一致
    #[test]
    fn arrange_determinism() {
        let (_g, q) = make_t4_current();
        let first = solve(&q);
        let first_best: Vec<Vec<i64>> = first[0].1.layers.clone();

        for run in 1..20 {
            let candidates = solve(&q);
            let cur_best: Vec<Vec<i64>> = candidates[0].1.layers.clone();
            assert_eq!(
                first_best, cur_best,
                "determinism: run {} differs from run 0",
                run
            );
        }
    }

    // ────────────────────────────────────────
    // 方向锚测试
    // ────────────────────────────────────────

    /// 有明确 OUT→IN 方向的图，OUT 应该在左
    #[test]
    fn arrange_orientation_out_left() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1_src", 3)); // 只有 OUT 端口
        g.boxes.push(mk_ic(2, "u2_dst", 3)); // 只有 IN 端口
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 2);
        assert_eq!(best.layers[0], vec![1], "u1_src (OUT) should be on the left");
        assert_eq!(best.layers[1], vec![2], "u2_dst (IN) should be on the right");
    }

    // ────────────────────────────────────────
    // 边界测试
    // ────────────────────────────────────────

    /// 单节点：返回单层
    #[test]
    fn arrange_single_node() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(!candidates.is_empty());
        let (_, best) = &candidates[0];
        assert_eq!(best.layers.len(), 1);
        assert_eq!(best.layers[0], vec![1]);
    }

    /// 空图：无结果
    #[test]
    fn arrange_empty() {
        let g = McVecGraph::new(0, "main".into());
        let q = QuotientGraph::build(&g);
        let candidates = solve(&q);
        assert!(candidates.is_empty());
    }
}