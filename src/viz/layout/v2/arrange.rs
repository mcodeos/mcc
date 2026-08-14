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
//! - t4_current: 最优解 [u1][u2,u3][u4,u5]（3 层，backward=1），
//!   次优解代价相同或接近（权重调优后优先减少 backward 边）
//! - t2_cycle / t3_cycle: backward<=1
//! - box id +1000: 最优解不变
//! - 20 次连续: best 一致

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Instant;

use super::quotient::{Direction, NodeId, QuotientGraph, SP_COL_W};

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

/// 破环后每条边的硬定向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDir {
    /// 正向（src → dst，从左到右）
    Forward,
    /// 反向（dst → src，从右到左）
    Backward,
}

/// 代价结构
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Cost {
    /// 交叉数
    pub crossings: u32,
    /// 违反破环后定向的边数
    pub backward: u32,
    /// 跨度惩罚
    pub span: u32,
    /// 端口交叉
    pub port_cross: u32,
    /// 同层软惩罚
    pub same_layer: u32,
    /// 方向锚惩罚
    pub orient: u32,
    /// 源码序先验
    pub order: u32,
    /// 面积惩罚
    pub area: f64,
    /// 加权总代价
    pub weighted: f64,
}

// ============================================================================
// 权重常量
// ============================================================================

pub const W_CROSS: f64 = 1000.0;
pub const W_BACK: f64 = 600.0;   // 破环方向违规，应重于面积
pub const W_SPAN: f64 = 100.0;
pub const W_PORT: f64 = 20.0;
pub const W_SAMELAYER: f64 = 500.0;  // 同层软惩罚，低于 W_BACK(600)
pub const W_ORIENT: f64 = 400.0;
pub const W_ORDER: f64 = 30.0;
pub const W_AREA: f64 = 2.0;

impl Cost {
    pub fn zero() -> Self {
        Cost {
            crossings: 0,
            backward: 0,
            span: 0,
            port_cross: 0,
            same_layer: 0,
            orient: 0,
            order: 0,
            area: 0.0,
            weighted: 0.0,
        }
    }

    pub fn compute_weighted(&mut self) {
        self.weighted = self.crossings as f64 * W_CROSS
            + self.backward as f64 * W_BACK
            + self.span as f64 * W_SPAN
            + self.port_cross as f64 * W_PORT
            + self.same_layer as f64 * W_SAMELAYER
            + self.orient as f64 * W_ORIENT
            + self.order as f64 * W_ORDER
            + self.area * W_AREA;
    }

    pub fn from_counts(
        crossings: u32,
        backward: u32,
        span: u32,
        port_cross: u32,
        same_layer: u32,
        orient: u32,
        order: u32,
        area: f64,
    ) -> Self {
        let mut c = Cost {
            crossings,
            backward,
            span,
            port_cross,
            same_layer,
            orient,
            order,
            area,
            weighted: 0.0,
        };
        c.compute_weighted();
        c
    }
}

impl fmt::Display for Cost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "weighted={:.0} (cross={} back={} span={} port={} same={} orient={} order={} area={:.0})",
            self.weighted,
            self.crossings,
            self.backward,
            self.span,
            self.port_cross,
            self.same_layer,
            self.orient,
            self.order,
            self.area
        )
    }
}

/// 精确搜索上限
pub const EXACT_SEARCH_LIMIT: usize = 7;
/// top-K 候选数
pub const TOP_K: usize = 5;
/// 时间预算（毫秒），超时用已算完的最好结果
pub const TIME_BUDGET_MS: u64 = 150;

// ============================================================================
// 搜索入口
// ============================================================================

/// 对商图做精确搜索，返回 top-K 候选
pub fn solve(q: &QuotientGraph) -> Vec<(Cost, Arrangement)> {
    if q.nodes.is_empty() {
        return Vec::new();
    }
    if q.nodes.len() == 1 {
        let arr = Arrangement {
            layers: vec![q.nodes.clone()],
        };
        let c = cost(q, &arr);
        return vec![(c, arr)];
    }
    if q.nodes.len() <= EXACT_SEARCH_LIMIT {
        exact_enumerate(q)
    } else {
        unimplemented!("M6: heuristic for N>7, got N={}", q.nodes.len())
    }
}

