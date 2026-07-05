// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Phase 2 — Signal-chain–driven sub-module layout engine
//!
//! ## What problem does this file solve
//! Sub-module layouts (e.g. DC-DC converters, LDOs, sensor front-ends) contain a main IC
//! surrounded by passive components (R, C, L) arranged along signal chains.  The existing
//! `FlowLayouter::sub()` treats all boxes equally, scattering passives and producing many
//! crossings.  `SchematicSubLayouter` uses the signal chain topology extracted by
//! [`chain::extract_signal_chains`] to place passives **along their IC pin's signal path**,
//! producing layouts that resemble hand-drawn schematics.
//!
//! ## Algorithm — Chain-Driven Pin-Anchored Placement
//!
//! ```text
//! ┌────────────────────────────────────┐
//! │ 1. PREPARE                         │
//! │    explode_power_rails_to_flags    │
//! │    promote_synthetic_pins          │
//! │    split_shared_pins               │
//! │    assign_default_sizes            │
//! │    assign_entry_points_coarse      │
//! └─────────────┬──────────────────────┘
//!               ▼
//! ┌────────────────────────────────────┐
//! │ 2. EXTRACT                         │
//! │    extract_signal_chains → hub +   │
//! │    chains + orphans                │
//! └─────────────┬──────────────────────┘
//!               ▼
//! ┌────────────────────────────────────┐
//! │ 3. ASSIGN DIRECTIONS               │
//! │    Classify each hub pin to a side │
//! │    (Left/Right/Up/Down) by pin     │
//! │    name semantics + balance        │
//! └─────────────┬──────────────────────┘
//!               ▼
//! ┌────────────────────────────────────┐
//! │ 4. PLACE                           │
//! │    a. hub IC → canvas center       │
//! │    b. each chain: extend outward   │
//! │       from hub pin in assigned dir │
//! │    c. loop chains (decoupling):    │
//! │       place near hub, offset       │
//! │    d. orphans → fill gaps          │
//! └─────────────┬──────────────────────┘
//!               ▼
//! ┌────────────────────────────────────┐
//! │ 5. POST                            │
//! │    pin_place_pipeline              │
//! │    recompute_sizes_with_pin_count  │
//! │    PlaceOptimizer                  │
//! │    normalize_positions             │
//! │    compute_canvas                  │
//! └────────────────────────────────────┘
//! ```
//!
//! ## Ideal Layout (moddcdc DC-DC example)
//!
//! ```text
//!             [VDD_3V3]
//!                 │
//!             C_dcdc_vin
//!                 │
//! C_dcdc_en ── lp322dcdc ── L_dcdc ── [VCC_1V2]
//!  (EN decoup)   │ (FB)                   │
//!             R_fb_high             C_dcdc_vout_10u
//!                │                  C_dcdc_vout_100n
//!             C_dcdc_fb                   │
//!                │                      [GND]
//!             R_fb_low
//!                │
//!              [GND]
//! ```

use std::collections::{HashMap, HashSet};

use crate::vector::graph::{naming, McVecGraph, NetKind};

use super::chain::{self, ChainDir, SignalChain, SignalChainResult};
use super::entry_points::{assign_entry_points_coarse, promote_synthetic_pins, split_shared_pins};
use super::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use super::optimize::PlaceOptimizer;
use super::rails::{explode_power_rails_to_flags, is_rail_box};
use super::size::{assign_default_sizes, recompute_sizes_with_pin_count, MIN_GAP};
use crate::viz::traits::Layouter;

// ============================================================================
// Configuration
// ============================================================================

/// Signal-chain–driven sub-module layouter
///
/// Designed for sub-level graphs where a central IC (hub) has passive chains
/// radiating from its pins.  Produces clean, schematic-like layouts.
pub struct SchematicSubLayouter {
    /// Horizontal gap between chain elements
    pub chain_gap: f64,
    /// Vertical gap between stacked chains on the same pin
    pub stack_gap: f64,
    /// Gap between loop (decoupling) capacitor and hub edge
    pub loop_offset: f64,
    /// Gap between power label and its consumer
    pub flag_gap: f64,
}

impl Default for SchematicSubLayouter {
    fn default() -> Self {
        Self {
            chain_gap: 60.0,
            stack_gap: 70.0,
            loop_offset: 80.0,
            flag_gap: 50.0,
        }
    }
}

