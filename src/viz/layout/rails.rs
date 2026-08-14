// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-3 —— Rail 三分法（契约 C1 的落地，MC_SCHEMATIC_ROADMAP_v6 §1.2）
//!
//! 对每一张带 [`RailSpec`]（由投影层 viz/project.rs 从端口声明解析）的网：
//!
//! ```text
//! driver(N) = spec.driver_pin 指向的端点所属盒子；None = 无源电源域（GND 属此类）
//!
//! R-1  无 driver 的 rail：一条边都不画。
//!      顶层：consumer 侧也不落符号（框图连 GND 都不画，目标图 1）。
//!      子层：每个端点就地落符号（Ground 类 → 接地符号朝下；Power 类 → 端子朝上）。
//!
//! R-2  有 driver 的 rail，对每个 consumer C：
//!      画 driver → C 的边 ⟺ C 是电源域节点（有 Power rail 的 Out 端点）或本层 hub
//!      （信号度最高的盒子）。
//!
//! R-3  R-2 判为"不画边"的 consumer：
//!      顶层不落任何符号；子层落一个就地 rail 端子符号（圆点 + 网名，朝上）。
//! ```
//!
//! ## 端子不是盒子（纪律 11）
//! R-1/R-3 的符号全部进 `graph.rail_decorations`（pin 渲染属性）：
//! 零布局成本、零布线成本、不进 `graph.boxes`。
//! 只有 R-2 的 driver 段建真实 `VizNet` 参与布线，两端是真盒子。
//!
//! ## C5 · 顶层框图不画无源件
//! 顶层（`is_top == true`）额外把二端无源件（R/C/L）从画布上拿掉并从信号网里
//! 撤销其端点；被抽空（<2 端点）的网一并删除——去耦/上拉电阻属于器件级视图。
//!
//! ## 已删除（反模式 §2.3"名字即判据"）
//! `explode_power_rails_to_flags` / `is_rail_box` / `name_has_power_token` ——
//! 对每个 (rail, consumer) 无差别炸 flag、无 driver 概念的旧机器整体移除。

use std::collections::{HashMap, HashSet};

use crate::vector::graph::naming;
use crate::vector::graph::graphdef::RailDecoration;
use crate::vector::graph::netdef::{IoDirection, NetRole};
use crate::vector::graph::{
    BoxKind, EndpointRef, EntryPoint, EntrySide, IoSummary, McVecBox, McVecGraph, NetKind, Symbol,
    VizNet,
};
use crate::vector::model::RailClass;

use super::normalize::{compute_canvas, normalize_positions};

/// 是否电源/地标签盒子。
///
/// ★ P7-3：旧的 `is_rail_box` 是 `(symbol.is_power_rail() || kind==PowerLabel)
/// && name_has_power_token(name)` —— 名字 token 表已随爆炸机器一起删除（反模式
/// §2.3"名字即判据"）。替换为纯 kind 判定：rail flag 盒子在 P7-3 后已不存在，
/// 剩下的 PowerLabel 是 `apply_net_labels` 造的网标盒子（也是要排除出核心布局的）。
/// 下游 20+ 个"排除守卫"（pin_place / passive_inline / islands / sp / ladder /
/// coalesce / semantic）语义不变，只是判据从名字改成结构。
pub fn is_rail_box(b: &McVecBox) -> bool {
    b.kind == BoxKind::PowerLabel
}

/// Driver 段网 id 基址（避免与既有 nid 冲突）
const DRIVER_NET_ID_BASE: i64 = 9_600_000_000;

