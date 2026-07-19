// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Two-lane bridged-passive ladder — deterministic placement (M11 `c07_pins`).
//!
//! **Status: fallback** — heuristic ladder placement; superseded by `ladder_model`
//! + `ladder_place` when the net-based ladder model matches. Kept as fallback
//! for graphs where the model bails. Further fixture validation needed to
//! determine if long-term retention is required.
//!
//! For a directional two-lane bus the picture is trivial: two anchors (u1/u2)
//! and two straight horizontal wires between them, with the series resistors
//! sitting *on* the wires and the bridge caps crossing vertically. So we don't
//! hand this pattern to the general passive-inline heuristic (which is built for
//! scattering passives around an IC and can push them past an anchor). Instead
//! the ladder — which already reconstructs the exact lane order by walking the
//! nets — places everything deterministically:
//!
//!   * u1 at left, u2 at right, on a shared baseline;
//!   * each lane's resistors evenly spaced on the straight line between the two
//!     anchor centres (side-independent → no circular dependency), oriented
//!     horizontally with pins Left/Right;
//!   * anchor pins sided by lane direction: source (u1) → Right, sink (u2) → Left;
//!   * resistors tagged `VisualRole::SeriesInline` so the generic passive passes
//!     leave them alone; caps stay `BridgePassive` for `place_bridge_passives`.
//!
//! Invoked from `FlowLayouter::layout` after `phase_size`, after `phase_placement`,
//! before `pin_place`.

use crate::vector::graph::boxdef::PinLayout;
use crate::vector::graph::{BoxKind, EntryPoint, EntrySide, McVecGraph, VisualRole};
use std::collections::{HashMap, HashSet};

const MARGIN: f64 = 100.0;
/// Horizontal room reserved per inline element on a lane.
const SLOT: f64 = 180.0;
/// Vertical distance between the two lane baselines.
const LANE_SEP: f64 = 150.0;

pub fn try_two_lane_ladder(graph: &mut McVecGraph) -> Option<()> {
    // ── 1. anchors: non-passive TwoPin boxes carrying I/O direction ──────────
    let anchor_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| {
            b.id >= 0
                && b.kind == BoxKind::TwoPin
                && !b.is_two_pin_passive()
                && (b.io_summary.outputs > 0 || b.io_summary.inputs > 0)
        })
        .map(|b| b.id)
        .collect();
    if anchor_ids.len() != 2 {
        return None;
    }
    let has_bridge = graph
        .boxes
        .iter()
        .any(|b| b.visual_role == Some(VisualRole::BridgePassive));
    if !has_bridge {
        return None;
    }

    let out_of = |id: i64| {
        graph
            .boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.io_summary.outputs)
            .unwrap_or(0)
    };
    let (left, right) = if out_of(anchor_ids[0]) >= out_of(anchor_ids[1]) {
        (anchor_ids[0], anchor_ids[1])
    } else {
        (anchor_ids[1], anchor_ids[0])
    };

    // ── 2. reconstruct the two lanes (ordered non-bridge passives u1→u2) ─────
    let is_bridge = |g: &McVecGraph, bid: i64| {
        g.boxes
            .iter()
            .any(|b| b.id == bid && b.visual_role == Some(VisualRole::BridgePassive))
    };
    let is_lane_passive = |g: &McVecGraph, bid: i64| {
        g.boxes.iter().any(|b| {
            b.id == bid
                && b.kind == BoxKind::TwoPin
                && b.is_two_pin_passive()
                && b.visual_role != Some(VisualRole::BridgePassive)
        })
    };
    let mut box_nets: HashMap<i64, Vec<usize>> = HashMap::new();
    for (ni, net) in graph.nets.iter().enumerate() {
        let mut seen = HashSet::new();
        for ep in &net.endpoints {
            if seen.insert(ep.box_id) {
                box_nets.entry(ep.box_id).or_default().push(ni);
            }
        }
    }
    let walk = |g: &McVecGraph, start_net: usize| -> Option<Vec<i64>> {
        let mut lane = Vec::new();
        let mut cur = start_net;
        let mut prev = left;
        for _ in 0..(g.boxes.len() + 2) {
            let next = g.nets[cur]
                .endpoints
                .iter()
                .map(|e| e.box_id)
                .find(|&b| b != prev && !is_bridge(g, b) && !lane.contains(&b))?;
            if next == right {
                return Some(lane);
            }
            if !is_lane_passive(g, next) {
                return None;
            }
            let exit = box_nets.get(&next)?.iter().copied().find(|&n| n != cur)?;
            lane.push(next);
            prev = next;
            cur = exit;
        }
        None
    };
    let left_nets = box_nets.get(&left)?.clone();
    if left_nets.len() < 2 {
        return None;
    }
    let lane0 = walk(graph, left_nets[0])?;
    let lane1 = walk(graph, left_nets[1])?;
    if lane0.is_empty() && lane1.is_empty() {
        return None;
    }

    // ── 3. anchor geometry + lane baselines ──────────────────────────────────
    let (lw, lh) = graph
        .boxes
        .iter()
        .find(|b| b.id == left)
        .map(|b| (b.w, b.h))
        .unwrap_or((80.0, 80.0));
    let max_lane = lane0.len().max(lane1.len());
    let inner_left = MARGIN + lw;
    let span = ((max_lane + 1) as f64) * SLOT;
    let inner_right = inner_left + span;
    let right_x = inner_right;

    let center_y = MARGIN + lh / 2.0;
    let lane0_y = center_y - LANE_SEP / 2.0; // top lane
    let lane1_y = center_y + LANE_SEP / 2.0; // bottom lane

    // anchors: keep box top-left so vertical centre == center_y
    for b in graph.boxes.iter_mut() {
        if b.id == left {
            b.x = MARGIN;
            b.y = center_y - b.h / 2.0;
        } else if b.id == right {
            b.x = right_x;
            b.y = center_y - b.h / 2.0;
        }
    }

    // ── 4. place each lane's resistors evenly on its baseline (horizontal) ────
    place_lane(graph, &lane0, left, right, inner_left, inner_right, lane0_y);
    place_lane(graph, &lane1, left, right, inner_left, inner_right, lane1_y);

    // ── 5. anchor pin sides by lane direction (net-derived, not a blind lock) ─
    freeze_side(graph, left, EntrySide::Right);
    freeze_side(graph, right, EntrySide::Left);

    crate::vlog!(
        "[ladder] two-lane deterministic: lane0={} lane1={} (u1={} u2={} y0={:.0} y1={:.0})",
        lane0.len(),
        lane1.len(),
        left,
        right,
        lane0_y,
        lane1_y
    );
    Some(())
}