// ============================================================================
// Layouter trait implementation
// ============================================================================

impl Layouter for SchematicSubLayouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        // ── Phase 1a: Connectivity-preserving prep ──
        //   These promote synthetic endpoints to real pin_ids and split shared
        //   pins, but do NOT drop or shatter any nets, so the shared power/ground
        //   hyperedges the chain extractor relies on stay intact.
        graph.fanout_star = false;
        promote_synthetic_pins(graph);
        split_shared_pins(graph);

        // ── Phase 2: Extract signal chains (BEFORE explode) ──
        //   ★ explode_power_rails_to_flags shatters the shared GND/VCC hyperedge
        //   `[rail, hub, cap1, cap2, …]` into per-consumer stubs, disconnecting the
        //   hub from its decoupling passives. Extraction must see the intact graph,
        //   so it runs here — before Phase 1b explodes the rails for rendering.
        let result = chain::extract_signal_chains(graph);

        crate::vlog!(
            "[layout::schematic_sub] hub='{}' (id={}), {} chain(s), {} orphan(s)",
            result.hub_name,
            result.hub_id,
            result.chains.len(),
            result.orphan_ids.len()
        );
        crate::vlog!("{}", result.dump(graph));

        // ── Phase 1b: Now safe to explode rails into per-consumer flags ──
        //   Extraction is done, so shattering the shared rails only affects
        //   rendering (short local stubs + flags). Any flag boxes created here are
        //   not covered by `result` and are placed by `place_orphans`, which sweeps
        //   *all* still-unplaced boxes (not just the pre-explode orphan set).
        explode_power_rails_to_flags(graph);

        // ── Phase 1c: Sizes + entry points (after explode so flags are sized too) ──
        assign_default_sizes(graph);
        assign_entry_points_coarse(graph);
        recompute_sizes_with_pin_count(graph);

        if graph.boxes.len() == 1 {
            graph.boxes[0].x = CANVAS_MARGIN;
            graph.boxes[0].y = CANVAS_MARGIN;
            return compute_canvas(graph);
        }

        // ── Phase 3: Assign directions to hub pins ──
        let dir_map = assign_pin_directions(graph, &result);

        // ── Phase 4: Place ──
        self.place_all(graph, &result, &dir_map);

        // ── Phase 5: Post-processing ──
        super::pin_place::pin_place_pipeline(graph, Some(result.hub_id), true, false);
        recompute_sizes_with_pin_count(graph);
        PlaceOptimizer::default().run(graph);
        normalize_positions(graph);
        compute_canvas(graph)
    }

    fn name(&self) -> &'static str {
        "schematic_sub_chain"
    }
}

// ============================================================================
// Direction assignment
// ============================================================================

/// Assigned direction for each hub pin (by pin_id)
type DirMap = HashMap<i64, ChainDir>;

/// Classify each hub pin to Left / Right / Up / Down based on pin name semantics,
/// then rebalance to avoid overcrowding one side.
fn assign_pin_directions(graph: &McVecGraph, result: &SignalChainResult) -> DirMap {
    let mut map = DirMap::new();
    let by_pin = result.by_pin();

    for (&pin_id, chains) in &by_pin {
        let pin_name = chains
            .first()
            .map(|c| c.hub_pin_name.as_str())
            .unwrap_or("");

        // 1. 语义脚名（EN/LX/FB/Vin/GND）→ 理想布局（等上游修好名字后自动生效）
        // 2. ★ 轨/终点语义兜底：数字脚名时用链的电气终点判方向
        //    （终于 GND→Down、终于电源→Up、去耦 loop→贴侧翼），用可信的连接而非名字
        // 3. Right 兜底（rebalance_directions 再分摊）
        let dir = semantic_dir_from_name(pin_name)
            .or_else(|| dir_from_rail_semantics(graph, chains))
            .unwrap_or(ChainDir::Right);
        map.insert(pin_id, dir);
    }

    rebalance_directions(&mut map, &by_pin);
    map
}

fn dir_from_pin_name(name: &str, chains: &[&SignalChain]) -> ChainDir {
    if let Some(d) = semantic_dir_from_name(name) {
        return d;
    }
    let all_loop = chains.iter().all(|c| c.loops_to_hub);
    if all_loop && !chains.is_empty() {
        return ChainDir::Up;
    }
    ChainDir::Right
}