/// ★ P7-3 主入口：对本层执行 R-1/R-2/R-3 三分法 + （顶层）C5。
pub fn classify_rails(graph: &mut McVecGraph, is_top: bool) {
    let has_rails = graph.nets.iter().any(|n| n.rail.is_some());
    if !has_rails {
        if is_top {
            drop_top_passives(graph);
        }
        return;
    }

    // ── 每盒元数据（在删除任何网之前计算）──────────────────────────────
    // 电源域节点：拥有 Power rail 的 Out 端点（modldo.VCC / moddcdc.VCC_1V2）
    let mut power_domain_boxes: HashSet<i64> = HashSet::new();
    // 信号度：本层信号网参与数（hub = 最高者，平局取 id 最小）。
    // ★ Signal 与 SubModuleIO 都算——promote（P08）把跨模块 Signal 网改写成
    //   SubModuleIO，只数 Signal 会得到空集、hub 判定失效（P7-3 实测踩过）。
    let mut signal_degree: HashMap<i64, usize> = HashMap::new();
    for net in &graph.nets {
        match net.kind {
            NetKind::Signal | NetKind::SubModuleIO => {
                for b in net.box_ids() {
                    *signal_degree.entry(b).or_insert(0) += 1;
                }
            }
            _ => {}
        }
        if let Some(spec) = &net.rail {
            if spec.class == RailClass::Power {
                for e in &net.endpoints {
                    if e.io_type == IoDirection::Output {
                        power_domain_boxes.insert(e.box_id);
                    }
                }
            }
        }
    }
    let hub: Option<i64> = signal_degree
        .iter()
        .max_by_key(|(id, deg)| (**deg, -*id))
        .map(|(id, _)| *id);

    // ── 逐张 rail 网三分 ────────────────────────────────────────────────
    let mut driver_edges: Vec<VizNet> = Vec::new();
    let mut decorations: Vec<RailDecoration> = Vec::new();
    let mut keep = vec![true; graph.nets.len()];
    let mut next_nid = DRIVER_NET_ID_BASE;

    for (idx, net) in graph.nets.iter().enumerate() {
        let Some(spec) = net.rail.clone() else { continue };
        keep[idx] = false; // 原始 rail 网必然被替换（边/装饰/删除）

        // 每盒取第一个端点为代表（同盒多 pin = 同一 consumer 的重复端点）
        let mut per_box: Vec<(i64, EndpointRef)> = Vec::new();
        for e in &net.endpoints {
            if !per_box.iter().any(|(b, _)| *b == e.box_id) {
                per_box.push((e.box_id, e.clone()));
            }
        }

        let driver = spec
            .driver_pin
            .and_then(|pin| per_box.iter().find(|(_, e)| e.pin_id == pin).cloned());

        crate::vlog!(
            "[layout::rails] layer='{}' rail net '{}' (class={:?}, driver_pin={:?}): {} endpoint(s) over {} box(es) → {:?}",
            graph.name,
            net.name,
            spec.class,
            spec.driver_pin,
            net.endpoints.len(),
            per_box.len(),
            driver.as_ref().map(|(b, e)| (b, e.pin_id))
        );

        match driver {
            None => {
                // ── R-1：无 driver（GND / 找不到产生侧）──────────────────
                // S1：每个 GND 端点（逐 pin，同盒多 pin 也要）恰好 1 个符号
                if !is_top {
                    for e in &net.endpoints {
                        decorations.push(RailDecoration {
                            box_id: e.box_id,
                            pin_id: e.pin_id,
                            is_ground: spec.class == RailClass::Ground,
                            label: net.name.clone(),
                        });
                    }
                }
            }
            Some((drv_box, drv_ep)) => {
                // ── R-2 / R-3 ────────────────────────────────────────────
                let mut driver_consumed = false;
                for (cbox, cep) in &per_box {
                    if *cbox == drv_box {
                        continue;
                    }
                    let qualifies = power_domain_boxes.contains(cbox) || Some(*cbox) == hub;
                    if qualifies {
                        driver_consumed = true;
                        let eps = vec![drv_ep.clone(), cep.clone()];
                        driver_edges.push(VizNet::new(
                            next_nid,
                            net.name.clone(),
                            NetKind::Power,
                            NetRole::Rail { volt: spec.volt.clone() },
                            eps,
                        ));
                        next_nid += 1;
                    } else if !is_top {
                        // R-3 子层：就地 rail 端子
                        decorations.push(RailDecoration {
                            box_id: cep.box_id,
                            pin_id: cep.pin_id,
                            is_ground: false,
                            label: net.name.clone(),
                        });
                    }
                }
                // 子层：没被任何 driver 段消费的 driver 引脚也要落端子，
                // 否则该 pin 视觉悬空（电源从这来，画个圆点+网名）。
                if !is_top && !driver_consumed {
                    decorations.push(RailDecoration {
                        box_id: drv_box,
                        pin_id: drv_ep.pin_id,
                        is_ground: false,
                        label: net.name.clone(),
                    });
                }
            }
        }
    }

    // ── 应用：rail 网 → driver 段 + 装饰 ────────────────────────────────
    let n_rail = keep.iter().filter(|k| !**k).count();
    let mut idx = 0usize;
    graph.nets.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
    graph.nets.extend(driver_edges);
    graph.rail_decorations.extend(decorations);

    crate::vlog!(
        "[layout::rails] P7-3: {} rail net(s) classified → {} driver edge(s), {} decoration(s) (is_top={})",
        n_rail,
        graph.nets.iter().filter(|n| n.nid >= DRIVER_NET_ID_BASE).count(),
        graph.rail_decorations.len(),
        is_top
    );

    // ── C5：顶层不画无源件 ──────────────────────────────────────────────
    if is_top {
        drop_top_passives(graph);
    }
}

