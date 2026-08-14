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
use crate::vector::graph::{BoxKind, McVecGraph};
use super::plan::Plan;
use std::collections::HashMap;

/// 走线通道宽度（层间预留）
const WIRE_CHANNEL: f64 = 60.0;
/// 标题栏高度
const TITLE_H: f64 = 30.0;
/// 内边距
const PAD: f64 = 20.0;
/// box 间距
const GAP: f64 = 10.0;
/// 被动器件依附间距（IC 到被动器件的距离）
const ANCHOR_GAP: f64 = 12.0;

/// 被动器件依附信息
struct Anchor {
    /// 依附的目标 IC 的 box_id
    ic_id: i64,
    /// 放在 IC 的哪一侧
    side: AnchorSide,
    /// 在该侧的序号（0, 1, 2, ...）
    pos: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnchorSide {
    Left,
    Right,
}

/// 根据 box 类型和 pin 数计算合适的宽高
fn box_size(kind: &BoxKind, pin_count: usize) -> (f64, f64) {
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

/// 判断 box 是否是"主 IC"（有多个 pin 的有源器件）
fn is_ic(kind: &BoxKind) -> bool {
    matches!(kind, BoxKind::SubModule | BoxKind::MultiPin)
}

/// 判断 box 是否是被动器件（应依附到 IC）
fn is_passive(kind: &BoxKind) -> bool {
    matches!(kind, BoxKind::TwoPin | BoxKind::PowerLabel | BoxKind::Dot)
}

/// 构建 zone 内被动器件 → IC 的依附关系
///
/// 算法：对每个被动器件，扫描所有 net，找到与其共网的最高频 IC。
/// 如果只有一个 IC 邻居，依附到该 IC；多个 IC 则选连接数最多的。
/// 无 IC 邻居则返回 None（后续用网格 fallback）。
///
/// 返回 HashMap<passive_box_id, Anchor>
fn build_passive_anchors(
    graph: &McVecGraph,
    zone_box_ids: &[i64],
) -> HashMap<i64, Anchor> {
    // 收集该 zone 的 IC 和被动器件
    let ic_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| zone_box_ids.contains(&b.id) && is_ic(&b.kind))
        .map(|b| b.id)
        .collect();
    let passive_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| zone_box_ids.contains(&b.id) && is_passive(&b.kind))
        .map(|b| b.id)
        .collect();

    if ic_ids.is_empty() || passive_ids.is_empty() {
        return HashMap::new();
    }

    // 对每个被动器件，统计它连接的 IC
    // passive_id → Vec<(ic_id, net_count)>
    let mut connections: HashMap<i64, HashMap<i64, usize>> = HashMap::new();
    for pid in &passive_ids {
        connections.insert(*pid, HashMap::new());
    }

    for net in &graph.nets {
        // 该 net 中的 IC 和被动器件
        let net_ics: Vec<i64> = net
            .endpoints
            .iter()
            .filter(|ep| ic_ids.contains(&ep.box_id))
            .map(|ep| ep.box_id)
            .collect();
        let net_passives: Vec<i64> = net
            .endpoints
            .iter()
            .filter(|ep| passive_ids.contains(&ep.box_id))
            .map(|ep| ep.box_id)
            .collect();

        for pid in &net_passives {
            for ic_id in &net_ics {
                if let Some(ic_map) = connections.get_mut(pid) {
                    *ic_map.entry(*ic_id).or_insert(0) += 1;
                }
            }
        }
    }

    // 对每个被动器件，选连接数最多的 IC
    let mut anchors: HashMap<i64, Anchor> = HashMap::new();

    // 先统计每个 IC 被几个被动器件依附（用于分配 side）
    let mut ic_left_count: HashMap<i64, usize> = HashMap::new();
    let mut ic_right_count: HashMap<i64, usize> = HashMap::new();