/// Recognized pin function name → side, or `None` if the name is meaningless
/// (bare pin numbers "1".."5", empty, instance names). `None` triggers the
/// rail-semantics fallback in `assign_pin_directions`.
fn semantic_dir_from_name(name: &str) -> Option<ChainDir> {
    let u = name.to_uppercase();
    if u.starts_with("VIN")
        || u.starts_with("VCC")
        || u.starts_with("VDD")
        || u.starts_with("VBUS")
        || u == "IN"
        || u.starts_with("PVDD")
    {
        return Some(ChainDir::Up);
    }
    if naming::is_ground(&u) {
        return Some(ChainDir::Down);
    }
    if u == "EN"
        || u == "ENABLE"
        || u.starts_with("RST")
        || u.starts_with("NRST")
        || u == "CE"
        || u == "SHDN"
        || u.starts_with("CLK")
        || u.starts_with("SCL")
    {
        return Some(ChainDir::Left);
    }
    if u.starts_with("OUT")
        || u.starts_with("VOUT")
        || u == "LX"
        || u == "SW"
        || u.starts_with("BUCK")
        || u.starts_with("BOOST")
    {
        return Some(ChainDir::Right);
    }
    if u == "FB" || u.starts_with("FB_") || u == "ADJ" {
        return Some(ChainDir::Down);
    }
    None
}

#[derive(Clone, Copy, PartialEq)]
enum RailTarget {
    Power,
    Ground,
    Loop,
    Signal,
}

/// Classify what a chain electrically terminates at (by terminus net/box kind,
/// not the possibly-numeric hub pin name).
fn classify_chain_target(graph: &McVecGraph, chain: &SignalChain) -> RailTarget {
    let net_is_ground = |nid: i64| {
        graph
            .nets
            .iter()
            .find(|n| n.nid == nid)
            .is_some_and(|n| matches!(n.kind, NetKind::Ground) || naming::is_ground(&n.name))
    };
    let net_is_power = |nid: i64| {
        graph
            .nets
            .iter()
            .find(|n| n.nid == nid)
            .is_some_and(|n| matches!(n.kind, NetKind::Power) || naming::is_power_rail(&n.name))
    };

    if let Some(t) = &chain.terminus {
        if net_is_ground(t.net_id) {
            return RailTarget::Ground;
        }
        if net_is_power(t.net_id) {
            return RailTarget::Power;
        }
        if let Some(b) = graph.boxes.iter().find(|b| b.id == t.box_id) {
            if naming::is_ground(&b.name) {
                return RailTarget::Ground;
            }
            if naming::is_power_rail(&b.name) {
                return RailTarget::Power;
            }
        }
        return RailTarget::Signal;
    }
    if chain.loops_to_hub {
        return RailTarget::Loop;
    }
    RailTarget::Signal
}

/// Pick a side from chains' electrical targets when the pin name gives no hint.
/// Majority vote: ground→Down, power→Up, pure decoupling loops→Up, signal→None
/// (let horizontal balancing decide).
fn dir_from_rail_semantics(graph: &McVecGraph, chains: &[&SignalChain]) -> Option<ChainDir> {
    let (mut power, mut ground, mut looping, mut signal) = (0u32, 0u32, 0u32, 0u32);
    for c in chains {
        match classify_chain_target(graph, c) {
            RailTarget::Power => power += 1,
            RailTarget::Ground => ground += 1,
            RailTarget::Loop => looping += 1,
            RailTarget::Signal => signal += 1,
        }
    }
    if ground > 0 && ground >= power && ground >= signal {
        return Some(ChainDir::Down);
    }
    if power > 0 && power >= signal {
        return Some(ChainDir::Up);
    }
    if looping > 0 && signal == 0 {
        return Some(ChainDir::Up);
    }
    None
}