/// ★ C5：把二端无源件从顶层拿掉（框图粒度），并撤销其在信号网上的端点。
/// 被抽到 <2 端点的网（如只剩 flash.3 的上拉 _WP）一并删除。
fn drop_top_passives(graph: &mut McVecGraph) {
    let passive_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.is_two_pin_passive())
        .map(|b| b.id)
        .collect();
    if passive_ids.is_empty() {
        return;
    }
    let n_boxes = passive_ids.len();
    graph.boxes.retain(|b| !passive_ids.contains(&b.id));

    let mut dropped_nets = 0usize;
    let mut cleaned: Vec<VizNet> = Vec::with_capacity(graph.nets.len());
    for mut net in std::mem::take(&mut graph.nets) {
        let before = net.endpoints.len();
        net.endpoints.retain(|e| !passive_ids.contains(&e.box_id));
        if net.endpoints.len() < 2 && before >= 2 {
            dropped_nets += 1;
            continue; // 被抽空的网（单端悬空）不画
        }
        cleaned.push(net);
    }
    graph.nets = cleaned;
    crate::vlog!(
        "[layout::rails] C5: dropped {} top-level passive box(es), {} emptied net(s)",
        n_boxes,
        dropped_nets
    );
}

// ============================================================================
// ★ Stage 1: net labels / air wires (long-net → named stubs)
// ============================================================================
//
// Long signal nets spanning the whole graph pass through a bunch of boxes → a bunch of crossings → a bunch of jumpers (bridges), the graph becomes messy. Industrial schematic
// standard practice is **net labels (net label / air wires)**: don't draw that long wire, but place a same-name short label stub next to each endpoint,
// same name = electrically connected. This pass transforms "long signal nets" into such label stubs:
//   - Create a **single-pin PowerLabel** next to each endpoint (reuses existing flag rendering, same style as sub-graph boundary ports) +
//     one **short stub** (label pin ↔ original pin), then **delete that long net**.
//   - Only modify nets of `NetKind::Signal` with **span over threshold**; power/ground (already flags), buses, and nets with either endpoint
//     already connected to label/flag are not touched.
//
// Must run **after layout, before routing** (at this point boxes have coordinates, can judge "long" by span; routing hasn't run yet,
// modifying boxes is safe). Hooked in api.rs Phase 1.8. Returns new canvas size (added label boxes, boundary needs recalculation).