/// Evenly place a lane's resistors on `lane_y`, oriented horizontally, pins
/// Left/Right (Left = pin toward the lane's left neighbour). Tag each as
/// `SeriesInline` so the generic passive passes skip it, and freeze its pins.
fn place_lane(
    graph: &mut McVecGraph,
    lane: &[i64],
    left: i64,
    right: i64,
    inner_left: f64,
    inner_right: f64,
    lane_y: f64,
) {
    let n = lane.len();
    if n == 0 {
        return;
    }
    let step = (inner_right - inner_left) / (n as f64 + 1.0);
    for (i, &rid) in lane.iter().enumerate() {
        let cx = inner_left + (i as f64 + 1.0) * step;

        // left/right neighbours in the chain (anchors at the ends)
        let left_nb = if i == 0 { left } else { lane[i - 1] };
        let right_nb = if i + 1 == n { right } else { lane[i + 1] };
        let left_pin = pin_toward(graph, rid, left_nb);
        let right_pin = pin_toward(graph, rid, right_nb);

        let Some(b) = graph.boxes.iter_mut().find(|b| b.id == rid) else {
            continue;
        };
        // orient horizontal: long side across the lane
        let long = b.w.max(b.h);
        let short = b.w.min(b.h);
        b.w = long;
        b.h = short;
        b.x = cx - b.w / 2.0;
        b.y = lane_y - b.h / 2.0;
        b.visual_role = Some(VisualRole::SeriesInline);

        // pins: left toward left neighbour, right toward right neighbour
        let ids: Vec<(i64, String)> = b
            .entry_points
            .iter()
            .map(|e| (e.pin_id, e.pin_name.clone()))
            .collect();
        let ids = if ids.len() == 2 {
            ids
        } else {
            b.pins
                .iter()
                .map(|p| (p.id, p.id.to_string()))
                .take(2)
                .collect()
        };
        if ids.len() == 2 {
            let (lp, rp) = match (left_pin, right_pin) {
                (Some(l), Some(r)) if l != r => (l, r),
                _ => (ids[0].0, ids[1].0), // fallback: existing order
            };
            let name_of = |pid: i64| {
                ids.iter()
                    .find(|(id, _)| *id == pid)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default()
            };
            b.entry_points = vec![
                EntryPoint {
                    pin_id: lp,
                    pin_name: name_of(lp),
                    side: EntrySide::Left,
                    offset: 0.5,
                },
                EntryPoint {
                    pin_id: rp,
                    pin_name: name_of(rp),
                    side: EntrySide::Right,
                    offset: 0.5,
                },
            ];
            let mut hint = PinLayout::default();
            hint.left = vec![name_of(lp), lp.to_string()];
            hint.right = vec![name_of(rp), rp.to_string()];
            b.set_layout_hint(hint);
        }
    }
}

/// The pin_id of `bid` that lies on the net shared with `neighbor`.
fn pin_toward(graph: &McVecGraph, bid: i64, neighbor: i64) -> Option<i64> {
    for net in &graph.nets {
        let has_nb = net.endpoints.iter().any(|e| e.box_id == neighbor);
        if !has_nb {
            continue;
        }
        if let Some(e) = net.endpoints.iter().find(|e| e.box_id == bid) {
            return Some(e.pin_id);
        }
    }
    None
}

/// Set every entry point of `id` to `side` and freeze it via `layout_hint`.
fn freeze_side(graph: &mut McVecGraph, id: i64, side: EntrySide) {
    let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) else {
        return;
    };
    let mut names: Vec<String> = Vec::new();
    for ep in &mut b.entry_points {
        ep.side = side.clone();
        names.push(ep.pin_name.clone());
        names.push(ep.pin_id.to_string());
    }
    let mut hint = PinLayout::default();
    match side {
        EntrySide::Right => hint.right = names,
        EntrySide::Left => hint.left = names,
        EntrySide::Top => hint.top = names,
        EntrySide::Bottom => hint.bottom = names,
    }
    b.set_layout_hint(hint);
}