/// Rebalance: if Right has >> Left, or Up has >> Down, move some pins.
fn rebalance_directions(map: &mut DirMap, by_pin: &HashMap<i64, Vec<&SignalChain>>) {
    let mut counts: HashMap<ChainDir, usize> = HashMap::new();
    for dir in map.values() {
        *counts.entry(dir.clone()).or_default() += 1;
    }

    let right = *counts.get(&ChainDir::Right).unwrap_or(&0);
    let left = *counts.get(&ChainDir::Left).unwrap_or(&0);

    // If Right has 3+ more than Left, move some generic pins to Left
    if right > left + 2 {
        let to_move = (right - left) / 2;
        let mut moved = 0;
        // Prefer moving pins whose chains are short (direct connections or single passive)
        let mut candidates: Vec<(i64, usize)> = map
            .iter()
            .filter(|(_, d)| **d == ChainDir::Right)
            .map(|(&pid, _)| {
                let depth: usize = by_pin
                    .get(&pid)
                    .map(|cs| cs.iter().map(|c| c.nodes.len()).max().unwrap_or(0))
                    .unwrap_or(0);
                (pid, depth)
            })
            .collect();
        candidates.sort_by_key(|&(_, depth)| depth);

        for (pid, _) in candidates {
            if moved >= to_move {
                break;
            }
            // Don't move pins with strong semantic direction
            let name = by_pin
                .get(&pid)
                .and_then(|cs| cs.first())
                .map(|c| c.hub_pin_name.to_uppercase())
                .unwrap_or_default();
            if name.starts_with("OUT") || name.starts_with("VOUT") || name == "LX" || name == "SW" {
                continue;
            }
            map.insert(pid, ChainDir::Left);
            moved += 1;
        }
    }
}

// ============================================================================
// Placement
// ============================================================================

impl SchematicSubLayouter {
    fn place_all(&self, graph: &mut McVecGraph, result: &SignalChainResult, dir_map: &DirMap) {
        // ── Step 1: Place hub IC at center ──
        let hub_id = result.hub_id;
        let (hub_w, hub_h) = graph
            .boxes
            .iter()
            .find(|b| b.id == hub_id)
            .map(|b| (b.w, b.h))
            .unwrap_or((150.0, 100.0));

        // Use a large starting coordinate to avoid negative positions; normalize_positions
        // will pull everything to the origin at the end.
        let hub_cx = 800.0;
        let hub_cy = 600.0;
        let hub_x = hub_cx - hub_w / 2.0;
        let hub_y = hub_cy - hub_h / 2.0;

        if let Some(hub) = graph.boxes.iter_mut().find(|b| b.id == hub_id) {
            hub.x = hub_x;
            hub.y = hub_y;
        }

        // ── Step 2: Group chains by direction ──
        let by_pin = result.by_pin();
        let mut placed: HashSet<i64> = HashSet::new();
        placed.insert(hub_id);

        // Track how many chains have been placed per direction for stacking
        let mut dir_slot: HashMap<ChainDir, usize> = HashMap::new();

        // Sort pins for deterministic layout: by direction, then by pin_name
        let mut pin_order: Vec<i64> = by_pin.keys().copied().collect();
        pin_order.sort_by(|a, b| {
            let da = dir_map.get(a).cloned().unwrap_or(ChainDir::Right);
            let db = dir_map.get(b).cloned().unwrap_or(ChainDir::Right);
            let dir_rank = |d: &ChainDir| match d {
                ChainDir::Left => 0,
                ChainDir::Right => 1,
                ChainDir::Up => 2,
                ChainDir::Down => 3,
            };
            dir_rank(&da).cmp(&dir_rank(&db)).then(a.cmp(b))
        });

        for &pin_id in &pin_order {
            let chains = match by_pin.get(&pin_id) {
                Some(cs) => cs,
                None => continue,
            };
            let dir = dir_map.get(&pin_id).cloned().unwrap_or(ChainDir::Right);
            let slot = dir_slot.entry(dir.clone()).or_default();

            for chain in chains {
                self.place_chain(
                    graph,
                    chain,
                    &dir,
                    hub_cx,
                    hub_cy,
                    hub_w,
                    hub_h,
                    *slot,
                    &mut placed,
                );
                *slot += 1;
            }
        }

        // ── Step 3: Place orphans ──
        self.place_orphans(graph, result, hub_cx, hub_cy, hub_w, hub_h, &mut placed);
    }

