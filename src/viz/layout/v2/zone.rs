// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Zone 树 — 功能分区
//!
//! 从 `McVecBox.inst_path` 构建功能分区树。
//! .mc 源码的 module 结构就是分区的天然答案，不需要聚类算法。
//!
//! 算法：
//! 1. 按 inst_path 建前缀树
//! 2. 压平单链
//! 3. 合并小 zone（box 数 < MIN_ZONE_SIZE）
//! 4. 深度上限 MAX_ZONE_DEPTH = 2

use std::collections::BTreeMap;

use crate::vector::graph::BoxKind;
use crate::vector::graph::McVecGraph;

/// 最小 zone 大小（box 数），小于此值的 zone 并入父 zone
pub const MIN_ZONE_SIZE: usize = 3;

/// 最大 zone 深度（纸面上超过两级视觉分区反而更乱）
pub const MAX_ZONE_DEPTH: usize = 2;

// ============================================================================
// Zone 数据结构
// ============================================================================

/// 单个功能分区
#[derive(Debug, Clone)]
pub struct Zone {
    pub id: usize,
    /// 路径，如 "main.modldo"
    pub path: String,
    /// 显示标题，如 "POWER_LDO"
    pub title: String,
    /// 该 zone 内的 box ids
    pub boxes: Vec<i64>,
    /// 子 zone 索引
    pub children: Vec<usize>,
    /// 父 zone 索引
    pub parent: Option<usize>,
}

/// 分区树
#[derive(Debug, Clone, Default)]
pub struct ZoneTree {
    pub zones: Vec<Zone>,
    /// 根 zone 索引
    pub roots: Vec<usize>,
}

// ============================================================================
// 内部：前缀树节点
// ============================================================================

/// 前缀树节点（构建时使用）
#[derive(Debug, Clone)]
struct TrieNode {
    /// 完整路径（如 "main.modldo"）
    path: String,
    /// 深度（从 0 开始）
    depth: usize,
    /// 该节点的 box ids（只有叶子才有 box）
    boxes: Vec<i64>,
    /// 子节点
    children: BTreeMap<String, TrieNode>,
}

impl TrieNode {
    fn new(path: String, depth: usize) -> Self {
        TrieNode {
            path,
            depth,
            boxes: Vec::new(),
            children: BTreeMap::new(),
        }
    }
}

// ============================================================================
// 构建
// ============================================================================

impl ZoneTree {
    /// 构建分区树
    ///
    /// 使用 `inst_path` 而不是 `scope_chain`（v2 修订），
    /// 直接用 path 做前缀树，压平/合并规则不变，少一层同步风险。
    pub fn build(graph: &McVecGraph) -> Self {
        // ── 收集需要分区的 box ──
        // PowerLabel / Dot 不进任何 zone（它们就近渲染）
        let mut zone_boxes: Vec<(i64, String)> = Vec::new();
        for b in &graph.boxes {
            if b.kind == BoxKind::PowerLabel || b.kind == BoxKind::Dot {
                continue;
            }
            if b.inst_path.is_empty() {
                continue;
            }
            zone_boxes.push((b.id, b.inst_path.clone()));
        }

        if zone_boxes.is_empty() {
            return ZoneTree::default();
        }

        // ── 1. 按 inst_path 建前缀树 ──
        let mut root = TrieNode::new("".to_string(), 0);

        for (box_id, path) in &zone_boxes {
            let segments: Vec<&str> = path.split('.').collect();
            let mut node = &mut root;
            for (i, seg) in segments.iter().enumerate() {
                let full_path = segments[..=i].join(".");
                node = node
                    .children
                    .entry(seg.to_string())
                    .or_insert_with(|| TrieNode::new(full_path, i + 1));
            }
            // 叶子节点：添加 box
            node.boxes.push(*box_id);
        }

        // ── 2. 压平单链 ──
        // 只有一个孩子且自己没有直属 box 的 zone，与孩子合并
        let mut flattened: Vec<Zone> = Vec::new();
        flatten_trie(&mut root, &mut flattened, None);

        // ── 3. 合并小 zone ──
        // box 数 < MIN_ZONE_SIZE 的 zone 并入父 zone
        merge_small_zones(&mut flattened);

        // ── 4. 深度限制 ──
        // 超过 MAX_ZONE_DEPTH 的 zone 压到父 zone
        enforce_depth_limit(&mut flattened);

        // ── 5. 计算 roots ──
        let roots: Vec<usize> = flattened
            .iter()
            .filter(|z| z.parent.is_none())
            .map(|z| z.id)
            .collect();

        // ── 日志 ──
        let leaf_count = flattened
            .iter()
            .filter(|z| z.children.is_empty())
            .count();
        let max_depth = flattened.iter().map(|z| z.path.matches('.').count()).max().unwrap_or(0);
        mcc_dbg!(
            "viz",
            "[zone] tree: {} root zone(s), {} leaf zone(s), depth={}",
            roots.len(),
            leaf_count,
            max_depth
        );
        for &root_id in &roots {
            log_zone_tree(&flattened, root_id, 0);
        }
        let tiny_merged = flattened
            .iter()
            .filter(|z| z.boxes.len() < MIN_ZONE_SIZE && z.children.is_empty())
            .count();
        if tiny_merged > 0 {
            mcc_dbg!("viz", "[zone] merged {} tiny zone(s) into parent", tiny_merged);
        }

        ZoneTree { zones: flattened, roots }
    }
}