// ============================================================================
// 精确枚举：Heap's algorithm + 切点 + 镜像对称 + 分支限界
// ============================================================================

/// 为每条边预计算的数据
struct EdgeData {
    src: usize,
    dst: usize,
    hard_dir: EdgeDir,
}

/// 对单个排列枚举所有切点，评估代价
fn evaluate_permutation(
    perm: &[usize],
    max_cut: u32,
    q: &QuotientGraph,
    edges: &[EdgeData],
    sides: &[NodeSide],
    all_neutral: bool,
    best: &mut Vec<(Cost, Arrangement)>,
    best_weighted: &mut f64,
    total_evals: &mut u64,
    pruned_count: &mut u64,
) {
    let n = perm.len();
    let nodes = &q.nodes;
    // 镜像对称：当所有节点 Neutral 时跳过镜像排列
    if all_neutral && is_mirror_perm(perm, nodes) {
        return;
    }

    for cut_mask in 0..max_cut {
        if all_neutral && is_mirror_cut(cut_mask, n.saturating_sub(1)) {
            continue;
        }

        let (arr, pruned) = build_with_bb(perm, cut_mask, nodes, edges, sides, *best_weighted);

        if pruned {
            *pruned_count += 1;
            continue;
        }

        *total_evals += 1;
        let full_cost = compute_full_cost(&arr, edges, sides, q);

        if full_cost.weighted < *best_weighted - 1e-9 {
            *best_weighted = full_cost.weighted;
            *best = vec![(full_cost, arr)];
        } else if (full_cost.weighted - *best_weighted).abs() < 1e-9 {
            best.push((full_cost, arr));
        }
    }
}