    /// Place a single signal chain extending from the hub.
    fn place_chain(
        &self,
        graph: &mut McVecGraph,
        chain: &SignalChain,
        dir: &ChainDir,
        hub_cx: f64,
        hub_cy: f64,
        hub_w: f64,
        hub_h: f64,
        slot: usize,
        placed: &mut HashSet<i64>,
    ) {
        // Compute the starting position: the edge of the hub box on the given side
        let slot_offset = slot as f64 * self.stack_gap;

        // For loop chains (decoupling caps), place close to hub
        if chain.loops_to_hub && chain.nodes.len() == 1 {
            let node = &chain.nodes[0];
            if placed.contains(&node.box_id) {
                return;
            }
            let (bw, bh) = box_size_of(graph, node.box_id);
            let (x, y) = match dir {
                ChainDir::Up => (
                    hub_cx - bw / 2.0 + slot_offset,
                    hub_cy - hub_h / 2.0 - self.loop_offset - bh,
                ),
                ChainDir::Down => (
                    hub_cx - bw / 2.0 + slot_offset,
                    hub_cy + hub_h / 2.0 + self.loop_offset,
                ),
                ChainDir::Left => (
                    hub_cx - hub_w / 2.0 - self.loop_offset - bw,
                    hub_cy - bh / 2.0 + slot_offset,
                ),
                ChainDir::Right => (
                    hub_cx + hub_w / 2.0 + self.loop_offset,
                    hub_cy - bh / 2.0 + slot_offset,
                ),
            };
            set_box_pos(graph, node.box_id, x, y);
            placed.insert(node.box_id);
            return;
        }

        // Non-loop chain: extend outward from hub edge
        let all_nodes: Vec<i64> = {
            let mut ids: Vec<i64> = chain.nodes.iter().map(|n| n.box_id).collect();
            if let Some(t) = &chain.terminus {
                ids.push(t.box_id);
            }
            ids
        };

        // Starting cursor at hub edge
        let (mut cx, mut cy) = match dir {
            ChainDir::Right => (hub_cx + hub_w / 2.0 + self.chain_gap, hub_cy + slot_offset),
            ChainDir::Left => (hub_cx - hub_w / 2.0 - self.chain_gap, hub_cy + slot_offset),
            ChainDir::Up => (hub_cx + slot_offset, hub_cy - hub_h / 2.0 - self.chain_gap),
            ChainDir::Down => (hub_cx + slot_offset, hub_cy + hub_h / 2.0 + self.chain_gap),
        };

        for &box_id in &all_nodes {
            if placed.contains(&box_id) {
                continue;
            }
            let (bw, bh) = box_size_of(graph, box_id);

            let (x, y) = match dir {
                ChainDir::Right => {
                    let pos = (cx, cy - bh / 2.0);
                    cx += bw + self.chain_gap;
                    pos
                }
                ChainDir::Left => {
                    cx -= bw;
                    let pos = (cx, cy - bh / 2.0);
                    cx -= self.chain_gap;
                    pos
                }
                ChainDir::Down => {
                    let pos = (cx - bw / 2.0, cy);
                    cy += bh + self.chain_gap;
                    pos
                }
                ChainDir::Up => {
                    cy -= bh;
                    let pos = (cx - bw / 2.0, cy);
                    cy -= self.chain_gap;
                    pos
                }
            };

            set_box_pos(graph, box_id, x, y);
            placed.insert(box_id);
        }
    }

    /// Place orphan boxes (not covered by any chain) in remaining space.
    fn place_orphans(
        &self,
        graph: &mut McVecGraph,
        _result: &SignalChainResult,
        hub_cx: f64,
        hub_cy: f64,
        hub_w: f64,
        hub_h: f64,
        placed: &mut HashSet<i64>,
    ) {
        // Collect unplaced boxes (orphans from chain result + any still unplaced)
        let unplaced: Vec<i64> = graph
            .boxes
            .iter()
            .filter(|b| !placed.contains(&b.id))
            .map(|b| b.id)
            .collect();

        if unplaced.is_empty() {
            return;
        }

        // Separate power labels from other orphans
        let mut power_labels: Vec<i64> = Vec::new();
        let mut ground_labels: Vec<i64> = Vec::new();
        let mut other_orphans: Vec<i64> = Vec::new();

        for &id in &unplaced {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == id) {
                if is_rail_box(b) {
                    if naming::is_ground(&b.name) {
                        ground_labels.push(id);
                    } else {
                        power_labels.push(id);
                    }
                } else {
                    other_orphans.push(id);
                }
            }
        }

        // Place power labels above the layout area
        let power_y = hub_cy - hub_h / 2.0 - 2.0 * self.flag_gap - 40.0;
        let mut px = hub_cx - hub_w / 2.0;
        for id in &power_labels {
            let (bw, _bh) = box_size_of(graph, *id);
            set_box_pos(graph, *id, px, power_y);
            px += bw + MIN_GAP;
            placed.insert(*id);
        }

