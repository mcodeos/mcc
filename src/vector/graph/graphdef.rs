// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`McVecGraph`] -- graph container
//!
//! Holds boxes / edges (legacy, deprecated) / nets / sub-graphs of one layer.
//!
//! ## ★ P03 (S1) Changes
//! - `edges` field **kept but no longer populated**:
//!   - `from_block.rs::build_mc_vec_graph` stopped writing to `graph.edges`
//!   - `components.rs::build_adjacency` now reads only `graph.nets`
//!   - `entry_points.rs::collect_pins_per_box` same as above
//!   - `wire.rs::render_edge` removed
//! - `nets: Vec<VizNet>` is the **only network representation**
//! - `total_edges()` / `total_wires()` still compile, but always return 0 under the production path
//!
//! ## Field evolution
//! - `boxes`      -- always present
//! - `edges`      -- **deprecated**, kept only for from_table.rs (legacy builder)
//! - `nets`       -- multi-endpoint hyperedge ([`VizNet`]), the only network model
//! - `sub_graphs` -- recursive sub-graphs

use std::fmt;

use super::boxdef::{McVecBox, PortDir, ZoneBorder};
use super::netdef::{McVecEdge, NetRole, VizNet};

// ============================================================================
// McVecGraph
// ============================================================================

#[derive(Debug, Clone)]
pub struct McVecGraph {
    /// ID of this layer's block (corresponds to InstTable)
    pub bid: i64,
    /// Name of this layer's block (module instance name)
    pub name: String,
    /// Boxes of this layer
    pub boxes: Vec<McVecBox>,
    /// Edges of this layer (★ P03: deprecated, only from_table.rs legacy builder still populates)
    ///
    /// New code cannot read any edge (because from_block no longer writes). Please use `nets`.
    pub edges: Vec<McVecEdge>,
    /// Nets of this layer (the only network representation after P03)
    ///
    /// One `VizNet` per net, no limit on endpoint count. Router uses this to compute paths.
    pub nets: Vec<VizNet>,
    /// Sub-graphs (recursive sub-modules, implementable as expandable)
    pub sub_graphs: Vec<McVecGraph>,
    /// ★ FIX (sub-graph): whether multi-endpoint single-driver nets in this layer use
    /// hub-star routing (with the main device pin as hub, multiple wires fanning out from
    /// the device) instead of TrunkTap (shared trunk). Set by the layouter:
    /// sub-layer = true, top layer = false (top-layer routing behavior unchanged).
    pub fanout_star: bool,
    /// ★ Layout coverage tracking: number of islands claimed by islands decomposition.
    /// Set by `islands::apply_islands`, read by `compute_fidelity` for the gate.
    pub islands_claimed: usize,
    pub islands_total: usize,
    /// ★ M0-2: 模块端口列表（端口名、方向、网络角色），来自模块声明
    pub module_ports: Vec<(String, PortDir, NetRole)>,
    /// ★ M2-3: Zone 边框列表（虚线圆角矩形 + 标题），由 v2 layout 填充
    pub zone_borders: Vec<ZoneBorder>,
    /// ★ M4-0: 画布提示（v2 layout 设置后，normalize 不再从 box 坐标重新计算）
    pub canvas_hint: Option<(f64, f64)>,
    /// ★ M4-1a: 是否为子模块图（子模块使用更小的画布最小约束）
    pub is_submodule: bool,
    /// ★ P7-3: rail 端子装饰（纪律 11：端子不是盒子）。
    ///
    /// R-1/R-3 判为"就地落符号"的电源/地端点，以 pin 渲染属性存在：
    /// 零布局成本、零布线成本、不进 `boxes`；渲染期由 pin 的 entry_point
    /// 定位，符号复用 `PowerRailShape`。
    pub rail_decorations: Vec<RailDecoration>,
    /// ★ P7-4: 本层几何双写诊断（段边界快照对比采集，只观测不阻止）。
    ///
    /// 维度所有权尺子（P7-4e）：xy/wh 归 Placement 段，pins 归 PinPlace 段，
    /// Route 段只读。跨段且越权维度的写入记一条；段内多函数协作自由。
    pub geom_double_writes: Vec<GeomDoubleWrite>,
}

