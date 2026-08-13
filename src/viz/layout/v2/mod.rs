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
    let zone_plans = zone_placement::place_zones(graph, &tree);
    let canvas = zone_placement::compute_canvas(&zone_plans);

    Plan {
        zones: zone_plans,
        cuts: Vec::new(),
        arrangements: Vec::new(),
        canvas,
    }
}