        // Place ground labels below the layout area
        let ground_y = hub_cy + hub_h / 2.0 + 2.0 * self.flag_gap + 40.0;
        let mut gx = hub_cx - hub_w / 2.0;
        for id in &ground_labels {
            let (bw, _bh) = box_size_of(graph, *id);
            set_box_pos(graph, *id, gx, ground_y);
            gx += bw + MIN_GAP;
            placed.insert(*id);
        }

        // Place remaining orphans in a row below ground
        if !other_orphans.is_empty() {
            let orphan_y = ground_y + 80.0;
            let mut ox = hub_cx - hub_w;
            for id in &other_orphans {
                let (bw, _bh) = box_size_of(graph, *id);
                set_box_pos(graph, *id, ox, orphan_y);
                ox += bw + MIN_GAP;
                placed.insert(*id);
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn box_size_of(graph: &McVecGraph, id: i64) -> (f64, f64) {
    graph
        .boxes
        .iter()
        .find(|b| b.id == id)
        .map(|b| (b.w, b.h))
        .unwrap_or((60.0, 40.0))
}

fn set_box_pos(graph: &mut McVecGraph, id: i64, x: f64, y: f64) {
    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
        b.x = x;
        b.y = y;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::{
        BoxKind, EndpointRef, EntryPoint, EntrySide, IoSummary, McVecBox, McVecGraph, NetKind,
        Symbol, VizNet,
    };
    use crate::viz::traits::Layouter;

    fn mk_ic(id: i64, name: &str, pin_count: usize) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::MultiPin,
            Symbol::Ic,
            None,
            None,
            pin_count,
            IoSummary::new(),
        )
    }

    fn mk_passive(id: i64, name: &str, sym: Symbol) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::TwoPin,
            sym,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
        )
    }

    fn mk_label(id: i64, name: &str) -> McVecBox {
        let is_gnd = naming::is_ground(name);
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: is_gnd },
            None,
            None,
            0,
            IoSummary::new(),
        )
    }

    fn mk_sub(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::SubModule,
            Symbol::Module,
            None,
            None,
            0,
            IoSummary::new(),
        )
    }

    fn mk_net(nid: i64, name: &str, eps: Vec<(i64, i64, &str)>) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            NetKind::Signal,
            eps.into_iter()
                .map(|(bid, pid, pn)| EndpointRef::new(bid, pid, pn))
                .collect(),
        )
    }

    /// Build the moddcdc DC-DC module graph from the phase2 prompt.
    ///
    /// ```text
    /// hub = lp322dcdc (id=1168), MultiPin, 8 pins
    /// chains:
    ///   [0] EN: C_dcdc_en -> hub(loop)
    ///   [1] LX: L_dcdc -> C_dcdc_vout_10u
    ///   [2] GND: (direct) -> moddcdc
    ///   [3] Vin: C_dcdc_vin -> hub(loop)
    ///   [4] Vin: (direct) -> moddcdc
    ///   [5] FB: R_fb_high -> C_dcdc_vout_10u
    ///   [6] FB: R_fb_low -> moddcdc
    ///   [7] FB: C_dcdc_fb -> moddcdc
    /// orphans: moddcdc (SubModule boundary port)
    /// ```
    fn build_moddcdc_graph() -> McVecGraph {
        let mut g = McVecGraph::new(42, "moddcdc".into());

        // Hub IC
        let mut ic = mk_ic(1168, "lp322dcdc", 8);
        // Add entry points so chain extraction can find pin sides
        ic.entry_points = vec![
            EntryPoint {
                pin_id: 10,
                pin_name: "EN".into(),
                side: EntrySide::Left,
                offset: 0.25,
            },
            EntryPoint {
                pin_id: 11,
                pin_name: "LX".into(),
                side: EntrySide::Right,
                offset: 0.25,
            },
            EntryPoint {
                pin_id: 12,
                pin_name: "GND".into(),
                side: EntrySide::Bottom,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 13,
                pin_name: "Vin".into(),
                side: EntrySide::Left,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 14,
                pin_name: "Vin".into(),
                side: EntrySide::Left,
                offset: 0.75,
            },
            EntryPoint {
                pin_id: 15,
                pin_name: "FB".into(),
                side: EntrySide::Right,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 16,
                pin_name: "FB".into(),
                side: EntrySide::Right,
                offset: 0.6,
            },
            EntryPoint {
                pin_id: 17,
                pin_name: "FB".into(),
                side: EntrySide::Right,
                offset: 0.7,
            },
        ];
        g.boxes.push(ic);

        // Passives
        g.boxes
            .push(mk_passive(2001, "C_dcdc_en", Symbol::Capacitor));
        g.boxes.push(mk_passive(2002, "L_dcdc", Symbol::Inductor));
        g.boxes
            .push(mk_passive(2003, "C_dcdc_vin", Symbol::Capacitor));
        g.boxes
            .push(mk_passive(2004, "R_fb_high", Symbol::Resistor));
        g.boxes.push(mk_passive(2005, "R_fb_low", Symbol::Resistor));
        g.boxes
            .push(mk_passive(2006, "C_dcdc_fb", Symbol::Capacitor));
        g.boxes
            .push(mk_passive(2007, "C_dcdc_vout_10u", Symbol::Capacitor));

        // Boundary sub-module (orphan)
        g.boxes.push(mk_sub(9999, "moddcdc"));

        // ── Nets ──

        // EN net: IC.EN(pin10) -- C_dcdc_en
        g.nets.push(mk_net(
            100,
            "net_en",
            vec![(1168, 10, "EN"), (2001, 101, "1")],
        ));
        // EN return: C_dcdc_en -- IC (loop back via different pin on same IC)
        g.nets.push(mk_net(
            101,
            "net_en_ret",
            vec![(2001, 102, "2"), (1168, 12, "GND")],
        ));

        // LX net: IC.LX(pin11) -- L_dcdc
        g.nets.push(mk_net(
            110,
            "net_lx",
            vec![(1168, 11, "LX"), (2002, 201, "1")],
        ));
        // L_dcdc -- C_dcdc_vout_10u
        g.nets.push(mk_net(
            111,
            "net_vout",
            vec![(2002, 202, "2"), (2007, 701, "1")],
        ));

        // GND direct: IC.GND(pin12) -- moddcdc
        g.nets.push(mk_net(
            120,
            "net_gnd",
            vec![(1168, 12, "GND"), (9999, 901, "GND")],
        ));

        // Vin net: IC.Vin(pin13) -- C_dcdc_vin
        g.nets.push(mk_net(
            130,
            "net_vin",
            vec![(1168, 13, "Vin"), (2003, 301, "1")],
        ));
        // Vin return: C_dcdc_vin -- IC (loop back)
        g.nets.push(mk_net(
            131,
            "net_vin_ret",
            vec![(2003, 302, "2"), (1168, 12, "GND")],
        ));

        // Vin direct: IC.Vin(pin14) -- moddcdc
        g.nets.push(mk_net(
            140,
            "net_vin_direct",
            vec![(1168, 14, "Vin"), (9999, 902, "Vin")],
        ));

        // FB nets: IC.FB(pin15) -- R_fb_high
        g.nets.push(mk_net(
            150,
            "net_fb_h",
            vec![(1168, 15, "FB"), (2004, 401, "1")],
        ));
        // R_fb_high -- C_dcdc_vout_10u (terminus)
        g.nets.push(mk_net(
            151,
            "net_fb_h_out",
            vec![(2004, 402, "2"), (2007, 702, "2")],
        ));

        // IC.FB(pin16) -- R_fb_low
        g.nets.push(mk_net(
            160,
            "net_fb_l",
            vec![(1168, 16, "FB"), (2005, 501, "1")],
        ));
        // R_fb_low -- moddcdc
        g.nets.push(mk_net(
            161,
            "net_fb_l_out",
            vec![(2005, 502, "2"), (9999, 903, "GND")],
        ));

        // IC.FB(pin17) -- C_dcdc_fb
        g.nets.push(mk_net(
            170,
            "net_fb_c",
            vec![(1168, 17, "FB"), (2006, 601, "1")],
        ));
        // C_dcdc_fb -- moddcdc
        g.nets.push(mk_net(
            171,
            "net_fb_c_out",
            vec![(2006, 602, "2"), (9999, 904, "FB_GND")],
        ));

        g
    }

    #[test]
    fn moddcdc_layout_runs_without_panic() {
        let mut g = build_moddcdc_graph();
        let layouter = SchematicSubLayouter::default();
        let (cw, ch) = layouter.layout(&mut g);
        assert!(cw > 100.0, "canvas width should be reasonable: {}", cw);
        assert!(ch > 100.0, "canvas height should be reasonable: {}", ch);
    }

    #[test]
    fn moddcdc_all_boxes_have_positions() {
        let mut g = build_moddcdc_graph();
        let layouter = SchematicSubLayouter::default();
        layouter.layout(&mut g);

        for b in &g.boxes {
            assert!(b.w > 0.0, "box '{}' should have width", b.name);
            assert!(b.h > 0.0, "box '{}' should have height", b.name);
            // After normalize, all positions should be ≥ 0
            assert!(b.x >= 0.0, "box '{}' x={} should be >= 0", b.name, b.x);
            assert!(b.y >= 0.0, "box '{}' y={} should be >= 0", b.name, b.y);
        }
    }

    #[test]
    fn moddcdc_no_box_overlap() {
        let mut g = build_moddcdc_graph();
        let layouter = SchematicSubLayouter::default();
        layouter.layout(&mut g);

        // Check pairwise overlap
        let boxes = &g.boxes;
        for i in 0..boxes.len() {
            for j in (i + 1)..boxes.len() {
                let a = &boxes[i];
                let b = &boxes[j];
                let overlap_x = a.x < b.x + b.w && b.x < a.x + a.w;
                let overlap_y = a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(
                    !(overlap_x && overlap_y),
                    "boxes '{}' ({},{},{},{}) and '{}' ({},{},{},{}) overlap",
                    a.name,
                    a.x,
                    a.y,
                    a.w,
                    a.h,
                    b.name,
                    b.x,
                    b.y,
                    b.w,
                    b.h,
                );
            }
        }
    }

    #[test]
    fn moddcdc_hub_centered_passives_radiate() {
        let mut g = build_moddcdc_graph();
        let layouter = SchematicSubLayouter::default();
        layouter.layout(&mut g);

        let hub = g.boxes.iter().find(|b| b.id == 1168).unwrap();
        let _hub_cx = hub.x + hub.w / 2.0;
        let _hub_cy = hub.y + hub.h / 2.0;

        // L_dcdc (LX chain) should be to the right of hub
        let l_dcdc = g.boxes.iter().find(|b| b.name == "L_dcdc").unwrap();
        assert!(
            l_dcdc.x > hub.x + hub.w / 2.0,
            "L_dcdc should be to the right of hub: L_dcdc.x={}, hub_right={}",
            l_dcdc.x,
            hub.x + hub.w
        );

        // C_dcdc_vout_10u should be further right than L_dcdc (it's downstream)
        let c_vout = g
            .boxes
            .iter()
            .find(|b| b.name == "C_dcdc_vout_10u")
            .unwrap();
        assert!(
            c_vout.x >= l_dcdc.x,
            "C_dcdc_vout_10u should be to the right of or aligned with L_dcdc"
        );
    }

    #[test]
    fn direction_assignment_basic() {
        assert!(matches!(dir_from_pin_name("Vin", &[]), ChainDir::Up));
        assert!(matches!(dir_from_pin_name("VCC", &[]), ChainDir::Up));
        assert!(matches!(dir_from_pin_name("GND", &[]), ChainDir::Down));
        assert!(matches!(dir_from_pin_name("EN", &[]), ChainDir::Left));
        assert!(matches!(dir_from_pin_name("LX", &[]), ChainDir::Right));
        assert!(matches!(dir_from_pin_name("FB", &[]), ChainDir::Down));
        assert!(matches!(dir_from_pin_name("OUT", &[]), ChainDir::Right));
    }

    #[test]
    fn single_box_no_panic() {
        let mut g = McVecGraph::new(1, "tiny".into());
        g.boxes.push(mk_passive(1, "R1", Symbol::Resistor));
        let layouter = SchematicSubLayouter::default();
        let (cw, ch) = layouter.layout(&mut g);
        assert!(cw > 0.0 && ch > 0.0);
    }

    #[test]
    fn empty_graph_no_panic() {
        let mut g = McVecGraph::new(1, "empty".into());
        let layouter = SchematicSubLayouter::default();
        let (cw, ch) = layouter.layout(&mut g);
        assert_eq!((cw, ch), (200.0, 100.0));
    }
}