/// ★ P7-4e: 几何段（roadmap 三段的落地细化）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeomStage {
    /// 决定 x/y/w/h：prepare / size / placement / schematic_model / two_lane /
    /// idiom / post / renormalize / net_labels（网标盒初摆）
    Placement,
    /// ★ PinFinal —— pin 驱动的几何终定：pin_place（分配 entry_point +
    /// hub 放大）→ islands/被动件贴 pin 摆位。这些写者互相接力是功能
    /// 需要（贴 pin 摆位依赖 pin 分配结果），段内全维度自由。
    /// 对应 roadmap 的 PinPlace 段，按现实依赖细化命名。
    PinFinal,
    /// 只读几何：route / feedback(reroute 版) / borders
    Route,
}

/// 写者标签 → 逻辑段。
pub fn stage_of(writer: &str) -> GeomStage {
    match writer {
        "7.pin_place" | "8.islands" | "8.sp_fallback" | "8.ladder_fallback"
        | "10.passive_inline" => GeomStage::PinFinal,
        "13.route" | "14.feedback" | "15.borders" | "16.net_labels_2" => GeomStage::Route,
        // 1.prepare 2.size 3.placement 4.schematic_model 5.two_lane 6.idiom
        // 9.post 11.renormalize 12.net_labels
        _ => GeomStage::Placement,
    }
}

/// ★ P7-4: 同一盒子被越权段写几何的结构化诊断
#[derive(Debug, Clone)]
pub struct GeomDoubleWrite {
    pub box_id: i64,
    pub box_name: String,
    pub prev_writer: &'static str,
    pub cur_writer: &'static str,
    /// 本次变化的维度：xy / wh / pins（新增盒子为 new）
    pub dims: Vec<&'static str>,
}

/// ★ P7-4: 段边界几何快照（`geom_snapshot` 的返回值，按 box id 对齐）
#[derive(Debug, Clone)]
pub struct BoxGeomSnapshot {
    sigs: Vec<(i64, f64, f64, f64, f64, Vec<super::boxdef::EntryPoint>)>,
}

/// ★ P7-3: 一个贴在 pin 上的电源/地端子符号
#[derive(Debug, Clone)]
pub struct RailDecoration {
    /// 所属盒子（真盒子，不是符号自身）
    pub box_id: i64,
    /// 被装饰的 pin（InstTable entry id，同 EndpointRef.pin_id）
    pub pin_id: i64,
    /// true = 接地符号（朝下，无文字）；false = rail 端子（朝上，圆点 + 网名）
    pub is_ground: bool,
    /// 显示文本（rail 端子 = 网名；接地符号不用）
    pub label: String,
}

impl McVecGraph {
    /// Create an empty graph
    pub fn new(bid: i64, name: String) -> Self {
        Self {
            bid,
            name,
            boxes: vec![],
            edges: vec![],
            nets: vec![],
            sub_graphs: vec![],
            fanout_star: false,
            islands_claimed: 0,
            islands_total: 0,
            module_ports: vec![],
            zone_borders: vec![],
            canvas_hint: None,
            is_submodule: false,
            rail_decorations: vec![],
            geom_double_writes: vec![],
        }
    }

    // ─── ★ P7-4: 几何写者观测（只观测不阻止） ────────────────────────────

    /// 段前快照：每盒 (id, x, y, w, h, entry_points)，**按 id 对齐**
    /// （段可能删盒/增盒，下标对齐会错位）。
    ///
    /// 段结束后把快照交给 [`McVecGraph::claim_geom_changes`]，几何签名变化
    /// 的盒子即被该段写入。等值重写视为未写（对输出无影响，不计入清单）。
    pub fn geom_snapshot(&self) -> BoxGeomSnapshot {
        BoxGeomSnapshot {
            sigs: self
                .boxes
                .iter()
                .map(|b| (b.id, b.x, b.y, b.w, b.h, b.entry_points.clone()))
                .collect(),
        }
    }