const NETLABEL_LONG_SPAN: f64 = 650.0; // Span over this value (px) to convert to air wire (adjustable)
const NETLABEL_GAP: f64 = 42.0; // Distance of label from pin
const NETLABEL_W: f64 = 14.0;
const NETLABEL_H: f64 = 14.0;

/// Pin coordinates = box edge + offset (consistent with renderer pin_position, inlined to avoid cross-module dependencies).
fn pin_xy(b: &McVecBox, ep: &EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Top => (b.x + b.w * ep.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * ep.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * ep.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * ep.offset),
    }
}

/// ★ Stage 1 main entry: convert long signal nets to net label stubs. Returns `Some(new canvas)` if changed, else `None`.
pub fn apply_net_labels(graph: &mut McVecGraph) -> Option<(f64, f64)> {
    // 1. (box_id, pin_id) → (pin coordinates, side); record which boxes are labels/flags (PowerLabel).
    let mut pin_pos: HashMap<(i64, i64), ((f64, f64), EntrySide)> = HashMap::new();
    let mut label_boxes: HashSet<i64> = HashSet::new();
    // ★ Stage A (A3): a net touching any two-pin passive must keep a real wire (never an air-wire),
    //   otherwise a plain series R/C loop turns into unreadable dangling labels (see image2).
    let mut passive_boxes: HashSet<i64> = HashSet::new();
    for b in &graph.boxes {
        if b.kind == BoxKind::PowerLabel {
            label_boxes.insert(b.id);
        }
        if b.is_two_pin_passive() {
            passive_boxes.insert(b.id);
        }
        for ep in &b.entry_points {
            pin_pos.insert((b.id, ep.pin_id), (pin_xy(b, ep), ep.side.clone()));
        }
    }

    // New box / new net ids increment from existing max value, eliminating collisions (two namespaces are independent).
    let mut next_box = graph.boxes.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    let mut next_net = graph.nets.iter().map(|n| n.nid).max().unwrap_or(0) + 1;

    let mut new_boxes: Vec<McVecBox> = Vec::new();
    let mut new_stubs: Vec<VizNet> = Vec::new();
    let mut drop_idx: HashSet<usize> = HashSet::new();

    for (idx, net) in graph.nets.iter().enumerate() {
        if !matches!(net.kind, NetKind::Signal) {
            continue; // Only process signal nets
        }
        if net.endpoints.len() < 2 {
            continue;
        }
        // ★ FIX: Only convert "meaningfully named" nets to air wires. Anonymous nets (__net_N / empty name) converting to labels is meaningless
        //   —— both ends labeled __net_25 users can't read, equals making visible wires disappear (this regression root cause). Anonymous nets drawn normally.
        if net.name.is_empty() || net.name.starts_with("__net") {
            continue;
        }
        // Either endpoint already label/flag → already "labeled", don't repeat (includes sub-graph boundary ports, power flags).
        if net
            .endpoints
            .iter()
            .any(|e| label_boxes.contains(&e.box_id))
        {
            continue;
        }
        // ★ Stage A (A3): never air-wire a net that touches a two-pin passive — its pins must
        //   be reached by real wires.
        if net
            .endpoints
            .iter()
            .any(|e| passive_boxes.contains(&e.box_id))
        {
            continue;
        }
        // ★ Stage A (A3): a net spanning only two boxes is a plain point-to-point wire, not a
        //   long bus worth labelling — route it normally regardless of pixel span.
        if net.box_ids().len() < 3 {
            continue;
        }
        // Endpoint coordinates + span (max pairwise distance between endpoints).
        let pts: Vec<(f64, f64)> = net
            .endpoints
            .iter()
            .filter_map(|e| pin_pos.get(&(e.box_id, e.pin_id)).map(|(p, _)| *p))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let mut span = 0.0_f64;
        for a in 0..pts.len() {
            for b in (a + 1)..pts.len() {
                let d = ((pts[a].0 - pts[b].0).powi(2) + (pts[a].1 - pts[b].1).powi(2)).sqrt();
                if d > span {
                    span = d;
                }
            }
        }
        if span < NETLABEL_LONG_SPAN {
            continue; // Short nets drawn normally
        }

        // ── Long signal net → one same-name label + one short stub per endpoint ──
        let is_gnd = naming::is_ground(&net.name);
        let lio = if is_gnd {
            IoDirection::Ground
        } else {
            IoDirection::Passive
        };
        for e in &net.endpoints {
            let ((px, py), side) = match pin_pos.get(&(e.box_id, e.pin_id)) {
                Some(v) => (v.0, v.1.clone()),
                None => continue,
            };
            // Label placed at GAP away from pin's outward direction; label's own pin turns back to face original pin (stub is a short straight line).
            let (bx, by, lside) = match side {
                EntrySide::Right => (px + NETLABEL_GAP, py - NETLABEL_H / 2.0, EntrySide::Left),
                EntrySide::Left => (
                    px - NETLABEL_GAP - NETLABEL_W,
                    py - NETLABEL_H / 2.0,
                    EntrySide::Right,
                ),
                EntrySide::Top => (
                    px - NETLABEL_W / 2.0,
                    py - NETLABEL_GAP - NETLABEL_H,
                    EntrySide::Bottom,
                ),
                EntrySide::Bottom => (px - NETLABEL_W / 2.0, py + NETLABEL_GAP, EntrySide::Top),
            };

            let box_id = next_box;
            next_box += 1;
            let pin_id = box_id; // Single pin, pin_id reuses box_id for uniqueness

            let mut io = IoSummary::new();
            io.other += 1;
            let mut lbox = McVecBox::new_v2(
                box_id,
                net.name.clone(),
                String::new(),
                BoxKind::PowerLabel,
                Symbol::PowerRail { is_ground: is_gnd },
                None,
                None,
                1,
                io,
                net.name.clone(),
                Vec::new(),
            );
            lbox.x = bx;
            lbox.y = by;
            lbox.w = NETLABEL_W;
            lbox.h = NETLABEL_H;
            lbox.entry_points = vec![EntryPoint {
                pin_id,
                pin_name: net.name.clone(),
                side: lside,
                offset: 0.5,
            }];
            new_boxes.push(lbox);

            let eps = vec![
                EndpointRef::with_io(box_id, pin_id, net.name.clone(), lio),
                e.clone(),
            ];
            // stub inherits original kind → SubModuleIO air wire stubs remain purple, consistent with same-name other segments visually
            new_stubs.push(VizNet::new(
                next_net,
                net.name.clone(),
                net.kind.clone(),
                NetRole::Signal,
                eps,
            ));
            next_net += 1;
        }
        drop_idx.insert(idx);
    }

    if new_boxes.is_empty() {
        return None;
    }

    // Apply: delete long net, add label + stub.
    let mut i = 0usize;
    graph.nets.retain(|_| {
        let keep = !drop_idx.contains(&i);
        i += 1;
        keep
    });
    let n_lbl = new_boxes.len();
    let n_drop = drop_idx.len();
    graph.boxes.extend(new_boxes);
    graph.nets.extend(new_stubs);

    crate::vlog!(
        "[viz::net_label] layer '{}' bid={}: {} long signal net(s) → {} label stub(s)",
        graph.name,
        graph.bid,
        n_drop,
        n_lbl
    );

    // Labels may extend past original canvas / land in negative coordinates → renormalize + recompute canvas (no routing yet, only modifying boxes is safe).
    normalize_positions(graph);
    Some(compute_canvas(graph))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_rail(id: i64, name: &str, is_ground: bool) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground },
            None,
            None,
            1,
            IoSummary::new(),
            name.to_string(),
            Vec::new(),
        )
    }

    fn mk_mod(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::SubModule,
            Symbol::Module,
            None,
            None,
            4,
            IoSummary::new(),
            name.to_string(),
            Vec::new(),
        )
    }

    /// Set box position + one pin (for net-label testing).
    fn placed(mut b: McVecBox, x: f64, w: f64, pin: i64, side: EntrySide) -> McVecBox {
        b.x = x;
        b.y = 0.0;
        b.w = w;
        b.h = 100.0;
        b.entry_points = vec![EntryPoint {
            pin_id: pin,
            pin_name: "S".into(),
            side,
            offset: 0.5,
        }];
        b
    }

    #[test]
    fn net_label_converts_long_signal_net() {
        // Stage A (A3): a net touching only 2 boxes is a point-to-point wire
        // and is NOT converted to labels regardless of span.
        // This test verifies the 3-box minimum requirement.
        let mut g = McVecGraph::new(0, "main".into());
        // A (right) at (100,50) → C (pass-through) → B (left) at (1000,50)
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 1000.0, 100.0, 21, EntrySide::Left));
        g.boxes
            .push(placed(mk_mod(3, "C"), 400.0, 100.0, 31, EntrySide::Right));
        g.nets.push(VizNet::new(
            50,
            "SIG".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Output),
                EndpointRef::with_io(3, 31, "S", IoDirection::Input),
                EndpointRef::with_io(2, 21, "S", IoDirection::Input),
            ],
        ));

        let r = apply_net_labels(&mut g);
        assert!(
            r.is_some(),
            "Long signal net (3 boxes, span > 650) should be converted to label"
        );
        assert!(
            g.nets.iter().all(|n| n.nid != 50),
            "Original long net should be deleted"
        );
    }

    #[test]
    fn net_label_leaves_short_net_alone() {
        // A right pin (100,50) ↔ B left pin (150,50): span 50 < 650 → don't touch
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 150.0, 100.0, 21, EntrySide::Left));
        g.nets.push(VizNet::new(
            50,
            "SIG".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Output),
                EndpointRef::with_io(2, 21, "S", IoDirection::Input),
            ],
        ));

        let r = apply_net_labels(&mut g);
        assert!(r.is_none(), "Short net doesn't convert to label");
        assert_eq!(g.nets.len(), 1, "Short net stays as is");
        assert!(
            g.boxes.iter().all(|x| x.kind != BoxKind::PowerLabel),
            "Shouldn't create label boxes"
        );
    }

    #[test]
    fn net_label_skips_power_net() {
        // Same distance, but kind=Ground → don't process (power/ground have their own flag rendering)
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 1000.0, 100.0, 21, EntrySide::Left));
        g.nets.push(VizNet::new(
            50,
            "GND".into(),
            NetKind::Ground,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Ground),
                EndpointRef::with_io(2, 21, "S", IoDirection::Ground),
            ],
        ));
        assert!(
            apply_net_labels(&mut g).is_none(),
            "Ground net doesn't convert to label"
        );
        assert_eq!(g.nets.len(), 1);
    }

    // ── ★ P7-3 classify_rails 三分法测试（R-1 / R-2 / R-3 / C5）─────────────

    fn rail_net(
        nid: i64,
        name: &str,
        class: RailClass,
        driver_pin: Option<i64>,
        eps: Vec<(i64, i64, IoDirection)>,
    ) -> VizNet {
        let mut n = VizNet::new(
            nid,
            name.into(),
            if class == RailClass::Ground { NetKind::Ground } else { NetKind::Power },
            NetRole::Rail { volt: None },
            eps.into_iter()
                .map(|(b, p, io)| EndpointRef::with_io(b, p, name, io))
                .collect(),
        );
        n.rail = Some(crate::vector::model::RailSpec {
            class,
            driver_pin,
            volt: None,
        });
        n
    }

    #[test]
    fn r1_ground_no_driver_no_edge_no_symbol_at_top() {
        // R-1 顶层：GND 无 driver —— 一条边都不画，也不落符号
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "A"));
        g.boxes.push(mk_mod(2, "B"));
        g.nets.push(rail_net(
            10,
            "GND",
            RailClass::Ground,
            None,
            vec![(1, 11, IoDirection::Passive), (2, 21, IoDirection::Passive)],
        ));
        classify_rails(&mut g, /*is_top=*/ true);
        assert!(g.nets.is_empty(), "GND 网应被删除: {:?}", g.nets);
        assert!(g.rail_decorations.is_empty(), "顶层 R-1 不落符号");
    }

    #[test]
    fn r1_ground_symbols_per_pin_at_sub_layer() {
        // R-1 子层：每个 GND 端点恰好 1 个接地符号（S1）
        let mut g = McVecGraph::new(0, "modA".into());
        g.boxes.push(mk_mod(1, "IC"));
        g.boxes.push(mk_mod(2, "C1"));
        g.nets.push(rail_net(
            10,
            "GND",
            RailClass::Ground,
            None,
            vec![
                (1, 11, IoDirection::Passive),
                (1, 12, IoDirection::Passive), // 同盒第二个 GND pin
                (2, 21, IoDirection::Passive),
            ],
        ));
        classify_rails(&mut g, /*is_top=*/ false);
        assert!(g.nets.is_empty());
        assert_eq!(g.rail_decorations.len(), 3, "每个端点一个符号（同盒多 pin 也要）");
        assert!(g.rail_decorations.iter().all(|d| d.is_ground));
    }

    #[test]
    fn r2_edges_only_to_power_domain_and_hub() {
        // 七行核对表的缩影：V3V3 = driver modldo → {moddcdc(电源域✓), mcu513(hub✓),
        // speaker(✗), flash(✗)} → 恰好 2 条 driver 边；R-3 顶层不落符号
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "modldo"));   // driver（VCC Out）
        g.boxes.push(mk_mod(2, "moddcdc"));  // 电源域节点（VCC_1V2 Out 在另一条 rail 上）
        g.boxes.push(mk_mod(3, "mcu513"));   // hub（8 条信号网 → 这里给 2 条已是最大）
        g.boxes.push(mk_mod(4, "speaker"));
        // 信号网：让 mcu513 成为 hub
        g.nets.push(VizNet::new(
            20,
            "S1".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(3, 31, "S", IoDirection::Output),
                EndpointRef::with_io(4, 41, "S", IoDirection::Input),
            ],
        ));
        g.nets.push(VizNet::new(
            21,
            "S2".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(3, 32, "S", IoDirection::Output),
                EndpointRef::with_io(4, 42, "S", IoDirection::Input),
            ],
        ));
        // moddcdc 的电源域资格：另一条 Power rail 上的 Out 端点（V1V2 由它驱动）
        g.nets.push(rail_net(
            11,
            "V1V2",
            RailClass::Power,
            Some(22),
            vec![(2, 22, IoDirection::Output), (3, 33, IoDirection::Input)],
        ));
        // 被测 rail：V3V3
        g.nets.push(rail_net(
            10,
            "V3V3",
            RailClass::Power,
            Some(11),
            vec![
                (1, 11, IoDirection::Output),   // modldo.VCC = driver
                (2, 21, IoDirection::Input),    // moddcdc 消费
                (3, 34, IoDirection::Bidir),    // mcu513 消费
                (4, 43, IoDirection::Input),    // speaker 消费
            ],
        ));
        classify_rails(&mut g, /*is_top=*/ true);

        // V1V2: driver moddcdc(2) → mcu513(3, hub) 1 条；V3V3: modldo(1) → {moddcdc(2 电源域), mcu513(3 hub)} 2 条
        let power_edges: Vec<&VizNet> = g
            .nets
            .iter()
            .filter(|n| matches!(n.kind, NetKind::Power))
            .collect();
        assert_eq!(power_edges.len(), 3, "V1V2 1 条 + V3V3 2 条 = 3 条 driver 边");
        let v33: Vec<(i64, i64)> = power_edges
            .iter()
            .filter(|n| n.name == "V3V3")
            .map(|n| (n.endpoints[0].box_id, n.endpoints[1].box_id))
            .collect();
        assert!(v33.contains(&(1, 2)), "modldo→moddcdc（电源域）: {v33:?}");
        assert!(v33.contains(&(1, 3)), "modldo→mcu513（hub）: {v33:?}");
        assert!(!v33.iter().any(|(_, t)| *t == 4), "speaker R-3 不画边");
        assert!(g.rail_decorations.is_empty(), "顶层 R-3 不落符号");
    }

    #[test]
    fn r3_sub_layer_consumers_get_rail_terminals() {
        // R-3 子层：不画边的 consumer 落 rail 端子（圆点+网名，非地）
        let mut g = McVecGraph::new(0, "modLDO".into());
        g.boxes.push(mk_mod(1, "ldo")); // driver（子层组件，pin 无 out → 由 spec 指定）
        g.boxes.push(mk_mod(2, "CAP")); // 普通消费
        g.nets.push(rail_net(
            10,
            "VCC",
            RailClass::Power,
            Some(11),
            vec![(1, 11, IoDirection::Passive), (2, 21, IoDirection::Passive)],
        ));
        classify_rails(&mut g, /*is_top=*/ false);
        // 消费者 CAP 无资格 → 无边；子层落端子；driver pin 画了边就不再落
        assert!(g.nets.iter().all(|n| n.rail.is_none()), "rail 网应被替换");
        // hub 判定：无信号网 → hub=None；CAP 无电源域资格 → 0 边
        // driver pin 未被边消费 → 也落端子
        assert_eq!(g.rail_decorations.len(), 2, "driver pin + consumer pin 各一个端子");
        assert!(g.rail_decorations.iter().all(|d| !d.is_ground));
        assert_eq!(g.rail_decorations[0].label, "VCC");
    }

    #[test]
    fn c5_top_layer_drops_two_pin_passives() {
        // C5：顶层不画无源件；被抽空的 _WP 网消失，跨模块网保留
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "flash"));
        g.boxes.push(mk_mod(2, "mcu"));
        let mut res = mk_mod(3, "RES");
        res.kind = BoxKind::TwoPin;
        res.symbol = Symbol::Resistor;
        res.class_name = "RES".into();
        g.boxes.push(res);
        // _WP: flash.3 ~ RES.1 —— 抽走 RES 后只剩 1 端 → 删
        g.nets.push(VizNet::new(
            30,
            "_WP".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 13, "3", IoDirection::Passive),
                EndpointRef::with_io(3, 31, "1", IoDirection::Passive),
            ],
        ));
        // CSN: flash.1 ~ RES.2 ~ mcu.10 —— 抽走 RES 后仍 2 端 → 留
        g.nets.push(VizNet::new(
            31,
            "CSN".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "1", IoDirection::Passive),
                EndpointRef::with_io(3, 32, "2", IoDirection::Passive),
                EndpointRef::with_io(2, 21, "10", IoDirection::Passive),
            ],
        ));
        classify_rails(&mut g, /*is_top=*/ true);
        assert!(!g.boxes.iter().any(|b| b.id == 3), "无源件盒子应删除");
        assert_eq!(g.nets.len(), 1, "_WP 删除、CSN 保留: {:?}", g.nets.iter().map(|n| &n.name).collect::<Vec<_>>());
        assert_eq!(g.nets[0].name, "CSN");
        assert_eq!(g.nets[0].endpoints.len(), 2);
    }

    #[test]
    fn is_rail_box_is_kind_based_not_name_based() {
        // ★ P7-3：name_has_power_token 关键字表已删除——判据只剩 kind
        assert!(is_rail_box(&mk_rail(1, "任意名字", true)), "PowerLabel kind 即 rail box");
        assert!(!is_rail_box(&mk_mod(2, "V3V3_ldo_power")), "名字带 token 也不算");
    }
}
