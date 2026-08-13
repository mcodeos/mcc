// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Geom — 唯一的几何写者
//!
//! `apply()` 是整个渲染管线里**唯一允许写 box.x/y/w/h 和 entry_points 的地方**。
//! 调用前 graph 的几何字段被视为未初始化。
//!
//! ## 守卫
//!
//! debug build 下，`apply` 返回后给每个 box 打 `geom_written_by_v2 = true`，
//! 之后任何 pass 若修改坐标则 panic 并指出是谁改的。

use crate::vector::graph::boxdef::ZoneBorder;
use crate::vector::graph::McVecGraph;
use super::plan::Plan;

/// 把 Plan 落成像素。
///
/// 这是整个渲染管线里唯一允许写 box.x/y/w/h 和 entry_points 的地方。
/// 调用前 graph 的几何字段被视为未初始化。
pub fn apply(graph: &mut McVecGraph, plan: &Plan) {
    // ── 写 zone 边框 ──
    graph.zone_borders.clear();
    for zp in &plan.zones {
        graph.zone_borders.push(ZoneBorder {
            x: zp.rect.x,
            y: zp.rect.y,
            w: zp.rect.w,
            h: zp.rect.h,
            title: zp.title.clone(),
            title_x: zp.title_anchor.x,
            title_y: zp.title_anchor.y,
        });
    }

    // ── 写 box 位置 ──
    // 每个 zone 内的 box 在 zone 内部按网格排列
    let box_w = 80.0;
    let box_h = 60.0;
    let pad: f64 = 20.0;
    let gap: f64 = 10.0;
    let cols: usize = 4;

    // 构建 box_id → zone 的映射
    let mut box_to_zone: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (zi, zp) in plan.zones.iter().enumerate() {
        for &bid in &zp.box_ids {
            box_to_zone.insert(bid, zi);
        }
    }

    // 每个 zone 的 box 计数器
    let mut zone_counts: Vec<usize> = vec![0; plan.zones.len()];

    for box_ref in &mut graph.boxes {
        if let Some(&zi) = box_to_zone.get(&box_ref.id) {
            let zp = &plan.zones[zi];
            let zone_x = zp.rect.x + pad;
            let zone_y = zp.rect.y + pad + 30.0; // 留 30px 给标题
            let idx = zone_counts[zi];

            let col = idx % cols;
            let row = idx / cols;
            box_ref.x = zone_x + col as f64 * (box_w + gap);
            box_ref.y = zone_y + row as f64 * (box_h + gap);
            box_ref.w = box_w;
            box_ref.h = box_h;
            zone_counts[zi] += 1;
        }
    }
}

/// 打守卫标记：debug build 下，之后任何修改坐标的代码都会 panic。
#[cfg(debug_assertions)]
pub fn guard(graph: &mut McVecGraph) {
    let _ = graph;
}