// ============================================================================
// 内部函数
// ============================================================================

/// 压平：将前缀树转为 Zone 列表，单链合并
fn flatten_trie(root: &mut TrieNode, zones: &mut Vec<Zone>, parent: Option<usize>) {
    // 递归处理每个子节点
    let child_keys: Vec<String> = root.children.keys().cloned().collect();
    let mut child_zone_ids: Vec<usize> = Vec::new();
    let mut root_boxes: Vec<i64> = root.boxes.clone();

    for key in child_keys {
        let mut child = root.children.remove(&key).unwrap();
        let (mut boxes, created_id) = flatten_subtree(&mut child, zones, parent);

        if let Some(id) = created_id {
            child_zone_ids.push(id);
        } else {
            // 子节点没有创建 zone，box 归入 root
            root_boxes.append(&mut boxes);
        }
    }

    // 如果有子 zone 或自己有 box → 创建 zone
    if !child_zone_ids.is_empty() || !root_boxes.is_empty() {
        let id = zones.len();
        let title = zone_title(if root.path.is_empty() { "main" } else { &root.path });
        let zone = Zone {
            id,
            path: if root.path.is_empty() {
                "main".to_string()
            } else {
                root.path.clone()
            },
            title,
            boxes: root_boxes,
            children: child_zone_ids.clone(),
            parent,
        };
        zones.push(zone);
        for &cid in &child_zone_ids {
            zones[cid].parent = Some(id);
        }
    }
}

/// 递归处理子树，返回 (boxes_to_attach, Option<zone_id>)
///
/// - 如果创建了 zone，返回 (remaining_boxes, Some(zone_id))
/// - 如果被压平，返回 (all_boxes, None)
fn flatten_subtree(
    node: &mut TrieNode,
    zones: &mut Vec<Zone>,
    parent: Option<usize>,
) -> (Vec<i64>, Option<usize>) {
    // 先递归处理子节点
    let child_keys: Vec<String> = node.children.keys().cloned().collect();
    let mut child_zone_ids: Vec<usize> = Vec::new();
    let mut node_boxes: Vec<i64> = node.boxes.clone();

    for key in child_keys {
        let mut child = node.children.remove(&key).unwrap();
        let (mut boxes, created_id) = flatten_subtree(&mut child, zones, parent);
        if let Some(id) = created_id {
            child_zone_ids.push(id);
        } else {
            node_boxes.append(&mut boxes);
        }
    }

    // 压平：只有一个孩子且自己没有直属 box → 合并到孩子
    if child_zone_ids.len() == 1 && node_boxes.is_empty() {
        let child = &mut zones[child_zone_ids[0]];
        child.path = node.path.clone();
        child.parent = parent;
        return (Vec::new(), Some(child_zone_ids[0]));
    }

    // 没有孩子：不创建 zone，box 返回给父节点
    if child_zone_ids.is_empty() {
        return (node_boxes, None);
    }

    // 创建 zone
    let id = zones.len();
    let title = zone_title(&node.path);
    let zone = Zone {
        id,
        path: node.path.clone(),
        title,
        boxes: node_boxes,
        children: child_zone_ids.clone(),
        parent,
    };
    zones.push(zone);
    for &cid in &child_zone_ids {
        zones[cid].parent = Some(id);
    }

    (Vec::new(), Some(id))
}

/// 合并小 zone：box 数 < MIN_ZONE_SIZE 的并入父 zone
fn merge_small_zones(zones: &mut Vec<Zone>) {
    let mut i = 0;
    while i < zones.len() {
        if zones[i].boxes.len() >= MIN_ZONE_SIZE || zones[i].parent.is_none() {
            i += 1;
            continue;
        }

        let pid = zones[i].parent.unwrap();
        // 把 box 合并到父 zone
        let boxes = zones[i].boxes.clone();
        let children = zones[i].children.clone();
        zones[pid].boxes.extend(boxes);

        // 把子 zone 的 children 重新挂到父 zone
        for &cid in &children {
            zones[cid].parent = Some(pid);
        }

        // 从父 zone 的 children 中移除当前 zone
        if let Some(pos) = zones[pid].children.iter().position(|&c| c == i) {
            zones[pid].children.remove(pos);
        }
        zones[pid].children.extend(children);

        i += 1;
    }
}

