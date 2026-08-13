// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Zone 纸面粗排（M2-3）
//!
//! 将 zone 树转为纸面上的 ZonePlan，包括：
//! 1. 每个 zone 的 rect 大小计算
//! 2. zone 间贪心左右排序（信号流：电源 → 主控 → 外设）
//! 3. 画布大小计算

use super::plan::{Plan, Rect, ZonePlan};
use super::zone::ZoneTree;
use crate::vector::graph::McVecGraph;

/// Zone 纸面排布参数
const ZONE_GAP: f64 = 40.0; // zone 间距
const ZONE_PAD: f64 = 20.0; // zone 内边距
const ZONE_MIN_W: f64 = 200.0; // zone 最小宽度
const ZONE_MIN_H: f64 = 150.0; // zone 最小高度
const BOX_W: f64 = 80.0; // 每个 box 估算宽度
const BOX_H: f64 = 60.0; // 每个 box 估算高度
const BOX_PER_ROW: usize = 4; // 每行 box 数

/// 计算 zone 的纸面位置
pub fn place_zones(graph: &McVecGraph, tree: &ZoneTree) -> Vec<ZonePlan> {
    if tree.zones.is_empty() {
        return Vec::new();
    }

    // ── 1. 计算每个 zone 的 rect ──
    let mut zone_rects: Vec<Rect> = Vec::new();
    for zone in &tree.zones {
        zone_rects.push(compute_zone_rect(zone, graph));
    }

    // ── 2. 按 zone 间连接数做贪心排序 ──
    // 简单策略：根据 zone 内 box 数排序（大 zone 优先），
    // 后续 M3 可以改为按信号流排序
    let order = order_zones_by_size(tree);

    // ── 3. 水平排列（左到右） ──
    let mut x = ZONE_PAD;
    let mut max_h = 0.0f64;
    let mut plans: Vec<ZonePlan> = Vec::new();

    for &zone_id in &order {
        let rect = zone_rects[zone_id];
        let zone = &tree.zones[zone_id];

        plans.push(ZonePlan {
            zone: zone_id,
            box_ids: zone.boxes.clone(),
            rect: Rect {
                x,
                y: ZONE_PAD,
                w: rect.w,
                h: rect.h,
            },
            title_anchor: super::plan::Point {
                x: x + ZONE_PAD,
                y: ZONE_PAD + ZONE_PAD,
            },
            title: zone.title.clone(),
        });

        x += rect.w + ZONE_GAP;
        max_h = max_h.max(rect.h);
    }

    // ── 4. 计算画布大小 ──
    let canvas_w = x + ZONE_PAD;
    let canvas_h = max_h + ZONE_PAD * 2.0;

    // 把画布大小写回第一个 zone plan（Plan 会从 plans 中推算出 canvas）
    // 实际 canvas 由 Plan 构造时计算
    let _ = (canvas_w, canvas_h);

    plans
}

/// 计算 zone 的 rect 大小
fn compute_zone_rect(zone: &super::zone::Zone, graph: &McVecGraph) -> Rect {
    let box_count = zone.boxes.len();
    if box_count == 0 {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: ZONE_MIN_W,
            h: ZONE_MIN_H,
        };
    }

    let cols = BOX_PER_ROW.min(box_count);
    let rows = (box_count + cols - 1) / cols;

    let w = (cols as f64 * BOX_W + ZONE_PAD * 2.0).max(ZONE_MIN_W);
    let h = (rows as f64 * BOX_H + ZONE_PAD * 2.0).max(ZONE_MIN_H);

    Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    }
}

/// 按 zone 内 box 数排序（大 zone 优先），root 放最前面
fn order_zones_by_size(tree: &ZoneTree) -> Vec<usize> {
    let mut ids: Vec<usize> = (0..tree.zones.len()).collect();
    ids.sort_by_key(|&id| {
        let zone = &tree.zones[id];
        // 根 zone 优先
        let is_root = tree.roots.contains(&id);
        let box_count = zone.boxes.len();
        // 根 zone 排最前，其余按 box 数降序
        (!is_root, -(box_count as i64))
    });
    ids
}

/// 从 zone plans 计算画布大小
pub fn compute_canvas(plans: &[ZonePlan]) -> (f64, f64) {
    let mut max_x = 0.0f64;
    let mut max_y = 0.0f64;

    for plan in plans {
        let right = plan.rect.x + plan.rect.w;
        let bottom = plan.rect.y + plan.rect.h;
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }

    (max_x + ZONE_PAD, max_y + ZONE_PAD)
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

    #[test]
    fn test_place_zones_trivial() {
        let mut graph = McVecGraph::new(0, String::new());
        graph.boxes.push(make_box(1, "main.R1", BoxKind::TwoPin));
        graph.boxes.push(make_box(2, "main.R2", BoxKind::TwoPin));
        graph.boxes.push(make_box(3, "main.C1", BoxKind::TwoPin));

        let tree = super::super::zone::ZoneTree::build(&graph);
        let plans = place_zones(&graph, &tree);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].rect.w >= ZONE_MIN_W);
        assert!(plans[0].rect.h >= ZONE_MIN_H);
    }

    #[test]
    fn test_place_zones_multiple() {
        let mut graph = McVecGraph::new(0, String::new());
        graph.boxes.push(make_box(1, "main.modldo.ldo", BoxKind::MultiPin));
        graph.boxes.push(make_box(2, "main.moddcdc.dcdc", BoxKind::MultiPin));
        graph.boxes.push(make_box(3, "main.mic.MIC", BoxKind::MultiPin));
        graph.boxes.push(make_box(4, "main.speaker.SPK", BoxKind::MultiPin));

        let tree = super::super::zone::ZoneTree::build(&graph);
        let plans = place_zones(&graph, &tree);
        // 4 个单 box zone → 每个都应非零
        assert!(plans.len() >= 1);
        for plan in &plans {
            assert!(plan.rect.w > 0.0);
            assert!(plan.rect.h > 0.0);
        }
    }

    #[test]
    fn test_compute_canvas() {
        let plans = vec![
            ZonePlan {
                zone: 0,
                box_ids: vec![],
                rect: Rect { x: 0.0, y: 0.0, w: 200.0, h: 150.0 },
                title_anchor: super::super::plan::Point { x: 0.0, y: 0.0 },
                title: String::new(),
            },
            ZonePlan {
                zone: 1,
                box_ids: vec![],
                rect: Rect { x: 240.0, y: 0.0, w: 200.0, h: 150.0 },
                title_anchor: super::super::plan::Point { x: 0.0, y: 0.0 },
                title: String::new(),
            },
        ];
        let (w, h) = compute_canvas(&plans);
        assert!(w >= 440.0 + ZONE_PAD);
        assert!(h >= 150.0 + ZONE_PAD);
    }
}