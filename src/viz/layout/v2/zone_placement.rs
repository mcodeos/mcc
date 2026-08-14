// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Zone 纸面粗排（M2-3）
//!
//! 将 zone 树转为纸面上的 ZonePlan，包括：
//! 1. 每个 zone 的 rect 大小计算
//! 2. zone 间贪心左右排序（信号流：电源 → 主控 → 外设）
//! 3. 画布大小计算

use super::plan::{Rect, ZonePlan};
use super::zone::ZoneTree;
use crate::vector::graph::{BoxKind, McVecGraph};

/// Zone 纸面排布参数
const ZONE_GAP: f64 = 80.0; // zone 间距（M4-0: 从 40 增加到 80，预留走线通道）
const ZONE_PAD: f64 = 20.0; // zone 内边距
const ZONE_MIN_W: f64 = 200.0; // zone 最小宽度
const ZONE_MIN_H: f64 = 150.0; // zone 最小高度
const WIRE_CHANNEL: f64 = 60.0; // 走线通道宽度（与 geom.rs 一致）
const TITLE_H: f64 = 30.0; // 标题栏高度
const BOX_PER_ROW: usize = 4; // 每行 box 数
const MIN_CANVAS_W: f64 = 1200.0; // 顶层最小画布宽
const MIN_CANVAS_H: f64 = 800.0; // 顶层最小画布高

// M4-1a: 子模块专用参数
const SUB_ZONE_MIN_W: f64 = 120.0; // 子模块 zone 最小宽度
const SUB_ZONE_MIN_H: f64 = 100.0; // 子模块 zone 最小高度
const SUB_MIN_CANVAS_W: f64 = 400.0; // 子模块最小画布宽
const SUB_MIN_CANVAS_H: f64 = 300.0; // 子模块最小画布高

/// 根据 box 类型和 pin 数估算尺寸（与 geom::box_size 保持一致）
fn est_box_size(kind: &BoxKind, pin_count: usize) -> (f64, f64) {
    match kind {
        BoxKind::PowerLabel | BoxKind::Dot => (24.0, 24.0),
        BoxKind::TwoPin => (80.0, 60.0),
        BoxKind::MultiPin => {
            let w = (120.0_f64).max(pin_count as f64 * 10.0);
            let h = (80.0_f64).max(pin_count as f64 * 8.0);
            (w, h)
        }
        BoxKind::SubModule => {
            let w = (140.0_f64).max(pin_count as f64 * 10.0);
            let h = (100.0_f64).max(pin_count as f64 * 8.0);
            (w, h)
        }
    }
}

/// 计算 zone 的纸面位置
pub fn place_zones(graph: &McVecGraph, tree: &ZoneTree, is_submodule: bool) -> Vec<ZonePlan> {
    if tree.zones.is_empty() {
        return Vec::new();
    }

    // ── 1. 计算每个 zone 的 rect（使用实际 box 尺寸） ──
    let mut zone_rects: Vec<Rect> = Vec::new();
    for zone in &tree.zones {
        zone_rects.push(compute_zone_rect(zone, graph, is_submodule));
    }

    // ── 2. 按 zone 间连接数做贪心排序 ──
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

    plans
}

/// 计算 zone 的 rect 大小（使用实际 box 尺寸 + 走线通道）
fn compute_zone_rect(zone: &super::zone::Zone, graph: &McVecGraph, is_submodule: bool) -> Rect {
    let (min_w, min_h) = if is_submodule {
        (SUB_ZONE_MIN_W, SUB_ZONE_MIN_H)
    } else {
        (ZONE_MIN_W, ZONE_MIN_H)
    };

    let box_count = zone.boxes.len();
    if box_count == 0 {
        return Rect {
            x: 0.0, y: 0.0,
            w: min_w,
            h: min_h,
        };
    }

    // 收集该 zone 内所有 box 的实际尺寸
    let mut max_w: f64 = 80.0;
    let mut max_h: f64 = 60.0;
    let mut found = 0usize;
    for b in &graph.boxes {
        if zone.boxes.contains(&b.id) {
            let (bw, bh) = est_box_size(&b.kind, b.pin_count);
            max_w = max_w.max(bw);
            max_h = max_h.max(bh);
            found += 1;
        }
    }
    if found == 0 {
        return Rect { x: 0.0, y: 0.0, w: min_w, h: min_h };
    }

    // 按 arrangement 可能的最大层数估算（最多每层 1 个 box，即 N 层）
    let max_layers = box_count;
    let cols = BOX_PER_ROW.min(box_count);
    let rows = (box_count + cols - 1) / cols;

    // 宽度 = 层数 × (max box 宽 + 走线通道) + 内边距
    let w = (max_layers as f64 * (max_w + WIRE_CHANNEL) + ZONE_PAD * 2.0).max(min_w);
    // 高度 = 行数 × (max box 高 + 间距) + 标题栏 + 内边距
    let h = (rows as f64 * (max_h + 10.0) + TITLE_H + ZONE_PAD * 2.0).max(min_h);

    Rect { x: 0.0, y: 0.0, w, h }
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

/// 从 zone plans 计算画布大小（带最小约束，子模块使用更小的最小值）
pub fn compute_canvas(plans: &[ZonePlan], is_submodule: bool) -> (f64, f64) {
    let (min_w, min_h) = if is_submodule {
        (SUB_MIN_CANVAS_W, SUB_MIN_CANVAS_H)
    } else {
        (MIN_CANVAS_W, MIN_CANVAS_H)
    };

    let mut max_x = 0.0f64;
    let mut max_y = 0.0f64;

    for plan in plans {
        let right = plan.rect.x + plan.rect.w;
        let bottom = plan.rect.y + plan.rect.h;
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }

    let w = (max_x + ZONE_PAD).max(min_w);
    let h = (max_y + ZONE_PAD).max(min_h);
    (w, h)
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
        let plans = place_zones(&graph, &tree, false);
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
        let plans = place_zones(&graph, &tree, false);
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
        let (w, h) = compute_canvas(&plans, false);
        assert!(w >= 440.0 + ZONE_PAD);
        assert!(h >= 150.0 + ZONE_PAD);
    }
}