/// 深度限制：超过 MAX_ZONE_DEPTH 的 zone 压到父 zone
fn enforce_depth_limit(zones: &mut Vec<Zone>) {
    // 递归计算每个 zone 的实际深度
    let depths = compute_zone_depths(zones);

    let mut i = 0;
    while i < zones.len() {
        let depth = depths.get(&i).copied().unwrap_or(0);
        if depth <= MAX_ZONE_DEPTH || zones[i].parent.is_none() {
            i += 1;
            continue;
        }

        let pid = zones[i].parent.unwrap();
        // 把 box 和 children 合并到父 zone
        let boxes = zones[i].boxes.clone();
        let children = zones[i].children.clone();
        zones[pid].boxes.extend(boxes);

        for &cid in &children {
            zones[cid].parent = Some(pid);
        }
        if let Some(pos) = zones[pid].children.iter().position(|&c| c == i) {
            zones[pid].children.remove(pos);
        }
        zones[pid].children.extend(children);

        i += 1;
    }
}

/// 计算每个 zone 的深度（从 root 开始）
fn compute_zone_depths(zones: &[Zone]) -> BTreeMap<usize, usize> {
    let mut depths = BTreeMap::new();
    for zone in zones {
        let mut depth = 0usize;
        let mut cur = zone.parent;
        while let Some(pid) = cur {
            depth += 1;
            cur = zones[pid].parent;
        }
        depths.insert(zone.id, depth);
    }
    depths
}

/// 生成 zone 标题：优先用最后一节的类名，退而用路径
fn zone_title(path: &str) -> String {
    // 取最后一段
    let leaf = path.rsplit('.').next().unwrap_or(path);
    // 如果最后一段是 main 且路径只有 main，返回 "main"
    if path == "main" {
        return "main".to_string();
    }
    leaf.to_string()
}

/// 递归打印 zone 树
fn log_zone_tree(zones: &[Zone], zone_id: usize, indent: usize) {
    let zone = &zones[zone_id];
    let prefix = "  ".repeat(indent);
    mcc_dbg!(
        "viz",
        "{}[zone] #{} '{}' ({} boxes) [{}]",
        prefix,
        zone.id,
        zone.title,
        zone.boxes.len(),
        zone.path
    );
    for &child_id in &zone.children {
        log_zone_tree(zones, child_id, indent + 1);
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{IoSummary, McVecBox};
    use crate::vector::graph::{BoxKind, Symbol};

    fn make_box(id: i64, inst_path: &str, kind: BoxKind) -> McVecBox {
        McVecBox::new_v2(
            id,
            String::new(),
            String::new(),
            kind,
            Symbol::Unknown,
            None,
            None,
            0,
            IoSummary::default(),
            inst_path.to_string(),
            Vec::new(),
        )
    }

    fn empty_graph() -> McVecGraph {
        McVecGraph::new(0, String::new())
    }

    #[test]
    fn test_zone_tree_trivial() {
        // 所有 box 在 main 下 → 单 zone
        let mut graph = empty_graph();
        graph.boxes.push(make_box(1, "main.R1", BoxKind::TwoPin));
        graph.boxes.push(make_box(2, "main.R2", BoxKind::TwoPin));
        graph.boxes.push(make_box(3, "main.C1", BoxKind::TwoPin));

        let tree = ZoneTree::build(&graph);
        assert_eq!(tree.roots.len(), 1);
        assert_eq!(tree.zones[tree.roots[0]].boxes.len(), 3);
        assert_eq!(tree.zones[tree.roots[0]].title, "main");
    }

    #[test]
    fn test_zone_tree_modules() {
        // 多个子模块 → 多个 zone
        let mut graph = empty_graph();
        graph.boxes.push(make_box(1, "main.modldo.ldo", BoxKind::MultiPin));
        graph.boxes.push(make_box(2, "main.moddcdc.dcdc", BoxKind::MultiPin));
        graph.boxes.push(make_box(3, "main.mic.MIC", BoxKind::MultiPin));
        graph.boxes.push(make_box(4, "main.speaker.SPK", BoxKind::MultiPin));

        let tree = ZoneTree::build(&graph);
        // 每个模块应该是一个 zone
        assert!(tree.roots.len() >= 1);
        let total_boxes: usize = tree.zones.iter().map(|z| z.boxes.len()).sum();
        assert_eq!(total_boxes, 4);
    }

    #[test]
    fn test_zone_tree_excludes_power_labels() {
        let mut graph = empty_graph();
        graph.boxes.push(make_box(1, "main.R1", BoxKind::TwoPin));
        graph.boxes.push(make_box(2, "main.GND", BoxKind::PowerLabel));
        graph.boxes.push(make_box(3, "main.VDD", BoxKind::PowerLabel));

        let tree = ZoneTree::build(&graph);
        let total_boxes: usize = tree.zones.iter().map(|z| z.boxes.len()).sum();
        // 只有 R1 进 zone，PowerLabel 不进
        assert_eq!(total_boxes, 1);
    }

    #[test]
    fn test_zone_tree_nested() {
        // 嵌套模块 → 压平后应合理
        let mut graph = empty_graph();
        graph.boxes.push(make_box(1, "main.pwr.ldo.ldo", BoxKind::MultiPin));
        graph.boxes.push(make_box(2, "main.pwr.dcdc.dcdc", BoxKind::MultiPin));
        graph.boxes.push(make_box(3, "main.audio.mic.MIC", BoxKind::MultiPin));

        let tree = ZoneTree::build(&graph);
        // 应有合理的分区结构
        assert!(!tree.roots.is_empty());
    }
}