    /// 段后认领：把几何变化的盒子记到 `writer` 名下，按维度所有权判违规：
    /// - xy/wh 变化且 writer 不在 Placement 段 → 记诊断
    /// - pins 变化且 writer 不在 PinPlace 段 → 记诊断
    /// - 段新增的盒子（id 不在快照里）是首写，不记；Placement 段新增合法，
    ///   其余段新增（理论上不存在）也会被 "new" 维度判违规。
    /// 段内（同 `GeomStage`）协作写不记。返回本段写入的盒子数。
    pub fn claim_geom_changes(&mut self, snap: &BoxGeomSnapshot, writer: &'static str) -> usize {
        let stage = stage_of(writer);
        let mut written = 0usize;
        for b in self.boxes.iter_mut() {
            let dims: Vec<&'static str> = match snap.sigs.iter().find(|(id, ..)| *id == b.id) {
                Some((_, x, y, w, h, eps)) => {
                    let mut d = Vec::new();
                    if b.x != *x || b.y != *y {
                        d.push("xy");
                    }
                    if b.w != *w || b.h != *h {
                        d.push("wh");
                    }
                    if &b.entry_points != eps {
                        d.push("pins");
                    }
                    d
                }
                None => vec!["new"],
            };
            if dims.is_empty() {
                continue;
            }
            // ★ 虚线框豁免：负 id 的顶层模块边框（P7-3：负 id、空名）是
            // 画布装饰，跟随内容物收缩，borders 段写它不构成几何双写。
            if b.id < 0 {
                continue;
            }
            let prev = b.geom_writer;
            b.geom_writer = Some(writer);
            written += 1;
            let violates = match stage {
                GeomStage::Placement => dims.contains(&"pins"),
                // PinFinal：pin 分配 + hub 放大 + 贴 pin 摆位，全维度自由
                GeomStage::PinFinal => false,
                GeomStage::Route => true, // 只读段，任何几何变化都是违规
            };
            if violates {
                if let Some(p) = prev {
                    self.geom_double_writes.push(GeomDoubleWrite {
                        box_id: b.id,
                        box_name: b.name.clone(),
                        prev_writer: p,
                        cur_writer: writer,
                        dims,
                    });
                }
            }
        }
        written
    }

    // ─── Statistics ─────────────────────────────────────────────────────────

    /// Recursive total box count
    pub fn total_boxes(&self) -> usize {
        self.boxes.len()
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_boxes())
                .sum::<usize>()
    }

    /// Recursive total edge count (legacy binary edges)
    pub fn total_edges(&self) -> usize {
        self.edges.len()
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_edges())
                .sum::<usize>()
    }

    /// Recursive total wire count (wires inside legacy binary edges)
    pub fn total_wires(&self) -> usize {
        let local: usize = self.edges.iter().map(|e| e.wires.len()).sum();
        local
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_wires())
                .sum::<usize>()
    }

    /// ★ NEW: Recursive total net count (new hyperedge)
    pub fn total_nets(&self) -> usize {
        self.nets.len()
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_nets())
                .sum::<usize>()
    }

    /// ★ NEW: Recursive total endpoint count
    pub fn total_endpoints(&self) -> usize {
        let local: usize = self.nets.iter().map(|n| n.endpoint_count()).sum();
        local
            + self
                .sub_graphs
                .iter()
                .map(|g| g.total_endpoints())
                .sum::<usize>()
    }

    // ─── Sub-graph query ─────────────────────────────────────────────────────

    /// Find a sub-graph by bid (used by frontend to locate during expand)
    pub fn find_subgraph(&self, bid: i64) -> Option<&McVecGraph> {
        if self.bid == bid {
            return Some(self);
        }
        for sub in &self.sub_graphs {
            if let Some(found) = sub.find_subgraph(bid) {
                return Some(found);
            }
        }
        None
    }

    // ─── Display (for debugging, with recursive indentation) ──────────────────

    fn fmt_with_indent(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let ind = "  ".repeat(depth);
        writeln!(
            f,
            "{}Graph(bid={}, name=\"{}\", boxes={}, edges={}, nets={})",
            ind,
            self.bid,
            self.name,
            self.boxes.len(),
            self.edges.len(),
            self.nets.len()
        )?;
        for b in &self.boxes {
            writeln!(
                f,
                "{}  Box(id={}, \"{}\" [{}], kind={}, pins={})",
                ind, b.id, b.name, b.class_name, b.kind, b.pin_count
            )?;
        }
        for e in &self.edges {
            writeln!(
                f,
                "{}  Edge({}->{}, {}, \"{}\")",
                ind, e.src_box, e.dst_box, e.edge_type, e.net_name
            )?;
        }
        for n in &self.nets {
            writeln!(
                f,
                "{}  Net(#{}, \"{}\", {}, endpoints={})",
                ind,
                n.nid,
                n.name,
                n.kind,
                n.endpoints.len()
            )?;
        }
        for sub in &self.sub_graphs {
            sub.fmt_with_indent(f, depth + 1)?;
        }
        Ok(())
    }
}

impl fmt::Display for McVecGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}