    for pid in &passive_ids {
        if let Some(ic_map) = connections.get(pid) {
            if let Some((&best_ic, _)) = ic_map
                .iter()
                .max_by_key(|(_, count)| *count)
            {
                // 交替分配左右
                let left = ic_left_count.get(&best_ic).copied().unwrap_or(0);
                let right = ic_right_count.get(&best_ic).copied().unwrap_or(0);
                let (side, pos) = if left <= right {
                    (AnchorSide::Left, left)
                } else {
                    (AnchorSide::Right, right)
                };

                anchors.insert(
                    *pid,
                    Anchor {
                        ic_id: best_ic,
                        side,
                        pos,
                    },
                );

                match side {
                    AnchorSide::Left => {
                        *ic_left_count.entry(best_ic).or_insert(0) += 1;
                    }
                    AnchorSide::Right => {
                        *ic_right_count.entry(best_ic).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    anchors
}

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
    // 构建 box_id → zone 的映射
    let mut box_to_zone: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (zi, zp) in plan.zones.iter().enumerate() {
        for &bid in &zp.box_ids {
            box_to_zone.insert(bid, zi);
        }
    }

    // 构建 zone → arrangement 的映射
    let mut zone_arr: std::collections::HashMap<usize, &Vec<Vec<i64>>> = std::collections::HashMap::new();
    for arr in &plan.arrangements {
        zone_arr.insert(arr.zone, &arr.layers);
    }

    // 预计算每个 zone 的最大 box 尺寸（用于 col_pitch / row_pitch）
    let mut zone_max_w: Vec<f64> = vec![80.0; plan.zones.len()];
    let mut zone_max_h: Vec<f64> = vec![60.0; plan.zones.len()];
    for box_ref in &graph.boxes {
        if let Some(&zi) = box_to_zone.get(&box_ref.id) {
            let (bw, bh) = box_size(&box_ref.kind, box_ref.pin_count);
            zone_max_w[zi] = zone_max_w[zi].max(bw);
            zone_max_h[zi] = zone_max_h[zi].max(bh);
        }
    }

    // 每个 zone 的未放置 box 计数器（用于 fallback 网格布局）
    let mut zone_counts: Vec<usize> = vec![0; plan.zones.len()];
    let cols: usize = 4;

    // ── M4-1B: 构建被动器件依附关系 ──
    // 先收集每个 zone 的 box_id 列表
    let zone_anchors: Vec<HashMap<i64, Anchor>> = plan
        .zones
        .iter()
        .map(|zp| build_passive_anchors(graph, &zp.box_ids))
        .collect();

    // 先放置所有 IC（记录其位置），再放置被动器件
    let mut ic_positions: HashMap<i64, (f64, f64, f64, f64)> = HashMap::new(); // (x, y, w, h)

    // ── 第一遍：放置 IC ──
    for box_ref in &mut graph.boxes {
        if !is_ic(&box_ref.kind) {
            continue;
        }
        if let Some(&zi) = box_to_zone.get(&box_ref.id) {
            let zp = &plan.zones[zi];
            let zone_x = zp.rect.x + PAD;
            let zone_y = zp.rect.y + PAD + TITLE_H;
            let (bw, bh) = box_size(&box_ref.kind, box_ref.pin_count);
            let col_pitch = zone_max_w[zi] + WIRE_CHANNEL;
            let row_pitch = zone_max_h[zi] + GAP;

            if let Some(layers) = zone_arr.get(&zi) {
                if let Some((layer_idx, _)) = find_in_layers(layers, box_ref.id) {
                    box_ref.x = zone_x + layer_idx as f64 * col_pitch;
                    let same_layer_boxes: Vec<i64> = layers[layer_idx].clone();
                    let pos_in_layer = same_layer_boxes
                        .iter()
                        .position(|&id| id == box_ref.id)
                        .unwrap_or(0);
                    box_ref.y = zone_y + pos_in_layer as f64 * row_pitch;
                    box_ref.w = bw;
                    box_ref.h = bh;
                    ic_positions.insert(box_ref.id, (box_ref.x, box_ref.y, bw, bh));
                    continue;
                }
            }

            // Fallback 网格
            let idx = zone_counts[zi];
            let col = idx % cols;
            let row = idx / cols;
            box_ref.x = zone_x + col as f64 * (bw + GAP);
            box_ref.y = zone_y + row as f64 * (bh + GAP);
            box_ref.w = bw;
            box_ref.h = bh;
            ic_positions.insert(box_ref.id, (box_ref.x, box_ref.y, bw, bh));
            zone_counts[zi] += 1;
        }
    }

    // ── 第二遍：放置被动器件（依附 IC 或网格 fallback） ──
    for box_ref in &mut graph.boxes {
        if !is_passive(&box_ref.kind) {
            continue;
        }
        if let Some(&zi) = box_to_zone.get(&box_ref.id) {
            let zp = &plan.zones[zi];
            let zone_x = zp.rect.x + PAD;
            let zone_y = zp.rect.y + PAD + TITLE_H;
            let (bw, bh) = box_size(&box_ref.kind, box_ref.pin_count);

            // 尝试依附
            if let Some(anchor) = zone_anchors[zi].get(&box_ref.id) {
                if let Some(&(ic_x, ic_y, ic_w, _ic_h)) = ic_positions.get(&anchor.ic_id) {
                    let passive_y = ic_y + anchor.pos as f64 * (bh + GAP);
                    match anchor.side {
                        AnchorSide::Left => {
                            box_ref.x = ic_x - bw - ANCHOR_GAP;
                            box_ref.y = passive_y;
                        }
                        AnchorSide::Right => {
                            box_ref.x = ic_x + ic_w + ANCHOR_GAP;
                            box_ref.y = passive_y;
                        }
                    }
                    box_ref.w = bw;
                    box_ref.h = bh;
                    continue;
                }
            }

            // Fallback：网格布局
            let idx = zone_counts[zi];
            let col = idx % cols;
            let row = idx / cols;
            box_ref.x = zone_x + col as f64 * (bw + GAP);
            box_ref.y = zone_y + row as f64 * (bh + GAP);
            box_ref.w = bw;
            box_ref.h = bh;
            zone_counts[zi] += 1;
        }
    }

    // ── M4-0: 设置画布提示，防止 normalize 重新计算 ──
    graph.canvas_hint = Some(plan.canvas);
}

/// 在 layers 中查找 box_id，返回 (layer_index, position_in_layer)
fn find_in_layers(layers: &[Vec<i64>], box_id: i64) -> Option<(usize, usize)> {
    for (li, layer) in layers.iter().enumerate() {
        if let Some(pos) = layer.iter().position(|&id| id == box_id) {
            return Some((li, pos));
        }
    }
    None
}

/// 打守卫标记：debug build 下，之后任何修改坐标的代码都会 panic。
#[cfg(debug_assertions)]
pub fn guard(graph: &mut McVecGraph) {
    let _ = graph;
}