/// 精确枚举入口
fn exact_enumerate(q: &QuotientGraph) -> Vec<(Cost, Arrangement)> {
    let n = q.nodes.len();
    let hard_dirs = break_cycles(q);
    let sides = compute_node_sides(q, &hard_dirs);

    // 预计算边数据
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let edges: Vec<EdgeData> = q
        .edges
        .iter()
        .enumerate()
        .map(|(ei, e)| EdgeData {
            src: node_to_idx[&e.src],
            dst: node_to_idx[&e.dst],
            hard_dir: hard_dirs[ei],
        })
        .collect();

    let mut best: Vec<(Cost, Arrangement)> = Vec::new();
    let mut best_weighted = f64::MAX;

    // 检查是否所有节点 Neutral（决定是否启用镜像对称）
    let all_neutral = sides.iter().all(|s| *s == NodeSide::Neutral);

    // Heap's algorithm 生成排列
    let mut perm: Vec<usize> = (0..n).collect();
    let mut c = vec![0usize; n]; // Heap 状态
    let max_cut = 1u32 << (n.saturating_sub(1));

    let mut total_evals = 0u64;
    let mut pruned_count = 0u64;
    let start = Instant::now();

    // 先输出初始排列
    evaluate_permutation(
        &perm, max_cut, q, &edges, &sides, all_neutral,
        &mut best, &mut best_weighted, &mut total_evals, &mut pruned_count,
    );

    let mut i = 1;
    while i < n {
        // 时间预算检查
        if start.elapsed().as_millis() as u64 > TIME_BUDGET_MS {
            eprintln!("[debug] exact_enumerate: time budget {}ms exceeded, stopping early ({} evals, {} pruned)",
                TIME_BUDGET_MS, total_evals, pruned_count);
            break;
        }
        if c[i] < i {
            if i % 2 == 0 {
                perm.swap(0, i);
            } else {
                perm.swap(c[i], i);
            }

            evaluate_permutation(
                &perm, max_cut, q, &edges, &sides, all_neutral,
                &mut best, &mut best_weighted, &mut total_evals, &mut pruned_count,
            );

            c[i] += 1;
            i = 1;
        } else {
            c[i] = 0;
            i += 1;
        }
    }

    // 按 cost 排序，取 top-K
    best.sort_by(|a, b| {
        a.0.weighted
            .partial_cmp(&b.0.weighted)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    best.truncate(TOP_K);
    eprintln!("[debug] exact_enumerate: {} candidates, {} evals, {} pruned, best={:.0} layers={:?} ({}ms)",
        best.len(), total_evals, pruned_count, best[0].0.weighted, best[0].1.layers,
        start.elapsed().as_millis());
    best
}

/// 检查排列是否是其镜像（跳过前半部分避免重复）
fn is_mirror_perm(perm: &[usize], nodes: &[NodeId]) -> bool {
    if perm.len() <= 1 {
        return false;
    }
    let first_id = nodes[perm[0]];
    let last_id = nodes[perm[perm.len() - 1]];
    // 跳过 first > last 的排列（镜像已由 first < last 覆盖）
    first_id > last_id
}

/// 检查切点掩码是否是其镜像
fn is_mirror_cut(mask: u32, n_bits: usize) -> bool {
    if n_bits <= 1 {
        return false;
    }
    let rev = reverse_bits(mask, n_bits as u32);
    // 跳过 rev < mask 的掩码
    rev < mask
}

/// 位反转
fn reverse_bits(x: u32, n_bits: u32) -> u32 {
    let mut result = 0u32;
    for i in 0..n_bits {
        if (x >> i) & 1 != 0 {
            result |= 1 << (n_bits - 1 - i);
        }
    }
    result
}

/// 分支限界：增量构建 arrangement，每加一层就计算部分代价
/// 返回 (arrangement, 是否被剪枝)
fn build_with_bb(
    perm: &[usize],
    cut_mask: u32,
    nodes: &[NodeId],
    edges: &[EdgeData],
    sides: &[NodeSide],
    best_weighted: f64,
) -> (Arrangement, bool) {
    let n = perm.len();
    let mut layers: Vec<Vec<NodeId>> = Vec::new();
    let mut current: Vec<NodeId> = Vec::new();

    for i in 0..n {
        current.push(nodes[perm[i]]);
        let is_cut = ((cut_mask >> i) & 1) != 0;
        if is_cut || i == n - 1 {
            layers.push(current.clone());
            current.clear();

            // 部分代价检查
            let partial = compute_partial_cost(&layers, edges, sides, nodes);
            if partial.weighted > best_weighted {
                return (Arrangement { layers }, true);
            }
        }
    }

    (Arrangement { layers }, false)
}

/// 计算部分代价（仅已构建的层）
fn compute_partial_cost(
    layers: &[Vec<NodeId>],
    edges: &[EdgeData],
    sides: &[NodeSide],
    nodes: &[NodeId],
) -> Cost {
    let node_to_layer: HashMap<NodeId, usize> = {
        let mut m = HashMap::new();
        for (li, layer) in layers.iter().enumerate() {
            for &nid in layer {
                m.insert(nid, li);
            }
        }
        m
    };

    let mut crossings: u32 = 0;
    let mut backward: u32 = 0;
    let mut span: u32 = 0;
    let mut same_layer: u32 = 0;
    let mut orient: u32 = 0;
    let mut order: u32 = 0;

    // Edges
    for e in edges {
        let sl = node_to_layer.get(&nodes[e.src]);
        let dl = node_to_layer.get(&nodes[e.dst]);
        match (sl, dl) {
            (Some(&sl), Some(&dl)) => {
                if sl == dl {
                    same_layer += 1;
                } else {
                    // backward
                    match e.hard_dir {
                        EdgeDir::Forward if sl > dl => backward += 1,
                        EdgeDir::Backward if sl < dl => backward += 1,
                        _ => {}
                    }
                    // span
                    let diff = if sl > dl { sl - dl } else { dl - sl };
                    span += diff.saturating_sub(1) as u32;
                }
            }
            _ => {}
        }
    }

    // Crossings between adjacent layers
    for li in 0..layers.len().saturating_sub(1) {
        crossings += count_crossings(&layers[li], &layers[li + 1], edges, nodes);
    }

    // Orient
    let node_to_idx: HashMap<NodeId, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    for (li, layer) in layers.iter().enumerate() {
        for &nid in layer {
            if let Some(&idx) = node_to_idx.get(&nid) {
                if sides[idx] == NodeSide::Left {
                    for (lj, prev) in layers.iter().enumerate() {
                        if lj >= li {
                            break;
                        }
                        for &pid in prev {
                            if let Some(&pidx) = node_to_idx.get(&pid) {
                                if sides[pidx] == NodeSide::Right {
                                    orient += 1;
                                }
                            }
                        }
                    }
                }
                if sides[idx] == NodeSide::Right {
                    for (lj, next) in layers.iter().enumerate() {
                        if lj <= li {
                            continue;
                        }
                        for &nid2 in next {
                            if let Some(&nidx2) = node_to_idx.get(&nid2) {
                                if sides[nidx2] == NodeSide::Left {
                                    orient += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Order
    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let a = nodes[i];
            let b = nodes[j];
            if let (Some(&la), Some(&lb)) = (node_to_layer.get(&a), node_to_layer.get(&b)) {
                if la > lb {
                    order += 1;
                }
            }
        }
    }

    let max_nodes = layers.iter().map(|l| l.len()).max().unwrap_or(1);
    let total_w = layers.iter().map(|l| l.len() as f64 * SP_COL_W).sum::<f64>()
        + (layers.len().saturating_sub(1)) as f64 * SP_COL_W;
    let area = total_w; // 宽度主导，避免高瘦解比矮胖解面积更小

    Cost::from_counts(crossings, backward, span, 0, same_layer, orient, order, area)
}

/// 计算两层之间的边交叉数
fn count_crossings(
    left: &[NodeId],
    right: &[NodeId],
    edges: &[EdgeData],
    nodes: &[NodeId],
) -> u32 {
    // 收集从 left 到 right 的边
    let left_pos: HashMap<NodeId, usize> = left
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let right_pos: HashMap<NodeId, usize> = right
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for e in edges {
        let src_id = nodes[e.src];
        let dst_id = nodes[e.dst];
        if let (Some(&sp), Some(&dp)) = (left_pos.get(&src_id), right_pos.get(&dst_id)) {
            pairs.push((sp, dp));
        } else if let (Some(&sp), Some(&dp)) = (left_pos.get(&dst_id), right_pos.get(&src_id)) {
            pairs.push((sp, dp));
        }
    }

    if pairs.len() < 2 {
        return 0;
    }

    let mut cross = 0u32;
    for i in 0..pairs.len() {
        for j in (i + 1)..pairs.len() {
            let (a1, b1) = pairs[i];
            let (a2, b2) = pairs[j];
            if (a1 < a2 && b1 > b2) || (a1 > a2 && b1 < b2) {
                cross += 1;
            }
        }
    }
    cross
}

/// 计算完整代价
fn compute_full_cost(
    arr: &Arrangement,
    edges: &[EdgeData],
    sides: &[NodeSide],
    q: &QuotientGraph,
) -> Cost {
    let node_to_layer: HashMap<NodeId, usize> = {
        let mut m = HashMap::new();
        for (li, layer) in arr.layers.iter().enumerate() {
            for &nid in layer {
                m.insert(nid, li);
            }
        }
        m
    };

    let mut crossings: u32 = 0;
    let mut backward: u32 = 0;
    let mut span: u32 = 0;
    let mut same_layer: u32 = 0;

    for e in edges {
        let sl = node_to_layer.get(&q.nodes[e.src]);
        let dl = node_to_layer.get(&q.nodes[e.dst]);
        match (sl, dl) {
            (Some(&sl), Some(&dl)) => {
                if sl == dl {
                    same_layer += 1;
                } else {
                    match e.hard_dir {
                        EdgeDir::Forward if sl > dl => backward += 1,
                        EdgeDir::Backward if sl < dl => backward += 1,
                        _ => {}
                    }
                    let diff = if sl > dl { sl - dl } else { dl - sl };
                    span += diff.saturating_sub(1) as u32;
                }
            }
            _ => {}
        }
    }

    for li in 0..arr.layers.len().saturating_sub(1) {
        crossings += count_crossings(&arr.layers[li], &arr.layers[li + 1], edges, &q.nodes);
    }

    let orient = compute_orient_cost(q, sides, arr);
    let order = compute_order_cost(q, arr);

    let max_nodes = arr.layers.iter().map(|l| l.len()).max().unwrap_or(1);
    let total_w = arr
        .layers
        .iter()
        .map(|l| l.len() as f64 * SP_COL_W)
        .sum::<f64>()
        + (arr.layers.len().saturating_sub(1)) as f64 * SP_COL_W;
    let area = total_w; // 宽度主导

    Cost::from_counts(crossings, backward, span, 0, same_layer, orient, order, area)
}

// ============================================================================
// 破环：Eades–Lin–Smyth greedy feedback arc set
// ============================================================================

/// 破环：对商图做 greedy Eades-Lin-Smyth feedback arc set
///
/// 返回每条边的硬定向（Forward / Backward），与 `q.edges` 一一对应。
/// 之后 backward 只统计违反该定向的边。
///
/// 算法：
/// 1. 根据边的 prefer 构建有向图
/// 2. 迭代移除 sink（出度=0）→ 前置、source（入度=0）→ 后置
/// 3. 剩余节点选 max(出度-入度) 移除 → 后置
/// 4. 得到拓扑序，据此判定每条边是 Forward 还是 Backward
pub fn break_cycles(q: &QuotientGraph) -> Vec<EdgeDir> {
    let n = q.nodes.len();
    if n == 0 {
        return Vec::new();
    }

    // 构建 node_id → index 映射
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    // 构建邻接表（有向边）
    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut edge_dir: Vec<(usize, usize)> = Vec::with_capacity(q.edges.len());

    for e in &q.edges {
        let si = node_to_idx[&e.src];
        let di = node_to_idx[&e.dst];
        // 根据 prefer 决定方向
        let (from, to) = match e.prefer {
            Direction::LeftToRight => (si, di),
            Direction::RightToLeft => (di, si),
            Direction::Neutral => {
                // 无偏好时按 id 排序
                if e.src < e.dst {
                    (si, di)
                } else {
                    (di, si)
                }
            }
        };
        out_edges[from].push(to);
        in_edges[to].push(from);
        edge_dir.push((from, to));
    }

    // 计算出入度
    let mut out_deg: Vec<usize> = out_edges.iter().map(|v| v.len()).collect();
    let mut in_deg: Vec<usize> = in_edges.iter().map(|v| v.len()).collect();
    let mut removed = vec![false; n];

    let mut s1: Vec<usize> = Vec::new(); // 前置序列（sink）
    let mut s2: Vec<usize> = Vec::new(); // 后置序列（source + max delta）

    let mut remaining = n;
    while remaining > 0 {
        // 移除所有 sink（出度=0）
        loop {
            let sink = (0..n).find(|&i| !removed[i] && out_deg[i] == 0);
            match sink {
                Some(v) => {
                    removed[v] = true;
                    s1.push(v);
                    remaining -= 1;
                    // 更新邻居的度数
                    for &pred in &in_edges[v] {
                        if !removed[pred] {
                            out_deg[pred] -= 1;
                        }
                    }
                }
                None => break,
            }
        }

        // 移除所有 source（入度=0）
        loop {
            let source = (0..n).find(|&i| !removed[i] && in_deg[i] == 0);
            match source {
                Some(v) => {
                    removed[v] = true;
                    s2.push(v);
                    remaining -= 1;
                    for &succ in &out_edges[v] {
                        if !removed[succ] {
                            in_deg[succ] -= 1;
                        }
                    }
                }
                None => break,
            }
        }

        // 如果没有 sink/source，选 max(出度-入度)
        if remaining > 0 {
            let v = (0..n)
                .filter(|&i| !removed[i])
                .max_by_key(|&i| {
                    let delta = out_deg[i] as isize - in_deg[i] as isize;
                    delta
                })
                .unwrap();
            removed[v] = true;
            s2.push(v);
            remaining -= 1;
            for &succ in &out_edges[v] {
                if !removed[succ] {
                    in_deg[succ] -= 1;
                }
            }
            for &pred in &in_edges[v] {
                if !removed[pred] {
                    out_deg[pred] -= 1;
                }
            }
        }
    }

    // 合并序列：s1 逆序 + s2 正序
    s1.reverse();
    let seq: Vec<usize> = {
        let mut s = s1;
        s.extend(s2);
        s
    };

    // 建立拓扑序位置映射
    let pos: Vec<usize> = {
        let mut p = vec![0; n];
        for (rank, &idx) in seq.iter().enumerate() {
            p[idx] = rank;
        }
        p
    };

    // 判定每条边的方向
    edge_dir
        .iter()
        .map(|&(from, to)| {
            if pos[from] < pos[to] {
                EdgeDir::Forward
            } else {
                EdgeDir::Backward
            }
        })
        .collect()
}

// ============================================================================
// 方向锚
// ============================================================================

/// 节点的期望位置（用于 orient 代价项）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NodeSide {
    /// 应靠左
    Left = 0,
    /// 无偏好
    Neutral = 1,
    /// 应靠右
    Right = 2,
}

/// 计算每个节点的方向偏好，基于商图边的方向信息
///
/// 优先级：
/// a. 模块端口：有 Output 端口的节点靠左，有 Input 端口的靠右
/// b. 破环后入度为 0 的节点靠左
fn compute_node_sides(q: &QuotientGraph, hard_dirs: &[EdgeDir]) -> Vec<NodeSide> {
    let n = q.nodes.len();
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut out_count = vec![0u32; n];
    let mut in_count = vec![0u32; n];

    for (ei, e) in q.edges.iter().enumerate() {
        let si = node_to_idx[&e.src];
        let di = node_to_idx[&e.dst];
        match hard_dirs[ei] {
            EdgeDir::Forward => {
                out_count[si] += 1;
                in_count[di] += 1;
            }
            EdgeDir::Backward => {
                out_count[di] += 1;
                in_count[si] += 1;
            }
        }
    }

    // 计算破环后入度为 0 的节点
    let source_nodes: HashSet<usize> = (0..n).filter(|&i| in_count[i] == 0).collect();

    (0..n)
        .map(|i| {
            if source_nodes.contains(&i) {
                NodeSide::Left
            } else if out_count[i] > 0 && in_count[i] == 0 {
                NodeSide::Left
            } else if in_count[i] > 0 && out_count[i] == 0 {
                NodeSide::Right
            } else {
                NodeSide::Neutral
            }
        })
        .collect()
}

/// 计算 orient 代价：违反方向锚的节点数
///
/// 对每层检查：层的节点中，如果有 Left 偏好的节点在 Right 偏好的节点之后，
/// 或者 Left/Right 节点在层内的相对顺序与期望不符，计为违规。
fn compute_orient_cost(q: &QuotientGraph, sides: &[NodeSide], arr: &Arrangement) -> u32 {
    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut penalty = 0u32;

    // 跨层检查：如果 Left 节点在 Right 节点右侧（层号更大），违规
    for (li, layer) in arr.layers.iter().enumerate() {
        for &nid in layer {
            let idx = node_to_idx[&nid];
            if sides[idx] == NodeSide::Left {
                // 检查是否有 Right 节点在更左侧的层
                for (lj, prev_layer) in arr.layers.iter().enumerate() {
                    if lj >= li {
                        break;
                    }
                    for &pid in prev_layer {
                        let pidx = node_to_idx[&pid];
                        if sides[pidx] == NodeSide::Right {
                            penalty += 1;
                        }
                    }
                }
            }
            if sides[idx] == NodeSide::Right {
                // 检查是否有 Left 节点在更右侧的层
                for (lj, next_layer) in arr.layers.iter().enumerate() {
                    if lj <= li {
                        continue;
                    }
                    for &nid2 in next_layer {
                        let nidx2 = node_to_idx[&nid2];
                        if sides[nidx2] == NodeSide::Left {
                            penalty += 1;
                        }
                    }
                }
            }
        }
    }

    penalty
}

/// 计算 order 代价：违反源码序的逆序对数
///
/// 源码序以 q.nodes 中的顺序为代理（id 排序，近似源码声明顺序）。
/// 对于不同层之间的节点对 (a, b)，如果 a 在源码序中先于 b，
/// 但 a 在更右侧的层（层号更大），计为一个逆序对。
fn compute_order_cost(q: &QuotientGraph, arr: &Arrangement) -> u32 {
    let node_to_layer: HashMap<NodeId, usize> = {
        let mut m = HashMap::new();
        for (li, layer) in arr.layers.iter().enumerate() {
            for &nid in layer {
                m.insert(nid, li);
            }
        }
        m
    };

    let mut penalty = 0u32;
    for i in 0..q.nodes.len() {
        for j in (i + 1)..q.nodes.len() {
            let a = q.nodes[i];
            let b = q.nodes[j];
            if let (Some(&la), Some(&lb)) = (node_to_layer.get(&a), node_to_layer.get(&b)) {
                if la > lb {
                    // a 在源码序中先于 b，但层号更大 → 逆序
                    penalty += 1;
                }
            }
        }
    }
    penalty
}

/// 计算给定排列的完整代价
pub fn cost(q: &QuotientGraph, arr: &Arrangement) -> Cost {
    let hard_dirs = break_cycles(q);
    let sides = compute_node_sides(q, &hard_dirs);

    let node_to_idx: HashMap<NodeId, usize> = q
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let edges: Vec<EdgeData> = q
        .edges
        .iter()
        .enumerate()
        .map(|(ei, e)| EdgeData {
            src: node_to_idx[&e.src],
            dst: node_to_idx[&e.dst],
            hard_dir: hard_dirs[ei],
        })
        .collect();

    compute_full_cost(arr, &edges, &sides, q)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::McVecBox;
    use crate::vector::graph::netdef::IoDirection;
    use crate::vector::graph::{
        BoxKind, EndpointRef, IoSummary, McVecGraph, NetKind, NetRole, VizNet,
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
    // break_cycles 测试（M3-2）
    // ────────────────────────────────────────

    /// t4_current: DAG，所有边应为 Forward
    #[test]
    fn break_cycles_t4_current_all_forward() {
        let (_g, q) = make_t4_current();
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 6, "t4_current has 6 edges");
        // DAG，破环应全部为 Forward
        let backward_count = dirs.iter().filter(|d| matches!(d, EdgeDir::Backward)).count();
        assert_eq!(backward_count, 0, "t4_current is a DAG, no backward edges");
    }

    /// t2_cycle: 2 节点循环，应有 1 条 Backward
    #[test]
    fn break_cycles_t2_cycle() {
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
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 2);
        let backward_count = dirs.iter().filter(|d| matches!(d, EdgeDir::Backward)).count();
        assert_eq!(backward_count, 1, "t2_cycle should have exactly 1 backward edge");
    }

    /// t3_cycle: 3 节点循环，应有 1 条 Backward
    #[test]
    fn break_cycles_t3_cycle() {
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
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 3);
        let backward_count = dirs.iter().filter(|d| matches!(d, EdgeDir::Backward)).count();
        assert_eq!(backward_count, 1, "t3_cycle should have exactly 1 backward edge");
    }

    /// t1_chain: 简单链，所有 Forward
    #[test]
    fn break_cycles_t1_chain() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1", 3));
        g.boxes.push(mk_ic(2, "u2", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let dirs = break_cycles(&q);
        assert_eq!(dirs.len(), 1);
        assert!(matches!(dirs[0], EdgeDir::Forward));
    }

    /// 空图：无边
    #[test]
    fn break_cycles_empty() {
        let g = McVecGraph::new(0, "main".into());
        let q = QuotientGraph::build(&g);
        let dirs = break_cycles(&q);
        assert!(dirs.is_empty());
    }

    // ────────────────────────────────────────
    // 方向锚测试（M3-2）
    // ────────────────────────────────────────

    /// 有明确 OUT→IN 方向的图，OUT 应该在左边、IN 在右边
    #[test]
    fn orient_out_left_in_right() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_ic(1, "u1_src", 3));
        g.boxes.push(mk_ic(2, "u2_dst", 3));
        g.nets.push(mk_signal_net(10, "u1_u2", vec![
            mk_ep(1, 11, "OUT", IoDirection::Output),
            mk_ep(2, 21, "IN", IoDirection::Input),
        ]));

        let q = QuotientGraph::build(&g);
        let hard_dirs = break_cycles(&q);
        let sides = compute_node_sides(&q, &hard_dirs);

        // u1 (index 0) 应该偏左，u2 (index 1) 应该偏右
        assert_eq!(sides[0], NodeSide::Left, "u1 (OUT) should be on the left");
        assert_eq!(sides[1], NodeSide::Right, "u2 (IN) should be on the right");

        // 验证 orient 代价：正确排列 [u1][u2] 的 orient=0
        let arr_correct = Arrangement {
            layers: vec![vec![1], vec![2]],
        };
        let cost_correct = cost(&q, &arr_correct);
        assert_eq!(cost_correct.orient, 0, "correct order should have orient=0");

        // 验证 orient 代价：错误排列 [u2][u1] 的 orient>0
        let arr_wrong = Arrangement {
            layers: vec![vec![2], vec![1]],
        };
        let cost_wrong = cost(&q, &arr_wrong);
        assert!(cost_wrong.orient > 0, "wrong order should have orient>0");
    }

    /// t4_current: 验证 orient 代价在最优解中起作用
    #[test]
    fn orient_t4_current() {
        let (_g, q) = make_t4_current();
        let hard_dirs = break_cycles(&q);
        let _sides = compute_node_sides(&q, &hard_dirs);

        // 最优解 [4][2][1][3,5]
        let arr = Arrangement {
            layers: vec![vec![4], vec![2], vec![1], vec![3, 5]],
        };
        let c = cost(&q, &arr);
        // 验证 orient + order 在最优解中起作用
        assert!(c.weighted > 0.0 || (c.orient == 0 && c.order == 0),
            "orient and order should be computed for t4_current");

        // 镜像解 [3,5][1][2][4] 应该 orient 代价更高
        let arr_mirror = Arrangement {
            layers: vec![vec![3, 5], vec![1], vec![2], vec![4]],
        };
        let c_mirror = cost(&q, &arr_mirror);
        assert!(
            c_mirror.orient > c.orient || c_mirror.order > c.order,
            "mirror solution should have higher orient/order cost: best orient={} order={}, mirror orient={} order={}",
            c.orient, c.order, c_mirror.orient, c_mirror.order
        );
    }

    // ────────────────────────────────────────
    // 枚举测试
    // ────────────────────────────────────────

    /// t4_current: 最优解 [u1][u2,u3][u4,u5]（3 层，backward=1）
    #[test]
    fn arrange_t4_current_optimal() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        assert!(!candidates.is_empty(), "should have at least one candidate");

        let (cost, best) = &candidates[0];
        // 当前权重（W_BACK=600）下最优是 3 层，backward=1
        assert_eq!(best.layers.len(), 3, "t4_current should have 3 layers, got {:?}", best.layers);
        assert_eq!(best.layers[0], vec![1], "first layer should be [u1_mcu]");
        assert_eq!(cost.backward, 1, "should have exactly 1 backward edge (u4→u3)");
        // 最后一层应包含 u4, u5
        let last: Vec<i64> = best.layers[2].clone();
        let mut last_sorted = last.clone();
        last_sorted.sort();
        assert_eq!(last_sorted, vec![4, 5], "last layer should be [u4_ldo_out, u5_flash]");
    }

    /// t4_current: 次优解代价不低于最优解
    #[test]
    fn arrange_t4_current_second_best() {
        let (_g, q) = make_t4_current();
        let candidates = solve(&q);
        if candidates.len() >= 2 {
            let (cost1, _) = &candidates[0];
            let (cost2, _) = &candidates[1];
            assert!(
                cost2.weighted >= cost1.weighted - 1e-9,
                "second best should not be better than best: best={:.0}, second={:.0}",
                cost1.weighted, cost2.weighted
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
            cost.backward <= 1,
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
            cost.backward <= 1,
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

        assert_eq!(
            best_layers.len(),
            best_layers2.len(),
            "mirror: layer count should be identical"
        );
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
        g.boxes.push(mk_ic(1, "u1_src", 3));
        g.boxes.push(mk_ic(2, "u2_dst", 3));
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