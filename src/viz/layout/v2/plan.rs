// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Plan — 搜索器与几何层之间的唯一契约
//!
//! `Plan` 是搜索器的全部输出，也是几何层的全部输入。
//! 一旦产出就是只读的；`geom::apply` 是唯一被允许写坐标的函数。

use crate::vector::graph::boxdef::McVecBox;

// ============================================================================
// Zone 计划
// ============================================================================

/// 单个 zone 的纸面位置计划
#[derive(Debug, Clone)]
pub struct ZonePlan {
    /// Zone 索引（对应 ZoneTree 中的 zone id）
    pub zone: usize,
    /// 该 zone 的 box ids
    pub box_ids: Vec<i64>,
    /// 纸面矩形 (x, y, w, h)
    pub rect: Rect,
    /// 标题锚点位置
    pub title_anchor: Point,
    /// 标题文本
    pub title: String,
}

/// 纸面矩形
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 2D 点
#[derive(Debug, Clone, Copy, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// ============================================================================
// 割集决策
// ============================================================================

/// 一条边的割集决策：wire 还是 label
#[derive(Debug, Clone)]
pub struct CutDecision {
    /// 端点对 (box_a, box_b) 或 (box_id, port_id)
    pub edge: (i64, i64),
    /// true = wire（画线），false = label（画标签）
    pub is_wire: bool,
}

// ============================================================================
// 分层排列
// ============================================================================

/// 单个 zone 内部的分层排列（M3 填充）
#[derive(Debug, Clone, Default)]
pub struct Arrangement {
    /// 所属 zone id
    pub zone: usize,
    /// 层 → 该层内的 box ids（从左到右）
    pub layers: Vec<Vec<i64>>,
}

// ============================================================================
// Plan
// ============================================================================

/// 布局计划：搜索的全部输出，几何层的全部输入。
///
/// 一旦产出就是只读的；[`super::geom::apply`] 是唯一被允许写坐标的函数。
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// 分区及其纸面位置
    pub zones: Vec<ZonePlan>,
    /// 哪些边走 label（M4 填充）
    pub cuts: Vec<CutDecision>,
    /// 每个 zone 内部的分层（M3 填充）
    pub arrangements: Vec<Arrangement>,
    /// 画布大小
    pub canvas: (f64, f64),
}

impl Plan {
    /// 创建平凡 Plan：所有节点退化成一个 zone，arrangement 暂为空。
    pub fn trivial(boxes: &[McVecBox]) -> Self {
        let canvas = (800.0, 600.0);
        Self {
            zones: vec![ZonePlan {
                zone: 0,
                box_ids: Vec::new(),
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: canvas.0,
                    h: canvas.1,
                },
                title_anchor: Point { x: 0.0, y: 0.0 },
                title: String::new(),
            }],
            cuts: Vec::new(),
            arrangements: Vec::new(),
            canvas,
        }
    }
}