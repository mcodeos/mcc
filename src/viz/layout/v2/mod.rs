// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # v2 · 绞杀者模式
//!
//! 新布局管线，通过环境变量 `MC_LAYOUT_V2=1` 启用。
//! 默认走旧路径，旧路径一行不改，直到 align 度量全绿。
//!
//! ## 架构
//!
//! ```text
//! solve(graph) → Plan
//!   ├── zone::build_zone_tree(graph)     → ZoneTree   (M2-2)
//!   ├── {zone 粗排}                       → ZonePlan[] (M2-3)
//!   ├── quotient::build(graph, zones)    → 商图       (M3)
//!   ├── arrange::layers(quotient)        → Arrangement (M3)
//!   └── cutset::decide(graph, zones)     → CutDecision (M4)
//!
//! geom::apply(graph, &plan)  ← 唯一几何写者
//! ```

pub mod arrange;
pub mod cutset;
pub mod geom;
pub mod plan;
pub mod quotient;
pub mod zone;
pub mod zone_placement;

use crate::vector::graph::McVecGraph;
use plan::Plan;

/// 搜索入口：分析 graph → 产出 Plan。
pub fn solve(graph: &McVecGraph) -> Plan {
    let tree = zone::ZoneTree::build(graph);
    let zone_plans = zone_placement::place_zones(graph, &tree, graph.is_submodule);
    let canvas = zone_placement::compute_canvas(&zone_plans, graph.is_submodule);

    // ── M3: 对每个 zone 构建商图并做层排列 ──
    let mut arrangements: Vec<plan::Arrangement> = Vec::new();
    for zp in &zone_plans {
        if zp.box_ids.is_empty() {
            continue;
        }
        let q = quotient::QuotientGraph::build_for_ids(graph, &zp.box_ids);
        if q.nodes.is_empty() {
            continue;
        }
        // 精确搜索（N ≤ 7），超限时跳过
        if q.nodes.len() <= arrange::EXACT_SEARCH_LIMIT {
            let candidates = arrange::solve(&q);
            if let Some((_cost, best)) = candidates.into_iter().next() {
                arrangements.push(plan::Arrangement {
                    zone: zp.zone,
                    layers: best.layers,
                });
            }
        }
    }

    Plan {
        zones: zone_plans,
        cuts: Vec::new(),
        arrangements,
        canvas,
    }
}